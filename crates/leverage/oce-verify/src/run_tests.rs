//! The post-run order's refusals that no loaded set reaches through a run: a run-class engine
//! refusal, a frame replay refused for a missing input, and a window with no executed tick. Each
//! is driven here with the cause replay would hand back, so its code is pinned even though the
//! facade has no public call that produces it (a fresh engine never exhausts its frame
//! sequence, every vector input is a step list from `t = 0`, and a run that refused no frame
//! executed every tick of every window).

use oce_replay::{EngineRefusal, RefusalCause, ReplayError};

use super::{coverage_error, stop_outcome};

#[test]
fn a_run_class_refusal_is_the_error_run_refused() {
    let error = stop_outcome(
        "scenarios[3]",
        7,
        &RefusalCause::Engine {
            refusal: EngineRefusal::Run,
            message: "accepted frame sequence is exhausted".to_owned(),
        },
    )
    .expect_err("a refusal about the run is an error, not a verdict");
    assert_eq!(error.code(), "run-refused");
    assert_eq!(error.subject(), "scenarios[3]");
    assert!(error.detail().contains("exhausted"), "{error}");
}

#[test]
fn every_frame_class_refusal_fails_the_scenario_at_its_row() {
    for refusal in [
        EngineRefusal::InputType,
        EngineRefusal::InputNonFinite,
        EngineRefusal::InputDomain,
        EngineRefusal::NonFiniteTime,
        EngineRefusal::TimeOutOfRange,
        EngineRefusal::TimeRegression,
        EngineRefusal::NonFiniteTimeStep,
        EngineRefusal::ModelTimeUnrepresentable,
    ] {
        let stopped = stop_outcome(
            "scenarios[0]",
            2,
            &RefusalCause::Engine {
                refusal,
                message: String::new(),
            },
        );
        assert_eq!(stopped.ok(), Some((2, refusal)), "{refusal:?}");
    }
}

#[test]
fn a_frame_refused_for_a_missing_input_is_a_source_failure_and_not_a_verdict() {
    let error = stop_outcome(
        "scenarios[1]",
        0,
        &RefusalCause::MissingInput {
            slot: 0,
            path: "urn:x#top.u".to_owned(),
        },
    )
    .expect_err("no step source leaves an input missing");
    assert_eq!(error.code(), "replay-failed");
    assert_eq!(error.cause(), Some("source"));
    assert_eq!(error.path(), Some("urn:x#top.u"));
}

#[test]
fn an_uncovered_window_names_its_expectation_by_its_place_in_the_scenario() {
    // The coverage check counts windows only; the second window here is the scenario's
    // expectation 3 (a diagnostic instant sits before it).
    let positions = [0, 3];
    let error = coverage_error(
        "scenarios[2]",
        &positions,
        &ReplayError::Uncovered { index: 1 },
    );
    assert_eq!(error.code(), "window-uncovered");
    assert_eq!(error.subject(), "scenarios[2].expect[3]");
    // Any other error of the check is a replay failure.
    let other = coverage_error(
        "scenarios[2]",
        &positions,
        &ReplayError::InvalidWindow { index: 0 },
    );
    assert_eq!(other.code(), "replay-failed");
    assert_eq!(other.cause(), Some("engine-setup"));
}
