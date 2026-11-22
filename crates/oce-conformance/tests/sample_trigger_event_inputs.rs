//! `CDL.Logical.Sources.SampleTrigger` feeding an event input at a period at or below the frame
//! step (issue #800, engine only).
//!
//! `SampleTrigger` is true on each frame that reaches a new sample instant. When its period is at
//! or below the host frame step, every frame reaches one, so it is true on every frame: an event
//! input it feeds sees one false-to-true transition at most, at frame 0, where no `when` fires
//! (initialisation), and then none. `TriggeredSampler` therefore never samples and holds `y_start`.
//! In Modelica `sample(...)` is true only at each sample instant, so the `when` would fire at each.
//! With the period above the frame step the trigger falls between samples and every sample fires.
//!
//! This pins the engine's behaviour under HostTick v1, the widest case of the projection limit in
//! `docs/execution-profile.md` ("Conformance boundary"). It is not registered as a known divergence:
//! no tool run backs it, only the `.mo` text. Issue #800 waits on the owner's decision; a change
//! that makes these frames sample fails here and must update that section and this file. The
//! `conformance` binary runs in no per-PR job (nightly and `gate.sh full` only), so such a change
//! would merge on `CI OK` and fail first in the nightly run.

use oce_api::{Engine, Value};

const MODEL: &str = "http://example.org#sample_trigger_event_input";

/// `SampleTrigger(period) -> TriggeredSampler(y_start = 7).trigger`, `u` from the model's input;
/// the trigger and the sample are the model's outputs.
fn document(period: &str) -> String {
    let m = MODEL;
    let bool_in = r#""@type": "S231:BooleanInput", "S231:isOfDataType": { "@id": "S231:Boolean" }"#;
    let real_in = r#""@type": "S231:RealInput", "S231:isOfDataType": { "@id": "S231:Real" }"#;
    let bool_out =
        r#""@type": "S231:BooleanOutput", "S231:isOfDataType": { "@id": "S231:Boolean" }"#;
    let real_out = r#""@type": "S231:RealOutput", "S231:isOfDataType": { "@id": "S231:Real" }"#;
    let double = "http://www.w3.org/2001/XMLSchema#double";
    format!(
        r#"{{
  "@context": {{ "S231": "http://data.ashrae.org/S231#", "base": "http://example.org#" }},
  "@graph": [
    {{ "@id": "{m}", "@type": "S231:Block", "S231:label": "sample_trigger_event_input",
      "S231:containsBlock": [{{ "@id": "{m}.sam" }}, {{ "@id": "{m}.trg" }}],
      "S231:hasInput": {{ "@id": "{m}.u" }},
      "S231:hasOutput": [{{ "@id": "{m}.pulse" }}, {{ "@id": "{m}.y" }}] }},
    {{ "@id": "{m}.sam", "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Sources.SampleTrigger",
      "S231:label": "sam", "S231:hasParameter": [{{ "@id": "{m}.sam.period" }}, {{ "@id": "{m}.sam.shift" }}],
      "S231:hasOutput": {{ "@id": "{m}.sam.y" }} }},
    {{ "@id": "{m}.sam.period", "S231:value": {{ "@value": "{period}", "@type": "{double}" }} }},
    {{ "@id": "{m}.sam.shift", "S231:value": {{ "@value": "0.0", "@type": "{double}" }} }},
    {{ "@id": "{m}.trg", "@type": "http://example.org#Buildings.Controls.OBC.CDL.Discrete.TriggeredSampler",
      "S231:label": "trg", "S231:hasParameter": {{ "@id": "{m}.trg.y_start" }},
      "S231:hasInput": [{{ "@id": "{m}.trg.u" }}, {{ "@id": "{m}.trg.trigger" }}],
      "S231:hasOutput": {{ "@id": "{m}.trg.y" }} }},
    {{ "@id": "{m}.trg.y_start", "S231:value": {{ "@value": "7.0", "@type": "{double}" }} }},
    {{ "@id": "{m}.trg.u", {real_in} }},
    {{ "@id": "{m}.trg.trigger", {bool_in} }},
    {{ "@id": "{m}.u", {real_in}, "S231:isConnectedTo": {{ "@id": "{m}.trg.u" }},
      "S231:label": "sample_trigger_event_input.u" }},
    {{ "@id": "{m}.sam.y", {bool_out},
      "S231:isConnectedTo": [{{ "@id": "{m}.trg.trigger" }}, {{ "@id": "{m}.pulse" }}] }},
    {{ "@id": "{m}.trg.y", {real_out}, "S231:isConnectedTo": {{ "@id": "{m}.y" }} }},
    {{ "@id": "{m}.pulse", {bool_out} }},
    {{ "@id": "{m}.y", {real_out} }}
  ]
}}"#
    )
}

/// The trigger and the sample at frames 0, 1, ..., 7 s, with `u = 10 + t`.
fn run(period: &str) -> (Vec<bool>, Vec<f64>) {
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(document(period).as_bytes())
        .expect("the model loads");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let u = format!("{MODEL}.u");
    let (pulse, y) = (format!("{MODEL}.pulse"), format!("{MODEL}.y"));
    let mut pulses = Vec::new();
    let mut samples = Vec::new();
    for k in 0..8 {
        let t = f64::from(k);
        let frame = engine
            .prepare_frame(t, &[(u.as_str(), Value::Real(10.0 + t))])
            .expect("prepares");
        let done = engine.execute_frame(frame).expect("executes");
        for (path, value) in done.outputs() {
            match value {
                Value::Boolean(b) if *path == pulse => pulses.push(*b),
                Value::Real(v) if *path == y => samples.push(*v),
                other => panic!("t {t}: {path} {other:?}"),
            }
        }
    }
    assert_eq!((pulses.len(), samples.len()), (8, 8));
    (pulses, samples)
}

#[test]
fn a_sample_trigger_at_or_below_the_frame_step_is_true_on_every_frame_and_samples_nothing() {
    for period in ["0.5", "1.0"] {
        let (pulses, samples) = run(period);
        assert_eq!(pulses, [true; 8], "period {period} s");
        // One `true` from frame 0 on: no false-to-true transition after frame 0, so no sample.
        assert_eq!(samples, [7.0; 8], "period {period} s");
    }
}

#[test]
fn a_sample_trigger_above_the_frame_step_falls_between_samples_and_each_sample_fires() {
    let (pulses, samples) = run("2.0");
    assert_eq!(pulses, [true, false, true, false, true, false, true, false]);
    // Frame 0 is initialisation; each later sample instant is a rising edge.
    assert_eq!(samples, [7.0, 7.0, 12.0, 12.0, 14.0, 14.0, 16.0, 16.0]);
}
