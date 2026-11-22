//! Tolerance windows run against a subject (vectors note, "Tolerance windows"; decisions 38 to
//! 48): each tolerance kind at its bound under both envelopes, a one-tick window, `of` by
//! zero-order hold, out-of-range outputs, the self-enclosure refusal and its remedies, the point
//! caps, a refused frame cutting a window, a hand-derived exceedance, and that no output ends a
//! run in an error. The mask (`when`) is `tests/tolerance_masks.rs`.

mod support;

use oce_verify::{
    CompareLimits, Failure, ReceiptValue, ScenarioReceipt, ScenarioStatus, VerifyError,
    VerifyLimits, VerifyOptions,
};

use support::{SIGNALS, body, only_scenario, receipt, refused, run, run_with, set_on, subject};

/// The `signals` subject's inputs with `ahu.T` (passed through to `ahu.y`) held as `ahu_t`, a
/// JSON constant or step list.
fn inputs(ahu_t: &str) -> String {
    format!(r#""ahu.T": {ahu_t}, "zone.T": 2.0, "n": 3, "b": true, "lim": 5.0"#)
}

/// A set, on the 60 s clock to 1,800 s with `envelope` (a JSON member, or empty), of one
/// scenario with `ahu.T` held as `ahu_t` and one tolerance window on `ahu.y` from `from_s` to
/// `to_s`.
fn ahu_window(envelope: &str, ahu_t: &str, span: (u32, u32), within: &str, of: &str) -> String {
    let (from_s, to_s) = span;
    let set = set_on(
        "60",
        "1800",
        &support::scenario(
            &inputs(ahu_t),
            &format!(
                r#"{{"output": "ahu.y", "from_s": {from_s}, "to_s": {to_s}, "within": {within}, "of": {of}}}"#
            ),
        ),
    );
    if envelope.is_empty() {
        set
    } else {
        set.replacen('{', &format!("{{{envelope}, "), 1)
    }
}

const FUNNEL: &str = "";
const ALIGNED: &str = r#""envelope": "aligned-grid""#;

fn status(text: &str) -> ScenarioStatus {
    only_scenario(&receipt(text, &subject("signals"))).status
}

fn scenario_of(text: &str) -> ScenarioReceipt {
    only_scenario(&receipt(text, &subject("signals"))).clone()
}

/// The `tolerance-exceeded` records of a scenario: (t, expected, actual, exceedance).
fn exceeded(scenario: &ScenarioReceipt) -> Vec<(f64, f64, f64, f64)> {
    scenario
        .failures
        .iter()
        .map(|failure| match failure {
            Failure::ToleranceExceeded {
                t,
                expected: ReceiptValue::Real(expected),
                actual: ReceiptValue::Real(actual),
                exceedance,
                ..
            } => (*t, *expected, *actual, *exceedance),
            other => panic!("a tolerance-exceeded record, found {other:?}"),
        })
        .collect()
}

#[test]
fn each_tolerance_kind_alone_passes_at_its_band_and_fails_below_it() {
    let offset = r#"[{"t": 0, "value": 0.4}, {"t": 600, "value": 10.4}]"#;
    let stepped = r#"[{"t": 0, "value": 0}, {"t": 600, "value": 10}]"#;
    let late = r#"[{"t": 0, "value": 0}, {"t": 660, "value": 10}]"#;
    // (ahu.T, of, the passing within, the failing within)
    let cases = [
        // y: an output 0.4 above its expectation.
        ("10.4", "10", r#"{"atoly": 0.5}"#, r#"{"atoly": 0.25}"#),
        ("10.4", "10", r#"{"ltoly": 0.05}"#, r#"{"ltoly": 0.02}"#),
        (offset, stepped, r#"{"rtoly": 0.05}"#, r#"{"rtoly": 0.02}"#),
        // x: an output one tick (60 s) late on a 1,200 s window.
        (late, stepped, r#"{"atolx": 60}"#, r#"{"atolx": 30}"#),
        (late, stepped, r#"{"rtolx": 0.05}"#, r#"{"rtolx": 0.025}"#),
        // The last tick before the step is 540 s, so its rectangle reaches 600 s at 1 / 9.
        (late, stepped, r#"{"ltolx": 0.12}"#, r#"{"ltolx": 0.1}"#),
    ];
    for envelope in [FUNNEL, ALIGNED] {
        for (ahu_t, of, passing, failing) in cases {
            let pass = ahu_window(envelope, ahu_t, (0, 1200), passing, of);
            assert_eq!(status(&pass), ScenarioStatus::Pass, "{envelope} {passing}");
            let fail = ahu_window(envelope, ahu_t, (0, 1200), failing, of);
            let scenario = scenario_of(&fail);
            assert_eq!(
                scenario.status,
                ScenarioStatus::Fail,
                "{envelope} {failing}"
            );
            assert!(
                exceeded(&scenario).iter().all(|record| record.3 > 0.0),
                "{envelope} {failing}"
            );
        }
    }
}

#[test]
fn an_output_equal_to_its_expectation_passes_with_any_tolerance() {
    // Self-enclosure means a window that is judged at all passes its own expectation.
    let of = r#"[{"t": 0, "value": 3}, {"t": 600, "value": -7.5}, {"t": 1200, "value": 1e3}]"#;
    let ahu_t = of;
    for envelope in [FUNNEL, ALIGNED] {
        for within in [
            r#"{"atoly": 0}"#,
            r#"{"atoly": 1e-9}"#,
            r#"{"atolx": 60, "atoly": 0.001}"#,
            r#"{"rtolx": 0.5, "rtoly": 0.5}"#,
            r#"{"ltolx": 1, "ltoly": 1}"#,
            r#"{"atolx": 1e4, "atoly": 1e4}"#,
        ] {
            let text = ahu_window(envelope, ahu_t, (0, 1800), within, of);
            assert_eq!(status(&text), ScenarioStatus::Pass, "{envelope} {within}");
        }
    }
}

#[test]
fn a_one_tick_window_is_two_one_point_series() {
    for envelope in [FUNNEL, ALIGNED] {
        let pass = ahu_window(envelope, "10.4", (600, 600), r#"{"atoly": 0.5}"#, "10");
        let scenario = scenario_of(&pass);
        assert_eq!(scenario.status, ScenarioStatus::Pass);
        assert_eq!(scenario.instants.checked, 1);
        // A window between two ticks' times that holds one tick.
        let pass = ahu_window(envelope, "10.4", (590, 610), r#"{"atolx": 1e6}"#, "10.4");
        assert_eq!(scenario_of(&pass).instants.checked, 1);
        let fail = ahu_window(envelope, "10.4", (600, 600), r#"{"atoly": 0.25}"#, "10");
        let scenario = scenario_of(&fail);
        assert_eq!(scenario.instants.failed, 1);
        let records = exceeded(&scenario);
        assert_eq!(
            (records[0].0, records[0].1, records[0].2),
            (600.0, 10.0, 10.4)
        );
    }
}

#[test]
fn of_holds_each_step_from_the_first_tick_at_or_after_it() {
    // A step between two ticks (630 s) takes effect at 660 s, as an input's step does.
    let of = r#"[{"t": 0, "value": 1}, {"t": 630, "value": 2}]"#;
    for envelope in [FUNNEL, ALIGNED] {
        let on_time = ahu_window(
            envelope,
            r#"[{"t": 0, "value": 1}, {"t": 660, "value": 2}]"#,
            (0, 1800),
            r#"{"atoly": 0.001}"#,
            of,
        );
        assert_eq!(status(&on_time), ScenarioStatus::Pass, "{envelope}");
        // An output that steps at 600 s instead fails the tick at 600 s, where `of` is still 1.
        let early = ahu_window(
            envelope,
            r#"[{"t": 0, "value": 1}, {"t": 600, "value": 2}]"#,
            (0, 1800),
            r#"{"atoly": 0.001}"#,
            of,
        );
        let records = exceeded(&scenario_of(&early));
        assert_eq!(records.len(), 1, "{envelope}");
        assert_eq!(
            (records[0].0, records[0].1, records[0].2),
            (600.0, 1.0, 2.0)
        );
        // aligned-grid's band at 600 s is exactly `of` there plus atoly; funnel interpolates its
        // bound across the step's corners in normalised `x`, which rounds by a few 1e-12.
        let formula = 2.0 - (1.0 + 0.001);
        if envelope == ALIGNED {
            assert_eq!(records[0].3, formula);
        } else {
            assert!((records[0].3 - formula).abs() < 1e-11, "{}", records[0].3);
        }
        // A step on a tick takes effect at that tick.
        let on_tick = ahu_window(
            envelope,
            r#"[{"t": 0, "value": 1}, {"t": 600, "value": 2}]"#,
            (0, 1800),
            r#"{"atoly": 0.001}"#,
            r#"[{"t": 0, "value": 1}, {"t": 600, "value": 2}]"#,
        );
        assert_eq!(status(&on_tick), ScenarioStatus::Pass, "{envelope}");
    }
}

#[test]
fn the_exceedance_is_the_distance_outside_the_band_by_the_notes_formula() {
    // `max(0, y - y_up) - min(0, y - y_low)` against a constant 10 with atoly 0.5: the band is
    // [9.5, 10.5] under both envelopes, so 10.75 is 0.25 above and 9.0 is 0.5 below, exactly.
    for envelope in [FUNNEL, ALIGNED] {
        for (output, expected) in [("10.75", 0.25), ("9.0", 0.5), ("-2.5", 12.0)] {
            let text = ahu_window(envelope, output, (0, 120), r#"{"atoly": 0.5}"#, "10");
            let records = exceeded(&scenario_of(&text));
            assert_eq!(records.len(), 3, "{envelope} {output}");
            for record in records {
                assert_eq!(record.3, expected, "{envelope} {output}");
            }
        }
    }
    // Exactly zero passes and anything above fails: the band's edge 10.5 is inside, and the
    // next binary64 above it is outside by one ulp.
    let edge = 10.5_f64;
    let past = f64::from_bits(edge.to_bits() + 1);
    for envelope in [FUNNEL, ALIGNED] {
        let on = ahu_window(
            envelope,
            &format!("{edge:e}"),
            (0, 120),
            r#"{"atoly": 0.5}"#,
            "10",
        );
        assert_eq!(status(&on), ScenarioStatus::Pass, "{envelope}");
        let off = ahu_window(
            envelope,
            &format!("{past:e}"),
            (0, 120),
            r#"{"atoly": 0.5}"#,
            "10",
        );
        let records = exceeded(&scenario_of(&off));
        assert_eq!(records.len(), 3, "{envelope}");
        assert!(
            records.iter().all(|record| record.3 == past - edge),
            "{envelope}"
        );
    }
    // A sloped band under aligned-grid, by hand: `of` 0, then 10 from 60 s, on ticks 0, 60 and
    // 120 s with atolx 60. The rectangles are x in [-60, 60] at 0, [0, 120] at 10 and [60, 180]
    // at 10; the band is the extremes of those covering each tick: [0, 10] at 0 s and at 60 s,
    // [10, 10] at 120 s. Outputs 10, 0 and 5 are inside, inside, and 5 below.
    let text = ahu_window(
        ALIGNED,
        r#"[{"t": 0, "value": 10}, {"t": 60, "value": 0}, {"t": 120, "value": 5}]"#,
        (0, 120),
        r#"{"atolx": 60}"#,
        r#"[{"t": 0, "value": 0}, {"t": 60, "value": 10}]"#,
    );
    let records = exceeded(&scenario_of(&text));
    assert_eq!(records.len(), 1);
    assert_eq!(
        (records[0].0, records[0].1, records[0].3),
        (120.0, 10.0, 5.0)
    );
}

#[test]
fn an_out_of_range_output_fails_its_instant_and_every_other_tick_is_judged() {
    // `ratio` is zone.T / ahu.T: 0 / 0 is a NaN, 1 / 0 an infinity.
    for (zone, ahu, actual_bits) in [
        ("0.0", "0.0", f64::NAN.to_bits()),
        ("1.0", "0.0", f64::INFINITY.to_bits()),
        ("-1.0", "0.0", f64::NEG_INFINITY.to_bits()),
    ] {
        for envelope in [FUNNEL, ALIGNED] {
            let set = set_on(
                "60",
                "1800",
                &support::scenario(
                    &format!(
                        r#""ahu.T": [{{"t": 0, "value": 2.0}}, {{"t": 600, "value": {ahu}}}, {{"t": 660, "value": 2.0}}],
                           "zone.T": [{{"t": 0, "value": 1.0}}, {{"t": 600, "value": {zone}}}, {{"t": 660, "value": 1.0}}],
                           "n": 3, "b": true, "lim": 5.0"#
                    ),
                    r#"{"output": "ratio", "from_s": 0, "to_s": 1800, "within": {"atoly": 0.01, "atolx": 60}, "of": 0.5}"#,
                ),
            )
            .replacen('{', &format!("{{{envelope}{}", if envelope.is_empty() { "" } else { ", " }), 1);
            let scenario = scenario_of(&set);
            assert_eq!(scenario.instants.checked, 31);
            assert_eq!(scenario.instants.failed, 1, "only the out-of-range tick");
            let Failure::ToleranceExceeded {
                t,
                actual: ReceiptValue::Real(actual),
                exceedance,
                ..
            } = &scenario.failures[0]
            else {
                panic!("a tolerance-exceeded record");
            };
            assert_eq!(*t, 600.0);
            assert_eq!(actual.to_bits(), actual_bits);
            assert_eq!(*exceedance, f64::INFINITY);
        }
    }
}

#[test]
fn two_to_the_thousand_is_judged_and_the_next_binary64_is_out_of_range() {
    let edge = 2.0_f64.powi(1000);
    let above = f64::from_bits(edge.to_bits() + 1);
    for envelope in [FUNNEL, ALIGNED] {
        for (value, finite) in [(edge, true), (-edge, true), (above, false), (-above, false)] {
            let ahu_t = format!(
                r#"[{{"t": 0, "value": 1.0}}, {{"t": 600, "value": {value:e}}}, {{"t": 660, "value": 1.0}}]"#
            );
            let text = ahu_window(envelope, &ahu_t, (0, 1800), r#"{"atoly": 0.5}"#, "1.0");
            let records = exceeded(&scenario_of(&text));
            assert_eq!(records.len(), 1, "{envelope} {value:e}");
            assert_eq!(records[0].2.to_bits(), value.to_bits());
            if finite {
                // 2^1000 - 1.5 and 0.5 + 2^1000 both round to 2^1000.
                assert_eq!(records[0].3, edge, "{envelope} {value:e}");
            } else {
                assert_eq!(records[0].3, f64::INFINITY);
            }
        }
        // The expectation itself may be 2^1000: its band stays inside the range and an output
        // equal to it passes.
        let text = ahu_window(
            envelope,
            &format!("{edge:e}"),
            (0, 1800),
            r#"{"atoly": 1}"#,
            &format!("{edge:e}"),
        );
        assert_eq!(status(&text), ScenarioStatus::Pass, "{envelope}");
    }
}

#[test]
fn no_output_value_ends_a_run_in_an_error() {
    // Every output is judged or fails its instant; none makes the comparison refuse. The values
    // reach the output through ahu.T (finite) or ratio (any binary64 a division gives).
    let mut sequence = support::Sequence(0x0068_4684);
    let specials = [
        0.0,
        -0.0,
        5e-324,
        f64::MAX,
        -f64::MAX,
        2.0_f64.powi(1000),
        2.0_f64.powi(1001),
        1e308,
        -1e300,
    ];
    for round in 0..48 {
        let values: Vec<f64> = (0..31)
            .map(|tick| {
                if (round + tick) % 5 == 0 {
                    specials[sequence.below(specials.len())]
                } else {
                    let bits = sequence.next();
                    let value = f64::from_bits(bits);
                    if value.is_finite() { value } else { 1.0 }
                }
            })
            .collect();
        let steps: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(tick, value)| format!(r#"{{"t": {}, "value": {value:e}}}"#, tick * 60))
            .collect();
        let envelope = if round % 2 == 0 { FUNNEL } else { ALIGNED };
        let within = [
            r#"{"atoly": 0.5}"#,
            r#"{"atolx": 60, "atoly": 1}"#,
            r#"{"rtoly": 0.1}"#,
        ][round % 3];
        let text = ahu_window(
            envelope,
            &format!("[{}]", steps.join(", ")),
            (0, 1800),
            within,
            "1.0",
        );
        let outcome = run(&text, &subject("signals"));
        assert!(
            outcome.is_ok(),
            "round {round}: {:?}",
            outcome.err().map(|e| e.to_string())
        );
    }
    // Divisions that overflow or give NaN, through `ratio`.
    for (zone, ahu) in [
        ("1e308", "1e-308"),
        ("0.0", "0.0"),
        ("-1e300", "1e-10"),
        ("5.0", "-0.0"),
    ] {
        let set = set_on(
            "60",
            "1800",
            &support::scenario(
                &format!(r#""ahu.T": {ahu}, "zone.T": {zone}, "n": 3, "b": true, "lim": 5.0"#),
                r#"{"output": "ratio", "from_s": 0, "to_s": 1800, "within": {"rtolx": 0.1, "atoly": 2}, "of": [{"t": 0, "value": 1}, {"t": 900, "value": -1e300}]}"#,
            ),
        );
        assert!(run(&set, &subject("signals")).is_ok(), "{zone} / {ahu}");
    }
}

/// The rounding case of decision 41 on `ahu.y`: `of` 2e6 then 0 at 60 s over ticks 0 to 120 s.
fn rounding(envelope: &str, within: &str, ahu_t: &str) -> String {
    ahu_window(
        envelope,
        ahu_t,
        (0, 120),
        within,
        r#"[{"t": 0, "value": 2e6}, {"t": 60, "value": 0}]"#,
    )
}

fn compare_refusal(text: &str) -> VerifyError {
    refused(text, &subject("signals"))
}

#[test]
fn the_rounding_case_is_refused_and_each_remedy_admits_it() {
    let error = compare_refusal(&rounding(FUNNEL, r#"{"atolx": 60}"#, "0.0"));
    assert_eq!(error.code(), "compare-refused");
    assert_eq!(error.cause(), Some("reference-outside-envelope"));
    assert_eq!(error.subject(), "scenarios[0].expect[0]");
    assert_eq!(error.path(), Some(&format!("{SIGNALS}ahu.y")[..]));
    // The refusal reads the expectation, never the output: any output gives the same one.
    for ahu_t in [
        "2e6",
        "-1.0",
        r#"[{"t": 0, "value": 2e6}, {"t": 60, "value": 0}]"#,
    ] {
        let again = compare_refusal(&rounding(FUNNEL, r#"{"atolx": 60}"#, ahu_t));
        assert_eq!((again.code(), again.cause()), (error.code(), error.cause()));
    }
    // The three remedies the note names, each with an output equal to the expectation.
    let equal = r#"[{"t": 0, "value": 2e6}, {"t": 60, "value": 0}]"#;
    for (envelope, within) in [
        (FUNNEL, r#"{"atolx": 60, "atoly": 2}"#),
        (FUNNEL, r#"{"atolx": 60, "rtoly": 1e-6}"#),
        (FUNNEL, r#"{"atolx": 59}"#),
        (FUNNEL, r#"{"atolx": 61}"#),
        (ALIGNED, r#"{"atolx": 60}"#),
    ] {
        let text = rounding(envelope, within, equal);
        assert_eq!(status(&text), ScenarioStatus::Pass, "{envelope} {within}");
    }
}

#[test]
fn a_band_past_two_to_the_thousand_is_refused_whatever_the_output() {
    // An expectation of 2^1000 with atoly 2^1000 has a top bound of 2^1001: finite, but past
    // the range in which every error is finite. Near the largest binary64 the port's own
    // arithmetic overflows. Both are `non-finite-envelope`, for any output.
    let edge = 2.0_f64.powi(1000);
    for (of, within) in [
        (format!("{edge:e}"), format!(r#"{{"atoly": {edge:e}}}"#)),
        (format!("{:e}", -edge), r#"{"ltoly": 1}"#.to_owned()),
        ("1e308".to_owned(), r#"{"atoly": 1e308}"#.to_owned()),
    ] {
        for envelope in [FUNNEL, ALIGNED] {
            for ahu_t in ["0.0", "1e300"] {
                let text = ahu_window(envelope, ahu_t, (0, 120), &within, &of);
                let error = compare_refusal(&text);
                assert_eq!(
                    (error.code(), error.cause()),
                    ("compare-refused", Some("non-finite-envelope")),
                    "{envelope} {of} {within}"
                );
            }
        }
    }
    // Just inside: an expectation of 2^999 with atoly 2^999 tops out at 2^1000 exactly.
    let half = 2.0_f64.powi(999);
    let text = ahu_window(
        FUNNEL,
        &format!("{half:e}"),
        (0, 120),
        &format!(r#"{{"atoly": {half:e}}}"#),
        &format!("{half:e}"),
    );
    assert_eq!(status(&text), ScenarioStatus::Pass);
}

#[test]
fn a_comparison_past_its_work_bound_is_refused_with_the_ports_cause() {
    // A sawtooth expectation under a wide x tolerance makes loops to remove; a work bound of
    // one unit cannot remove them.
    let steps: Vec<String> = (0..31)
        .map(|tick| format!(r#"{{"t": {}, "value": {}}}"#, tick * 60, tick % 2))
        .collect();
    let of = format!("[{}]", steps.join(", "));
    let text = ahu_window(
        FUNNEL,
        &of,
        (0, 1800),
        r#"{"atolx": 300, "atoly": 0.1}"#,
        &of,
    );
    let options = |work| VerifyOptions {
        limits: VerifyLimits {
            compare: CompareLimits {
                max_loop_removal_work: work,
                ..CompareLimits::MAX
            },
            ..VerifyLimits::MAX
        },
        ..VerifyOptions::default()
    };
    let error = run_with(&text, &subject("signals"), &options(1)).expect_err("refused");
    assert_eq!(
        (error.code(), error.cause()),
        ("compare-refused", Some("work-limit-exceeded"))
    );
    assert!(
        run_with(
            &text,
            &subject("signals"),
            &options(CompareLimits::MAX.max_loop_removal_work)
        )
        .is_ok()
    );
}

#[test]
fn each_point_cap_admits_a_window_at_it_and_refuses_one_past() {
    for (envelope, cap) in [
        (FUNNEL, "max_funnel_points"),
        (ALIGNED, "max_aligned_grid_points"),
    ] {
        let text = ahu_window(envelope, "1.0", (0, 1800), r#"{"atoly": 0.5}"#, "1.0");
        let at = |points: usize| {
            let mut compare = CompareLimits::MAX;
            if cap == "max_funnel_points" {
                compare.max_funnel_points = points;
            } else {
                compare.max_aligned_grid_points = points;
            }
            run_with(
                &text,
                &subject("signals"),
                &VerifyOptions {
                    limits: VerifyLimits {
                        compare,
                        ..VerifyLimits::MAX
                    },
                    ..VerifyOptions::default()
                },
            )
        };
        assert!(at(31).is_ok(), "{cap}");
        let error = at(30).expect_err("one past");
        assert_eq!(
            (error.code(), error.subject()),
            ("too-many-points", "scenarios[0].expect[0]"),
            "{cap}"
        );
        // The other envelope's cap does not apply.
        let mut other = CompareLimits::MAX;
        if cap == "max_funnel_points" {
            other.max_aligned_grid_points = 0;
        } else {
            other.max_funnel_points = 0;
        }
        let options = VerifyOptions {
            limits: VerifyLimits {
                compare: other,
                ..VerifyLimits::MAX
            },
            ..VerifyOptions::default()
        };
        assert!(
            run_with(&text, &subject("signals"), &options).is_ok(),
            "{cap}"
        );
    }
    // The documented caps: a 1,048,577-tick window is refused under funnel, and 32,769 ticks
    // under aligned-grid, before any frame runs.
    let wide = set_on(
        "1",
        "1048576",
        &support::scenario(
            &inputs("1.0"),
            r#"{"output": "ahu.y", "from_s": 0, "to_s": 1048576, "within": {"atoly": 1}, "of": 1}"#,
        ),
    );
    assert_eq!(
        refused(&wide, &subject("signals")).code(),
        "too-many-points"
    );
    let grid = set_on(
        "1",
        "1800",
        &support::scenario(
            &inputs("1.0"),
            r#"{"output": "ahu.y", "from_s": 0, "to_s": 1800, "within": {"atoly": 1}, "of": 1}"#,
        ),
    )
    .replacen('{', &format!("{{{ALIGNED}, "), 1)
    .replace("\"horizon_s\": 1800", "\"horizon_s\": 32768")
    .replace("\"to_s\": 1800", "\"to_s\": 32768");
    assert_eq!(
        refused(&grid, &subject("signals")).code(),
        "too-many-points"
    );
}

#[test]
fn a_tolerance_window_on_a_discrete_output_is_refused_before_any_frame() {
    // `lim` steps out of its [0, 10] bound at 600 s, so a run would refuse a frame there; the
    // bind refusal comes first.
    for output in ["nOut", "bOut"] {
        let text = set_on(
            "60",
            "1800",
            &support::scenario(
                r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": true, "lim": [{"t": 0, "value": 5.0}, {"t": 600, "value": 20.0}]"#,
                &format!(
                    r#"{{"output": "{output}", "from_s": 0, "to_s": 1800, "within": {{"atoly": 0.5}}, "of": 1}}"#
                ),
            ),
        );
        let error = refused(&text, &subject("signals"));
        assert_eq!(error.code(), "tolerance-on-discrete-output", "{output}");
        assert_eq!(error.path(), Some(&format!("{SIGNALS}{output}")[..]));
    }
}

#[test]
fn a_refused_frame_cuts_a_window_to_its_ticks_before_the_refusal() {
    // `lim` (bounded to [0, 10]) steps to 20 at 600 s: the frame at tick 10 is refused.
    let text = set_on(
        "60",
        "1800",
        &support::scenario(
            r#""ahu.T": 1.0, "zone.T": 2.0, "n": 3, "b": true, "lim": [{"t": 0, "value": 5.0}, {"t": 600, "value": 20.0}]"#,
            r#"{"output": "limOut", "from_s": 0, "to_s": 1800, "within": {"atoly": 0.1, "atolx": 60}, "of": 5},
               {"output": "limOut", "from_s": 600, "to_s": 1800, "within": {"atoly": 0.1}, "of": 5}"#,
        ),
    );
    let scenario = scenario_of(&text);
    assert_eq!(scenario.status, ScenarioStatus::Fail);
    assert_eq!(
        scenario.instants.checked, 10,
        "ticks 0 to 9 of the first window"
    );
    assert_eq!(scenario.instants.failed, 0);
    assert_eq!(scenario.failures.len(), 1);
    assert_eq!(scenario.failures[0].code(), "frame-refused");
    assert_eq!(scenario.failures[0].t(), 600.0);
}

#[test]
fn a_set_with_an_envelope_and_no_tolerance_window_says_exact() {
    let text = set_on(
        "60",
        "1800",
        &support::scenario(
            &inputs("1.0"),
            r#"{"output": "ahu.y", "from_s": 0, "to_s": 60, "equals": 1.0}"#,
        ),
    )
    .replacen('{', &format!("{{{ALIGNED}, "), 1);
    let receipt = receipt(&text, &subject("signals"));
    assert_eq!(receipt.header.comparison.mode.as_str(), "exact");
    assert_eq!(receipt.header.comparison.envelope, None);
    assert_eq!(body(&receipt).funnel_reference, None);
    let json = receipt.to_canonical_json();
    assert!(json.contains("\"comparison\": {\n    \"mode\": \"exact\"\n  },"));
    assert!(!json.contains("funnel_reference"));
}

#[test]
fn an_exact_window_beside_a_tolerance_window_is_still_compared_by_bits() {
    let text = set_on(
        "60",
        "1800",
        &support::scenario(
            &inputs("1.0"),
            r#"{"output": "ahu.y", "from_s": 0, "to_s": 60, "within": {"atoly": 1}, "of": 1.5},
               {"output": "ahu.y", "from_s": 0, "to_s": 60, "equals": 1.0000000000000002}"#,
        ),
    );
    let receipt = receipt(&text, &subject("signals"));
    assert_eq!(receipt.header.comparison.mode.as_str(), "tolerance");
    let scenario = only_scenario(&receipt);
    assert_eq!(scenario.instants.failed, 2);
    assert!(
        scenario
            .failures
            .iter()
            .all(|f| f.code() == "value-mismatch")
    );
}
