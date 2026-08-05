//! Two-pass emission-policy pins for the shared own-declaration mechanism (issue #240):
//! tagged findings surface once per import — from the lowering view when both passes visit a
//! chain, through the withheld release when only the specialize pass does (R20-7) — and only
//! for COMPOSITE chains at the specialize invocation (R20-9): a leaf chain's cycle or
//! duplicate participants fail to ground there silently, leaving any refusal to the fenced
//! member-level machinery. Scope-resolution, permutation, and probe-graduation pins live in
//! `resolve_declaration_scope.rs`, whose import helpers these mirror.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{ModelGraph, Value};
use serde_json::{Value as Json, json};

fn import_json(doc: &Json) -> Result<(ModelGraph, Vec<Diagnostic>), Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(doc).expect("serializable test document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((graph, report)) => Ok((graph, report.diagnostics)),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("unexpected non-validation import failure: {other:?}"),
    }
}

fn import_clean(doc: &Json) -> ModelGraph {
    match import_json(doc) {
        Ok((graph, diags)) => {
            assert!(
                diags.is_empty(),
                "expected a warning-free load, got {diags:?}"
            );
            graph
        }
        Err(diags) => panic!("expected a clean load, got a refusal: {diags:#?}"),
    }
}

/// A composite chain BOTH passes visit (the root carries a conditional contained block AND a
/// declaration cycle) reports the tagged finding exactly once per import, from the lowering
/// view (R20-7 suppression direction — the specialize pass's withheld copy must not release).
#[test]
fn cycle_chain_visited_by_both_passes_emits_exactly_one_tagged_diagnostic() {
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#R", "@type": "S231:Block",
              "S231:hasParameter": [
                  { "@id": "http://example.org#R.a" },
                  { "@id": "http://example.org#R.b" },
                  { "@id": "http://example.org#R.have_x" } ],
              "S231:containsBlock": [
                  { "@id": "http://example.org#R.con" },
                  { "@id": "http://example.org#R.condblk" } ],
              "S231:hasOutput": { "@id": "http://example.org#R.y" } },
            { "@id": "http://example.org#R.a", "S231:value": "b + 1.0" },
            { "@id": "http://example.org#R.b", "S231:value": "a + 1.0" },
            { "@id": "http://example.org#R.have_x", "S231:value": false },
            { "@id": "http://example.org#R.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": { "@id": "http://example.org#R.con.k" },
              "S231:hasOutput": { "@id": "http://example.org#R.con.y" } },
            { "@id": "http://example.org#R.con.k", "S231:value": 1.0 },
            { "@id": "http://example.org#R.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConnectedTo": { "@id": "http://example.org#R.y" } },
            { "@id": "http://example.org#R.condblk",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:isConditionalComponent": true,
              "S231:conditionalExpression": "have_x",
              "S231:hasParameter": { "@id": "http://example.org#R.condblk.k" },
              "S231:hasOutput": { "@id": "http://example.org#R.condblk.y" } },
            { "@id": "http://example.org#R.condblk.k", "S231:value": 2.0 },
            { "@id": "http://example.org#R.condblk.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": "http://example.org#R.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diags = import_json(&doc).expect_err("the cycle refuses the import");
    let rendered: Vec<(DiagCode, Option<&str>, &str)> = diags
        .iter()
        .map(|d| (d.code, d.subject.as_deref(), d.message.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![(
            DiagCode::MalformedDocument,
            Some("http://example.org#R.a"),
            "composite/declaration-cycle: cycle in the block's own declaration references: \
             http://example.org#R.a -> http://example.org#R.b -> http://example.org#R.a",
        )],
        "one tagged diagnostic per import — the lowering view emits, the withheld copy stays \
         suppressed"
    );
}
/// Withheld findings survive a root-classification failure (here: two candidate roots): the
/// lowering pass evaluates no chain at all, so every specialize-pass tagged finding releases.
#[test]
fn withheld_findings_release_when_root_classification_leaves_no_chain_evaluated() {
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:hasParameter": { "@id": "http://example.org#M.x" },
              "S231:containsBlock": [ { "@id": "http://example.org#M.condblk" } ] },
            { "@id": "http://example.org#M2", "@type": "S231:Block",
              "S231:containsBlock": [ { "@id": "http://example.org#M.condblk" } ] },
            { "@id": "http://example.org#M.x", "S231:value": "x" },
            { "@id": "http://example.org#M.condblk",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:isConditionalComponent": true,
              "S231:conditionalExpression": "have_x",
              "S231:hasParameter": { "@id": "http://example.org#M.condblk.k" },
              "S231:hasOutput": { "@id": "http://example.org#M.condblk.y" } },
            { "@id": "http://example.org#M.condblk.k", "S231:value": 1.0 },
            { "@id": "http://example.org#M.condblk.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diags = import_json(&doc).expect_err("refuses");
    assert!(
        diags
            .iter()
            .any(|d| d.message.starts_with("composite/root-count: ")),
        "root classification fails first: {diags:#?}"
    );
    assert!(
        diags.iter().any(|d| d.message
            == "composite/declaration-cycle: cycle in the block's own declaration references: \
                http://example.org#M.x -> http://example.org#M.x"),
        "the self-reference finding the specialize pass computed for M's chain must not vanish \
         on the no-root path: {diags:#?}"
    );
}
/// A COMPOSITE declaration cycle visited only by the specialize pass: the composite is pruned
/// by a false guard, so `collect_leaves` never evaluates its chain, yet the specialize pass
/// did (the composite carries a conditional child of its own, so guard evaluation grounds its
/// chain, `@graph`-wide and unfiltered). The tagged finding must still surface — withheld by
/// the specialize pass and released after lowering (R20-7). This is the sole-visitor release
/// shape that remains tagged under R20-9, which scopes the tagged rules to composite chains.
#[test]
fn pruned_composite_chain_cycle_still_surfaces_its_tagged_finding() {
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:hasParameter": { "@id": "http://example.org#M.have_sub" },
              "S231:containsBlock": [
                  { "@id": "http://example.org#M.con" },
                  { "@id": "http://example.org#M.sub" } ] },
            { "@id": "http://example.org#M.have_sub", "S231:value": false },
            { "@id": "http://example.org#M.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": { "@id": "http://example.org#M.con.k" },
              "S231:hasOutput": { "@id": "http://example.org#M.con.y" } },
            { "@id": "http://example.org#M.con.k", "S231:value": 1.0 },
            { "@id": "http://example.org#M.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": "http://example.org#M.sub",
              "@type": "http://example.org#Vendor.Sequences.Inner",
              "S231:isConditionalComponent": true,
              "S231:conditionalExpression": "have_sub",
              "S231:hasParameter": [
                  { "@id": "http://example.org#M.sub.a" },
                  { "@id": "http://example.org#M.sub.b" },
                  { "@id": "http://example.org#M.sub.have_inner" } ],
              "S231:containsBlock": [ { "@id": "http://example.org#M.sub.inner" } ] },
            { "@id": "http://example.org#M.sub.a", "S231:value": "b" },
            { "@id": "http://example.org#M.sub.b", "S231:value": "a" },
            { "@id": "http://example.org#M.sub.have_inner", "S231:value": false },
            { "@id": "http://example.org#M.sub.inner",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:isConditionalComponent": true,
              "S231:conditionalExpression": "have_inner",
              "S231:hasParameter": { "@id": "http://example.org#M.sub.inner.k" },
              "S231:hasOutput": { "@id": "http://example.org#M.sub.inner.y" } },
            { "@id": "http://example.org#M.sub.inner.k", "S231:value": 1.0 },
            { "@id": "http://example.org#M.sub.inner.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diags = import_json(&doc).expect_err("the pruned composite's cycle refuses the import");
    let rendered: Vec<(DiagCode, Option<&str>, &str)> = diags
        .iter()
        .map(|d| (d.code, d.subject.as_deref(), d.message.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![(
            DiagCode::MalformedDocument,
            Some("http://example.org#M.sub.a"),
            "composite/declaration-cycle: cycle in the block's own declaration references: \
             http://example.org#M.sub.a -> http://example.org#M.sub.b -> \
             http://example.org#M.sub.a",
        )],
        "the withheld tagged finding of the specialize-only composite chain is released once"
    );
}

/// A LEAF declaration cycle at the specialize invocation produces NO tagged finding (R20-9 —
/// a leaf chain's values are member modifications, a level the own-declarations model does not
/// govern): the cycle participants simply fail to ground there, silently, and the refusal that
/// remains is the fenced member-level Step-7 machinery's own pair of loud grounding failures.
#[test]
fn leaf_chain_cycle_stays_untagged_and_refuses_through_member_level_grounding_failures() {
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:containsBlock": [ { "@id": "http://example.org#M.con" } ] },
            { "@id": "http://example.org#M.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": [
                  { "@id": "http://example.org#M.con.k" },
                  { "@id": "http://example.org#M.con.a" },
                  { "@id": "http://example.org#M.con.b" },
                  { "@id": "http://example.org#M.con.have_hol" } ],
              "S231:hasInput": [ { "@id": "http://example.org#M.con.uHol" } ],
              "S231:hasOutput": { "@id": "http://example.org#M.con.y" } },
            { "@id": "http://example.org#M.con.k", "S231:value": 1.0 },
            { "@id": "http://example.org#M.con.a", "S231:value": "b" },
            { "@id": "http://example.org#M.con.b", "S231:value": "a" },
            { "@id": "http://example.org#M.con.have_hol", "S231:value": false },
            { "@id": "http://example.org#M.con.uHol", "@type": "S231:RealInput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConditionalComponent": true,
              "S231:conditionalExpression": "have_hol" },
            { "@id": "http://example.org#M.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let diags = import_json(&doc).expect_err("the member-level failures refuse the import");
    let rendered: Vec<(DiagCode, Option<&str>, &str)> = diags
        .iter()
        .map(|d| (d.code, d.subject.as_deref(), d.message.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            (
                DiagCode::GroundingFailed,
                Some("http://example.org#M.con.a"),
                "expression binding did not ground: unknown identifier: b",
            ),
            (
                DiagCode::GroundingFailed,
                Some("http://example.org#M.con.b"),
                "expression binding did not ground: unknown identifier: a",
            ),
        ],
        "no composite/declaration-cycle finding for a leaf chain; only Step 7's own machinery \
         fires"
    );
}

/// A LEAF duplicate local name at the specialize invocation likewise produces no tagged
/// finding (R20-9): the later occurrence is refused silently in the guard scope, the guard
/// still grounds, and the document loads — both occurrences reach the fenced member level,
/// which grounds them in member order exactly as it always has.
#[test]
fn leaf_chain_duplicate_declaration_stays_untagged_and_the_document_loads() {
    let doc = json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" },
        "@graph": [
            { "@id": "http://example.org#M", "@type": "S231:Block",
              "S231:containsBlock": [ { "@id": "http://example.org#M.con" } ] },
            { "@id": "http://example.org#M.con",
              "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
              "S231:hasParameter": [
                  { "@id": "http://example.org#M.con.k" },
                  { "@id": "http://example.org#M.con.settings.k" },
                  { "@id": "http://example.org#M.con.have_hol" } ],
              "S231:hasInput": [ { "@id": "http://example.org#M.con.uHol" } ],
              "S231:hasOutput": { "@id": "http://example.org#M.con.y" } },
            { "@id": "http://example.org#M.con.k", "S231:value": 1.0 },
            { "@id": "http://example.org#M.con.settings.k", "S231:value": 2.0 },
            { "@id": "http://example.org#M.con.have_hol", "S231:value": false },
            { "@id": "http://example.org#M.con.uHol", "@type": "S231:RealInput",
              "S231:isOfDataType": { "@id": "S231:Real" },
              "S231:isConditionalComponent": true,
              "S231:conditionalExpression": "have_hol" },
            { "@id": "http://example.org#M.con.y", "@type": "S231:RealOutput",
              "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    });
    let graph = import_clean(&doc);
    let params: Vec<(&str, &Value)> = graph.blocks[0]
        .params
        .values
        .iter()
        .map(|(name, value)| (name.as_ref(), value))
        .collect();
    assert_eq!(
        params.len(),
        3,
        "both k occurrences and have_hol: {params:?}"
    );
    assert_eq!(params[0].0, "k");
    assert!(params[0].1.bit_eq(&Value::Real(1.0)));
    assert_eq!(params[1].0, "k");
    assert!(
        params[1].1.bit_eq(&Value::Real(2.0)),
        "the member level keeps grounding both occurrences in member order (fenced behavior)"
    );
    assert_eq!(params[2].0, "have_hol");
}
