//! Acceptance (issue #445): the controller fixture replays one day at 5 s within the documented
//! size (design note, "Size budget per frame": 89 B/frame outputs only, 91 B with the
//! substitution mask, 180 B with inputs; 1.5, 1.6 and 3.1 MB for 17,280 frames).
//!
//! The day is the golden-gen reference (minute samples) on a 5 s grid from 0 to 86,400 s
//! inclusive: 17,281 ticks. As a step source (signals, no holds) the trace is outputs only; as a
//! columnar source with every input on `HoldLast { max_age: 60 }` it carries the mask; with
//! inputs retained it carries them too. The frame bytes are exactly `rows × B/frame` and within
//! the documented megabytes; the warnings table is reported beside them.

#![allow(clippy::print_stdout)] // The budget lines are the evidence the PR pastes.

#[path = "support/controller.rs"]
mod controller;
#[path = "support/fixtures.rs"]
mod fixtures;

use fixtures::load;
use oce_replay::{FrameSource, MissingPolicy, Replay, ReplayOptions, TimeBase, Trace};

fn day<S: FrameSource>(
    missing: &MissingPolicy,
    inputs: bool,
    make: impl Fn(&oce_replay::InputMap, &controller::Table) -> S,
) -> Trace {
    let reference = controller::reference();
    let engine = load(controller::FIXTURE);
    let map = controller::map(&engine, &reference, missing);
    let source = make(&map, &reference);
    let mut options = ReplayOptions::new(TimeBase::Grid { t0: 0.0, step: 5.0 });
    options.trace.inputs = inputs;
    let (_, trace) = Replay::new(engine, map, source, options)
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(trace.rows(), 17_281, "0 to 86,400 s at 5 s");
    trace
}

fn check(label: &str, trace: &Trace, per_frame: u64, documented_mb: f64) {
    let rows = trace.rows() as u64;
    assert_eq!(trace.bytes_per_frame(), per_frame, "{label}");
    assert_eq!(trace.frame_bytes(), rows * per_frame, "{label}");
    // The note's figures are for 17,280 frames, rounded to 0.1 MB.
    let mb = (17_280 * per_frame) as f64 / 1e6;
    assert!(
        (mb - documented_mb).abs() < 0.05,
        "{label}: {mb} MB vs {documented_mb}"
    );
    assert!(
        trace.frame_bytes() as f64 / 1e6 < documented_mb + 0.05,
        "{label}"
    );
    println!(
        "{label}: {rows} rows x {per_frame} B = {} B frame columns; {} warnings, {} B in all",
        trace.frame_bytes(),
        trace.diagnostics().len(),
        trace.byte_size()
    );
}

#[test]
fn the_controller_replays_one_day_at_5_s_within_the_documented_size() {
    let outputs_only = day(&MissingPolicy::Refuse, false, controller::steps);
    assert!(outputs_only.is_clean());
    check("outputs only", &outputs_only, 89, 1.5);

    let held = day(
        &MissingPolicy::HoldLast { max_age: 60.0 },
        false,
        controller::columnar,
    );
    assert!(!held.is_clean());
    assert!(
        held.status().iter().all(|&s| s <= 1),
        "every tick executed or held"
    );
    check("outputs and substitution mask", &held, 91, 1.6);

    let with_inputs = day(&MissingPolicy::Refuse, true, controller::steps);
    check("outputs and inputs", &with_inputs, 180, 3.1);
}
