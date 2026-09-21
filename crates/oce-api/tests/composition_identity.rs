//! Public compatibility observations for OCE's executable ordering profile.
//! A content tag identifies export bytes, NOT executable compatibility. Restore is the public
//! executable-identity probe; no private manifest fields or fingerprint byte offsets are used.

#[path = "../../oce-cxf/tests/resolve_ordering/documents.rs"]
mod documents;

use documents::{COMPACT, EXPANDED, node_mut, reverse_object_keys};
use oce_api::oce_store::Store;
use oce_api::{Engine, EngineStateError, OcError};
use serde_json::Value;

fn load(bytes: &[u8]) -> Engine<impl Store + use<>> {
    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(bytes).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    engine
}

fn content(engine: &Engine<impl Store>) -> String {
    engine.export_cxf().unwrap().content_id_complete().unwrap()
}

#[test]
fn normalized_spellings_preserve_content_and_public_restore_compatibility() {
    let baseline = load(COMPACT.as_bytes());
    let snapshot = baseline.state_snapshot().unwrap();
    for source in [COMPACT, EXPANDED] {
        let mut doc: Value = serde_json::from_str(source).unwrap();
        for node in doc["@graph"].as_array_mut().unwrap() {
            for field in [
                "S231:hasInput",
                "S231:hasOutput",
                "S231:hasParameter",
                "S231:hasConstant",
            ] {
                if let Some(array) = node.get_mut(field).and_then(Value::as_array_mut) {
                    array.reverse();
                }
            }
        }
        for bytes in [
            source.as_bytes().to_vec(),
            reverse_object_keys(&doc).into_bytes(),
        ] {
            let mut target = load(&bytes);
            assert_eq!(content(&target), content(&baseline));
            assert_eq!(
                target.state_snapshot().unwrap().as_bytes(),
                snapshot.as_bytes()
            );
            target.restore_state(&snapshot).unwrap();
        }
    }
}

#[test]
fn containment_and_connector_transpositions_change_identity_without_changing_authored_names() {
    let baseline = load(COMPACT.as_bytes());
    let snapshot = baseline.state_snapshot().unwrap();
    for axis in ["containsBlock", "@graph", "positional ports"] {
        let source = if axis == "positional ports" {
            COMPACT.replace(".u1", ".inLeft").replace(".u2", ".inRight")
        } else {
            COMPACT.to_owned()
        };
        let source_engine = load(source.as_bytes());
        let source_snapshot = source_engine.state_snapshot().unwrap();
        let mut doc: Value = serde_json::from_str(&source).unwrap();
        match axis {
            "containsBlock" => node_mut(&mut doc, "m:M")["S231:containsBlock"]
                .as_array_mut()
                .unwrap()
                .reverse(),
            "@graph" => doc["@graph"].as_array_mut().unwrap().swap(5, 10),
            _ => node_mut(&mut doc, "m:M.branch.aSum")["S231:hasInput"]
                .as_array_mut()
                .unwrap()
                .reverse(),
        }
        let mut target = load(&serde_json::to_vec(&doc).unwrap());
        let names = |engine: &Engine<_>| {
            let mut names: Vec<_> = engine
                .point_list(None)
                .unwrap()
                .into_iter()
                .map(|p| p.path)
                .collect();
            names.sort();
            names
        };
        assert_eq!(
            names(&target),
            names(&source_engine),
            "{axis}: authored names survive"
        );
        assert_ne!(content(&target), content(&source_engine), "{axis}");
        let before = target.state_snapshot().unwrap();
        assert_ne!(before.as_bytes(), source_snapshot.as_bytes(), "{axis}");
        assert!(
            matches!(
                target.restore_state(&source_snapshot),
                Err(OcError::State(
                    EngineStateError::IncompatibleExecution { .. }
                ))
            ),
            "{axis}"
        );
        assert_eq!(
            target.state_snapshot().unwrap().as_bytes(),
            before.as_bytes()
        );
        target.restore_state(&before).unwrap();
    }
    assert_eq!(
        baseline.state_snapshot().unwrap().as_bytes(),
        snapshot.as_bytes()
    );
}

#[test]
fn direct_fanout_order_changes_content_but_not_execution_compatibility() {
    let baseline = load(COMPACT.as_bytes());
    let mut doc: Value = serde_json::from_str(COMPACT).unwrap();
    node_mut(&mut doc, "m:M.branch.zSource.y")["S231:isConnectedTo"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let mut target = load(&serde_json::to_vec(&doc).unwrap());
    assert_ne!(content(&target), content(&baseline));
    let snapshot = baseline.state_snapshot().unwrap();
    // The executable manifest canonicalizes the connection edge set; exported arrays do not.
    assert_eq!(
        target.state_snapshot().unwrap().as_bytes(),
        snapshot.as_bytes()
    );
    target.restore_state(&snapshot).unwrap();
}

#[test]
fn member_order_preserves_content_and_execution_compatibility() {
    let source = include_bytes!(
        "../../oce-cxf/tests/fixtures/composite_contract/accepted/synthesized_order_across_owners.jsonld"
    );
    let baseline = load(source);
    let mut doc: Value = serde_json::from_slice(source).unwrap();
    for node in doc["@graph"].as_array_mut().unwrap() {
        if let Some(members) = node
            .get_mut("S231:hasInstance")
            .and_then(Value::as_array_mut)
        {
            members.reverse();
        }
    }
    let mut target = load(reverse_object_keys(&doc).as_bytes());
    assert_eq!(content(&target), content(&baseline));
    let snapshot = baseline.state_snapshot().unwrap();
    assert_eq!(
        target.state_snapshot().unwrap().as_bytes(),
        snapshot.as_bytes()
    );
    target.restore_state(&snapshot).unwrap();
}
