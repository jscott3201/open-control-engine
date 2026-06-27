//! Typed `CDL.*.Sources.Pulse` goldens derived from the Buildings source equations.

use crate::oracle::{Golden, Sample, ValueKind};

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}

/// Build Logical/Reals/Integers `Sources.Pulse` goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    {
        let time = vec![-1.4, -1.0, 0.0, 0.6, 1.0, 2.6, 3.0, 9.0, 10.6];
        let width = 0.2;
        let period = 2.0;
        let shift = 0.6;
        let samples = time
            .iter()
            .copied()
            .map(|t| b(logical_pulse_y(t, width, period, shift)))
            .collect();
        out.push(Golden::new(
            "CDL.Logical.Sources.Pulse",
            "y",
            ValueKind::Boolean,
            time,
            samples,
            "width=0.2, period=2, shift=0.6; negative start, exact rising and falling boundaries",
            "Buildings Logical/Sources/Pulse.mo: t0=round(integer(time/period)*period+mod(shift,period),n=6), t1=t0+width*period, y true on [t0,t1)",
        ));
    }

    {
        let time = vec![0.0, 0.1, 0.49, 0.5, 2.1, 2.5, 4.1, 4.5];
        let width = 0.2;
        let period = 2.0;
        let shift = -1.9;
        let samples = time
            .iter()
            .copied()
            .map(|t| b(logical_pulse_y(t, width, period, shift)))
            .collect();
        out.push(
            Golden::new(
                "CDL.Logical.Sources.Pulse",
                "y",
                ValueKind::Boolean,
                time,
                samples,
                "scenario=negative_shift_folded; width=0.2, period=2, shift=-1.9",
                "Modelica mod(shift,period) folds negative shifts into the positive period interval; falling boundary is exclusive",
            )
            .with_scenario("negative_shift_folded"),
        );
    }

    {
        let time = vec![-10.0, -1.4, -1.0, 0.0, 0.6, 1.0, 2.6, 9.0];
        let width = 1.0;
        let period = 2.0;
        let shift = 0.6;
        let samples = time
            .iter()
            .copied()
            .map(|t| b(logical_pulse_y(t, width, period, shift)))
            .collect();
        out.push(
            Golden::new(
                "CDL.Logical.Sources.Pulse",
                "y",
                ValueKind::Boolean,
                time,
                samples,
                "scenario=width_one; width=1, period=2, shift=0.6",
                "width=1 yields coincident periodic false/true sample times; the first when-branch keeps y true",
            )
            .with_scenario("width_one"),
        );
    }

    {
        let time = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        let amplitude = 2.0;
        let width = 0.5;
        let period = 1.0;
        let shift = 0.0;
        let offset = 0.2;
        let samples = time
            .iter()
            .copied()
            .map(|t| r(real_pulse_y(t, amplitude, width, period, shift, offset)))
            .collect();
        out.push(Golden::new(
            "CDL.Reals.Sources.Pulse",
            "y",
            ValueKind::Real,
            time,
            samples,
            "amplitude=2, width=0.5, period=1, shift=0, offset=0.2",
            "Buildings Reals/Sources/Pulse.mo composite: Logical.Sources.Pulse -> BooleanToReal(realTrue=offset+amplitude, realFalse=offset)",
        ));
    }

    {
        let time = vec![0.0, 0.25, 0.75, 1.25];
        let amplitude = 3_i64;
        let width = 0.5;
        let period = 1.0;
        let shift = -1.25;
        let offset = -2_i64;
        let samples = time
            .iter()
            .copied()
            .map(|t| i(integer_pulse_y(t, amplitude, width, period, shift, offset)))
            .collect();
        out.push(Golden::new(
            "CDL.Integers.Sources.Pulse",
            "y",
            ValueKind::Integer,
            time,
            samples,
            "amplitude=3, width=0.5, period=1, shift=-1.25, offset=-2",
            "Buildings Integers/Sources/Pulse.mo composite: Logical.Sources.Pulse -> BooleanToInteger(integerTrue=offset+amplitude, integerFalse=offset)",
        ));
    }

    out
}

fn real_pulse_y(
    t: f64,
    amplitude: f64,
    width: f64,
    period: f64,
    shift: f64,
    offset: f64,
) -> f64 {
    if logical_pulse_y(t, width, period, shift) {
        offset + amplitude
    } else {
        offset
    }
}

fn integer_pulse_y(
    t: f64,
    amplitude: i64,
    width: f64,
    period: f64,
    shift: f64,
    offset: i64,
) -> i64 {
    if logical_pulse_y(t, width, period, shift) {
        offset + amplitude
    } else {
        offset
    }
}

fn logical_pulse_y(t: f64, width: f64, period: f64, shift: f64) -> bool {
    if width >= 1.0 {
        return true;
    }

    let phase = modelica_mod(shift, period);
    let mut t0 = buildings_round_six((t / period).floor() * period + phase);
    let mut t1 = t0 + width * period;

    if t + period < t1 {
        t0 -= period;
        t1 -= period;
    }
    if t >= t1 {
        t0 += period;
    } else if t < t0 {
        t1 -= period;
    }

    if t0 < t1 {
        t >= t0 && t < t1
    } else {
        !(t >= t1 && t < t0)
    }
}

fn modelica_mod(x: f64, y: f64) -> f64 {
    x - (x / y).floor() * y
}

fn buildings_round_six(x: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if x > 0.0 {
        (x * FACTOR + 0.5).floor() / FACTOR
    } else {
        (x * FACTOR - 0.5).ceil() / FACTOR
    }
}
