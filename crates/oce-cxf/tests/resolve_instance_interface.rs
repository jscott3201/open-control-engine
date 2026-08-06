//! Guard-scope pins for `hasInstance` member interfaces (`_spec/19` R19-15).
//!
//! Two families, both outside the contract corpus:
//!
//! - the **guard-scope pair**: a derivation-shaped child whose conditional member's guard names
//!   a value-carrying parameter member — the guard resolves through the shared declaration
//!   chain (site 3) instead of failing open, and the control naming an undeclared identifier
//!   refuses loudly with the member as subject;
//! - the **guard-scope duplication set**: six documents pinning the one-grounding-verdict
//!   invariant — one `(DiagCode, subject)` grounding verdict at most once per document, by
//!   architecture. The specialize invocation grounds silently everywhere; a composite's own
//!   chain emits from lowering, an instance's `hasParameter` ⧺ `hasConstant` from Step 7, a
//!   member from the derivation. The pruned-parent control pins a CLEAN zero-diagnostic load —
//!   a subject only the specialize pass reaches reports zero grounding verdicts — and its
//!   loud-guard twin pins that a failure that affects a pruning decision still refuses.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use serde_json::{Value as JsonValue, json};

const EX: &str = "http://example.org#";

fn ctx() -> JsonValue {
    json!({ "S231": "http://data.ashrae.org/S231P#", "base": "http://example.org#" })
}

fn iri(suffix: &str) -> String {
    format!("{EX}{suffix}")
}

fn r(suffix: &str) -> JsonValue {
    json!({ "@id": iri(suffix) })
}

fn import(doc: &JsonValue) -> Result<Vec<Diagnostic>, Vec<Diagnostic>> {
    let bytes = serde_json::to_vec(doc).expect("serialize test document");
    match import_cxf(&bytes, &ResolveOptions::default()) {
        Ok((_, report)) => Ok(report.diagnostics),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("unexpected non-validation failure: {other:?}"),
    }
}

fn error_with_subject(code: DiagCode, subject: &str, message: &str) -> Diagnostic {
    Diagnostic::error(code, message).with_subject(iri(subject))
}

/// Assert no `(DiagCode, subject)` grounding verdict appears more than once — the R19-15
/// invariant, checked over the grounding codes it governs.
fn assert_no_duplicate_grounding_verdicts(diags: &[Diagnostic], label: &str) {
    let mut seen = std::collections::HashSet::new();
    for d in diags {
        if matches!(
            d.code,
            DiagCode::GroundingFailed | DiagCode::UnresolvedReference
        ) {
            assert!(
                seen.insert((d.code, d.subject.clone())),
                "{label}: grounding verdict ({:?}, {:?}) reported more than once",
                d.code,
                d.subject
            );
        }
    }
}

// ---- guard-scope pair (R19-15 site 3) ---------------------------------------------------------

/// A derivation-shaped `CDL.Logical.Pre` whose conditional output member is guarded by its own
/// value-carrying parameter member `pre_u_start`. `guard` selects the identifier the
/// conditional expression names.
fn guarded_member_doc(guard: &str) -> JsonValue {
    json!({
        "@context": ctx(),
        "@graph": [
            {
                "@id": iri("M"),
                "@type": "S231:Block",
                "S231:hasInput": [ r("M.u") ],
                "S231:containsBlock": [ r("M.pre") ]
            },
            {
                "@id": iri("M.u"),
                "@type": "S231:BooleanInput",
                "S231:isOfDataType": { "@id": "S231:Boolean" },
                "S231:isConnectedTo": r("M.pre.u")
            },
            {
                "@id": iri("M.pre"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Pre",
                "S231:hasInstance": [ r("M.pre.u"), r("M.pre.y"), r("M.pre.pre_u_start") ]
            },
            {
                "@id": iri("M.pre.u"),
                "@type": "S231:BooleanInput",
                "S231:isOfDataType": { "@id": "S231:Boolean" }
            },
            {
                "@id": iri("M.pre.y"),
                "@type": "S231:BooleanOutput",
                "S231:isOfDataType": { "@id": "S231:Boolean" },
                "S231:isConditionalComponent": true,
                "S231:conditionalExpression": guard
            },
            { "@id": iri("M.pre.pre_u_start"), "@type": "S231:Parameter",
              "S231:isOfDataType": { "@id": "S231:Boolean" }, "S231:value": true }
        ]
    })
}

#[test]
fn member_guard_resolves_against_the_instances_value_carrying_members() {
    // Site 3: the guard scope gains the derivation-shaped parent's value-carrying members, so
    // `pre_u_start` resolves (true → the member stays active) and the document loads with zero
    // diagnostics — without the site the guard fails open LOUDLY with an unknown-parameter
    // Error, which is exactly what the control below pins.
    assert_eq!(
        import(&guarded_member_doc("pre_u_start")),
        Ok(vec![]),
        "a member guard naming a value-carrying parameter member must resolve cleanly"
    );
}

#[test]
fn member_guard_naming_an_undeclared_identifier_refuses_with_the_member_subject() {
    assert_eq!(
        import(&guarded_member_doc("nope")),
        Err(vec![error_with_subject(
            DiagCode::ConditionalGuardUnknownParameter,
            "M.pre.y",
            "conditional guard references unknown parameter `nope`",
        )]),
        "an unknown guard identifier fails open on the decision but still refuses loudly"
    );
}

/// A ROOT whose `hasInstance` list carries a valued `gate`, with a conditional `containsBlock`
/// child guarded by `gate`. The root declares `hasInput`/`hasOutput`, so it is not
/// derivation-shaped and R19-2 rules its list inert on every path — never the subject and never
/// the cause of a diagnostic. This document discriminates site 3's SCOPE, which the narrowing
/// direction alone cannot: widen the site to every parent and the inert list binds `gate = false`,
/// silently pruning the child.
fn root_list_guard_doc() -> JsonValue {
    json!({ "@context": ctx(), "@graph": [
        { "@id": iri("M"), "@type": "S231:Block",
          "S231:hasInstance": [ r("M.gate") ],
          "S231:containsBlock": [ r("M.n") ],
          "S231:hasInput": [ r("M.u") ], "S231:hasOutput": [ r("M.y") ] },
        { "@id": iri("M.gate"), "@type": "S231:Parameter",
          "S231:isOfDataType": { "@id": "S231:Boolean" }, "S231:value": false },
        { "@id": iri("M.u"), "@type": "S231:BooleanInput",
          "S231:isOfDataType": { "@id": "S231:Boolean" }, "S231:isConnectedTo": r("M.n.u") },
        { "@id": iri("M.y"), "@type": "S231:BooleanOutput",
          "S231:isOfDataType": { "@id": "S231:Boolean" } },
        { "@id": iri("M.n"), "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Not",
          "S231:isConditionalComponent": true, "S231:conditionalExpression": "gate",
          "S231:hasInput": [ r("M.n.u") ], "S231:hasOutput": [ r("M.n.y") ] },
        { "@id": iri("M.n.u"), "@type": "S231:BooleanInput",
          "S231:isOfDataType": { "@id": "S231:Boolean" } },
        { "@id": iri("M.n.y"), "@type": "S231:BooleanOutput",
          "S231:isOfDataType": { "@id": "S231:Boolean" }, "S231:isConnectedTo": r("M.y") }
    ]})
}

#[test]
fn a_roots_inert_member_list_never_reaches_a_conditional_guard() {
    // Site 3's scope restriction, pinned in the WIDENING direction. The narrowing probe (delete
    // the site) is already covered by the two tests above; nothing pinned the other side, so
    // widening the site to every parent passed the whole suite. Under the widening this document
    // stops refusing and instead prunes: InactiveConditionalNode on both child ports plus
    // UndrivenBoundaryOutput on the root's output — an ACCEPTANCE change caused by a list R19-2
    // rules inert on every path.
    assert_eq!(
        import(&root_list_guard_doc()),
        Err(vec![error_with_subject(
            DiagCode::ConditionalGuardUnknownParameter,
            "M.n",
            "conditional guard references unknown parameter `gate`",
        )]),
        "a root's inert member list must not bind its conditional child's guard"
    );
}

// ---- guard-scope duplication set (R19-15, the §5 six-document set) ----------------------------

/// The authored pair's base document: child `gain` declares `hasInput`/`hasOutput` and a
/// VALUELESS `hasParameter` `k`; the twin marks the child's own output conditional (no guard
/// expression).
fn authored_doc(conditional_output: bool) -> JsonValue {
    let mut gain_y = json!({
        "@id": iri("M.gain.y"),
        "@type": "S231:RealOutput",
        "S231:isOfDataType": { "@id": "S231:Real" },
        "S231:isConnectedTo": r("M.y")
    });
    if conditional_output {
        gain_y["S231:isConditionalComponent"] = json!(true);
    }
    json!({
        "@context": ctx(),
        "@graph": [
            {
                "@id": iri("M"),
                "@type": "S231:Block",
                "S231:containsBlock": [ r("M.gain") ],
                "S231:hasInput": [ r("M.u") ],
                "S231:hasOutput": [ r("M.y") ]
            },
            {
                "@id": iri("M.u"),
                "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": r("M.gain.u")
            },
            {
                "@id": iri("M.y"),
                "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            },
            {
                "@id": iri("M.gain"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.MultiplyByParameter",
                "S231:hasInput": [ r("M.gain.u") ],
                "S231:hasOutput": [ r("M.gain.y") ],
                "S231:hasParameter": [ r("M.gain.k") ]
            },
            {
                "@id": iri("M.gain.u"),
                "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" }
            },
            gain_y,
            {
                "@id": iri("M.gain.k"),
                "@type": "S231:Parameter",
                "S231:isOfDataType": { "@id": "S231:Real" }
            }
        ]
    })
}

/// The derivation-shaped pair: the authored pair's documents with the child's ports spelled on
/// `S231:hasInstance` (no `hasInput`/`hasOutput` on the child), the valueless `hasParameter`
/// retained.
fn derivation_shaped_doc(conditional_output: bool) -> JsonValue {
    let mut doc = authored_doc(conditional_output);
    let graph = doc["@graph"].as_array_mut().expect("@graph array");
    let gain = graph
        .iter_mut()
        .find(|n| n["@id"].as_str() == Some(&iri("M.gain")))
        .expect("gain node");
    let obj = gain.as_object_mut().expect("gain object");
    let u = obj.remove("S231:hasInput").expect("hasInput");
    let y = obj.remove("S231:hasOutput").expect("hasOutput");
    let members: Vec<JsonValue> = u
        .as_array()
        .expect("hasInput array")
        .iter()
        .chain(y.as_array().expect("hasOutput array"))
        .cloned()
        .collect();
    obj.insert("S231:hasInstance".to_owned(), JsonValue::Array(members));
    doc
}

/// The pruned-parent control: `sub` is pruned by a false guard on the root's `flag`; `sub`
/// carries a valueless `hasParameter` `q` and its own conditional output guarded by its
/// grounded `flag2`, so the specialize pass is the ONLY pass to reach `q` — and it grounds
/// silently, so the document loads clean.
fn pruned_parent_doc() -> JsonValue {
    json!({
        "@context": ctx(),
        "@graph": [
            {
                "@id": iri("M"),
                "@type": "S231:Block",
                "S231:hasParameter": [ r("M.flag") ],
                "S231:containsBlock": [ r("M.sub"), r("M.pass") ],
                "S231:hasInput": [ r("M.u") ],
                "S231:hasOutput": [ r("M.y") ]
            },
            { "@id": iri("M.flag"), "@type": "S231:Parameter",
              "S231:isOfDataType": { "@id": "S231:Boolean" }, "S231:value": false },
            {
                "@id": iri("M.u"),
                "@type": "S231:BooleanInput",
                "S231:isOfDataType": { "@id": "S231:Boolean" },
                "S231:isConnectedTo": r("M.pass.u")
            },
            {
                "@id": iri("M.y"),
                "@type": "S231:BooleanOutput",
                "S231:isOfDataType": { "@id": "S231:Boolean" }
            },
            {
                "@id": iri("M.sub"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Not",
                "S231:isConditionalComponent": true,
                "S231:conditionalExpression": "flag",
                "S231:hasInput": [ r("M.sub.u") ],
                "S231:hasOutput": [ r("M.sub.y") ],
                "S231:hasParameter": [ r("M.sub.q"), r("M.sub.flag2") ]
            },
            { "@id": iri("M.sub.q"), "@type": "S231:Parameter",
              "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": iri("M.sub.flag2"), "@type": "S231:Parameter",
              "S231:isOfDataType": { "@id": "S231:Boolean" }, "S231:value": true },
            {
                "@id": iri("M.sub.u"),
                "@type": "S231:BooleanInput",
                "S231:isOfDataType": { "@id": "S231:Boolean" }
            },
            {
                "@id": iri("M.sub.y"),
                "@type": "S231:BooleanOutput",
                "S231:isOfDataType": { "@id": "S231:Boolean" },
                "S231:isConditionalComponent": true,
                "S231:conditionalExpression": "flag2"
            },
            {
                "@id": iri("M.pass"),
                "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Not",
                "S231:hasInput": [ r("M.pass.u") ],
                "S231:hasOutput": [ r("M.pass.y") ]
            },
            {
                "@id": iri("M.pass.u"),
                "@type": "S231:BooleanInput",
                "S231:isOfDataType": { "@id": "S231:Boolean" }
            },
            {
                "@id": iri("M.pass.y"),
                "@type": "S231:BooleanOutput",
                "S231:isOfDataType": { "@id": "S231:Boolean" },
                "S231:isConnectedTo": r("M.y")
            }
        ]
    })
}

/// The loud-guard twin: the pruned-parent control with the conditional member's guard naming
/// the VALUELESS parameter `q` instead of the grounded `flag2`.
fn loud_guard_twin() -> JsonValue {
    let mut doc = pruned_parent_doc();
    let graph = doc["@graph"].as_array_mut().expect("@graph array");
    let sub_y = graph
        .iter_mut()
        .find(|n| n["@id"].as_str() == Some(&iri("M.sub.y")))
        .expect("sub.y node");
    sub_y["S231:conditionalExpression"] = json!("q");
    doc
}

#[test]
fn authored_pair_reports_each_grounding_verdict_once() {
    let base = import(&authored_doc(false)).expect_err("valueless k refuses");
    assert_eq!(
        base,
        vec![error_with_subject(
            DiagCode::GroundingFailed,
            "M.gain.k",
            "parameter has no value (Ground mode)",
        )],
        "the no-conditional document reports the failure exactly once, from Step 7"
    );
    let twin = import(&authored_doc(true)).expect_err("the conditional twin refuses");
    assert_eq!(
        twin,
        vec![
            error_with_subject(
                DiagCode::ConditionalGuardUnsupported,
                "M.gain.y",
                "conditional node is missing S231:conditionalExpression",
            ),
            error_with_subject(
                DiagCode::GroundingFailed,
                "M.gain.k",
                "parameter has no value (Ground mode)",
            ),
        ],
        "the conditional twin adds the guard verdict and nothing else — the specialize pass \
         grounds the chain silently, so the GroundingFailed still appears exactly once"
    );
    assert_no_duplicate_grounding_verdicts(&twin, "authored twin");
}

#[test]
fn derivation_shaped_pair_reports_each_grounding_verdict_once() {
    let base = import(&derivation_shaped_doc(false)).expect_err("valueless k refuses");
    let twin = import(&derivation_shaped_doc(true)).expect_err("the conditional twin refuses");
    assert_eq!(
        base,
        vec![error_with_subject(
            DiagCode::GroundingFailed,
            "M.gain.k",
            "parameter has no value (Ground mode)",
        )],
        "the derived interface resolves every port, leaving exactly the one grounding verdict"
    );
    assert_eq!(
        twin,
        vec![
            error_with_subject(
                DiagCode::ConditionalGuardUnsupported,
                "M.gain.y",
                "conditional node is missing S231:conditionalExpression",
            ),
            error_with_subject(
                DiagCode::GroundingFailed,
                "M.gain.k",
                "parameter has no value (Ground mode)",
            ),
        ],
        "site 1 makes the conditional member a candidate, `complete_scope` runs on the \
         instance — and the GroundingFailed still appears exactly once, Step 7 its one \
         emitting owner"
    );
    assert_no_duplicate_grounding_verdicts(&twin, "derivation-shaped twin");
}

#[test]
fn pruned_parent_control_loads_clean_and_its_loud_guard_twin_refuses() {
    // A subject only the specialize pass reaches reports ZERO grounding verdicts — the ruled
    // zero — while a failure that affects a pruning decision stays loud: the twin's guard
    // names the valueless `q` and refuses with the member as subject.
    assert_eq!(
        import(&pruned_parent_doc()),
        Ok(vec![]),
        "the pruned parent's valueless parameter is reached only by the silent specialize \
         pass; the document loads with zero diagnostics"
    );
    assert_eq!(
        import(&loud_guard_twin()),
        Err(vec![error_with_subject(
            DiagCode::ConditionalGuardUnknownParameter,
            "M.sub.y",
            "conditional guard references unknown parameter `q`",
        )]),
        "a guard failure that affects a pruning decision still refuses, member as subject"
    );
}
