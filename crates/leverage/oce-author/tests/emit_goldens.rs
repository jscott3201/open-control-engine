//! Emitter byte goldens (authoring note, "Emitter" and "Determinism tests"): the worked example,
//! the every-element sequence and every corpus fixture the importer covers, emitted with the
//! default namespace and compared bit for bit with `tests/fixtures/emit/` (`.cxf.json`, so the
//! engine's tracked-`.jsonld` corpus census does not count them); each golden loads
//! warning-free (acceptance precondition 4), emits twice to the same bytes and loads under an
//! overridden namespace too.

use crate::support;

use oce_author::{EmitOptions, Sequence, emit};

use support::cxf::{corpus_fixture, load_clean};
use support::{compare_or_bless, econ_enable, every_element, fixture};

/// The corpus fixtures the importer covers (see `library_round_trip.rs`).
const CORPUS: &[&str] = &[
    "cooling_only_system_requests",
    "generic_time_suppression",
    "multizone_vav_relief_fan_group",
    "reheat_overrides",
];

fn goldens() -> Vec<(String, Sequence)> {
    let mut all = vec![
        ("econ_enable".to_owned(), econ_enable()),
        ("every_element".to_owned(), every_element()),
    ];
    for name in CORPUS {
        let bytes = std::fs::read(corpus_fixture(name)).expect("read fixture");
        all.push(((*name).to_owned(), support::imported(&bytes)));
    }
    all
}

fn emitted(sequence: &Sequence, options: &EmitOptions) -> String {
    String::from_utf8(emit(sequence, options).expect("emit")).expect("UTF-8")
}

#[test]
fn every_golden_matches_its_checked_in_bytes() {
    for (name, sequence) in goldens() {
        let text = emitted(&sequence, &EmitOptions::new());
        compare_or_bless(&fixture(&format!("emit/{name}.cxf.json")), &text);
    }
}

#[test]
fn every_golden_loads_warning_free() {
    for (name, _) in goldens() {
        let text = std::fs::read(fixture(&format!("emit/{name}.cxf.json"))).expect("golden");
        load_clean(&text);
    }
}

#[test]
fn emitting_twice_yields_identical_bytes() {
    for (name, sequence) in goldens() {
        let options = EmitOptions::new();
        assert_eq!(
            emit(&sequence, &options).expect("emit"),
            emit(&sequence.clone(), &options).expect("emit"),
            "{name}"
        );
    }
}

#[test]
fn the_default_namespace_and_an_override_both_load() {
    for namespace in [
        oce_author::DEFAULT_NAMESPACE,
        "https://example.com/plant/ahu1/",
        "urn:site:b2:",
        "http://example.org/library#",
    ] {
        for (name, sequence) in goldens() {
            let text = emitted(&sequence, &EmitOptions::new().with_namespace(namespace));
            assert!(
                text.contains(&format!("\"{namespace}{}\"", sequence.name)),
                "{name} under {namespace}"
            );
            load_clean(text.as_bytes());
        }
    }
}

#[test]
fn the_worked_example_follows_the_notes_node_table() {
    let text = emitted(&econ_enable(), &EmitOptions::new());
    let value: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let nodes: Vec<(String, String)> = value["@graph"]
        .as_array()
        .expect("graph")
        .iter()
        .map(|n| {
            (
                n["@id"]
                    .as_str()
                    .expect("id")
                    .trim_start_matches("urn:oce:author:econ_enable")
                    .to_owned(),
                n["@type"].as_str().expect("type").to_owned(),
            )
        })
        .collect();
    let expected = [
        ("", "S231:Block"),
        (".dTHys", "S231:Parameter"),
        (
            ".sub",
            "http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract",
        ),
        (".sub.u1", "S231:RealInput"),
        (".sub.u2", "S231:RealInput"),
        (".sub.y", "S231:RealOutput"),
        (
            ".hys",
            "http://example.org#Buildings.Controls.OBC.CDL.Reals.Hysteresis",
        ),
        (".hys.uLow", "S231:Parameter"),
        (".hys.uHigh", "S231:Parameter"),
        (".hys.u", "S231:RealInput"),
        (".hys.y", "S231:BooleanOutput"),
        (".TOut", "S231:RealInput"),
        (".TOutCut", "S231:RealInput"),
        (".ecoEna", "S231:BooleanOutput"),
    ];
    let expected: Vec<(String, String)> = expected
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect();
    assert_eq!(nodes, expected);
    // `dTHys` is Real with an Integer literal: written as a Real; `uHigh` is the bare reference.
    assert!(text.contains("\"@value\": \"1.0\""));
    assert!(text.contains("\"S231:value\": \"dTHys\""));
    // `@brick` and `@provisional` are not emitted (decision 2).
    assert!(!text.contains("Enable_Command") && !text.contains("provisional"));
}

#[test]
fn the_writer_is_ascii_with_one_trailing_newline() {
    let mut s = every_element();
    let message = s
        .blocks
        .iter_mut()
        .find(|b| b.name.as_str() == "assMes")
        .and_then(|b| b.assignments.first_mut())
        .expect("assMes.message");
    message.value = oce_author::ParamValue::Literal(oce_author::Literal::Text(
        "Temp\u{e9}rature \u{1f321}".to_owned(),
    ));
    let text = emitted(&s, &EmitOptions::new());
    assert!(text.is_ascii());
    assert!(text.contains("Temp\\u00e9rature \\ud83c\\udf21"), "{text}");
    assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
    assert!(!text.contains('\t'));
}

#[test]
fn text_form_annotations_and_suffixes_never_reach_the_bytes() {
    let plain = emitted(&econ_enable(), &EmitOptions::new());
    let mut annotated = econ_enable();
    annotated.params[0].suffix = Some("K".to_owned());
    annotated.params[0].annotations.comments = vec!["hysteresis band".to_owned()];
    annotated.blocks[1].annotations.blank_line_before = true;
    annotated.blocks[1].assignments[0].suffix = Some("K".to_owned());
    annotated.blocks[1].assignments[0].annotations.comments = vec!["low".to_owned()];
    annotated.connections[0].annotations.comments = vec!["cut-off first".to_owned()];
    annotated.derived_from = Some(
        oce_author::ProvenanceId::new("cxf:fnv1a128:00112233445566778899aabbccddeeff").expect("id"),
    );
    assert_ne!(annotated, econ_enable());
    assert_eq!(emitted(&annotated, &EmitOptions::new()), plain);
}

/// The `@graph` nodes of a document by `@id`.
fn nodes(text: &str) -> std::collections::BTreeMap<String, serde_json::Value> {
    let value: serde_json::Value = serde_json::from_str(text).expect("JSON");
    value["@graph"]
        .as_array()
        .expect("graph")
        .iter()
        .map(|n| (n["@id"].as_str().expect("id").to_owned(), n.clone()))
        .collect()
}

/// The `@id`s of a relation, which the engine's export may write as one object or a list.
fn ids(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Array(items) => items.iter().flat_map(ids).collect(),
        serde_json::Value::Object(map) => map
            .get("@id")
            .and_then(serde_json::Value::as_str)
            .map(|s| vec![s.to_owned()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[test]
fn every_golden_loads_to_the_models_topology() {
    use oce_author::Endpoint;
    for (name, sequence) in goldens() {
        let text =
            std::fs::read_to_string(fixture(&format!("emit/{name}.cxf.json"))).expect("golden");
        let emitted = nodes(&text);
        let engine = load_clean(text.as_bytes());
        let topology = engine.topology();
        let iri =
            |local: &str| format!("{}{}.{local}", oce_author::DEFAULT_NAMESPACE, sequence.name);
        // Blocks, in authored order, with their classes.
        let blocks: Vec<(String, String)> = topology
            .blocks
            .iter()
            .map(|b| (b.instance_path.clone(), b.class_iri.clone()))
            .collect();
        let expected: Vec<(String, String)> = sequence
            .blocks
            .iter()
            .map(|b| (iri(b.name.as_str()), b.class_path.clone()))
            .collect();
        assert_eq!(blocks, expected, "{name}: blocks");
        // Every connection, by the kind of its two ends.
        let mut internal = 0;
        for connection in &sequence.connections {
            let spelled = connection.spelling();
            match (&connection.source, &connection.target) {
                (Endpoint::Port { .. }, Endpoint::Port { .. }) => {
                    internal += 1;
                    let (from, to) = (
                        iri(&connection.source.spelling()),
                        iri(&connection.target.spelling()),
                    );
                    assert!(
                        topology
                            .connections
                            .iter()
                            .any(|c| c.from == from && c.to == to),
                        "{name}: {spelled}"
                    );
                }
                (Endpoint::Boundary(input), Endpoint::Port { block, port }) => {
                    // The block's input at that port's index reads the boundary input.
                    let declared = ids(&emitted[&iri(block.as_str())]["S231:hasInput"]);
                    let index = declared
                        .iter()
                        .position(|p| *p == iri(&format!("{block}.{port}")))
                        .expect("port declared");
                    let loaded = topology
                        .blocks
                        .iter()
                        .find(|b| b.instance_path == iri(block.as_str()))
                        .expect("block");
                    assert_eq!(
                        loaded.inputs[index],
                        iri(input.as_str()),
                        "{name}: {spelled}"
                    );
                }
                (Endpoint::Port { .. }, Endpoint::Boundary(output)) => {
                    assert!(
                        topology
                            .boundary_outputs
                            .iter()
                            .any(|o| o.path == iri(output.as_str())
                                && o.driver_path == iri(&connection.source.spelling())),
                        "{name}: {spelled}"
                    );
                }
                (Endpoint::Boundary(input), Endpoint::Boundary(output)) => {
                    assert!(
                        topology
                            .pass_through
                            .iter()
                            .any(|p| p.input == iri(input.as_str())
                                && p.output == iri(output.as_str())),
                        "{name}: {spelled}"
                    );
                }
                _ => panic!("{name}: unexpected endpoint shape"),
            }
        }
        assert_eq!(
            topology.connections.len(),
            internal,
            "{name}: no extra edges"
        );
        // Boundary types and attributes survive load and export (a partial export may defer a
        // boundary with the blocks it touches; a complete one keeps every boundary).
        let export = engine.export_cxf().expect("export");
        let complete = export.warnings.is_empty();
        let exported = nodes(&String::from_utf8(export.bytes).expect("utf8"));
        for boundary in sequence.inputs.iter().chain(&sequence.outputs) {
            let Some(node) = exported.get(&iri(boundary.name.as_str())) else {
                assert!(
                    !complete,
                    "{name}: {} missing from a complete export",
                    boundary.name
                );
                continue;
            };
            assert_eq!(
                ids(&node["S231:isOfDataType"]),
                vec![format!("S231:{}", boundary.value_type.as_str())],
                "{name}: {}",
                boundary.name
            );
            for (key, value) in [
                ("S231:unit", boundary.semantic.unit.as_deref()),
                ("S231:quantity", boundary.quantity.as_deref()),
                ("S231:displayUnit", boundary.display_unit.as_deref()),
            ] {
                assert_eq!(
                    node.get(key).and_then(|v| v.as_str()),
                    value,
                    "{name}: {} {key}",
                    boundary.name
                );
            }
        }
    }
}
