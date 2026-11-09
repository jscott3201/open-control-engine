//! Block checks: class lookup, parameter lookup, kinds, references and the catalog rules, then
//! the block's resolved ports for the connection checks.
//!
//! The catalog rules are not re-implemented: the assignments that pass the author-side checks
//! are handed to [`oce_api::class_ports`], which runs the load gate's own parameter checks, and
//! each refusal it returns becomes a finding with the engine's code ([`oce_api::DiagCode::as_str`]).
//! Every code a finding carries is in [`super::STAGE_ONE_CODES`]: an engine code outside that list
//! (none is reachable through `class_ports` today) is reported as `parameter-out-of-range` with the
//! engine's code named in the detail, `ClassPortsError::UnknownClass` as `class-not-found`, and any
//! other `class_ports` refusal (a table whose ports cannot be named) as `parameter-out-of-range`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use oce_api::{ClassPortsError, Diagnostic, Value, ValueType, class_ports};

use crate::json::wire_string_len;
use crate::model::{Block, Literal, ParamValue, Sequence, SignalType};
use crate::wire::MAX_WIRE_STRING_BYTES;

use super::catalog::{ClassView, enum_type, lookup_class, lookup_parameter};
use super::index::{Declared, Ports};
use super::names::{exact_as_real, inexact_detail, literal_fits};
use super::{
    Anchor, CLASS_NOT_AUTHORABLE, CLASS_NOT_FOUND, DUPLICATE_PARAMETER, Findings,
    NON_FINITE_PARAMETER, PARAMETER_KIND_MISMATCH, PARAMETER_NOT_SETTABLE, PARAMETER_OUT_OF_RANGE,
    STAGE_ONE_CODES, UNKNOWN_ENUM_LITERAL, UNKNOWN_PARAMETER, UNKNOWN_PARAMETER_REFERENCE,
};

/// Check every block; return each block's resolved ports, or `None` where they are unknown (an
/// unknown class, a refused parameter table, or an assignment withheld by an author-side
/// finding, which could change the port widths).
pub(super) fn check(
    sequence: &Sequence,
    declared: &Declared<'_>,
    findings: &mut Findings,
) -> Vec<Option<Ports>> {
    sequence
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| check_block(sequence, declared, index, block, findings))
        .collect()
}

/// Why an assignment was withheld from the class-rule check.
struct Refusal {
    code: &'static str,
    detail: String,
}

fn refuse(code: &'static str, detail: String) -> Refusal {
    Refusal { code, detail }
}

fn check_block(
    sequence: &Sequence,
    declared: &Declared<'_>,
    index: usize,
    block: &Block,
    findings: &mut Findings,
) -> Option<Ports> {
    let block_position = Anchor::Block(index, None);
    let Some(class) = lookup_class(&block.class_path) else {
        findings.error(
            block_position,
            CLASS_NOT_FOUND,
            block.name.as_str(),
            format!(
                "block `{}` names class `{}`, which the catalog does not register",
                block.name, block.class_path
            ),
        );
        return None;
    };
    if class.entry.reserved {
        findings.error(
            block_position,
            CLASS_NOT_AUTHORABLE,
            block.name.as_str(),
            format!(
                "block `{}` names `{}`, an engine-reserved lowering identity no author may use",
                block.name, block.class_path
            ),
        );
    }

    let mut seen = BTreeSet::new();
    let mut withheld = BTreeSet::new();
    let mut table: Vec<(&str, Value)> = Vec::new();
    for (j, assignment) in block.assignments.iter().enumerate() {
        let name = assignment.name.as_str();
        let outcome = if seen.insert(name) {
            judge(sequence, declared, &class, block, name, &assignment.value)
        } else {
            Err(refuse(
                DUPLICATE_PARAMETER,
                format!("block `{}` assigns `{name}` more than once", block.name),
            ))
        };
        match outcome {
            Ok(Some(value)) => table.push((name, value)),
            Ok(None) => {
                withheld.insert(name);
            }
            Err(refusal) => {
                withheld.insert(name);
                findings.error(
                    Anchor::Block(index, Some(j)),
                    refusal.code,
                    format!("{}.{name}", block.name),
                    refusal.detail,
                );
            }
        }
    }

    let mut assigned: BTreeMap<&str, usize> = BTreeMap::new();
    for (j, assignment) in block.assignments.iter().enumerate() {
        assigned.entry(assignment.name.as_str()).or_insert(j);
    }
    match class_ports(class.entry.class_path, &table) {
        Ok(ports) => withheld.is_empty().then(|| Ports::new(ports)),
        Err(ClassPortsError::InvalidParameters { diagnostics, .. }) => {
            for diagnostic in &diagnostics {
                if let Some(mapped) = engine_finding(block, index, &assigned, &withheld, diagnostic)
                {
                    findings.error(mapped.anchor, mapped.code, mapped.subject, mapped.detail);
                }
            }
            None
        }
        Err(ClassPortsError::UnknownClass { class_path }) => {
            findings.error(
                block_position,
                CLASS_NOT_FOUND,
                block.name.as_str(),
                format!(
                    "block `{}` names class `{class_path}`, which the catalog does not register",
                    block.name
                ),
            );
            None
        }
        // `Unresolvable`, and any refusal a later facade adds: the table cannot be built into
        // nameable ports, which is a parameter defect of this block.
        Err(other) => {
            findings.error(
                block_position,
                PARAMETER_OUT_OF_RANGE,
                block.name.as_str(),
                format!(
                    "block `{}`: its parameters cannot be built into ports: {other}",
                    block.name
                ),
            );
            None
        }
    }
}

/// One engine diagnostic mapped onto the model.
pub(super) struct EngineFinding {
    pub(super) anchor: Anchor,
    pub(super) code: &'static str,
    pub(super) subject: String,
    pub(super) detail: String,
}

/// Map one `class_ports` diagnostic: the subject is the first assignment whose name the message
/// backticks (`block.param`), else the block; the code is the engine's when it is a stage-1 code,
/// else `parameter-out-of-range` naming the engine code. `None` when the message names an
/// assignment the author side already reported or withheld.
pub(super) fn engine_finding(
    block: &Block,
    index: usize,
    assigned: &BTreeMap<&str, usize>,
    withheld: &BTreeSet<&str>,
    diagnostic: &Diagnostic,
) -> Option<EngineFinding> {
    let named = backticked(&diagnostic.message);
    if named.iter().any(|token| withheld.contains(token)) {
        return None;
    }
    let engine_code = diagnostic.code.as_str();
    let (code, prefix) = if STAGE_ONE_CODES.contains(&engine_code) {
        (engine_code, String::new())
    } else {
        (
            PARAMETER_OUT_OF_RANGE,
            format!("engine code `{engine_code}`: "),
        )
    };
    let (anchor, subject) = match named.iter().find_map(|token| assigned.get(token)) {
        Some(&j) => (
            Anchor::Block(index, Some(j)),
            format!("{}.{}", block.name, block.assignments[j].name),
        ),
        None => (Anchor::Block(index, None), block.name.as_str().to_owned()),
    };
    Some(EngineFinding {
        anchor,
        code,
        subject,
        detail: format!("block `{}`: {prefix}{}", block.name, diagnostic.message),
    })
}

/// The backticked tokens of an engine message, in order.
fn backticked(message: &str) -> Vec<&str> {
    message.split('`').skip(1).step_by(2).collect()
}

/// Judge one assignment. `Ok(Some(value))` passes it to the class rules; `Ok(None)` withholds it
/// silently because a finding elsewhere already covers it (a broken sequence parameter it
/// references); `Err` is a finding on the assignment.
fn judge(
    sequence: &Sequence,
    declared: &Declared<'_>,
    class: &ClassView,
    block: &Block,
    name: &str,
    value: &ParamValue,
) -> Result<Option<Value>, Refusal> {
    // A reserved lowering identity takes no authored parameter at all (#440: `settable` is false
    // only there), so every assignment on one is unsettable, whether or not the name exists.
    if class.entry.reserved {
        return Err(refuse(
            PARAMETER_NOT_SETTABLE,
            format!(
                "`{}` is an engine-reserved lowering identity; no author may assign its parameter `{name}`",
                block.class_path
            ),
        ));
    }
    let Some(parameter) = lookup_parameter(class.metadata, name) else {
        return Err(refuse(
            UNKNOWN_PARAMETER,
            format!(
                "`{}` has no parameter `{name}`; array parameters are assigned per element (`k_1`)",
                block.class_path
            ),
        ));
    };
    if !parameter.settable {
        return Err(refuse(
            PARAMETER_NOT_SETTABLE,
            format!(
                "parameter `{name}` of `{}` cannot be assigned by an author",
                block.class_path
            ),
        ));
    }
    let expected = parameter.value_type;
    let mismatch = |found: &str| {
        refuse(
            PARAMETER_KIND_MISMATCH,
            format!(
                "parameter `{name}` of `{}` takes {}; the value is {found}",
                block.class_path,
                kind_name(expected)
            ),
        )
    };
    let literal = match value {
        ParamValue::Literal(literal) => literal,
        ParamValue::Ref(target) => {
            let Some(param) = declared
                .params
                .get(target.as_str())
                .map(|&i| &sequence.params[i])
            else {
                return Err(refuse(
                    UNKNOWN_PARAMETER_REFERENCE,
                    format!("`{name}` references `{target}`, which is not a sequence parameter"),
                ));
            };
            let finite = !matches!(param.value, Literal::Real(v) if !v.is_finite());
            if !literal_fits(param.value_type, &param.value) || !finite {
                return Ok(None);
            }
            return match (expected, param.value_type, &param.value) {
                (ValueType::Real, SignalType::Real, Literal::Real(v)) => Ok(Some(Value::Real(*v))),
                // A Real sequence parameter widens its Integer literal, as CDL does.
                // An inexact Integer on the Real sequence parameter is reported there.
                (ValueType::Real, SignalType::Real, Literal::Integer(v)) if !exact_as_real(*v) => {
                    Ok(None)
                }
                (ValueType::Real, SignalType::Real, Literal::Integer(v)) => {
                    Ok(Some(Value::Real(*v as f64)))
                }
                (ValueType::Real, SignalType::Integer, Literal::Integer(v))
                    if !exact_as_real(*v) =>
                {
                    Err(refuse(
                        PARAMETER_OUT_OF_RANGE,
                        inexact_detail(&format!("parameter `{name}` (via `{target}`)"), *v),
                    ))
                }
                (
                    ValueType::Real | ValueType::Integer,
                    SignalType::Integer,
                    Literal::Integer(v),
                ) => Ok(Some(Value::Integer(*v))),
                (ValueType::Boolean, SignalType::Boolean, Literal::Boolean(v)) => {
                    Ok(Some(Value::Boolean(*v)))
                }
                (_, declared, _) => Err(mismatch(&format!(
                    "a reference to the {} sequence parameter `{target}`",
                    declared.as_str()
                ))),
            };
        }
    };
    match (expected, literal) {
        (ValueType::Real, Literal::Real(v)) if !v.is_finite() => Err(refuse(
            NON_FINITE_PARAMETER,
            format!("parameter `{name}` must be finite; the value is {v}"),
        )),
        (ValueType::Real, Literal::Real(v)) => Ok(Some(Value::Real(*v))),
        (ValueType::Real, Literal::Integer(v)) if !exact_as_real(*v) => Err(refuse(
            PARAMETER_OUT_OF_RANGE,
            inexact_detail(&format!("parameter `{name}`"), *v),
        )),
        (ValueType::Real | ValueType::Integer, Literal::Integer(v)) => Ok(Some(Value::Integer(*v))),
        (ValueType::Boolean, Literal::Boolean(v)) => Ok(Some(Value::Boolean(*v))),
        (ValueType::String, Literal::Text(v)) => match text_problem(v) {
            Some(problem) => Err(refuse(
                PARAMETER_OUT_OF_RANGE,
                format!("parameter `{name}`: the string {problem}"),
            )),
            None => Ok(Some(Value::String(Arc::from(v.as_str())))),
        },
        (ValueType::Enum(class_id), Literal::EnumMember(member)) => {
            let Some(enum_type) = enum_type(expected) else {
                return Err(refuse(
                    UNKNOWN_ENUM_LITERAL,
                    format!("parameter `{name}` takes an enumeration whose members are not known"),
                ));
            };
            let ordinal = enum_type
                .members
                .iter()
                .position(|m| *m == member.member)
                .filter(|_| member.type_name == enum_type.type_name);
            match ordinal.and_then(|o| u32::try_from(o + 1).ok()) {
                Some(ordinal) => Ok(Some(Value::Enum {
                    class: class_id,
                    ordinal,
                })),
                None => Err(refuse(
                    UNKNOWN_ENUM_LITERAL,
                    format!(
                        "`{}` is not a member of `{}` ({}); write `{}.<member>` with one of: {}",
                        member.spelling(),
                        enum_type.type_name,
                        enum_type.class_path,
                        enum_type.type_name,
                        enum_type.members.join(", ")
                    ),
                )),
            }
        }
        (_, found) => Err(mismatch(found.kind_name())),
    }
}

/// Why a String value cannot be carried by the wire form or the text form, if it cannot: a
/// control character other than line feed, a bidirectional override or isolate (the text form
/// refuses them, issue #482), or more than [`MAX_WIRE_STRING_BYTES`] bytes as written (escapes
/// counted). A validate-clean model therefore always reads back from its own wire bytes and text.
fn text_problem(text: &str) -> Option<String> {
    if text.chars().any(|c| c.is_control() && c != '\n') {
        return Some("carries a control character other than a line feed".to_owned());
    }
    if text.chars().any(crate::text::is_bidi_control) {
        return Some("carries a bidirectional override or isolate".to_owned());
    }
    let written = wire_string_len(text);
    (written > MAX_WIRE_STRING_BYTES).then(|| {
        format!(
            "is {written} bytes as written; the wire form carries at most {MAX_WIRE_STRING_BYTES}"
        )
    })
}

fn kind_name(value_type: ValueType) -> &'static str {
    match value_type {
        ValueType::Real => "a Real (or an Integer literal)",
        ValueType::Integer => "an Integer",
        ValueType::Boolean => "a Boolean",
        ValueType::String => "a String",
        ValueType::Enum(_) => "an enumeration member",
    }
}

#[cfg(test)]
#[path = "blocks_tests.rs"]
mod tests;
