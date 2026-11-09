//! `Trace::canonical_bytes()` goldens (design note, test plan: determinism goldens; decision 23).
//!
//! Four runs are pinned byte for byte under `tests/fixtures/`, each covering a different path:
//!
//! * `controller_minute_day.trace` — `cooling_only_controller`, its whole minute-by-minute
//!   reference day through `ColumnarSource` on the `Recorded` time base (1,441 rows, outputs only).
//! * `controller_grid_hold.trace` — the same fixture on a 5 s grid over two hours with every
//!   input on `HoldLast { max_age: 60 }`: ticks, holds and the substitution mask.
//! * `every_status.trace` — every status byte, the refusal table and retained inputs
//!   (`support/scenarios.rs`).
//! * `csv_instants.trace` — `generic_time_suppression` from a CSV whose times are RFC 3339 instants
//!   with `Z`, negative and positive offsets and a fraction, against a header-only origin, with a
//!   blank and an `NA` cell held and an ignored column.
//!
//! None takes a state snapshot: a snapshot may be target-bound, and these bytes must be equal on
//! x86_64 and aarch64. Regenerate only through the repository's bless switch:
//! `OCE_BLESS_REPLAY=1 cargo test -p oce-replay --test canonical_goldens`, then run again without
//! it and say in the PR which files moved and why.

#[path = "support/controller.rs"]
mod controller;
#[path = "support/fixtures.rs"]
mod fixtures;
#[path = "support/scenarios.rs"]
mod scenarios;

use std::path::PathBuf;

use fixtures::load;
use oce_replay::{
    ColumnKind, ColumnMap, ColumnSpec, CsvFormat, CsvSource, CsvTime, InputMap, MissingPolicy,
    Replay, ReplayLimits, ReplayOptions, TRACE_MAGIC, TimeBase, TimeOrigin, Trace,
};

fn check(name: &str, trace: &Trace) {
    let bytes = trace.canonical_bytes();
    assert_eq!(&bytes[..8], TRACE_MAGIC);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.trace"));
    if oce_bless::enabled("OCE_BLESS_REPLAY") {
        std::fs::write(&path, &bytes).unwrap();
        return;
    }
    let golden =
        std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    if golden != bytes {
        let first = golden
            .iter()
            .zip(&bytes)
            .position(|(a, b)| a != b)
            .unwrap_or(golden.len().min(bytes.len()));
        panic!(
            "{name}: first difference at byte {first:#x}; golden {} bytes, trace {} bytes",
            golden.len(),
            bytes.len()
        );
    }
}

/// The CSV-instants scenario's text.
pub const CSV_INSTANTS: &str = "stamp,TZon,TSet,comment\n\
2026-11-08T00:00:00Z,293.15,295.15,start\n\
2026-11-08T00:00:05Z,293.65,295.15,\n\
2026-11-07T19:00:10-05:00,,295.15,blank TZon is held\n\
2026-11-08T05:30:15+05:30,294.5,NA,NA TSet is held\n\
2026-11-08T00:00:20.5Z,295.25,295.15,fraction\n";

fn csv_instants() -> Trace {
    let engine = load("generic_time_suppression");
    let specs = [
        ColumnSpec::new("TZon", ColumnKind::Real),
        ColumnSpec::new("TSet", ColumnKind::Real),
    ];
    let map = InputMap::resolve(
        &engine.input_definitions().unwrap(),
        &specs,
        &ColumnMap::by_name(&specs),
        &MissingPolicy::HoldLast { max_age: 10.0 },
        &[],
    )
    .unwrap();
    let format = CsvFormat {
        time: CsvTime::Instant {
            origin: TimeOrigin::parse("2026-11-08T00:00:00Z").unwrap(),
        },
        ..CsvFormat::seconds("stamp")
    };
    let source = CsvSource::new(
        &map,
        CSV_INSTANTS.as_bytes(),
        format,
        &ReplayLimits::default(),
    )
    .unwrap();
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.trace.inputs = true;
    let (_, trace) = Replay::new(engine, map, source, options)
        .unwrap()
        .run()
        .unwrap();
    trace
}

#[test]
fn the_controller_minute_day_trace_matches_its_golden() {
    let reference = controller::reference();
    let engine = load(controller::FIXTURE);
    let map = controller::map(&engine, &reference, &MissingPolicy::Refuse);
    let source = controller::columnar(&map, &reference);
    let replay = Replay::new(engine, map, source, ReplayOptions::new(TimeBase::Recorded)).unwrap();
    let (_, trace) = replay.run().unwrap();
    assert_eq!(trace.rows(), 1_441);
    check("controller_minute_day", &trace);
}

#[test]
fn the_controller_grid_hold_trace_matches_its_golden() {
    let (_, trace) = scenarios::controller_grid_hold().run().unwrap();
    assert_eq!(trace.rows(), 1_441, "ticks 0, 5, ..., 7200");
    assert!(trace.substitution_mask().is_some());
    check("controller_grid_hold", &trace);
}

#[test]
fn the_every_status_trace_matches_its_golden() {
    let (_, trace) = scenarios::every_status(|_| {}).run().unwrap();
    let mut seen: Vec<u8> = trace.status().to_vec();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, [0, 1, 2, 3, 4], "every status is present");
    check("every_status", &trace);
}

#[test]
fn the_csv_instants_trace_matches_its_golden() {
    let trace = csv_instants();
    let times: Vec<f64> = trace.times().to_vec();
    assert_eq!(times, [0.0, 5.0, 10.0, 15.0, 20.5]);
    assert_eq!(trace.status(), [0, 0, 1, 1, 0]);
    assert_eq!(trace.meta().ignored_columns, ["comment"]);
    assert_eq!(
        trace.meta().origin.as_ref().map(TimeOrigin::as_str),
        Some("2026-11-08T00:00:00Z")
    );
    check("csv_instants", &trace);
}

#[test]
fn canonical_bytes_are_identical_across_runs() {
    let a = scenarios::every_status(|_| {})
        .run()
        .unwrap()
        .1
        .canonical_bytes();
    let b = scenarios::every_status(|_| {})
        .run()
        .unwrap()
        .1
        .canonical_bytes();
    assert_eq!(a, b);
}
