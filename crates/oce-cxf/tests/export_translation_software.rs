//! Every exported document names the software that wrote it, and no import reads that back
//! (GitHub #527).
//!
//! The OpenBuildingControl (OBC) specification makes the label optional (OBC §8.4), and
//! modelica-json's `CXF-Core.jsonld` declares its two properties, `S231:translationSoftware` and
//! `S231:translationSoftwareVersion`, with domain `S231:Block` and range `S231:String`. The
//! exporter writes both on the root node. The importer models neither: whatever a document says
//! there, in any shape, the imported model is the same. Expected values are stated by hand.

use oce_cxf::{CxfError, ResolveOptions, export, import_cxf, parse_document};
use oce_diag::DiagCode;
use oce_model::ModelGraph;
use serde_json::{Map, Value as Json, json};

const MINIMAL_LOOP: &str = include_str!("fixtures/minimal_loop.jsonld");
const SOFTWARE_KEY: &str = "S231:translationSoftware";
const VERSION_KEY: &str = "S231:translationSoftwareVersion";
const SOFTWARE: &str = "open-control-engine";
/// The workspace version: this test crate and the exporter share it.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const ROOT: &str = "http://example.org#MinLoop";
const LEAF_BLOCK: &str = "http://example.org#MinLoop.con";
const PORT: &str = "http://example.org#MinLoop.con.y";

fn import(bytes: &[u8]) -> ModelGraph {
    import_cxf(bytes, &ResolveOptions::default())
        .map(|(graph, _)| graph)
        .unwrap_or_else(|error| panic!("expected a clean import, got {error:?}"))
}

/// The complete import outcome, rendered: the model and every diagnostic.
fn outcome(bytes: &[u8]) -> String {
    match import_cxf(bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => format!("ok {graph:?}\n{:?}", report.diagnostics),
        Err(error) => format!("err {error:?}"),
    }
}

fn fixture() -> Json {
    serde_json::from_str(MINIMAL_LOOP).expect("fixture is JSON")
}

fn node_mut<'a>(doc: &'a mut Json, id: &str) -> &'a mut Map<String, Json> {
    doc["@graph"]
        .as_array_mut()
        .expect("@graph array")
        .iter_mut()
        .find(|entry| entry["@id"] == id)
        .and_then(Json::as_object_mut)
        .expect("node present")
}

fn bytes(doc: &Json) -> Vec<u8> {
    serde_json::to_vec(doc).expect("serialize doc")
}

#[test]
fn the_root_of_every_export_names_this_engine_and_the_workspace_version() {
    for (name, source) in [
        ("minimal_loop", MINIMAL_LOOP),
        (
            "pass_through_miniature",
            include_str!("fixtures/pass_through_miniature.jsonld"),
        ),
        (
            "enum_deferral_miniature",
            include_str!("fixtures/enum_deferral_miniature.jsonld"),
        ),
        (
            "declared_input_attrs",
            include_str!("fixtures/declared_input_attrs.jsonld"),
        ),
        (
            "vav_single_zone",
            include_str!("fixtures/g36/vav_single_zone.jsonld"),
        ),
    ] {
        let exported = export(&import(source.as_bytes())).expect("exports");
        let document: Json = serde_json::from_slice(&exported).expect("export is JSON");
        let graph = document["@graph"].as_array().expect("@graph");
        assert_eq!(
            graph[0]["@id"], "urn:open-control:cxf-export:root",
            "{name}"
        );
        assert_eq!(graph[0][SOFTWARE_KEY], json!(SOFTWARE), "{name}");
        assert_eq!(graph[0][VERSION_KEY], json!(VERSION), "{name}");
        // Exactly once each, on the root and nowhere else.
        let text = String::from_utf8(exported).expect("UTF-8");
        assert_eq!(text.matches("translationSoftware\"").count(), 1, "{name}");
        assert_eq!(
            text.matches("translationSoftwareVersion\"").count(),
            1,
            "{name}"
        );
        for node in &graph[1..] {
            let node = node.as_object().expect("node object");
            assert!(!node.contains_key(SOFTWARE_KEY) && !node.contains_key(VERSION_KEY));
        }
    }
}

#[test]
fn the_label_is_the_last_thing_on_the_root_and_the_export_is_a_fixpoint() {
    let first = export(&import(MINIMAL_LOOP.as_bytes())).expect("exports");
    let text = String::from_utf8(first.clone()).expect("UTF-8");
    let root_end = format!(
        "\"S231:translationSoftware\":\"{SOFTWARE}\",\"S231:translationSoftwareVersion\":\"{VERSION}\"}},"
    );
    assert!(text.contains(&root_end), "{text}");
    // Re-importing the labelled export and exporting again reproduces it byte for byte: the
    // label is written, never accumulated or echoed.
    let second = export(&import(&first)).expect("re-exports");
    assert_eq!(first, second);
    assert_eq!(
        format!("{:?}", import(&first)),
        format!("{:?}", import(&second))
    );
}

#[test]
fn a_label_on_import_changes_nothing_whatever_it_says() {
    let baseline = outcome(MINIMAL_LOOP.as_bytes());
    assert!(baseline.starts_with("ok "), "{baseline}");
    let huge = "x".repeat(1 << 16);
    let cases: Vec<(&str, Json, Json)> = vec![
        ("this engine's", json!(SOFTWARE), json!(VERSION)),
        ("another tool's", json!("modelica-json"), json!("2.1.0")),
        ("an older release's", json!(SOFTWARE), json!("0.0.1")),
        ("empty strings", json!(""), json!("")),
        ("non-ASCII", json!("Übersetzer \u{1F600}"), json!("β")),
        ("a long string", json!(huge), json!("1")),
        ("numbers", json!(7), json!(1.5)),
        ("null", Json::Null, Json::Null),
        ("booleans", json!(true), json!(false)),
        ("arrays", json!(["a", "b"]), json!([])),
        (
            "typed literals",
            json!({ "@value": "tool", "@type": "http://www.w3.org/2001/XMLSchema#string" }),
            json!({ "@value": "1" }),
        ),
        (
            "references",
            json!({ "@id": "http://example.org#MinLoop.con" }),
            json!({ "@id": "urn:absent" }),
        ),
    ];
    for (name, software, version) in &cases {
        // On the root, where the exporter writes it.
        let mut doc = fixture();
        let root = node_mut(&mut doc, ROOT);
        root.insert(SOFTWARE_KEY.into(), software.clone());
        root.insert(VERSION_KEY.into(), version.clone());
        assert_eq!(outcome(&bytes(&doc)), baseline, "{name} on the root");
        // On a leaf block and on a connector: still not a property the importer reads.
        for id in [LEAF_BLOCK, PORT] {
            let mut doc = fixture();
            let node = node_mut(&mut doc, id);
            node.insert(SOFTWARE_KEY.into(), software.clone());
            node.insert(VERSION_KEY.into(), version.clone());
            assert_eq!(outcome(&bytes(&doc)), baseline, "{name} on {id}");
        }
    }
    // One member without the other, and each non-compact spelling.
    for key in [
        SOFTWARE_KEY.to_owned(),
        VERSION_KEY.to_owned(),
        "http://data.ashrae.org/S231#translationSoftware".to_owned(),
        "http://data.ashrae.org/S231#translationSoftwareVersion".to_owned(),
        "cdl:translationSoftware".to_owned(),
        "translationSoftware".to_owned(),
    ] {
        let mut doc = fixture();
        doc["@context"]["cdl"] = json!("http://data.ashrae.org/S231#");
        node_mut(&mut doc, ROOT).insert(key.clone(), json!("some tool"));
        assert_eq!(outcome(&bytes(&doc)), baseline, "{key}");
    }
}

#[test]
fn an_imported_label_is_replaced_by_this_engines_on_export() {
    let mut doc = fixture();
    let root = node_mut(&mut doc, ROOT);
    root.insert(SOFTWARE_KEY.into(), json!("modelica-json"));
    root.insert(VERSION_KEY.into(), json!("2.1.0"));
    let labelled = export(&import(&bytes(&doc))).expect("exports");
    assert_eq!(
        labelled,
        export(&import(MINIMAL_LOOP.as_bytes())).expect("exports")
    );
    let text = String::from_utf8(labelled).expect("UTF-8");
    assert!(!text.contains("modelica-json") && !text.contains("2.1.0"));
}

#[test]
fn the_layer_a_document_keeps_a_foreign_label_verbatim() {
    // RT-1 is about the lossless DTO, not the model: parse then write keeps what was authored.
    let mut doc = fixture();
    let root = node_mut(&mut doc, ROOT);
    root.insert(SOFTWARE_KEY.into(), json!("modelica-json"));
    root.insert(VERSION_KEY.into(), json!(["2", 1]));
    let parsed = parse_document(&bytes(&doc)).expect("parses");
    let root = parsed
        .graph
        .iter()
        .find(|node| node.id == ROOT)
        .expect("root");
    assert_eq!(root.other[SOFTWARE_KEY], json!("modelica-json"));
    assert_eq!(root.other[VERSION_KEY], json!(["2", 1]));
}

#[test]
fn a_label_written_twice_on_one_node_is_refused_like_any_repeated_key() {
    // The label is exempt from interpretation, not from the one-key-once rule: JSON parsing
    // would keep one of the two values silently.
    let text = MINIMAL_LOOP.replacen(
        "\"S231:label\": \"MinLoop\",",
        "\"S231:label\": \"MinLoop\", \"S231:translationSoftware\": \"a\", \
         \"S231:translationSoftware\": \"b\",",
        1,
    );
    assert_ne!(text, MINIMAL_LOOP);
    match import_cxf(text.as_bytes(), &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => {
            assert_eq!(diags.len(), 1, "{diags:?}");
            assert_eq!(diags[0].code, DiagCode::MalformedDocument);
            assert_eq!(diags[0].subject.as_deref(), Some(ROOT));
            assert_eq!(
                diags[0].message,
                format!(
                    "node `{ROOT}` writes the key `S231:translationSoftware` 2 times; a key may \
                     appear once per node, and a repeat would silently keep only one of the values"
                )
            );
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}
