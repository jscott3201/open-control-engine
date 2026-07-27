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

    // Each case carries its EXPECTED outcome. An earlier version of this test skipped every `Err`
    // and asserted only on the `Ok` branch, which made it blind in the one direction a
    // too-aggressive check fails in: a mutation that rejected everything left it green while
    // sinking its own `"one driver"` case. Asserting acceptance is half the property.
    // The sweep spans in-degree 0, 1, 2 and well above, because the OUTCOME property has to hold
    // at every degree, not only at the two the per-shape tests use. A threshold that happened to
    // be right for 2 and 3 — and wrong for 4 — would emit `Ok` bytes here that do not load, and
    // without the high-fan-in cases nothing in this sweep would see it.
    let fan4 = fan_in(4);
    let fan9 = fan_in(9);
    for (label, g, expect_accept) in [
        ("two drivers", &doubly_driven, false),
        ("one edge twice", &duplicated_edge, false),
        ("one driver", &single, true),
        (
            "two inputs, one driver each",
            &multi_input_singly_driven(),
            true,
        ),
        ("four drivers", &fan4, false),
        ("nine drivers", &fan9, false),
    ] {
        let report = match export_with_report(g) {
            Ok(report) => {
                assert!(
                    expect_accept,
                    "`{label}` must be rejected, but exported {} bytes",
                    report.bytes.len()
                );
                report
            }
            Err(e) => {
                assert!(
                    !expect_accept,
                    "`{label}` must export, but was rejected: {e:?}"
                );
                continue;
            }
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

/// One `CDL.Reals.Add` whose TWO inputs each carry exactly one driver — the shape a per-block
/// count gets wrong.
///
/// This is not an exotic graph: every multi-input block in the G36 corpus looks like this. In-degree
/// is a property of a *connector*, not of the block that owns it, and an implementation that
/// counted "edges arriving at this block" would read 2 here and reject a perfectly valid graph.
/// Every other graph in this file has at most one driven input per block, so without this case the
/// whole suite cannot tell the two implementations apart.
fn multi_input_singly_driven() -> ModelGraph {
    ModelGraph {
        blocks: vec![
            source(0, "left", 0, 1.0),
            source(1, "right", 1, 2.0),
            block(2, "CDL.Reals.Add", "join", &[2, 3], &[4], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
            conn(2, 2, Dir::In, ValueType::Real),
            conn(3, 2, Dir::In, ValueType::Real),
            conn(4, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 2), wire(1, 3)],
        external_inputs: vec![],
    }
}

#[test]
fn a_block_whose_inputs_are_each_singly_driven_is_accepted() {
    // The false-reject guard for the counter's granularity, asserted on its own so a failure says
    // "per-block counting" rather than surfacing inside the property sweep.
    let g = multi_input_singly_driven();
    let report = export_ok(&g);
    let g2 = reimport_clean(&report.bytes);
    assert_eq!(
        g2.connections.len(),
        2,
        "both singly-driven edges must round-trip"
    );
}

/// `n` distinct sources all driving one input. Returns a graph whose post-filter in-degree is
/// exactly `n`.
fn fan_in(n: u32) -> ModelGraph {
    let mut blocks: Vec<_> = (0..n)
        .map(|i| source(i, &format!("src{i}"), i, f64::from(i)))
        .collect();
    let sink_in = n;
    let sink_out = n + 1;
    blocks.push(block(
        n,
        "CDL.Reals.Abs",
        "sink",
        &[sink_in],
        &[sink_out],
        vec![],
    ));

    let mut connectors: Vec<_> = (0..n)
        .map(|i| conn(i, i, Dir::Out, ValueType::Real))
        .collect();
    connectors.push(conn(sink_in, n, Dir::In, ValueType::Real));
    connectors.push(conn(sink_out, n, Dir::Out, ValueType::Real));

    ModelGraph {
        blocks,
        connectors,
        connections: (0..n).map(|i| wire(i, sink_in)).collect(),
        external_inputs: vec![],
    }
}

#[test]
fn rejection_holds_at_every_in_degree_above_one() {
    // The threshold is `> 1`, not a set of small values. Sweeping the boundary and well past it
    // kills any implementation that happens to be right for the two- and three-driver cases the
    // per-shape tests above use — an arm matching `2 | 3`, say, which those tests cannot see.
    for n in [2u32, 3, 4, 5, 9, 17] {
        let diags = export_rejection(&fan_in(n));
        assert_eq!(
            multiply_driven(&diags).len(),
            1,
            "in-degree {n} is one defect on one connector"
        );
    }
}

#[test]
fn a_large_fan_in_rejects_without_panicking() {
    // Totality at scale. The planner is a total function over arbitrary `ModelGraph` state, so a
    // large edge count must produce a diagnostic, never an arithmetic panic — the counter
    // saturates rather than overflowing. 300 exceeds a `u8` counter, which is the smallest width
    // an implementation could plausibly have reached for.
    let diags = export_rejection(&fan_in(300));
    assert_eq!(multiply_driven(&diags).len(), 1);
}

#[test]
fn repeated_exports_of_a_rejection_are_diagnostic_identical() {
    // Determinism (TESTING.md pillar 4) applies to the rejection path too: diagnostics are ordered
    // by connector position, so two runs over the same graph must produce the identical list. A
    // set-based or hash-ordered implementation passes every other test in this file.
    let g = fan_in(4);
    let first: Vec<(String, String)> = multiply_driven(&export_rejection(&g));
    let second: Vec<(String, String)> = multiply_driven(&export_rejection(&g));
    assert_eq!(first, second, "the rejection list must be deterministic");
}
