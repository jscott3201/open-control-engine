//! The published schema agrees with the parser: a small structural JSON Schema checker (the
//! keywords of the strict profile: `type`, `properties`, `required`, `additionalProperties`,
//! `items`, `enum`, `anyOf`, `$ref`) accepts every bundle fixture, and for each value shape the
//! schema and `from_wire_str` accept and refuse the same values.

use crate::support;

use oce_author::{WIRE_SCHEMA, from_wire_str};
use serde_json::{Value, json};

use support::fixture;

/// Whether `instance` satisfies `schema` (resolving `$ref` against `root`).
fn conforms(instance: &Value, schema: &Value, root: &Value) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.strip_prefix("#/$defs/").expect("local $ref");
        return conforms(instance, &root["$defs"][name], root);
    }
    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
        return branches.iter().any(|b| conforms(instance, b, root));
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(instance)
    {
        return false;
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("string") => instance.is_string(),
        Some("number") => instance.is_number(),
        Some("boolean") => instance.is_boolean(),
        Some("array") => instance
            .as_array()
            .is_some_and(|items| items.iter().all(|i| conforms(i, &schema["items"], root))),
        Some("object") => {
            let Some(object) = instance.as_object() else {
                return false;
            };
            let properties = schema["properties"].as_object().expect("properties");
            let required = schema["required"].as_array().expect("required");
            required
                .iter()
                .all(|k| object.contains_key(k.as_str().expect("key")))
                && object
                    .iter()
                    .all(|(k, v)| properties.get(k).is_some_and(|p| conforms(v, p, root)))
        }
        Some(other) => panic!("type `{other}` is outside the profile"),
        None => true,
    }
}

fn schema() -> Value {
    serde_json::from_str(WIRE_SCHEMA).expect("schema")
}

#[test]
fn every_bundle_fixture_conforms_to_the_published_schema() {
    let root = schema();
    let mut count = 0;
    for entry in std::fs::read_dir(fixture("wire")).expect("fixtures") {
        let path = entry.expect("entry").path();
        let text = std::fs::read_to_string(&path).expect("read");
        let instance: Value = serde_json::from_str(&text).expect("json");
        // The defects bundle names a class outside the catalog on purpose: the schema refuses
        // it, the parser reads it and the validator reports `class-not-found`.
        let expected = !path.ends_with("defects.json");
        assert_eq!(
            conforms(&instance, &root, &root),
            expected,
            "{}",
            path.display()
        );
        assert!(from_wire_str(&text).is_ok());
        count += 1;
    }
    assert!(count >= 4);
}

/// A minimal document whose one block assignment's value is `value`, and one whose sequence
/// parameter's value is `value`.
fn documents(value: &Value) -> [Value; 2] {
    [
        json!({"sequence": "s", "extends": "", "params": [], "inputs": [], "outputs": [],
               "blocks": [{"name": "b", "class": "CDL.Reals.Sources.Constant",
                           "params": [{"name": "k", "value": value}]}],
               "connections": []}),
        json!({"sequence": "s", "extends": "", "params": [{"name": "p", "type": "Real", "value": value}],
               "inputs": [], "outputs": [], "blocks": [], "connections": []}),
    ]
}

#[test]
fn schema_and_parser_agree_on_every_value_shape() {
    let root = schema();
    // (value, accepted as a block value, accepted as a sequence-parameter value)
    let cases = [
        (json!(1), true, true),
        (json!(-0.5), true, true),
        (json!(true), true, true),
        (json!("text"), true, true),
        (json!({"ref": "p"}), true, false),
        (json!({"member": "SimpleController.PI"}), true, true),
        // One negative control per shape.
        (json!(null), false, false),
        (json!([1]), false, false),
        (json!({}), false, false),
        (json!({"ref": 1}), false, false),
        (json!({"member": false}), false, false),
        (json!({"ref": "p", "member": "A.B"}), false, false),
        (json!({"ref": "p", "extra": 1}), false, false),
        (json!({"value": 1}), false, false),
    ];
    for (value, block_ok, param_ok) in cases {
        for (document, expected) in documents(&value).iter().zip([block_ok, param_ok]) {
            assert_eq!(
                conforms(document, &root, &root),
                expected,
                "schema on {value}"
            );
            assert_eq!(
                from_wire_str(&document.to_string()).is_ok(),
                expected,
                "parser on {value}"
            );
        }
    }
}

#[test]
fn the_class_enum_refuses_what_the_validator_calls_class_not_found() {
    let root = schema();
    let mut document = documents(&json!(1))[0].clone();
    assert!(conforms(&document, &root, &root));
    document["blocks"][0]["class"] = json!("CDL.Reals.Nope");
    assert!(!conforms(&document, &root, &root));
    let model = from_wire_str(&document.to_string()).expect("the parser reads any class string");
    assert!(
        oce_author::validate(&model)
            .iter()
            .any(|f| f.code == "class-not-found")
    );
}

#[test]
fn the_checker_refuses_closed_object_violations() {
    let root = schema();
    let good = documents(&json!(1))[0].clone();
    let mut extra = good.clone();
    extra["note"] = json!("x");
    let mut missing = good.clone();
    missing.as_object_mut().expect("object").remove("extends");
    let mut wrong = good;
    wrong["connections"] = json!("a -> b");
    for (name, document) in [("extra", extra), ("missing", missing), ("wrong", wrong)] {
        assert!(!conforms(&document, &root, &root), "{name}");
        assert!(from_wire_str(&document.to_string()).is_err(), "{name}");
    }
}

/// A minimal document with one Real input `u` whose `min` is `min`, and one with a sequence
/// parameter of type `value_type` whose value is `value`.
fn bounded(min: &Value) -> Value {
    json!({"sequence": "s", "extends": "", "params": [],
           "inputs": [{"name": "u", "type": "Real", "quantity": "", "display_unit": "",
                       "min": min, "max": [],
                       "semantic": {"name": "", "label": "", "unit": "", "brick": "", "s223": "",
                                    "qudt": "", "haystack": [], "provisional": false}}],
           "outputs": [], "blocks": [], "connections": []})
}

fn typed(value_type: &Value, value: &Value) -> Value {
    json!({"sequence": "s", "extends": "", "params": [{"name": "p", "type": value_type, "value": value}],
           "inputs": [], "outputs": [], "blocks": [], "connections": []})
}

#[test]
fn schema_and_parser_agree_on_bounds_and_parameter_types() {
    let root = schema();
    // (min, schema accepts, parser accepts): the one divergence is a second item, which the
    // strict profile cannot refuse (no `maxItems`) and the reader does (the description says so).
    for (min, schema_ok, parser_ok) in [
        (json!([]), true, true),
        (json!([0]), true, true),
        (json!([-1.5]), true, true),
        (json!([0, 1]), true, false),
        (json!(0), false, false),
        (json!(["0"]), false, false),
        (json!([[0]]), false, false),
        (json!([null]), false, false),
        (json!(null), false, false),
    ] {
        let document = bounded(&min);
        assert_eq!(
            conforms(&document, &root, &root),
            schema_ok,
            "schema on {min}"
        );
        assert_eq!(
            from_wire_str(&document.to_string()).is_ok(),
            parser_ok,
            "parser on {min}"
        );
    }
    // (type, value, schema accepts, parser accepts): the parser reads any type string, as it reads
    // any class string, and the validator names what the schema's enum leaves out.
    let member = json!({"member": "SimpleController.PI"});
    for (value_type, value, schema_ok, parser_ok) in [
        (json!("SimpleController"), member.clone(), true, true),
        (
            json!("EnergyStandard"),
            json!({"member": "EnergyStandard.ASHRAE90_1"}),
            true,
            true,
        ),
        (
            json!("Integer"),
            json!({"member": "OperationModes.occupied"}),
            true,
            true,
        ),
        (
            json!("OperationModes"),
            json!({"member": "OperationModes.occupied"}),
            false,
            true,
        ),
        (json!("Float"), json!(1), false, true),
        (json!(7), json!(1), false, false),
        (json!(null), member, false, false),
    ] {
        let document = typed(&value_type, &value);
        assert_eq!(
            conforms(&document, &root, &root),
            schema_ok,
            "schema on {value_type}"
        );
        let read = from_wire_str(&document.to_string());
        assert_eq!(read.is_ok(), parser_ok, "parser on {value_type}");
        if let Ok(model) = read
            && !schema_ok
        {
            assert!(
                oce_author::validate(&model)
                    .iter()
                    .any(|f| f.code == "unknown-enum-type"),
                "{value_type}"
            );
        }
    }
}
