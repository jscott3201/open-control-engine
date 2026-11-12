//! The published wire-form schema: regenerated and compared byte for byte, walked for keywords
//! outside the strict profile, and checked for closed objects and the catalog class enum
//! (decision 8).

use crate::support;

use oce_author::{SCHEMA_PROFILE_KEYWORDS, WIRE_SCHEMA, authorable_class_paths, wire_schema};
use serde_json::Value;

use support::{compare_or_bless, crate_dir};

#[test]
fn published_schema_matches_the_regenerated_bytes() {
    let generated = wire_schema();
    compare_or_bless(&crate_dir().join("schema/sequence.schema.json"), &generated);
    if !oce_bless::enabled(support::BLESS_SWITCH) {
        assert_eq!(WIRE_SCHEMA, generated, "the compiled-in copy is the file");
    }
}

#[test]
fn schema_generation_is_deterministic() {
    assert_eq!(wire_schema(), wire_schema());
}

/// Walk every schema object (not the contents of `properties` maps, `enum` lists or
/// `required` lists, whose keys and items are data) and collect what breaks the profile.
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
                    for (i, branch) in branches.iter().enumerate() {
                        profile_violations(branch, &format!("{here}/{i}"), out);
                    }
                }
                _ => out.push(format!("{here}: not an array")),
            },
            "additionalProperties" if value != &Value::Bool(false) => {
                out.push(format!("{here}: objects are closed (false)"));
            }
            _ => {}
        }
    }
    if map.get("type") == Some(&Value::String("object".to_owned())) {
        if map.get("additionalProperties") != Some(&Value::Bool(false)) {
            out.push(format!(
                "{path}: object without additionalProperties: false"
            ));
        }
        let mut properties: Vec<&str> = map
            .get("properties")
            .and_then(Value::as_object)
            .map(|p| p.keys().map(String::as_str).collect())
            .unwrap_or_default();
        let mut required: Vec<&str> = map
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        properties.sort_unstable();
        required.sort_unstable();
        if properties != required {
            out.push(format!("{path}: every property must be required"));
        }
    }
}

fn published() -> Value {
    serde_json::from_str(WIRE_SCHEMA).expect("the published schema is JSON")
}

#[test]
fn published_schema_keeps_to_the_strict_profile() {
    let mut violations = Vec::new();
    profile_violations(&published(), "#", &mut violations);
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn the_profile_walk_refuses_each_banned_keyword() {
    for banned in [
        "oneOf", "allOf", "pattern", "minItems", "format", "const", "default",
    ] {
        let mut schema = published();
        schema["$defs"]["Assignment"][banned] = Value::Bool(true);
        let mut violations = Vec::new();
        profile_violations(&schema, "#", &mut violations);
        assert_eq!(
            violations,
            vec![format!(
                "#/$defs/Assignment/{banned}: keyword outside the strict profile"
            )],
            "{banned}"
        );
    }
    let mut open = published();
    open["$defs"]["Block"]["additionalProperties"] = Value::Bool(true);
    let mut violations = Vec::new();
    profile_violations(&open, "#", &mut violations);
    assert_eq!(violations.len(), 2, "{violations:#?}");
    let mut optional = published();
    optional["$defs"]["Block"]["required"] = serde_json::json!(["name", "class"]);
    let mut violations = Vec::new();
    profile_violations(&optional, "#", &mut violations);
    assert_eq!(
        violations,
        vec!["#/$defs/Block: every property must be required".to_owned()]
    );
}

#[test]
fn any_of_appears_only_over_the_value_shapes() {
    let schema = published();
    let defs = schema["$defs"].as_object().expect("$defs");
    let with_any_of: Vec<&str> = defs
        .iter()
        .filter(|(_, def)| def.get("anyOf").is_some())
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(with_any_of, vec!["LiteralValue", "ParameterValue"]);
    let shapes =
        |name: &str| -> Vec<Value> { defs[name]["anyOf"].as_array().expect("anyOf").clone() };
    let scalar = |t: &str| serde_json::json!({ "type": t });
    let reference = |d: &str| serde_json::json!({ "$ref": format!("#/$defs/{d}") });
    assert_eq!(
        shapes("ParameterValue"),
        vec![
            scalar("number"),
            scalar("boolean"),
            scalar("string"),
            reference("Reference"),
            reference("Member")
        ]
    );
    assert_eq!(
        shapes("LiteralValue"),
        vec![
            scalar("number"),
            scalar("boolean"),
            scalar("string"),
            reference("Member")
        ]
    );
    assert!(!WIRE_SCHEMA.contains("\"oneOf\""));
}

#[test]
fn class_enum_is_the_catalogs_authorable_class_paths() {
    let schema = published();
    let listed: Vec<&str> = schema["$defs"]["Block"]["properties"]["class"]["enum"]
        .as_array()
        .expect("class enum")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let expected: Vec<&str> = oce_api::catalog()
        .iter()
        .filter(|e| !e.reserved)
        .map(|e| e.class_path)
        .collect();
    assert_eq!(listed, expected);
    assert_eq!(listed, authorable_class_paths());
    assert_eq!(listed.len(), 133, "the note's count of authorable classes");
    assert!(
        listed.iter().all(|c| !c.starts_with("urn:")),
        "no reserved class"
    );
}

#[test]
fn root_requires_the_seven_sequence_keys_in_wire_order() {
    let schema = published();
    assert_eq!(
        schema["required"],
        serde_json::json!([
            "sequence",
            "derived_from",
            "params",
            "inputs",
            "outputs",
            "blocks",
            "connections"
        ])
    );
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["title"], "Sequence");
}

#[test]
fn the_parameter_type_enum_lists_the_scalars_and_every_enumeration_but_no_package() {
    let schema = published();
    let listed: Vec<&str> = schema["$defs"]["SequenceParameter"]["properties"]["type"]["enum"]
        .as_array()
        .expect("type enum")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    assert_eq!(listed, oce_author::parameter_type_names());
    let enumerations: Vec<&str> = oce_api::enumeration_types()
        .map(oce_api::EnumerationClass::type_name)
        .collect();
    assert_eq!(&listed[3..], enumerations.as_slice());
    for row in oce_api::enumerations() {
        let is_type = row.kind == oce_api::EnumerationKind::Enumeration;
        assert_eq!(
            listed.contains(&row.type_name()),
            is_type,
            "{}",
            row.class_path
        );
    }
    // Far under the 1,000-value cap of common structured-output decoders.
    assert!(listed.len() < 100, "{}", listed.len());
    // A boundary keeps the three scalar types.
    assert_eq!(
        schema["$defs"]["Boundary"]["properties"]["type"]["$ref"],
        "#/$defs/SignalType"
    );
    assert_eq!(
        schema["$defs"]["SignalType"]["enum"],
        serde_json::json!(["Real", "Integer", "Boolean"])
    );
}

#[test]
fn boundary_bounds_are_number_lists_whose_description_says_at_most_one() {
    let schema = published();
    let boundary = &schema["$defs"]["Boundary"];
    let required: Vec<&str> = boundary["required"]
        .as_array()
        .expect("required")
        .iter()
        .map(|v| v.as_str().expect("string"))
        .collect();
    assert_eq!(
        required,
        [
            "name",
            "type",
            "quantity",
            "display_unit",
            "min",
            "max",
            "semantic"
        ]
    );
    for side in ["min", "max"] {
        let property = &boundary["properties"][side];
        assert_eq!(property["type"], "array", "{side}");
        assert_eq!(
            property["items"],
            serde_json::json!({"type": "number"}),
            "{side}"
        );
        let description = property["description"].as_str().expect("description");
        assert!(
            description.contains("at most one item"),
            "{side}: {description}"
        );
    }
}
