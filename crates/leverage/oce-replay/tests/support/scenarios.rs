//! Named replay scenarios shared by the golden, chunk-size and census suites.
//!
//! * `every_status`: `multizone_vav_economizer_modulations_reliefs` (five Real inputs, four of
//!   them bounded to `[0, 1]`) over 24 hand-written frames under `RecordAndContinue`, with
//!   `uOutDam_max` on `HoldLast { max_age: 30 }`, `uOutDam_min` on `SkipFrame`, `uTSup` on
//!   `Fallback(0.5)` and the other two on the run-level `Refuse`. Its missing cells and one
//!   out-of-domain value produce every status: Executed, Substituted, Skipped, RefusedMissing and
//!   RefusedEngine.
//! * `controller_grid_hold`: `cooling_only_controller` on a 5 s grid over the first two hours of
//!   its minute-by-minute reference, every input on `HoldLast { max_age: 60 }`, so each tick
//!   between reference rows is a hold recorded in the substitution mask.

#![allow(dead_code)]

use oce_api::Value;
use oce_replay::{
    ColumnarSource, InputMap, MissingPolicy, OnRefusal, Replay, ReplayOptions, TimeBase, TimeColumn,
};

use crate::controller;
use crate::fixtures::{Frames, columnar_with_missing, load, path_specs};

/// The every-status fixture.
pub const EVERY_STATUS_FIXTURE: &str = "multizone_vav_economizer_modulations_reliefs";

/// The every-status frames: `(t, [uOutDam_max, uOutDam_min, uRetDam_max, uRetDam_min, uTSup])`.
pub fn every_status_frames() -> Frames {
    let times = [
        0.0, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0, 45.0, 60.0, 61.0, 75.0, 90.0, 120.0, 150.0, 151.0,
        152.0, 180.0, 240.0, 300.0, 301.0, 360.0, 420.0, 480.0, 540.0,
    ];
    times
        .iter()
        .enumerate()
        .map(|(k, &t)| {
            let phase = (k % 5) as f64 / 5.0;
            let values = vec![
                Value::Real(0.2 + phase / 2.0),
                Value::Real(phase / 4.0),
                // Frame 9 is out of [0, 1]: the engine refuses it with InputDomain.
                Value::Real(if k == 9 { 2.0 } else { 0.9 - phase / 3.0 }),
                Value::Real(0.1 + phase / 5.0),
                Value::Real(285.0 + k as f64 * 0.25),
            ];
            (t, values)
        })
        .collect()
}

/// The every-status missing cells, `(frame, slot)`.
pub const EVERY_STATUS_MISSING: &[(usize, usize)] = &[
    (2, 0),  // held: last observed at 1.0, age 1
    (3, 4),  // fallback 0.5
    (4, 1),  // skipped
    (6, 2),  // refused: Refuse
    (8, 0),  // age 60 - 45 = 15 since frame 7: held
    (11, 0), // held from 75 (frame 10): age 15
    (12, 0), // held from 75: age 45 > 30: refused (HoldLast expired)
    (13, 0), // held from 75: still expired: refused
    (13, 4), // fallback (the frame is refused anyway; the mask still records it)
    (16, 1), // skipped
    (16, 3), // refused: refusal dominates the skip
    (20, 4), // fallback
    (21, 0), // held from 360 (frame 20): age 60 > 30: refused
];

/// The every-status map: run-level `Refuse` with the three overrides.
pub fn every_status_map() -> InputMap {
    let engine = load(EVERY_STATUS_FIXTURE);
    let specs = path_specs(&engine);
    InputMap::resolve(
        &engine.input_definitions().unwrap(),
        &specs,
        &oce_replay::ColumnMap::by_name(&specs),
        &MissingPolicy::Refuse,
        &[
            (
                "uOutDam_max".to_owned(),
                MissingPolicy::HoldLast { max_age: 30.0 },
            ),
            ("uOutDam_min".to_owned(), MissingPolicy::SkipFrame),
            (
                "uTSup".to_owned(),
                MissingPolicy::Fallback(Value::Real(0.5)),
            ),
        ],
    )
    .unwrap()
}

/// The every-status replay, inputs retained, `RecordAndContinue`; `tweak` adjusts the options.
pub fn every_status(tweak: impl FnOnce(&mut ReplayOptions)) -> Replay<ColumnarSource> {
    let map = every_status_map();
    let source = columnar_with_missing(&map, &every_status_frames(), EVERY_STATUS_MISSING);
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.on_refusal = OnRefusal::RecordAndContinue;
    options.trace.inputs = true;
    tweak(&mut options);
    Replay::new(load(EVERY_STATUS_FIXTURE), map, source, options).unwrap()
}

/// The controller on a 5 s grid over the reference's first two hours, every input held up to
/// 60 s.
pub fn controller_grid_hold() -> Replay<ColumnarSource> {
    let mut reference = controller::reference();
    reference.rows.truncate(121);
    let engine = load(controller::FIXTURE);
    let map = controller::map(
        &engine,
        &reference,
        &MissingPolicy::HoldLast { max_age: 60.0 },
    );
    let source = ColumnarSource::new(
        &map,
        TimeColumn::Seconds(reference.column("time")),
        controller::columns(&reference),
        &oce_replay::ReplayLimits::default(),
    )
    .unwrap();
    let options = ReplayOptions::new(TimeBase::Grid { t0: 0.0, step: 5.0 });
    Replay::new(engine, map, source, options).unwrap()
}
