//! What a deferral *reports*, as opposed to which blocks it reaches: the fully rendered warning
//! text, the order the warnings arrive in, and the rule that a deferred block's out-of-subset
//! String connector is silent rather than fatal.
//!
//! `tests/export_deferral.rs` owns the deferred-*set* behaviour and reads its warnings through a
//! helper that sorts and dedups subjects. That helper is deliberately order-blind, so nothing over
//! there can see a reordering, and nothing over there reads a message body — a review found that
//! substituting a wrong block name into a warning, and reversing the whole warning vector, both
//! left the entire suite green. These tests close that: every assertion here reads either the
//! exact rendered string or the emitted sequence.
//!
//! Every graph is hand-built. Deferral is decided from the `ModelGraph` alone (the exporter takes
//! no registry dependency), so a deferred block's class path and arity never have to be
//! registry-valid; the surviving cone's do, and is, wherever a test re-imports the emitted bytes.

use std::sync::Arc;

use oce_cxf::{CxfError, ExportReport, ResolveOptions, export_with_report, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity, has_errors};
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, EnumClassId, ModelGraph,
    ParamTable, Value, ValueType,
};

/// IRI prefix for every hand-built block in this file — the same one `tests/export_deferral.rs`
/// uses, so a warning subject reads back as the block's name and the rendered goldens below match
/// the shape a host sees for those graphs too.
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

/// An enum-valued parameter of the given class, named so the `{name}` slot of the parameter
/// warning carries something no other slot could be mistaken for.
fn enum_param(name: &str, class: EnumClassId) -> Vec<(Arc<str>, Value)> {
    vec![(Arc::from(name), Value::Enum { class, ordinal: 1 })]
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

/// Export `g`, requiring rejection, and return the diagnostics. Tolerates a mixed list: a
/// rejection can ride alongside the `ExportDeferred` warnings for blocks that did defer.
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

/// The warning subjects **as emitted** — no sort, no dedup. The order is the assertion here, so
/// this deliberately does the opposite of `export_deferral.rs`'s set-comparison helper.
fn warning_subjects(report: &ExportReport) -> Vec<&str> {
    report
        .warnings
        .iter()
        .map(|d| {
            assert_eq!(
                d.code,
                DiagCode::ExportDeferred,
                "every warning defers: {d:?}"
            );
            assert_eq!(d.severity, Severity::Warning, "deferral is non-aborting");
            d.subject
                .as_deref()
                .expect("a deferral warning always names its block")
        })
        .collect()
}

/// Re-import the emitted bytes and require zero **error** diagnostics. Bytes coming back is not
/// the property — a document that re-imports with a `SingleAssignment` or `UnresolvedReference`
/// error is a broken export that merely parsed.
fn reimport_clean(bytes: &[u8]) {
    let (_g, report) =
        import_cxf(bytes, &ResolveOptions::default()).expect("emitted bytes must re-import");
    assert!(
        !has_errors(&report.diagnostics),
        "re-import must produce zero error diagnostics, got: {:?}",
        report.diagnostics
    );
}

/// Assert the emitted document names `survivors` and mentions no `@id` belonging to any of
/// `deferred` — neither the block node nor a minted port node under it.
fn assert_emission(bytes: &[u8], survivors: &[&str], deferred: &[&str]) {
    let doc: serde_json::Value = serde_json::from_slice(bytes).expect("export emits JSON");
    let ids: Vec<String> = doc["@graph"]
        .as_array()
        .expect("@graph is an array")
        .iter()
        .filter_map(|n| n["@id"].as_str().map(String::from))
        .collect();
    for name in survivors {
        assert!(
            ids.contains(&iri(name)),
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

// ─────────────────────────────────────────────────────────────────────────
// Rendered warning goldens — every slot, filled.
//
// These are written out in full rather than assembled from the renderer's own format strings.
// Assembling them would re-run the code under test and assert nothing: whatever the renderer
// produced would be what the test expected. Each golden below fails on any wrong value in any
// slot, which is what caught a warning naming a block that does not exist in the document.
// ─────────────────────────────────────────────────────────────────────────

/// The connector-shape message rendered for `sink`, whose enum input carries `Smoothness` (class
/// id 2). The class slot is load-bearing: before this round every id of 2 or more rendered as a
/// single literal, so this golden also pins the class label away from that fold.
const RENDERED_CONNECTOR_WARNING: &str = "export subset: deferring block \
     `http://example.org#Defer.sink` — enumeration-typed connector (class `EnumClass#2`) has no \
     CXF literal form; the block and its downstream consumers are omitted from the emitted \
     document so the enum-free remainder can export";

/// The parameter-shape message rendered for `tuned`, whose `extrapolation` parameter carries
/// `Extrapolation` (class id 3). Three distinct slots — subject, name, class — and no two of them
/// hold the same text, so swapping any pair fails.
const RENDERED_PARAM_WARNING: &str = "export subset: deferring block \
     `http://example.org#Defer.tuned` — parameter `extrapolation` is enumeration-valued (class \
     `EnumClass#3`); the block and its downstream consumers are omitted from the emitted document \
     so the enum-free remainder can export";

/// The cascade-shape message rendered for `sink`, cascade-deferred on its **second** input. `in1`
/// rather than `in0` is the point: the block's first input is undriven and skipped, so a
/// connector-name substitution that reported the first input, or a hardcoded `in0`, fails here.
const RENDERED_CASCADE_WARNING: &str = "export subset: deferring block \
     `http://example.org#Defer.sink` — all drivers of input connector `in1` were deferred \
     (upstream enumeration); the block is omitted from the emitted document so the enum-free \
     remainder can export";

#[test]
fn the_enum_connector_deferral_warning_renders_every_placeholder() {
    // `sink`'s enum input defers it; `keep` survives so the export is non-aborting and the
    // warning is reachable through the public report at all.
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
            block(1, "CDL.Reals.Abs", "sink", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Enum(EnumClassId::SMOOTHNESS)),
            conn(2, 1, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(report.warnings.len(), 1, "got: {:?}", report.warnings);
    assert_eq!(report.warnings[0].message, RENDERED_CONNECTOR_WARNING);
    assert_eq!(
        report.warnings[0].subject.as_deref(),
        Some(iri("sink").as_str())
    );
    reimport_clean(&report.bytes);
}

#[test]
fn the_enum_parameter_deferral_warning_renders_every_placeholder() {
    // The parameter axis. `tuned` carries no enum connector, so the parameter arm — not the
    // connector arm — is the one that fires.
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
            block(
                1,
                "CDL.Reals.Sources.Constant",
                "tuned",
                &[],
                &[1],
                enum_param("extrapolation", EnumClassId::EXTRAPOLATION),
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
    assert_eq!(report.warnings.len(), 1, "got: {:?}", report.warnings);
    assert_eq!(report.warnings[0].message, RENDERED_PARAM_WARNING);
    assert_eq!(
        report.warnings[0].subject.as_deref(),
        Some(iri("tuned").as_str())
    );
    reimport_clean(&report.bytes);
}

#[test]
fn the_cascade_deferral_warning_renders_every_placeholder() {
    // `sink`'s first input has no driver at all (skipped, not a cascade trigger); its second is
    // fed only by the deferred `src`. So the cascade fires on `in1`, and the warning must say so.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "src",
                &[],
                &[0],
                enum_param("controllerType", EnumClassId::SIMPLE_CONTROLLER),
            ),
            block(1, "CDL.Reals.Add", "sink", &[1, 2], &[3], vec![]),
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
            conn(2, 1, Dir::In, ValueType::Real),
            conn(3, 1, Dir::Out, ValueType::Real),
            conn(4, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 2)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(warning_subjects(&report), vec![iri("src"), iri("sink")]);
    assert_eq!(report.warnings[1].message, RENDERED_CASCADE_WARNING);
    reimport_clean(&report.bytes);
}

// ─────────────────────────────────────────────────────────────────────────
// Emission order — the documented block-then-cascade sequence.
// ─────────────────────────────────────────────────────────────────────────

/// A graph carrying all three warning shapes at once: `zebra` defers on an enum connector,
/// `alpha` on an enum parameter, `consumer` cascades off `zebra`, and `keep` survives.
///
/// The names are chosen against the block indices on purpose. Block order is `zebra`, `alpha`;
/// alphabetical order is `alpha`, `zebra`. A helper that sorted the subjects — which is exactly
/// how the deferred-set assertions read them — cannot tell the two apart, and neither can one
/// that reversed them.
fn every_warning_shape_graph() -> ModelGraph {
    ModelGraph {
        blocks: vec![
            block(0, "CDL.Reals.Abs", "zebra", &[], &[0, 1], vec![]),
            block(
                1,
                "CDL.Reals.Sources.Constant",
                "alpha",
                &[],
                &[2],
                enum_param("extrapolation", EnumClassId::EXTRAPOLATION),
            ),
            block(2, "CDL.Reals.Abs", "consumer", &[3], &[4], vec![]),
            block(
                3,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[5],
                real_param(4.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Enum(EnumClassId::SMOOTHNESS)),
            conn(1, 0, Dir::Out, ValueType::Real),
            conn(2, 1, Dir::Out, ValueType::Real),
            conn(3, 2, Dir::In, ValueType::Real),
            conn(4, 2, Dir::Out, ValueType::Real),
            conn(5, 3, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(1, 3)],
        ..ModelGraph::new()
    }
}

#[test]
fn deferral_warnings_arrive_in_block_then_cascade_order() {
    // `deferral_set`'s rustdoc promises this sequence, and a host that surfaces only the first
    // warning shows the user whichever block is at the front — so the front must be a block that
    // actually carries enum content, never one the cascade merely reached.
    let report = export_report(&every_warning_shape_graph());
    assert_eq!(
        warning_subjects(&report),
        vec![iri("zebra"), iri("alpha"), iri("consumer")],
        "originating enum blocks come first in block order, then the cascade"
    );
    assert_emission(&report.bytes, &["keep"], &["zebra", "alpha", "consumer"]);
    reimport_clean(&report.bytes);
}

#[test]
fn the_cascade_warning_never_precedes_the_block_that_caused_it() {
    // The ordering property stated as a relation rather than a fixed sequence, so it keeps
    // meaning if the graph grows: `consumer` was deferred *because* `zebra` was, and a reader
    // walking the list top-down must meet the cause before the consequence.
    let report = export_report(&every_warning_shape_graph());
    let subjects = warning_subjects(&report);
    let cause = subjects
        .iter()
        .position(|s| *s == iri("zebra"))
        .expect("the enum block is warned");
    let effect = subjects
        .iter()
        .position(|s| *s == iri("consumer"))
        .expect("the cascade-reached block is warned");
    assert!(
        cause < effect,
        "the cascade warning must follow its cause, got: {subjects:?}"
    );
}

#[test]
fn no_rendered_warning_ships_an_unsubstituted_placeholder() {
    // Braces in a rendered warning are now always data — the messages are rendered by one
    // `format!` each, so a slot with no argument is a compile error rather than a literal
    // `{name}` shipped to an operator. What this still catches is a stray brace written into a
    // message literal itself. None of this graph's own text contains one, so none may appear;
    // `a_parameter_named_like_a_placeholder_survives_verbatim` covers the opposite direction,
    // where braces in the *data* must be preserved.
    let report = export_report(&every_warning_shape_graph());
    for d in &report.warnings {
        assert!(
            !d.message.contains('{') && !d.message.contains('}'),
            "an unsubstituted placeholder reached the message: {}",
            d.message
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Placeholder-shaped data — a value is never re-read as template.
//
// `instance_iri` and parameter names are host-supplied `ModelGraph` state, and a deferred block
// bypasses the survivor-side parameter-name validation, so both reach the renderer unscreened.
// While the messages were rendered by a chain of `str::replace`, each step re-scanned what the
// previous step had inserted, and a value shaped like a placeholder was rewritten by a later
// step. These three tests — one per message shape — carry placeholder-shaped values end to end
// through the public API and require them to arrive verbatim.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn a_parameter_named_like_a_placeholder_survives_verbatim() {
    // The reviewer's reproduction. A parameter literally named `{class}` used to render as its
    // own class label — `parameter `EnumClass#3`` — so the warning named a parameter that does
    // not exist on the block, and the operator's grep for the real name found nothing.
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
            block(
                1,
                "CDL.Reals.Sources.Constant",
                "tuned",
                &[],
                &[1],
                enum_param("{class}", EnumClassId::EXTRAPOLATION),
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
    let message = &report.warnings[0].message;
    assert!(
        message.contains("parameter `{class}`"),
        "the parameter's real name must appear verbatim, got: {message}"
    );
    assert!(
        message.contains("class `EnumClass#3`"),
        "the class slot must still carry the class label, got: {message}"
    );
    assert!(
        !message.contains("parameter `EnumClass#3`"),
        "the class label must not have overwritten the parameter name, got: {message}"
    );
    reimport_clean(&report.bytes);
}

#[test]
fn a_parameter_named_like_the_subject_slot_survives_verbatim() {
    // The permutation the test above cannot see. `{class}` names a slot filled *after* the
    // parameter name, so it only catches a renderer working front to back; `{subject}` names one
    // filled *before* it, and catches a renderer working the other way. A review built exactly
    // that — a `class → name → subject` chain — and every test passed while this parameter
    // rendered as `parameter \`http://example.org#Defer.tuned\``, naming the block instead.
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
            block(
                1,
                "CDL.Reals.Sources.Constant",
                "tuned",
                &[],
                &[1],
                enum_param("{subject}", EnumClassId::EXTRAPOLATION),
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
    let message = &report.warnings[0].message;
    assert!(
        message.contains("parameter `{subject}`"),
        "the parameter's real name must appear verbatim, got: {message}"
    );
    assert!(
        message.contains(&format!("block `{}`", iri("tuned"))),
        "the subject slot must still carry the block IRI, got: {message}"
    );
    assert!(
        !message.contains(&format!("parameter `{}`", iri("tuned"))),
        "the block IRI must not have overwritten the parameter name, got: {message}"
    );
    reimport_clean(&report.bytes);
}

#[test]
fn a_block_iri_shaped_like_a_placeholder_survives_verbatim() {
    // The wider hole: `{subject}` is substituted first, and its value is an `instance_iri` — host
    // -supplied rather than derived — so under the chain every later slot rewrote the text it had
    // just inserted. `mixed` carries an enum connector (the class shape) and `tuned` an enum
    // parameter (the name shape); each block's IRI names the slot that used to eat it.
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
            block(1, "CDL.Reals.Abs", "mixed{class}", &[1], &[2], vec![]),
            block(
                2,
                "CDL.Reals.Sources.Constant",
                "tuned{name}",
                &[],
                &[3],
                enum_param("controllerType", EnumClassId::SIMPLE_CONTROLLER),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Enum(EnumClassId::SMOOTHNESS)),
            conn(2, 1, Dir::Out, ValueType::Real),
            conn(3, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(
        warning_subjects(&report),
        vec![iri("mixed{class}"), iri("tuned{name}")],
        "the subject field itself is unaffected either way — the defect was only ever in the text"
    );

    let connector_message = &report.warnings[0].message;
    assert!(
        connector_message.contains(&format!("block `{}`", iri("mixed{class}")))
            && connector_message.contains("class `EnumClass#2`"),
        "the IRI's braces must not have been eaten by the class slot, got: {connector_message}"
    );
    let param_message = &report.warnings[1].message;
    assert!(
        param_message.contains(&format!("block `{}`", iri("tuned{name}")))
            && param_message.contains("parameter `controllerType`"),
        "the IRI's braces must not have been eaten by the name slot, got: {param_message}"
    );
    reimport_clean(&report.bytes);
}

#[test]
fn a_cascade_deferred_block_iri_shaped_like_a_placeholder_survives_verbatim() {
    // The third shape. `sink{conn}` is deferred by cascade, and `{conn}` is substituted after
    // `{subject}`, so under the chain the block's own IRI absorbed the connector name and the
    // warning named a block nobody could find.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "src",
                &[],
                &[0],
                enum_param("controllerType", EnumClassId::SIMPLE_CONTROLLER),
            ),
            block(1, "CDL.Reals.Abs", "sink{conn}", &[1], &[2], vec![]),
            block(
                2,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[3],
                real_param(3.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(2, 1, Dir::Out, ValueType::Real),
            conn(3, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 1)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    let cascade_message = &report.warnings[1].message;
    assert!(
        cascade_message.contains(&format!("block `{}`", iri("sink{conn}"))),
        "the cascade subject must keep its literal braces, got: {cascade_message}"
    );
    assert!(
        cascade_message.contains("input connector `in0`") && !cascade_message.contains("sinkin0"),
        "the connector name must fill only its own slot, got: {cascade_message}"
    );
    reimport_clean(&report.bytes);
}

// ─────────────────────────────────────────────────────────────────────────
// The enum class label — distinct classes must read distinctly.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn two_enum_classes_on_one_block_do_not_render_as_the_same_label() {
    // `mixed` carries `Smoothness` (2) on its input and `Extrapolation` (3) on its output. The
    // reported class is the first in `inputs`-then-`outputs` order — an assertion that means
    // nothing unless the two ids render differently, which is precisely what the old label,
    // folding everything from 2 upward into one string, took away.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[0],
                real_param(5.0),
            ),
            block(1, "CDL.Reals.Abs", "mixed", &[1], &[2], vec![]),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Enum(EnumClassId::SMOOTHNESS)),
            conn(2, 1, Dir::Out, ValueType::Enum(EnumClassId::EXTRAPOLATION)),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    let message = &report.warnings[0].message;
    assert!(
        message.contains("class `EnumClass#2`"),
        "the input connector's class must be named, got: {message}"
    );
    assert!(
        !message.contains("class `EnumClass#3`"),
        "the output connector's class must not be the one reported, got: {message}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// String connectors — fatal on a survivor, silent on a deferred block.
// ─────────────────────────────────────────────────────────────────────────

/// The rejection a String connector earns on a block the document actually emits.
const MSG_STRING_CONNECTOR: &str =
    "export subset: String connectors are not permitted in CXF (§7.8)";

#[test]
fn a_string_connector_on_an_enum_deferred_block_does_not_abort_the_export() {
    // `stringy` is deferred on its enum parameter, so its String output is never emitted and can
    // never reach the wire. Rejecting on it would sink a whole document over bytes nobody writes
    // — the same rule the attribute and boundary arms already follow, one match arm away.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[0],
                real_param(6.0),
            ),
            block(
                1,
                "CDL.Reals.Sources.Constant",
                "stringy",
                &[],
                &[1],
                enum_param("controllerType", EnumClassId::SIMPLE_CONTROLLER),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::String),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(warning_subjects(&report), vec![iri("stringy")]);
    assert_emission(&report.bytes, &["keep"], &["stringy"]);
    reimport_clean(&report.bytes);
}

#[test]
fn a_string_connector_on_a_cascade_deferred_block_does_not_abort_the_export() {
    // The harder half: `stringy` carries no enum of its own. It is deferred purely because its
    // only driver was, so the String arm has to read the deferred set rather than any local enum
    // property to stay silent here.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "enumsrc",
                &[],
                &[0],
                enum_param("controllerType", EnumClassId::SIMPLE_CONTROLLER),
            ),
            block(1, "CDL.Reals.Abs", "stringy", &[1], &[2], vec![]),
            block(
                2,
                "CDL.Reals.Sources.Constant",
                "keep",
                &[],
                &[3],
                real_param(7.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::In, ValueType::Real),
            conn(2, 1, Dir::Out, ValueType::String),
            conn(3, 2, Dir::Out, ValueType::Real),
        ],
        connections: vec![wire(0, 1)],
        ..ModelGraph::new()
    };

    let report = export_report(&g);
    assert_eq!(
        warning_subjects(&report),
        vec![iri("enumsrc"), iri("stringy")]
    );
    assert_emission(&report.bytes, &["keep"], &["enumsrc", "stringy"]);
    reimport_clean(&report.bytes);
}

#[test]
fn a_string_connector_on_a_surviving_block_still_rejects_alongside_a_deferral() {
    // The other arm, and the reason the deferral check has to be a condition rather than a
    // deletion: `stringy` survives, so its String output WOULD be emitted, carrying the
    // `S231:Real` placeholder — a document that lies about the signal type. A deferral elsewhere
    // in the graph must not buy it an exemption.
    let g = ModelGraph {
        blocks: vec![
            block(
                0,
                "CDL.Reals.Sources.Constant",
                "enumsrc",
                &[],
                &[0],
                enum_param("controllerType", EnumClassId::SIMPLE_CONTROLLER),
            ),
            block(
                1,
                "CDL.Reals.Sources.Constant",
                "stringy",
                &[],
                &[1],
                real_param(8.0),
            ),
        ],
        connectors: vec![
            conn(0, 0, Dir::Out, ValueType::Real),
            conn(1, 1, Dir::Out, ValueType::String),
        ],
        connections: vec![],
        ..ModelGraph::new()
    };

    let diags = export_rejection(&g);
    let errors: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "got: {diags:?}");
    assert_eq!(errors[0].code, DiagCode::ExportUnsupported);
    assert_eq!(errors[0].message, MSG_STRING_CONNECTOR);
    assert_eq!(errors[0].subject.as_deref(), Some(iri("stringy").as_str()));
}
