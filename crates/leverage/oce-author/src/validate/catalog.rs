//! The catalog view stage 1 judges against: class lookup, parameter lookup (array templates
//! included) and enumeration members, all read through `oce-api`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use oce_api::{
    CatalogEntry, ClassMetadata, ClassParameter, EnumerationClass, EnumerationKind,
    PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER, PARAM_ROW_PLACEHOLDER, ValueType, catalog,
    class_metadata, enumeration_by_id, param_element_name,
};

/// One registered class: its catalog entry and its authoring metadata.
pub(crate) struct ClassView {
    /// The revision-1 catalog entry (for `reserved`).
    pub(crate) entry: &'static CatalogEntry,
    /// The per-class authoring metadata (#440).
    pub(crate) metadata: &'static ClassMetadata,
}

/// Look up a class by its exact catalog path (an index built once per process).
pub(crate) fn lookup_class(class_path: &str) -> Option<ClassView> {
    static INDEX: OnceLock<BTreeMap<&'static str, (usize, usize)>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        let metadata: BTreeMap<&str, usize> = class_metadata()
            .iter()
            .enumerate()
            .map(|(i, m)| (m.class_path, i))
            .collect();
        catalog()
            .iter()
            .enumerate()
            .filter_map(|(i, e)| metadata.get(e.class_path).map(|&m| (e.class_path, (i, m))))
            .collect()
    });
    let &(entry, metadata) = index.get(class_path)?;
    Some(ClassView {
        entry: &catalog()[entry],
        metadata: &class_metadata()[metadata],
    })
}

/// Find the class parameter an assignment name addresses: a scalar by its name, an array element
/// by matching a template (`k_<i>`) and confirming the spelling with
/// [`oce_api::param_element_name`], so the element naming stays the facade's.
pub(crate) fn lookup_parameter<'a>(
    class: &'a ClassMetadata,
    name: &str,
) -> Option<&'a ClassParameter> {
    class.parameters.iter().find(|parameter| {
        if parameter.name == name {
            return !is_template(parameter.name);
        }
        match_template(parameter.name, name)
            .and_then(|indices| param_element_name(parameter.name, &indices))
            .is_some_and(|expanded| expanded == name)
    })
}

const PLACEHOLDERS: [&str; 3] = [
    PARAM_INDEX_PLACEHOLDER,
    PARAM_ROW_PLACEHOLDER,
    PARAM_COL_PLACEHOLDER,
];

pub(crate) fn is_template(name: &str) -> bool {
    PLACEHOLDERS.iter().any(|p| name.contains(p))
}

/// Read the 1-based indices `name` fills into `template`, or `None` when it does not fit. Each
/// placeholder takes a run of digits without a leading zero.
pub(crate) fn match_template(template: &str, name: &str) -> Option<Vec<usize>> {
    let mut indices = Vec::new();
    let mut rest_template = template;
    let mut rest_name = name;
    loop {
        let next = PLACEHOLDERS
            .iter()
            .filter_map(|p| rest_template.find(p).map(|at| (at, p.len())))
            .min();
        let Some((at, len)) = next else {
            return (rest_template == rest_name && !indices.is_empty()).then_some(indices);
        };
        rest_name = rest_name.strip_prefix(&rest_template[..at])?;
        let digits = rest_name.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || rest_name.starts_with('0') {
            return None;
        }
        indices.push(rest_name[..digits].parse().ok()?);
        rest_name = &rest_name[digits..];
        rest_template = &rest_template[at + len..];
    }
}

/// The enumeration type of an enum-valued parameter type: the facade's table row for the class
/// `ValueType::Enum` carries (issue #468), with its class path, last segment
/// ([`EnumerationClass::type_name`]) and members in one-based ordinal order. `None` for any other
/// value type or an id the engine does not know.
pub(crate) fn enum_type(value_type: ValueType) -> Option<&'static EnumerationClass> {
    let ValueType::Enum(class) = value_type else {
        return None;
    };
    enumeration_by_id(class).filter(|row| row.kind == EnumerationKind::Enumeration)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
