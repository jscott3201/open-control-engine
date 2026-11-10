//! The published schema agrees with the reader. A small structural JSON Schema checker (the
//! keywords of the strict profile) accepts every document the reader loads, and for every closed
//! object the schema's properties and required keys are exactly the keys the reader knows and
//! requires. The schema is derived from the reader's shapes but the reader matches keys by hand,
//! so this is what keeps a key's spelling from drifting between the two.

mod support;

use oce_verify::VECTORS_JSON_SCHEMA;
use serde_json::{Map, Value, json};

use support::{LIBRARY_RULES, code_of, fixture_bytes, library_vectors, load};

/// Whether `instance` satisfies `schema` (resolving `$ref` against `root`).
fn conforms(instance: &Value, schema: &Value, root: &Value) -> bool {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let name = reference.strip_prefix("#/$defs/").expect("local $ref");
        return conforms(instance, &root["$defs"][name], root);
    }
    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
        return branches
            .iter()
            .any(|branch| conforms(instance, branch, root));
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
        Some("array") => instance.as_array().is_some_and(|items| {
            items
                .iter()
                .all(|item| conforms(item, &schema["items"], root))
        }),
        Some("object") => {
            let Some(object) = instance.as_object() else {
                return false;
            };
            let empty = Map::new();
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .unwrap_or(&empty);
            let required = schema.get("required").and_then(Value::as_array);
            let additional = &schema["additionalProperties"];
            required
                .into_iter()
                .flatten()
                .all(|key| object.contains_key(key.as_str().expect("key")))
                && object.iter().all(|(key, value)| match properties.get(key) {
                    Some(property) => conforms(value, property, root),
                    None => additional.is_object() && conforms(value, additional, root),
                })
        }
        Some(other) => panic!("type `{other}` is outside the profile"),
        None => true,
    }
}

fn schema() -> Value {
    serde_json::from_str(VECTORS_JSON_SCHEMA).expect("schema")
}

fn value(text: &str) -> Value {
    serde_json::from_str(text).expect("JSON")
}

/// A document with every optional key present, one object of every closed shape.
const FULL: &str = r#"{
  "schema": "oce/vectors/v1",
  "clock": { "step_s": 60, "horizon_s": 1800 },
  "params": { "a.k": 1 },
  "scenarios": [
    {
      "name": "s",
      "description": "d",
      "params": { "b.k": 2.5 },
      "inputs": { "u": [ { "t": 0, "value": 1 } ], "v": true },
      "expect": [
        { "output": "y", "from_s": 0, "to_s": 1800, "equals": false },
        { "diagnostic": { "block": "CDL.Utilities.Assert", "message": "m" }, "at_s": 60 }
      ]
    }
  ]
}"#;

#[test]
fn every_document_the_reader_loads_conforms_to_the_schema() {
    let root = schema();
    let mut documents: Vec<String> = LIBRARY_RULES
        .iter()
        .map(|rule| String::from_utf8(library_vectors(rule)).expect("UTF-8"))
        .collect();
    for fixture in [
        "sets/every_field.json",
        "canonical/every_field.json",
        "canonical/AHU-0016.json",
        "canonical/AHU-0004.json",
    ] {
        documents.push(String::from_utf8(fixture_bytes(fixture)).expect("UTF-8"));
    }
    documents.push(FULL.to_owned());
    documents.push(support::set_of(""));
    for document in &documents {
        assert_eq!(code_of(document), "loaded");
        assert!(conforms(&value(document), &root, &root), "{document:.120}");
        // And what the reader writes conforms as well.
        let written = load(document).expect("loads").to_canonical_json();
        assert!(conforms(&value(&written), &root, &root), "{written:.120}");
    }
}

/// The closed objects of the schema: the definition name (`#` for the root) and the JSON pointer
/// of one such object in [`FULL`].
const OBJECTS: &[(&str, &str)] = &[
    ("#", ""),
    ("Clock", "/clock"),
    ("Scenario", "/scenarios/0"),
    ("Step", "/scenarios/0/inputs/u/0"),
    ("Window", "/scenarios/0/expect/0"),
    ("Instant", "/scenarios/0/expect/1"),
    ("Diagnostic", "/scenarios/0/expect/1/diagnostic"),
];

fn definition<'a>(root: &'a Value, name: &str) -> &'a Value {
    if name == "#" {
        root
    } else {
        &root["$defs"][name]
    }
}

fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect()
}

fn document(edited: &Value) -> String {
    serde_json::to_string(edited).expect("JSON")
}

#[test]
fn the_table_names_every_closed_object_of_the_schema() {
    let root = schema();
    let mut closed: Vec<&str> = root["$defs"]
        .as_object()
        .expect("$defs")
        .iter()
        .filter(|(_, def)| def.get("properties").is_some())
        .map(|(name, _)| name.as_str())
        .collect();
    closed.push("#");
    closed.sort_unstable();
    let mut listed: Vec<&str> = OBJECTS.iter().map(|(name, _)| *name).collect();
    listed.sort_unstable();
    assert_eq!(listed, closed, "a new closed object needs a row in OBJECTS");
}

#[test]
fn the_full_document_holds_every_property_of_every_closed_object() {
    let root = schema();
    let full = value(FULL);
    assert_eq!(code_of(FULL), "loaded");
    for (name, pointer) in OBJECTS {
        let object = full.pointer(pointer).expect("the object");
        let mut written = keys(object);
        written.sort_unstable();
        assert_eq!(
            written,
            keys(&definition(&root, name)["properties"]),
            "{name}"
        );
    }
}

#[test]
fn the_reader_knows_exactly_the_keys_the_schema_lists() {
    let root = schema();
    let full = value(FULL);
    for (name, pointer) in OBJECTS {
        let def = definition(&root, name);
        let required: Vec<&str> = def["required"]
            .as_array()
            .expect("required")
            .iter()
            .map(|key| key.as_str().expect("key"))
            .collect();
        for key in keys(&def["properties"]) {
            // Without the key: refused as missing when the schema requires it, loaded when not.
            let mut without = full.clone();
            without
                .pointer_mut(pointer)
                .and_then(Value::as_object_mut)
                .expect("the object")
                .remove(&key);
            let (expected, verdict) = if required.contains(&key.as_str()) {
                ("missing-key", false)
            } else {
                ("loaded", true)
            };
            assert_eq!(
                code_of(&document(&without)),
                expected,
                "{name} without {key}"
            );
            assert_eq!(
                conforms(&without, &root, &root),
                verdict,
                "{name} without {key}"
            );
        }
        // With one key more: the reader refuses it, and so does the schema.
        let mut with = full.clone();
        with.pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .expect("the object")
            .insert("zz_not_a_key".to_owned(), json!(1));
        assert_eq!(code_of(&document(&with)), "unknown-key", "{name}");
        assert!(!conforms(&with, &root, &root), "{name}");
    }
}

#[test]
fn the_schema_and_the_reader_accept_and_refuse_the_same_value_shapes() {
    let root = schema();
    // (the pointer of a value in FULL, replacements, whether each is the right shape there)
    let cases: [(&str, &[(&str, bool)]); 7] = [
        (
            "/scenarios/0/expect/0/equals",
            &[
                ("1", true),
                ("1.5", true),
                ("true", true),
                ("\"1\"", false),
                ("null", false),
                ("[1]", false),
                ("{}", false),
            ],
        ),
        (
            "/scenarios/0/inputs/v",
            &[
                ("0", true),
                ("false", true),
                ("[{\"t\": 0, \"value\": 2}]", true),
                ("\"on\"", false),
                ("null", false),
                ("{\"csv\": \"x\"}", false),
                ("[1]", false),
                ("[[0, 1]]", false),
            ],
        ),
        (
            "/params/a.k",
            &[
                ("0", true),
                ("false", true),
                ("\"x\"", false),
                ("null", false),
                ("[]", false),
            ],
        ),
        (
            "/clock/step_s",
            &[
                ("60", true),
                ("60.0", true),
                ("\"60\"", false),
                ("true", false),
                ("null", false),
            ],
        ),
        (
            "/scenarios/0/name",
            &[
                ("\"t\"", true),
                ("1", false),
                ("null", false),
                ("[\"s\"]", false),
            ],
        ),
        (
            "/scenarios/0/expect/1/diagnostic",
            &[
                ("{\"block\": \"B\"}", true),
                ("\"B\"", false),
                ("[\"B\"]", false),
                ("null", false),
            ],
        ),
        (
            "/schema",
            &[("\"oce/vectors/v1\"", true), ("1", false), ("null", false)],
        ),
    ];
    let full = value(FULL);
    for (pointer, replacements) in cases {
        for (replacement, right) in replacements {
            let mut edited = full.clone();
            *edited.pointer_mut(pointer).expect("the value") = value(replacement);
            let text = document(&edited);
            assert_eq!(
                conforms(&edited, &root, &root),
                *right,
                "{pointer} = {replacement}"
            );
            assert_eq!(
                code_of(&text) == "loaded",
                *right,
                "{pointer} = {replacement}: {}",
                code_of(&text)
            );
            if !right {
                assert_eq!(code_of(&text), "wrong-type", "{pointer} = {replacement}");
            }
        }
    }
    // The tag: the schema's one-value enum and the reader's `unknown-schema`.
    let mut retagged = full.clone();
    retagged["schema"] = json!("cxf-library/vectors/v1");
    assert!(!conforms(&retagged, &root, &root));
    assert_eq!(code_of(&document(&retagged)), "unknown-schema");
}

#[test]
fn an_expectation_of_both_forms_or_neither_fails_the_schema_and_the_reader() {
    let root = schema();
    let full = value(FULL);
    for expectation in [
        json!({}),
        json!({"output": "y", "from_s": 0, "to_s": 60, "equals": false, "at_s": 60}),
        json!({"diagnostic": {"block": "B"}, "at_s": 60, "equals": true}),
    ] {
        let mut edited = full.clone();
        edited["scenarios"][0]["expect"][0] = expectation.clone();
        assert!(!conforms(&edited, &root, &root), "{expectation}");
        assert_eq!(
            code_of(&document(&edited)),
            "ambiguous-expectation",
            "{expectation}"
        );
    }
}

#[test]
fn the_schema_admits_documents_only_the_readers_rules_refuse() {
    // What a JSON Schema of this profile cannot say stays the reader's: the cadence, step
    // order, a window's ticks, names. Each conforms and is refused with its own code.
    let root = schema();
    let full = value(FULL);
    let edits: [(&str, &str, &str); 6] = [
        ("/clock/horizon_s", "1830", "horizon-off-cadence"),
        ("/clock/step_s", "0", "invalid-clock"),
        ("/scenarios/0/inputs/u", "[]", "step-list-empty"),
        ("/scenarios/0/expect/0/from_s", "61", "window-outside-clock"),
        ("/scenarios/0/expect/1/at_s", "90", "instant-off-cadence"),
        ("/scenarios/0/name", "\"\"", "empty-name"),
    ];
    for (pointer, replacement, code) in edits {
        let mut edited = full.clone();
        *edited.pointer_mut(pointer).expect("the value") = value(replacement);
        if pointer.ends_with("from_s") {
            edited["scenarios"][0]["expect"][0]["to_s"] = json!(1860);
        }
        assert!(conforms(&edited, &root, &root), "{pointer}");
        assert_eq!(code_of(&document(&edited)), code, "{pointer}");
    }
}
