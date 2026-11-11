//! Named types (importer note, "Sequence parameters of an enumeration type" and "Library
//! constants as block parameter values"; issue #492): resolving a `Type.Member` and an
//! enumeration parameter type through the facade's one table, `oce_api::enumerations()`.
//!
//! A type name is the last segment of a row's class path, unique across the table (pinned by
//! the facade), so a `Type.Member` spelling names at most one row: an enumeration (whose members
//! are ordinals) or an Integer constant package (whose members are Integers). The type name
//! decides which, never the parameter it is assigned to.

use oce_api::{EnumerationClass, EnumerationKind, MemberValue, enumerations};

use crate::model::{Literal, MemberRef, ParameterType, SequenceParameter};

use super::names::{exact_as_real, inexact_detail};
use super::suffixes;
use super::{
    NON_FINITE_PARAMETER, PARAMETER_KIND_MISMATCH, PARAMETER_OUT_OF_RANGE, UNIT_SUFFIX_MISMATCH,
    UNKNOWN_ENUM_LITERAL, UNKNOWN_ENUM_TYPE,
};

/// The row whose type name ([`EnumerationClass::type_name`]) is exactly `type_name`, enumeration
/// or constant package. `None` for any other spelling.
pub(crate) fn named_type(type_name: &str) -> Option<&'static EnumerationClass> {
    enumerations()
        .iter()
        .find(|row| row.type_name() == type_name)
}

/// The enumeration row a sequence parameter type names: `Ok(None)` for a scalar type,
/// `Ok(Some(row))` for a known enumeration, `Err(detail)` for an unknown name or a constant
/// package (never a parameter type).
pub(crate) fn parameter_enum(
    value_type: &ParameterType,
) -> Result<Option<&'static EnumerationClass>, String> {
    let ParameterType::Enum(type_name) = value_type else {
        return Ok(None);
    };
    match named_type(type_name) {
        Some(row) if row.kind == EnumerationKind::Enumeration => Ok(Some(row)),
        Some(row) => Err(format!(
            "`{type_name}` ({}) is a constant package, never a parameter type; declare the parameter `Integer` and assign `{type_name}.<member>`",
            row.class_path
        )),
        None => Err(format!(
            "`{type_name}` is not a type; write `Real`, `Integer`, `Boolean` or an enumeration: {}",
            oce_api::enumeration_types()
                .map(EnumerationClass::type_name)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The detail of a member that is not one of `row`'s.
pub(crate) fn not_a_member(member: &MemberRef, row: &EnumerationClass) -> String {
    format!(
        "`{}` is not a member of `{}` ({}); write `{}.<member>` with one of: {}",
        member.spelling(),
        row.type_name(),
        row.class_path,
        row.type_name(),
        row.members.join(", ")
    )
}

/// The detail of a member whose type names no row.
pub(crate) fn unknown_member_type(member: &MemberRef) -> String {
    format!(
        "`{}` names no enumeration or constant package the engine knows",
        member.spelling()
    )
}

/// The Integer a constant-package member stands for, judged for an Integer slot: `Ok(value)`,
/// or `Err((code, detail))` for an enumeration member (`parameter-kind-mismatch`) and for an
/// unknown type or member (`unknown-enum-literal`).
pub(crate) fn constant_member(
    member: &MemberRef,
    slot: &str,
) -> Result<i64, (&'static str, String)> {
    let Some(row) = named_type(&member.type_name) else {
        return Err((UNKNOWN_ENUM_LITERAL, unknown_member_type(member)));
    };
    match row.member_value(&member.member) {
        Some(MemberValue::Integer(value)) => Ok(value),
        Some(_) | None if row.kind == EnumerationKind::Enumeration => Err((
            PARAMETER_KIND_MISMATCH,
            format!(
                "{slot} takes an Integer; `{}` is a member of the enumeration `{}`",
                member.spelling(),
                row.type_name()
            ),
        )),
        _ => Err((UNKNOWN_ENUM_LITERAL, not_a_member(member, row))),
    }
}

/// The one-based ordinal of `member` in the enumeration `row`, or the
/// `unknown-enum-literal` detail when it is not one of `row`'s members (another type, a constant,
/// an unknown name).
pub(crate) fn enum_ordinal(member: &MemberRef, row: &EnumerationClass) -> Result<u32, String> {
    if member.type_name != row.type_name() {
        return Err(not_a_member(member, row));
    }
    match row.member_value(&member.member) {
        Some(MemberValue::Ordinal(ordinal)) => Ok(ordinal),
        _ => Err(not_a_member(member, row)),
    }
}

/// What is wrong with a sequence parameter's unit suffix, declared type and value, if anything:
/// the finding code and its detail. `None` means stage 1 accepts the parameter, so a block may take a
/// reference to it.
pub(crate) fn parameter_problem(param: &SequenceParameter) -> Option<(&'static str, String)> {
    let name = &param.name;
    // A sequence parameter declares no unit: a base-unit suffix stays an annotation, a scaled
    // one has nothing to scale to (vocabulary note, "Units").
    if let Some(suffix) = &param.suffix
        && let Some(detail) =
            suffixes::mismatch(&format!("sequence parameter `{name}`"), "", suffix)
    {
        return Some((UNIT_SUFFIX_MISMATCH, detail));
    }
    let declared = param.value_type.as_str();
    let mismatch = || {
        Some((
            PARAMETER_KIND_MISMATCH,
            format!(
                "sequence parameter `{name}` is declared {declared} but its value is {}",
                param.value.kind_name()
            ),
        ))
    };
    let row = match parameter_enum(&param.value_type) {
        Ok(row) => row,
        Err(detail) => {
            return Some((
                UNKNOWN_ENUM_TYPE,
                format!("sequence parameter `{name}`: {detail}"),
            ));
        }
    };
    match (&param.value_type, &param.value) {
        (ParameterType::Enum(_), Literal::Member(member)) => {
            let row = row?;
            enum_ordinal(member, row).err().map(|detail| {
                (
                    UNKNOWN_ENUM_LITERAL,
                    format!("sequence parameter `{name}`: {detail}"),
                )
            })
        }
        (ParameterType::Real, Literal::Real(value)) if !value.is_finite() => Some((
            NON_FINITE_PARAMETER,
            format!("sequence parameter `{name}` is {value}"),
        )),
        (ParameterType::Real, Literal::Integer(value)) if !exact_as_real(*value) => Some((
            PARAMETER_OUT_OF_RANGE,
            inexact_detail(&format!("sequence parameter `{name}`"), *value),
        )),
        (ParameterType::Real, Literal::Real(_) | Literal::Integer(_))
        | (ParameterType::Integer, Literal::Integer(_))
        | (ParameterType::Boolean, Literal::Boolean(_)) => None,
        (ParameterType::Integer, Literal::Member(member)) => {
            constant_member(member, &format!("sequence parameter `{name}`"))
                .err()
                .map(|(code, detail)| {
                    let detail = if code == UNKNOWN_ENUM_LITERAL {
                        format!("sequence parameter `{name}`: {detail}")
                    } else {
                        detail
                    };
                    (code, detail)
                })
        }
        _ => mismatch(),
    }
}
