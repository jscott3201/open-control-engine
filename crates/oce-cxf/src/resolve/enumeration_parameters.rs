//! Parameter nodes typed by an enumeration (issues #637, #762; G36 profile, issue #468).
//!
//! A parameter's `S231:isOfDataType` names an enumeration or constant package by its canonical
//! class path only (`oce_model::enumeration_class_by_iri`): `…#Buildings.Controls.OBC.CDL.Types.ZeroTime`
//! types the parameter, while `…#ZeroTime`, `…#Foo.ZeroTime`, `…#Modelica.Blocks.Types.ZeroTime`,
//! a CDL enumeration under `…ASHRAE.G36.Types.` or a typo in the package
//! (`…#Buildings.Controls.OBC.CDL.Typos.ZeroTime`) is refused `unknown-enum-type`, naming the
//! canonical path, on the parameter node. Before, a CDL enumeration was matched by its last
//! segment, and an IRI that matched nothing typed nothing, so the value loaded as written.
//!
//! A G36 enumeration's value is judged here, before grounding, with the profile's own codes
//! (`enum-integer-standin`, `unknown-enum-literal`, `type-mismatch`). A CDL enumeration's value is
//! judged once grounded (`real_parameters::hold_in_declared_type`): a value that is not a member
//! of the declared enumeration is `parameter-kind-mismatch`.

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::{EvalResult, Scope};
use oce_model::{
    EnumClassId, Value, enum_class_id, enum_member_ordinal, enumeration_class_by_iri,
    enumeration_spelling_hint, g36_enum_class_id, g36_integer_constant,
    is_g36_integer_constant_package, is_g36_type_path,
};

use crate::dto::{CxfValue, Node};
use crate::ground::non_canonical_enumeration_message;
use oce_diag::DiagnosticList;

const CDL_TYPES_PREFIX: &str = "Buildings.Controls.OBC.CDL.Types.";

/// Judge a parameter node's declared enumeration type, and a G36 enumeration's value, before
/// generic grounding (see the module documentation). Raises nothing for a parameter that
/// declares no type, a scalar type, a canonical CDL enumeration (its value is judged once
/// grounded) or a canonical G36 Integer constant package.
///
/// It runs once for each instance that lists the node, and a refusal quotes the node's value or
/// typing IRI, so each refusal is built only when `diags` lists it: once the list has ended, a
/// further listing counts its refusal and builds nothing (issue #799).
pub(super) fn validate_enumeration_parameter_value(
    node: &Node,
    value: &CxfValue,
    scope: &dyn Scope,
    diags: &mut DiagnosticList,
) {
    let Some(type_iri) = node.is_of_data_type.as_ref().map(|iri| iri.id.as_str()) else {
        return;
    };
    if let Some(class) = g36_enum_class_id(type_iri) {
        if let Some((code, refusal)) = g36_enum_value_refusal(type_iri, class, value, scope) {
            diags.raise(|| Diagnostic::error(code, refusal.text()).with_subject(node.id.clone()));
        }
    } else if enum_class_id(type_iri).is_some() || is_g36_integer_constant_package(type_iri) {
        // A canonical CDL enumeration (judged once grounded, `hold_in_declared_type`) or a
        // canonical G36 constant package (an Integer).
    } else if let Some(refusal) = non_canonical_enumeration_type(type_iri) {
        diags.raise(|| refusal.diagnostic().with_subject(node.id.clone()));
    } else if is_g36_type_path(type_iri) {
        diags.raise(|| {
            Diagnostic::error(
                DiagCode::UnknownEnumType,
                format!("unknown G36 enum type `{type_iri}`"),
            )
            .with_subject(node.id.clone())
        });
    }
}

/// A typing IRI that names an enumeration or constant package by a path other than its
/// canonical class path, or an unknown class under the CDL `Types` package
/// ([`non_canonical_enumeration_type`]): what its refusal names, its message not yet built.
pub(super) enum NonCanonicalType<'a> {
    /// Another spelling of a known class: the written fragment and the class's canonical path.
    Spelling {
        fragment: &'a str,
        canonical: &'static str,
    },
    /// An unknown class under the CDL `Types` package: the written fragment.
    UnknownCdl { fragment: &'a str },
}

impl NonCanonicalType<'_> {
    /// The refusal, with no subject yet: `unknown-enum-type`, the written IRI quoted by its
    /// excerpt, made visible.
    pub(super) fn diagnostic(&self) -> Diagnostic {
        let message = match *self {
            NonCanonicalType::Spelling {
                fragment,
                canonical,
            } => non_canonical_enumeration_message(fragment, canonical),
            NonCanonicalType::UnknownCdl { fragment } => format!(
                "unknown CDL enumeration type `{}`; the CDL enumerations are {}",
                oce_diag::visible(&oce_diag::excerpt(fragment)),
                oce_model::enumeration_classes()
                    .iter()
                    .filter(|row| row.qualified_path.starts_with(CDL_TYPES_PREFIX))
                    .map(|row| row.qualified_path)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        Diagnostic::error(DiagCode::UnknownEnumType, message)
    }
}

/// Whether a typing IRI names an enumeration or constant package by a path other than its
/// canonical class path (`…#Foo.SimpleController`, `…#ZeroTime`,
/// `…#Buildings.Controls.OBC.CDL.Typos.ZeroTime`; issues #637, #762), or an unknown class under
/// the CDL `Types` package (`…#Buildings.Controls.OBC.CDL.Types.ZeroTim`), and what its refusal
/// names. `None` for any other IRI, a canonical one included. Allocates nothing: the refusal is
/// built by [`NonCanonicalType::diagnostic`].
pub(super) fn non_canonical_enumeration_type(type_iri: &str) -> Option<NonCanonicalType<'_>> {
    if enumeration_class_by_iri(type_iri).is_some() {
        return None;
    }
    let fragment = type_iri.rsplit(['#', '/']).next().unwrap_or(type_iri);
    match enumeration_spelling_hint(type_iri) {
        Some(row) => Some(NonCanonicalType::Spelling {
            fragment,
            canonical: row.qualified_path,
        }),
        None if fragment.starts_with(CDL_TYPES_PREFIX) => {
            Some(NonCanonicalType::UnknownCdl { fragment })
        }
        None => None,
    }
}

/// The message of a G36 enumeration value's refusal, not yet built: each quotes the value's text
/// or the typing IRI, so it is spelled only when a list takes it.
enum G36Refusal<'a> {
    /// A numeric or typed-literal stand-in.
    NumericStandIn,
    /// A Boolean or a list.
    NotALiteral,
    /// A G36 integer constant written for the enumeration.
    IntegerConstant,
    /// A bare name that resolves to an enumeration value of another type.
    ReferenceMismatch { text: &'a str, type_iri: &'a str },
    /// A bare name that resolves to no enumeration value.
    NotQualified { text: &'a str },
    /// A qualified name whose class is no G36 enumeration.
    NotALiteralOf { text: &'a str, type_iri: &'a str },
    /// A literal of another G36 enumeration.
    LiteralMismatch { text: &'a str, type_iri: &'a str },
    /// No member of the declared enumeration.
    UnknownLiteral { text: &'a str },
}

impl G36Refusal<'_> {
    /// The message.
    fn text(&self) -> String {
        match *self {
            G36Refusal::NumericStandIn => {
                "G36 enum parameters require canonical enum literals, not numeric stand-ins"
                    .to_owned()
            }
            G36Refusal::NotALiteral => {
                "G36 enum parameter value is not a canonical enum literal".to_owned()
            }
            G36Refusal::IntegerConstant => {
                "G36 integer constants cannot stand in for enum literals".to_owned()
            }
            G36Refusal::ReferenceMismatch { text, type_iri } => format!(
                "G36 enum parameter reference `{text}` does not match declared type `{type_iri}`"
            ),
            G36Refusal::NotQualified { text } => {
                format!("G36 enum literal `{text}` is not fully qualified")
            }
            G36Refusal::NotALiteralOf { text, type_iri } => {
                format!("`{text}` is not a literal of `{type_iri}`")
            }
            G36Refusal::LiteralMismatch { text, type_iri } => {
                format!("G36 enum literal `{text}` does not match declared type `{type_iri}`")
            }
            G36Refusal::UnknownLiteral { text } => format!("unknown G36 enum literal `{text}`"),
        }
    }
}

/// How a G36 enumeration's `value` is refused, if it is: the code and the message to build.
fn g36_enum_value_refusal<'a>(
    type_iri: &'a str,
    declared: EnumClassId,
    value: &'a CxfValue,
    scope: &dyn Scope,
) -> Option<(DiagCode, G36Refusal<'a>)> {
    match value {
        CxfValue::Expr(text) => g36_enum_expr_refusal(type_iri, declared, text, scope),
        CxfValue::Int(_) | CxfValue::Float(_) | CxfValue::Typed { .. } => {
            Some((DiagCode::EnumIntegerStandin, G36Refusal::NumericStandIn))
        }
        CxfValue::Bool(_) | CxfValue::List(_) => {
            Some((DiagCode::UnknownEnumLiteral, G36Refusal::NotALiteral))
        }
    }
}

fn g36_enum_expr_refusal<'a>(
    type_iri: &'a str,
    declared: EnumClassId,
    text: &'a str,
    scope: &dyn Scope,
) -> Option<(DiagCode, G36Refusal<'a>)> {
    if g36_integer_constant(text).is_some() {
        return Some((DiagCode::EnumIntegerStandin, G36Refusal::IntegerConstant));
    }
    let Some((class_path, literal)) = text.rsplit_once('.') else {
        if let Some(EvalResult::Scalar(Value::Enum { class, .. })) = scope.lookup(text) {
            if *class == declared {
                return None;
            }
            return Some((
                DiagCode::TypeMismatch,
                G36Refusal::ReferenceMismatch { text, type_iri },
            ));
        }
        return Some((
            DiagCode::UnknownEnumLiteral,
            G36Refusal::NotQualified { text },
        ));
    };
    let Some(actual) = g36_enum_class_id(class_path) else {
        if class_path.trim() == class_path
            && enum_class_id(class_path).is_none()
            && enumeration_spelling_hint(class_path).is_some()
        {
            // Another spelling of a known enumeration's path (issue #762): grounding the value
            // refuses it once, `unknown-enum-type` naming the canonical path. A path with
            // whitespace around it keeps the profile's own refusal (`type-mismatch`), as before.
            return None;
        }
        let code = if is_g36_type_path(class_path) {
            DiagCode::UnknownEnumType
        } else {
            DiagCode::TypeMismatch
        };
        return Some((code, G36Refusal::NotALiteralOf { text, type_iri }));
    };
    if actual != declared {
        return Some((
            DiagCode::TypeMismatch,
            G36Refusal::LiteralMismatch { text, type_iri },
        ));
    }
    if enum_member_ordinal(actual, literal).is_none() {
        return Some((
            DiagCode::UnknownEnumLiteral,
            G36Refusal::UnknownLiteral { text },
        ));
    }
    None
}
