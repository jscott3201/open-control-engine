//! Document-to-document oracle for declared boundary-input attribute fidelity (#243).
//!
//! The former loss happened before `ModelGraph` existed, so model-level round trips agreed with
//! themselves while dropping the authored values. These tests derive expectations from the source
//! JSON and compare declaration nodes directly.

mod bless;
#[path = "export_declared_input_attrs/document.rs"]
mod document;
#[path = "export_declared_input_attrs/validation.rs"]
mod validation;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use oce_cxf::{CxfDocument, ResolveOptions, export, import_cxf, parse_document, write_document};
use oce_model::Attrs;
use serde_json::Value as JsonValue;

use document::node_mut;

const ATTR_KEYS: [&str; 5] = [
    "S231:unit",
    "S231:quantity",
    "S231:displayUnit",
    "S231:min",
    "S231:max",
];

const DECLARED_INPUT_ATTRS: &str = include_str!("fixtures/declared_input_attrs.jsonld");

const NORMALIZED_EXPORT_HEADER: &[u8] = b"\
# Declared boundary-input allowed-delta expectations.
# Owner test: crates/oce-cxf/tests/export_declared_input_attrs.rs.
# First generated at development@f88b2a3 before #243 behavior.
# Each record contains canonical exported bytes after removing only the five scoped attributes
# from root boundary-input nodes. Post-#243 output must reduce to these exact bytes.
# Format: <fixture stem> <byte length>, newline, canonical CXF bytes, newline.
";

#[derive(Debug, PartialEq, Eq)]
struct Census {
    fixtures: usize,
    authored: usize,
    surviving: usize,
    attr_carrying: usize,
    attr_population: [usize; 5],
    lost_attr_population: [usize; 5],
    authored_type: usize,
    lost_type: usize,
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/g36")
}

fn sorted_fixture_paths() -> Vec<PathBuf> {
    let mut paths = fs::read_dir(corpus_dir())
        .expect("read swept CXF corpus")
        .map(|entry| entry.expect("readable fixture entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonld")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn fixture_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .expect("UTF-8 fixture name")
}

fn import_ok(fixture: &str, bytes: &[u8]) -> oce_model::ModelGraph {
    let (graph, report) = import_cxf(bytes, &ResolveOptions::default())
        .unwrap_or_else(|error| panic!("`{fixture}` imports: {error:?}"));
    assert!(
        report.is_empty(),
        "`{fixture}` expected no diagnostics, got {:?}",
        report.diagnostics
    );
    graph
}

fn top_composite(document: &JsonValue) -> &JsonValue {
    document["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node.get("S231:containsBlock").is_some())
        .expect("top composite")
}

fn reference_ids(value: Option<&JsonValue>) -> BTreeSet<String> {
    let Some(value) = value else {
        return BTreeSet::new();
    };
    value
        .as_array()
        .map_or_else(|| vec![value], |items| items.iter().collect())
        .into_iter()
        .map(|reference| reference["@id"].as_str().expect("reference @id").to_owned())
        .collect()
}

fn node_by_id<'document>(document: &'document JsonValue, id: &str) -> &'document JsonValue {
    document["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .find(|node| node["@id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("node `{id}` exists"))
}

fn real_attrs(attrs: &Attrs) -> (&str, Option<&str>, Option<&str>, Option<u64>, Option<u64>) {
    let Attrs::Real(attrs) = attrs else {
        panic!("expected Real attrs, got {attrs:?}");
    };
    (
        attrs.unit.as_deref().expect("unit"),
        attrs.quantity.as_deref(),
        attrs.display_unit.as_deref(),
        attrs.min.map(f64::to_bits),
        attrs.max.map(f64::to_bits),
    )
}

#[test]
fn represented_declarations_keep_their_own_attrs_in_graph_order() {
    let graph = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    let declarations = graph
        .boundary_inputs
        .iter()
        .map(|input| input.iri.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(
        declarations,
        [
            "http://example.org#DeclaredInputAttrs.uPass",
            "http://example.org#DeclaredInputAttrs.uExt",
        ],
        "declaration order follows boundary-node @graph position, not root hasInput order"
    );
    assert_eq!(
        real_attrs(&graph.boundary_inputs[0].attrs),
        (
            "Pa",
            Some("PressureDifference"),
            None,
            Some((-50.0f64).to_bits()),
            Some(50.0f64.to_bits()),
        )
    );
    assert_eq!(
        real_attrs(&graph.boundary_inputs[1].attrs),
        (
            "K",
            Some("ThermodynamicTemperature"),
            Some("degC"),
            Some(200.0f64.to_bits()),
            Some(330.0f64.to_bits()),
        )
    );

    let child_attrs = graph
        .external_inputs
        .iter()
        .filter_map(|id| graph.connectors.get(id.0 as usize))
        .filter(|connector| {
            connector
                .iri
                .as_deref()
                .is_some_and(|iri| iri.ends_with("uExt"))
        })
        .map(|connector| real_attrs(&connector.attrs))
        .collect::<Vec<_>>();
    assert_eq!(
        child_attrs,
        [
            ("K", None, Some("degF"), None, None),
            ("K", None, None, None, None),
        ],
        "child connectors retain their own attrs instead of receiving the declaration attrs"
    );
}

#[test]
fn fanout_and_pass_through_emit_one_attr_set_per_declaration() {
    let graph = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    let bytes = export(&graph).expect("fixture is in the export subset");
    let document: JsonValue = serde_json::from_slice(&bytes).expect("exported JSON");

    let external = node_by_id(&document, "http://example.org#DeclaredInputAttrs.uExt");
    assert_eq!(
        (
            external["S231:unit"].as_str(),
            external["S231:quantity"].as_str(),
            external["S231:displayUnit"].as_str(),
            external["S231:min"].as_f64().map(f64::to_bits),
            external["S231:max"].as_f64().map(f64::to_bits),
        ),
        (
            Some("K"),
            Some("ThermodynamicTemperature"),
            Some("degC"),
            Some(200.0f64.to_bits()),
            Some(330.0f64.to_bits()),
        )
    );
    assert_eq!(
        reference_ids(external.get("S231:isConnectedTo")),
        BTreeSet::from([
            "http://example.org#DeclaredInputAttrs.add.in0".to_owned(),
            "http://example.org#DeclaredInputAttrs.add.in1".to_owned(),
        ]),
        "fan-out remains one declaration with both child targets"
    );

    let first_child = node_by_id(&document, "http://example.org#DeclaredInputAttrs.add.in0");
    assert_eq!(
        (
            first_child["S231:unit"].as_str(),
            first_child["S231:displayUnit"].as_str(),
            first_child.get("S231:quantity"),
        ),
        (Some("K"), Some("degF"), None),
        "the child node emits its own attrs"
    );

    let pass_through = node_by_id(&document, "http://example.org#DeclaredInputAttrs.uPass");
    assert_eq!(
        (
            pass_through["S231:unit"].as_str(),
            pass_through["S231:quantity"].as_str(),
            pass_through["S231:min"].as_f64().map(f64::to_bits),
            pass_through["S231:max"].as_f64().map(f64::to_bits),
        ),
        (
            Some("Pa"),
            Some("PressureDifference"),
            Some((-50.0f64).to_bits()),
            Some(50.0f64.to_bits()),
        ),
        "pass-through lowering retains the input declaration attrs"
    );
}

#[test]
fn exported_declarations_reimport_with_bit_identical_sidecar_attrs() {
    let first = import_ok(
        "declared_input_attrs.jsonld",
        DECLARED_INPUT_ATTRS.as_bytes(),
    );
    let bytes = export(&first).expect("fixture exports");
    let second = import_ok("declared_input_attrs export", &bytes);
    let render = |graph: &oce_model::ModelGraph| {
        graph
            .boundary_inputs
            .iter()
            .map(|input| {
                let (unit, quantity, display_unit, min, max) = real_attrs(&input.attrs);
                (
                    input.iri.to_string(),
                    unit.to_owned(),
                    quantity.map(str::to_owned),
                    display_unit.map(str::to_owned),
                    min,
                    max,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(render(&second), render(&first));
}

#[test]
fn integer_bounds_remain_integer_declaration_attrs_across_export_and_reimport() {
    let document = document::attr_input_document(
        "Integer",
        "CDL.Integers.Add",
        serde_json::json!({ "S231:min": i64::MIN, "S231:max": i64::MAX }),
    );
    let source = serde_json::to_vec(&document).expect("document serializes");
    let first = import_ok("integer boundary attrs", &source);
    let Attrs::Integer(first_attrs) = &first.boundary_inputs[0].attrs else {
        panic!("integer boundary has Integer attrs");
    };
    assert_eq!(
        (first_attrs.min, first_attrs.max),
        (Some(i64::MIN), Some(i64::MAX))
    );

    let bytes = export(&first).expect("integer boundary attrs export");
    let exported: JsonValue = serde_json::from_slice(&bytes).expect("exported JSON");
    let boundary = node_by_id(&exported, "http://example.org#AttrInput.uExt");
    assert_eq!(boundary["S231:min"].as_i64(), Some(i64::MIN));
    assert_eq!(boundary["S231:max"].as_i64(), Some(i64::MAX));

    let second = import_ok("integer boundary attrs export", &bytes);
    let second_boundary = second
        .boundary_inputs
        .iter()
        .find(|input| input.iri.ends_with("uExt"))
        .expect("reimported uExt declaration");
    let Attrs::Integer(second_attrs) = &second_boundary.attrs else {
        panic!("reimported integer boundary has Integer attrs");
    };
    assert_eq!(
        (second_attrs.min, second_attrs.max),
        (first_attrs.min, first_attrs.max)
    );
}

#[test]
fn real_bound_sign_bits_survive_export_and_reimport() {
    let document = document::attr_input_document(
        "Real",
        "CDL.Reals.Add",
        serde_json::json!({ "S231:min": -0.0, "S231:max": 0.0 }),
    );
    let source = serde_json::to_vec(&document).expect("document serializes");
    let first = import_ok("signed-zero boundary attrs", &source);
    let boundary_bits = |graph: &oce_model::ModelGraph| {
        let boundary = graph
            .boundary_inputs
            .iter()
            .find(|input| input.iri.ends_with("uExt"))
            .expect("uExt declaration");
        let Attrs::Real(attrs) = &boundary.attrs else {
            panic!("uExt has Real attrs");
        };
        (
            attrs.min.expect("min").to_bits(),
            attrs.max.expect("max").to_bits(),
        )
    };
    assert_eq!(
        boundary_bits(&first),
        ((-0.0f64).to_bits(), 0.0f64.to_bits())
    );

    let bytes = export(&first).expect("signed-zero boundary attrs export");
    let exported: JsonValue = serde_json::from_slice(&bytes).expect("exported JSON");
    let boundary = node_by_id(&exported, "http://example.org#AttrInput.uExt");
    assert_eq!(
        (
            boundary["S231:min"].as_f64().expect("min").to_bits(),
            boundary["S231:max"].as_f64().expect("max").to_bits(),
        ),
        ((-0.0f64).to_bits(), 0.0f64.to_bits())
    );
    let second = import_ok("signed-zero boundary attrs export", &bytes);
    assert_eq!(boundary_bits(&second), boundary_bits(&first));
}

#[test]
fn represented_attribute_free_inputs_get_sidecars_but_undriven_inputs_do_not() {
    let mut document =
        document::attr_input_document("Real", "CDL.Reals.Add", serde_json::json!({}));
    node_mut(&mut document, "uExt")
        .as_object_mut()
        .expect("boundary node object")
        .remove("S231:isConnectedTo");
    node_mut(&mut document, "AttrInput")["S231:hasInput"]
        .as_array_mut()
        .expect("root hasInput array")
        .push(serde_json::json!({ "@id": "http://example.org#AttrInput.uReplacement" }));
    document["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .push(serde_json::json!({
            "@id": "http://example.org#AttrInput.uReplacement",
            "@type": "S231:RealInput",
            "S231:isOfDataType": { "@id": "S231:Real" },
            "S231:isConnectedTo": { "@id": "http://example.org#AttrInput.add.u1" }
        }));

    let source = serde_json::to_vec(&document).expect("document serializes");
    let graph = import_ok("attribute-free and undriven boundaries", &source);
    assert_eq!(
        graph
            .boundary_inputs
            .iter()
            .map(|input| input.iri.as_ref())
            .collect::<Vec<_>>(),
        [
            "http://example.org#AttrInput.uOther",
            "http://example.org#AttrInput.uReplacement",
        ]
    );
    for input in &graph.boundary_inputs {
        let Attrs::Real(attrs) = &input.attrs else {
            panic!("represented declaration has Real attrs");
        };
        assert!(
            attrs.unit.is_none()
                && attrs.quantity.is_none()
                && attrs.display_unit.is_none()
                && attrs.min.is_none()
                && attrs.max.is_none()
        );
    }
}

fn declaration_iris(document: &JsonValue) -> Vec<String> {
    let bytes = serde_json::to_vec(document).expect("document serializes");
    import_ok("order-discriminator", &bytes)
        .boundary_inputs
        .into_iter()
        .map(|input| input.iri.to_string())
        .collect()
}

fn authored_boundary_order(document: &JsonValue) -> Vec<String> {
    let declared = reference_ids(top_composite(document).get("S231:hasInput"));
    document["@graph"]
        .as_array()
        .expect("@graph array")
        .iter()
        .filter_map(|node| node["@id"].as_str())
        .filter(|id| declared.contains(*id))
        .map(str::to_owned)
        .collect()
}

#[test]
fn declaration_order_ignores_reference_arrays_but_follows_graph_position() {
    let original: JsonValue = serde_json::from_str(DECLARED_INPUT_ATTRS).expect("fixture JSON");
    let expected = authored_boundary_order(&original);
    assert_eq!(declaration_iris(&original), expected);

    let mut reference_permutation = original.clone();
    node_mut(&mut reference_permutation, "DeclaredInputAttrs")["S231:hasInput"]
        .as_array_mut()
        .expect("root hasInput array")
        .reverse();
    node_mut(&mut reference_permutation, "uExt")["S231:isConnectedTo"]
        .as_array_mut()
        .expect("fan-out array")
        .reverse();
    assert_eq!(
        declaration_iris(&reference_permutation),
        authored_boundary_order(&reference_permutation)
    );

    let mut graph_permutation = original;
    let graph = graph_permutation["@graph"]
        .as_array_mut()
        .expect("@graph array");
    let pass_index = graph
        .iter()
        .position(|node| node["@id"].as_str().is_some_and(|id| id.ends_with("uPass")))
        .expect("uPass position");
    let external_index = graph
        .iter()
        .position(|node| node["@id"].as_str().is_some_and(|id| id.ends_with("uExt")))
        .expect("uExt position");
    graph.swap(pass_index, external_index);
    let moved = authored_boundary_order(&graph_permutation);
    assert_eq!(declaration_iris(&graph_permutation), moved);
    assert_eq!(moved, expected.into_iter().rev().collect::<Vec<_>>());
}

fn term_string(fixture: &str, id: &str, key: &str, value: &JsonValue) -> String {
    value
        .as_str()
        .unwrap_or_else(|| panic!("`{fixture}` {id} {key}: expected a bare string, got {value}"))
        .to_owned()
}

#[derive(Debug, PartialEq, Eq)]
enum BoundValue {
    Real(u64),
    Integer(i64),
}

fn bound_value(fixture: &str, id: &str, key: &str, value: &JsonValue, integer: bool) -> BoundValue {
    if let Some(object) = value.as_object() {
        let lexical = object
            .get("@value")
            .and_then(JsonValue::as_str)
            .unwrap_or_else(|| panic!("`{fixture}` {id} {key}: typed literal without @value"));
        if integer {
            BoundValue::Integer(lexical.parse::<i64>().unwrap_or_else(|error| {
                panic!("`{fixture}` {id} {key}: invalid Integer bound: {error}")
            }))
        } else {
            BoundValue::Real(
                lexical
                    .parse::<f64>()
                    .unwrap_or_else(|error| {
                        panic!("`{fixture}` {id} {key}: invalid Real bound: {error}")
                    })
                    .to_bits(),
            )
        }
    } else if integer {
        BoundValue::Integer(
            value.as_i64().unwrap_or_else(|| {
                panic!("`{fixture}` {id} {key}: expected an Integer, got {value}")
            }),
        )
    } else {
        BoundValue::Real(
            value
                .as_f64()
                .unwrap_or_else(|| panic!("`{fixture}` {id} {key}: expected a Real, got {value}"))
                .to_bits(),
        )
    }
}

fn current_census() -> (Census, Vec<String>) {
    let mut census = Census {
        fixtures: 0,
        authored: 0,
        surviving: 0,
        attr_carrying: 0,
        attr_population: [0; 5],
        lost_attr_population: [0; 5],
        authored_type: 0,
        lost_type: 0,
    };
    let mut mismatches = Vec::new();

    for path in sorted_fixture_paths() {
        let fixture = fixture_name(&path);
        let bytes = fs::read(&path).unwrap_or_else(|error| panic!("`{fixture}` reads: {error}"));
        let graph = import_ok(fixture, &bytes);
        let exported =
            export(&graph).unwrap_or_else(|error| panic!("`{fixture}` exports: {error:?}"));
        let authored_doc: JsonValue = serde_json::from_slice(&bytes).expect("authored JSON");
        let exported_doc: JsonValue = serde_json::from_slice(&exported).expect("exported JSON");
        let authored_inputs = reference_ids(top_composite(&authored_doc).get("S231:hasInput"));
        let exported_inputs = reference_ids(top_composite(&exported_doc).get("S231:hasInput"));
        census.fixtures += 1;
        census.authored += authored_inputs.len();

        for id in authored_inputs.intersection(&exported_inputs) {
            let authored = node_by_id(&authored_doc, id);
            let exported = node_by_id(&exported_doc, id);
            let integer_bounds = authored["S231:isOfDataType"]["@id"]
                .as_str()
                .is_some_and(|datatype| datatype.ends_with("Integer"));
            census.surviving += 1;
            let mut carries_attr = false;
            for (index, key) in ATTR_KEYS.iter().enumerate() {
                if authored.get(*key).is_some() {
                    census.attr_population[index] += 1;
                    carries_attr = true;
                    if exported.get(*key).is_none() {
                        census.lost_attr_population[index] += 1;
                    }
                }
            }
            for key in ["S231:unit", "S231:quantity", "S231:displayUnit"] {
                let authored_value = authored
                    .get(key)
                    .map(|value| term_string(fixture, id, key, value));
                let exported_value = exported
                    .get(key)
                    .map(|value| term_string(fixture, id, key, value));
                if authored_value != exported_value {
                    mismatches.push(format!(
                        "`{fixture}` {id} {key}: authored {authored_value:?}, exported \
                         {exported_value:?}"
                    ));
                }
            }
            for key in ["S231:min", "S231:max"] {
                let authored_value = authored
                    .get(key)
                    .map(|value| bound_value(fixture, id, key, value, integer_bounds));
                let exported_value = exported
                    .get(key)
                    .map(|value| bound_value(fixture, id, key, value, integer_bounds));
                if authored_value != exported_value {
                    mismatches.push(format!(
                        "`{fixture}` {id} {key}: authored {authored_value:?}, exported \
                         {exported_value:?}"
                    ));
                }
            }
            census.attr_carrying += usize::from(carries_attr);
            if authored.get("@type").is_some() {
                census.authored_type += 1;
                census.lost_type += usize::from(exported.get("@type").is_none());
            }
        }
    }

    (census, mismatches)
}

#[test]
fn corpus_declared_inputs_export_their_authored_attrs_key_by_key() {
    let (census, mismatches) = current_census();
    assert!(
        mismatches.is_empty(),
        "declared boundary-input attribute mismatches ({}):\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    assert_eq!(
        census,
        Census {
            fixtures: 47,
            authored: 184,
            surviving: 170,
            attr_carrying: 100,
            // unit, quantity, displayUnit, min, max
            attr_population: [100, 64, 10, 22, 21],
            lost_attr_population: [0; 5],
            authored_type: 170,
            lost_type: 170,
        },
        "the compared boundary-input population moved"
    );
}

fn strip_scoped_attrs(mut document: CxfDocument) -> CxfDocument {
    let boundary_ids: BTreeSet<String> = document
        .graph
        .first()
        .map(|root| {
            root.has_input
                .iter()
                .map(|reference| reference.id.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    for node in &mut document.graph {
        if boundary_ids.contains(&node.id) {
            node.unit = None;
            node.quantity = None;
            node.display_unit = None;
            node.min = None;
            node.max = None;
        }
    }
    document
}

fn rendered_normalized_expectations() -> Vec<u8> {
    let mut rendered = NORMALIZED_EXPORT_HEADER.to_vec();
    for path in sorted_fixture_paths() {
        let fixture = fixture_name(&path);
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("UTF-8 fixture stem");
        let bytes = fs::read(&path).unwrap_or_else(|error| panic!("`{fixture}` reads: {error}"));
        let graph = import_ok(fixture, &bytes);
        let exported =
            export(&graph).unwrap_or_else(|error| panic!("`{fixture}` exports: {error:?}"));
        let document = parse_document(&exported).expect("exported CXF parses");
        let normalized =
            write_document(&strip_scoped_attrs(document)).expect("normalized CXF writes");
        rendered.extend_from_slice(format!("{stem} {}\n", normalized.len()).as_bytes());
        rendered.extend_from_slice(&normalized);
        rendered.push(b'\n');
    }
    rendered
}

#[test]
fn removing_only_scoped_attrs_recovers_every_recorded_export() {
    let actual = rendered_normalized_expectations();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/expectations/g36_boundary_input_allowed_delta.golden");
    if bless::enabled() {
        fs::write(&path, &actual).expect("write normalized export expectations");
        return;
    }
    let expected = fs::read(&path).expect("read normalized export expectations");
    assert_eq!(
        actual, expected,
        "export changed outside the five scoped boundary-input attributes"
    );
}
