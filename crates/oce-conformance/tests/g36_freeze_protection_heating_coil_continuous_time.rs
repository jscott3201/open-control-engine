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
//! The fine-step reference (`dt = 1/16 s`, `k*dt/Ti = 0.125`) must stay within one frame of the
//! largest integral rate of that closed form. The 60 s reference (`k*dt/Ti = 120`) must not: it is
//! labelled frame-rate dependent, and this oracle shows the departure is real.

use std::path::{Path, PathBuf};

use oce_conformance::CombiTimeTable;

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

/// Closed-form continuous-time CDL PI (no derivative, `r = 1`, reverse acting).
#[derive(Clone, Copy)]
struct ContinuousPi {
    k: f64,
    ti: f64,
    ni: f64,
    y_min: f64,
    y_max: f64,
}

impl ContinuousPi {
    fn output(&self, x: f64, e: f64) -> f64 {
        (self.k * e + x).clamp(self.y_min, self.y_max)
    }

    /// `der(xI)` at state `x` and error `e`.
    fn rate(&self, x: f64, e: f64) -> f64 {
        let unlimited = self.k * e + x;
        let limited = unlimited.clamp(self.y_min, self.y_max);
        (self.k / self.ti) * (e - (unlimited - limited) / (self.k * self.ni))
    }

    /// Exact integrator state after `duration` seconds of constant error `e`, starting from `x`.
    fn advance(&self, mut x: f64, e: f64, duration: f64) -> f64 {
        let tau = self.ti * self.ni;
        let mut remaining = duration;
        // Each pass ends in a mode switch; a constant error crosses at most three modes.
        for _ in 0..4 {
            if remaining <= 0.0 {
                return x;
            }
            let unlimited = self.k * e + x;
            let saturated_at = if unlimited > self.y_max || (unlimited == self.y_max && e > 0.0) {
                Some(self.y_max)
            } else if unlimited < self.y_min || (unlimited == self.y_min && e < 0.0) {
                Some(self.y_min)
            } else {
                None
            };
            match saturated_at {
                Some(limit) => {
                    // der(x) = -(x - target)/tau; the loop leaves this limit when x reaches
                    // `boundary`, which happens only if the target lies beyond it.
                    let target = limit + self.k * e * (self.ni - 1.0);
                    let boundary = limit - self.k * e;
                    let leaves = (x - boundary) * (target - boundary) < 0.0;
                    if leaves {
                        let exit = tau * ((x - target) / (boundary - target)).ln();
                        if exit < remaining {
                            x = boundary;
                            remaining -= exit;
                            continue;
                        }
                    }
                    return target + (x - target) * (-remaining / tau).exp();
                }
                None => {
                    let slope = (self.k / self.ti) * e;
                    if slope == 0.0 {
                        return x;
                    }
                    let limit = if slope > 0.0 { self.y_max } else { self.y_min };
                    let reach = (limit - self.k * e - x) / slope;
                    if reach >= remaining {
                        return x + slope * remaining;
                    }
                    x = limit - self.k * e;
                    remaining -= reach;
                }
            }
        }
        panic!("constant-error PI response switched modes more than three times");
    }
}

/// Forward Euler with the frame convention of the HostTick engine: each frame's output uses the
/// state before the frame, and the frame then integrates its own error over the time since the
/// previous frame (nothing on the first frame).
fn forward_euler(pi: &ContinuousPi, errors: &[f64], step: f64) -> Vec<f64> {
    let mut x = 0.0;
    let mut out = Vec::with_capacity(errors.len());
    for (frame, &e) in errors.iter().enumerate() {
        out.push(pi.output(x, e));
        let dt = if frame == 0 { 0.0 } else { step };
        x += pi.rate(x, e) * dt;
    }
    out
}

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

/// Continuous-time PI outputs and integral rates sampled at each frame instant.
struct ClosedForm {
    output: Vec<f64>,
    rate: Vec<f64>,
}

fn closed_form(pi: &ContinuousPi, time: &[f64], errors: &[f64]) -> ClosedForm {
    let mut x = 0.0;
    let mut output = Vec::with_capacity(time.len());
    let mut rate = Vec::with_capacity(time.len());
    for (row, &e) in errors.iter().enumerate() {
        output.push(pi.output(x, e));
        rate.push(pi.rate(x, e));
        if let Some(next) = time.get(row + 1) {
            x = pi.advance(x, e, next - time[row]);
        }
    }
    ClosedForm { output, rate }
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

/// Largest `|golden - closed form|` over rows whose source the stage decides, and the largest
/// closed-form integral rate on those rows.
fn compare(rows: &HeatingCoilRows, stage_two_uses_stage_one_pi: bool) -> (f64, f64, usize) {
    let stage_one = closed_form(&HEATING_COIL_PI, &rows.time, &rows.stage_one_errors());
    let stage_three = closed_form(&HEATING_COIL_PI, &rows.time, &rows.stage_three_errors());
    let mut max_error: f64 = 0.0;
    let mut max_rate: f64 = 0.0;
    let mut pi_rows = 0;
    for row in 0..rows.time.len() {
        let Some(source) = source(rows.stage[row], stage_two_uses_stage_one_pi) else {
            continue;
        };
        let golden = rows.heating_coil_command[row];
        let (expected, rate) = match source {
            Source::HeatingCoilInput => {
                assert_eq!(
                    golden.to_bits(),
                    rows.heating_coil_input[row].to_bits(),
                    "stage 0 passes uHeaCoi through at t={}",
                    rows.time[row]
                );
                continue;
            }
            Source::StageOnePi => (stage_one.output[row], stage_one.rate[row]),
            Source::StageThreePi => (stage_three.output[row], stage_three.rate[row]),
        };
        pi_rows += 1;
        max_error = max_error.max((golden - expected).abs());
        max_rate = max_rate.max(rate.abs());
    }
    (max_error, max_rate, pi_rows)
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

#[test]
fn fine_step_heating_coil_reference_tracks_the_continuous_time_pi() {
    let rows = HeatingCoilRows::read(FINE_STEP_SEQUENCE);
    let step = frame_step(&rows.time);
    assert_eq!(step, 0.0625);
    assert_eq!(HEATING_COIL_PI.k * step / HEATING_COIL_PI.ti, 0.125);

    // The 300 s stage-1 timer latches stage 1 together with stages 2 and 3, and the supply air
    // never stays above 7 C for 300 s in this window, so stage 2 carries the stage-1 PI.
    let (max_error, max_rate, pi_rows) = compare(&rows, true);
    assert!(pi_rows > 400, "only {pi_rows} rows carry a PI output");

    // One frame of the largest integral action: forward Euler's first-frame skip and its
    // truncation in the anti-windup relaxation (time constant Ti*Ni = 0.45 s, step ratio 0.139)
    // and at mode corners each stay below a single frame's increment of the integrator.
    let tolerance = step * max_rate;
    // The largest rate is the stage-1 PI leaving high saturation at 314 s:
    // (k/Ti)*(e - (yU - y)/(k*Ni)) with e = -0.85 and xI near 0.45, about 0.81/s.
    assert!(
        (0.050..0.052).contains(&tolerance),
        "tolerance {tolerance} drifted; revisit the scenario"
    );
    assert!(
        max_error <= tolerance,
        "fine-step heating coil departs from the continuous PI by {max_error} > {tolerance}"
    );
    assert!(max_error > 0.0, "a discretized trace cannot match exactly");
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
    // Stage 2 is skipped here: the 60 s scenario reaches it with and without the stage-1 latch.
    let (max_error, _, pi_rows) = compare(&rows, false);
    assert!(pi_rows >= 10, "only {pi_rows} rows carry a PI output");
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
    let max_departure = |step: f64| -> f64 {
        let (time, errors) = errors_at(step);
        let exact = closed_form(&HEATING_COIL_PI, &time, &errors).output;
        // Euler's first-frame skip is part of its O(step) departure, so no shift is applied.
        let euler = forward_euler(&HEATING_COIL_PI, &errors, step);
        euler
            .iter()
            .zip(&exact)
            .map(|(euler, exact)| (euler - exact).abs())
            .fold(0.0, f64::max)
    };
    let coarse = max_departure(1.0 / 16.0);
    let fine = max_departure(1.0 / 256.0);
    let finest = max_departure(1.0 / 4096.0);
    assert!(coarse < 1e-3, "1/16 s departure {coarse}");
    assert!(finest < 1e-5, "1/4096 s departure {finest}");
    // First-order convergence: each 16x refinement shrinks the departure by about 16.
    for (wide, narrow) in [(coarse, fine), (fine, finest)] {
        let ratio = wide / narrow;
        assert!(
            (8.0..32.0).contains(&ratio),
            "forward Euler does not converge to the closed form at first order: {ratio}"
        );
    }
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
