//! `CDL.Reals.IntegratorWithReset` integrating a nonzero input against OpenModelica (issues #670
//! and #787 review).
//!
//! `IntegratorWithResetRampPilot` (`tools/openmodelica-event-reference/events/`) integrates
//! `u = 0.5`, stepping to `u = 2` at 5 s, with `k = 1` and `y_start = 0`, and resets at 3 s (to
//! `y_reset_in = -4`) and 7 s (to `6`), at 1 s frames with each input held. OpenModelica
//! integrates `der(y) = k*u` continuously (cvode, tolerance 1e-6); the canonical table keeps, at
//! each frame, the value after any event there.
//!
//! The engine's discretisation, written out: a frame emits the state before its own step, and
//! that step integrates the frame's own `u` over the interval before the frame (forward Euler
//! on the tick convention); a reset frame emits `y_reset_in` and takes no step. With held inputs
//! at a uniform frame step the two shifts cancel except for one step: at every frame after the
//! start or a reset the engine trails OpenModelica by exactly `k * u * dt`, `u` the input held at
//! that start or reset, and equals it at frame 0 and at each reset frame. (At uneven steps the
//! difference after the reset or start at frame `r` is
//! `k * [sum over j > r of u(j) * (dt(j) - dt(j+1)) - u(r) * dt(r+1)]`; these frames are uniform.) The step of `u` at 5 s does not change the
//! offset. A reset frame that took its step (the reading decision 4 of
//! `docs/design/reset-timing.md` rejects) would leave no offset after a reset while keeping it
//! after the start: the native-frame test fails it at 4, 5, 6, 8 and 9 s. That is what
//! OpenModelica shows about the skipped step: the engine's lag is the same one frame after a
//! reset as after the start. It does not show the skipped step is "right"; it is the engine's
//! convention, kept uniform.
//!
//! On finer frames of the same held inputs the offset is `k * u * h`: it halves with the step,
//! and the reset frames stay exact.

use oce_api::{Engine, Value};
use oce_conformance::CombiTimeTable;

use crate::block_harness::{B, BlockCase, Param, ParamValue, Port, R, build_cxf, case};
use crate::open_modelica_event_triggers::{assert_run_record, columns, sha256, vendored_source};

const CANONICAL: &str =
    include_str!("fixtures/open_modelica/reals_integrator_with_reset/ramp.canonical.csv");
const PROVENANCE: &str =
    include_str!("fixtures/open_modelica/reals_integrator_with_reset/ramp.provenance.json");
const RAW: &[u8] =
    include_bytes!("fixtures/open_modelica/reals_integrator_with_reset/ramp.raw.csv");
const PILOT: &[u8] = include_bytes!(
    "../../../tools/openmodelica-event-reference/events/IntegratorWithResetRampPilot.mo"
);

const RAMP: BlockCase = case(
    "openmodelica_reals_integrator_with_reset_ramp",
    "CDL.Reals.IntegratorWithReset",
    "external",
    &[
        Port { name: "u", kind: R },
        Port {
            name: "y_reset_in",
            kind: R,
        },
        Port {
            name: "trigger",
            kind: B,
        },
    ],
    &[
        Param {
            name: "k",
            value: ParamValue::Real("1.0"),
        },
        Param {
            name: "y_start",
            value: ParamValue::Real("0.0"),
        },
    ],
    &[Port { name: "y", kind: R }],
);

/// OpenModelica's own error at the frames: each table event lands about `t * 2e-10` s after its
/// frame, so the held input is integrated that much longer (4e-10 at most here), plus cvode's.
const SOLVER: f64 = 1e-8;

/// The reference's frames: `(t, u, y_reset_in, trigger, y)`.
fn frames() -> Vec<[f64; 5]> {
    let table = CombiTimeTable::parse(CANONICAL).expect("the canonical table");
    let (names, rows) = columns(&table);
    assert_eq!(names, ["time", "u", "y_reset_in", "trigger", "y"]);
    rows.iter()
        .map(|row| [row[0], row[1], row[2], row[3], row[4]])
        .collect()
}

/// The engine's `y` at the reference's frame instants, run on a grid of `step` seconds from 0 to
/// the last frame with each frame's inputs held until the next.
fn engine(step: f64) -> Vec<f64> {
    let frames = frames();
    let mut engine = Engine::new();
    engine
        .load_cxf(build_cxf(&RAMP).as_bytes())
        .expect("the block loads");
    // The model's root connectors, which the frame reports.
    let input = |name: &str| format!("http://example.org#{}.{name}", RAMP.slug);
    let (u, reset_in, trigger) = (input("u"), input("y_reset_in"), input("trigger"));
    let y = input("y");
    let last = frames.last().expect("frames")[0];
    let mut out = Vec::new();
    let mut held = 0;
    for n in 0..=((last / step).round() as u64) {
        let t = n as f64 * step;
        while held + 1 < frames.len() && frames[held + 1][0] <= t {
            held += 1;
        }
        let row = frames[held];
        let entries = [
            (u.as_str(), Value::Real(row[1])),
            (reset_in.as_str(), Value::Real(row[2])),
            (trigger.as_str(), Value::Boolean(row[3] == 1.0)),
        ];
        let frame = engine.prepare_frame(t, &entries).expect("prepares");
        let done = engine.execute_frame(frame).expect("executes");
        if frames.iter().any(|f| f[0] == t) {
            match done.outputs().iter().find(|(path, _)| *path == y) {
                Some((_, Value::Real(value))) => out.push(*value),
                other => panic!("t {t}: {other:?}"),
            }
        }
    }
    assert_eq!(out.len(), frames.len());
    out
}

/// The engine's expected offset from OpenModelica on a grid of `step`: 0 at frame 0 and at each
/// reset frame; elsewhere `-k * u * step`, `u` held at the last reset or at the start.
fn expected_offset(step: f64) -> Vec<f64> {
    let mut since = None;
    let mut previous_trigger = false;
    frames()
        .iter()
        .enumerate()
        .map(|(k, row)| {
            let trigger = row[3] == 1.0;
            let reset = k > 0 && trigger && !previous_trigger;
            previous_trigger = trigger;
            if k == 0 || reset {
                since = Some(row[1]);
                0.0
            } else {
                -since.expect("a start") * step
            }
        })
        .collect()
}

#[test]
fn the_engine_trails_openmodelica_by_one_frame_of_the_input_held_at_the_last_reset() {
    let frames = frames();
    let engine = engine(1.0);
    let expected = expected_offset(1.0);
    assert_eq!(
        expected,
        [0.0, -0.5, -0.5, 0.0, -0.5, -0.5, -0.5, 0.0, -2.0, -2.0]
    );
    for ((row, y), offset) in frames.iter().zip(&engine).zip(&expected) {
        let t = row[0];
        if *offset == 0.0 {
            // Frame 0 and the reset frames (3 s, 7 s): `y_start` or that frame's `y_reset_in`.
            assert_eq!(y.to_bits(), row[4].to_bits(), "t {t} s");
        } else {
            assert!(
                (y - row[4] - offset).abs() <= SOLVER,
                "t {t} s: engine {y}, OpenModelica {}",
                row[4]
            );
        }
    }
}

#[test]
fn on_finer_frames_the_offset_halves_with_the_step_and_the_resets_stay_exact() {
    let frames = frames();
    let mut previous: Option<f64> = None;
    for step in [1.0 / 16.0, 1.0 / 32.0, 1.0 / 64.0] {
        let engine = engine(step);
        let expected = expected_offset(step);
        let mut worst = 0.0_f64;
        for ((row, y), offset) in frames.iter().zip(&engine).zip(&expected) {
            assert!(
                (y - row[4] - offset).abs() <= SOLVER,
                "step {step}, t {} s",
                row[0]
            );
            worst = worst.max((y - row[4]).abs());
        }
        // The largest offset is `k * 2 * step`, after the reset at 7 s with `u = 2`.
        assert!((worst - 2.0 * step).abs() <= SOLVER, "step {step}: {worst}");
        if let Some(before) = previous {
            let ratio = before / worst;
            assert!((1.9..=2.1).contains(&ratio), "step {step}: ratio {ratio}");
        }
        previous = Some(worst);
    }
}

#[test]
fn the_ramp_run_s_provenance_binds_its_pilot_table_and_raw_result() {
    let record: serde_json::Value = serde_json::from_str(PROVENANCE).expect("provenance JSON");
    assert_eq!(record["class"], "CDL.Reals.IntegratorWithReset");
    assert_run_record(
        &record,
        "ramp",
        PILOT,
        vendored_source("CDL.Reals.IntegratorWithReset"),
        RAW,
    );
    assert_eq!(
        record["ramp_sha256"].as_str(),
        Some(sha256(CANONICAL.as_bytes()).as_str())
    );
}
