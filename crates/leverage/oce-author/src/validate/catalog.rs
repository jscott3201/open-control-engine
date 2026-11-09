//! The catalog view stage 1 judges against: class lookup, parameter lookup (array templates
//! included) and enumeration members, all read through `oce-api`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use oce_api::{
    CatalogEntry, ClassMetadata, ClassParameter, PARAM_COL_PLACEHOLDER, PARAM_INDEX_PLACEHOLDER,
    PARAM_ROW_PLACEHOLDER, ValueType, catalog, class_metadata, param_element_name,
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

fn is_template(name: &str) -> bool {
    PLACEHOLDERS.iter().any(|p| name.contains(p))
}

/// Read the 1-based indices `name` fills into `template`, or `None` when it does not fit. Each
/// placeholder takes a run of digits without a leading zero.
fn match_template(template: &str, name: &str) -> Option<Vec<usize>> {
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

/// One enumeration type an enum parameter may take.
pub(crate) struct EnumType {
    /// The type's last segment, as authored (`SimpleController`).
    pub(crate) type_name: &'static str,
    /// The enumeration class path.
    pub(crate) class_path: &'static str,
    /// The members in one-based ordinal order.
    pub(crate) members: &'static [&'static str],
}

/// The enumeration types of the registered enum parameters, keyed by the stable class id
/// `oce_api::ValueType::Enum` carries.
///
/// The facade publishes members for `ZeroTime` (a `Required` rule's kind) and for the
/// `TimeTable` parameters (`EnumMembers` rules), but not for `SimpleController`, and names no
/// enumeration class path for the `TimeTable` ones (facade accessor requested in GitHub #468); so
/// the table is pinned here and the
/// `enum_table_agrees_with_the_catalog` test checks every row the catalog can see — members,
/// class path, defaults and the ordinal count the load gate accepts — against `oce-api`.
pub(crate) const ENUM_TYPES: &[(u32, EnumType)] = &[
    (
        1,
        EnumType {
            type_name: "SimpleController",
            class_path: "CDL.Types.SimpleController",
            members: &["P", "PI", "PD", "PID"],
        },
    ),
    (
        2,
        EnumType {
            type_name: "Smoothness",
            class_path: "CDL.Types.Smoothness",
            members: &["LinearSegments", "ConstantSegments"],
        },
    ),
    (
        3,
        EnumType {
            type_name: "Extrapolation",
            class_path: "CDL.Types.Extrapolation",
            members: &["HoldLastPoint", "LastTwoPoints", "Periodic"],
        },
    ),
    (
        4,
        EnumType {
            type_name: "ZeroTime",
            class_path: "CDL.Types.ZeroTime",
            members: &[
                "UnixTimeStamp",
                "UnixTimeStampGMT",
                "Custom",
                "NY2010",
                "NY2011",
                "NY2012",
                "NY2013",
                "NY2014",
                "NY2015",
                "NY2016",
                "NY2017",
                "NY2018",
                "NY2019",
                "NY2020",
                "NY2021",
                "NY2022",
                "NY2023",
                "NY2024",
                "NY2025",
                "NY2026",
                "NY2027",
                "NY2028",
                "NY2029",
                "NY2030",
                "NY2031",
                "NY2032",
                "NY2033",
                "NY2034",
                "NY2035",
                "NY2036",
                "NY2037",
                "NY2038",
                "NY2039",
                "NY2040",
                "NY2041",
                "NY2042",
                "NY2043",
                "NY2044",
                "NY2045",
                "NY2046",
                "NY2047",
                "NY2048",
                "NY2049",
                "NY2050",
            ],
        },
    ),
];

/// The enumeration type of an enum-valued parameter type, if this crate knows it.
pub(crate) fn enum_type(value_type: ValueType) -> Option<&'static EnumType> {
    let ValueType::Enum(class) = value_type else {
        return None;
    };
    ENUM_TYPES
        .iter()
        .find(|(id, _)| *id == class.0)
        .map(|(_, t)| t)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
