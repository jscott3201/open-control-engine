//! The cadence rule against a brute-force search over tick products, the tick bound at its
//! boundary, and windows decided from tick values.

use super::*;
use crate::error::VectorErrorKind;

const MAX_TICKS: u64 = 16_777_216;

fn clock(step_s: f64, horizon_s: f64) -> Result<Clock, VectorErrorKind> {
    limited(step_s, horizon_s, MAX_TICKS)
}

fn limited(step_s: f64, horizon_s: f64, max_ticks: u64) -> Result<Clock, VectorErrorKind> {
    Clock::new(Number::Real(step_s), Number::Real(horizon_s), max_ticks)
        .map_err(|error| error.kind())
}

/// The reference the rule is checked against: every product, compared exactly.
#[expect(
    clippy::cast_precision_loss,
    reason = "test clocks stay far below 2^53 ticks"
)]
fn search(time_s: f64, step_s: f64, last: u64) -> Option<u64> {
    (0..=last).find(|tick| (*tick as f64) * step_s == time_s)
}

/// Steps with awkward binary expansions, a subnormal one and very large and very small ones.
const STEPS: &[f64] = &[
    0.1,
    0.3,
    1.0 / 3.0,
    0.7,
    1e-3,
    1.1,
    60.0,
    300.0,
    std::f64::consts::PI,
    1.000_000_000_000_000_2,
    5e-324,
    2.5e-310,
    1.5e300,
];

#[test]
fn a_horizon_the_quotient_would_floor_short_of_is_on_the_cadence() {
    // floor(4.3 / 0.1) is 42, yet 43 steps of 0.1 are exactly 4.3: the quotient is rounded and
    // the candidates are tested by multiplication.
    let quotient: f64 = 4.3 / 0.1;
    assert_eq!(quotient.floor().to_bits(), 42.0_f64.to_bits());
    assert_eq!((43.0_f64 * 0.1).to_bits(), 4.3_f64.to_bits());
    let clock = clock(0.1, 4.3).expect("on the cadence");
    assert_eq!(clock.last_tick(), 43);
    assert_eq!(clock.tick_count(), 44);
    assert_eq!(
        clock.tick_time(43).map(f64::to_bits),
        Some(4.3_f64.to_bits())
    );
    assert_eq!(clock.tick_time(44), None);
}

#[test]
fn a_horizon_one_ulp_off_the_cadence_is_refused() {
    for off in [4.3_f64.next_up(), 4.3_f64.next_down()] {
        assert_eq!(clock(0.1, off), Err(VectorErrorKind::HorizonOffCadence));
    }
    for off in [1800.0_f64.next_up(), 1800.0_f64.next_down(), 1830.0, 1799.0] {
        assert_eq!(clock(60.0, off), Err(VectorErrorKind::HorizonOffCadence));
    }
    assert_eq!(clock(60.0, 1800.0).map(|c| c.last_tick()), Ok(30));
}

#[test]
fn a_zero_horizon_is_one_tick_and_negative_zero_reads_as_zero() {
    for horizon in [0.0, -0.0] {
        let clock = clock(60.0, horizon).expect("one tick");
        assert_eq!((clock.last_tick(), clock.tick_count()), (0, 1));
        assert_eq!(clock.horizon_s().to_bits(), 0.0_f64.to_bits());
        assert_eq!(clock.tick_at(0.0), Some(0));
        assert_eq!(clock.tick_at(-0.0), Some(0));
        assert_eq!(clock.ticks_within(0.0, 0.0), Some((0, 0)));
    }
}

#[test]
fn a_step_that_is_not_positive_and_a_negative_horizon_are_invalid() {
    for step in [0.0, -0.0, -1.0, -5e-324] {
        assert_eq!(clock(step, 10.0), Err(VectorErrorKind::InvalidClock));
    }
    for horizon in [-1.0, -5e-324, -60.0] {
        assert_eq!(clock(60.0, horizon), Err(VectorErrorKind::InvalidClock));
    }
    // The smallest positive step is a clock.
    assert_eq!(clock(5e-324, 5e-324).map(|c| c.last_tick()), Ok(1));
    // Not finite cannot be written in a document; the constructor still refuses it.
    for bad in [f64::NAN, f64::INFINITY] {
        assert_eq!(clock(bad, 10.0), Err(VectorErrorKind::InvalidClock));
        assert_eq!(clock(1.0, bad), Err(VectorErrorKind::InvalidClock));
    }
    // The subjects name the field.
    let step = Clock::new(Number::Integer(0), Number::Integer(10), MAX_TICKS).unwrap_err();
    assert_eq!(step.subject(), "clock.step_s");
    let horizon = Clock::new(Number::Integer(1), Number::Integer(-1), MAX_TICKS).unwrap_err();
    assert_eq!(horizon.subject(), "clock.horizon_s");
}

#[test]
fn the_tick_bound_is_accepted_at_the_limit_and_refused_one_past() {
    // 16,777,216 ticks: N = 16,777,215.
    let at_limit = clock(1.0, 16_777_215.0).expect("at the bound");
    assert_eq!(at_limit.tick_count(), MAX_TICKS);
    assert_eq!(clock(1.0, 16_777_216.0), Err(VectorErrorKind::TooManyTicks));
    // The same with a step that is not a power of two.
    let step = 0.1_f64;
    let at_limit = clock(step, 16_777_215.0 * step).expect("at the bound");
    assert_eq!(at_limit.tick_count(), MAX_TICKS);
    assert_eq!(
        clock(step, 16_777_216.0 * step),
        Err(VectorErrorKind::TooManyTicks)
    );
}

#[test]
fn a_lowered_tick_bound_is_the_bound() {
    assert_eq!(limited(60.0, 1800.0, 31).map(|c| c.tick_count()), Ok(31));
    assert_eq!(
        limited(60.0, 1800.0, 30),
        Err(VectorErrorKind::TooManyTicks)
    );
    assert_eq!(limited(60.0, 0.0, 1).map(|c| c.tick_count()), Ok(1));
    assert_eq!(limited(60.0, 60.0, 1), Err(VectorErrorKind::TooManyTicks));
    // A bound of zero admits no clock at all, the one-tick clock included.
    assert_eq!(limited(60.0, 0.0, 0), Err(VectorErrorKind::TooManyTicks));
}

#[test]
fn a_quotient_past_the_bound_is_too_many_ticks_before_any_candidate_is_tested() {
    // An overflowing quotient, a huge one, and one that is past the bound and off the cadence:
    // all are the tick bound's refusal, never `horizon-off-cadence`.
    assert_eq!(clock(5e-324, 1e308), Err(VectorErrorKind::TooManyTicks));
    assert_eq!(clock(1e-9, 1e9), Err(VectorErrorKind::TooManyTicks));
    assert_eq!(clock(1.0, 1e10 + 0.5), Err(VectorErrorKind::TooManyTicks));
    assert_eq!(clock(60.0, 1e300), Err(VectorErrorKind::TooManyTicks));
    // Just inside the bound and off the cadence is the cadence's refusal.
    assert_eq!(
        clock(1.0, 16_777_214.5),
        Err(VectorErrorKind::HorizonOffCadence)
    );
}

#[test]
fn the_rule_finds_exactly_the_tick_a_search_over_every_product_finds() {
    let last = 600;
    let mut on_cadence = 0_u32;
    for step in STEPS {
        for tick in 0..=last {
            let time = tick_time(tick, *step);
            if !time.is_finite() {
                continue;
            }
            for probe in [time, time.next_up(), time.next_down()] {
                let expected = search(probe, *step, last);
                assert_eq!(
                    cadence_index(probe, *step, last),
                    expected,
                    "step {step:e}, tick {tick}, probe {probe:e}"
                );
                on_cadence += u32::from(expected.is_some());
            }
            assert_eq!(cadence_index(time, *step, last), Some(tick));
        }
    }
    // Every tick itself was found, so the comparison above was not vacuous.
    assert!(on_cadence >= 600 * 12, "{on_cadence}");
}

#[test]
#[expect(
    clippy::cast_precision_loss,
    reason = "tick indexes stay at or below 2^24, where the cast is exact"
)]
fn inside_the_tick_bound_the_rounded_quotient_is_already_the_tick() {
    // Why the two neighbours are a guard and not a second way in: for a tick inside the bound
    // the quotient is off by far less than a half, so rounding lands on the tick itself (and
    // flooring, which the rule never does, can land one short). Rounding and the neighbours are
    // each enough on their own here; the rule keeps both.
    let mut floored_short = 0_u32;
    for step in STEPS {
        let top = (MAX_TICKS - 2_000)..MAX_TICKS;
        for tick in (0..2_000).chain((0..MAX_TICKS).step_by(65_521)).chain(top) {
            let time = tick_time(tick, *step);
            if !time.is_finite() {
                continue;
            }
            let quotient: f64 = time / *step;
            assert_eq!(
                quotient.round().to_bits(),
                (tick as f64).to_bits(),
                "{step:e} {tick}"
            );
            floored_short += u32::from(quotient.floor() < tick as f64);
        }
    }
    assert!(floored_short > 0, "flooring is short of the tick somewhere");
}

#[test]
fn a_time_between_two_ticks_is_not_on_the_cadence() {
    for step in STEPS {
        for tick in [0_u64, 1, 2, 17, 599] {
            let (here, next) = (tick_time(tick, *step), tick_time(tick + 1, *step));
            if !next.is_finite() {
                continue;
            }
            let between = here + (next - here) / 2.0;
            if between > here && between < next {
                assert_eq!(cadence_index(between, *step, 600), None, "{step:e} {tick}");
            }
        }
    }
}

#[test]
fn tick_products_strictly_increase_up_to_the_tick_bound() {
    // "At most one candidate holds": inside the bound two ticks never share a product, here at
    // the bottom, across a stride of the range and at the top.
    for step in STEPS {
        let stride = (0..MAX_TICKS).step_by(65_521);
        let top = (MAX_TICKS - 2_000)..MAX_TICKS - 1;
        for tick in (0..2_000).chain(stride).chain(top) {
            let (here, next) = (tick_time(tick, *step), tick_time(tick + 1, *step));
            if next.is_finite() {
                assert!(here < next, "step {step:e}, tick {tick}");
            }
        }
    }
}

#[test]
fn the_rule_finds_every_tick_at_the_top_of_the_tick_bound() {
    let last = MAX_TICKS - 1;
    for step in STEPS {
        for tick in (last - 2_000)..=last {
            let time = tick_time(tick, *step);
            if time.is_finite() {
                assert_eq!(cadence_index(time, *step, last), Some(tick), "{step:e}");
                // One past the bound is never found, even when its product exists.
                assert_eq!(cadence_index(time, *step, tick - 1), None, "{step:e}");
            }
        }
    }
}

#[test]
fn a_negative_or_non_finite_time_is_never_a_tick() {
    let clock = clock(60.0, 1800.0).expect("clock");
    for time in [-60.0, -5e-324, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(clock.tick_at(time), None, "{time}");
    }
    assert_eq!(clock.tick_at(1800.0), Some(30));
    assert_eq!(clock.tick_at(1860.0), None, "after the horizon");
    assert_eq!(clock.tick_at(1_800.0_f64.next_up()), None);
    assert_eq!(clock.tick_at(90.0), None, "between two ticks");
}

/// Every tick whose product lies in the window, by looking at each one.
fn covered(clock: &Clock, from_s: f64, to_s: f64) -> Option<(u64, u64)> {
    let inside: Vec<u64> = (0..=clock.last_tick())
        .filter(|tick| {
            let time = clock.tick_time(*tick).expect("a tick");
            from_s <= time && time <= to_s
        })
        .collect();
    Some((*inside.first()?, *inside.last()?))
}

#[test]
fn a_window_holds_exactly_the_ticks_whose_products_lie_in_it() {
    let third = 1.0 / 3.0;
    for (step, horizon) in [
        (0.1, 4.3),
        (60.0, 1800.0),
        (0.3, 30.0 * 0.3),
        (third, 9.0 * third),
    ] {
        let clock = clock(step, horizon).expect("clock");
        let mut probes = vec![-1.0, -0.0, horizon + step, f64::INFINITY, f64::NEG_INFINITY];
        for tick in 0..=clock.last_tick() {
            let time = clock.tick_time(tick).expect("a tick");
            probes.extend([time, time.next_up(), time.next_down(), time + step / 2.0]);
        }
        let mut windows = 0_u32;
        for from in &probes {
            for to in &probes {
                assert_eq!(
                    clock.ticks_within(*from, *to),
                    covered(&clock, *from, *to),
                    "step {step}, window [{from}, {to}]"
                );
                windows += 1;
            }
        }
        assert!(windows > 1_000, "{windows}");
    }
}

#[test]
fn a_window_touching_a_tick_at_either_bound_holds_it_and_one_between_ticks_holds_none() {
    let clock = clock(60.0, 1800.0).expect("clock");
    // Touching at the upper bound, at the lower bound, and at both.
    assert_eq!(clock.ticks_within(30.0, 60.0), Some((1, 1)));
    assert_eq!(clock.ticks_within(60.0, 90.0), Some((1, 1)));
    assert_eq!(clock.ticks_within(60.0, 60.0), Some((1, 1)));
    assert_eq!(clock.ticks_within(0.0, 1800.0), Some((0, 30)));
    assert_eq!(clock.ticks_within(1800.0, 1800.0), Some((30, 30)));
    // One ulp short of a tick at either end.
    assert_eq!(clock.ticks_within(30.0, 60.0_f64.next_down()), None);
    assert_eq!(clock.ticks_within(60.0_f64.next_up(), 90.0), None);
    // Strictly between two ticks, reversed, and NaN.
    assert_eq!(clock.ticks_within(61.0, 119.0), None);
    assert_eq!(clock.ticks_within(120.0, 60.0), None);
    assert_eq!(clock.ticks_within(f64::NAN, 60.0), None);
    assert_eq!(clock.ticks_within(0.0, f64::NAN), None);
}

#[test]
fn a_window_on_a_clock_at_the_tick_bound_is_decided_without_walking_it() {
    let clock = clock(1.0, 16_777_215.0).expect("at the bound");
    assert_eq!(clock.ticks_within(0.0, 16_777_215.0), Some((0, 16_777_215)));
    assert_eq!(
        clock.ticks_within(16_777_214.5, 16_777_215.0),
        Some((16_777_215, 16_777_215))
    );
    assert_eq!(clock.ticks_within(8_388_607.25, 8_388_607.75), None);
    assert_eq!(clock.tick_at(16_777_215.0), Some(16_777_215));
    assert_eq!(clock.tick_at(16_777_216.0), None);
}

#[test]
fn the_clock_keeps_its_numbers_as_written() {
    let clock = Clock::new(Number::Integer(60), Number::Real(1800.0), MAX_TICKS).expect("clock");
    assert_eq!(clock.step_written(), Number::Integer(60));
    assert_eq!(clock.horizon_written(), Number::Real(1800.0));
    assert_eq!(clock.step_s().to_bits(), 60.0_f64.to_bits());
    let same_times = Clock::new(Number::Real(60.0), Number::Integer(1800), MAX_TICKS).expect("ok");
    assert_ne!(clock, same_times, "the written kind is part of the clock");
    assert_eq!(clock.last_tick(), same_times.last_tick());
}
