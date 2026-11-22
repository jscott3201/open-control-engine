//! Every parameter of a registered class holds a value of the kind its catalog type admits
//! (issue #734).
//!
//! The block readers fall back to a default for an absent parameter, and used to for a present
//! one of another kind too, so a document that gave `Integers.Sources.Pulse.amplitude` a Real
//! loaded clean and ran on the default amplitude. This rule is table-driven: for every parameter a
//! registered class's catalog types ([`oce_blocks::parameter_value_type`], scalars and flattened
//! array elements alike), the bound value must be one [`oce_blocks::admits_parameter_value`]
//! admits, the predicate the compiled-schedule blob decoder applies too. A class added to the
//! registry is covered by the same table, with no rule to write.
//!
//! - A value of the wrong kind is [`DiagCode::ParameterKindMismatch`]: a Real (`2.0` included),
//!   Boolean or String on an Integer parameter, an Integer on a Boolean one, an enumeration value
//!   of another class.
//! - An Integer above 2^53 in magnitude on a Real parameter is [`DiagCode::ParameterOutOfRange`],
//!   the code the CXF resolver refuses the same binding with.
//! - An ordinal or String stand-in on an enumeration parameter that names no member is
//!   [`DiagCode::ParameterOutOfRange`], unless the class's `EnumMembers` rule for the parameter
//!   reports it already; an enumeration value whose ordinal is outside its own class is reported
//!   by the enumeration-ordinal check beside this one.
//!
//! A class rule that types a parameter (a `Required` kind, a bound, an array element rule) judges
//! that parameter's kind itself and keeps its own finding; this rule then adds only the two faults
//! no class rule checks, an enumeration value of another class and an Integer past 2^53 on a Real
//! parameter. Every other parameter, the ones typed by their default alone, is judged here, which
//! is where the silent defaults were: `Pulse.amplitude`, the threshold `t` of the comparators,
//! `PID.k` and its siblings, and the rest the catalog-wide test lists. The subject is the block, as
//! for every class-level parameter finding, and the message names the parameter. A parameter the
//! catalog does not type is outside the rule.

use oce_blocks::{ParamRule, ParameterKindFault, parameter_kind_fault, parameter_kind_is_ruled};
use oce_diag::{DiagCode, Diagnostic, DiagnosticList};
use oce_model::{BlockInstance, Value, enum_descriptor};

use super::block_subject_of;

/// Report every parameter of `blk` whose value its catalog type does not admit and no class rule
/// already refuses, in parameter order. Total and panic-free.
pub(super) fn check_parameter_kinds(
    blk: &BlockInstance,
    rules: &[ParamRule],
    diags: &mut DiagnosticList,
) {
    for (name, value) in &blk.params.values {
        let Some(fault) = parameter_kind_fault(&blk.class_iri, name, value) else {
            continue;
        };
        let class = &blk.class_iri;
        let (code, message) = match fault {
            // A rule that judges the parameter's kind refuses a value of another kind itself,
            // except an enumeration value of another class (`EnumMembers` reads its ordinal).
            ParameterKindFault::WrongKind { .. }
                if !matches!(value, Value::Enum { .. }) && parameter_kind_is_ruled(class, name) =>
            {
                continue;
            }
            ParameterKindFault::WrongKind { declared } => (
                DiagCode::ParameterKindMismatch,
                format!(
                    "parameter `{name}` on block `{class}` must have kind {declared:?}; got {}",
                    value_kind_name(value)
                ),
            ),
            ParameterKindFault::InexactInteger => (
                DiagCode::ParameterOutOfRange,
                format!(
                    "parameter `{name}` on block `{class}` binds the Integer {} to a Real \
                     parameter; an Integer above 2^53 in magnitude is not exact as a Real",
                    integer_text(value)
                ),
            ),
            ParameterKindFault::UnknownMember { declared } => {
                let reported_elsewhere = matches!(value, Value::Enum { .. })
                    || rules.iter().any(|rule| {
                        matches!(rule, ParamRule::EnumMembers { name: ruled, .. } if *ruled == name.as_ref())
                    });
                if reported_elsewhere {
                    continue;
                }
                let (members, path) = enum_descriptor(declared).map_or((0, ""), |descriptor| {
                    (descriptor.members.len(), descriptor.class_path)
                });
                (
                    DiagCode::ParameterOutOfRange,
                    format!(
                        "parameter `{name}` on block `{class}` must name one of the {members} \
                         members of `{path}`"
                    ),
                )
            }
        };
        diags.raise(|| Diagnostic::error(code, message).with_subject(block_subject_of(blk)));
    }
}

/// The kind of a value as a parameter-kind finding names it.
fn value_kind_name(value: &Value) -> &'static str {
    match value {
        Value::Real(_) => "Real",
        Value::Integer(_) => "Integer",
        Value::Boolean(_) => "Boolean",
        Value::String(_) => "String",
        Value::Enum { .. } => "Enum",
    }
}

/// The decimal text of an Integer value (the only kind [`ParameterKindFault::InexactInteger`]
/// names).
fn integer_text(value: &Value) -> String {
    match value {
        Value::Integer(integer) => integer.to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
#[path = "params_kinds_tests.rs"]
mod tests;
