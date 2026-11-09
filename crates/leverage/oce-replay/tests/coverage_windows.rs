//! Coverage before verdicts (design note, test plan: trace size and coverage; decision 7): a
//! window with no executed tick refuses, one touching an executed tick at a bound passes, and a
//! window covered only by refused or skipped rows refuses.

#[path = "support/controller.rs"]
mod controller;
#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/scenarios.rs"]
mod scenarios;

use oce_replay::ReplayError;

#[test]
fn windows_need_an_executed_tick_and_a_tick_on_a_bound_counts() {
    let mut replay = scenarios::every_status(|_| {});
    // Before any frame, nothing is covered.
    assert!(matches!(
        replay.check_coverage(&[(0.0, 1.0)]),
        Err(ReplayError::Uncovered { index: 0 })
    ));
    assert!(replay.check_coverage(&[]).is_ok());
    replay.advance(100).unwrap();
    // Times: 0, 1, 2, 5, 10, 20, 30, 45, 60, 61, ...; statuses 0 0 1 1 2 0 3 0 1 4 ...
    let ok = [
        (0.0, 0.0),     // a point window on an executed tick
        (0.5, 1.0),     // the tick on the upper bound
        (1.0, 1.5),     // the tick on the lower bound
        (2.0, 2.0),     // a substituted row executed
        (-10.0, 100.0), // everything
    ];
    for window in ok {
        assert!(replay.check_coverage(&[window]).is_ok(), "{window:?}");
    }
    let uncovered = [
        (0.25, 0.75),   // between ticks
        (10.0, 10.0),   // a skipped row only
        (25.0, 35.0),   // a RefusedMissing row only (30)
        (61.0, 61.0),   // a RefusedEngine row only
        (600.0, 700.0), // after the last tick
    ];
    for window in uncovered {
        assert!(
            matches!(
                replay.check_coverage(&[window]),
                Err(ReplayError::Uncovered { index: 0 })
            ),
            "{window:?}"
        );
    }
    // The first uncovered window is named, after every window is validated.
    assert!(matches!(
        replay.check_coverage(&[(0.0, 1.0), (0.25, 0.75), (10.0, 10.0)]),
        Err(ReplayError::Uncovered { index: 1 })
    ));
    for (index, bad) in [(f64::NAN, 1.0), (0.0, f64::INFINITY), (2.0, 1.0)]
        .iter()
        .enumerate()
    {
        let windows = [(0.0, 1.0), (0.25, 0.75), *bad];
        assert!(
            matches!(
                replay.check_coverage(&windows),
                Err(ReplayError::InvalidWindow { index: 2 })
            ),
            "{index}"
        );
    }
    let (_, trace) = replay.run().unwrap();
    assert!(trace.check_coverage(&[(0.0, 0.0)]).is_ok());
}
