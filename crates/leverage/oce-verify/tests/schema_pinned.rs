//! The published schema of the vector format: regenerated and compared byte for byte, walked for
//! keywords outside the strict profile, and checked for its closed objects, its two map shapes
//! and its three `anyOf` sites (vectors note decision 17).

mod support;

use oce_verify::{
    SCHEMA_PROFILE_KEYWORDS, VECTORS_JSON_SCHEMA, VECTORS_SCHEMA, vectors_json_schema,
};
use serde_json::{Value, json};

use support::{compare_or_bless, crate_dir};

#[test]
fn published_schema_matches_the_regenerated_bytes() {
    let generated = vectors_json_schema();
    compare_or_bless(&crate_dir().join("schema/vectors.schema.json"), &generated);
    if !oce_bless::enabled(support::BLESS_SWITCH) {
        assert_eq!(
            VECTORS_JSON_SCHEMA, generated,
            "the compiled-in copy is the file"
        );
    }
}

#[test]
fn schema_generation_is_deterministic() {
    assert_eq!(vectors_json_schema(), vectors_json_schema());
}

#[test]
fn the_comparison_fails_on_one_changed_byte() {
    // The control for the byte comparison above: an edited description is a different document.
    let generated = vectors_json_schema();
    let edited = generated.replacen("Seconds between ticks", "Seconds between two ticks", 1);
    assert_ne!(
        edited, generated,
        "the schema carries the reader's documentation"
    );
    assert_ne!(edited, VECTORS_JSON_SCHEMA);
}

fn published() -> Value {
    serde_json::from_str(VECTORS_JSON_SCHEMA).expect("the published schema is JSON")
}

/// Walk every schema object (not the contents of `properties` maps, `enum` lists or `required`
/// lists, whose keys and items are data) and collect what breaks the profile.
fn profile_violations(schema: &Value, path: &str, out: &mut Vec<String>) {
    let Value::Object(map) = schema else {
        out.push(format!("{path}: a schema must be an object"));
        return;
    };
    for (key, value) in map {
        let here = format!("{path}/{key}");
        if !SCHEMA_PROFILE_KEYWORDS.contains(&key.as_str()) {
            out.push(format!("{here}: keyword outside the strict profile"));
            continue;
        }
        match key.as_str() {
            "$defs" | "properties" => match value {
                Value::Object(children) => {
                    for (name, child) in children {
                        profile_violations(child, &format!("{here}/{name}"), out);
                    }
                }
                _ => out.push(format!("{here}: not an object")),
            },
            "items" => profile_violations(value, &here, out),
            "anyOf" => match value {
                Value::Array(branches) => {
                    for (index, branch) in branches.iter().enumerate() {
                        profile_violations(branch, &format!("{here}/{index}"), out);
                    }
                }
                _ => out.push(format!("{here}: not an array")),
            },
            // `false` closes an object; a schema makes it a map of that shape.
            "additionalProperties" if value != &Value::Bool(false) => {
                if value.is_object() {
                    profile_violations(value, &here, out);
                } else {
                    out.push(format!("{here}: neither false nor a schema"));
                }
            }
            "type" if !value.is_string() => {
                out.push(format!("{here}: one type per schema, never a list or null"));
            }
            _ => {}
        }
    }
    if map.get("type") == Some(&json!("null")) {
        out.push(format!("{path}: null is never a value"));
    }
    if map.get("type") == Some(&json!("object")) {
        let closed = map.get("additionalProperties") == Some(&Value::Bool(false));
        let is_map = map
            .get("additionalProperties")
            .is_some_and(Value::is_object);
        match (map.contains_key("properties"), closed, is_map) {
            (true, true, _) | (false, _, true) => {}
            (true, false, _) => out.push(format!("{path}: an object with properties is closed")),
            (false, _, false) => out.push(format!("{path}: an object is closed or a map")),
        }
        let properties: Vec<&str> = map
            .get("properties")
            .and_then(Value::as_object)
            .map(|p| p.keys().map(String::as_str).collect())
            .unwrap_or_default();
        for key in map
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !key.as_str().is_some_and(|k| properties.contains(&k)) {
                out.push(format!(
                    "{path}: `required` names {key}, which is no property"
                ));
            }
        }
    }
}

#[test]
fn published_schema_keeps_to_the_strict_profile() {
    let mut violations = Vec::new();
    profile_violations(&published(), "#", &mut violations);
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn the_profile_walk_refuses_each_banned_keyword_and_each_open_object() {
    for banned in [
        "oneOf", "allOf", "pattern", "minItems", "format", "const", "default", "not",
    ] {
        let mut schema = published();
        schema["$defs"]["Step"][banned] = Value::Bool(true);
        let mut violations = Vec::new();
        profile_violations(&schema, "#", &mut violations);
        assert_eq!(
            violations,
            vec![format!(
                "#/$defs/Step/{banned}: keyword outside the strict profile"
            )],
            "{banned}"
        );
    }
    let violations_of = |edit: &dyn Fn(&mut Value)| {
        let mut schema = published();
        edit(&mut schema);
        let mut violations = Vec::new();
        profile_violations(&schema, "#", &mut violations);
        violations
    };
    assert_eq!(
        violations_of(&|s| s["$defs"]["Clock"]["additionalProperties"] = json!(true)),
        [
            "#/$defs/Clock/additionalProperties: neither false nor a schema",
            "#/$defs/Clock: an object with properties is closed"
        ]
    );
    assert_eq!(
        violations_of(&|s| {
            s["$defs"]["Clock"]
                .as_object_mut()
                .expect("object")
                .remove("additionalProperties");
        }),
        ["#/$defs/Clock: an object with properties is closed"]
    );
    assert_eq!(
        violations_of(&|s| s["$defs"]["Clock"]["required"] = json!(["step_s", "stride_s"])),
        ["#/$defs/Clock: `required` names \"stride_s\", which is no property"]
    );
    assert_eq!(
        violations_of(
            &|s| s["$defs"]["Clock"]["properties"]["step_s"]["type"] = json!(["number", "null"])
        ),
        ["#/$defs/Clock/properties/step_s/type: one type per schema, never a list or null"]
    );
    assert_eq!(
        violations_of(&|s| s["$defs"]["Clock"]["properties"]["step_s"]["type"] = json!("null")),
        ["#/$defs/Clock/properties/step_s: null is never a value"]
    );
    assert_eq!(
        violations_of(&|s| s["properties"]["params"] = json!({"type": "object"})),
        ["#/properties/params: an object is closed or a map"]
    );
}

fn defs() -> serde_json::Map<String, Value> {
    published()["$defs"].as_object().expect("$defs").clone()
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("a list")
        .iter()
        .map(|item| item.as_str().expect("a string"))
        .collect()
}

#[test]
fn the_root_is_the_vector_set_with_its_one_schema_tag() {
    let schema = published();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["title"], "VectorSet");
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        strings(&schema["required"]),
        ["schema", "clock", "scenarios"]
    );
    let properties: Vec<&str> = schema["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(properties, ["clock", "params", "scenarios", "schema"]);
    assert_eq!(schema["properties"]["schema"]["type"], "string");
    assert_eq!(
        schema["properties"]["schema"]["enum"],
        json!([VECTORS_SCHEMA])
    );
    assert_eq!(VECTORS_SCHEMA, "oce/vectors/v1");
    // No key the format does not have yet.
    for absent in ["mutants", "within", "of", "when", "tolerance"] {
        assert!(
            !VECTORS_JSON_SCHEMA.contains(&format!("\"{absent}\"")),
            "{absent}"
        );
    }
}

#[test]
fn every_definition_has_the_keys_the_format_gives_it() {
    let defs = defs();
    let names: Vec<&str> = defs.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "Clock",
            "Diagnostic",
            "Expectation",
            "Input",
            "Instant",
            "Scenario",
            "Step",
            "Value",
            "Window"
        ]
    );
    // (definition, every property, the required ones in declaration order)
    let objects: [(&str, &[&str], &[&str]); 6] = [
        ("Clock", &["horizon_s", "step_s"], &["step_s", "horizon_s"]),
        ("Diagnostic", &["block", "message"], &["block"]),
        ("Instant", &["at_s", "diagnostic"], &["diagnostic", "at_s"]),
        (
            "Scenario",
            &["description", "expect", "inputs", "name", "params"],
            &["name", "inputs", "expect"],
        ),
        ("Step", &["t", "value"], &["t", "value"]),
        (
            "Window",
            &["equals", "from_s", "output", "to_s"],
            &["output", "from_s", "to_s", "equals"],
        ),
    ];
    for (name, properties, required) in objects {
        let def = &defs[name];
        assert_eq!(def["type"], "object", "{name}");
        assert_eq!(def["additionalProperties"], false, "{name}");
        let listed: Vec<&str> = def["properties"]
            .as_object()
            .expect("properties")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(listed, properties, "{name}");
        assert_eq!(strings(&def["required"]), required, "{name}");
        assert!(
            def["description"].as_str().is_some_and(|d| !d.is_empty()),
            "{name} is described"
        );
    }
}

#[test]
fn any_of_appears_only_over_values_inputs_and_expectations() {
    let defs = defs();
    let with_any_of: Vec<&str> = defs
        .iter()
        .filter(|(_, def)| def.get("anyOf").is_some())
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(with_any_of, ["Expectation", "Input", "Value"]);
    assert_eq!(VECTORS_JSON_SCHEMA.matches("\"anyOf\"").count(), 3);
    let number = json!({"type": "number"});
    let boolean = json!({"type": "boolean"});
    assert_eq!(defs["Value"]["anyOf"], json!([number, boolean]));
    assert_eq!(
        defs["Input"]["anyOf"],
        json!([number, boolean, {"type": "array", "items": {"$ref": "#/$defs/Step"}}])
    );
    assert_eq!(
        defs["Expectation"]["anyOf"],
        json!([{"$ref": "#/$defs/Window"}, {"$ref": "#/$defs/Instant"}])
    );
    // Six strings (the tag, a scenario's name and description, an output, a diagnostic's block
    // and message), none of them a value; and no null or integer type anywhere.
    for (name, count) in [("string", 6), ("null", 0), ("integer", 0)] {
        let in_types = VECTORS_JSON_SCHEMA
            .matches(&format!("\"type\": \"{name}\""))
            .count();
        assert_eq!(in_types, count, "{name}");
    }
}

#[test]
fn the_two_map_shapes_are_inputs_and_params() {
    let schema = published();
    let defs = defs();
    let value_map = json!({"$ref": "#/$defs/Value"});
    for params in [
        &schema["properties"]["params"],
        &defs["Scenario"]["properties"]["params"],
    ] {
        assert_eq!(params["type"], "object");
        assert_eq!(params["additionalProperties"], value_map);
        assert!(params.get("properties").is_none());
    }
    let inputs = &defs["Scenario"]["properties"]["inputs"];
    assert_eq!(inputs["type"], "object");
    assert_eq!(
        inputs["additionalProperties"],
        json!({"$ref": "#/$defs/Input"})
    );
    assert_eq!(
        VECTORS_JSON_SCHEMA
            .matches("\"additionalProperties\": {")
            .count(),
        3
    );
    // Times are plain numbers.
    for (def, key) in [
        ("Clock", "step_s"),
        ("Clock", "horizon_s"),
        ("Step", "t"),
        ("Window", "from_s"),
        ("Window", "to_s"),
        ("Instant", "at_s"),
    ] {
        assert_eq!(
            defs[def]["properties"][key]["type"], "number",
            "{def}.{key}"
        );
    }
    assert_eq!(
        defs["Scenario"]["properties"]["expect"]["items"],
        json!({"$ref": "#/$defs/Expectation"})
    );
    assert_eq!(
        schema["properties"]["scenarios"]["items"],
        json!({"$ref": "#/$defs/Scenario"})
    );
}
