//! Behavioral tests for the enum tracked-deferral pre-pass: which blocks the cascade reaches, how
//! far it iterates, and what a deferred block is allowed to contribute to the export.
//!
//! These live here rather than beside the export acceptance/rejection surface in
//! `src/export_tests.rs` because they exercise deferral *behavior* end to end through the public
//! `export` / `export_with_report` API — the observable contract a host depends on — and because
//! that module is near the 700-LOC cap.
//!
//! Every graph is hand-built. Deferral is decided from the `ModelGraph` alone (the exporter takes
//! no registry dependency), so a deferred block's class path and arity never have to be
//! registry-valid; the surviving cone's do, and is, wherever a test re-imports the emitted bytes.

use std::sync::Arc;

use oce_cxf::{CxfError, ExportReport, ResolveOptions, export_with_report, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity, has_errors};
use oce_model::{
    Attrs, BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, EnumClassId,
    ModelGraph, NoAttrs, ParamTable, RealAttrs, Value, ValueType,
};

/// IRI prefix for every hand-built block in this file. Block `@id`s are `{PREFIX}{name}`, so a
/// warning subject reads back as the block's name.
const PREFIX: &str = "http://example.org#Defer.";

/// The block `@id` for `name` — the subject an `ExportDeferred` warning carries.
fn iri(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// A block instance owning the given connector positions. `inputs`/`outputs` are raw connector
/// indices; `id`/`decl_order` come from the block's own position, keeping the graph dense.
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

/// An output→input connection between two raw connector indices.
fn wire(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

/// The enum-valued parameter that puts a block in `enum_blocks` on the parameter axis.
fn enum_param() -> Vec<(Arc<str>, Value)> {
    vec![(
        Arc::from("controllerType"),
        Value::Enum {
            class: EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 1,
        },
    )]
}

/// A single Real parameter — the in-subset payload that keeps a survivor block interesting.
fn real_param(k: f64) -> Vec<(Arc<str>, Value)> {
    vec![(Arc::from("k"), Value::Real(k))]
}

/// Export `g`, requiring success, and return the report. Panics with the rejection diagnostics
/// when the export aborts, so a regression reads as its own diagnostics rather than as `unwrap`.
fn export_report(g: &ModelGraph) -> ExportReport {
    match export_with_report(g) {
        Ok(report) => report,
        Err(CxfError::Validation(diags)) => {
            panic!("expected a deferring export to succeed, but it rejected: {diags:?}")
        }
        Err(other) => panic!("unexpected error shape: {other:?}"),
    }
}

/// Export `g`, requiring rejection, and return the diagnostics. Unlike the all-error `rejection`
/// helper in `src/export_tests.rs`, this tolerates a mixed list: a total-deferral rejection
/// deliberately carries the `ExportDeferred` warnings that explain *why* nothing survived
/// alongside the `ExportUnsupported` error.
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

/// The set of blocks the deferral pre-pass reached, read off the report as sorted `@id`s. Every
/// warning is required to be an `ExportDeferred` at `Warning` severity on the way through, so a
/// severity or code regression fails here rather than silently widening the set.
///
/// Sorting is what makes this a *set* comparison, and it makes every assertion built on it blind
/// to the order the warnings were emitted in — including a wholesale reversal. `deferral_set`
/// promises block-then-cascade order, and that promise is pinned in
/// `tests/export_deferral_diagnostics.rs`, which reads the sequence as emitted. Do not add
/// ordering assertions here; add them there.
fn deferred_blocks(report: &ExportReport) -> Vec<String> {
    let mut subjects: Vec<String> = report
        .warnings
        .iter()
        .map(|d| {
            assert_eq!(
                d.code,
                DiagCode::ExportDeferred,
                "every export warning is a deferral: {d:?}"
            );
            assert_eq!(d.severity, Severity::Warning, "deferral is non-aborting");
            d.subject
                .as_deref()
                .expect("a deferral warning always names its block")
                .to_owned()
        })
        .collect();
    subjects.sort();
    subjects.dedup();
    subjects
}

/// Every `@id` in the emitted `@graph` — block nodes, parameter nodes, port nodes, boundary nodes,
/// and the synthetic root.
fn emitted_ids(bytes: &[u8]) -> Vec<String> {
    let doc: serde_json::Value = serde_json::from_slice(bytes).expect("export emits JSON");
    doc["@graph"]
        .as_array()
        .expect("@graph is an array")
        .iter()
        .filter_map(|n| n["@id"].as_str().map(String::from))
        .collect()
}

/// Assert the emitted document names `survivors` and mentions no `@id` belonging to any of
/// `deferred` — neither the block node nor a minted port node under it.
fn assert_emission(bytes: &[u8], survivors: &[&str], deferred: &[&str]) {
    let ids = emitted_ids(bytes);
    for name in survivors {
        let want = iri(name);
        assert!(
            ids.contains(&want),
            "the survivor `{name}` must be emitted, got: {ids:?}"
        );
    }
    for name in deferred {
        let gone = iri(name);
        let leaked: Vec<&String> = ids
            .iter()
            .filter(|id| **id == gone || id.starts_with(&format!("{gone}.")))
            .collect();
        assert!(
            leaked.is_empty(),
            "no node belonging to the deferred block `{name}` may be emitted, got: {leaked:?}"
        );
    }
}

/// Re-import the emitted bytes and require zero **error** diagnostics. Bytes coming back is not
/// the property — a document that re-imports with a `SingleAssignment` or `UnresolvedReference`
/// error is a broken export that merely parsed.
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
// Detection — the enum predicate is searched across the whole port list.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn an_enum_connector_past_the_first_port_position_still_defers() {
    // `has_enum_connector` searches `inputs.chain(outputs)` for the enum PREDICATE, not merely for
    // the first connector that resolves. `sink` carries a Real input at port-list position 0 and
    // its enum output at position 1: a detector that stopped at position 0 would miss it, leave it
    // out of `enum_blocks`, and let the Phase 4 enum arm reject the export outright.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "src",
                &[],
                &[0],
                real_param(2.0),
            ),
            block(1, "CDL.Reals.Abs", "sink", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(
                2,
                1,
                Dir::Out,
                ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
            ),
        ],
        connections: vec![wire(0, 1)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(
        deferred_blocks(&report),
        vec![iri("sink")],
        "only the enum-bearing block defers; the upstream source survives"
    );
    assert_emission(&report.bytes, &["src"], &["sink"]);
    reimport_clean(&report.bytes);
}

#[test]
fn the_reported_enum_class_is_the_first_one_in_port_list_order() {
    // The returned `EnumClassId` must stay the FIRST enum in `inputs`-then-`outputs` order, so the
    // `{class}` slot of the warning text is deterministic. `mixed` carries two enum connectors of
    // different classes; the input's class is the one named.
    let other = EnumClassId(0);
    assert_ne!(
        other,
        EnumClassId::SIMPLE_CONTROLLER,
        "the two connectors must carry distinct classes for this test to discriminate"
    );
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[0],
                real_param(1.0),
            ),
            block(1, "CDL.Reals.Abs", "mixed", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(
                1,
                1,
                Dir::In,
                ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
            ),
            conn(2, 1, Dir::Out, ValueType::Enum(other)),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(deferred_blocks(&report), vec![iri("mixed")]);
    let message = &report.warnings[0].message;
    assert!(
        message.contains("class `EnumClass#1`"),
        "the input connector's class (the first in port-list order) must be named, got: {message}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Total deferral — nothing survives, so there is nothing to emit.
// ─────────────────────────────────────────────────────────────────────────

/// The whole-operation message for a graph the cascade emptied. Pinned verbatim: a host greps for
/// it to tell "your graph is too enum-heavy to export" apart from "your graph was empty".
const MSG_TOTAL_DEFERRAL: &str = "CXF export requires at least one emitted runtime block: all \
     blocks were deferred or reserved lowering-only, leaving no runtime composite to emit";

#[test]
fn a_graph_whose_every_block_defers_is_rejected_not_emitted_as_a_root_only_document() {
    // The zero-block guard at the top of `plan` sees only the INPUT graph. When the cascade defers
    // everything, `plan.blocks` is empty, `containsBlock` is omitted, and the emitted root-only
    // document fails re-import as `MalformedDocument` — returned as `Ok` with the warnings
    // discarded by `export`, so a host would see success and no signal at all.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "src",
                &[],
                &[0],
                enum_param(),
            ),
            block(1, "CDL.Reals.Abs", "mid", &[1], &[2], vec![]),
            block(2, "CDL.Reals.Abs", "tail", &[3], &[4], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(2, 1, Dir::Out, ValueType::Real),
            conn(3, 2, Dir::In, ValueType::Real),
            conn(4, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 1), wire(2, 3)],
        ..ModelGraph::new()
    };

    let diags = export_rejection(&g);
    let errors: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(
        errors.len(),
        1,
        "exactly one whole-operation error: {diags:?}"
    );
    assert_eq!(errors[0].code, DiagCode::ExportUnsupported);
    assert_eq!(
        errors[0].subject, None,
        "a whole-operation rejection must not blame any single node"
    );
    assert_eq!(errors[0].message, MSG_TOTAL_DEFERRAL);
    // The deferral warnings ride along: they are the explanation for the error, so a host can name
    // every block it must remove enum content from.
    let mut warned: Vec<&str> = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .filter_map(|d| d.subject.as_deref())
        .collect();
    warned.sort();
    assert_eq!(
        warned,
        vec![iri("mid"), iri("src"), iri("tail")],
        "every deferred block is still named: {diags:?}"
    );
}

#[test]
fn a_lone_enum_bearing_block_is_rejected_rather_than_emitting_an_empty_composite() {
    // The one-block floor of the same defect, and the shape the review found regress-locked as
    // `Ok`: a single block carrying an enum connector defers, leaving zero survivors.
    let g = ModelGraph {
        blocks: vec![block(
            0,
            "CDL.Reals.Sources.Constant",
            "only",
            &[],
            &[0],
            vec![],
        )],
        connectors: vec![conn(
            0,
            0,
            Dir::Out,
            ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
        )],
        connections: vec![],
        ..ModelGraph::new()
    };

    let diags = export_rejection(&g);
    assert!(
        diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.message == MSG_TOTAL_DEFERRAL),
        "total deferral must reject with the no-runtime-block message: {diags:?}"
    );
}

#[test]
fn a_single_survivor_keeps_the_export_non_aborting() {
    // The boundary of the total-deferral gate: one survivor is enough. Guards against the gate
    // being widened into "any deferral aborts", which would retire tracked deferral entirely.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "only",
                &[],
                &[0],
                enum_param(),
            ),
            block(
                1,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[1],
                real_param(3.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(deferred_blocks(&report), vec![iri("only")]);
    assert_emission(&report.bytes, &["keep"], &["only"]);
    reimport_clean(&report.bytes);
}

// ─────────────────────────────────────────────────────────────────────────
// The cascade fixpoint — transitivity, and the soundness limits on it.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn the_cascade_follows_a_chain_that_needs_several_reiterations_to_close() {
    // The blocks are laid out in REVERSE dataflow order — `tail` at index 0, the enum source at
    // index 3 — so one sweep of the block vector can only ever advance the deferred set by one
    // link: sweep 1 reaches `head`, sweep 2 `mid`, sweep 3 `tail`. A cascade that swept once and
    // stopped would leave `mid` and `tail` alive, emitting two blocks whose only driver the
    // document omits. This is the test the `while grew` loop exists for.
    let g = ModelGraph {
        blocks: vec![
            block(0, "CDL.Reals.Abs", "tail", &[0], &[1], vec![]),
            block(1, "CDL.Reals.Abs", "mid", &[2], &[3], vec![]),
            block(2, "CDL.Reals.Abs", "head", &[4], &[5], vec![]),
            block(
                3,
                "CDL.Reals.Sources.Constant",
                "src",
                &[],
                &[6],
                enum_param(),
            ),
            block(
                4,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[7],
                real_param(4.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real),
            conn(1, 0, Dir::Out, ValueType::Real),
            conn(2, 1, Dir::In, ValueType::Real),
            conn(3, 1, Dir::Out, ValueType::Real),
            conn(4, 2, Dir::In, ValueType::Real),
            conn(5, 2, Dir::Out, ValueType::Real),
            conn(6, 3, Dir::Out, ValueType::Real),
            conn(7, 4, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(6, 4), wire(5, 2), wire(3, 0)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(
        deferred_blocks(&report),
        vec![iri("head"), iri("mid"), iri("src"), iri("tail")],
        "the cascade must reach the far end of the chain, not just the first link"
    );
    assert_emission(&report.bytes, &["keep"], &["src", "head", "mid", "tail"]);
    reimport_clean(&report.bytes);
}

#[test]
fn a_fan_out_cascade_reaches_every_branch_of_the_cone() {
    // Transitivity across a branch, not just a line: the enum source drives `left` and `right`,
    // and `join` consumes both. Every input of `join` loses all its drivers, so the whole cone
    // goes. Laid out in reverse order again so the closure needs more than one sweep.
    let g = ModelGraph {
        blocks: vec![
            block(0, "CDL.Reals.Add", "join", &[0, 1], &[2], vec![]),
            block(1, "CDL.Reals.Abs", "left", &[3], &[4], vec![]),
            block(2, "CDL.Reals.Abs", "right", &[5], &[6], vec![]),
            block(
                3,
                "CDL.Reals.Sources.Constant",
                "src",
                &[],
                &[7],
                enum_param(),
            ),
            block(
                4,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[8],
                real_param(5.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::In, ValueType::Real),
            conn(1, 0, Dir::In, ValueType::Real),
            conn(2, 0, Dir::Out, ValueType::Real),
            conn(3, 1, Dir::In, ValueType::Real),
            conn(4, 1, Dir::Out, ValueType::Real),
            conn(5, 2, Dir::In, ValueType::Real),
            conn(6, 2, Dir::Out, ValueType::Real),
            conn(7, 3, Dir::Out, ValueType::Real),
            conn(8, 4, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(7, 3), wire(7, 5), wire(4, 0), wire(6, 1)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(
        deferred_blocks(&report),
        vec![iri("join"), iri("left"), iri("right"), iri("src")],
        "the whole downstream cone defers"
    );
    assert_emission(&report.bytes, &["keep"], &["src", "left", "right", "join"]);
}

#[test]
fn one_surviving_driver_keeps_a_consumer_out_of_the_cascade() {
    // The soundness limit on transitivity: an input cascades only when EVERY driver is deferred.
    // `mix`'s single input is fed by both the enum source and a live source, so `mix` survives and
    // the emitted document keeps it wired to the live driver. Widening the rule to "any deferred
    // driver" would delete blocks that still have a signal.
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
            block(
                1,
                "CDL.Reals.Sources.Constant",
                "livesrc",
                &[],
                &[1],
                real_param(6.0),
            ),
            block(2, "CDL.Reals.Abs", "mix", &[2], &[3], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::Real),
            conn(2, 2, Dir::In, ValueType::Real),
            conn(3, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 2), wire(1, 2)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(
        deferred_blocks(&report),
        vec![iri("enumsrc")],
        "a consumer with one live driver must not cascade"
    );
    assert_emission(&report.bytes, &["livesrc", "mix"], &["enumsrc"]);
    // The surviving edge is the live one, and it is the ONLY driver left on `mix` — the emitted
    // document is single-assignment, which is what makes the re-import error-free.
    let g2 = reimport_clean(&report.bytes);
    assert_eq!(
        g2.connections.len(),
        1,
        "exactly the live edge survives: {:?}",
        g2.connections
    );
}

#[test]
fn an_input_that_was_never_driven_does_not_cascade() {
    // A boundary input is undriven inter-block in the ORIGINAL graph, so it has no drivers to lose
    // and must not be read as "all drivers deferred". Treating zero drivers as a vacuous truth
    // would defer every boundary-fed block the moment anything else in the graph carried an enum.
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
            block(1, "CDL.Reals.Abs", "boundaryfed", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real).with_iri("http://example.org#Defer.uSet"),
            conn(2, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(1)],
        boundary_outputs: vec![],
    };

    let report = export_report(&g);
    assert_eq!(
        deferred_blocks(&report),
        vec![iri("enumsrc")],
        "a boundary-fed block must survive an unrelated deferral"
    );
    assert_emission(&report.bytes, &["boundaryfed"], &["enumsrc"]);
    let g2 = reimport_clean(&report.bytes);
    assert_eq!(
        g2.external_inputs.len(),
        1,
        "the boundary input is rebuilt on re-import"
    );
}

#[test]
fn a_feedback_cycle_reached_by_the_cascade_terminates_with_both_ends_deferred() {
    // Termination under a cycle: `spin` and `back` drive each other, and `spin`'s other input is
    // fed only by the enum source. `spin` cascades on that input, `back` then cascades on `spin`,
    // and `spin` is already in the set — so the fixpoint closes instead of chasing the loop. That
    // the test returns at all is half the assertion; the other half is that BOTH ends went.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "src",
                &[],
                &[0],
                enum_param(),
            ),
            block(1, "CDL.Reals.Add", "spin", &[1, 2], &[3], vec![]),
            block(2, "CDL.Reals.Abs", "back", &[4], &[5], vec![]),
            block(
                3,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[6],
                real_param(7.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(2, 1, Dir::In, ValueType::Real),
            conn(3, 1, Dir::Out, ValueType::Real),
            conn(4, 2, Dir::In, ValueType::Real),
            conn(5, 2, Dir::Out, ValueType::Real),
            conn(6, 3, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 1), wire(3, 4), wire(5, 2)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(
        deferred_blocks(&report),
        vec![iri("back"), iri("spin"), iri("src")],
        "both ends of the cycle enter the deferred set"
    );
    assert_emission(&report.bytes, &["keep"], &["src", "spin", "back"]);
}

#[test]
fn a_self_loop_on_a_survivor_terminates_without_deferring_it() {
    // The degenerate cycle: `spin`'s only driver is itself. It is its own live driver, so it never
    // cascades — and the fixpoint must still close rather than revisit it forever.
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
            block(1, "CDL.Reals.Abs", "spin", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(2, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(2, 1)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(deferred_blocks(&report), vec![iri("enumsrc")]);
    assert_emission(&report.bytes, &["spin"], &["enumsrc"]);
}

// ─────────────────────────────────────────────────────────────────────────
// What a deferred ordinary block may contribute — nothing error-severity.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn out_of_subset_connector_attributes_on_a_deferred_block_do_not_abort_the_export() {
    // `bad`'s Real output carries `nominal`, which is outside the canonical export subset and
    // rejects on a surviving block. `bad` is deferred, so that connector is never emitted and its
    // attributes can never reach the wire — letting them abort would sink a document over a defect
    // in bytes nobody writes.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[0],
                real_param(8.0),
            ),
            block(1, "CDL.Reals.Abs", "bad", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(
                1,
                1,
                Dir::In,
                ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
            ),
            conn(2, 1, Dir::Out, ValueType::Real)
                .with_attrs(Attrs::Real(RealAttrs {
                    nominal: Some(1.0),
                    unbounded: Some(true),
                    min: Some(f64::NAN),
                    ..RealAttrs::default()
                }))
                .expect("Real attrs on a Real connector"),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(deferred_blocks(&report), vec![iri("bad")]);
    assert_emission(&report.bytes, &["keep"], &["bad"]);
    reimport_clean(&report.bytes);
}

#[test]
fn a_tag_mismatched_attribute_set_on_a_deferred_block_does_not_abort_the_export() {
    // The other Phase 4 attribute arm: an `Attrs` variant that disagrees with the connector's
    // `ValueType`. `with_attrs` refuses to build one, so the field is set directly — the shape a
    // hand-built graph can still reach. On a survivor this is a structure rejection; on a deferred
    // block it must be silent.
    let mut mismatched = conn(2, 1, Dir::Out, ValueType::Real);
    mismatched.attrs = Attrs::Boolean(NoAttrs);
    assert!(
        !mismatched.attrs.matches(mismatched.value_type),
        "the connector must genuinely violate the tag invariant for this test to mean anything"
    );
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[0],
                real_param(9.0),
            ),
            block(1, "CDL.Reals.Abs", "bad", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(
                1,
                1,
                Dir::In,
                ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER),
            ),
            mismatched,
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(deferred_blocks(&report), vec![iri("bad")]);
    assert_emission(&report.bytes, &["keep"], &["bad"]);
}

#[test]
fn boundary_entry_for_a_deferred_block_is_dropped_not_rejected() {
    // The Phase 6 skip, pinned. `sink`'s enum input is listed in `external_inputs` but carries no
    // stored boundary IRI — on a surviving block that is an `MSG_EXTERNAL_IRI` rejection. Because
    // `sink` is deferred the entry describes a port the document does not contain, so it is
    // dropped silently. Delete the `deferred.contains` skip in Phase 6 and the missing-IRI check
    // below it fires, turning this export into an abort.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[0],
                real_param(10.0),
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
            ),
            conn(2, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        external_inputs: vec![ConnectorId(1)],
        boundary_outputs: vec![],
    };

    let report = export_report(&g);
    assert_eq!(deferred_blocks(&report), vec![iri("sink")]);
    assert_emission(&report.bytes, &["keep"], &["sink"]);
    // No boundary node survives either: the root carries no `hasInput` at all.
    let doc: serde_json::Value = serde_json::from_slice(&report.bytes).expect("export emits JSON");
    let root = doc["@graph"]
        .as_array()
        .expect("@graph is an array")
        .iter()
        .find(|n| n["@id"] == "urn:open-control:cxf-export:root")
        .expect("the root is always emitted");
    assert!(
        root.get("S231:hasInput").is_none(),
        "the dropped boundary must leave no root hasInput, got: {root}"
    );
    reimport_clean(&report.bytes);
}
