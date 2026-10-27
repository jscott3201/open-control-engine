//! Fine-step FreezeProtection scenario that resolves the heating-coil PI controllers.
//!
//! FreezeProtection.mo drives both heating-coil PI controllers with `k = 1`, `Ti = 0.5 s`. At the
//! 60 s frame of the staged scenario forward Euler has `k*dt/Ti = 120`, so that column follows the
//! shared discretization rather than the continuous PI. This scenario runs the same source-default
//! graph at `dt = 1/16 s` (`k*dt/Ti = 0.125`), where forward Euler tracks the continuous PI to
//! within one frame of integral action. Every input breakpoint lies on the 1/16 s grid, so the
//! inputs are exactly piecewise constant between frames and the continuous-time PI response has a
//! closed form that `oce-conformance` checks this trace against.
//!
//! The window covers the 300 s stage-1/2/3 timers with the supply air at 0.5 C, steps the
//! mixed-air temperature to drive the stage-3 PI through a ramp, saturation and an unsaturated
//! hold, resets to stage 2 at 312 s, and then steps the supply-air temperature to drive the
//! stage-1 PI through low saturation, a ramp into high saturation, a ramp down and a hold.

use crate::oracle::{Golden, ValueKind};

use super::super::sequence_golden;
use super::{Inputs, freeze_protection_inputs, freeze_protection_trace, input_rows, samples};

/// Scenario name of the fine-step FreezeProtection reference.
pub(in crate::sequences) const FREEZE_PROTECTION_FINE_STEP: &str =
    "multizone_vav_freeze_protection_fine_step";

/// Frame step: 1/16 s, exact in binary so every frame-to-frame `dt` is identical.
const SAMPLE_STEP: f64 = 0.0625;
const TICKS_PER_SECOND: u32 = 16;
const T_STOP_SECONDS: u32 = 330;

const OUTPUTS: [(&str, ValueKind); 2] = [
    ("freeze_protection_stage", ValueKind::Integer),
    ("heating_coil_command", ValueKind::Real),
];

pub(super) fn goldens() -> Vec<Golden> {
    let time = expected_times();
    let rows = input_rows(&time, input_row);
    let inputs = freeze_protection_inputs(&rows);
    let trace = freeze_protection_trace(&time, input_row);

    OUTPUTS
        .iter()
        .map(|&(signal, kind)| {
            sequence_golden(
                FREEZE_PROTECTION_FINE_STEP,
                signal,
                kind,
                time.clone(),
                samples(signal, &trace),
                "FreezeProtection source-default at a 1/16 s frame step (k*dt/Ti=0.125): supply air at 0.5 C latches stages 1, 2 and 3 at 300 s; mixed air steps to 299.75 K at 300 s, 300.5 K at 304 s and 300.15 K at 305 s; operator reset to stage 2 at 312 s; supply air steps to 280.0 K at 314 s, 278.85 K at 318 s, 279.35 K at 322 s and 279.15 K at 323.5 s",
                "Pinned FreezeProtection.mo source-default graph at a frame step well below the heating-coil PI integral time (Ti=0.5 s), so forward Euler tracks the continuous PI with anti-windup; every input breakpoint lies on the frame grid",
                inputs.clone(),
            )
        })
        .collect()
}

fn expected_times() -> Vec<f64> {
    (0..=T_STOP_SECONDS * TICKS_PER_SECOND)
        .map(|tick| f64::from(tick) * SAMPLE_STEP)
        .collect()
}

fn input_row(t: f64) -> Inputs {
    Inputs {
        outdoor_damper_min_position: 0.20,
        outdoor_damper: 0.62,
        heating_coil: 0.31,
        minimum_outdoor_damper: 0.17,
        return_damper: 0.73,
        supply_air_temperature: supply_air_temperature(t),
        software_reset: t == 312.0,
        supply_fan_status: true,
        supply_fan_speed: 0.58,
        cooling_coil: 0.44,
        mixed_air_temperature: mixed_air_temperature(t),
    }
}

/// Supply-air temperature in K. Below 1 C until 314 s, then around the 279.15 K stage-1 setpoint.
fn supply_air_temperature(t: f64) -> f64 {
    if t < 314.0 {
        273.65
    } else if t < 318.0 {
        280.0
    } else if t < 322.0 {
        278.85
    } else if t < 323.5 {
        279.35
    } else {
        279.15
    }
}

/// Mixed-air temperature in K, around the 300.15 K stage-3 setpoint.
fn mixed_air_temperature(t: f64) -> f64 {
    if t < 300.0 {
        300.15
    } else if t < 304.0 {
        299.75
    } else if t < 305.0 {
        300.5
    } else {
        300.15
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_input_breakpoint_lies_on_the_frame_grid() {
        for breakpoint in [300.0, 304.0, 305.0, 312.0, 314.0, 318.0, 322.0, 323.5] {
            assert_eq!((breakpoint / SAMPLE_STEP).fract(), 0.0, "{breakpoint}");
        }
        let time = expected_times();
        assert!(time.windows(2).all(|pair| pair[1] - pair[0] == SAMPLE_STEP));
        assert_eq!(time.last().copied(), Some(f64::from(T_STOP_SECONDS)));
    }

    #[test]
    fn frame_step_is_well_below_the_heating_coil_integral_time() {
        // FreezeProtection.mo: k = 1, Ti = 0.5 s for both heating-coil PI controllers.
        let ratio = 1.0 * SAMPLE_STEP / 0.5;
        assert_eq!(ratio, 0.125);
    }

    #[test]
    fn scenario_reaches_stage_three_then_resets_to_stage_two() {
        let time = expected_times();
        let trace = freeze_protection_trace(&time, input_row);
        let stage_at = |t: f64| trace.freeze_protection_stage[(t / SAMPLE_STEP) as usize];
        assert_eq!(stage_at(299.9375), 0);
        assert_eq!(stage_at(300.0), 3);
        assert_eq!(stage_at(311.9375), 3);
        assert_eq!(stage_at(312.0), 2);
        assert_eq!(stage_at(330.0), 2);
    }
}
