//! `CDL.Reals.Sources` goldens derived from the Buildings source equations.

use crate::oracle::{Golden, Sample, ValueKind};

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

/// Build stateless `CDL.Reals.Sources` goldens beyond Constant.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    {
        let time = vec![-1.0, 0.0, 1.0];
        let samples = time.iter().copied().map(r).collect();
        out.push(Golden::new(
            "CDL.Reals.Sources.CivilTime",
            "y",
            ValueKind::Real,
            time,
            samples,
            "no inputs or parameters; scheduler/model time samples include negative start",
            "y = time; Buildings Controls OBC CDL Reals Sources CivilTime.mo",
        ));
    }

    {
        let time = vec![0.0, 1.0, 2.5, 4.0, 5.0];
        let height = 2.0;
        let duration = 3.0;
        let offset = 0.5;
        let start_time = 1.0;
        let samples = time
            .iter()
            .copied()
            .map(|t| r(source_ramp_y(t, height, duration, offset, start_time)))
            .collect();
        out.push(Golden::new(
            "CDL.Reals.Sources.Ramp",
            "y",
            ValueKind::Real,
            time,
            samples,
            "height=2, duration=3, offset=0.5, startTime=1; before, start, interior, end, after",
            "y = offset + (if time < startTime then 0 else if time < startTime+duration then (time-startTime)*height/duration else height); Buildings Reals/Sources/Ramp.mo",
        ));
    }

    {
        let time = vec![-2.0, -1.0, 0.0, 1.0];
        let height = -2.0;
        let duration = 2.0;
        let offset = 10.0;
        let start_time = -1.0;
        let samples = time
            .iter()
            .copied()
            .map(|t| r(source_ramp_y(t, height, duration, offset, start_time)))
            .collect();
        out.push(
            Golden::new(
                "CDL.Reals.Sources.Ramp",
                "y",
                ValueKind::Real,
                time,
                samples,
                "scenario=negative_height_start; height=-2, duration=2, offset=10, startTime=-1",
                "same source piecewise equation with negative height and negative simulation start",
            )
            .with_scenario("negative_height_start"),
        );
    }

    {
        let time = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let amplitude = 2.0;
        let freq_hz = 0.25;
        let phase = std::f64::consts::FRAC_PI_2;
        let offset = 0.5;
        let start_time = 1.0;
        let samples = time
            .iter()
            .copied()
            .map(|t| r(source_sin_y(t, amplitude, freq_hz, phase, offset, start_time)))
            .collect();
        out.push(
            Golden::new(
                "CDL.Reals.Sources.Sin",
                "y",
                ValueKind::Real,
                time,
                samples,
                "amplitude=2, freqHz=0.25, phase=pi/2, offset=0.5, startTime=1; before start, observable start boundary, phase plumbing, and period samples",
                "y = offset + (if time < startTime then 0 else amplitude*sin(2*pi*freqHz*(time-startTime)+phase)); Buildings Reals/Sources/Sin.mo, deterministic libm 0.2.16",
            )
            .with_provenance(
                "math_library",
                "libm 0.2.16, default-features=false, pure Rust deterministic math",
            ),
        );
    }

    out
}

fn source_ramp_y(t: f64, height: f64, duration: f64, offset: f64, start_time: f64) -> f64 {
    offset
        + if t < start_time {
            0.0
        } else if t < start_time + duration {
            (t - start_time) * height / duration
        } else {
            height
        }
}

fn source_sin_y(
    t: f64,
    amplitude: f64,
    freq_hz: f64,
    phase: f64,
    offset: f64,
    start_time: f64,
) -> f64 {
    offset
        + if t < start_time {
            0.0
        } else {
            amplitude * libm::sin(2.0 * std::f64::consts::PI * freq_hz * (t - start_time) + phase)
        }
}
