//! `CDL.Logical.Sources.SampleTrigger` feeding an event input at periods at, below and above the
//! frame step (issue #800, engine only; `open_modelica_pulse_events.rs` checks the same shapes
//! against OpenModelica).
//!
//! Under the settled-value rule (design note `docs/design/pulse-event-inputs.md`, option (g)) a
//! `SampleTrigger` output is a *sample* to an event input: in Modelica `sample()` stays true
//! through every event iteration of its instant and falls in continuous time, where no `when` sees
//! it fall, so an input it feeds fires on every frame it is true. At a period at or below the
//! frame step that is every frame; at frame 0 the start (t = 0) is an instant, so the sampler
//! samples there too (decision 3). Below the frame step a frame holds several instants and still
//! fires once: the block warns once, the first time. The `conformance` binary runs in no per-PR
//! job (nightly and `gate.sh full` only).

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

/// The trigger, the sample and the frame warnings at frames 0, 1, ..., 7 s, with `u = 10 + t`.
fn run(period: &str) -> (Vec<bool>, Vec<f64>, Vec<usize>) {
    let mut engine = Engine::new();
    let (report, _) = engine
        .load_cxf(document(period).as_bytes())
        .expect("the model loads");
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let u = format!("{MODEL}.u");
    let (pulse, y) = (format!("{MODEL}.pulse"), format!("{MODEL}.y"));
    let mut pulses = Vec::new();
    let mut samples = Vec::new();
    let mut warnings = Vec::new();
    for k in 0..8 {
        let t = f64::from(k);
        let frame = engine
            .prepare_frame(t, &[(u.as_str(), Value::Real(10.0 + t))])
            .expect("prepares");
        let done = engine.execute_frame(frame).expect("executes");
        warnings.push(done.diagnostics().len());
        for (path, value) in done.outputs() {
            match value {
                Value::Boolean(b) if *path == pulse => pulses.push(*b),
                Value::Real(v) if *path == y => samples.push(*v),
                other => panic!("t {t}: {path} {other:?}"),
            }
        }
    }
    assert_eq!((pulses.len(), samples.len()), (8, 8));
    (pulses, samples, warnings)
}

#[test]
fn a_sample_trigger_at_or_below_the_frame_step_samples_on_every_frame_from_the_start() {
    let (pulses, samples, warnings) = run("1.0");
    assert_eq!(pulses, [true; 8]);
    // The start is an instant: frame 0 samples; then every frame fires.
    assert_eq!(samples, [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0]);
    assert_eq!(
        warnings, [0; 8],
        "one instant per frame is no cadence violation"
    );
}

#[test]
fn a_sample_trigger_below_the_frame_step_fires_once_a_frame_and_warns_once() {
    let (pulses, samples, warnings) = run("0.5");
    assert_eq!(pulses, [true; 8]);
    assert_eq!(samples, [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0]);
    // Frame 1 spans the instants at 0.5 and 1 s: one warning, never again.
    assert_eq!(warnings, [0, 1, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn a_sample_trigger_above_the_frame_step_falls_between_samples_and_each_sample_fires() {
    let (pulses, samples, warnings) = run("2.0");
    assert_eq!(pulses, [true, false, true, false, true, false, true, false]);
    // The start is an instant; each later instant samples at its own frame.
    assert_eq!(samples, [10.0, 10.0, 12.0, 12.0, 14.0, 14.0, 16.0, 16.0]);
    assert_eq!(warnings, [0; 8]);
}
