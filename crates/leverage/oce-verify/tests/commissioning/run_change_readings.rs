//! What the change-of-value comparison decides, case by case (issue #746, ruled by the owner on
//! 2026-11-22): the two false passes the review of GitHub #744 reproduced now fail, with their
//! figures; the false pass found while building it (a pulse the trend never logged) fails; the
//! trends that are right pass; and the orchestrator's five readings of 2026-11-22 are each
//! pinned. The cases are `tests/fixtures/commissioning/run/change_of_value/`, whose every
//! figure `run_change_of_value.rs` also holds to `derive_run.py`.

use oce_verify::{CommissionOptions, CompareLimits, OutputStatus, Series, Tolerances, funnel};

use super::run_change_of_value::{case_inputs, case_spec_text, report, run_case, run_case_with};
use super::{body_of, commission, curves_named, loaded};

/// (status, compared, worst instant, worst exceedance, first instant, time outside).
fn figures(case: &str, output: &str) -> (OutputStatus, u64, Option<(f64, f64)>, Option<f64>, f64) {
    let run = run_case(case);
    let output = report(&run, output);
    (
        output.status,
        output.compared,
        output.worst.map(|w| (w.t, w.exceedance)),
        output.first_exceedance_t,
        output.exceedance_time_s,
    )
}

/// The setpoint's step is 1 K, its band 0.05 K high: a trend on the wrong side of the step is
/// outside by 0.95 K, as the doubles round it.
const OUTSIDE_BY: f64 = 0.949_999_999_999_988_6;

#[test]
fn a_trend_step_three_seconds_late_fails_under_a_one_second_tolerance() {
    // The engine's setpoint steps at 10 s; the trend, logged on change, at 13 s. The band's
    // lower bound rises at 11 s, and the trend holds the old value until 13 s: outside for 2 s.
    let (status, compared, worst, first, time) = figures("late_step", "TSet");
    assert_eq!(status, OutputStatus::Fail);
    assert_eq!(worst, Some((11.0, OUTSIDE_BY)));
    assert_eq!(first, Some(11.0));
    assert_eq!(time, 2.0);
    // 0, 9, 10, 11, 13, 20 and 30: the series' times and the band's corners at 9 and 11.
    assert_eq!(compared, 7);
}

#[test]
fn under_the_first_rule_the_late_step_passed() {
    // As GitHub #744 first built it: the reference at its own samples, drawn as a ramp, and the
    // test read at its own times. The same step, 3 s late, is inside that band.
    let reference = Series::new(&[0.0, 10.0, 20.0, 30.0], &[0.0, 1.0, 1.0, 1.0]);
    let test = Series::new(&[0.0, 13.0, 30.0], &[0.0, 1.0, 1.0]);
    let tolerances = Tolerances {
        atolx: 1.0,
        atoly: 0.05,
        ..Tolerances::ZERO
    };
    let result = funnel(reference, test, &tolerances, &CompareLimits::default()).expect("runs");
    assert!(result.within_envelope());
}

#[test]
fn a_boolean_toggling_between_two_logged_points_fails() {
    // The engine's enable is false from 10 s to 20 s; the trend logged true at 0 s and 30 s
    // only. The union of both series' times sees the toggle: a mismatch from 10 s to 20 s.
    let (status, compared, worst, first, time) = figures("unlogged_toggle", "enOut");
    assert_eq!(status, OutputStatus::Fail);
    assert_eq!(compared, 4);
    assert_eq!(worst, Some((10.0, 1.0)));
    assert_eq!(first, Some(10.0));
    assert_eq!(time, 10.0);
    let run = run_case("unlogged_toggle");
    assert_eq!(
        curves_named(&run, "enOut").errors.x(),
        [0.0, 10.0, 20.0, 30.0]
    );
    assert_eq!(curves_named(&run, "enOut").errors.y(), [0.0, 1.0, 0.0, 0.0]);
}

#[test]
fn a_pulse_the_trend_never_logged_fails_between_the_bands_corners() {
    // The engine's setpoint is 1 K higher from 10 s to 20 s, and the trend logs only its value
    // at 0 s. At the series' times, 10 s and 20 s, the band is at its widest; between its
    // corners at 11 s and 19 s it is not.
    let (status, _, worst, first, time) = figures("unlogged_pulse", "TSet");
    assert_eq!(status, OutputStatus::Fail);
    assert_eq!(worst, Some((11.0, OUTSIDE_BY)));
    assert_eq!(first, Some(11.0));
    assert_eq!(time, 8.0);
}

#[test]
fn trends_that_follow_the_engine_pass() {
    for (case, output) in [
        // Steps logged at the engine's own frames.
        ("exact_steps", "TSet"),
        ("exact_steps", "enOut"),
        ("exact_steps", "nOut"),
        ("exact_steps", "limOut"),
        // A step half the x tolerance late, and one exactly the tolerance late.
        ("step_within_tolerance", "TSet"),
        ("step_one_tolerance_late", "TSet"),
        // A slow ramp logged on change past a 0.04 K deadband, under a 0.05 K band.
        ("deadband_analogue", "TSet"),
        // A Boolean that switches on an engine frame, logged there as two rows.
        ("boolean_switch_on_a_frame", "enOut"),
        // Nothing ever changes, and the trend logs once.
        ("never_changes", "TSet"),
        ("never_changes", "enOut"),
        ("never_changes", "nOut"),
        ("never_changes", "limOut"),
    ] {
        let (status, compared, worst, _, time) = figures(case, output);
        assert_eq!(
            (status, worst, time),
            (OutputStatus::Pass, None, 0.0),
            "{case} {output}"
        );
        assert!(compared > 0, "{case} {output}");
    }
}

#[test]
fn half_a_tolerance_past_the_corner_is_outside_for_that_half() {
    let (status, _, worst, first, time) = figures("step_beyond_tolerance", "TSet");
    assert_eq!(status, OutputStatus::Fail);
    assert_eq!(worst, Some((11.0, OUTSIDE_BY)));
    assert_eq!(first, Some(11.0));
    assert_eq!(time, 0.5);
}

#[test]
fn equal_times_in_a_trend_collapse_to_the_last_row() {
    // Reading 1. The trend logs the switch at 15 s as `true` then `false`: one instant, held at
    // the last row, which matches the engine.
    let run = run_case("boolean_switch_on_a_frame");
    assert_eq!(curves_named(&run, "enOut").errors.x(), [0.0, 15.0, 30.0]);
    // Swapped, the last row is `true` and the trend is wrong from 15 s on.
    let swapped = b"time,Setpoint,Enabled,Staged,Limited\n0,21,true,1,50\n15,21,false,1,50\n15,21,true,1,50\n";
    let spec = loaded(&case_spec_text("boolean_switch_on_a_frame", &[]));
    let inputs = case_inputs("boolean_switch_on_a_frame", &[("outputs.csv", swapped)]);
    let run = commission(&spec, &inputs, &CommissionOptions::default()).expect("runs");
    let enable = report(&run, "enOut");
    assert_eq!(enable.status, OutputStatus::Fail);
    assert_eq!(enable.first_exceedance_t, Some(15.0));
    assert_eq!(enable.exceedance_time_s, 15.0);
}

#[test]
fn one_sample_before_the_window_is_held_over_it() {
    // Reading 2. The trend's one sample is at 0 s and the window starts at 5 s.
    let run = run_case("one_sample_before_start");
    for output in ["TSet", "enOut", "limOut", "nOut"] {
        assert_eq!(report(&run, output).status, OutputStatus::Pass, "{output}");
    }
    assert_eq!(
        curves_named(&run, "nOut").errors.x(),
        [5.0, 10.0, 20.0, 30.0]
    );
    // With no sample at or before the start, there is nothing to hold: no data.
    let run = run_case("no_sample_at_start");
    assert_eq!(body_of(&run.receipt).status, OutputStatus::NoData);
    assert_eq!(report(&run, "TSet").compared, 0);
}

#[test]
fn a_trend_that_stops_logging_is_held_to_the_stop() {
    // Reading 3. The trend's last sample is at 10 s; the engine's setpoint steps at 20 s. The
    // trend is held, not reported missing: it fails from the band's corner at 21 s to the stop.
    let (status, compared, worst, first, time) = figures("trend_ends_before_stop", "TSet");
    assert_eq!(status, OutputStatus::Fail);
    assert_eq!(worst, Some((21.0, OUTSIDE_BY)));
    assert_eq!(first, Some(21.0));
    assert_eq!(time, 9.0);
    let run = run_case("trend_ends_before_stop");
    assert_eq!(report(&run, "TSet").uncovered, 0);
    assert!(compared > 0);
}

#[test]
fn mask_and_uncovered_edges_are_instants_of_both_series_and_zeroed_there() {
    // Reading 4. The fan is off from 5 s to 15 s, and the limit's hold runs out at the frame at
    // 10 s, uncovered to the next executed frame at 20 s. Each edge is an instant, and the
    // reference steps to zero at the mask's edge.
    let mut options = CommissionOptions::default();
    options.bounds = true;
    let run = run_case_with("masked_and_uncovered", &options);
    let limited = curves_named(&run, "limOut");
    let instants = limited.errors.x();
    for edge in [5.0, 10.0, 15.0, 20.0] {
        assert!(instants.contains(&edge), "{edge} in {instants:?}");
    }
    let reference = &limited.bounds.as_ref().expect("bounds kept").reference;
    let at_five: Vec<f64> = reference
        .points()
        .filter(|(x, _)| *x == 5.0)
        .map(|(_, y)| y)
        .collect();
    assert_eq!(
        at_five,
        [0.5, 0.0],
        "the value carried to the edge, then zero"
    );
    let output = report(&run, "limOut");
    assert_eq!((output.masked, output.uncovered), (4, 2));
    assert_eq!(output.masked_intervals.len(), 1);
    assert_eq!(
        (
            output.masked_intervals[0].from,
            output.masked_intervals[0].to
        ),
        (5.0, 12.0)
    );
}

#[test]
fn a_vertical_segment_counts_no_time() {
    // Reading 5. The trend's step at 13 s is a carry point and a new value at one instant; the
    // time outside is the stretch from the band's corner to it, 2 s, and nothing more.
    let run = run_case("late_step");
    let set = curves_named(&run, "TSet");
    assert_eq!(set.errors.x(), [0.0, 9.0, 10.0, 11.0, 13.0, 20.0, 30.0]);
    assert_eq!(report(&run, "TSet").exceedance_time_s, 2.0);
    // Both series step at one instant: no time outside at all.
    assert_eq!(figures("exact_steps", "TSet").4, 0.0);
}

#[test]
fn the_trend_as_reference_builds_the_band_around_the_trend() {
    // The band now steps at 13 s and the engine, the test, is above it from 10 s to its corner
    // at 12 s.
    let (status, _, worst, first, time) = figures("trend_as_reference", "TSet");
    assert_eq!(status, OutputStatus::Fail);
    assert_eq!(worst, Some((10.0, OUTSIDE_BY)));
    assert_eq!(first, Some(10.0));
    assert_eq!(time, 2.0);
}

#[test]
fn the_aligned_grid_envelope_takes_the_same_rule() {
    let (status, _, worst, _, time) = figures("aligned_grid_late_step", "TSet");
    assert_eq!(status, OutputStatus::Fail);
    assert_eq!(worst, Some((11.0, OUTSIDE_BY)));
    assert_eq!(time, 2.0);
}
