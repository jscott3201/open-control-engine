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
        validate_g36_enum_value(node, type_iri, class, value, scope, diags);
    } else if enum_class_id(type_iri).is_some() || is_g36_integer_constant_package(type_iri) {
        // A canonical CDL enumeration (judged once grounded, `hold_in_declared_type`) or a
        // canonical G36 constant package (an Integer).
    } else if let Some(diagnostic) = non_canonical_enumeration_type(type_iri) {
        diags.push(diagnostic.with_subject(node.id.clone()));
    } else if is_g36_type_path(type_iri) {
        diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumType,
                format!("unknown G36 enum type `{type_iri}`"),
            )
            .with_subject(node.id.clone()),
        );
    }
}

/// The refusal of a typing IRI that names an enumeration or constant package by a path other than
/// its canonical class path (`…#Foo.SimpleController`, `…#ZeroTime`,
/// `…#Buildings.Controls.OBC.CDL.Typos.ZeroTime`; issues #637, #762), or an unknown class under
/// the CDL `Types` package (`…#Buildings.Controls.OBC.CDL.Types.ZeroTim`). `None` for any other
/// IRI, a canonical one included. The written IRI is quoted by its excerpt, made visible; the
/// diagnostic has no subject yet. Allocates only the diagnostic.
pub(super) fn non_canonical_enumeration_type(type_iri: &str) -> Option<Diagnostic> {
    if enumeration_class_by_iri(type_iri).is_some() {
        return None;
    }
    let fragment = type_iri.rsplit(['#', '/']).next().unwrap_or(type_iri);
    let message = match enumeration_spelling_hint(type_iri) {
        Some(row) => non_canonical_enumeration_message(fragment, row.qualified_path),
        None if fragment.starts_with(CDL_TYPES_PREFIX) => format!(
            "unknown CDL enumeration type `{}`; the CDL enumerations are {}",
            oce_diag::visible(&oce_diag::excerpt(fragment)),
            oce_model::enumeration_classes()
                .iter()
                .filter(|row| row.qualified_path.starts_with(CDL_TYPES_PREFIX))
                .map(|row| row.qualified_path)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => return None,
    };
    Some(Diagnostic::error(DiagCode::UnknownEnumType, message))
}

fn validate_g36_enum_value(
    node: &Node,
    type_iri: &str,
    declared: EnumClassId,
    value: &CxfValue,
    scope: &dyn Scope,
    diags: &mut DiagnosticList,
) {
    match value {
        CxfValue::Expr(text) => {
            validate_g36_enum_expr(node, type_iri, declared, text, scope, diags)
        }
        CxfValue::Int(_) | CxfValue::Float(_) | CxfValue::Typed { .. } => diags.push(
            Diagnostic::error(
                DiagCode::EnumIntegerStandin,
                "G36 enum parameters require canonical enum literals, not numeric stand-ins",
            )
            .with_subject(node.id.clone()),
        ),
        CxfValue::Bool(_) | CxfValue::List(_) => diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumLiteral,
                "G36 enum parameter value is not a canonical enum literal",
            )
            .with_subject(node.id.clone()),
        ),
    }
}

fn validate_g36_enum_expr(
    node: &Node,
    type_iri: &str,
    declared: EnumClassId,
    text: &str,
    scope: &dyn Scope,
    diags: &mut DiagnosticList,
) {
    if g36_integer_constant(text).is_some() {
        diags.push(
            Diagnostic::error(
                DiagCode::EnumIntegerStandin,
                "G36 integer constants cannot stand in for enum literals",
            )
            .with_subject(node.id.clone()),
        );
        return;
    }
    let Some((class_path, literal)) = text.rsplit_once('.') else {
        if let Some(EvalResult::Scalar(Value::Enum { class, .. })) = scope.lookup(text) {
            if *class == declared {
                return;
            }
            diags.push(
                Diagnostic::error(
                    DiagCode::TypeMismatch,
                    format!(
                        "G36 enum parameter reference `{text}` does not match declared type `{type_iri}`"
                    ),
                )
                .with_subject(node.id.clone()),
            );
            return;
        }
        diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumLiteral,
                format!("G36 enum literal `{text}` is not fully qualified"),
            )
            .with_subject(node.id.clone()),
        );
        return;
    };
    let Some(actual) = g36_enum_class_id(class_path) else {
        if enum_class_id(class_path).is_none() && enumeration_spelling_hint(class_path).is_some() {
            // Another spelling of a known enumeration's path (issue #762): grounding the value
            // refuses it once, `unknown-enum-type` naming the canonical path.
            return;
        }
        let code = if is_g36_type_path(class_path) {
            DiagCode::UnknownEnumType
        } else {
            DiagCode::TypeMismatch
        };
        diags.push(
            Diagnostic::error(code, format!("`{text}` is not a literal of `{type_iri}`"))
                .with_subject(node.id.clone()),
        );
        return;
    };
    if actual != declared {
        diags.push(
            Diagnostic::error(
                DiagCode::TypeMismatch,
                format!("G36 enum literal `{text}` does not match declared type `{type_iri}`"),
            )
            .with_subject(node.id.clone()),
        );
        return;
    }
    if enum_member_ordinal(actual, literal).is_none() {
        diags.push(
            Diagnostic::error(
                DiagCode::UnknownEnumLiteral,
                format!("unknown G36 enum literal `{text}`"),
            )
            .with_subject(node.id.clone()),
        );
    }
}
