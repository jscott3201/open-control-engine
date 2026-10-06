//! Bounded interpreter for the keywords used by the catalog JSON schema fixture.
//! This checks the artifact; it is not a general JSON Schema implementation.

use oce_api::{ContractDomain, catalog, catalog_to_json, contract_descriptors};
use serde_json::{Value, json};

fn schema() -> Value {
    serde_json::from_str(
        contract_descriptors()
            .iter()
            .find(|descriptor| descriptor.domain == ContractDomain::Catalog)
            .unwrap()
            .schema,
    )
    .unwrap()
}

fn accepts(schema: &Value, value: &Value, root: &Value) -> bool {
    if let Some(reference) = schema.get("$ref") {
        return accepts(
            root.pointer(reference.as_str().unwrap().strip_prefix('#').unwrap())
                .unwrap(),
            value,
            root,
        );
    }
    if let Some(choices) = schema.get("oneOf") {
        return choices
            .as_array()
            .unwrap()
            .iter()
            .filter(|choice| accepts(choice, value, root))
            .count()
            == 1;
    }
    if let Some(expected) = schema.get("const") {
        return value == expected;
    }
    if let Some(choices) = schema.get("enum") {
        return choices.as_array().unwrap().contains(value);
    }
    if let Some(kind) = schema.get("type") {
        let matches = |kind: &str| match kind {
            "null" => value.is_null(),
            "string" => value.is_string(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            "boolean" => value.is_boolean(),
            "integer" => value.as_i64().is_some(),
            other => panic!("unhandled schema type {other}"),
        };
        let correct = kind.as_str().map_or_else(
            || {
                kind.as_array()
                    .unwrap()
                    .iter()
                    .any(|kind| matches(kind.as_str().unwrap()))
            },
            matches,
        );
        if !correct {
            return false;
        }
    }
    if let Some(pattern) = schema.get("pattern") {
        assert_eq!(pattern, "^[0-9a-f]{16}$");
        let text = value.as_str().unwrap();
        if text.len() != 16
            || !text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return false;
        }
    }
    if let Some(items) = schema.get("items")
        && !value
            .as_array()
            .unwrap()
            .iter()
            .all(|value| accepts(items, value, root))
    {
        return false;
    }
    if let Some(properties) = schema.get("properties") {
        let object = value.as_object().unwrap();
        if !schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .all(|key| object.contains_key(key.as_str().unwrap()))
        {
            return false;
        }
        if schema["additionalProperties"] == false
            && object.keys().any(|key| properties.get(key).is_none())
        {
            return false;
        }
        if !object
            .iter()
            .all(|(key, value)| accepts(&properties[key], value, root))
        {
            return false;
        }
    }
    if let Some(minimum) = schema.get("minimum")
        && value.as_i64().unwrap() < minimum.as_i64().unwrap()
    {
        return false;
    }
    if let Some(maximum) = schema.get("maximum")
        && value.as_i64().unwrap() > maximum.as_i64().unwrap()
    {
        return false;
    }
    true
}

#[test]
fn schema_accepts_every_catalog_entry_and_rejects_lost_or_unknown_fields() {
    let schema = schema();
    let value: Value = serde_json::from_str(&catalog_to_json(catalog())).unwrap();
    assert!(accepts(&schema, &value, &schema));
    for key in value["entries"][0].as_object().unwrap().keys() {
        let mut changed = value.clone();
        changed["entries"][0].as_object_mut().unwrap().remove(key);
        assert!(!accepts(&schema, &changed, &schema), "missing {key}");
    }
    let mut changed = value.clone();
    changed["entries"][0]["unknown"] = json!(false);
    assert!(!accepts(&schema, &changed, &schema));
    let mut changed = value.clone();
    changed["schema_revision"] = json!(2);
    assert!(!accepts(&schema, &changed, &schema));
    let index = value["entries"]
        .as_array()
        .unwrap()
        .iter()
        .position(|entry| !entry["param_rules"].as_array().unwrap().is_empty())
        .unwrap();
    for key in value["entries"][index]["param_rules"][0]
        .as_object()
        .unwrap()
        .keys()
    {
        let mut changed = value.clone();
        changed["entries"][index]["param_rules"][0]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            !accepts(&schema, &changed, &schema),
            "missing rule payload {key}"
        );
    }
}

#[test]
fn catalog_schema_covers_all_rule_variants_and_literal_kinds() {
    let schema = schema();
    assert_eq!(
        schema["properties"]["entries"]["items"]["properties"]["param_rules"]["items"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        27
    );
    let defaults = &schema["properties"]["entries"]["items"]["properties"]["param_defaults"]["items"]
        ["properties"]["default"];
    for value in [
        json!({"kind":"real","bits":"8000000000000000"}),
        json!({"kind":"integer","value":i64::MIN}),
        json!({"kind":"integer","value":i64::MAX}),
        json!({"kind":"boolean","value":true}),
        json!({"kind":"string","value":"quote\"\nλ"}),
        json!({"kind":"enum","value":"CDL.Types.SimpleController.PI"}),
        json!({"kind":"derived","formula":"2 * x"}),
        json!({"kind":"required"}),
    ] {
        assert!(accepts(defaults, &value, &schema));
    }
    for value in [
        json!({"kind":"real","bits":"NaN"}),
        json!({"kind":"real","bits":0.0}),
        json!({"kind":"real","bits":"7FF8000000000001"}),
        json!({"kind":"enum"}),
        json!({"kind":"required","value":null}),
        json!({"kind":"unknown"}),
    ] {
        assert!(!accepts(defaults, &value, &schema));
    }
}
