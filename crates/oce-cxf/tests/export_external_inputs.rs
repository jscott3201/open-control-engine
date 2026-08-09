//! The `external_inputs` export subset: which boundary entries survive, which reject, and the
//! fan-out shape that must keep working either way.
//!
//! One boundary IRI legitimately drives several distinct child inputs — that is fan-out, and it
//! groups onto a single boundary node. Listing the *same* connector twice looks similar and is
//! not: the repeat only re-pushes an `isConnectedTo` target the node already carries, and the
//! importer deduplicates it back to one `external_inputs` entry. That combination — export `Ok`,
//! bytes short one entry on the way back — is the failure this file exists to keep out.
//!
//! Graphs are hand-built. The resolver dedupes `external_inputs` on the way in
//! (`resolve/mod.rs`'s `external_inputs.contains` guard), so no imported graph can carry a
//! duplicate and no fixture can exercise the rejection.

use std::sync::Arc;

use oce_cxf::{CxfError, ResolveOptions, export_with_report, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity, has_errors};
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, EnumClassId, ModelGraph,
    ParamTable, Value, ValueType,
};

/// IRI prefix for every hand-built block here.
const PREFIX: &str = "http://example.org#Bound.";

/// The exact rejection a repeated `external_inputs` entry earns.
const MSG_DUPLICATE_EXTERNAL_INPUT: &str = "export subset: a connector is listed more than once \
     in external_inputs, and re-import deduplicates the repeat away";
/// The exact rejection for a fan-out boundary whose driven connector types disagree.
const MSG_BOUNDARY_TYPE_MISMATCH: &str =
    "export subset: one boundary input drives child inputs with different value types";

fn iri(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// A block instance owning the given connector positions.
fn block(
    idx: u32,
    class: &str,
    name: &str,
    inputs: &[u32],
    outputs: &[u32],
    params: Vec<(Arc<str>, Value)>,
) -> BlockInstance {
    BlockInstance {
        id: BlockId(idx),
        class_iri: Arc::from(class),
        inputs: inputs.iter().copied().map(ConnectorId).collect(),
        outputs: outputs.iter().copied().map(ConnectorId).collect(),
        params: ParamTable { values: params },
        decl_order: idx,
        instance_iri: Some(Arc::from(iri(name).as_str())),
    }
}

/// A connector at position `idx` owned by block `owner`.
fn conn(idx: u32, owner: u32, dir: Dir, value_type: ValueType) -> Connector {
    Connector::new(ConnectorId(idx), BlockId(owner), dir, value_type, 0)
}

/// A single Real parameter — in-subset payload for a survivor block.
fn real_param(k: f64) -> Vec<(Arc<str>, Value)> {
    vec![(Arc::from("k"), Value::Real(k))]
}

/// An enum-valued parameter — the payload that puts a block in the deferred set on its own.
fn enum_param() -> Vec<(Arc<str>, Value)> {
    vec![(
        Arc::from("controllerType"),
        Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 1,
        },
    )]
}

/// An output→input connection between two raw connector indices.
fn wire(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

/// Export `g`, requiring rejection, and return the diagnostics.
fn export_rejection(g: &ModelGraph) -> Vec<Diagnostic> {
    match export_with_report(g) {
        Err(CxfError::Validation(diags)) => {
            assert!(
                has_errors(&diags),
                "a rejection must carry at least one error-severity diagnostic: {diags:?}"
            );
            diags
        }
        Ok(report) => panic!(
            "expected a rejection, got Ok with {} byte(s) and {} warning(s)",
            report.bytes.len(),
            report.warnings.len()
        ),
        Err(other) => panic!("unexpected error shape: {other:?}"),
    }
}

/// Re-import the emitted bytes, requiring zero error diagnostics, and hand back the graph.
fn reimport_clean(bytes: &[u8]) -> ModelGraph {
    let (g, report) =
        import_cxf(bytes, &ResolveOptions::default()).expect("emitted bytes must re-import");
    assert!(
        !has_errors(&report.diagnostics),
        "re-import must produce zero error diagnostics, got: {:?}",
        report.diagnostics
    );
    g
}

#[test]
fn a_connector_listed_twice_in_external_inputs_is_rejected() {
    // `[ConnectorId(1), ConnectorId(1)]`. Before this rejection existed the export returned `Ok`
    // and the boundary node carried the same `isConnectedTo` target twice; re-import collapsed it
    // to one entry, so the survivor cone did not come back bit-identical and nothing said so.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "src",
                &[],
                &[0],
                real_param(1.0),
            ),
            block(1, "CDL.Reals.Abs", "sink", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real).with_iri("http://example.org#Bound.uSet"),
            conn(2, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(1), ConnectorId(1)],
        boundary_outputs: vec![],
    };

    let diags = export_rejection(&g);
    let errors: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "exactly one rejection: {diags:?}");
    assert_eq!(errors[0].code, DiagCode::ExportUnsupported);
    assert_eq!(errors[0].message, MSG_DUPLICATE_EXTERNAL_INPUT);
    assert_eq!(
        errors[0].subject.as_deref(),
        Some(iri("sink").as_str()),
        "the rejection names the block owning the repeated connector"
    );
}

#[test]
fn one_boundary_driving_several_distinct_inputs_still_exports() {
    // The shape the rejection must not catch. `uSet` drives two DIFFERENT child inputs, which is
    // fan-out: both entries group onto one boundary node, and re-import rebuilds both. A
    // duplicate check keyed on the boundary IRI rather than the connector would break this.
    let g = ModelGraph {
        blocks: vec![
            block(0, "CDL.Reals.Abs", "left", &[0], &[1], vec![]),
            block(1, "CDL.Reals.Abs", "right", &[2], &[3], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real).with_iri("http://example.org#Bound.uSet"),
            conn(1, 0, Dir::Out, ValueType::Real),
            conn(2, 1, Dir::In, ValueType::Real).with_iri("http://example.org#Bound.uSet"),
            conn(3, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(2)],
        boundary_outputs: vec![],
    };

    let report = export_with_report(&g).expect("fan-out is inside the subset");
    let g2 = reimport_clean(&report.bytes);
    assert_eq!(
        g2.external_inputs.len(),
        2,
        "both distinct boundary-fed inputs must come back: {:?}",
        g2.external_inputs
    );
}

#[test]
fn one_boundary_driving_different_value_types_is_rejected() {
    let g = ModelGraph {
        blocks: vec![
            block(0, "CDL.Reals.Abs", "left", &[0], &[1], vec![]),
            block(1, "CDL.Reals.Abs", "right", &[2], &[3], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real).with_iri("http://example.org#Bound.uSet"),
            conn(1, 0, Dir::Out, ValueType::Real),
            conn(2, 1, Dir::In, ValueType::Boolean).with_iri("http://example.org#Bound.uSet"),
            conn(3, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0), ConnectorId(2)],
        boundary_outputs: vec![],
    };

    let diags = export_rejection(&g);
    let errors: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "exactly one rejection: {diags:?}");
    assert_eq!(errors[0].code, DiagCode::ExportUnsupported);
    assert_eq!(errors[0].message, MSG_BOUNDARY_TYPE_MISMATCH);
    assert_eq!(errors[0].subject.as_deref(), Some(iri("right").as_str()));
}

#[test]
fn a_duplicate_external_input_on_a_cascade_deferred_block_does_not_abort_the_export() {
    // The half of the survivor scoping the locally-enum-bearing case cannot see. `add` carries no
    // enum of its own — it is deferred only because `enumsrc`, its sole driver on `in0`, was. Its
    // *other* input is the one listed twice.
    //
    // The distinction is load-bearing because Phase 6 skips on `deferred.contains`, not on "this
    // block carries an enum". Narrow that skip to locally enum-bearing blocks and every other
    // test in the crate still passes — a review did exactly that — while this graph wrongly
    // rejects with `ExportUnsupported` instead of exporting `keep` with two deferral warnings.
    // A cascade-deferred block is as absent from the document as the block that doomed it, so
    // its boundary entries cannot reach the wire either.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "enumsrc",
                &[],
                &[0],
                enum_param(),
            ),
            block(1, "CDL.Reals.Add", "add", &[1, 2], &[3], vec![]),
            block(
                2,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[4],
                real_param(3.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(2, 1, Dir::In, ValueType::Real).with_iri("http://example.org#Bound.uSet"),
            conn(3, 1, Dir::Out, ValueType::Real),
            conn(4, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 1)],
        external_inputs: vec![ConnectorId(2), ConnectorId(2)],
        boundary_outputs: vec![],
    };

    let report =
        export_with_report(&g).expect("a cascade-deferred block's duplicate must not abort");
    let subjects: Vec<&str> = report
        .warnings
        .iter()
        .map(|d| {
            assert_eq!(d.code, DiagCode::ExportDeferred, "no rejection may appear");
            assert_eq!(d.severity, Severity::Warning);
            d.subject.as_deref().expect("a deferral names its block")
        })
        .collect();
    assert_eq!(
        subjects,
        vec![iri("enumsrc"), iri("add")],
        "the enum source defers, then the cascade reaches `add`"
    );

    let doc: serde_json::Value = serde_json::from_slice(&report.bytes).expect("export emits JSON");
    let ids: Vec<String> = doc["@graph"]
        .as_array()
        .expect("@graph is an array")
        .iter()
        .filter_map(|n| n["@id"].as_str().map(String::from))
        .collect();
    assert!(
        ids.contains(&iri("keep")),
        "the survivor must be emitted, got: {ids:?}"
    );
    for gone in [iri("enumsrc"), iri("add")] {
        assert!(
            !ids.iter()
                .any(|id| *id == gone || id.starts_with(&format!("{gone}."))),
            "no node belonging to the deferred block `{gone}` may be emitted, got: {ids:?}"
        );
    }
    let g2 = reimport_clean(&report.bytes);
    assert!(
        g2.external_inputs.is_empty(),
        "the cascade-deferred block's boundary entries are dropped entirely: {:?}",
        g2.external_inputs
    );
}

#[test]
fn a_duplicate_external_input_on_an_enum_bearing_deferred_block_does_not_abort_the_export() {
    // The rejection is scoped to survivors. `sink` is deferred on its enum input, so its boundary
    // entries describe a port the document never contains — repeated or not, they are dropped in
    // Phase 6 before the duplicate check can see them. Rejecting here would sink an export whose
    // only defect sits in bytes nobody writes, which is the rule every other deferred-block arm
    // already follows.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[0],
                real_param(2.0),
            ),
            block(1, "CDL.Reals.Abs", "sink", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(
                1,
                1,
                Dir::In,
                ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
            )
            .with_iri("http://example.org#Bound.uSet"),
            conn(2, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(1), ConnectorId(1)],
        boundary_outputs: vec![],
    };

    let report = export_with_report(&g).expect("a deferred block's duplicate must not abort");
    assert_eq!(
        report.warnings.len(),
        1,
        "just the deferral warning: {:?}",
        report.warnings
    );
    assert_eq!(report.warnings[0].code, DiagCode::ExportDeferred);
    assert_eq!(
        report.warnings[0].subject.as_deref(),
        Some(iri("sink").as_str())
    );
    let g2 = reimport_clean(&report.bytes);
    assert!(
        g2.external_inputs.is_empty(),
        "the deferred block's boundary entries are dropped entirely: {:?}",
        g2.external_inputs
    );
}

#[test]
fn a_single_external_input_entry_round_trips_unchanged() {
    // The positive control. Without it the rejection above could pass by rejecting every
    // `external_inputs` graph, and the two negatives would look just as green.
    let g = ModelGraph {
        blocks: vec![block(0, "CDL.Reals.Abs", "sink", &[0], &[1], vec![])],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real).with_iri("http://example.org#Bound.uSet"),
            conn(1, 0, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(0)],
        boundary_outputs: vec![],
    };

    let report = export_with_report(&g).expect("a lone boundary entry is inside the subset");
    assert!(report.warnings.is_empty(), "nothing defers here");
    let g2 = reimport_clean(&report.bytes);
    assert_eq!(g2.external_inputs.len(), 1);
    assert_eq!(
        g2.connectors[g2.external_inputs[0].0 as usize]
            .iri
            .as_deref(),
        Some("http://example.org#Bound.uSet"),
        "the boundary IRI is restored on the child connector"
    );
}
