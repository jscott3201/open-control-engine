//! The funnel on a fixed tick grid, as a vector set's clock gives one: is a series inside its
//! own band, and is the same series one tick late?
//!
//! `oce-conformance` keeps its wider grid envelope because a corner construction "can reject a
//! bit-identical trace when `atolx` is large relative to sample spacing". These tests measure
//! that for funnel's construction, with the series, step sizes and tolerances a vector file
//! would use. The scope of each statement matters, so it is spelled out:
//!
//! - **On a grid from `x = 0` with one sample per instant the construction does not leak:** a
//!   series is inside its own funnel for every `x` tolerance tried, from none to ten ticks,
//!   even with no `y` tolerance.
//! - **One tick is a knife edge:** a series shifted by exactly one tick is inside from an `x`
//!   tolerance of exactly one tick, and at that tolerance the bound's corner sits on the sample.
//! - **So rounding decides when the band has no height:** with no `y` tolerance the band's
//!   half-height is funnel's `1e-10` floor, and a step of a million or more rounds by more than
//!   that. An identical series then leaves its own band by a few parts in 10^16 of the step.
//!   Any `y` tolerance above the rounding removes it.
//!
//! Outside that scope funnel itself rejects an identical series, by whole units of `y` and
//! with a `y` tolerance, and the port follows it (each is a hostile case of the differential
//! fixture, so pyfunnel says the same):
//!
//! - **several samples at one instant with no `x` tolerance.** The rectangles are then the
//!   `1e-10` floor wide, which funnel's equality cannot tell from zero once `x` is a few units
//!   large. Three samples that do not rise or fall in order leak from an `x` of about 4; a step
//!   written as two samples leaks once `x` is in the millions. Any `x` tolerance removes both.
//! - **`x` more than about 1e10 times its step**, such as epoch seconds in steps under 0.17 s:
//!   funnel's equality, `1e-10` relative, then reads neighbouring instants as one.

mod support;

use oce_verify::{CompareLimits, FunnelResult, Series, Tolerances, funnel};

use support::Sequence;

const TICKS: usize = 40;

/// The grid `k × step_s` for `TICKS` ticks, by multiplication as a vector clock decides it.
fn grid(step_s: f64) -> Vec<f64> {
    (0..TICKS).map(|tick| tick as f64 * step_s).collect()
}

/// Series a vector file's inputs and outputs look like on the grid, each scaled by `scale`:
/// values held for several ticks, a 0/1 signal, a ramp that resets, a one-tick pulse train, a
/// sawtooth, a random walk and noise.
fn shapes(sequence: &mut Sequence, scale: f64) -> Vec<(&'static str, Vec<f64>)> {
    let mut held = Vec::new();
    let mut level = 0.0;
    for _ in 0..TICKS {
        if sequence.below(4) == 0 {
            level = sequence.below(5) as f64;
        }
        held.push(level);
    }
    let mut switch = Vec::new();
    let mut on = 0.0;
    for _ in 0..TICKS {
        if sequence.below(5) == 0 {
            on = 1.0 - on;
        }
        switch.push(on);
    }
    let mut walk = Vec::new();
    let mut position = 0.0;
    for _ in 0..TICKS {
        position += sequence.below(7) as f64 - 3.0;
        walk.push(position);
    }
    let noise: Vec<f64> = (0..TICKS)
        .map(|_| sequence.below(1000) as f64 / 100.0)
        .collect();
    let ramp: Vec<f64> = (0..TICKS)
        .map(|tick| ((tick / 6) % 2 * (tick % 6)) as f64)
        .collect();
    let pulse: Vec<f64> = (0..TICKS)
        .map(|tick| if tick % 9 == 4 { 1.0 } else { 0.0 })
        .collect();
    let sawtooth: Vec<f64> = (0..TICKS).map(|tick| (tick % 2) as f64).collect();
    [
        ("held", held),
        ("switch", switch),
        ("ramp", ramp),
        ("pulse", pulse),
        ("sawtooth", sawtooth),
        ("walk", walk),
        ("noise", noise),
    ]
    .into_iter()
    .map(|(name, values)| (name, values.iter().map(|value| value * scale).collect()))
    .collect()
}

fn compare(x: &[f64], reference: &[f64], test: &[f64], tolerances: Tolerances) -> FunnelResult {
    funnel(
        Series::new(x, reference),
        Series::new(x, test),
        &tolerances,
        &CompareLimits::default(),
    )
    .expect("a series on a tick grid compares")
}

#[test]
fn a_series_is_inside_its_own_funnel_for_every_x_tolerance_from_none_to_ten_ticks() {
    let mut compared = 0;
    for step_s in [1.0, 60.0, 0.1] {
        let x = grid(step_s);
        for ticks in [0.0, 0.25, 0.5, 0.999, 1.0, 1.001, 1.5, 2.0, 3.0, 5.0, 10.0] {
            for atoly in [0.0, 0.01] {
                let mut sequence = Sequence(655);
                for _ in 0..20 {
                    for (name, y) in shapes(&mut sequence, 1.0) {
                        let tolerances = Tolerances {
                            atolx: ticks * step_s,
                            atoly,
                            ..Tolerances::ZERO
                        };
                        let result = compare(&x, &y, &y, tolerances);
                        assert!(
                            result.within_envelope(),
                            "{name} on a {step_s} s grid, atolx {ticks} ticks, atoly {atoly}: \
                             {} at {:?}",
                            result.max_error,
                            result.max_error_x
                        );
                        compared += 1;
                    }
                }
            }
        }
    }
    assert_eq!(compared, 3 * 11 * 2 * 20 * 7);
}

#[test]
fn every_short_series_of_four_levels_is_inside_its_own_funnel_with_no_y_tolerance() {
    // Exhaustive: every series of three to six ticks over the levels 0 to 3, under an x
    // tolerance of one, one and a half and two ticks and nothing else.
    let mut compared = 0;
    for len in 3..=6_u32 {
        let x: Vec<f64> = (0..len).map(f64::from).collect();
        for code in 0..4_u32.pow(len) {
            let y: Vec<f64> = (0..len)
                .map(|tick| f64::from(code / 4_u32.pow(tick) % 4))
                .collect();
            for atolx in [1.0, 1.5, 2.0] {
                let tolerances = Tolerances {
                    atolx,
                    ..Tolerances::ZERO
                };
                assert!(
                    compare(&x, &y, &y, tolerances).within_envelope(),
                    "{y:?} under atolx {atolx}"
                );
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 3 * (64 + 256 + 1024 + 4096));
}

#[test]
fn a_series_one_tick_late_or_early_is_inside_from_an_x_tolerance_of_exactly_one_tick() {
    for step_s in [1.0, 60.0, 0.1] {
        let x = grid(step_s);
        let mut sequence = Sequence(655);
        let (mut shifted, mut outside_below) = (0, 0);
        for _ in 0..20 {
            for (name, y) in shapes(&mut sequence, 1.0) {
                if !["held", "switch", "ramp"].contains(&name) {
                    continue;
                }
                let mut late = vec![y[0]];
                late.extend_from_slice(&y[..TICKS - 1]);
                let mut early = y[1..].to_vec();
                early.push(y[TICKS - 1]);
                if late == y {
                    continue;
                }
                for test in [&late, &early] {
                    let at = |ticks: f64| {
                        let tolerances = Tolerances {
                            atolx: ticks * step_s,
                            ..Tolerances::ZERO
                        };
                        compare(&x, &y, test, tolerances).within_envelope()
                    };
                    // Exactly one tick is enough, with no y tolerance at all; so is more.
                    assert!(at(1.0), "{name} on a {step_s} s grid, shifted one tick");
                    assert!(at(1.5) && at(2.0), "{name} on a {step_s} s grid");
                    // A thousandth of a tick less is not.
                    if !at(0.999) {
                        outside_below += 1;
                    }
                    shifted += 1;
                }
            }
        }
        assert!(
            shifted >= 100,
            "{shifted} shifted series on a {step_s} s grid"
        );
        assert_eq!(outside_below, shifted, "on a {step_s} s grid");
    }
}

#[test]
fn with_no_y_tolerance_a_step_of_millions_leaves_its_own_band_by_rounding() {
    // Three ticks a minute apart, a step down of two million, an x tolerance of one tick. The
    // upper bound's corner after the step sits on the sample at 120 s, the segment before it
    // falls two million in a minute, and interpolating along it rounds by 2.3e-10: more than
    // the 1e-10 the band is high.
    let x = [0.0, 60.0, 120.0];
    let y = [2_000_000.0, 0.0, 0.0];
    let one_tick = Tolerances {
        atolx: 60.0,
        ..Tolerances::ZERO
    };
    let leaked = compare(&x, &y, &y, one_tick);
    assert!(!leaked.within_envelope());
    assert_eq!(leaked.max_error_x, Some(120.0));
    assert_eq!(leaked.errors.y()[..2], [0.0, 0.0]);
    assert!(leaked.max_error > 1e-10 && leaked.max_error < 1e-9);
    // Relative to the step it is rounding: about one part in 10^16.
    assert!(leaked.max_error / 2_000_000.0 < 2e-16);

    // A y tolerance of one part in a million of the step is far above it,
    let with_height = Tolerances {
        atoly: 2.0,
        ..one_tick
    };
    assert!(compare(&x, &y, &y, with_height).within_envelope());
    // and so is a step small enough for the floor: the same series a millionth as high.
    let small: Vec<f64> = y.iter().map(|value| value / 1_000_000.0).collect();
    assert!(compare(&x, &small, &small, one_tick).within_envelope());
    // An x tolerance that puts no corner on a sample avoids the knife edge as well.
    let between = Tolerances {
        atolx: 90.0,
        ..Tolerances::ZERO
    };
    assert!(compare(&x, &y, &y, between).within_envelope());
}

#[test]
fn a_y_tolerance_above_rounding_keeps_large_series_inside_their_own_funnel() {
    // The shapes of the first test at a million and a billion times the size, where a band of
    // the floor's height can leak: with a y tolerance of one part in a million none does.
    for scale in [1.0e6, 1.0e9] {
        for step_s in [1.0, 60.0, 0.1] {
            let x = grid(step_s);
            for ticks in [1.0, 2.0, 3.0] {
                let mut sequence = Sequence(655);
                for _ in 0..20 {
                    for (name, y) in shapes(&mut sequence, scale) {
                        let tolerances = Tolerances {
                            atolx: ticks * step_s,
                            atoly: scale * 1e-6,
                            ..Tolerances::ZERO
                        };
                        assert!(
                            compare(&x, &y, &y, tolerances).within_envelope(),
                            "{name} × {scale} on a {step_s} s grid, atolx {ticks} ticks"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn three_samples_at_one_instant_leave_their_own_band_when_there_is_no_x_tolerance() {
    let y = [0.0, 2.0, 0.0, 1.0, 0.0];
    let height = Tolerances {
        atoly: 0.1,
        ..Tolerances::ZERO
    };
    // A spike at x = 100: the series is reported 0.9 above its own band at the spike's top.
    let x = [0.0, 100.0, 100.0, 100.0, 200.0];
    let leaked = compare(&x, &y, &y, height);
    assert!(!leaked.within_envelope());
    assert_eq!(leaked.max_error_x, Some(100.0));
    assert_eq!(leaked.max_error, 2.0 - (1.0 + 0.1));
    assert_eq!(leaked.errors.y()[2..], [0.0, 0.0, 0.0]);

    // The same spike at x = 1 is inside: the floor is still wider than funnel's equality.
    assert!(compare(&[0.0, 1.0, 1.0, 1.0, 2.0], &y, &y, height).within_envelope());
    // And any x tolerance at all keeps it inside at x = 100.
    let a_little_width = Tolerances {
        atolx: 1e-6,
        ..height
    };
    assert!(compare(&x, &y, &y, a_little_width).within_envelope());
    // Samples at one instant that only rise are a step, and are inside.
    let rising = [0.0, 0.0, 1.0, 2.0, 2.0];
    assert!(compare(&x, &rising, &rising, height).within_envelope());
}

#[test]
fn a_step_written_as_two_samples_at_one_instant_leaks_once_x_is_in_the_millions() {
    let y = [0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
    let height = Tolerances {
        atoly: 0.1,
        ..Tolerances::ZERO
    };
    let pulse = |scale: f64| -> Vec<f64> {
        [0.0, 1.0, 1.0, 2.0, 2.0, 3.0]
            .iter()
            .map(|instant| instant * scale)
            .collect()
    };
    // A day, and three days: inside.
    for scale in [1.0, 86_400.0, 259_200.0] {
        assert!(
            compare(&pulse(scale), &y, &y, height).within_envelope(),
            "{scale}"
        );
    }
    // Steps at one and two million: the falling step's lower sample is 0.9 below the band.
    let x = pulse(1.0e6);
    let leaked = compare(&x, &y, &y, height);
    assert!(!leaked.within_envelope());
    assert_eq!(leaked.max_error_x, Some(2.0e6));
    assert_eq!(leaked.max_error, (1.0 - 0.1) - 0.0);
    // A millisecond of x tolerance is enough.
    let a_little_width = Tolerances {
        atolx: 1e-3,
        ..height
    };
    assert!(compare(&x, &y, &y, a_little_width).within_envelope());
}

#[test]
fn where_x_is_more_than_1e10_times_its_step_neighbouring_instants_merge() {
    let y = [0.0, 1.0, 3.0, 2.0, 2.0, 5.0, 4.0, 1.0, 1.0, 0.0, -2.0, -1.0];
    let epoch = |step_s: f64| -> Vec<f64> {
        (0..y.len())
            .map(|tick| 1.7e9 + tick as f64 * step_s)
            .collect()
    };
    let outside = |step_s: f64, ticks: f64| {
        let tolerances = Tolerances {
            atolx: ticks * step_s,
            atoly: 0.1,
            ..Tolerances::ZERO
        };
        let x = epoch(step_s);
        let result = compare(&x, &y, &y, tolerances);
        result
            .errors
            .y()
            .iter()
            .filter(|error| **error > 0.0)
            .count()
    };
    // Epoch seconds a second or a fifth of a second apart (a ratio of 1.7e9 and 8.5e9): inside
    // with no x tolerance and with one of a tick.
    for step_s in [1.0, 0.2] {
        assert_eq!(
            (outside(step_s, 0.0), outside(step_s, 1.0)),
            (0, 0),
            "{step_s}"
        );
    }
    // A tenth of a second apart (1.7e10): half of the series is outside its own band with no x
    // tolerance and some of it with one of half a tick; from one tick it is inside here.
    assert_eq!(outside(0.1, 0.0), 6);
    assert_eq!(outside(0.1, 0.5), 2);
    assert_eq!((outside(0.1, 1.0), outside(0.1, 1.5)), (0, 0));
    // A hundredth apart (1.7e11): outside with no x tolerance, and still at one and at three
    // ticks.
    assert_eq!(outside(0.01, 0.0), 6);
    assert_eq!((outside(0.01, 1.0), outside(0.01, 3.0)), (1, 7));
}
