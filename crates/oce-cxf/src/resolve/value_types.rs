//! Connector datatype recognition for CXF resolution.

use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ValueType, enum_class_id, is_g36_integer_constant_package};

use crate::dto::Node;

/// First `@type` term of a node, if any.
pub(super) fn first_type(node: &Node) -> Option<&str> {
    node.r#type
        .as_ref()
        .and_then(|types| types.as_slice().first())
        .map(String::as_str)
}

/// The trailing term of a compact or absolute IRI.
pub(super) fn term_of(iri: &str) -> &str {
    iri.rsplit([':', '#', '/']).next().unwrap_or(iri)
}

/// Map an `isOfDataType` IRI to its executable value type.
fn value_type_of_datatype(iri: &str) -> Option<ValueType> {
    match term_of(iri) {
        "Real" => Some(ValueType::Real),
        "Integer" => Some(ValueType::Integer),
        "Boolean" => Some(ValueType::Boolean),
        _ => enum_class_id(iri)
            .map(ValueType::Enum)
            .or_else(|| is_g36_integer_constant_package(iri).then_some(ValueType::Integer)),
    }
}

/// Derive a connector type, using `Real` only as dense-storage recovery after a diagnostic.
pub(super) fn derive_value_type(node: &Node, diags: &mut Vec<Diagnostic>) -> ValueType {
    try_derive_value_type(node, diags).unwrap_or(ValueType::Real)
}

/// Derive a connector type without introducing a placeholder-based follow-on mismatch.
pub(super) fn try_derive_value_type(node: &Node, diags: &mut Vec<Diagnostic>) -> Option<ValueType> {
    if let Some(datatype) = &node.is_of_data_type {
        return value_type_of_datatype(&datatype.id).or_else(|| {
            diags.push(
                Diagnostic::error(DiagCode::UnresolvedReference, "unresolved isOfDataType")
                    .with_subject(datatype.id.clone()),
            );
            None
        });
    }
    match first_type(node).map(term_of) {
        Some(term) if term.starts_with("Real") => Some(ValueType::Real),
        Some(term) if term.starts_with("Integer") => Some(ValueType::Integer),
        Some(term) if term.starts_with("Boolean") => Some(ValueType::Boolean),
        Some(term) if term.starts_with("Analog") => {
            diags.push(
                Diagnostic::warning(
                    DiagCode::AnalogCoercedToReal,
                    "Analog connector coerced to Real",
                )
                .with_subject(node.id.clone()),
            );
            Some(ValueType::Real)
        }
        Some(term) if term.starts_with("String") => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "String connector not permitted (§7.8)",
                )
                .with_subject(node.id.clone()),
            );
            None
        }
        _ => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MalformedDocument,
                    "connector lacks a recognized data type",
                )
                .with_subject(node.id.clone()),
            );
            None
        }
    }
}
