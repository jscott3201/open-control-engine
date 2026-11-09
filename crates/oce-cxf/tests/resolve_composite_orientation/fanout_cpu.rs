//! CPU bounds on long connection sources fanned out to many targets (GitHub issue #479).
//!
//! Resolve Step 9 once classified both ends of every edge by string content, so one long source
//! shared by a whole fan-out cost fan-out × source length: a 1 MiB derivation-skipped synthesized
//! source fanned out to 65,535 targets took about a minute of release CPU (and about ten minutes
//! in debug) before it was refused with ordinary diagnostics. Step 9 now interns each distinct
//! endpoint once, so these documents resolve in time linear in their size.
//!
//! The 30 s wall-clock bound per import is deliberately generous — it guards the complexity class,
//! not a speed. With the fix a 1 MiB × 65,535 import takes about 1 s in debug (about 0.08 s in
//! release) on a developer machine; allowing roughly 3× for a Windows cell or a loaded CI runner
//! still leaves about 10× headroom. Before the fix the same import took about 71 s in release and
//! about ten minutes in debug, so the pre-fix code fails the bound in either profile. Each test
//! builds at most three 10 MB documents so it stays well under 10 s in debug.

use std::time::{Duration, Instant};

use super::synthesized::{FanoutTarget, derived_output_document};
use super::*;

/// One mebibyte of source-IRI padding.
const MIB: usize = 1024 * 1024;

/// The examined-target limit of the composite boundary traversal.
const TARGET_LIMIT: usize = 65_536;

/// Generous bound on one import; see the module docs.
const IMPORT_BOUND: Duration = Duration::from_secs(30);

/// A derivation-skipped synthesized source whose owner IRI carries `padding` bytes, re-anchored
/// onto `fanout` copies of one target of `target_kind`. Returns the document and the source IRI.
fn skipped_source_fanout(
    target_kind: FanoutTarget,
    padding: usize,
    fanout: usize,
) -> (Value, String) {
    let mut document = derived_output_document(false, false);
    let owner = format!("http://example.org#{}.con", "x".repeat(padding));
    let parameter = format!("{owner}.k");
    let output = format!("{owner}.y");
    node_mut(&mut document, ".sub")["S231:containsBlock"] = json!({ "@id": owner });
    node_mut(&mut document, ".sub.con.k")["@id"] = json!(parameter);
    let instance = node_mut(&mut document, ".sub.con");
    instance["@id"] = json!(owner);
    // Listing the output twice makes the derivation refuse the instance, so its output is a
    // node-less source no connector answers for and whose bytes no budget charges.
    instance["S231:hasInstance"] = json!([
        { "@id": parameter },
        { "@id": output },
        { "@id": output }
    ]);
    let target = match target_kind {
        FanoutTarget::LeafInput => "http://example.org#derived_output.post.u",
        FanoutTarget::BoundaryInput => {
            node_mut(&mut document, "#derived_output")["S231:hasInput"] =
                json!({ "@id": "http://example.org#derived_output.u" });
            push_node(
                &mut document,
                json!({
                    "@id": "http://example.org#derived_output.u",
                    "@type": "S231:RealInput",
                    "S231:isOfDataType": { "@id": "S231:Real" }
                }),
            );
            "http://example.org#derived_output.u"
        }
        FanoutTarget::BoundaryOutput => {
            node_mut(&mut document, "#derived_output")["S231:hasOutput"] =
                json!({ "@id": "http://example.org#derived_output.y" });
            push_node(
                &mut document,
                json!({
                    "@id": "http://example.org#derived_output.y",
                    "@type": "S231:RealOutput",
                    "S231:isOfDataType": { "@id": "S231:Real" }
                }),
            );
            "http://example.org#derived_output.y"
        }
    };
    let mut targets = vec![json!({ "@id": output })];
    targets.extend(std::iter::repeat_n(json!({ "@id": target }), fanout));
    node_mut(&mut document, ".sub.y")["S231:isConnectedTo"] = Value::Array(targets);
    (document, output)
}

fn push_node(document: &mut Value, node: Value) {
    document["@graph"]
        .as_array_mut()
        .expect("@graph")
        .push(node);
}

/// Import `document`, returning its refusal and how long the import took.
fn timed_refusal(document: &Value, label: &str) -> (Vec<Diagnostic>, Duration) {
    let bytes = serde_json::to_vec(document).expect("serialize");
    let started = Instant::now();
    let result = import_cxf(&bytes, &ResolveOptions::default());
    let elapsed = started.elapsed();
    match result {
        Err(CxfError::Validation(diagnostics)) => (diagnostics, elapsed),
        Ok(_) => panic!("{label}: the document must be refused"),
        Err(error) => panic!("{label}: unexpected import error: {error:?}"),
    }
}

fn missing_message(target_kind: FanoutTarget) -> &'static str {
    match target_kind {
        FanoutTarget::LeafInput => "connection source not found",
        FanoutTarget::BoundaryInput => "boundary-input target not found",
        FanoutTarget::BoundaryOutput => "boundary-output source not found",
    }
}

fn kind_name(target_kind: FanoutTarget) -> &'static str {
    match target_kind {
        FanoutTarget::LeafInput => "leaf input",
        FanoutTarget::BoundaryInput => "boundary input",
        FanoutTarget::BoundaryOutput => "boundary output",
    }
}

/// Assert the at-limit verdict: the long source is reported once, as an ordinary unresolved
/// endpoint sharing one subject allocation, and nothing trips a resource bound.
fn assert_reported_once(target_kind: FanoutTarget, source: &str, diagnostics: &[Diagnostic]) {
    let label = kind_name(target_kind);
    let missing = diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == DiagCode::UnresolvedReference
                && diagnostic.message == missing_message(target_kind)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        missing.len(),
        1,
        "{label}: {} diagnostics",
        diagnostics.len()
    );
    assert_eq!(missing[0].subject.as_deref(), Some(source), "{label}");
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagCode::MalformedDocument),
        "{label}: an at-limit fan-out must not trip a resource bound"
    );
}

/// A 1 MiB skipped source fanned out to the examined-target limit resolves to its ordinary
/// diagnostics within the bound, for every target role. Before the fix each import took about a
/// minute in release.
#[test]
fn mebibyte_skipped_source_at_the_target_limit_resolves_within_the_cpu_bound() {
    for target_kind in ROLES {
        let (document, source) = skipped_source_fanout(target_kind, MIB, TARGET_LIMIT - 1);
        let (diagnostics, elapsed) = timed_refusal(&document, kind_name(target_kind));
        assert_reported_once(target_kind, &source, &diagnostics);
        assert!(
            elapsed < IMPORT_BOUND,
            "{}: import took {elapsed:?}, bound {IMPORT_BOUND:?}",
            kind_name(target_kind)
        );
    }
}

const ROLES: [FanoutTarget; 3] = [
    FanoutTarget::LeafInput,
    FanoutTarget::BoundaryInput,
    FanoutTarget::BoundaryOutput,
];

fn over_limit() -> Diagnostic {
    Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "composite boundary resolution exceeds the supported target examination count \
             ({TARGET_LIMIT})"
        ),
    )
}

/// Assert that a fan-out one past the examined-target limit is refused by the target-count bound
/// within the CPU bound.
fn assert_refused_past_the_limit(target_kind: FanoutTarget, padding: usize) {
    let label = format!("{} padding={padding}", kind_name(target_kind));
    let (document, _) = skipped_source_fanout(target_kind, padding, TARGET_LIMIT);
    let (diagnostics, elapsed) = timed_refusal(&document, &label);
    assert!(
        diagnostics.contains(&over_limit()),
        "{label}: {} diagnostics, none the target-count refusal",
        diagnostics.len()
    );
    assert!(elapsed < IMPORT_BOUND, "{label} over limit: {elapsed:?}");
}

/// A 1 MiB skipped source fanned out one past the examined-target limit is refused by the
/// target-count bound, for every target role, within the CPU bound. The source bytes are never
/// charged, so the verdict is the one a short source gets.
#[test]
fn mebibyte_skipped_source_past_the_target_limit_is_refused_within_the_cpu_bound() {
    for target_kind in ROLES {
        assert_refused_past_the_limit(target_kind, MIB);
    }
}

/// A one-byte-padded skipped source at the examined-target limit keeps the same ordinary verdict
/// as the 1 MiB one. (Past the limit only the 1 MiB source is swept: the over-limit refusal comes
/// from the lowering target budget, which never charges the source, and each extra
/// 65,536-target document costs about a second in debug.)
#[test]
fn short_skipped_source_keeps_the_at_limit_verdict() {
    for target_kind in ROLES {
        let label = kind_name(target_kind);
        let (document, source) = skipped_source_fanout(target_kind, 1, TARGET_LIMIT - 1);
        let (diagnostics, elapsed) = timed_refusal(&document, label);
        assert_reported_once(target_kind, &source, &diagnostics);
        assert!(elapsed < IMPORT_BOUND, "{label} at limit: {elapsed:?}");
    }
}

/// Resolving one fan-out document twice in one process yields identical diagnostic vectors, for
/// every target role: endpoint ids and the source memo never leak into the output.
#[test]
fn repeated_fanout_import_reports_identical_diagnostics() {
    for target_kind in ROLES {
        let label = kind_name(target_kind);
        let (document, _) = skipped_source_fanout(target_kind, 64, 16);
        let (first, _) = timed_refusal(&document, label);
        let (second, _) = timed_refusal(&document, label);
        assert!(!first.is_empty(), "{label}");
        assert_eq!(
            first, second,
            "{label}: repeated import changed the diagnostics"
        );
    }
}
