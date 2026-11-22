//! The Building 33 day against OpenModelica's simulation of upstream's own verification model
//! (issue #755). `tests/fixtures/commissioning/building33/openmodelica/yCooVal_day.csv` is
//! `ControlsVerification_CoolingCoilValve`, unmodified at the vendored pin, simulated by the
//! pinned OpenModelica 1.25.1 container (`run_openmodelica.sh`; cvode, tolerance 1e-6): the
//! continuous-time model, its trends interpolated linearly, its `CoolingCoilValve` upstream's
//! and not the transcription. The test reads the checked-in file; no container runs here.
//!
//! - The engine's valve signal over the day, from the transcription, stays within 0.0034 of
//!   OpenModelica's at every 5 s instant: the transcription and the engine's discrete time
//!   reproduce the continuous model far inside the 0.01 band.
//! - OpenModelica's signal, put through the same funnel against the same trend, leaves the band
//!   in the same afternoon from the same first instant, worst 0.0234 at 67,335 s against the
//!   engine's 0.0209 at 65,965 s: upstream's own model run by OpenModelica fails this day too,
//!   so the day's `fail` is the recorded controller's departure from the CDL sequence and not
//!   the transcription's.

use oce_verify::{CommissionOptions, CompareLimits, Series, Tolerances, funnel};

use super::building_33_day::{example_file, example_inputs, example_spec};

/// A series as two columns: its times, then its values.
type Columns = (Vec<f64>, Vec<f64>);

/// A two-column `x,y` CSV file of this fixture, read as numbers.
fn series(text: &str) -> Columns {
    text.lines()
        .skip(1)
        .map(|line| {
            let (x, y) = line.split_once(',').expect("two fields");
            (
                x.parse::<f64>().expect("a time"),
                y.parse::<f64>().expect("a value"),
            )
        })
        .unzip()
}

fn openmodelica() -> Columns {
    series(&String::from_utf8(example_file("openmodelica/yCooVal_day.csv")).expect("UTF-8"))
}

/// The engine's valve signal and the trend, at the day's 5 s instants, from the run.
fn run_series() -> (Columns, Columns) {
    let mut options = CommissionOptions::default();
    options.bounds = true;
    let inputs = example_inputs(
        example_file("cooling_coil_valve.cxf.json"),
        example_file("b33-points.json"),
    );
    let run = oce_verify::run_commissioning(&example_spec(&[]), 0, &inputs, &options)
        .expect("the day runs");
    let bounds = run.curves[0].bounds.as_ref().expect("an envelope");
    (
        (bounds.reference.x().to_vec(), bounds.reference.y().to_vec()),
        (bounds.test.x().to_vec(), bounds.test.y().to_vec()),
    )
}

#[test]
fn the_engine_stays_within_0_0034_of_openmodelica_at_every_instant_of_the_day() {
    let (om_t, om_y) = openmodelica();
    let ((engine_t, engine_y), _) = run_series();
    assert_eq!(om_t, engine_t, "the same 17,281 instants");
    let (worst, at) = engine_y
        .iter()
        .zip(&om_y)
        .zip(&engine_t)
        .map(|((e, o), t)| ((e - o).abs(), *t))
        .fold((0.0_f64, 0.0_f64), |best, next| {
            if next.0 > best.0 { next } else { best }
        });
    let mean = engine_y
        .iter()
        .zip(&om_y)
        .map(|(e, o)| (e - o).abs())
        .sum::<f64>()
        / engine_y.len() as f64;
    assert!(worst < 0.0034, "largest difference {worst} at {at} s");
    assert_eq!(at, 67_375.0, "where the largest difference is");
    assert!(mean < 1.5e-4, "mean absolute difference {mean}");
}

#[test]
fn openmodelica_s_signal_also_leaves_the_band_around_the_trend_in_the_same_afternoon() {
    let (om_t, om_y) = openmodelica();
    let (_, (trend_t, trend_y)) = run_series();
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.01,
        ltolx: 0.0,
        ltoly: 0.0,
    };
    let result = funnel(
        Series::new(&om_t, &om_y),
        Series::new(&trend_t, &trend_y),
        &tolerances,
        &CompareLimits::default(),
    )
    .expect("the funnel compares");
    let errors = result.errors_at_test();
    let outside: Vec<f64> = trend_t
        .iter()
        .zip(errors)
        .filter(|(_, error)| **error > 0.0)
        .map(|(t, _)| *t)
        .collect();
    let (worst, at) = trend_t
        .iter()
        .zip(errors)
        .map(|(t, error)| (*error, *t))
        .fold((0.0_f64, 0.0_f64), |best, next| {
            if next.0 > best.0 { next } else { best }
        });
    // 834 instants, 61,770 to 67,755 s; the engine's run: 817 (its `exceedance_time_s` of
    // 4,085 s over 5 s instants), over the same stretch, 61,770 to 67,755 s.
    assert_eq!(outside.len(), 834);
    assert_eq!(
        (outside.first().copied(), outside.last().copied()),
        (Some(61_770.0), Some(67_755.0))
    );
    assert_eq!(
        (worst.to_bits(), at),
        (0.023_447_070_857_326_002_f64.to_bits(), 67_335.0)
    );
}
