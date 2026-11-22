//! A Real parameter holds a Real, and every reader of it sees that Real (issues #683, #735).
//!
//! Grounding is literal-natural: a bare JSON integer, a typed `xsd:integer` literal, an Integer
//! expression and a reference to an Integer-valued parameter all ground to [`Value::Integer`],
//! whatever the parameter they are bound to. CDL widens an Integer bound to a Real parameter, and
//! every block constructor already reads it that way, so the two spellings of one number (`0` and
//! `0.0`) ran identically while the resolved model, the parameter table, the compiled-schedule
//! blob and the canonical content id told them apart (issue #683).
//!
//! Each binding is therefore held in its parameter's type **as it is grounded, before anything
//! reads it**: before it enters the parameter table, and before it enters the scope a later
//! binding evaluates against (the sibling members of the same instance, or the composite scope
//! its children inherit). Modelica's rule is the owner's ruling (issue #735): the value of
//! `parameter Real period = 4` is the Real 4.0, so a later `parameter Integer amplitude = period`
//! reads a Real (and the load gate refuses it as the wrong kind), in both spellings alike.
//!
//! - A block member ([`hold_in_catalog_type`]): an Integer bound to a parameter the block catalog
//!   types Real ([`oce_blocks::parameter_value_type`]: scalars and flattened array elements alike)
//!   becomes the Real of the same value.
//! - A composite (sequence) parameter ([`hold_in_declared_type`]): an Integer bound to a
//!   parameter whose `S231:isOfDataType` is `S231:Real` becomes the Real of the same value. A
//!   value of another kind than the declared Real, Integer or Boolean (a Real on a declared
//!   Integer, `2.0` included; a Boolean on a declared Real) is refused with
//!   [`DiagCode::ParameterKindMismatch`] on the declaration: no binding of it reaches a reader.
//!
//! The conversion is refused, never rounded: an Integer above 2^53 in magnitude
//! ([`oce_blocks::MAX_EXACT_INTEGER_AS_REAL`]) is not exact as a binary64 in general, so it is
//! [`DiagCode::ParameterOutOfRange`] on the parameter node that bound it, the code the authoring
//! stage uses for the same literal. The bound is the magnitude, not exactness of the one value:
//! `i64::MIN` is a power of two and still refused.
//!
//! Nothing else moves. An Integer on an Integer, enumeration or untyped parameter stays an
//! Integer; a Real stays the Real it was, `-0.0` included. No Integer spells a negative zero: a
//! typed `xsd:integer` of `-0` is the Integer zero and so `+0.0`, while a bare JSON `-0` is
//! already the float `-0.0` to the JSON reader and was never an Integer. A refused binding is
//! kept as grounded so the rest of the document is still checked; the document is refused, so the
//! value reaches nothing.

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{Value, ValueType};

use oce_diag::DiagnosticList;

use crate::dto::Node;

use super::value_types::declared_parameter_type;

/// `value`, bound to the parameter `name` of a block of class `class_path`, held in the type the
/// block catalog gives that parameter: the Real of an Integer on a Real parameter, `value`
/// itself otherwise. `owner` is the instance and `source` the parameter node that bound it (one
/// node binds every element of a preserved array); an Integer that cannot be converted exactly
/// is refused on `source` and returned as grounded.
pub(crate) fn hold_in_catalog_type(
    class_path: &str,
    owner: &str,
    source: &str,
    name: &str,
    value: Value,
    diags: &mut DiagnosticList,
) -> Value {
    let Value::Integer(integer) = value else {
        return value;
    };
    if oce_blocks::parameter_value_type(class_path, name) != Some(ValueType::Real) {
        return value;
    }
    match oce_blocks::exact_real(integer) {
        Some(real) => Value::Real(real),
        None => {
            // Raised, not pushed: every element of one array can be refused, and each message
            // names the owner, so nothing is built once the list has ended.
            diags.raise(|| {
                Diagnostic::error(
                    DiagCode::ParameterOutOfRange,
                    format!(
                        "parameter `{name}` on `{owner}` binds the Integer {integer} to a Real \
                         parameter of `{class_path}`; an Integer above 2^53 in magnitude is not \
                         exact as a Real"
                    ),
                )
                .with_subject(source.to_owned())
            });
            value
        }
    }
}

/// Why a composite parameter's grounded value does not fit its declared type.
#[derive(Debug, PartialEq)]
pub(super) enum DeclaredTypeFault {
    /// An Integer above 2^53 in magnitude on a parameter declared Real.
    InexactInteger(i64),
    /// A value of another kind than the declared Real, Integer or Boolean, or a value that is no
    /// member of the declared CDL enumeration.
    WrongKind {
        /// The declared type.
        declared: ValueType,
    },
}

impl DeclaredTypeFault {
    /// The diagnostic refusing the declaration `iri` (local name `name`) for this fault.
    pub(super) fn diagnostic(&self, iri: &str, name: &str, value: &Value) -> Diagnostic {
        let (code, message) = match self {
            Self::InexactInteger(integer) => (
                DiagCode::ParameterOutOfRange,
                format!(
                    "parameter `{name}` is declared Real and binds the Integer {integer}; an \
                     Integer above 2^53 in magnitude is not exact as a Real"
                ),
            ),
            Self::WrongKind {
                declared: ValueType::Enum(class),
            } => (
                DiagCode::ParameterKindMismatch,
                format!(
                    "parameter `{name}` is declared the enumeration `{}` and binds a value that \
                     is not one of its members; write `{}.<member>`",
                    enumeration_path(*class),
                    enumeration_path(*class)
                ),
            ),
            Self::WrongKind { declared } => (
                DiagCode::ParameterKindMismatch,
                format!(
                    "parameter `{name}` is declared {declared:?} and binds a value of kind {:?}",
                    value.value_type()
                ),
            ),
        };
        Diagnostic::error(code, message).with_subject(iri.to_owned())
    }
}

/// The canonical class path of the enumeration `class` (engine text), for a refusal.
fn enumeration_path(class: oce_model::EnumClassId) -> &'static str {
    oce_model::enumeration_class_by_id(class).map_or("?", |row| row.qualified_path)
}

/// `value`, bound to the composite parameter `node`, held in the type its `S231:isOfDataType`
/// declares: the Real of an Integer on a declared Real. `Err` names the fault for a value of
/// another kind than the declared Real, Integer or Boolean, an Integer a declared Real cannot
/// hold exactly, or a value that is no member of the declared CDL enumeration (issue #762: an
/// ordinal, a string, another enumeration's member); the caller reports it (or not, in the
/// specialize pass) and keeps `value`. A parameter with no declared type, or one declared a G36
/// enumeration (judged before grounding by `enumeration_parameters`), is returned untouched.
/// Total; never panics.
pub(super) fn hold_in_declared_type(
    node: &Node,
    value: Value,
) -> Result<Value, (DeclaredTypeFault, Value)> {
    let Some(declared) = declared_parameter_type(node) else {
        return Ok(value);
    };
    match (declared, &value) {
        (ValueType::Real, Value::Integer(integer)) => match oce_blocks::exact_real(*integer) {
            Some(real) => Ok(Value::Real(real)),
            None => Err((DeclaredTypeFault::InexactInteger(*integer), value)),
        },
        (ValueType::Real, Value::Real(_))
        | (ValueType::Integer, Value::Integer(_))
        | (ValueType::Boolean, Value::Boolean(_))
        | (ValueType::String, _) => Ok(value),
        (ValueType::Enum(class), Value::Enum { class: bound, .. }) if *bound == class => Ok(value),
        (ValueType::Enum(class), _) if oce_model::g36_enum_literals(class).is_some() => Ok(value),
        (ValueType::Enum(_), _) => Err((DeclaredTypeFault::WrongKind { declared }, value)),
        (ValueType::Real | ValueType::Integer | ValueType::Boolean, _) => {
            Err((DeclaredTypeFault::WrongKind { declared }, value))
        }
    }
}
