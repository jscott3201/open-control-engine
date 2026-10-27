//! Continuous-time oracle for the G36 FreezeProtection heating-coil command.
//!
//! FreezeProtection.mo drives `yHeaCoi` from two CDL `PID` blocks (`heaCoiCon1` in stages 1 and 2,
//! `heaCoiMod` in stage 3), both PI with `k = 1`, `Ti = 0.5 s` and the CDL defaults `Ni = 0.9`,
//! `r = 1`, `yMin = 0`, `yMax = 1`, `reverseActing = true`. Per `CDL/Reals/PID.mo` and
//! `IntegratorWithReset.mo`, with `e = u_s - u_m`:
//!
//! ```text
//! yU = k*e + xI,   y = min(max(yU, yMin), yMax),   der(xI) = (k/Ti)*(e - (yU - y)/(k*Ni))
//! ```
//!
//! For a constant `e` this is piecewise linear: unsaturated, `xI` ramps at `(k/Ti)*e`; saturated
//! at a limit `L`, it relaxes exponentially with time constant `Ti*Ni` towards
//! `L + k*e*(Ni - 1)`. The golden's inputs change only on its frame grid, so between frames `e` is
//! exactly constant and the response has a closed form, solved here event by event without any
//! time stepping. This oracle shares no code with golden-gen or the engine.
//!
//! Each row of the fine-step reference (`dt = 1/16 s`, `k*dt/Ti = 0.125`) must lie within a
//! per-row bound on how far forward Euler can drift from that closed form: one-frame truncation
//! errors taken from the closed form itself, carried through the Euler map's Lipschitz constant
//! (see `continuous_pi::closed_form`). The bound is sharp, and it rejects a reference whose `Ti` is off by 1%.
//! The 60 s reference (`k*dt/Ti = 120`) gets no bound: it is labelled frame-rate dependent, and
//! this oracle shows the departure is real.
//!
//! Limits of this oracle:
//!
//! - FreezeProtection.mo has `k = 1`, where the anti-windup gains `1/(k*Ni)`, `k/Ni` and `1/Ni`
//!   coincide, so these goldens cannot tell them apart. The `k != 1` placement is pinned by the
//!   CDL PID Tier-A goldens (`k = 0.37`, `Ti = 0.3 s`, saturating at `yMax = 0.42`), which do
//!   separate the three forms and which the engine matches exactly in
//!   `per_block_reals_dynamics.rs` (`reals_dynamics_blocks_match_exact_oracle`, cases
//!   `reals_pid_pi_recurrence` and `reals_pid_with_reset_pi_recurrence`). No `oce-blocks` unit
//!   test exercises anti-windup with `k != 1`. Here, the convergence test runs the closed form at
//!   `k = 0.37` as well.
//! - In the fine-step scenario, the rows on which a PI drives the output come from a stage
//!   schedule derived from FreezeProtection.mo (`fine_step_expected_stage`), and the golden's
//!   stage column must equal it. The 60 s check uses the golden's own stage column.

use std::path::{Path, PathBuf};

use oce_conformance::CombiTimeTable;

#[path = "g36_freeze_protection_heating_coil_continuous_time/continuous_pi.rs"]
mod continuous_pi;

use continuous_pi::{ContinuousPi, closed_form, forward_euler};

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";
const FINE_STEP_SEQUENCE: &str = "multizone_vav_freeze_protection_fine_step";
const COARSE_SEQUENCE: &str = "multizone_vav_freeze_protection";
const FREEZE_PROTECTION_MO: &str = "../../third_party/modelica-buildings-cdl/Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/FreezeProtection.mo";
const PID_MO: &str =
    "../../third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Reals/PID.mo";

/// `supTemSet.k` in FreezeProtection.mo: the stage-1 heating-coil setpoint, K.
const STAGE_ONE_SETPOINT: f64 = 273.15 + 6.0;
/// `con4.k` in FreezeProtection.mo: the stage-3 heating-coil setpoint, K.
const STAGE_THREE_SETPOINT: f64 = 273.15 + 27.0;

/// FreezeProtection.mo heating-coil PI: `k = 1`, `Ti = 0.5 s`, CDL defaults otherwise.
const HEATING_COIL_PI: ContinuousPi = ContinuousPi {
    k: 1.0,
    ti: 0.5,
    ni: 0.9,
    y_min: 0.0,
    y_max: 1.0,
};

struct HeatingCoilRows {
    time: Vec<f64>,
    stage: Vec<i64>,
    heating_coil_input: Vec<f64>,
    supply_air_temperature: Vec<f64>,
    mixed_air_temperature: Vec<f64>,
    heating_coil_command: Vec<f64>,
}

impl HeatingCoilRows {
    fn read(sequence: &str) -> Self {
        let path = golden_dir(sequence).join("reference.csv");
        let table = CombiTimeTable::read(&path)
            .unwrap_or_else(|err| panic!("read {} failed: {err}", path.display()));
        let names = table.col_names.clone().expect("reference column names");
        let column = |name: &str| -> Vec<f64> {
            let index = names
                .iter()
                .position(|column| column == name)
                .unwrap_or_else(|| panic!("{sequence} has no column {name}"));
            (0..table.n_rows)
                .map(|row| table.data[row * table.n_cols + index])
                .collect()
        };
        Self {
            time: column("time"),
            stage: column("freeze_protection_stage")
                .into_iter()
                .map(|stage| stage as i64)
                .collect(),
            heating_coil_input: column("heating_coil"),
            supply_air_temperature: column("supply_air_temperature"),
            mixed_air_temperature: column("mixed_air_temperature"),
            heating_coil_command: column("heating_coil_command"),
        }
    }

    fn stage_one_errors(&self) -> Vec<f64> {
        self.supply_air_temperature
            .iter()
            .map(|supply| STAGE_ONE_SETPOINT - supply)
            .collect()
    }

    fn stage_three_errors(&self) -> Vec<f64> {
        self.supply_air_temperature
            .iter()
            .zip(&self.mixed_air_temperature)
            .map(|(supply, mixed)| STAGE_THREE_SETPOINT - supply.max(*mixed))
            .collect()
    }
}

/// Which PI the golden's `heating_coil_command` carries at `row`, if the stage decides it.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    HeatingCoilInput,
    StageOnePi,
    StageThreePi,
}

fn source(stage: i64, stage_two_uses_stage_one_pi: bool) -> Option<Source> {
    match stage {
        0 => Some(Source::HeatingCoilInput),
        1 => Some(Source::StageOnePi),
        // Stage 2 carries the stage-1 PI only while the stage-1 latch is also set.
        2 => stage_two_uses_stage_one_pi.then_some(Source::StageOnePi),
        3 => Some(Source::StageThreePi),
        other => panic!("unexpected freeze-protection stage {other}"),
    }
}

/// Stage of the fine-step scenario, derived from FreezeProtection.mo rather than read from the
/// golden. The supply air (273.65 K) is below the 277.55, 276.45 and 274.15 K thresholds from
/// t = 0, so the 300 s timers `tim`, `tim2` and `tim4` all pass at 300 s and latch stages 1, 2
/// and 3. The operator reset at 312 s clears only the stage-3 latch (`tim4` stays passed, so no
/// new rising edge re-latches it). Stage 2 is then held by `holSta2` for 3600 s, and the supply
/// air never exceeds 280.15 K, so the stage-1 clear timer `tim1` never starts.
fn fine_step_expected_stage(t: f64) -> i64 {
    if t < 300.0 {
        0
    } else if t < 312.0 {
        3
    } else {
        2
    }
}

/// How a heating-coil column compares with the closed form over the rows whose PI is known.
struct Comparison {
    max_error: f64,
    /// Largest `error / bound`; at most 1 when every row lies within its bound.
    max_bound_ratio: f64,
    max_bound: f64,
    pi_rows: usize,
}

fn compare(
    rows: &HeatingCoilRows,
    pi: &ContinuousPi,
    stages: &[i64],
    stage_two_uses_stage_one_pi: bool,
) -> Comparison {
    let stage_one = closed_form(pi, &rows.time, &rows.stage_one_errors());
    let stage_three = closed_form(pi, &rows.time, &rows.stage_three_errors());
    let mut comparison = Comparison {
        max_error: 0.0,
        max_bound_ratio: 0.0,
        max_bound: 0.0,
        pi_rows: 0,
    };
    assert_eq!(stages.len(), rows.time.len());
    for (row, &stage) in stages.iter().enumerate() {
        let Some(source) = source(stage, stage_two_uses_stage_one_pi) else {
            continue;
        };
        let golden = rows.heating_coil_command[row];
        let closed = match source {
            Source::HeatingCoilInput => {
                assert_eq!(
                    golden.to_bits(),
                    rows.heating_coil_input[row].to_bits(),
                    "stage 0 passes uHeaCoi through at t={}",
                    rows.time[row]
                );
                continue;
            }
            Source::StageOnePi => &stage_one,
            Source::StageThreePi => &stage_three,
        };
        let error = (golden - closed.output[row]).abs();
        let bound = closed.bound[row];
        comparison.pi_rows += 1;
        comparison.max_error = comparison.max_error.max(error);
        comparison.max_bound = comparison.max_bound.max(bound);
        comparison.max_bound_ratio = comparison.max_bound_ratio.max(error / bound);
    }
    comparison
}

fn golden_dir(sequence: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(sequence)
}

fn frame_step(time: &[f64]) -> f64 {
    let step = time[1] - time[0];
    assert!(
        time.windows(2).all(|pair| pair[1] - pair[0] == step),
        "reference grid is uniform"
    );
    step
}

fn fine_step_comparison(pi: &ContinuousPi) -> Comparison {
    let rows = HeatingCoilRows::read(FINE_STEP_SEQUENCE);
    let stages = rows
        .time
        .iter()
        .map(|&t| fine_step_expected_stage(t))
        .collect::<Vec<_>>();
    compare(&rows, pi, &stages, true)
}

#[test]
fn fine_step_stage_follows_the_hand_derived_schedule() {
    let rows = HeatingCoilRows::read(FINE_STEP_SEQUENCE);
    for (row, &t) in rows.time.iter().enumerate() {
        assert_eq!(
            rows.stage[row],
            fine_step_expected_stage(t),
            "fine-step stage at t={t}"
        );
    }
}

#[test]
fn fine_step_heating_coil_reference_tracks_the_continuous_time_pi() {
    let rows = HeatingCoilRows::read(FINE_STEP_SEQUENCE);
    let step = frame_step(&rows.time);
    assert_eq!(step, 0.0625);
    assert_eq!(HEATING_COIL_PI.k * step / HEATING_COIL_PI.ti, 0.125);

    let comparison = fine_step_comparison(&HEATING_COIL_PI);
    assert!(
        comparison.pi_rows > 400,
        "only {} rows carry a PI output",
        comparison.pi_rows
    );
    // Every row lies within its own forward-Euler bound. Observed: largest error 0.0049, largest
    // bound 0.0099. The bound is sharp: on unsaturated rows after a single mode corner the Euler
    // map has slope 1 and the error equals the bound to within the rounding allowance (largest
    // error/bound 0.99999991). Rows in a hold or a long saturation carry bounds near zero, which
    // is what makes a mistuned reference fail by orders of magnitude.
    assert!(
        comparison.max_bound_ratio <= 1.0,
        "fine-step heating coil leaves its forward-Euler bound: error/bound = {}",
        comparison.max_bound_ratio
    );
    assert!(
        comparison.max_bound < 0.011,
        "bound {} drifted; revisit the scenario",
        comparison.max_bound
    );
    assert!(
        comparison.max_error > 0.0,
        "a discretized trace cannot match exactly"
    );
}

/// A reference built from a PI with the wrong parameters must leave the bound. Each case solves
/// the closed form with one parameter off, which is equivalent to checking a golden generated
/// with the correct parameters against that mistuned PI. Observed largest error/bound ratios:
/// Ti -1% 1.2e6, Ti +1% 1.2e6, Ti -2% 2.3e6, Ti +2% 2.4e6, Ti -5% 6.0e6, Ti +5% 5.9e6,
/// Ni -5% 1.1e4, Ni +5% 606, yMax 0.95 6.9e9.
#[test]
fn fine_step_bound_rejects_a_mistuned_pi() {
    let mutants = [
        (
            "Ti -2%",
            ContinuousPi {
                ti: 0.49,
                ..HEATING_COIL_PI
            },
        ),
        (
            "Ti +2%",
            ContinuousPi {
                ti: 0.51,
                ..HEATING_COIL_PI
            },
        ),
        (
            "Ti -5%",
            ContinuousPi {
                ti: 0.475,
                ..HEATING_COIL_PI
            },
        ),
        (
            "Ti +5%",
            ContinuousPi {
                ti: 0.525,
                ..HEATING_COIL_PI
            },
        ),
        (
            "Ni -5%",
            ContinuousPi {
                ni: 0.855,
                ..HEATING_COIL_PI
            },
        ),
        (
            "Ni +5%",
            ContinuousPi {
                ni: 0.945,
                ..HEATING_COIL_PI
            },
        ),
        (
            "yMax 0.95",
            ContinuousPi {
                y_max: 0.95,
                ..HEATING_COIL_PI
            },
        ),
        (
            "Ti -1%",
            ContinuousPi {
                ti: 0.495,
                ..HEATING_COIL_PI
            },
        ),
        (
            "Ti +1%",
            ContinuousPi {
                ti: 0.505,
                ..HEATING_COIL_PI
            },
        ),
    ];
    for (name, pi) in mutants {
        let ratio = fine_step_comparison(&pi).max_bound_ratio;
        assert!(
            ratio > 1.0,
            "{name}: error/bound {ratio} is within the bound"
        );
    }
}

#[test]
fn fine_step_heating_coil_reference_exercises_ramps_saturation_and_holds() {
    let rows = HeatingCoilRows::read(FINE_STEP_SEQUENCE);
    let value_at = |t: f64| {
        let row = rows.time.iter().position(|time| *time == t).expect("row");
        (rows.stage[row], rows.heating_coil_command[row])
    };
    assert_eq!(value_at(299.9375), (0, 0.31));
    // Stage 3: ramp from 0.4 at 0.8/s into saturation, then a ramp down to an unsaturated hold.
    let (stage, ramp) = value_at(300.5);
    assert_eq!(stage, 3);
    assert!((ramp - 0.8).abs() < 1e-9, "stage-3 ramp at 300.5 s: {ramp}");
    assert_eq!(value_at(303.0), (3, 1.0));
    let (_, hold) = value_at(311.9375);
    assert!(hold > 0.2 && hold < 0.3, "stage-3 hold {hold}");
    assert_eq!(value_at(312.0), (2, 1.0));
    // Stage 2 on the stage-1 PI: low saturation, ramp into high saturation, ramp down, hold.
    assert_eq!(value_at(317.0), (2, 0.0));
    let (_, ramp) = value_at(318.5);
    assert!(ramp > 0.6 && ramp < 0.8, "stage-1 ramp {ramp}");
    assert_eq!(value_at(321.0), (2, 1.0));
    let (_, hold) = value_at(330.0);
    assert!(hold > 0.3 && hold < 0.45, "stage-1 hold {hold}");
}

#[test]
fn coarse_step_heating_coil_reference_departs_from_the_continuous_time_pi() {
    let rows = HeatingCoilRows::read(COARSE_SEQUENCE);
    let step = frame_step(&rows.time);
    assert_eq!(HEATING_COIL_PI.k * step / HEATING_COIL_PI.ti, 120.0);
    // The PI-active rows come from the golden's own stage column here. Stage 2 is skipped: the
    // 60 s scenario reaches it with and without the stage-1 latch.
    let comparison = compare(&rows, &HEATING_COIL_PI, &rows.stage, false);
    let max_error = comparison.max_error;
    assert!(
        comparison.pi_rows >= 10,
        "only {} rows carry a PI output",
        comparison.pi_rows
    );
    assert!(
        max_error >= 0.5,
        "the 60 s column is labelled frame-rate dependent but tracks the continuous PI \
         (max error {max_error}); revisit its frame_rate_dependence note"
    );
}

#[test]
fn closed_form_and_fine_forward_euler_converge_to_each_other() {
    // An error schedule crossing every mode: high saturation, exit, ramp to low saturation,
    // ramp into high saturation, and a hold.
    let segments = [
        (5.5, 4.0),
        (-0.85, 4.0),
        (0.3, 4.0),
        (-0.2, 1.5),
        (0.0, 2.0),
    ];
    let errors_at = |step: f64| -> (Vec<f64>, Vec<f64>) {
        let mut time = Vec::new();
        let mut errors = Vec::new();
        let mut t = 0.0;
        for (error, length) in segments {
            let frames = (length / step) as usize;
            for _ in 0..frames {
                time.push(t);
                errors.push(error);
                t += step;
            }
        }
        (time, errors)
    };
    let max_departure = |pi: &ContinuousPi, step: f64| -> f64 {
        let (time, errors) = errors_at(step);
        let closed = closed_form(pi, &time, &errors);
        // Euler's first-frame skip is part of its O(step) departure, so no shift is applied.
        let euler = forward_euler(pi, &errors, step);
        for (row, value) in euler.iter().enumerate() {
            assert!(
                (value - closed.output[row]).abs() <= closed.bound[row],
                "step {step}: row {row} leaves the forward-Euler bound"
            );
        }
        euler
            .iter()
            .zip(&closed.output)
            .map(|(euler, exact)| (euler - exact).abs())
            .fold(0.0, f64::max)
    };
    // The FreezeProtection PI has k = 1, where 1/(k*Ni), k/Ni and 1/Ni coincide. A k != 1 PI
    // (the gains of the CDL PID Tier-A golden) checks that the closed form's relaxation target
    // L + k*e*(Ni - 1) and time constant Ti*Ni agree with the 1/(k*Ni) anti-windup gain.
    let general_pi = ContinuousPi {
        k: 0.37,
        ti: 0.3,
        ..HEATING_COIL_PI
    };
    for pi in [HEATING_COIL_PI, general_pi] {
        let coarse = max_departure(&pi, 1.0 / 16.0);
        let fine = max_departure(&pi, 1.0 / 256.0);
        let finest = max_departure(&pi, 1.0 / 4096.0);
        assert!(coarse < 1e-3, "k={}: 1/16 s departure {coarse}", pi.k);
        assert!(finest < 1e-5, "k={}: 1/4096 s departure {finest}", pi.k);
        // First-order convergence: each 16x refinement shrinks the departure by about 16.
        for (wide, narrow) in [(coarse, fine), (fine, finest)] {
            let ratio = wide / narrow;
            assert!(
                (8.0..32.0).contains(&ratio),
                "k={}: forward Euler does not converge to the closed form at first order: \
                 {ratio}",
                pi.k
            );
        }
    }
}

#[test]
fn leaving_saturation_does_not_re_enter_it_through_rounding() {
    // A PI and error whose exit boundary `yMax - k*e` rounds back above yMax when `k*e` is added
    // again: the state an explicit mode carry must not misread as still saturated. (No such
    // error exists for k = 1, yMax = 1; a random search over k, yMax and e found this one.)
    let pi = ContinuousPi {
        k: 0.839_700_174_644_322_9,
        y_max: 1.091_783_156_765_493_6,
        ..HEATING_COIL_PI
    };
    let e = -0.899_532_638_512_687_6;
    assert!(
        pi.k * e + (pi.y_max - pi.k * e) > pi.y_max,
        "the rounding precondition no longer holds"
    );
    let boundary = pi.y_max - pi.k * e;
    let target = pi.y_max + pi.k * e * (pi.ni - 1.0);
    let start = boundary + 0.5;
    let tau = pi.ti * pi.ni;
    let exit = tau * ((start - target) / (boundary - target)).ln();
    // After the exit the loop ramps down at (k/Ti)*e for the rest of the interval.
    let after_exit = 0.1;
    let slope = (pi.k / pi.ti) * e;
    let expected = boundary + slope * after_exit;
    let actual = pi.advance(start, e, exit + after_exit);
    assert!(
        (actual - expected).abs() < 1e-9,
        "e={e}: expected the ramp to {expected}, got {actual}"
    );
}

#[test]
fn closed_form_switches_modes_at_the_analytic_instants() {
    let pi = HEATING_COIL_PI;
    // Unsaturated ramp from x = 0 at e = 0.4: yU reaches yMax when x = 0.6, after 0.75 s.
    let at_corner = pi.advance(0.0, 0.4, 0.75);
    assert!((at_corner - 0.6).abs() < 1e-12);
    // Past the corner it relaxes towards yMax + k*e*(Ni - 1) = 0.96.
    let relaxed = pi.advance(0.0, 0.4, 100.0);
    assert!((relaxed - 0.96).abs() < 1e-12);
    // A negative error from there is unsaturated (yU = 0.61) and ramps down at 0.7/s ...
    let ramped = pi.advance(0.96, -0.35, 0.5);
    assert!((ramped - 0.61).abs() < 1e-12, "{ramped}");
    // ... until yU reaches yMin at x = 0.35 after 0.61/0.7 s, then relaxes towards 0.035.
    let corner = 0.61 / 0.7;
    let past_corner = pi.advance(0.96, -0.35, corner + 0.45);
    let expected = 0.035 + (0.35 - 0.035) * (-1.0f64).exp();
    assert!((past_corner - expected).abs() < 1e-12, "{past_corner}");
    // Zero error holds any unsaturated state.
    assert_eq!(pi.advance(0.37, 0.0, 10.0), 0.37);
    // Saturated with zero error, the state relaxes to the limit and never leaves it.
    let settled = pi.advance(1.5, 0.0, 50.0);
    assert!((settled - 1.0).abs() < 1e-12);
    assert_eq!(pi.output(settled, 0.0), 1.0);
}

#[test]
fn oracle_parameters_match_the_vendored_sources() {
    let read = |path: &str| {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
            .unwrap_or_else(|err| panic!("read {path} failed: {err}"));
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let freeze_protection = read(FREEZE_PROTECTION_MO);
    for declaration in [
        "parameter Real k(unit=\"1\")=1",
        "parameter Real Ti(unit=\"s\")=0.5",
        "parameter Real yMax=1",
        "parameter Real yMin=0",
        "Buildings.Controls.OBC.CDL.Reals.Sources.Constant supTemSet( final k=273.15+ 6)",
        "Buildings.Controls.OBC.CDL.Reals.Sources.Constant con4( final k=273.15 + 27)",
        "connect(supTemSet.y, heaCoiCon1.u_s)",
        "connect(con4.y, heaCoiMod.u_s)",
        "connect(max1.y, heaCoiMod.u_m)",
    ] {
        assert!(
            freeze_protection.contains(declaration),
            "vendored FreezeProtection.mo no longer contains `{declaration}`"
        );
    }
    let pid = read(PID_MO);
    for declaration in [
        "min=100*Constants.eps)=0.9 \"Ni*Ti is time constant of anti-windup compensation\"",
        "parameter Boolean reverseActing=true",
        "MultiplyByParameter antWinGai(k=1/(k*Ni))",
        "IntegratorWithReset I( y(nominal=max(abs(yMax), abs(yMin))), final k=k/Ti,",
    ] {
        assert!(
            pid.contains(declaration),
            "vendored CDL PID.mo no longer contains `{declaration}`"
        );
    }
}
