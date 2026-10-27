//! CDL-legal feedback into a proportional-only reset PID's `trigger` is refused at load with the
//! `block-algebraic-loop` code (GitHub issue #347).
//!
//! `CDL.Reals.PIDWithReset` with `controllerType = P` holds no state, so it is an algebraic block
//! that reads `trigger` when it fires, yet `y` depends only on `u_s` and `u_m` (upstream
//! `PIDWithReset.mo` connects `trigger` only to the integrator, which P mode lacks). Feeding `y`
//! back through a threshold into `trigger` leaves the connector graph acyclic, but atomic block
//! firing cannot place the controller. In every other mode the controller is stateful and
//! `trigger` is read only by the state update, so the same model loads.

use serde_json::{Value, json};

use super::algebraic_loop_diagnostics::{build_message, loop_order, refuse, render};
use crate::{DiagnosticSeverity, DiagnosticStage, DiagnosticSubject, Engine};

/// The P-mode refusal's receipt.
const RESET_PID_P_BLOCK_LOOP_GOLDEN: &str =
    include_str!("fixtures/golden/reset_pid_p_block_loop.receipt.txt");

const PREFIX: &str = "http://example.org#rst.";

fn iri(local: &str) -> String {
    format!("{PREFIX}{local}")
}

/// `us → pid.u_s`, `um → pid.u_m`, `pid.y → y` and `pid.y → thr.u`; with `feedback`,
/// `thr.y → pid.trigger` closes the loop, otherwise boundary input `trg` drives `trigger`.
fn reset_pid_trigger(member: &str, feedback: bool) -> Vec<u8> {
    let real = || json!({ "@id": "S231:Real" });
    let boolean = || json!({ "@id": "S231:Boolean" });
    let connector = |local: &str, kind: &str, data: Value, to: Option<&str>| {
        let mut node = json!({ "@id": iri(local), "@type": kind, "S231:isOfDataType": data });
        if let Some(to) = to {
            node["S231:isConnectedTo"] = json!({ "@id": iri(to) });
        }
        node
    };
    let mut boundary_inputs = vec![json!({ "@id": iri("us") }), json!({ "@id": iri("um") })];
    if !feedback {
        boundary_inputs.push(json!({ "@id": iri("trg") }));
    }
    let mut graph = vec![
        json!({
            "@id": "http://example.org#rst",
            "@type": "S231:Block",
            "S231:containsBlock": [{ "@id": iri("pid") }, { "@id": iri("thr") }],
            "S231:hasInput": boundary_inputs,
            "S231:hasOutput": { "@id": iri("y") },
        }),
        connector("us", "S231:RealInput", real(), Some("pid.u_s")),
        connector("um", "S231:RealInput", real(), Some("pid.u_m")),
        connector("y", "S231:RealOutput", real(), None),
        json!({
            "@id": iri("pid"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.PIDWithReset",
            "S231:hasParameter": { "@id": iri("pid.controllerType") },
            "S231:hasInput": [
                { "@id": iri("pid.u_s") },
                { "@id": iri("pid.u_m") },
                { "@id": iri("pid.trigger") },
            ],
            "S231:hasOutput": { "@id": iri("pid.y") },
        }),
        json!({
            "@id": iri("pid.controllerType"),
            "@type": "S231:Parameter",
            "S231:isOfDataType": {
                "@id": "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController"
            },
            "S231:value": format!("Buildings.Controls.OBC.CDL.Types.SimpleController.{member}"),
        }),
        connector("pid.u_s", "S231:RealInput", real(), None),
        connector("pid.u_m", "S231:RealInput", real(), None),
        connector("pid.trigger", "S231:BooleanInput", boolean(), None),
        json!({
            "@id": iri("pid.y"), "@type": "S231:RealOutput", "S231:isOfDataType": real(),
            "S231:isConnectedTo": [{ "@id": iri("y") }, { "@id": iri("thr.u") }],
        }),
        json!({
            "@id": iri("thr"),
            "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.GreaterThreshold",
            "S231:hasParameter": { "@id": iri("thr.t") },
            "S231:hasInput": { "@id": iri("thr.u") },
            "S231:hasOutput": { "@id": iri("thr.y") },
        }),
        json!({
            "@id": iri("thr.t"), "@type": "S231:Parameter", "S231:isOfDataType": real(),
            "S231:value": { "@value": "0.5", "@type": "http://www.w3.org/2001/XMLSchema#double" },
        }),
        connector("thr.u", "S231:RealInput", real(), None),
        connector(
            "thr.y",
            "S231:BooleanOutput",
            boolean(),
            feedback.then_some("pid.trigger"),
        ),
    ];
    if !feedback {
        graph.push(connector(
            "trg",
            "S231:BooleanInput",
            boolean(),
            Some("pid.trigger"),
        ));
    }
    serde_json::to_vec(&json!({
        "@context": { "S231": "http://data.ashrae.org/S231P#" },
        "@graph": graph,
    }))
    .expect("serialize document")
}

#[test]
fn proportional_reset_pid_trigger_feedback_is_refused_as_a_block_loop() {
    let failure = refuse(&reset_pid_trigger("P", true));
    assert_eq!(failure.stage(), DiagnosticStage::Schedule);
    assert_eq!(render(failure.diagnostics()), RESET_PID_P_BLOCK_LOOP_GOLDEN);
    let observed: Vec<_> = failure
        .diagnostics()
        .records()
        .iter()
        .map(|record| {
            (
                record.key().stage(),
                record.key().severity(),
                record.key().code(),
                record.key().subject().clone(),
            )
        })
        .collect();
    assert_eq!(
        observed,
        [iri("pid"), iri("thr")].map(|subject| (
            DiagnosticStage::Schedule,
            DiagnosticSeverity::Error,
            "block-algebraic-loop",
            DiagnosticSubject::Opaque(subject),
        ))
    );
    assert_eq!(
        loop_order(failure.diagnostics(), "block-algebraic-loop"),
        [iri("pid"), iri("thr")]
    );
    let message = build_message(&failure);
    assert!(message.contains("The model may be CDL-legal"), "{message}");
}

#[test]
fn reset_pid_trigger_feedback_loads_in_every_mode_with_state() {
    for member in ["PI", "PD", "PID"] {
        Engine::in_memory()
            .load_cxf(&reset_pid_trigger(member, true))
            .unwrap_or_else(|err| panic!("{member} must load: {err:?}"));
    }
}

#[test]
fn proportional_reset_pid_without_the_feedback_loads() {
    Engine::in_memory()
        .load_cxf(&reset_pid_trigger("P", false))
        .unwrap_or_else(|err| panic!("P without feedback must load: {err:?}"));
}
