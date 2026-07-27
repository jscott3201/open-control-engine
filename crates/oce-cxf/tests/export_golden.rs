//! Byte goldens for the CXF exporter (#141): checked-in expected output files compared
//! byte-for-byte, plus export idempotence and emission-order stability.
//!
//! These are the only export tests that can catch a regression which is *stable across the round
//! trip*. The RT-2 fixpoint assertions elsewhere (`render(G1) == render(G2)`,
//! `export(G2) == bytes`) are self-consistency checks: a change that alters the emitted bytes but
//! alters them identically on both sides of the trip satisfies every one of them. Only an
//! expectation recorded outside the implementation — a file in the repo — pins the actual wire
//! format. `TESTING.md` pillar 2 requires that, which is why each fixture below carries a golden
//! rather than only a fixpoint.
//!
//! Each golden covers a distinct emission axis:
//!
//! | golden                                   | pins                                              |
//! |------------------------------------------|---------------------------------------------------|
//! | `minimal_loop.export.cxf.json`           | the base wire format: root, blocks, params, ports, boundary node |
//! | `connector_attrs.export.cxf.json`        | the five §7.4.1 attrs in Bare-Scalar Canonical shape |
//! | `array_flattened.export.cxf.json`        | 1-D flattened array params (`k_1`, `k_2`)         |
//! | `array2d_flattened.export.cxf.json`      | 2-D flattened array params (multi-underscore `k_1_1`) |
//! | `g36_vav_single_zone.export.cxf.json`    | an enum-free G36-scale topology at full node count |
//! | `enum_deferral_miniature.export.cxf.json`| the deferral survivor cone — what is emitted *and* what is absent |
//!
//! To regenerate a golden after an **intentional** format change:
//!
//! ```text
//! OCE_BLESS=1 cargo test -p oce-cxf --test export_golden
//! ```
//!
//! then review the diff. Blessing is opt-in and never happens on a default run — a test that
//! recomputed its own expectation would assert nothing. Any unreviewed byte drift is a
//! determinism defect.

use std::path::PathBuf;

use oce_cxf::{ResolveOptions, export, export_with_report, import_cxf};
use oce_diag::DiagCode;
use oce_model::ModelGraph;

const FIXTURE: &str = include_str!("fixtures/minimal_loop.jsonld");
/// The §7.4.1 attr-rich fixture (all three `TermAttr` wire shapes + Real `min`/`max`).
const ATTRS_FIXTURE: &str = include_str!("fixtures/connector_attrs.jsonld");
/// 1-D and 2-D flattened array fixtures — the flattened `local_name` emission path.
const ARRAY_FIXTURE: &str = include_str!("fixtures/array_flattened.jsonld");
const ARRAY_2D_FIXTURE: &str = include_str!("fixtures/array2d_flattened.jsonld");
/// An enum-free G36 fixture: the largest in-subset topology, exported whole (nothing defers).
const G36_FIXTURE: &str = include_str!("fixtures/g36/vav_single_zone.jsonld");
/// The enum-deferral miniature — exported down to its survivor cone.
const DEFERRAL_FIXTURE: &str = include_str!("fixtures/enum_deferral_miniature.jsonld");

/// The synthesized root `@id` (`ModelGraph` does not record the source root IRI). Pinned here
/// against the exporter's constant: changing it is a byte-format break.
const EXPORT_ROOT_IRI: &str = "urn:open-control:cxf-export:root";

fn import_ok(src: &str) -> ModelGraph {
    let (g, report) = import_cxf(src.as_bytes(), &ResolveOptions::default())
        .expect("fixture must resolve without error");
    assert!(
        report.is_empty(),
        "expected zero diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

fn golden_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/export")
        .join(rel)
}

/// Compare `bytes` against the checked-in golden `rel`, or rewrite it when `OCE_BLESS` is set.
/// The comparison runs twice — once as text so a mismatch prints a readable diff, once raw so a
/// non-UTF-8 divergence cannot slip through the lossy conversion.
fn assert_golden(bytes: &[u8], rel: &str) {
    let path = golden_path(rel);
    if std::env::var_os("OCE_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().expect("the golden dir has a parent"))
            .expect("golden dir is creatable");
        std::fs::write(&path, bytes).expect("golden is writable");
        return;
    }
    let expected = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("golden {rel} missing ({e}) — regenerate with OCE_BLESS=1"));
    assert_eq!(
        String::from_utf8_lossy(bytes),
        String::from_utf8_lossy(&expected),
        "{rel}: exported document diverged from the checked-in byte golden"
    );
    assert_eq!(bytes, expected, "{rel}: byte-level divergence (non-UTF-8?)");
}

/// Export `src` and require repeated exports of the same graph to be byte-identical. Returns the
/// bytes so the caller can golden them.
fn export_stable(src: &str) -> Vec<u8> {
    let g = import_ok(src);
    let first = export(&g).expect("fixture is inside the export subset");
    for _ in 0..3 {
        assert_eq!(
            export(&g).expect("fixture is inside the export subset"),
            first,
            "repeated exports of the same graph must be byte-identical"
        );
    }
    first
}

#[test]
fn minimal_loop_bytes_match_the_checked_in_golden() {
    assert_golden(&export_stable(FIXTURE), "minimal_loop.export.cxf.json");
}

#[test]
fn attr_bearing_bytes_match_the_checked_in_golden() {
    // The five BSC §7.4.1 attrs on the port node, recorded on the wire rather than inferred from a
    // round trip: a change to how `unit`/`quantity`/`displayUnit`/`min`/`max` serialize survives
    // every fixpoint assertion (both sides move together) but breaks this file.
    assert_golden(
        &export_stable(ATTRS_FIXTURE),
        "connector_attrs.export.cxf.json",
    );
}

#[test]
fn flattened_array_bytes_match_the_checked_in_golden() {
    // The 1-D flattened parameter names (`k_1`, `k_2`) as emitted `@id` suffixes.
    assert_golden(
        &export_stable(ARRAY_FIXTURE),
        "array_flattened.export.cxf.json",
    );
}

#[test]
fn flattened_2d_array_bytes_match_the_checked_in_golden() {
    // The 2-D case is a separate golden because multi-underscore names (`k_1_1` .. `k_2_2`) are a
    // distinct emission path from the single-underscore 1-D ones; the 1-D golden does not cover it.
    assert_golden(
        &export_stable(ARRAY_2D_FIXTURE),
        "array2d_flattened.export.cxf.json",
    );
}

#[test]
fn enum_free_g36_topology_bytes_match_the_checked_in_golden() {
    // A full G36 controller, enum-free, so nothing defers and the whole graph is emitted. At this
    // node count a regression in ordering, minting, or edge emission has room to be *consistently*
    // wrong — which is exactly what a round-trip fixpoint cannot see and this file can.
    assert_golden(
        &export_stable(G36_FIXTURE),
        "g36_vav_single_zone.export.cxf.json",
    );
}

#[test]
fn deferral_survivor_cone_bytes_match_the_checked_in_golden() {
    // The deferral golden records what the survivor cone actually looks like on the wire — and,
    // by omission, what the cascade removed. A cascade that grew or shrank changes this file even
    // when every survivor-cone fixpoint assertion still holds, because those compare the survivors
    // to themselves.
    let g = import_ok(DEFERRAL_FIXTURE);
    let report = export_with_report(&g).expect("the miniature defers, it does not reject");
    assert!(
        report
            .warnings
            .iter()
            .all(|d| d.code == DiagCode::ExportDeferred),
        "every warning is a deferral: {:?}",
        report.warnings
    );
    let mut deferred: Vec<&str> = report
        .warnings
        .iter()
        .filter_map(|d| d.subject.as_deref())
        .collect();
    deferred.sort_unstable();
    deferred.dedup();
    assert_eq!(
        deferred,
        vec![
            "http://example.org#Mini.con",
            "http://example.org#Mini.cons"
        ],
        "the golden below is only meaningful for this exact deferred set"
    );
    assert_golden(&report.bytes, "enum_deferral_miniature.export.cxf.json");
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
