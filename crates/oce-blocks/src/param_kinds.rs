//! Which bound values a parameter's catalog type admits (issues #734, #735).
//!
//! Every registered block reads its parameters through readers that fall back to a default when a
//! parameter is absent, and, before this rule, also when it was present with a value of another
//! kind: an Integer parameter given a Real ran on its default (issue #734). A value of the wrong
//! kind is therefore refused before a block is built from it, by one predicate every gate shares:
//! [`admits_parameter_value`] against the type [`crate::parameter_value_type`] gives the
//! parameter. The load gate (`oce-validate`) reports a refusal as a diagnostic, and the
//! compiled-schedule blob decoder (`oce-graph`) refuses the blob, so no registered class is
//! constructed with a default in place of a present parameter.
//!
//! What each catalog type admits, and nothing else:
//!
//! - **Real**: a Real; an Integer up to 2^53 in magnitude ([`MAX_EXACT_INTEGER_AS_REAL`]), which
//!   every reader widens exactly (CDL's Integer-to-Real widening). A larger Integer is not exact
//!   as a binary64 in general and is [`ParameterKindFault::InexactInteger`].
//! - **Integer**: an Integer. A Real is never narrowed, `2.0` included: CDL does not convert a
//!   Real to an Integer implicitly (`parameter Integer n = 2.0` is a type error in Modelica).
//! - **Boolean**, **String**: a value of that kind.
//! - **an enumeration**: a member of that enumeration, as an enumeration value of the same class,
//!   or as one of the two established stand-ins the readers decode the same way: the member's
//!   one-based ordinal as an Integer, or a String whose last dotted segment is the member's name.
//!   An enumeration value of another class is [`ParameterKindFault::WrongKind`]; a stand-in or an
//!   ordinal naming no member is [`ParameterKindFault::UnknownMember`].
//!
//! A parameter the catalog does not type (an unregistered class, a name the class does not
//! declare) is outside this rule: [`parameter_kind_fault`] returns `None` for it.
//!
//! Total and allocation-free apart from the catalog lookup; never panics.

use oce_model::{EnumClassId, Value, ValueType, enum_descriptor};

use crate::{ParamNameTemplate, ParamRule};

/// The largest magnitude an Integer bound to a Real parameter may have: 2^53. Every integer up to
/// it is exact as a binary64; 2^53 + 1 is the first that is not.
pub const MAX_EXACT_INTEGER_AS_REAL: u64 = 1 << 53;

/// The Real of `integer`, or `None` when its magnitude is above [`MAX_EXACT_INTEGER_AS_REAL`].
/// The bound is on the magnitude, not on the exactness of the one value: `i64::MIN` is a power of
/// two and still refused, so the rule is one comparison a reader can check. Total.
#[must_use]
pub fn exact_real(integer: i64) -> Option<f64> {
    // The cast is exact for every value the comparison admits.
    (integer.unsigned_abs() <= MAX_EXACT_INTEGER_AS_REAL).then_some(integer as f64)
}

/// Why a bound value is not one its parameter's catalog type admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterKindFault {
    /// The value is of another kind than the catalog type: a Real, Boolean or String on an
    /// Integer parameter, an Integer on a Boolean one, an enumeration value of another class.
    WrongKind {
        /// The type the catalog gives the parameter.
        declared: ValueType,
    },
    /// An Integer above 2^53 in magnitude on a Real parameter: no binary64 holds it exactly in
    /// general, so it is refused rather than rounded.
    InexactInteger,
    /// An enumeration value, ordinal stand-in or String stand-in on an enumeration parameter that
    /// names no member of the enumeration.
    UnknownMember {
        /// The enumeration the catalog gives the parameter.
        declared: EnumClassId,
    },
}

/// Whether `value` is one a parameter of catalog type `declared` admits (see the module
/// documentation for the table). `Ok(())` when it is; the fault otherwise. Total; never panics.
///
/// # Errors
/// The [`ParameterKindFault`] that refuses the value.
pub fn admits_parameter_value(
    declared: ValueType,
    value: &Value,
) -> Result<(), ParameterKindFault> {
    let wrong = Err(ParameterKindFault::WrongKind { declared });
    match (declared, value) {
        (ValueType::Real, Value::Real(_))
        | (ValueType::Integer, Value::Integer(_))
        | (ValueType::Boolean, Value::Boolean(_))
        | (ValueType::String, Value::String(_)) => Ok(()),
        (ValueType::Real, Value::Integer(integer)) => exact_real(*integer)
            .map(|_| ())
            .ok_or(ParameterKindFault::InexactInteger),
        (ValueType::Enum(class), Value::Enum { class: actual, .. }) if *actual != class => wrong,
        (ValueType::Enum(class), Value::Enum { .. } | Value::Integer(_) | Value::String(_)) => {
            if enum_member_of(class, value) {
                Ok(())
            } else {
                Err(ParameterKindFault::UnknownMember { declared: class })
            }
        }
        _ => wrong,
    }
}

/// The fault of `value` bound to the parameter `name` of the registered class `class_path`, or
/// `None` when the catalog admits it or does not type the parameter (an unregistered class, a
/// name the class does not declare). `name` is the name a resolved model binds: a scalar's own
/// name or one flattened array element (`k_2`, `table_1_2`). Total; never panics.
#[must_use]
pub fn parameter_kind_fault(
    class_path: &str,
    name: &str,
    value: &Value,
) -> Option<ParameterKindFault> {
    let declared = crate::parameter_value_type(class_path, name)?;
    admits_parameter_value(declared, value).err()
}

/// Whether a parameter rule of the registered class `class_path` judges the kind of the bound
/// parameter `name` (a scalar's name or a flattened array element) whenever it is present: a
/// `Required` kind, a single-parameter bound (`Real…`, `Integer…`, `Boolean`), an `EnumMembers`
/// list, or the elements of an array or table rule. Such a rule refuses a value of another kind
/// itself, so the load gate's table-driven kind rule adds to it only what it does not check: an
/// enumeration value of another class and an Integer past 2^53 on a Real parameter.
///
/// A rule over two parameters (`RealLessOrEqual`, `IntegerProductLessOrEqualConstant`, …) and a
/// rule's length, count, shape or option parameters do not count: they read their operands only
/// when every one is present and of its kind, so a lone wrong value passed them (issue #734).
/// `false` for an unregistered class or an undeclared name. Total; never panics.
#[must_use]
pub fn parameter_kind_is_ruled(class_path: &str, name: &str) -> bool {
    let Some(entry) = crate::registry::lookup(class_path) else {
        return false;
    };
    entry.param_rules().iter().any(|rule| match *rule {
        ParamRule::Required { name: ruled, .. }
        | ParamRule::Boolean { name: ruled }
        | ParamRule::Real { name: ruled }
        | ParamRule::RealFinite { name: ruled }
        | ParamRule::RealGreaterThan { name: ruled, .. }
        | ParamRule::RealFiniteGreaterThan { name: ruled, .. }
        | ParamRule::RealGreaterOrEqual { name: ruled, .. }
        | ParamRule::RealLessOrEqualConstant { name: ruled, .. }
        | ParamRule::IntegerGreaterOrEqual { name: ruled, .. }
        | ParamRule::IntegerLessOrEqualConstant { name: ruled, .. }
        | ParamRule::EnumMembers { name: ruled, .. } => ruled == name,
        ParamRule::IntegerArrayElements { base, .. }
        | ParamRule::IntegerArrayElementsInRange { base, .. }
        | ParamRule::RealArrayElements { base, .. }
        | ParamRule::BooleanArrayElements { base, .. }
        | ParamRule::BooleanArrayTrueCountEquals { base, .. }
        | ParamRule::TimeTableOffset { base, .. } => ParamNameTemplate::Vector { base }.names(name),
        ParamRule::RealMatrixElements { base, .. } | ParamRule::TimeTableMatrix { base, .. } => {
            ParamNameTemplate::Matrix { base }.names(name)
        }
        ParamRule::Structural { .. }
        | ParamRule::StructuralArrayElements { .. }
        | ParamRule::RealTimesIntegerInclusiveRange { .. }
        | ParamRule::IntegerProductLessOrEqualConstant { .. }
        | ParamRule::RealLessOrEqual { .. }
        | ParamRule::RealLessOrEqualWarning { .. }
        | ParamRule::RealGreaterOrEqualScaledWarning { .. }
        | ParamRule::RealEqualWarning { .. } => false,
    })
}

/// Whether `value` (an enumeration value of `class`, an ordinal or a String) names a member of
/// the enumeration `class`, the way the block readers decode it.
fn enum_member_of(class: EnumClassId, value: &Value) -> bool {
    let Some(descriptor) = enum_descriptor(class) else {
        return false;
    };
    let ordinal_names_member = |ordinal: i64| {
        usize::try_from(ordinal)
            .is_ok_and(|ordinal| (1..=descriptor.members.len()).contains(&ordinal))
    };
    match value {
        Value::Enum { ordinal, .. } => ordinal_names_member(i64::from(*ordinal)),
        Value::Integer(ordinal) => ordinal_names_member(*ordinal),
        Value::String(text) => {
            let member = text.rsplit('.').next().unwrap_or(text);
            descriptor.members.contains(&member)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "param_kinds_tests.rs"]
mod tests;
