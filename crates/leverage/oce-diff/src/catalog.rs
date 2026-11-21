//! What the diff reads from the facade's catalog, and nothing more (diff note, decision 10): a
//! parameter's literal default, whether a parameter is a Real, and the catalog's content id.
//!
//! The diff never grounds a reference, resolves a port or builds a block. It asks the catalog two
//! questions about an assignment, both answered from `oce_api::class_metadata` alone:
//!
//! - **Is this value the parameter's literal default?** Only a scalar parameter with a literal
//!   default can say yes. A derived default is an expression the diff never evaluates, a required
//!   parameter has no default, and an element of an array parameter is part of the array's shape,
//!   so dropping one is never read as "back to the default".
//! - **Is this parameter a Real?** An Integer literal and the equal Real literal are one value
//!   there and two different ones anywhere else.
//!
//! A class or a parameter the catalog does not know answers no to both.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use oce_api::{
    CatalogDefault, ClassMetadata, ClassParameter, PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER,
    PARAM_ROW_PLACEHOLDER, ValueType, catalog, catalog_content_id, class_metadata,
    enumeration_by_id, param_element_name,
};
use oce_author::Literal;

use crate::authoring::lower::exact_real;

/// The content id of the catalog this build reads, computed once.
pub(crate) fn content_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| catalog_content_id(catalog()))
}

fn class(class_path: &str) -> Option<&'static ClassMetadata> {
    static INDEX: OnceLock<BTreeMap<&'static str, &'static ClassMetadata>> = OnceLock::new();
    INDEX
        .get_or_init(|| {
            class_metadata()
                .iter()
                .map(|metadata| (metadata.class_path, metadata))
                .collect()
        })
        .get(class_path)
        .copied()
}

const PLACEHOLDERS: [&str; 3] = [
    PARAM_INDEX_PLACEHOLDER,
    PARAM_ROW_PLACEHOLDER,
    PARAM_COL_PLACEHOLDER,
];

fn is_template(name: &str) -> bool {
    PLACEHOLDERS
        .iter()
        .any(|placeholder| name.contains(placeholder))
}

/// The scalar parameter `name` of `class_path`, by its exact name.
fn scalar(class_path: &str, name: &str) -> Option<&'static ClassParameter> {
    class(class_path)?
        .parameters
        .iter()
        .find(|parameter| parameter.name == name && !is_template(parameter.name))
}

/// The array parameter of `class_path` that `name` is an element of (`k_2` of `k_<i>`,
/// `K_1_2` of `K_<row>_<col>`): the template's fixed text matched around runs of digits, and the
/// spelling confirmed by the facade's own `param_element_name`.
fn element(class_path: &str, name: &str) -> Option<&'static ClassParameter> {
    class(class_path)?.parameters.iter().find(|parameter| {
        is_template(parameter.name)
            && indices(parameter.name, name)
                .and_then(|indices| param_element_name(parameter.name, &indices))
                .is_some_and(|expanded| expanded == name)
    })
}

/// The 1-based indices `name` fills into `template`, or `None` when it does not fit.
fn indices(template: &str, name: &str) -> Option<Vec<usize>> {
    let mut indices = Vec::new();
    let (mut template, mut name) = (template, name);
    loop {
        let next = PLACEHOLDERS
            .iter()
            .filter_map(|placeholder| template.find(placeholder).map(|at| (at, placeholder.len())))
            .min();
        let Some((at, length)) = next else {
            return (template == name && !indices.is_empty()).then_some(indices);
        };
        name = name.strip_prefix(template.get(..at)?)?;
        let digits = name.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || name.starts_with('0') {
            return None;
        }
        indices.push(name.get(..digits)?.parse().ok()?);
        name = name.get(digits..)?;
        template = template.get(at + length..)?;
    }
}

/// Whether the parameter `name` of `class_path` (a scalar or an array element) is a Real.
pub(crate) fn is_real(class_path: &str, name: &str) -> bool {
    scalar(class_path, name)
        .or_else(|| element(class_path, name))
        .is_some_and(|parameter| parameter.value_type == ValueType::Real)
}

/// Whether `value`, assigned to the scalar parameter `name` of `class_path`, is that parameter's
/// literal default: a Real by its bits (an Integer literal as the Real it means), an enumeration
/// member by its type and name.
pub(crate) fn is_literal_default(class_path: &str, name: &str, value: &Literal) -> bool {
    let Some(parameter) = scalar(class_path, name) else {
        return false;
    };
    match (&parameter.default, value) {
        (CatalogDefault::Real(default), Literal::Real(real)) => default.to_bits() == real.to_bits(),
        (CatalogDefault::Real(default), Literal::Integer(integer)) => {
            exact_real(*integer).is_some_and(|real| real.to_bits() == default.to_bits())
        }
        (CatalogDefault::Integer(default), Literal::Integer(integer)) => default == integer,
        (CatalogDefault::Boolean(default), Literal::Boolean(boolean)) => default == boolean,
        (CatalogDefault::String(default), Literal::Text(text)) => default == text,
        (CatalogDefault::EnumMember(default), Literal::Member(member)) => {
            let ValueType::Enum(id) = parameter.value_type else {
                return false;
            };
            *default == member.member
                && enumeration_by_id(id).is_some_and(|class| class.type_name() == member.type_name)
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
