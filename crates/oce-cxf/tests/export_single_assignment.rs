//! §7.10 single assignment on the export side: an input connector driven by more than one
//! **surviving** output is rejected rather than emitted.
//!
//! Why this rejection exists at all, given that `oce-validate` and the resolver both check the
//! same law: the exporter's contract is RT-2, and a multiply-driven input breaks it in the worst
//! available way. Every driver's `isConnectedTo` target reaches the document, so `export` returns
//! `Ok` with bytes that fail re-import outright with `DiagCode::SingleAssignment` — not bytes that
//! come back lossy, bytes that do not come back. The exporter already refuses the milder version
//! of that (a connector listed twice in `external_inputs`, which re-imports one entry short), so
//! tolerating the worse one was never a defensible line.
//!
//! Reachability, which is why this is a contract hole and not a plant hazard: the resolver's
//! single-assignment pre-check rejects in-degree ≥ 2 and withholds the graph, so no *imported*
//! graph can carry one. Every graph in this file is therefore hand-built, which is exactly the
//! surface a direct embedder can reach through the public `&ModelGraph` API.
//!
//! These tests live in their own file rather than in `src/export_tests.rs` because that module is
//! within a dozen lines of the 700-LOC cap.
//!
//! **The counted edge set is the survivor cone, not `g.connections`.** That distinction is what
//! most of this file tests. A driver on a deferred block is not in the document, so it must not
//! count; a multiply-driven input on a deferred block is not in the document either, so it must
//! not reject. Counting raw `g.connections` would get both wrong and would sink legitimate
//! exports.

use std::sync::Arc;

use oce_cxf::{CxfError, ExportReport, ResolveOptions, export_with_report, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity, has_errors};
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, EnumClassId, ModelGraph,
    ParamTable, Value, ValueType,
};

/// The rejection text, written out literally rather than imported. `MSG_MULTIPLY_DRIVEN` is
/// private to `src/export.rs`, and a golden that re-derived it from the constant would pass
/// against any edit to the constant — including one that made the message describe the wrong
/// defect. This is the same discipline the deferral-diagnostic goldens use.
const MSG_MULTIPLY_DRIVEN: &str = "export subset: input connector is driven by more than one surviving output, and the emitted \
     document fails re-import with a single-assignment error (§7.10)";

/// IRI prefix for every hand-built block here, so a diagnostic subject reads back as a name.
const PREFIX: &str = "http://example.org#SingleAssign.";

fn iri(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// A block instance owning the given connector positions, keeping ids dense.
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

fn conn(idx: u32, owner: u32, dir: Dir, value_type: ValueType) -> Connector {
    Connector::new(ConnectorId(idx), BlockId(owner), dir, value_type, 0)
}

fn wire(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

/// A Real source block. Every surviving block in this file must be registry-valid, because every
/// accepted export here is re-imported.
fn source(idx: u32, name: &str, out: u32, k: f64) -> BlockInstance {
    block(
        idx,
        "CDL.Reals.Sources.Constant",
        name,
        &[],
        &[out],
        vec![(Arc::from("k"), Value::Real(k))],
    )
}

/// An enum-valued parameter — the payload that puts a block in the deferred set.
fn enum_param() -> Vec<(Arc<str>, Value)> {
    vec![(
        Arc::from("controllerType"),
        Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 1,
        },
    )]
}

/// Export `g`, requiring rejection, and return the diagnostics. Tolerates a mixed list: a
/// rejection on a partly-deferred graph carries its `ExportDeferred` warnings alongside the error.
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

/// Export `g`, requiring acceptance, surfacing the diagnostics on failure rather than `unwrap`.
fn export_ok(g: &ModelGraph) -> ExportReport {
    match export_with_report(g) {
        Ok(report) => report,
        Err(CxfError::Validation(diags)) => {
            panic!("expected this graph to export, but it rejected: {diags:?}")
        }
        Err(other) => panic!("unexpected error shape: {other:?}"),
    }
}

/// The multiply-driven rejections only, as `(subject, message)` in the order they were emitted.
/// Filtering by message rather than by code keeps an unrelated `ExportUnsupported` from being
/// counted as one of these — the count assertions below would otherwise be satisfiable by the
/// wrong diagnostic entirely.
fn multiply_driven(diags: &[Diagnostic]) -> Vec<(String, String)> {
    diags
        .iter()
        .filter(|d| d.message == MSG_MULTIPLY_DRIVEN)
        .map(|d| {
            assert_eq!(d.code, DiagCode::ExportUnsupported, "rejection code: {d:?}");
            assert_eq!(d.severity, Severity::Error, "a rejection aborts: {d:?}");
            (
                d.subject
                    .as_deref()
                    .expect("a multiply-driven rejection always names the target's owner")
                    .to_owned(),
                d.message.clone(),
            )
        })
        .collect()
}

/// Re-import the emitted bytes and require zero error diagnostics. Bytes coming back is not the
/// property — a document that re-imports with a `SingleAssignment` error is a broken export that
/// merely parsed, which is precisely the outcome this whole file exists to make unreachable.
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

// ─────────────────────────────────────────────────────────────────────────
// Rejection — the emitted document would not load.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn two_surviving_drivers_on_one_input_are_rejected() {
    // The base case. Both sources survive, both edges reach the document, and the emitted port
    // node would carry two `isConnectedTo` entries pointing at `sink.in0`.
    let g = ModelGraph {
        blocks: vec![
            source(0, "left", 0, 1.0),
            source(1, "right", 1, 2.0),
            block(2, "CDL.Reals.Abs", "sink", &[2], &[3], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
            conn(2, 2, Dir::In, ValueType::Real),
            conn(3, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 2), wire(1, 2)],
        external_inputs: vec![],
    };

    assert_eq!(
        multiply_driven(&export_rejection(&g)),
        vec![(iri("sink"), MSG_MULTIPLY_DRIVEN.to_owned())],
        "the TARGET's owning block is named, not either driver's"
    );
}

#[test]
fn the_same_edge_listed_twice_is_rejected() {
    // In-degree is counted over connection ENTRIES, not over distinct sources. One duplicated
    // `Connection` emits an `isConnectedTo` array carrying the same `@id` twice, which re-import
    // counts as in-degree 2 exactly as it counts two distinct drivers. A check written against
    // "how many distinct blocks drive this" would pass this graph and ship unloadable bytes.
    let g = ModelGraph {
        blocks: vec![
            source(0, "src", 0, 1.0),
            block(1, "CDL.Reals.Abs", "sink", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(2, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 1), wire(0, 1)],
        external_inputs: vec![],
    };

    assert_eq!(
        multiply_driven(&export_rejection(&g)),
        vec![(iri("sink"), MSG_MULTIPLY_DRIVEN.to_owned())],
    );
}

#[test]
fn a_triply_driven_input_yields_exactly_one_diagnostic() {
    // One diagnostic per offending CONNECTOR, not per excess edge. A reject pushed at the counting
    // site would emit two entries here and three for a quadruple driver, turning one defect into a
    // diagnostic list whose length is a function of how badly it was broken.
    let g = ModelGraph {
        blocks: vec![
            source(0, "a", 0, 1.0),
            source(1, "b", 1, 2.0),
            source(2, "c", 2, 3.0),
            block(3, "CDL.Reals.Abs", "sink", &[3], &[4], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
            conn(2, 2, Dir::Out, ValueType::Real),
            conn(3, 3, Dir::In, ValueType::Real),
            conn(4, 3, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 3), wire(1, 3), wire(2, 3)],
        external_inputs: vec![],
    };

    assert_eq!(
        multiply_driven(&export_rejection(&g)).len(),
        1,
        "three drivers are one defect on one connector"
    );
}

#[test]
fn offenders_are_reported_in_connector_position_order() {
    // Diagnostic determinism (TESTING.md pillar 4): the scan walks `g.connectors` in position
    // order, so the list order is a property of the graph and not of hash iteration. `late`'s
    // input sits at connector 5 and `early`'s at 4, and the blocks are declared in the opposite
    // order, so a scan driven by block order rather than connector order reverses this.
    let g = ModelGraph {
        blocks: vec![
            source(0, "a", 0, 1.0),
            source(1, "b", 1, 2.0),
            block(2, "CDL.Reals.Abs", "late", &[5], &[7], vec![]),
            block(3, "CDL.Reals.Abs", "early", &[4], &[6], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
            conn(2, 0, Dir::Out, ValueType::Real),
            conn(3, 1, Dir::Out, ValueType::Real),
            conn(4, 3, Dir::In, ValueType::Real),
            conn(5, 2, Dir::In, ValueType::Real),
            conn(6, 3, Dir::Out, ValueType::Real),
            conn(7, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 4), wire(1, 4), wire(2, 5), wire(3, 5)],
        external_inputs: vec![],
    };

    let subjects: Vec<String> = multiply_driven(&export_rejection(&g))
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    assert_eq!(
        subjects,
        vec![iri("early"), iri("late")],
        "connector 4 (`early`) precedes connector 5 (`late`)"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Survivor scoping — the count describes the emitted document, not the input.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_deferred_driver_does_not_count_toward_a_survivors_in_degree() {
    // Source-side scoping. `mix` has two drivers in the input graph but only one in the document,
    // because `enumsrc` is deferred and its edge never reaches the wire. Post-filter in-degree is
    // 1, so this is a legitimate export — counting raw `g.connections` would reject a graph whose
    // emitted bytes are single-assignment and re-import cleanly.
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
            source(1, "livesrc", 1, 6.0),
            block(2, "CDL.Reals.Abs", "mix", &[2], &[3], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
            conn(2, 2, Dir::In, ValueType::Real),
            conn(3, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 2), wire(1, 2)],
        external_inputs: vec![],
    };

    let report = export_ok(&g);
    let g2 = reimport_clean(&report.bytes);
    assert_eq!(
        g2.connections.len(),
        1,
        "exactly the live edge survives into the document"
    );
}

#[test]
fn a_multiply_driven_input_on_a_deferred_block_does_not_abort_the_export() {
    // Target-side scoping, and the mirror of the rule the deferral phases already follow: an
    // omitted block contributes no error diagnostic of its own. `enumsink` is doubly driven, but
    // it is deferred, so neither edge reaches the document and the defect is not in the emitted
    // bytes to complain about. Rejecting here would sink an export whose document is clean.
    let g = ModelGraph {
        blocks: vec![
            source(0, "left", 0, 1.0),
            source(1, "right", 1, 2.0),
            block(2, "CDL.Reals.Abs", "enumsink", &[2], &[3], enum_param()),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
            conn(2, 2, Dir::In, ValueType::Real),
            conn(3, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 2), wire(1, 2)],
        external_inputs: vec![],
    };

    let report = export_ok(&g);
    assert_eq!(
        report.warnings.len(),
        1,
        "the enum block defers: {:?}",
        report.warnings
    );
    let g2 = reimport_clean(&report.bytes);
    assert_eq!(
        g2.connections.len(),
        0,
        "both edges are dropped with their deferred target"
    );
}

#[test]
fn a_boundary_input_that_is_also_driven_by_one_output_is_accepted() {
    // The false-reject guard. `fed.in0` is both an `external_inputs` entry and the target of one
    // wire. Re-import elides the boundary node into `external_inputs` rather than into a
    // connection, so the resolver's pre-check sees in-degree 1 and accepts. An export-side count
    // that folded boundary targets in would see 2 and reject a graph that round-trips — a false
    // reject is not the safe direction here, it is a regression that blocks legitimate work.
    let g = ModelGraph {
        blocks: vec![
            source(0, "src", 0, 4.0),
            block(1, "CDL.Reals.Abs", "fed", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real).with_iri("http://example.org#SingleAssign.uSet"),
            conn(2, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 1)],
        external_inputs: vec![ConnectorId(1)],
    };

    let report = export_ok(&g);
    let g2 = reimport_clean(&report.bytes);
    assert_eq!(g2.connections.len(), 1, "the driving edge round-trips");
    assert_eq!(
        g2.external_inputs.len(),
        1,
        "the boundary entry round-trips alongside it"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Negative space — the property the rejection exists to guarantee.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_accepted_export_never_re_imports_with_a_single_assignment_error() {
    // The RT-2 property stated directly, over every accepted shape in this file plus the two
    // rejected ones. Each per-case test above pins one mechanism; this pins the outcome those
    // mechanisms exist for, so a future refactor that moves the check somewhere subtly wrong is
    // caught here even if it satisfies every individual assertion.
    let doubly_driven = ModelGraph {
        blocks: vec![
            source(0, "left", 0, 1.0),
            source(1, "right", 1, 2.0),
            block(2, "CDL.Reals.Abs", "sink", &[2], &[3], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
            conn(2, 2, Dir::In, ValueType::Real),
            conn(3, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 2), wire(1, 2)],
        external_inputs: vec![],
    };
    let duplicated_edge = ModelGraph {
        connections: vec![wire(0, 2), wire(0, 2)],
        ..doubly_driven.clone()
    };
    let single = ModelGraph {
        connections: vec![wire(0, 2)],
        ..doubly_driven.clone()
    };

    for (label, g) in [
        ("two drivers", &doubly_driven),
        ("one edge twice", &duplicated_edge),
        ("one driver", &single),
    ] {
        // Either the export rejects, or its bytes re-import without a single-assignment error.
        // Never `Ok` bytes that fail to load — that is the whole contract.
        let Ok(report) = export_with_report(g) else {
            continue;
        };
        let (_, import) =
            import_cxf(&report.bytes, &ResolveOptions::default()).unwrap_or_else(|e| {
                panic!("`{label}` exported Ok but did not re-import at all: {e:?}")
            });
        let offenders: Vec<&Diagnostic> = import
            .diagnostics
            .iter()
            .filter(|d| d.code == DiagCode::SingleAssignment && d.severity == Severity::Error)
            .collect();
        assert!(
            offenders.is_empty(),
            "`{label}` exported Ok with bytes that re-import as multiply driven: {offenders:?}"
        );
    }
}
