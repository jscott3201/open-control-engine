//! The 78 synthetic Building 33 frames against OpenModelica's run of upstream's own
//! `CoolingCoilValve` (issue #787): the check of the reset timing rule (issue #768) against the
//! continuous-time model, which shares nothing with the engine's discretisation.
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
//! - **At every frame where OpenModelica reinitialised the integrator, the engine equals it to
//!   rounding (1e-12).** No integration enters there: Modelica's `reinit(y, y_reset_in)` with
//!   `y_reset_in = y_reset - (P + D)` makes the controller's output `lim(y_reset)` at the
//!   instant, whatever the integral was, and so does the engine's rule. Those are the nine
//!   changes of the enable, the four re-enables of issue #768 (25, 237, 272 and 307 s) and both
//!   ends of the appended one-frame disable (377 and 382 s). The engine before issue #768 showed
//!   the four re-enables one frame late, 0.09, 0.045, 0.018 and 0.018 away from OpenModelica: at
//!   least 1.8e10 times this bound.
//! - The same bound holds wherever the valve is the line or the enable is off, no integral
//!   reaching it.
//! - **Elsewhere the difference is the integration**: the engine's forward Euler, which emits
//!   the integral before the frame's step and takes no step at a reset frame, against cvode. At
//!   these frames' 2 to 13 s steps, against the anti-windup's 27 s time constant, it reaches
//!   0.0277 (187 s, after the PI's saturation under the high limit). That it is integration and
//!   not timing is the last test: on finer frames it falls in proportion to the step, while the
//!   reset frames stay exact.

use super::building_33_day::example_file;
use super::building_33_transcription::{OracleRow, ROOT, oracle_rows, valve_series};

use oce_api::{Engine, Value};

/// The bound where no integral reaches the valve: rounding only (the two tools evaluate the
/// line's expression in a different order, 1.6e-15 apart on these frames).
const EXACT: f64 = 1e-12;

/// The run's frames: time, OpenModelica's valve signal, and whether it reset the integrator.
struct Frame {
    t: f64,
    y: f64,
    reset: bool,
}

fn openmodelica() -> Vec<Frame> {
    String::from_utf8(example_file("openmodelica/yCooVal_frames.csv"))
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

/// Whether no integral reaches the valve at this frame: OpenModelica reset the integrator, the
/// enable is off, or the high limit's line is below the PI (the oracle's branch label).
fn algebraic(frame: &Frame, row: &OracleRow) -> bool {
    frame.reset || row.branch.starts_with("off:") || row.branch.starts_with("limited:")
}

/// The largest absolute difference over `frames` and the first time it occurs.
fn largest(pairs: impl Iterator<Item = (f64, f64, f64)>) -> (f64, f64) {
    pairs.fold((0.0, f64::NAN), |best, (t, a, b)| {
        let d = (a - b).abs();
        if d > best.0 { (d, t) } else { best }
    })
}

#[test]
fn the_run_saw_the_oracle_s_frames_held() {
    let rows = oracle_rows();
    let frames = openmodelica();
    assert_eq!(frames.len(), 78);
    for (frame, row) in frames.iter().zip(&rows) {
        assert_eq!(frame.t.to_bits(), row.t.to_bits());
    }
    // The wrapper's table is the oracle's input columns, row for row, as `frames_wrapper.py`
    // writes them: the run cannot be of other frames than the ones the engine runs.
    let wrapper =
        String::from_utf8(example_file("openmodelica/CoolingCoilValveFrames.mo")).expect("UTF-8");
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
    assert!(wrapper.contains(&block), "the wrapper's table is stale");
    assert!(
        wrapper
            .contains("Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve cooVal")
    );
    assert!(wrapper.contains("smoothness=Modelica.Blocks.Types.Smoothness.ConstantSegments"));
    let provenance =
        String::from_utf8(example_file("openmodelica/provenance_frames.json")).expect("UTF-8");
    for field in [
        r#""omc": "OpenModelica 1.25.1""#,
        r#""architecture": "arm64""#,
        r#""buildings_commit": "a131864e4c4df22ebcd52bb8da439de0087ac365""#,
        r#""msl_commit": "7a4bf7de77a3986e8eb1e88cbb515d646f78f834""#,
        r#""rows": 78"#,
    ] {
        assert!(provenance.contains(field), "{field}");
    }
}

#[test]
fn openmodelica_resets_the_integrator_at_every_change_of_the_enable_and_nowhere_else() {
    let resets: Vec<f64> = openmodelica()
        .iter()
        .filter(|frame| frame.reset)
        .map(|frame| frame.t)
        .collect();
    assert_eq!(
        resets,
        [25.0, 207.0, 237.0, 257.0, 272.0, 292.0, 307.0, 377.0, 382.0]
    );
}

/// Frames where the engine's signal `engine` leaves OpenModelica's by more than [`EXACT`] where
/// no integral reaches the valve, with the difference.
fn algebraic_failures(engine: &[f64]) -> Vec<(f64, f64)> {
    openmodelica()
        .iter()
        .zip(oracle_rows())
        .zip(engine)
        .filter(|((frame, row), _)| algebraic(frame, row))
        .map(|((frame, _), y)| (frame.t, (y - frame.y).abs()))
        .filter(|(_, d)| *d > EXACT)
        .collect()
}

#[test]
fn the_engine_equals_openmodelica_at_every_reset_and_wherever_no_integral_reaches_the_valve() {
    let engine = valve_series(&example_file("cooling_coil_valve.cxf.json"));
    assert_eq!(algebraic_failures(&engine), []);
    let frames = openmodelica();
    let rows = oracle_rows();
    let counted = frames
        .iter()
        .zip(&rows)
        .filter(|(frame, row)| algebraic(frame, row))
        .count();
    assert_eq!(counted, 32, "frames compared to rounding");
    for t in [25.0, 237.0, 272.0, 307.0, 382.0] {
        let at = frames.iter().position(|f| f.t == t).expect("a frame");
        assert_eq!(
            (engine[at], frames[at].y),
            (0.0, 0.0),
            "the re-enable at {t} s"
        );
    }
    // Where the integral reaches the valve: the integration difference, recorded.
    let (worst, at) = largest(
        frames
            .iter()
            .zip(&rows)
            .zip(&engine)
            .filter(|((frame, row), _)| !algebraic(frame, row))
            .map(|((frame, _), y)| (frame.t, *y, frame.y)),
    );
    assert_eq!(at, 187.0, "where the largest difference is");
    assert!((worst - 0.027_691_742_615_876_147).abs() < EXACT, "{worst}");
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

/// The engine's valve signal at the oracle's frame instants, run on a grid of `step` seconds
/// from 0 to the last frame with each oracle row's inputs held until the next row: the oracle's
/// inputs, on finer frames.
fn refined(step: f64) -> Vec<f64> {
    let rows = oracle_rows();
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

/// The bound on finer frames of `step` seconds: three steps of the integral's largest rate,
/// plus the solver. The integral `I` moves at `k/Ti` times `e - (yUnl - y)/(k*Ni)`, and the
/// anti-windup term never exceeds the largest error, so the rate is at most `k/Ti` times twice
/// the largest error. Forward Euler trails the continuous integral by one step where the frame
/// emits before its step, by one more where a reset frame takes none, and by at most half a step
/// more on the anti-windup's contracting dynamics: two and a half steps, taken as three. The
/// solver's share is 1e-5, ten times cvode's relative tolerance on a signal of order 1.
fn refined_bound(step: f64) -> f64 {
    let rows = oracle_rows();
    let largest_error = rows
        .iter()
        .map(|row| (row.inputs[0] - row.inputs[1]).abs())
        .fold(0.0, f64::max);
    assert_eq!(largest_error, 51.0, "TSup 281 K against TSupSet 230 K");
    let k = 0.018_000_000_000_000_002;
    let ti = 30.0;
    3.0 * (k / ti) * 2.0 * largest_error * step + 1e-5
}

#[test]
fn on_finer_frames_the_difference_falls_with_the_step_and_the_resets_stay_exact() {
    let frames = openmodelica();
    let mut previous: Option<f64> = None;
    for step in [1.0 / 16.0, 1.0 / 32.0, 1.0 / 64.0] {
        let engine = refined(step);
        let (worst, at) = largest(
            frames
                .iter()
                .zip(&engine)
                .map(|(frame, y)| (frame.t, *y, frame.y)),
        );
        assert!(
            worst <= refined_bound(step),
            "step {step}: {worst} at {at} s"
        );
        assert_eq!(at, 187.0, "step {step}");
        for (frame, y) in frames.iter().zip(&engine) {
            if frame.reset {
                assert!(
                    (y - frame.y).abs() <= EXACT,
                    "step {step}, reset at {} s",
                    frame.t
                );
            }
        }
        // First order: halving the step halves the largest difference.
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
