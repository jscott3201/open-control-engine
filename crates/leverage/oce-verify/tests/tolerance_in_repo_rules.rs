//! The funnel port's tick-grid measurement re-pinned on the windows of the two in-repo rules
//! (issue #684, "First: measure, and stop"; vectors note decision 41).
//!
//! `tests/funnel_tick_grid.rs` (GitHub #687) measured that a series on a vector clock's grid is
//! inside its own funnel for every `x` tolerance from none to ten ticks, the one failure being
//! rounding with no `y` tolerance and a step of about a million. The two rules vendored here,
//! AHU-0016 and AHU-0004, are what a tolerance window would sit on: their 21 windows are Boolean
//! `equals` windows. Read as tolerance windows, each window's expectation over its own ticks is
//! the reference a self-enclosure call would see (a Boolean as 0 or 1), and every one of them
//! is inside its own envelope under both envelopes, at an `x` tolerance of none, one and two
//! steps, with no `y` tolerance and with a small one. The same holds for each scenario's
//! expectations joined into one series over the whole clock, which has the windows' edges as
//! steps. Had any been refused, the build would have stopped and reported on the issue.

mod support;

use oce_verify::{
    CompareLimits, Envelope, Expectation, Literal, Series, Tolerances, VectorSet, VerifyLimits,
    Window,
};

use support::{LIBRARY_RULES, library_vectors};

fn rule(name: &str) -> VectorSet {
    VectorSet::from_json_bytes(&library_vectors(name), &VerifyLimits::default())
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// A Boolean or number literal as the reference's `y`.
fn level(literal: Literal) -> f64 {
    match literal {
        Literal::Boolean(value) => f64::from(u8::from(value)),
        Literal::Integer(value) => i32::try_from(value).map_or(f64::NAN, f64::from),
        Literal::Real(value) => value,
        _ => f64::NAN,
    }
}

/// The tolerances tried: `x` of none, one and two steps, `y` of none and of 0.01.
fn tolerances(step_s: f64) -> Vec<Tolerances> {
    let mut all = Vec::new();
    for ticks in [0.0, 1.0, 2.0] {
        for atoly in [0.0, 0.01] {
            all.push(Tolerances {
                atolx: ticks * step_s,
                atoly,
                ..Tolerances::ZERO
            });
        }
    }
    all
}

/// Whether the reference `(x, y)` is inside its own envelope under every tolerance and both
/// envelopes; the failures otherwise, named.
fn self_enclosed(name: &str, x: &[f64], y: &[f64], step_s: f64) -> Vec<String> {
    let mut failures = Vec::new();
    for envelope in Envelope::ALL {
        for tolerances in tolerances(step_s) {
            let reference = Series::new(x, y);
            match envelope.compare(reference, reference, &tolerances, &CompareLimits::default()) {
                Ok(result) if result.within_envelope() => {}
                Ok(result) => failures.push(format!(
                    "{name} under {envelope} {tolerances:?}: outside by {} at {:?}",
                    result.max_error, result.max_error_x
                )),
                Err(error) => failures.push(format!("{name} under {envelope}: refused, {error}")),
            }
        }
    }
    failures
}

fn windows(set: &VectorSet) -> Vec<(String, &Window)> {
    set.scenarios()
        .iter()
        .flat_map(|scenario| {
            scenario
                .expectations()
                .iter()
                .enumerate()
                .filter_map(move |(index, expectation)| match expectation {
                    Expectation::Window(window) => {
                        Some((format!("{}[{index}]", scenario.name()), window))
                    }
                    _ => None,
                })
        })
        .collect()
}

#[test]
fn every_window_of_the_in_repo_rules_is_inside_its_own_envelope() {
    let mut counted = 0;
    let mut failures = Vec::new();
    for name in LIBRARY_RULES {
        let set = rule(name);
        let clock = set.clock();
        for (label, window) in windows(&set) {
            let ticks: Vec<f64> = (window.first_tick()..=window.last_tick())
                .map(|tick| clock.tick_time(tick).expect("a tick of the clock"))
                .collect();
            let value = level(window.equals().expect("every in-repo window is exact"));
            let y = vec![value; ticks.len()];
            failures.extend(self_enclosed(
                &format!("{name} {label}"),
                &ticks,
                &y,
                clock.step_s(),
            ));
            counted += 1;
        }
    }
    assert_eq!(counted, 21, "the note counts 21 windows in the two rules");
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn each_scenarios_expectations_over_the_clock_are_inside_their_own_envelope() {
    // A sharper reference than one window: a scenario's windows joined in tick order, so each
    // window edge where the expected level changes is a step of the series. Ticks no window
    // covers hold the level before them.
    let mut failures = Vec::new();
    let mut stepped = 0;
    for name in LIBRARY_RULES {
        let set = rule(name);
        let clock = set.clock();
        for scenario in set.scenarios() {
            let mut levels: Vec<Option<f64>> =
                vec![None; usize::try_from(clock.tick_count()).expect("fits")];
            for expectation in scenario.expectations() {
                if let Expectation::Window(window) = expectation {
                    for tick in window.first_tick()..=window.last_tick() {
                        let at = usize::try_from(tick).expect("fits");
                        levels[at] = Some(level(window.equals().expect("exact")));
                    }
                }
            }
            let mut held = 0.0;
            let y: Vec<f64> = levels
                .iter()
                .map(|level| {
                    held = level.unwrap_or(held);
                    held
                })
                .collect();
            if y.windows(2).any(|pair| pair[0] != pair[1]) {
                stepped += 1;
            }
            let x: Vec<f64> = (0..clock.tick_count())
                .map(|tick| clock.tick_time(tick).expect("a tick"))
                .collect();
            failures.extend(self_enclosed(
                &format!("{name} {}", scenario.name()),
                &x,
                &y,
                clock.step_s(),
            ));
        }
    }
    assert!(stepped > 0, "some scenario's expectation steps");
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_rounding_case_scaled_onto_a_rules_clock_is_the_ports() {
    // The port's one measured failure, on AHU-0016's 60 s clock: a 0/1 window scaled to a step
    // of 2e6 with no y tolerance and an x tolerance of one step leaves its own funnel, and a y
    // tolerance of one part in a million of the scale, an x tolerance off the step or
    // aligned-grid each removes it (as `tests/tolerance_windows.rs` shows through a run).
    let step_s = rule("AHU-0016").clock().step_s();
    let x = [0.0, step_s, 2.0 * step_s];
    let y = [2e6, 0.0, 0.0];
    let reference = Series::new(&x, &y);
    let outside = Envelope::Funnel
        .compare(
            reference,
            reference,
            &Tolerances {
                atolx: step_s,
                ..Tolerances::ZERO
            },
            &CompareLimits::default(),
        )
        .expect("compared");
    assert!(!outside.within_envelope());
    assert!(
        outside.max_error > 0.0 && outside.max_error < 1e-9,
        "{}",
        outside.max_error
    );
    for (envelope, tolerances) in [
        (
            Envelope::Funnel,
            Tolerances {
                atolx: step_s,
                atoly: 2.0,
                ..Tolerances::ZERO
            },
        ),
        (
            Envelope::Funnel,
            Tolerances {
                atolx: step_s - 1.0,
                ..Tolerances::ZERO
            },
        ),
        (
            Envelope::AlignedGrid,
            Tolerances {
                atolx: step_s,
                ..Tolerances::ZERO
            },
        ),
    ] {
        let result = envelope
            .compare(reference, reference, &tolerances, &CompareLimits::default())
            .expect("compared");
        assert!(result.within_envelope(), "{envelope} {tolerances:?}");
    }
}
