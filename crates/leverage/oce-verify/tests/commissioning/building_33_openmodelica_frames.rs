//! The 78 synthetic Building 33 frames against OpenModelica's run of upstream's own
//! `CoolingCoilValve` (issue #787): the reset timing rule (issue #768) against the continuous-time
//! model, which shares nothing with the engine's discretisation.
//!
//! `openmodelica/yCooVal_frames.csv` is `Buildings.Utilities.Plotters.Examples.BaseClasses.
//! CoolingCoilValve`, unmodified at the vendored pin, fed `valve_oracle.txt`'s inputs through one
//! `CombiTimeTable` with constant segments (each frame's inputs held until the next frame, as the
//! engine holds them; `CoolingCoilValveFrames.mo`), simulated by the pinned OpenModelica 1.25.1
//! container under cvode at tolerance 1e-6 (`run_openmodelica_frames.sh`). At each frame instant
//! it is the last row OpenModelica wrote within 1e-6 s of the instant, the value after any event
//! there; its `reset` column is 1 where OpenModelica reinitialised the PI's integrator at the
//! instant. No container runs here.
//!
//! What the comparison can and cannot see:
//!
//! - **OpenModelica resets at all nine changes of the enable; the engine at eight of them.**
//!   `Change` is true only at each change's instant in Modelica, so each fires the PI's
//!   `when trigger`. Under HostTick v1 `Change` holds for a frame, and the disable at 377 s and the
//!   re-enable at 382 s are consecutive frames: one `true` lasting two frames, one rising edge, one
//!   reset (at 377 s). At 382 s OpenModelica resets and the engine does not (issue #800, an open
//!   question for the owner). Here the inputs at 377 s and 382 s are the same, so both give 0 and
//!   this scenario cannot show the difference; `building_33_openmodelica_reenable.rs` runs the
//!   same frames with the supply air changed at 382 s and pins it.
//! - **At the engine's eight resets, and wherever the enable is off or the line cuts the PI, the
//!   engine equals OpenModelica to rounding (1e-12).** No integration enters there: `reinit` with
//!   `y_reset - (P + D)` makes the controller's output `lim(y_reset)` at the instant, and so does
//!   the engine's rule. The engine before issue #768 showed the four re-enables (25, 237, 272 and
//!   307 s) one frame late, 0.09, 0.045, 0.018 and 0.018 away: at least 1.8e10 times this bound.
//! - **After each re-enable the engine trails OpenModelica by one frame of integral action**:
//!   over the uniform 5 s steps with a constant error that follow 25, 237, 272 and 307 s,
//!   OpenModelica leads by `k/Ti * e * dt` = 0.009 at every frame, as after the start. That is the
//!   reset frame taking no Euler step (decision 4 of the reset timing note): a reset frame that
//!   took its step would close the lag after a reset, to 0, and fails here.
//! - **Elsewhere the difference is the integration, recorded, not bounded.** At these frames'
//!   uneven 2 to 13 s steps, against the anti-windup's 27 s time constant, it reaches 0.0277 (187 s,
//!   after the PI's saturation under the high limit), above the 0.01 band the Building 33 day is
//!   judged by. The value is pinned, so a change shows, but no bound is claimed: at uneven steps
//!   the engine integrates each frame's error over the interval before that frame, which adds
//!   error beyond forward Euler's, and refining a uniform grid cannot separate the two. The last
//!   test refines the frames: on a uniform grid the difference falls in proportion to the step,
//!   located at 187 s, while the engine's resets stay exact.

use sha2::{Digest, Sha256};

use super::building_33_day::example_file;
use super::building_33_transcription::{OracleRow, ROOT, oracle_rows, valve_series};
use super::support;

use oce_api::{Engine, Value};

/// The bound where no integral reaches the valve: rounding only (the two tools evaluate the
/// line's expression in a different order, 1.6e-15 apart on these frames).
pub(super) const EXACT: f64 = 1e-12;

/// The vendored copy of the model OpenModelica simulated.
const MODEL: &str = "../../../third_party/modelica-buildings-cdl/Buildings/Utilities/Plotters/\
                     Examples/BaseClasses/CoolingCoilValve.mo";

/// A run's frames: time, OpenModelica's valve signal, and whether it reset the integrator.
pub(super) struct Frame {
    pub t: f64,
    pub y: f64,
    pub reset: bool,
}

/// The frames of one of the committed OpenModelica signals.
pub(super) fn openmodelica_signal(name: &str) -> Vec<Frame> {
    String::from_utf8(example_file(&format!("openmodelica/{name}")))
        .expect("UTF-8")
        .lines()
        .skip(1)
        .map(|line| {
            let cells: Vec<&str> = line.split(',').collect();
            assert_eq!(cells.len(), 3, "{line}");
            Frame {
                t: cells[0].parse().expect("a time"),
                y: cells[1].parse().expect("a value"),
                reset: match cells[2] {
                    "1" => true,
                    "0" => false,
                    other => panic!("reset {other}"),
                },
            }
        })
        .collect()
}

fn openmodelica() -> Vec<Frame> {
    openmodelica_signal("yCooVal_frames.csv")
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The reset instants of the oracle's frames under each reading of `Change -> when trigger`:
/// Modelica's (every change of the enable) and HostTick v1's (a rising edge of `Change`'s
/// frame-long output). The enable is off exactly at the frames labelled `off:`.
pub(super) fn resets(rows: &[OracleRow]) -> (Vec<f64>, Vec<f64>) {
    let mut modelica = Vec::new();
    let mut hosttick = Vec::new();
    let (mut enable_before, mut change_before) = (false, false);
    for (k, row) in rows.iter().enumerate() {
        let enable = !row.branch.starts_with("off:");
        let change = enable != enable_before;
        if k > 0 && change {
            modelica.push(row.t);
            if !change_before {
                hosttick.push(row.t);
            }
        }
        (enable_before, change_before) = (enable, change);
    }
    (modelica, hosttick)
}

/// Whether no integral reaches the engine's valve at this frame: the engine resets there, the
/// enable is off, or the high limit's line is below the PI (the oracle's branch label).
fn algebraic(row: &OracleRow, engine_resets: &[f64]) -> bool {
    engine_resets.contains(&row.t)
        || row.branch.starts_with("off:")
        || row.branch.starts_with("limited:")
}

/// The largest absolute difference over `frames` and the first time it occurs.
pub(super) fn largest(pairs: impl Iterator<Item = (f64, f64, f64)>) -> (f64, f64) {
    pairs.fold((0.0, f64::NAN), |best, (t, a, b)| {
        let d = (a - b).abs();
        if d > best.0 { (d, t) } else { best }
    })
}

#[test]
fn the_run_saw_the_oracle_s_frames_held_and_its_record_binds_them() {
    let rows = oracle_rows();
    let frames = openmodelica();
    assert_eq!(frames.len(), 78);
    for (frame, row) in frames.iter().zip(&rows) {
        assert_eq!(frame.t.to_bits(), row.t.to_bits());
    }
    // The wrapper's table is the oracle's input columns, row for row, as `frames_wrapper.py`
    // writes them: the run cannot be of other frames than the ones the engine runs.
    let wrapper = example_file("openmodelica/CoolingCoilValveFrames.mo");
    let text = String::from_utf8(wrapper.clone()).expect("UTF-8");
    let table: Vec<String> = rows
        .iter()
        .map(|row| {
            let [t_sup, t_sup_set, t_out, fan_fee] = row.inputs;
            let fan = if row.fan_sta { 1 } else { 0 };
            format!(
                "{:?}, {t_sup:?}, {t_sup_set:?}, {t_out:?}, {fan_fee:?}, {fan}",
                row.t
            )
        })
        .collect();
    let block = format!("table=[\n      {}],", table.join(";\n      "));
    assert!(text.contains(&block), "the wrapper's table is stale");
    assert!(
        text.contains("Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve cooVal")
    );
    assert!(text.contains("smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments"));
    let record: serde_json::Value =
        serde_json::from_slice(&example_file("openmodelica/provenance_frames.json"))
            .expect("provenance JSON");
    let field = |key: &str| record[key].as_str().expect(key).to_owned();
    for (key, value) in [
        (
            "image",
            "openmodelica/openmodelica@sha256:\
             79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864",
        ),
        ("omc", "OpenModelica 1.25.1"),
        ("architecture", "arm64"),
        (
            "buildings_commit",
            "a131864e4c4df22ebcd52bb8da439de0087ac365",
        ),
        ("buildings_tree", "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09"),
        ("msl_commit", "7a4bf7de77a3986e8eb1e88cbb515d646f78f834"),
        ("msl_tree", "43d7d8fc1a991358e9e5e91976e27cdc4280173f"),
    ] {
        assert_eq!(field(key), value, "{key}");
    }
    let vendored = std::fs::read(support::crate_dir().join(MODEL)).expect("the vendored model");
    assert_eq!(field("model_source_sha256"), sha256(&vendored));
    assert_eq!(
        field("oracle_sha256"),
        sha256(&example_file("valve_oracle.txt"))
    );
    assert_eq!(field("wrapper_sha256"), sha256(&wrapper));
    assert_eq!(
        field("simulate_template_sha256"),
        sha256(&example_file("openmodelica/simulate_frames.mos"))
    );
    assert_eq!(
        field("signal_sha256"),
        sha256(&example_file("openmodelica/yCooVal_frames.csv"))
    );
    assert_eq!(
        field("raw_sha256"),
        sha256(&example_file("openmodelica/CoolingCoilValveFrames.raw.csv"))
    );
    assert_eq!(record["rows"], 78);
}

#[test]
fn openmodelica_resets_at_all_nine_changes_of_the_enable_and_the_engine_at_eight() {
    let (modelica, hosttick) = resets(&oracle_rows());
    let openmodelica: Vec<f64> = openmodelica()
        .iter()
        .filter(|frame| frame.reset)
        .map(|frame| frame.t)
        .collect();
    assert_eq!(
        openmodelica, modelica,
        "OpenModelica resets at each change of the enable"
    );
    assert_eq!(
        modelica,
        [25.0, 207.0, 237.0, 257.0, 272.0, 292.0, 307.0, 377.0, 382.0]
    );
    // The engine (HostTick v1): `Change` true at 377 s and at 382 s is one rising edge.
    assert_eq!(
        hosttick,
        [25.0, 207.0, 237.0, 257.0, 272.0, 292.0, 307.0, 377.0]
    );
}

/// Frames where the engine's signal `engine` leaves OpenModelica's by more than [`EXACT`] where
/// no integral reaches the engine's valve, with the difference.
fn algebraic_failures(engine: &[f64]) -> Vec<(f64, f64)> {
    let rows = oracle_rows();
    let (_, hosttick) = resets(&rows);
    openmodelica()
        .iter()
        .zip(&rows)
        .zip(engine)
        .filter(|((_, row), _)| algebraic(row, &hosttick))
        .map(|((frame, _), y)| (frame.t, (y - frame.y).abs()))
        .filter(|(_, d)| *d > EXACT)
        .collect()
}

#[test]
fn the_engine_equals_openmodelica_at_its_resets_and_wherever_no_integral_reaches_the_valve() {
    let engine = valve_series(&example_file("cooling_coil_valve.cxf.json"));
    assert_eq!(algebraic_failures(&engine), []);
    let frames = openmodelica();
    let rows = oracle_rows();
    let (_, hosttick) = resets(&rows);
    let counted = rows.iter().filter(|row| algebraic(row, &hosttick)).count();
    assert_eq!(counted, 31, "frames compared to rounding");
    for t in [25.0, 237.0, 272.0, 307.0] {
        let at = frames.iter().position(|f| f.t == t).expect("a frame");
        assert_eq!(
            (engine[at], frames[at].y),
            (0.0, 0.0),
            "the re-enable at {t} s"
        );
    }
    // Where the integral reaches the valve: the integration difference, pinned, not bounded.
    let (worst, at) = largest(
        frames
            .iter()
            .zip(&rows)
            .zip(&engine)
            .filter(|((_, row), _)| !algebraic(row, &hosttick))
            .map(|((frame, _), y)| (frame.t, *y, frame.y)),
    );
    assert_eq!(at, 187.0, "where the largest difference is");
    assert!((worst - 0.027_691_742_615_876_147).abs() < EXACT, "{worst}");
}

#[test]
fn after_each_re_enable_the_engine_trails_openmodelica_by_one_frame_of_integral_action() {
    // `k/Ti * e * dt` with `k = 0.018`, `Ti = 30 s`, `e = 3 K` and 5 s steps; OpenModelica's own
    // error over these frames is below 1e-9 (cvode, and table events landing about t * 1e-10 s late).
    let lead = 0.018 / 30.0 * 3.0 * 5.0;
    let engine = valve_series(&example_file("cooling_coil_valve.cxf.json"));
    let frames = openmodelica();
    let stretches: [(f64, f64); 4] = [(30.0, 85.0), (242.0, 252.0), (277.0, 287.0), (312.0, 322.0)];
    let mut checked = 0;
    for (frame, y) in frames.iter().zip(&engine) {
        if stretches.iter().any(|(a, b)| (*a..=*b).contains(&frame.t)) {
            assert!(
                (frame.y - y - lead).abs() < 1e-9,
                "t {} s: OpenModelica {}, engine {y}",
                frame.t,
                frame.y
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 21);
}

#[test]
fn the_timing_before_issue_768_fails_at_the_four_re_enables_by_far_more_than_the_bound() {
    // `valve_oracle.txt` before GitHub #789 (at 56c81a85), which the engine then matched bit for
    // bit: each re-enable showed the integrator's reset one frame late.
    let before: [(f64, u64); 4] = [
        (25.0, 0x3fb7_0a3d_70a3_d70b),
        (237.0, 0x3fa7_0a3d_70a3_d70b),
        (272.0, 0x3f92_6e97_8d4f_df3c),
        (307.0, 0x3f92_6e97_8d4f_df3c),
    ];
    let mut engine = valve_series(&example_file("cooling_coil_valve.cxf.json"));
    let rows = oracle_rows();
    for (t, bits) in before {
        let at = rows.iter().position(|row| row.t == t).expect("a frame");
        engine[at] = f64::from_bits(bits);
    }
    let failures = algebraic_failures(&engine);
    let times: Vec<f64> = failures.iter().map(|(t, _)| *t).collect();
    assert_eq!(times, [25.0, 237.0, 272.0, 307.0]);
    let smallest = failures
        .iter()
        .map(|(_, d)| *d)
        .fold(f64::INFINITY, f64::min);
    assert!(smallest >= 0.018, "{smallest}");
    assert!(smallest / EXACT > 1.0e10);
}

/// The engine's valve signal at `rows`' frame instants, run on a grid of `step` seconds from 0
/// to the last frame with each row's inputs held until the next row.
pub(super) fn refined(rows: &[OracleRow], step: f64) -> Vec<f64> {
    let mut engine = Engine::new();
    engine
        .load_cxf(&example_file("cooling_coil_valve.cxf.json"))
        .expect("the CXF loads");
    let names =
        ["TSup", "TSupSet", "TOut", "uFanFee", "uFanSta", "yCooVal"].map(|n| format!("{ROOT}{n}"));
    let last = rows.last().expect("frames").t;
    let steps = (last / step).round() as u64;
    let mut out = Vec::with_capacity(rows.len());
    let mut held = 0;
    for n in 0..=steps {
        let t = n as f64 * step;
        while held + 1 < rows.len() && rows[held + 1].t <= t {
            held += 1;
        }
        let row = &rows[held];
        let mut entries: Vec<(&str, Value)> = row
            .inputs
            .iter()
            .zip(&names)
            .map(|(x, name)| (name.as_str(), Value::Real(*x)))
            .collect();
        entries.push((names[4].as_str(), Value::Boolean(row.fan_sta)));
        let frame = engine.prepare_frame(t, &entries).expect("prepares");
        let done = engine.execute_frame(frame).expect("executes");
        if rows.iter().any(|r| r.t == t) {
            match done.outputs().iter().find(|(path, _)| *path == names[5]) {
                Some((_, Value::Real(y))) => out.push(*y),
                other => panic!("t {t}: {other:?}"),
            }
        }
    }
    assert_eq!(out.len(), rows.len());
    out
}

/// A sanity bound on finer frames of `step` seconds, not a derived one: three steps at a rate
/// of `k/Ti` times twice the largest error, plus 1e-5 for the solver. It is not shown that the
/// anti-windup term stays below the largest error (with `Ni = 0.9` a step in the error can take it
/// past by about `1/Ni`), so the bound only rules out gross failures; the tests that carry the
/// claim are that the largest difference halves with the step and stays at 187 s.
fn sanity_bound(step: f64) -> f64 {
    let largest_error = oracle_rows()
        .iter()
        .map(|row| (row.inputs[0] - row.inputs[1]).abs())
        .fold(0.0, f64::max);
    assert_eq!(largest_error, 51.0, "TSup 281 K against TSupSet 230 K");
    3.0 * (0.018_000_000_000_000_002 / 30.0) * 2.0 * largest_error * step + 1e-5
}

#[test]
fn on_finer_frames_the_difference_halves_with_the_step_and_the_engine_s_resets_stay_exact() {
    let frames = openmodelica();
    let rows = oracle_rows();
    let mut previous: Option<f64> = None;
    for step in [1.0 / 16.0, 1.0 / 32.0, 1.0 / 64.0] {
        let engine = refined(&rows, step);
        let (worst, at) = largest(
            frames
                .iter()
                .zip(&engine)
                .map(|(frame, y)| (frame.t, *y, frame.y)),
        );
        assert!(
            worst <= sanity_bound(step),
            "step {step}: {worst} at {at} s"
        );
        assert_eq!(at, 187.0, "step {step}");
        // On a finer grid 377 s and 382 s are many frames apart, so the engine resets at both, as
        // OpenModelica does, and every OpenModelica reset is an engine reset.
        for (frame, y) in frames.iter().zip(&engine) {
            if frame.reset {
                assert!(
                    (y - frame.y).abs() <= EXACT,
                    "step {step}, reset at {} s",
                    frame.t
                );
            }
        }
        if let Some(before) = previous {
            let ratio = before / worst;
            assert!((1.9..=2.1).contains(&ratio), "step {step}: ratio {ratio}");
        }
        previous = Some(worst);
    }
    // At 1/64 s the largest difference is 7.2e-5, against 0.0277 at the oracle's own frames.
    let finest = previous.expect("three steps");
    assert!(finest < 1e-4, "{finest}");
}
