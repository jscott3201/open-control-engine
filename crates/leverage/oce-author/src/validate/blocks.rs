//! Block checks: class lookup, parameter lookup, kinds, references and the catalog rules, then
//! the block's resolved ports for the connection checks.
//!
//! The catalog rules are not re-implemented: the assignments that pass the author-side checks
//! are handed to [`oce_api::class_ports`], which runs the load gate's own parameter checks, and
//! each refusal it returns becomes a finding with the engine's code ([`oce_api::DiagCode::as_str`]).

use std::collections::BTreeSet;
use std::sync::Arc;

use oce_api::{ClassPorts, ClassPortsError, Value, ValueType, class_ports};

use crate::model::{Block, Literal, ParamValue, Sequence, SignalType};

use super::catalog::{ClassView, enum_type, lookup_class, lookup_parameter};
use super::names::literal_fits;
use super::{
    Anchor, CLASS_NOT_AUTHORABLE, CLASS_NOT_FOUND, DUPLICATE_PARAMETER, Findings,
    NON_FINITE_PARAMETER, PARAMETER_KIND_MISMATCH, PARAMETER_NOT_SETTABLE, PARAMETER_OUT_OF_RANGE,
    UNKNOWN_ENUM_LITERAL, UNKNOWN_PARAMETER, UNKNOWN_PARAMETER_REFERENCE,
};

/// Check every block; return each block's resolved ports, or `None` where they are unknown (an
/// unknown class, a refused parameter table, or an assignment withheld by an author-side
/// finding, which could change the port widths).
pub(super) fn check(sequence: &Sequence, findings: &mut Findings) -> Vec<Option<ClassPorts>> {
    sequence
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| check_block(sequence, index, block, findings))
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
    index: usize,
    block: &Block,
    findings: &mut Findings,
) -> Option<ClassPorts> {
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
            judge(sequence, &class, block, name, &assignment.value)
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

    match class_ports(class.entry.class_path, &table) {
        Ok(ports) => withheld.is_empty().then_some(ports),
        Err(ClassPortsError::InvalidParameters { diagnostics, .. }) => {
            for diagnostic in diagnostics {
                let code = diagnostic.code.as_str();
                let named = backticked(&diagnostic.message);
                if named.iter().any(|token| withheld.contains(token)) {
                    // Already reported (or withheld for a reported reason) by the author side.
                    continue;
                }
                let position = block
                    .assignments
                    .iter()
                    .position(|a| named.contains(&a.name.as_str()));
                let (position, subject) = match position {
                    Some(j) => (
                        Anchor::Block(index, Some(j)),
                        format!("{}.{}", block.name, block.assignments[j].name),
                    ),
                    None => (block_position, block.name.as_str().to_owned()),
                };
                findings.error(
                    position,
                    code,
                    subject,
                    format!("block `{}`: {}", block.name, diagnostic.message),
                );
            }
            None
        }
        Err(other) => {
            findings.error(
                block_position,
                PARAMETER_OUT_OF_RANGE,
                block.name.as_str(),
                format!("block `{}`: {other}", block.name),
            );
            None
        }
    }
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
            let Some(param) = sequence.params.iter().find(|p| p.name == *target) else {
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
                (ValueType::Real, SignalType::Real, Literal::Integer(v)) => {
                    Ok(Some(Value::Real(*v as f64)))
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
        (ValueType::Real | ValueType::Integer, Literal::Integer(v)) => Ok(Some(Value::Integer(*v))),
        (ValueType::Boolean, Literal::Boolean(v)) => Ok(Some(Value::Boolean(*v))),
        (ValueType::String, Literal::Text(v)) => Ok(Some(Value::String(Arc::from(v.as_str())))),
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
mod tests {
    use super::backticked;

    #[test]
    fn backticked_tokens_are_read_in_order() {
        assert_eq!(
            backticked("block `CDL.Reals.Hysteresis` is missing required parameter `uLow`"),
            vec!["CDL.Reals.Hysteresis", "uLow"]
        );
        assert!(backticked("no tokens").is_empty());
    }
}
