//! The settled-value rule (issue #800, design note `docs/design/pulse-event-inputs.md`) at the
//! facade: what a stored word means across capture, restore, `set_param` and the blob entry.
//!
//! `fixtures/pulse_snapshot.jsonld`: `Change(u)` into `OnCounter.trigger` and `Latch.u`, and
//! `SampleTrigger(1 s)` into `TriggeredSampler.trigger` (`u` from `uR`).
//! `fixtures/state_before_settled_rule.hex`: that model's snapshot after frames at 0, 1 and 2 s
//! with `u` false, true, false (a change at 1 s and at 2 s) and `uR = 10 + t`, captured by a
//! scratch program on `development` 56f93247, the commit before the rule. Its previous-trigger
//! words hold the frame value `v` (true: `Change` was true at 2 s), where this build stores `w`
//! (false for a direct pulse), and its `SampleTrigger` word is the plain index 2 (not warned).

use oce_api::{Engine, EngineStateSnapshot, Value};
use oce_graph::blob::Executable;

const MODEL: &str = include_str!("fixtures/pulse_snapshot.jsonld");
const BEFORE_RULE: &str = include_str!("fixtures/state_before_settled_rule.hex");
const ROOT: &str = "http://example.org#pulse_snapshot.";

fn unhex(text: &str) -> Vec<u8> {
    let hex = text.trim();
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn loaded() -> Engine {
    let mut engine = Engine::new();
    engine.load_cxf(MODEL.as_bytes()).expect("loads");
    engine
}

/// One frame: the count, the latch, the sample, and how many warnings it emitted.
fn frame(engine: &mut Engine, t: f64, u: bool) -> (i64, bool, f64, usize) {
    let (pu, pr) = (format!("{ROOT}u"), format!("{ROOT}uR"));
    let prepared = engine
        .prepare_frame(
            t,
            &[
                (pu.as_str(), Value::Boolean(u)),
                (pr.as_str(), Value::Real(10.0 + t)),
            ],
        )
        .expect("prepares");
    let done = engine.execute_frame(prepared).expect("executes");
    let get = |name: &str| {
        done.outputs()
            .iter()
            .find(|(p, _)| *p == format!("{ROOT}{name}"))
            .map(|(_, v)| v.clone())
            .expect(name)
    };
    let count = match get("y_cnt") {
        Value::Integer(n) => n,
        other => panic!("{other:?}"),
    };
    let latch = matches!(get("y_lat"), Value::Boolean(true));
    let sample = match get("y_ts") {
        Value::Real(x) => x,
        other => panic!("{other:?}"),
    };
    (count, latch, sample, done.diagnostics().len())
}

#[test]
fn a_snapshot_written_before_the_rule_restores_with_at_most_one_event_merged_at_the_boundary() {
    // This build from the start: changes at 1, 2, 3 and 5 s, all counted.
    let mut fresh = loaded();
    let mut counts = Vec::new();
    for (t, u) in [
        (0.0, false),
        (1.0, true),
        (2.0, false),
        (3.0, true),
        (4.0, true),
        (5.0, false),
    ] {
        counts.push(frame(&mut fresh, t, u).0);
    }
    assert_eq!(counts, [0, 1, 2, 3, 3, 4]);

    // The pre-rule snapshot after 2 s holds count 1 (it merged 1 and 2 s) and the stored frame
    // value `true`. Restored here, the change at 3 s meets that stored `true` where `w = false`
    // would be: it merges with the one before the snapshot, as the old rule did, and nothing is
    // counted twice. The change at 5 s, with a word this build wrote, counts.
    let snapshot =
        EngineStateSnapshot::from_bytes(&unhex(BEFORE_RULE)).expect("the 0.6.0 snapshot decodes");
    let mut restored = loaded();
    restored.restore_state(&snapshot).expect("restores");
    let after: Vec<i64> = [(3.0, true), (4.0, true), (5.0, false)]
        .into_iter()
        .map(|(t, u)| frame(&mut restored, t, u).0)
        .collect();
    assert_eq!(after, [1, 1, 2]);

    // Its SampleTrigger word is unwarned: the first frame that spans instants warns, once.
    let warnings: Vec<usize> = [(7.5, false), (10.0, false)]
        .into_iter()
        .map(|(t, u)| frame(&mut restored, t, u).3)
        .collect();
    assert_eq!(warnings, [1, 0]);
}

#[test]
fn the_span_warning_mark_survives_capture_and_restore() {
    // After the warning: a restore is "already warned", the next span warns no more.
    let mut engine = loaded();
    assert_eq!(frame(&mut engine, 0.0, false).3, 0);
    assert_eq!(frame(&mut engine, 2.5, false).3, 1);
    let warned = engine.state_snapshot().expect("captures");
    let mut target = loaded();
    target.restore_state(&warned).expect("restores");
    assert_eq!(frame(&mut target, 5.0, false).3, 0);

    // Before it: "not yet", the next span warns.
    let mut engine = loaded();
    frame(&mut engine, 0.0, false);
    let not_yet = engine.state_snapshot().expect("captures");
    let mut target = loaded();
    target.restore_state(&not_yet).expect("restores");
    assert_eq!(frame(&mut target, 2.5, false).3, 1);
    assert_eq!(frame(&mut target, 5.0, false).3, 0);
}

#[test]
fn sample_trigger_period_and_shift_edits_keep_the_plan_and_resume_rearms_the_warning() {
    // `period` and `shift` change when the trigger pulses, never what it is to a cone: the edit
    // is accepted, `resume` keeps the schedule, and the sampler samples on the new grid.
    let mut engine = loaded();
    frame(&mut engine, 0.0, false);
    assert_eq!(frame(&mut engine, 2.5, false).3, 1);
    engine.halt().unwrap();
    engine
        .set_param(&format!("{ROOT}s1.period"), Value::Real(2.0))
        .expect("not structural");
    engine
        .set_param(&format!("{ROOT}s1.shift"), Value::Real(1.0))
        .expect("not structural");
    engine.resume().expect("resumes");
    // Instants at 1, 3, 5, ... s. A fresh state: frame 0 (t = 0) is off the grid and the next
    // span warns again (the state was re-seeded).
    let rows: Vec<(f64, usize)> = [
        (0.0, false),
        (1.0, true),
        (2.0, false),
        (3.0, true),
        (8.0, false),
    ]
    .into_iter()
    .map(|(t, u)| {
        let (_, _, sample, warnings) = frame(&mut engine, t, u);
        (sample, warnings)
    })
    .collect();
    assert_eq!(rows, [(7.0, 0), (11.0, 0), (11.0, 0), (13.0, 0), (18.0, 1)]);
    // The counter still counts consecutive changes (1, 2 and 3 s) after the resume.
    let mut engine = loaded();
    engine.halt().unwrap();
    engine
        .set_param(&format!("{ROOT}s1.period"), Value::Real(0.25))
        .unwrap();
    engine.resume().unwrap();
    let counts: Vec<i64> = [(0.0, false), (1.0, true), (2.0, false), (3.0, true)]
        .into_iter()
        .map(|(t, u)| frame(&mut engine, t, u).0)
        .collect();
    assert_eq!(counts, [0, 1, 2, 3]);
}

#[test]
fn the_blob_entry_derives_the_same_plan_and_runs_the_same_frames() {
    let mut engine = loaded();
    let blob = engine.compile_blob().expect("compiles").bytes;
    let mut executable = Executable::from_blob(&blob).expect("decodes");
    assert!(
        !executable.decoded().schedule.events.is_empty(),
        "the decoder derived the plan"
    );
    let inputs = executable.inputs().to_vec();
    for (t, u) in [
        (0.0, false),
        (1.0, true),
        (2.0, false),
        (3.0, true),
        (6.5, true),
    ] {
        let (count, latch, sample, _) = frame(&mut engine, t, u);
        let values: Vec<Value> = inputs
            .iter()
            .map(|input| {
                if input.path.ends_with(".uR") {
                    Value::Real(10.0 + t)
                } else {
                    Value::Boolean(u)
                }
            })
            .collect();
        executable
            .execute_frame(t, &values, &oce_blocks::NoopDiagnostics)
            .expect("executes");
        let outs: Vec<Value> = executable.output_values().cloned().collect();
        let paths: Vec<&str> = executable.outputs().iter().map(|o| o.path).collect();
        let at =
            |name: &str| outs[paths.iter().position(|p| p.ends_with(name)).expect(name)].clone();
        assert!(at(".y_cnt").bit_eq(&Value::Integer(count)), "t {t}");
        assert!(at(".y_lat").bit_eq(&Value::Boolean(latch)), "t {t}");
        assert!(at(".y_ts").bit_eq(&Value::Real(sample)), "t {t}");
    }
    assert_eq!(engine.state_words(), executable.state_words());
}
