//! Shared JSON and fixture helpers for ASHRAE G36 catalog guard tests.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::dto::CxfDocument;
use crate::g36_catalog_guard_data::FixtureSource;

pub(crate) fn parameter_names(document: &CxfDocument) -> BTreeSet<String> {
    document
        .graph
        .iter()
        .flat_map(|node| node.has_parameter.iter())
        .map(|iri| local_name(&iri.id).to_owned())
        .collect()
}

pub(crate) fn enum_literals(catalog: &Value) -> BTreeMap<String, BTreeSet<String>> {
    array_field(&catalog["g36_types"], "enumerations")
        .iter()
        .map(|entry| {
            (
                str_field(entry, "class_path").to_owned(),
                string_set(array_field(entry, "literals")),
            )
        })
        .collect()
}

pub(crate) fn constant_packages(catalog: &Value) -> BTreeMap<String, BTreeMap<String, i64>> {
    array_field(&catalog["g36_types"], "integer_constant_packages")
        .iter()
        .map(|entry| {
            let constants = entry["constants"]
                .as_object()
                .unwrap_or_else(|| panic!("missing constants object in {entry:?}"))
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        value
                            .as_i64()
                            .unwrap_or_else(|| panic!("constant {name} is not i64")),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            (str_field(entry, "class_path").to_owned(), constants)
        })
        .collect()
}

pub(crate) fn parse_cxf(fixture: &FixtureSource, errors: &mut Vec<String>) -> CxfDocument {
    serde_json::from_str(fixture.text).unwrap_or_else(|err| {
        errors.push(format!("fixture-json-parse: {}:{err}", fixture.name));
        CxfDocument {
            context: crate::Context::Map(BTreeMap::new()),
            graph: Vec::new(),
            other: BTreeMap::new(),
        }
    })
}

pub(crate) fn assert_usize_field(
    actual: usize,
    entry: &Value,
    field: &str,
    fixture: &FixtureSource,
    errors: &mut Vec<String>,
) {
    if entry.get(field).and_then(Value::as_u64) != Some(actual as u64) {
        errors.push(format!("fixture-shape-drift: {}:{field}", fixture.name));
    }
}

pub(crate) fn package_order<'a>(catalog: &'a Value, package: &str) -> Option<&'a Value> {
    array_field(catalog, "package_orders")
        .iter()
        .find(|entry| str_field(entry, "package") == package)
}

pub(crate) fn local_name(id: &str) -> &str {
    id.rsplit(['#', '.']).next().unwrap_or(id)
}

pub(crate) fn jsonld_fragment(id: &str) -> &str {
    id.rsplit('#').next().unwrap_or(id)
}

pub(crate) fn parse(text: &str) -> Value {
    serde_json::from_str(text).expect("G36 catalog JSON parses")
}

pub(crate) fn array_field<'a>(value: &'a Value, name: &str) -> &'a Vec<Value> {
    value
        .get(name)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("missing array field `{name}`"))
}

pub(crate) fn str_field<'a>(value: &'a Value, name: &str) -> &'a str {
    value.get(name).and_then(Value::as_str).unwrap_or("")
}

pub(crate) fn bool_field(value: &Value, name: &str) -> bool {
    value.get(name).and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn string_vec(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .map(|item| item.as_str().unwrap_or("").to_owned())
        .collect()
}

pub(crate) fn string_set(values: &[Value]) -> BTreeSet<String> {
    string_vec(values).into_iter().collect()
}

pub(crate) fn remove_string(values: &mut Vec<Value>, target: &str) -> bool {
    let Some(pos) = values
        .iter()
        .position(|value| value.as_str() == Some(target))
    else {
        return false;
    };
    values.remove(pos);
    true
}

pub(crate) fn remove_path_entry(values: &mut Vec<Value>, field: &str, target: &str) -> bool {
    let Some(pos) = values
        .iter()
        .position(|value| str_field(value, field) == target)
    else {
        return false;
    };
    values.remove(pos);
    true
}

pub(crate) fn catalog_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
