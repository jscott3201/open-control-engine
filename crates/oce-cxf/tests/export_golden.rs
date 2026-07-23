//! Byte-determinism goldens for the minimal CXF exporter (#141): the checked-in exported bytes,
//! export idempotence, and emission-order stability.
//!
//! The golden is the exact byte output of `export(import(minimal_loop.jsonld))`. To regenerate
//! after an *intentional* format change, run:
//!
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test export_golden exported_bytes_match_the_checked_in_golden
//! ```
//!
//! and review the diff — any unreviewed byte drift is a determinism defect.

use std::path::PathBuf;

use oce_cxf::{ResolveOptions, export, import_cxf};
use oce_model::ModelGraph;

const FIXTURE: &str = include_str!("fixtures/minimal_loop.jsonld");
const GOLDEN_REL: &str = "tests/fixtures/export/minimal_loop.export.cxf.json";

/// The §7.4.1 attr-rich fixture (all three `TermAttr` wire shapes + Real `min`/`max`), used by
/// the R6 attr-bearing byte golden + idempotence test.
const ATTRS_FIXTURE: &str = include_str!("fixtures/connector_attrs.jsonld");
/// The checked-in exported-bytes golden for the attr-bearing fixture (`export(import(...))`).
const ATTRS_GOLDEN_REL: &str = "tests/fixtures/export/connector_attrs.export.cxf.json";

/// The synthesized root `@id` (`ModelGraph` does not record the source root IRI). Pinned here
/// against the exporter's constant: changing it is a byte-format break.
const EXPORT_ROOT_IRI: &str = "urn:open-control:cxf-export:root";

fn import_ok(src: &str) -> ModelGraph {
    let (g, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("minimal_loop must resolve without error");
    assert!(
        report.is_empty(),
        "expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_REL)
}

fn attrs_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ATTRS_GOLDEN_REL)
}

#[test]
fn exported_bytes_match_the_checked_in_golden() {
    let g = import_ok(FIXTURE);
    let bytes = export(&g).expect("minimal_loop is inside the minimal export subset");

    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), &bytes).unwrap();
        return;
    }
    let expected =
        std::fs::read(golden_path()).expect("export golden missing — regenerate with OCE_BLESS=1");
    // Compare as strings so a mismatch prints a readable diff, then re-assert raw bytes.
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        String::from_utf8_lossy(&expected),
        "exported document diverged from the checked-in byte golden"
    );
    assert_eq!(bytes, expected, "byte-level divergence (non-UTF-8?)");
}

#[test]
fn export_is_byte_idempotent_across_calls() {
    let g = import_ok(FIXTURE);
    let first = export(&g).expect("export succeeds");
    for _ in 0..3 {
        assert_eq!(
            export(&g).expect("export succeeds"),
            first,
            "repeated exports of the same graph must be byte-identical"
        );
    }
}

#[test]
fn attr_bearing_exported_bytes_match_the_checked_in_golden() {
    // R6 byte golden for the attr-bearing fixture: the exact bytes of
    // `export(import(connector_attrs.jsonld))`, with the five BSC §7.4.1 attrs on the port node.
    // Regenerate after an intentional format change with:
    //   OCE_BLESS=1 cargo test -p oce-cxf --test export_golden attr_bearing_exported_bytes_match_the_checked_in_golden
    // and review the diff — any unreviewed byte drift is a determinism defect.
    let g = import_ok(ATTRS_FIXTURE);
    let bytes = export(&g).expect("attr-bearing connector exports under BSC");

    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(attrs_golden_path().parent().unwrap()).unwrap();
        std::fs::write(attrs_golden_path(), &bytes).unwrap();
        return;
    }
    let expected = std::fs::read(attrs_golden_path())
        .expect("attr export golden missing — regenerate with OCE_BLESS=1");
    assert_eq!(
        String::from_utf8_lossy(&bytes),
        String::from_utf8_lossy(&expected),
        "attr-bearing exported document diverged from the checked-in byte golden"
    );
    assert_eq!(bytes, expected, "byte-level divergence (non-UTF-8?)");
}

#[test]
fn attr_bearing_export_is_byte_idempotent_across_calls() {
    // R6 idempotence: repeated exports of the same attr-bearing graph are byte-identical (no map
    // iteration feeds emission; the bare shapes are deterministic).
    let g = import_ok(ATTRS_FIXTURE);
    let first = export(&g).expect("attr-bearing connector exports under BSC");
    for _ in 0..3 {
        assert_eq!(
            export(&g).expect("attr-bearing connector exports under BSC"),
            first,
            "repeated exports of the same attr-bearing graph must be byte-identical"
        );
    }
}

#[test]
fn graph_node_order_follows_model_graph_vector_order() {
    // Targeted order-stability assertion: recompute the expected @graph @id sequence from the
    // ModelGraph vectors alone (root, blocks + their params in ParamTable order, one port per
    // connector in ConnectorId order via the owner's port-list position, boundaries in
    // external_inputs first-occurrence order) and require the emitted sequence to match. Any
    // map-iteration order leaking into emission breaks this before it breaks the byte golden.
    let g = import_ok(FIXTURE);
    let bytes = export(&g).expect("export succeeds");

    let mut expected: Vec<String> = vec![EXPORT_ROOT_IRI.to_owned()];
    let mut port_iri: Vec<Option<String>> = vec![None; g.connectors.len()];
    for b in &g.blocks {
        let iri = b
            .instance_iri
            .as_deref()
            .expect("imported blocks have IRIs");
        expected.push(iri.to_owned());
        for (name, _) in &b.params.values {
            expected.push(format!("{iri}.{name}"));
        }
        for (k, cid) in b.inputs.iter().enumerate() {
            port_iri[cid.0 as usize] = Some(format!("{iri}.in{k}"));
        }
        for (k, cid) in b.outputs.iter().enumerate() {
            port_iri[cid.0 as usize] = Some(format!("{iri}.out{k}"));
        }
    }
    for minted in port_iri {
        expected.push(minted.expect("every connector is owned by a port list"));
    }
    for cid in &g.external_inputs {
        let boundary = g.connectors[cid.0 as usize]
            .iri
            .as_deref()
            .expect("external inputs carry their boundary IRI")
            .to_owned();
        if !expected.contains(&boundary) {
            expected.push(boundary);
        }
    }

    let doc: serde_json::Value = serde_json::from_slice(&bytes).expect("export emits JSON");
    let emitted: Vec<&str> = doc["@graph"]
        .as_array()
        .expect("@graph is an array")
        .iter()
        .map(|n| n["@id"].as_str().expect("every node has an @id"))
        .collect();
    assert_eq!(
        emitted, expected,
        "emission order must derive from the ModelGraph vectors alone"
    );
}
