//! CPU bounds on long connection sources fanned out to many targets (GitHub issue #479).
//!
//! Resolve Step 9 once classified both ends of every edge by string content, so one long source
//! shared by a whole fan-out cost fan-out × source length: a 1 MiB derivation-skipped synthesized
//! source fanned out to 65,535 targets took about a minute of release CPU (and about ten minutes
//! in debug) before it was refused with ordinary diagnostics. Step 9 now interns each distinct
//! endpoint once, so these documents resolve in time linear in their size.
//!
//! The wall-clock bound is deliberately generous — it guards the complexity class, not a speed:
//! the pre-fix code exceeds it by more than an order of magnitude in either profile, and the
//! fixed code clears it by more than an order of magnitude in debug.

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
    for target_kind in [
        FanoutTarget::LeafInput,
        FanoutTarget::BoundaryInput,
        FanoutTarget::BoundaryOutput,
    ] {
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

/// Across source lengths, a fan-out at the examined-target limit keeps its ordinary verdict and
/// one past it is refused by the target-count bound, each within the CPU bound. The source bytes
/// are never charged, so the verdict at each fan-out is the same for every source length.
#[test]
fn source_length_sweep_keeps_target_limit_verdicts_within_the_cpu_bound() {
    let over_limit = Diagnostic::error(
        DiagCode::MalformedDocument,
        format!(
            "composite boundary resolution exceeds the supported target examination count \
             ({TARGET_LIMIT})"
        ),
    );
    for padding in [1, MIB] {
        for target_kind in [FanoutTarget::LeafInput, FanoutTarget::BoundaryOutput] {
            let label = format!("{} padding={padding}", kind_name(target_kind));
            let (document, source) = skipped_source_fanout(target_kind, padding, TARGET_LIMIT - 1);
            let (diagnostics, elapsed) = timed_refusal(&document, &label);
            assert_reported_once(target_kind, &source, &diagnostics);
            assert!(elapsed < IMPORT_BOUND, "{label} at limit: {elapsed:?}");

            let (document, _) = skipped_source_fanout(target_kind, padding, TARGET_LIMIT);
            let (diagnostics, elapsed) = timed_refusal(&document, &label);
            assert!(
                diagnostics.contains(&over_limit),
                "{label} over limit: {diagnostics:?}"
            );
            assert!(elapsed < IMPORT_BOUND, "{label} over limit: {elapsed:?}");
        }
    }
}
