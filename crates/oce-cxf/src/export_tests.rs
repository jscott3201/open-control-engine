//! Tests for the minimal exporter's acceptance and rejection surfaces: an imported
//! `minimal_loop` and `connector_attrs` fixture both export cleanly (the five in-subset §7.4.1
//! attrs emitted under the Bare-Scalar Canonical shape), while everything outside the subset —
//! an empty graph, `nominal`/`unbounded`/non-finite bounds, enum parameters, IRI-less blocks,
//! non-finite Real parameters — is a typed `ExportUnsupported` rejection (subject = the owning
//! block's `instance_iri`), never a panic, and identical across repeated calls.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic, Severity, has_errors};
use oce_model::{
    Attrs, BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, RealAttrs,
    Value, ValueType,
};

use super::{CxfError, ResolveOptions, export, export_with_report, import_cxf};

const MINIMAL_LOOP: &str = include_str!("../tests/fixtures/minimal_loop.jsonld");
const CONNECTOR_ATTRS: &str = include_str!("../tests/fixtures/connector_attrs.jsonld");
const G36_ENUM_PARAM: &str = include_str!("../tests/fixtures/g36/cooling_only_dampers.jsonld");

fn import(src: &str) -> ModelGraph {
    let (graph, _report) =
        import_cxf(src.as_bytes(), &ResolveOptions::default()).expect("fixture resolves");
    graph
}

/// Unwrap a rejection: `export` must return `Err(CxfError::Validation(_))` with a non-empty
/// diagnostic list where every entry is an `ExportUnsupported` error. Panics (failing the
/// calling test) on `Ok` or any other error shape.
fn rejection(model: &ModelGraph) -> Vec<Diagnostic> {
    match export(model) {
        Err(CxfError::Validation(diags)) => {
            assert!(!diags.is_empty(), "a rejection must carry diagnostics");
            for d in &diags {
                assert_eq!(
                    d.code,
                    DiagCode::ExportUnsupported,
                    "unexpected code: {d:?}"
                );
                assert_eq!(d.severity, Severity::Error, "unexpected severity: {d:?}");
            }
            diags
        }
        Ok(bytes) => panic!(
            "expected an export rejection, but got Ok with {} byte(s)",
            bytes.len()
        ),
        Err(other) => panic!("expected CxfError::Validation, got {other:?}"),
    }
}

/// A hand-built one-block graph: `Constant` (registered, arity 0/1) with the given params and a
/// single Real output connector — the smallest graph inside the export subset.
fn constant_graph(params: Vec<(Arc<str>, Value)>) -> ModelGraph {
    ModelGraph {
        blocks: vec![BlockInstance {
            id: BlockId(0),
            class_iri: Arc::from("CDL.Reals.Sources.Constant"),
            inputs: vec![],
            outputs: vec![ConnectorId(0)],
            params: ParamTable { values: params },
            decl_order: 0,
            instance_iri: Some(Arc::from("http://example.org#Hand.con")),
        }],
        connectors: vec![Connector::new(
            ConnectorId(0),
            BlockId(0),
            Dir::Out,
            ValueType::Real,
            0,
        )],
        connections: vec![],
        external_inputs: vec![],
    }
}

#[test]
fn resolved_import_now_exports_cleanly() {
    // R4's staged floor rejected this exact graph; the exporter accepts it.
    let bytes = export(&import(MINIMAL_LOOP)).expect("minimal_loop is inside the export subset");
    assert!(!bytes.is_empty());
}

#[test]
fn empty_model_graph_is_rejected_without_a_subject() {
    // Pinned decision: a zero-block ModelGraph rejects. A root with no containsBlock is not a
    // runtime composite — a root-only document re-imports as MalformedDocument (zero candidate
    // roots), so there is no warning-free document to emit.
    let diags = rejection(&ModelGraph::new());
    assert_eq!(diags.len(), 1);
    assert_eq!(
        diags[0].subject, None,
        "a whole-operation rejection must not blame any node"
    );
    assert_eq!(
        diags[0].message,
        "CXF export requires at least one block: an empty ModelGraph has no runtime composite \
         to emit"
    );
}

#[test]
fn declared_connector_attrs_export_cleanly_under_the_bare_scalar_canonical_shape() {
    // R6: the five in-subset §7.4.1 attrs on authored output `A.con.y` are EMITTED, not
    // rejected. The full RT-2 render fixpoint (`render(import(export(G))) == render(G)`, all five
    // BSC `RealAttrs` fields) lives in `tests/export_roundtrip.rs` — `render()` is not reachable
    // from this `src/` unit-test module. Here we assert the unit-level acceptance: the export is
    // `Ok` and the emitted port carries the bare `S231:unit` key (string-contains on the bytes).
    let graph = import(CONNECTOR_ATTRS);
    let bytes = export(&graph).expect("attr-bearing connector exports under BSC");
    let text = std::str::from_utf8(&bytes).expect("export emits UTF-8 JSON");
    assert!(
        text.contains(r#""S231:unit":"K""#),
        "the bare-string unit must be emitted on the port node, got: {text}"
    );
    // Imported ports retain their authored identity through export.
    assert!(
        text.contains(r#""@id":"http://example.org#A.con.y""#),
        "the attr-bearing port must retain its authored @id, got: {text}"
    );
}

#[test]
fn attr_bearing_export_is_byte_stable_across_repeated_calls() {
    // R6 repurpose: `CONNECTOR_ATTRS` no longer rejects — it exports. Repeated exports of the
    // same graph must be byte-identical (determinism), the acceptance analog of the old
    // rejection-stability test.
    let graph = import(CONNECTOR_ATTRS);
    let first = export(&graph).expect("attr-bearing connector exports under BSC");
    for _ in 0..3 {
        assert_eq!(
            export(&graph).expect("attr-bearing connector exports under BSC"),
            first,
            "repeated exports of the same attr-bearing graph must be byte-identical"
        );
    }
}

#[test]
fn enum_valued_parameter_defers_with_the_owning_block_subject_and_cascades() {
    // R7: cooling_only_dampers carries `controllerType = Enum` on the conPID BlockInstance (the
    // high-limits/EnergyStandard fixtures do NOT — their enums are specialization-consumed and
    // never reach a BlockInstance param). Under deferral the export is `Ok`, the bytes re-import
    // enum-free with zero error diags, and the report carries ≥1 `ExportDeferred` warning. This
    // is BOTH the retirement of the pre-R7 enum-param rejection AND the cascade smoke test —
    // folded, not duplicated. The deferred set is pinned EXACTLY, in both directions: a G36-scale
    // graph is precisely where an over-reaching cascade could delete most of the document while a
    // "≥1 warning" assertion still passed, and where a cascade that stopped short could emit a
    // block whose only driver the document omits. The cone here is two blocks — the enum-bearing
    // conPID, and swi2, whose u3 input conPID.y solely drives.
    let graph = import(G36_ENUM_PARAM);
    let report = export_with_report(&graph).expect("enum-carrying graph defers, not rejects");
    assert!(
        !report.bytes.is_empty(),
        "the enum-free survivor subset still emits bytes"
    );
    // T1: Ok (above). T3: the deferred set is exactly {conPID, swi2}, every warning a deferral.
    assert!(
        report
            .warnings
            .iter()
            .all(|d| d.code == DiagCode::ExportDeferred && d.severity == Severity::Warning),
        "every warning is a non-aborting deferral (no errors): {:?}",
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
            "http://example.org#g36.source.cooling_only_dampers.conPID",
            "http://example.org#g36.source.cooling_only_dampers.swi2",
        ],
        "the cascade must reach exactly conPID and its sole-driven consumer swi2"
    );
    // T2: the emitted bytes re-import to an enum-free graph with zero error diags.
    let (reimported, re_report) =
        import_cxf(&report.bytes, &ResolveOptions::default()).expect("emitted bytes re-import");
    assert!(
        !has_errors(&re_report.diagnostics),
        "re-import has no error diags (no SingleAssignment/UnresolvedReference): {:?}",
        re_report.diagnostics
    );
    assert!(
        reimported
            .connectors
            .iter()
            .all(|c| !matches!(c.value_type, ValueType::Enum(_))),
        "zero enum connectors in the re-imported graph"
    );
    assert!(
        reimported.blocks.iter().all(|b| b
            .params
            .values
            .iter()
            .all(|(_, v)| !matches!(v, Value::Enum { .. }))),
        "zero enum params in the re-imported graph"
    );
    // The survivor set is the input's blocks minus exactly those two, named individually — the
    // structural counterpart to the deferred-set pin above. Asserted against the RE-IMPORTED graph
    // so it also covers the emission path, not just the pre-pass.
    fn survivors(g: &ModelGraph, deferred: &[&str]) -> Vec<String> {
        let mut iris: Vec<String> = g
            .blocks
            .iter()
            .filter_map(|b| b.instance_iri.as_deref())
            .filter(|iri| !deferred.contains(iri))
            .map(str::to_owned)
            .collect();
        iris.sort();
        iris
    }
    assert_eq!(
        survivors(&reimported, &deferred),
        survivors(&graph, &deferred),
        "every non-deferred block survives to the re-imported graph, and nothing else appears"
    );
    assert_eq!(
        reimported.blocks.len(),
        graph.blocks.len() - deferred.len(),
        "exactly the deferred blocks are missing"
    );
    // T4: the enum-bearing conPID emits NO node (block or port) in the @graph.
    let text = std::str::from_utf8(&report.bytes).expect("export emits UTF-8 JSON");
    assert!(
        !text.contains("\"@id\":\"http://example.org#g36.source.cooling_only_dampers.conPID\""),
        "conPID's block node must be absent from the emitted @graph: {text}"
    );
    assert!(
        !text.contains("\"http://example.org#g36.source.cooling_only_dampers.conPID."),
        "no minted port @id under conPID may appear in the emitted @graph: {text}"
    );
}

#[test]
fn block_without_an_instance_iri_is_rejected() {
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].instance_iri = None;
    let diags = rejection(&graph);
    assert_eq!(diags[0].subject.as_deref(), Some("block#0"));
    assert_eq!(
        diags[0].message,
        "export subset: block has no instance_iri to name its CXF node"
    );
}

#[test]
fn non_finite_real_parameters_are_rejected() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        // serde_json serializes a non-finite f64 as `null`, which no CxfValue arm re-parses —
        // emitting it would poison the whole document. Reject instead.
        let graph = constant_graph(vec![(Arc::from("k"), Value::Real(bad))]);
        let diags = rejection(&graph);
        assert_eq!(
            diags[0].subject.as_deref(),
            Some("http://example.org#Hand.con")
        );
        assert_eq!(
            diags[0].message,
            "export subset: parameter `k` is a non-finite Real, which cannot round-trip through \
             JSON"
        );
    }
}

#[test]
fn dotted_parameter_name_is_rejected() {
    // Re-import recovers the parameter name as the segment after the last `.`; a dotted name
    // would silently come back renamed.
    let graph = constant_graph(vec![(Arc::from("k.nested"), Value::Real(2.0))]);
    let diags = rejection(&graph);
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: parameter name `k.nested` is not a bare member name (re-import recovers \
         the name after the last `.`)"
    );
}

#[test]
fn string_parameter_round_trips_through_export_and_reimport() {
    // Strings are in-subset: emitted as a quoted CDL string-literal expression (escaping `\` and
    // `"`), re-grounded by oce-expr to the bit-identical Value::String.
    let tricky = r#"deg "C" \ path"#;
    let graph = constant_graph(vec![
        (Arc::from("k"), Value::Real(2.0)),
        (Arc::from("note"), Value::String(Arc::from(tricky))),
    ]);
    let bytes = export(&graph).expect("String params are inside the subset");
    let (reimported, report) =
        import_cxf(&bytes, &ResolveOptions::default()).expect("exported doc re-imports");
    assert!(
        report.is_empty(),
        "clean reimport: {:?}",
        report.diagnostics
    );
    let note = reimported.blocks[0]
        .params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == "note")
        .expect("note parameter survives");
    assert!(
        note.1.bit_eq(&Value::String(Arc::from(tricky))),
        "got {:?}",
        note.1
    );
}

#[test]
fn mismatched_port_wiring_is_rejected_not_exported_shifted() {
    // The block lists its Out connector under `inputs`: structurally inconsistent — exporting it
    // would emit wiring the importer rebuilds differently. Two diagnostics: the failed claim
    // (wrong direction) and the orphan scan (the connector ends up in no port list).
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].inputs = vec![ConnectorId(0)];
    graph.blocks[0].outputs = vec![];
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 2, "got: {diags:?}");
    for d in &diags {
        assert_eq!(d.subject.as_deref(), Some("http://example.org#Hand.con"));
        assert_eq!(
            d.message,
            "export subset: block/connector wiring is structurally inconsistent"
        );
    }
}

/// A hand-built one-block graph around `Abs` (one In, one Out connector) — the smallest shape
/// with an input, for the external-input and connection-direction rejection tests.
fn abs_graph() -> ModelGraph {
    ModelGraph {
        blocks: vec![BlockInstance {
            id: BlockId(0),
            class_iri: Arc::from("CDL.Reals.Abs"),
            inputs: vec![ConnectorId(0)],
            outputs: vec![ConnectorId(1)],
            params: ParamTable::default(),
            decl_order: 0,
            instance_iri: Some(Arc::from("http://example.org#Hand.abs")),
        }],
        connectors: vec![
            Connector::new(ConnectorId(0), BlockId(0), Dir::In, ValueType::Real, 0),
            Connector::new(ConnectorId(1), BlockId(0), Dir::Out, ValueType::Real, 1),
        ],
        connections: vec![],
        external_inputs: vec![],
    }
}

#[test]
fn external_input_without_a_boundary_iri_is_rejected() {
    // The resolver always stores the elided boundary IRI on the driven child input; a hand-built
    // graph without one cannot rebuild the root's hasInput.
    let mut graph = abs_graph();
    graph.external_inputs = vec![ConnectorId(0)];
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.abs")
    );
    assert_eq!(
        diags[0].message,
        "export subset: external input carries no boundary IRI to rebuild the root hasInput"
    );
}

#[test]
fn hand_built_attr_bearing_connector_exports_under_the_bare_scalar_canonical_shape() {
    // R6 rewrite: a Real connector with `unit: Some("K")` is a BSC EMIT field, not a rejection.
    // Proven on the builder path (mirrors the imported fixture): the export is `Ok` and the
    // emitted port carries the bare `S231:unit` "K" key (assert on bytes — no `render()` needed).
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0] = Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0)
        .with_attrs(Attrs::Real(RealAttrs {
            unit: Some(Arc::from("K")),
            ..RealAttrs::default()
        }))
        .expect("Real attrs on a Real connector");
    let bytes = export(&graph).expect("a unit-bearing Real connector exports under BSC");
    let text = std::str::from_utf8(&bytes).expect("export emits UTF-8 JSON");
    assert!(
        text.contains(r#""S231:unit":"K""#),
        "the bare-string unit must be emitted on the port node, got: {text}"
    );
}

/// The owning block `instance_iri` for the hand-built `constant_graph` (the subject of every
/// per-connector rejection in these unit tests — connectors carry no IRI of their own).
const HAND_CON_SUBJECT: &str = "http://example.org#Hand.con";

#[test]
fn nominal_attr_on_real_connector_is_rejected_with_the_owning_block_subject() {
    // Removal-mutation pin: delete the `a.nominal.is_some()` check in `classify_attrs` → the
    // graph is accepted (nominal is silently dropped) → `rejection()` panics on `Ok`. The
    // importer hardcodes `nominal: None`, so any `Some` is outside the canonical subset. A Real
    // OUTPUT connector so the subject is the OWNING BLOCK's `instance_iri`.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0] = Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0)
        .with_attrs(Attrs::Real(RealAttrs {
            nominal: Some(1.0),
            ..RealAttrs::default()
        }))
        .expect("Real attrs on a Real connector");
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "exactly one offender: {diags:?}");
    assert_eq!(diags[0].code, DiagCode::ExportUnsupported);
    assert_eq!(diags[0].subject.as_deref(), Some(HAND_CON_SUBJECT));
    assert_eq!(
        diags[0].message,
        "export subset: connector carries a non-default §7.4.1 nominal attribute, \
         which is outside the canonical (bare-scalar) export subset"
    );
}

#[test]
fn unbounded_attr_on_real_connector_is_rejected_with_the_owning_block_subject() {
    // Removal-mutation pin: delete the `a.unbounded.is_some()` check in `classify_attrs` → the
    // graph is accepted → `rejection()` panics on `Ok`. The importer hardcodes
    // `unbounded: None`, so any `Some` is outside the canonical subset.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0] = Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0)
        .with_attrs(Attrs::Real(RealAttrs {
            unbounded: Some(true),
            ..RealAttrs::default()
        }))
        .expect("Real attrs on a Real connector");
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "exactly one offender: {diags:?}");
    assert_eq!(diags[0].code, DiagCode::ExportUnsupported);
    assert_eq!(diags[0].subject.as_deref(), Some(HAND_CON_SUBJECT));
    assert_eq!(
        diags[0].message,
        "export subset: connector carries a non-default §7.4.1 unbounded attribute, \
         which is outside the canonical (bare-scalar) export subset"
    );
}

#[test]
fn non_finite_real_bound_is_rejected_with_the_owning_block_subject() {
    // Removal-mutation pin (R6-C): delete the `is_finite()` guard in `finite_real_bound` →
    // `CxfValue::Float(NaN)` emits as JSON `null` → the export is `Ok` → `rejection()` panics on
    // `Ok`. Mirrors `non_finite_real_parameters_are_rejected`. Each of the 6 cases sets exactly
    // ONE field non-finite (`min` XOR `max`) and the other `None`, so each yields exactly one
    // `MSG_ATTR_NONFINITE_BOUND` diag — `assert_eq!(diags.len(), 1)` per case (NOT both fields
    // non-finite at once, which would yield 2 diags).
    let check = |attrs: RealAttrs, label: &str, bad: f64| {
        let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
        graph.connectors[0] =
            Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::Real, 0)
                .with_attrs(Attrs::Real(attrs))
                .expect("Real attrs on a Real connector");
        let diags = rejection(&graph);
        assert_eq!(
            diags.len(),
            1,
            "{label} (bad={bad}): exactly one non-finite-bound diag, got: {diags:?}"
        );
        assert_eq!(diags[0].code, DiagCode::ExportUnsupported);
        assert_eq!(diags[0].subject.as_deref(), Some(HAND_CON_SUBJECT));
        assert_eq!(
            diags[0].message,
            "export subset: connector carries a non-finite §7.4.1 min/max bound, \
             which is outside the canonical (bare-scalar) export subset"
        );
    };
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        check(
            RealAttrs {
                min: Some(bad),
                max: None,
                ..RealAttrs::default()
            },
            "min non-finite, max None",
            bad,
        );
        check(
            RealAttrs {
                min: None,
                max: Some(bad),
                ..RealAttrs::default()
            },
            "max non-finite, min None",
            bad,
        );
    }
}

#[test]
fn out_of_range_external_input_is_rejected_without_a_subject() {
    // Removal-mutation pin (residual slice a): delete the diag push in the `let Some(c) = … else`
    // arm of the `external_inputs` loop (keep the `let-else`) → the out-of-range entry silently
    // `continue`s with no diag → `diags` is empty → the export is `Ok` → `rejection()` panics on
    // `Ok`. Subjectless: there is no connector (index 99 is out of range), so no owner to name.
    let mut graph = abs_graph();
    graph.external_inputs = vec![ConnectorId(99)];
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "exactly one subjectless diag: {diags:?}");
    assert_eq!(diags[0].code, DiagCode::ExportUnsupported);
    assert_eq!(
        diags[0].subject, None,
        "no connector → no owner → subjectless"
    );
    assert_eq!(
        diags[0].message,
        "export subset: block/connector wiring is structurally inconsistent"
    );
}

#[test]
fn root_urn_collision_is_rejected_as_a_duplicate_id() {
    // Removal-mutation pin (residual slice b): delete the `seen.insert(EXPORT_ROOT_IRI.to_owned())`
    // pre-seed in `plan()` → the colliding block's first `claim_emitted_id` insert SUCCEEDS → no
    // `MSG_DUPLICATE_ID` → the export is `Ok` → `rejection()` panics on `Ok`. The subject is the
    // colliding `instance_iri` itself (the root URN). Arms the existing `MSG_DUPLICATE_ID` gate
    // against the pre-seed's removal.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].instance_iri = Some(Arc::from("urn:open-control:cxf-export:root"));
    let diags = rejection(&graph);
    // The colliding block node emits under the root URN; `claim_emitted_id` then fires on the
    // pre-seeded entry. (The minted port id `urn:open-control:cxf-export:root.out0` is distinct,
    // so only the block node collides.)
    assert_eq!(diags.len(), 1, "exactly one duplicate-id diag: {diags:?}");
    assert_eq!(diags[0].code, DiagCode::ExportUnsupported);
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("urn:open-control:cxf-export:root")
    );
    assert_eq!(
        diags[0].message,
        "export subset: emitted node @id collides with another emitted node @id"
    );
}

#[test]
fn class_path_with_a_hash_is_rejected_by_the_bridge_oracle() {
    // A `#` in the class path shifts the fragment `class_path_of` extracts on re-import
    // ("Constant" here, not the full path) — a silent identity flip, caught by running the real
    // bridge over the @type before emission.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].class_iri = Arc::from("CDL.Reals.Sources#Constant");
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: class path does not survive the class-IRI bridge round-trip"
    );
}

#[test]
fn obc_prefixed_class_path_is_rejected_before_emission() {
    // An already-OBC-prefixed class path PASSES the bridge round-trip (the bridge strips exactly
    // the one prefix the exporter adds), but registry keys never carry the prefix, so the bytes
    // would always fail re-import with ClassNotFound. Rejected explicitly instead.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].class_iri = Arc::from("Buildings.Controls.OBC.CDL.Reals.Sources.Constant");
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: class path does not survive the class-IRI bridge round-trip"
    );
}

#[test]
fn string_typed_connector_is_rejected_with_the_owning_block_subject() {
    // Silent acceptance would emit the `S231:Real` placeholder as the connector's datatype — an
    // exported document that lies about the signal type.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0] =
        Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::String, 0);
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: String connectors are not permitted in CXF (§7.8)"
    );
}

#[test]
fn enum_typed_connector_defers_with_the_owning_block_subject() {
    // R7: a `ValueType::Enum` connector is deferred (warning), not rejected (error). Hand-built
    // for isolation, but this arm is import-reachable too: the resolver's
    // `value_type_of_datatype` maps enum datatype IRIs to `ValueType::Enum`. The export is `Ok`
    // and the report carries exactly one `ExportDeferred` warning naming the owning block.
    //
    // The graph carries a SECOND, enum-free block. Without it every block would be deferred, and
    // total deferral is a rejection, not an `Ok` with an empty composite — the shape this test
    // used to assert `Ok` on, regress-locking that defect. The lone-block variant is asserted
    // below; the deferral-cascade behaviour itself lives in `tests/export_deferral.rs`.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0] = Connector::new(
        ConnectorId(0),
        BlockId(0),
        Dir::Out,
        ValueType::Enum(oce_model::EnumClassId::SIMPLE_CONTROLLER),
        0,
    );
    let lone = graph.clone();
    graph.blocks.push(BlockInstance {
        id: BlockId(1),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![ConnectorId(1)],
        params: ParamTable::default(),
        decl_order: 1,
        instance_iri: Some(Arc::from("http://example.org#Hand.keep")),
    });
    graph.connectors.push(Connector::new(
        ConnectorId(1),
        BlockId(1),
        Dir::Out,
        ValueType::Real,
        1,
    ));

    let report = export_with_report(&graph).expect("enum connector defers, not rejects");
    let defers: Vec<&Diagnostic> = report
        .warnings
        .iter()
        .filter(|d| d.code == DiagCode::ExportDeferred)
        .collect();
    assert_eq!(
        defers.len(),
        1,
        "exactly one deferral warning: {:?}",
        report.warnings
    );
    assert_eq!(defers[0].severity, Severity::Warning, "non-aborting");
    assert_eq!(
        defers[0].subject.as_deref(),
        Some("http://example.org#Hand.con"),
        "the owning block is the subject"
    );
    // The enum connector's port node is absent from the emitted @graph (T4-style pin).
    let text = std::str::from_utf8(&report.bytes).expect("export emits UTF-8 JSON");
    assert!(
        !text.contains("\"http://example.org#Hand.con.out0\""),
        "the enum connector's port node must be absent: {text}"
    );

    // The corrected expectation for the lone-block shape: deferring the only block leaves no
    // runtime composite, so the export rejects instead of returning a root-only document.
    let diags = match export(&lone) {
        Err(CxfError::Validation(diags)) => diags,
        Ok(bytes) => panic!(
            "total deferral must reject, got Ok with {} byte(s)",
            bytes.len()
        ),
        Err(other) => panic!("expected CxfError::Validation, got {other:?}"),
    };
    assert!(
        diags.iter().any(|d| d.severity == Severity::Error
            && d.code == DiagCode::ExportUnsupported
            && d.subject.is_none()
            && d.message
                .starts_with("CXF export requires at least one emitted runtime block:")),
        "total deferral must carry the no-runtime-block error: {diags:?}"
    );
}

#[test]
fn non_dense_ids_are_rejected_wholesale_without_a_subject() {
    // Every position-based index in plan() (and re-import itself) assumes BlockId.0 /
    // ConnectorId.0 equal vector position; a non-dense graph is rejected whole rather than
    // exported with shifted identities. Whole-graph defect, so no subject.
    let expect_wholesale = |graph: &ModelGraph| {
        let diags = rejection(graph);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
        assert_eq!(diags[0].subject, None);
        assert_eq!(
            diags[0].message,
            "export subset: block/connector wiring is structurally inconsistent"
        );
    };
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks[0].id = BlockId(5);
    expect_wholesale(&graph);

    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.connectors[0].id = ConnectorId(7);
    expect_wholesale(&graph);
}

#[test]
fn backwards_connection_is_rejected_with_the_source_owner_subject() {
    // An in-range connection that is not output→input: emitting it would put isConnectedTo on
    // an input port and the importer would rebuild different wiring. The subject is the source
    // endpoint's owning block.
    let mut graph = abs_graph();
    graph.connections = vec![oce_model::Connection {
        from: ConnectorId(0), // In — backwards
        to: ConnectorId(1),   // Out
    }];
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.abs")
    );
    assert_eq!(
        diags[0].message,
        "export subset: block/connector wiring is structurally inconsistent"
    );
}

#[test]
fn duplicate_block_instance_iris_are_rejected() {
    // Two blocks sharing one instance_iri would emit two @graph nodes with one @id (plus
    // colliding minted port ids) — re-import would fail DuplicateId. Rejected at plan time:
    // one diagnostic for the block-node collision, one for the colliding minted port.
    let mut graph = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    graph.blocks.push(BlockInstance {
        id: BlockId(1),
        class_iri: Arc::from("CDL.Reals.Sources.Constant"),
        inputs: vec![],
        outputs: vec![ConnectorId(1)],
        params: ParamTable::default(),
        decl_order: 1,
        instance_iri: Some(Arc::from("http://example.org#Hand.con")), // same @id as block 0
    });
    graph.connectors.push(Connector::new(
        ConnectorId(1),
        BlockId(1),
        Dir::Out,
        ValueType::Real,
        1,
    ));
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 2, "got: {diags:?}");
    for d in &diags {
        assert_eq!(d.subject.as_deref(), Some("http://example.org#Hand.con"));
        assert_eq!(
            d.message,
            "export subset: emitted node @id collides with another emitted node @id"
        );
    }
}

#[test]
fn parameter_named_like_a_minted_port_id_is_rejected() {
    // A parameter literally named `out0` mints the same @id as the block's first minted output
    // port — the emitted document would carry two nodes with one @id. Rejected at plan time.
    let graph = constant_graph(vec![(Arc::from("out0"), Value::Real(1.0))]);
    let diags = rejection(&graph);
    assert_eq!(diags.len(), 1, "got: {diags:?}");
    assert_eq!(
        diags[0].subject.as_deref(),
        Some("http://example.org#Hand.con")
    );
    assert_eq!(
        diags[0].message,
        "export subset: emitted node @id collides with another emitted node @id"
    );
}

#[test]
fn every_rejection_path_returns_instead_of_panicking() {
    // The never-panics property from the R4 floor survives on every path: every shape returns
    // either `Ok` (acceptance) or `Err(Validation(_))` (rejection) — never a panic, never
    // another error shape. After R6 the acceptance shapes are named explicitly and split out of
    // the rejection loop, and the rejection loop asserts each graph actually REJECTS (not
    // `Ok|Err(Validation)`), so the blanket test can no longer mask a silent-acceptance
    // regression like the OOR deletion (slice a).
    let with_class = |class: &str| {
        let mut g = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
        g.blocks[0].class_iri = Arc::from(class);
        g
    };
    let mut non_dense = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
    non_dense.blocks[0].id = BlockId(5);
    let mut backwards = abs_graph();
    backwards.connections = vec![oce_model::Connection {
        from: ConnectorId(0),
        to: ConnectorId(1),
    }];
    let mut no_boundary_iri = abs_graph();
    no_boundary_iri.external_inputs = vec![ConnectorId(0)];
    let mut out_of_range_external = abs_graph();
    out_of_range_external.external_inputs = vec![ConnectorId(9)];

    // Acceptance shapes (after R6 these export cleanly — emit, not reject). Named explicitly so
    // a future regression that silently accepts a rejection shape cannot hide among them.
    let acceptance: Vec<ModelGraph> = vec![import(MINIMAL_LOOP), import(CONNECTOR_ATTRS)];
    for g in &acceptance {
        match export(g) {
            Ok(_) => {}
            Err(CxfError::Validation(diags)) => panic!(
                "expected an acceptance shape to export, but it rejected: {:?}",
                diags
            ),
            Err(other) => panic!("unexpected error shape for an acceptance graph: {other:?}"),
        }
    }

    // Rejection shapes: every one must return `Err(Validation(_))` (not `Ok`). The OOR external
    // input (slice a) is in this list — a silent `continue` deletion would make it `Ok`, caught
    // here, where the old `Ok|Err(Validation)` blanket would have masked it. Enum-carrying graphs
    // are here only in their TOTALLY-deferred form: after R7 a partly enum-bearing graph defers
    // (Ok with warnings), but a graph the cascade empties has no composite to emit and rejects.
    // The enum-param entry restores the coverage the retired enum-param rejection graph carried —
    // that exact shape is what surfaced the root-only-document defect.
    let rejections: Vec<ModelGraph> = vec![
        ModelGraph::new(),
        constant_graph(vec![(Arc::from("k"), Value::Real(f64::NAN))]),
        constant_graph(vec![(Arc::from("out0"), Value::Real(1.0))]),
        constant_graph(vec![(
            Arc::from("controllerType"),
            Value::Enum {
                class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 1,
            },
        )]),
        with_class("CDL.Reals.Sources#Constant"),
        with_class("Buildings.Controls.OBC.CDL.Reals.Sources.Constant"),
        {
            let mut g = constant_graph(vec![(Arc::from("k"), Value::Real(2.0))]);
            g.connectors[0] =
                Connector::new(ConnectorId(0), BlockId(0), Dir::Out, ValueType::String, 0);
            g
        },
        non_dense,
        backwards,
        no_boundary_iri,
        out_of_range_external,
    ];
    for g in &rejections {
        match export(g) {
            Err(CxfError::Validation(_)) => {}
            Ok(_) => panic!("a rejection shape was silently accepted (no diag) — {g:?}"),
            Err(other) => panic!("unexpected error shape for a rejection graph: {other:?}"),
        }
    }
}
