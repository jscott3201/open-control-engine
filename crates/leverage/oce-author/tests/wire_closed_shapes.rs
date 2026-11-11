//! Closed shapes in the wire form (GitHub #544): an object is read only from a JSON object, never
//! from the list of its values (which a `serde`-derived struct would read positionally) or from
//! any other type; a signal type is read only from a string; `null` is never a value. Each
//! refusal is `WireError::WrongType` at the path of the offending value.
//!
//! The sweeps replace one value of a fixture at a time and keep every other byte, over every wire
//! fixture and the note's hand-formatted example (plus, for the list sweep, the 31 corpus fixtures
//! the importer reads, written in the wire form). Which JSON types a position takes comes from the published schema, so a DTO added
//! later is swept by construction, and `every_schema_object_is_swept` fails until a fixture
//! carries it.

use std::collections::BTreeSet;

use crate::support;

use oce_author::{WIRE_SCHEMA, WireError, from_wire_str};
use serde_json::Value;

use support::fixture;
use support::spans::{Kind, Site, Step, sites};

/// The hand-written wire documents: every bundle under `tests/fixtures/wire/` (sorted) and the
/// note's worked example, whose spacing is not canonical.
fn hand_written() -> Vec<(String, String)> {
    let mut paths: Vec<_> = std::fs::read_dir(fixture("wire"))
        .expect("wire fixtures")
        .map(|entry| entry.expect("entry").path())
        .collect();
    paths.sort();
    paths.push(fixture("note/wire_example.json"));
    let documents: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned();
            (name, std::fs::read_to_string(path).expect("read"))
        })
        .collect();
    assert!(documents.len() >= 5, "the fixtures are present");
    documents
}

/// The hand-written documents and, written in the wire form, every corpus fixture the importer
/// reads.
fn every_document() -> Vec<(String, String)> {
    let mut documents = hand_written();
    for name in support::cxf::IMPORTED {
        let bytes = std::fs::read(support::cxf::corpus_fixture(name)).expect("corpus fixture");
        let model = support::imported(&bytes);
        documents.push(((*name).to_owned(), oce_author::to_wire_string(&model)));
    }
    documents
}

fn schema() -> Value {
    serde_json::from_str(WIRE_SCHEMA).expect("schema")
}

/// The schema of the value at `steps`, and the name of the definition it is (`Sequence` for the
/// root; a `$ref`'s name; `None` for an inline schema). A step into a value object takes the
/// `anyOf` branch whose properties hold the key.
fn schema_at<'s>(root: &'s Value, steps: &[Step]) -> (&'s Value, Option<String>) {
    let mut schema = root;
    let mut name = Some("Sequence".to_owned());
    for step in steps {
        let next = match step {
            Step::Key(key) => &branch(root, schema, key)["properties"][key],
            Step::Index(_) => &schema["items"],
        };
        (schema, name) = resolve(root, next);
    }
    (schema, name)
}

fn resolve<'s>(root: &'s Value, schema: &'s Value) -> (&'s Value, Option<String>) {
    match schema.get("$ref").and_then(Value::as_str) {
        Some(reference) => {
            let name = reference.strip_prefix("#/$defs/").expect("a local $ref");
            (&root["$defs"][name], Some(name.to_owned()))
        }
        None => (schema, None),
    }
}

/// The object schema `key` belongs to: `schema` itself, or the `anyOf` branch that defines it.
fn branch<'s>(root: &'s Value, schema: &'s Value, key: &str) -> &'s Value {
    let Some(branches) = schema.get("anyOf").and_then(Value::as_array) else {
        return schema;
    };
    branches
        .iter()
        .map(|b| resolve(root, b).0)
        .find(|b| b["properties"].get(key).is_some())
        .unwrap_or_else(|| panic!("no branch defines `{key}`"))
}

/// The name of the object definition an object site is: its own, or for a value object the
/// `anyOf` branch its first key selects.
fn object_definition(root: &Value, text: &str, site: &Site) -> String {
    let (schema, name) = schema_at(root, &site.steps);
    if schema.get("anyOf").is_none() {
        return name.expect("an object site resolves to a definition");
    }
    let inner = sites(&text[site.span.clone()]);
    let Some(Step::Key(key)) = inner.get(1).and_then(|s| s.steps.first()) else {
        panic!("a value object has a key: {}", site.path());
    };
    let reference = schema["anyOf"]
        .as_array()
        .expect("branches")
        .iter()
        .filter_map(|b| b.get("$ref").and_then(Value::as_str))
        .find(|r| {
            let name = r.strip_prefix("#/$defs/").expect("local");
            root["$defs"][name]["properties"]
                .get(key.as_str())
                .is_some()
        })
        .unwrap_or_else(|| panic!("no branch for `{key}` at {}", site.path()));
    reference
        .strip_prefix("#/$defs/")
        .expect("local")
        .to_owned()
}

/// The JSON types the schema lets a value at `schema` have.
fn kinds(schema: &Value, root: &Value) -> Vec<Kind> {
    if let Some(branches) = schema.get("anyOf").and_then(Value::as_array) {
        return branches
            .iter()
            .flat_map(|b| kinds(resolve(root, b).0, root))
            .collect();
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => vec![Kind::Object],
        Some("array") => vec![Kind::List],
        Some("string") => vec![Kind::String],
        Some("number") => vec![Kind::Number],
        Some("boolean") => vec![Kind::Boolean],
        other => panic!("a schema type outside the profile: {other:?}"),
    }
}

/// `None` when `text` is refused as a wrong type at `path`, placed no earlier than `line` (the
/// line the value starts on: the reader places a refusal where its parse stopped, which for a
/// value refused after it is read is past it); otherwise what it did.
fn wrong_type_at(text: &str, path: &str, line: u32) -> Option<String> {
    match from_wire_str(text) {
        Err(WireError::WrongType { path: got, at, .. }) if got == path && at.line >= line => None,
        other => Some(format!("{other:?}")),
    }
}

#[test]
fn every_object_refuses_the_list_of_its_values() {
    let root = schema();
    let mut swept = 0usize;
    let mut accepted = Vec::new();
    for (name, text) in every_document() {
        assert!(from_wire_str(&text).is_ok(), "{name} reads as written");
        for site in sites(&text).iter().filter(|s| s.kind == Kind::Object) {
            swept += 1;
            let mutant = site.as_list_of_values(&text);
            if let Some(outcome) = wrong_type_at(&mutant, &site.path(), site.line(&text)) {
                let definition = object_definition(&root, &text, site);
                accepted.push(format!("{name} {} ({definition}): {outcome}", site.path()));
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "{} of {swept} objects were not refused as the list of their values:\n{}",
        accepted.len(),
        accepted.join("\n")
    );
    assert!(swept > 1_500, "the sweep reached the corpus: {swept}");
}

/// A replacement value of each JSON type.
const OTHER_TYPES: [(&str, Kind); 6] = [
    ("null", Kind::Null),
    ("true", Kind::Boolean),
    ("0", Kind::Number),
    ("\"\"", Kind::String),
    ("[]", Kind::List),
    ("{}", Kind::Object),
];

/// Replace every value of every hand-written document by each value in `replacements` whose JSON
/// type the schema does not allow there, and return what was not refused at its path.
fn sweep_other_types(replacements: &[(&str, Kind)]) -> (usize, Vec<String>) {
    let root = schema();
    let mut swept = 0usize;
    let mut missed = Vec::new();
    for (name, text) in hand_written() {
        for site in sites(&text) {
            let allowed = kinds(schema_at(&root, &site.steps).0, &root);
            assert!(allowed.contains(&site.kind), "{name} {}", site.path());
            for (spelling, kind) in replacements {
                if allowed.contains(kind) {
                    continue;
                }
                swept += 1;
                let mutant = site.replaced(&text, spelling);
                if let Some(outcome) = wrong_type_at(&mutant, &site.path(), site.line(&text)) {
                    missed.push(format!("{name} {} = {spelling}: {outcome}", site.path()));
                }
            }
        }
    }
    (swept, missed)
}

#[test]
fn every_value_refuses_a_json_type_its_position_does_not_take() {
    let (swept, missed) = sweep_other_types(&OTHER_TYPES);
    assert!(
        missed.is_empty(),
        "{} of {swept} values of another type were not refused at their path:\n{}",
        missed.len(),
        missed.join("\n")
    );
    assert!(swept > 2_000, "the sweep reached the fixtures: {swept}");
}

#[test]
fn null_is_refused_at_every_position() {
    // The wire form has no nullable value: every key is required and an absent value is "", []
    // or false (authoring note, "Wire form"). No position's schema takes null, so every value is
    // swept.
    let (swept, missed) = sweep_other_types(&OTHER_TYPES[..1]);
    assert!(missed.is_empty(), "{}", missed.join("\n"));
    let positions: usize = hand_written().iter().map(|(_, t)| sites(t).len()).sum();
    assert_eq!(swept, positions, "null replaced every value");
}

#[test]
fn every_schema_object_is_swept() {
    // A struct added to the wire form gets a schema definition; until a fixture carries one, the
    // sweeps above never replace it, so this names it.
    let root = schema();
    let mut defined: BTreeSet<String> = root["$defs"]
        .as_object()
        .expect("$defs")
        .iter()
        .filter(|(_, d)| d.get("type").and_then(Value::as_str) == Some("object"))
        .map(|(name, _)| name.clone())
        .collect();
    defined.insert("Sequence".to_owned());
    let mut met = BTreeSet::new();
    for (_, text) in hand_written() {
        for site in sites(&text).iter().filter(|s| s.kind == Kind::Object) {
            met.insert(object_definition(&root, &text, site));
        }
    }
    assert_eq!(
        met, defined,
        "every object the schema defines is in a fixture"
    );
    assert_eq!(
        defined,
        [
            "Assignment",
            "Block",
            "Boundary",
            "Member",
            "Reference",
            "SemanticPoint",
            "Sequence",
            "SequenceParameter",
        ]
        .map(str::to_owned)
        .into(),
        "the schema's objects"
    );
}

/// The refusal of `text` with the value at `path` replaced by the list of its values.
fn listed(text: &str, path: &str) -> WireError {
    let site = sites(text)
        .into_iter()
        .find(|s| s.path() == path)
        .unwrap_or_else(|| panic!("no value at {path}"));
    from_wire_str(&site.as_list_of_values(text)).expect_err(path)
}

#[test]
fn each_derived_struct_names_its_schema_object_when_given_a_list() {
    let econ = std::fs::read_to_string(fixture("wire/econ_enable.json")).expect("econ");
    let every = std::fs::read_to_string(fixture("wire/every_element.json")).expect("every");
    let value_shapes = r#"a number, a boolean, a string, {"ref": …} or {"member": …}"#;
    let cases = [
        (&econ, "document", "a Sequence object".to_owned()),
        (&econ, "params[0]", "a SequenceParameter object".to_owned()),
        (&econ, "inputs[0]", "a Boundary object".to_owned()),
        (
            &econ,
            "inputs[1].semantic",
            "a SemanticPoint object".to_owned(),
        ),
        (&econ, "outputs[0]", "a Boundary object".to_owned()),
        (
            &econ,
            "outputs[0].semantic",
            "a SemanticPoint object".to_owned(),
        ),
        (&econ, "blocks[0]", "a Block object".to_owned()),
        (
            &econ,
            "blocks[1].params[0]",
            "an Assignment object".to_owned(),
        ),
        // The value shapes are read by a visitor of their own, which takes no list.
        (&econ, "blocks[1].params[1].value", value_shapes.to_owned()),
        (&every, "params[4].value", value_shapes.to_owned()),
    ];
    for (text, path, expected) in cases {
        match listed(text, path) {
            WireError::WrongType {
                detail, path: got, ..
            } => {
                assert_eq!(got, path);
                assert_eq!(
                    detail,
                    format!("invalid type: sequence, expected {expected}")
                );
            }
            other => panic!("{path}: {other:?}"),
        }
    }
}

#[test]
fn the_refusal_is_placed_at_the_value_it_names() {
    let econ = std::fs::read_to_string(fixture("wire/econ_enable.json")).expect("econ");
    let error = listed(&econ, "inputs[0].semantic");
    // `serde_json` places a refused list at the byte before its `[`: the `[` of
    // `"semantic": [` is column 19 of line 19.
    assert_eq!(
        error,
        WireError::WrongType {
            detail: "invalid type: sequence, expected a SemanticPoint object".to_owned(),
            path: "inputs[0].semantic".to_owned(),
            at: oce_author::Location::new(19, 18),
        }
    );
    assert_eq!(
        error.to_string(),
        "invalid type: sequence, expected a SemanticPoint object at inputs[0].semantic, line 19 \
         column 18"
    );
}

#[test]
fn a_signal_type_is_read_from_a_string_only() {
    let econ = std::fs::read_to_string(fixture("wire/econ_enable.json")).expect("econ");
    let site = sites(&econ)
        .into_iter()
        .find(|s| s.path() == "inputs[0].type")
        .expect("a boundary type");
    let variants = r#"one of the strings "Real", "Integer", "Boolean""#;
    for (spelling, detail) in [
        // A derived enum also reads the map form `{"Variant": null}`; the wire form does not.
        (
            r#"{"Real": null}"#,
            format!("invalid type: map, expected {variants}"),
        ),
        (
            "[\"Real\"]",
            format!("invalid type: sequence, expected {variants}"),
        ),
        // A number or null here was a `json-syntax` refusal ("expected value") before GitHub #544.
        (
            "7",
            format!("invalid type: integer `7`, expected {variants}"),
        ),
        ("null", format!("invalid type: null, expected {variants}")),
        (
            "\"real\"",
            "unknown variant `real`, expected one of `Real`, `Integer`, `Boolean`".to_owned(),
        ),
    ] {
        match from_wire_str(&site.replaced(&econ, spelling)) {
            Err(WireError::WrongType {
                detail: got, path, ..
            }) => {
                assert_eq!(path, "inputs[0].type", "{spelling}");
                assert_eq!(got, detail, "{spelling}");
            }
            other => panic!("{spelling}: {other:?}"),
        }
    }
    for spelling in ["\"Real\"", "\"Integer\"", "\"Boolean\""] {
        assert!(
            from_wire_str(&site.replaced(&econ, spelling)).is_ok(),
            "{spelling}"
        );
    }
}

#[test]
fn an_absent_value_is_spelled_empty_never_null() {
    let econ = std::fs::read_to_string(fixture("wire/econ_enable.json")).expect("econ");
    let at = |path: &str| {
        sites(&econ)
            .into_iter()
            .find(|s| s.path() == path)
            .unwrap_or_else(|| panic!("{path}"))
    };
    // The absent spellings read as absent ...
    let brick = at("inputs[0].semantic.brick");
    let read = from_wire_str(&brick.replaced(&econ, "\"\"")).expect("an empty string");
    assert_eq!(read.inputs[0].semantic.brick, None);
    // ... and `null`, in the same positions and in every other kind of slot, is a wrong type.
    for path in [
        "inputs[0].semantic.brick",
        "inputs[0].semantic.haystack",
        "inputs[0].semantic.provisional",
        "inputs[0].min",
        "inputs[0].quantity",
        "extends",
        "params[0].value",
        "blocks[1].params[1].value.ref",
    ] {
        let site = at(path);
        match from_wire_str(&site.replaced(&econ, "null")) {
            Err(WireError::WrongType {
                detail, path: got, ..
            }) => {
                assert_eq!(got, path);
                assert!(
                    detail.starts_with("invalid type: null, expected"),
                    "{detail}"
                );
            }
            other => panic!("{path}: {other:?}"),
        }
    }
}

/// The `serde` attributes the wire DTOs may carry. Any other can let a value past the
/// closed-shape adapter: `untagged`, `tag` and `content` read a struct variant from serde's
/// buffered content (a list included, positionally), `flatten` reads through a map of its own,
/// `default` and `skip_deserializing` let a key go missing, `alias` and `other` read a key or a
/// variant the schema does not name.
const ALLOWED_SERDE_ATTRIBUTES: [&str; 2] = ["deny_unknown_fields", "rename"];

/// What a `cfg_attr` that carries a serde attribute is recorded as: never allowed, since the
/// attribute it applies under a condition is one the scan cannot read as written.
const CONDITIONAL_SERDE: &str = "cfg_attr carrying serde";

/// The names inside every `#[serde(...)]` attribute of `source` (`rename = "x"` gives `rename`), and
/// [`CONDITIONAL_SERDE`] for each `#[cfg_attr(...)]` that carries one. Whitespace is removed first,
/// so `#[serde (untagged)]` and `#[ serde(untagged) ]` are read like `#[serde(untagged)]`.
fn serde_attributes(source: &str) -> Vec<String> {
    let compact: String = source.chars().filter(|c| !c.is_whitespace()).collect();
    let mut names = Vec::new();
    let mut rest = compact.as_str();
    while let Some(at) = rest.find("#[serde(") {
        let body = &rest[at + "#[serde(".len()..];
        let end = body.find(")]").expect("a closed serde attribute");
        for part in body[..end].split(',') {
            let name = part.split('=').next().unwrap_or("");
            if !name.is_empty() {
                names.push(name.to_owned());
            }
        }
        rest = &body[end..];
    }
    let mut rest = compact.as_str();
    while let Some(at) = rest.find("#[cfg_attr(") {
        let body = &rest[at + "#[cfg_attr(".len()..];
        let end = body.find(")]").map_or(body.len(), |end| end);
        if body[..end].contains("serde") {
            names.push(CONDITIONAL_SERDE.to_owned());
        }
        rest = &body[end..];
    }
    names
}

/// A tripwire, not a proof: it reads the attribute text of every source file, so it stops the
/// attributes that read through serde's buffered content (or loosen keys and variants) from being
/// added to a DTO by accident. A hand-written `Deserialize` that buffers the value through
/// `serde_json::Value` or `from_value` is out of its reach; the shape sweeps above catch that at
/// any position a fixture carries, and `every_schema_object_is_swept` makes every schema object
/// such a position.
#[test]
fn the_wire_dtos_carry_no_serde_attribute_that_bypasses_the_closed_shapes() {
    let mut seen = 0usize;
    let mut refused = Vec::new();
    let mut pending = vec![support::crate_dir().join("src")];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("source dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !path.to_string_lossy().ends_with("_tests.rs")
            {
                // A unit-test file (`closed_tests.rs`) defines its own types, never a DTO.
                let source = std::fs::read_to_string(&path).expect("source");
                for name in serde_attributes(&source) {
                    seen += 1;
                    if !ALLOWED_SERDE_ATTRIBUTES.contains(&name.as_str()) {
                        refused.push(format!("{}: {name}", path.display()));
                    }
                }
            }
        }
    }
    assert!(refused.is_empty(), "{}", refused.join("\n"));
    assert!(seen >= 10, "the scan found the DTO attributes: {seen}");
    assert_eq!(
        serde_attributes("#[serde(deny_unknown_fields, rename = \"A\")]\n#[serde(untagged)]"),
        ["deny_unknown_fields", "rename", "untagged"],
        "the scan reads every name"
    );
    assert_eq!(
        serde_attributes(
            "#[serde (untagged)]\n#[ serde( tag = \"t\" ) ]\n#[serde(\n    flatten\n)]"
        ),
        ["untagged", "tag", "flatten"],
        "whitespace does not hide a name"
    );
    assert_eq!(
        serde_attributes(
            "#[cfg_attr(feature = \"x\", serde(untagged))]\n#[cfg_attr(test, derive(Debug))]"
        ),
        [CONDITIONAL_SERDE],
        "a conditional serde attribute is refused; another cfg_attr is not read"
    );
    assert!(!ALLOWED_SERDE_ATTRIBUTES.contains(&CONDITIONAL_SERDE));
}
