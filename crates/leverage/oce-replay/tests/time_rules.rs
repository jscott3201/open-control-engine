//! Time: the grid, equal times, pass-through, regression, missing time, and the conversion
//! goldens (design note, test plan: time; decisions 5 and 17).

#[path = "support/fixtures.rs"]
mod fixtures;

use fixtures::{Frames, columnar, load, path_map};
use oce_api::Value;
use oce_replay::time::seconds_from_nanos;
use oce_replay::{
    Column, ColumnarSource, CsvFormat, CsvSource, DuplicateTime, MissingPolicy, Progress, Replay,
    ReplayError, ReplayLimits, ReplayOptions, SourceError, StepSource, TimeBase, TimeColumn,
    TimeOrigin,
};

const FIXTURE: &str = "generic_time_suppression";

fn frames(times: &[f64]) -> Frames {
    times
        .iter()
        .map(|&t| (t, vec![Value::Real(295.0), Value::Real(293.0)]))
        .collect()
}

fn recorded(times: &[f64], duplicate: DuplicateTime) -> (Result<Progress, ReplayError>, Vec<f64>) {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let source = columnar(&map, &frames(times));
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.duplicate_time = duplicate;
    let mut replay = Replay::new(engine, map, source, options).unwrap();
    let progress = replay.advance(100);
    (progress, replay.trace().times().to_vec())
}

#[test]
fn grid_ticks_are_t0_plus_k_times_step_computed_directly() {
    let k: u64 = 1 << 53;
    for (t0, step) in [(0.0, 5.0), (0.25, 0.1), (1_000.3, 0.7), (-7.5, 1.0 / 3.0)] {
        let grid = TimeBase::Grid { t0, step };
        #[allow(clippy::cast_precision_loss)]
        let direct = t0 + (k as f64) * step;
        assert_eq!(
            grid.tick(k).unwrap().to_bits(),
            direct.to_bits(),
            "{t0} {step}"
        );
        assert_eq!(grid.tick(0).unwrap().to_bits(), t0.to_bits());
    }
    assert_eq!(TimeBase::Recorded.tick(3), None);

    // A run's grid times are those ticks, and they are not an accumulated sum.
    let (t0, step) = (1_000.3, 0.1);
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let steps = (0..map.len())
        .map(|slot| (map.column(slot).to_owned(), vec![(t0, Value::Real(290.0))]))
        .collect();
    let source =
        StepSource::with_instants(&map, steps, vec![t0 + 100.0], &ReplayLimits::default()).unwrap();
    let options = ReplayOptions::new(TimeBase::Grid { t0, step });
    let (_, trace) = Replay::new(engine, map, source, options)
        .unwrap()
        .run()
        .unwrap();
    let mut accumulated = t0;
    let mut drifted = false;
    for (k, t) in trace.times().iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let direct = t0 + (k as f64) * step;
        assert_eq!(t.to_bits(), direct.to_bits(), "tick {k}");
        drifted |= accumulated.to_bits() != t.to_bits();
        accumulated += step;
    }
    assert!(
        drifted,
        "the accumulated sum must differ somewhere for this step"
    );
    assert!(
        *trace.times().last().unwrap() <= t0 + 100.0,
        "ticks stop at the last sample"
    );
    // 1000 * 0.1 rounds to exactly 100, so tick 1000 lands on the last instant: 1,001 ticks.
    assert_eq!(trace.rows(), 1_001);
    assert_eq!(trace.times()[1_000].to_bits(), (t0 + 100.0).to_bits());
}

#[test]
fn an_invalid_grid_is_refused() {
    for (t0, step) in [
        (0.0, 0.0),
        (0.0, -1.0),
        (f64::NAN, 1.0),
        (0.0, f64::INFINITY),
        (f64::INFINITY, 1.0),
    ] {
        let engine = load(FIXTURE);
        let map = path_map(&engine, &MissingPolicy::Refuse);
        let source = columnar(&map, &frames(&[0.0]));
        let result = Replay::new(
            engine,
            map,
            source,
            ReplayOptions::new(TimeBase::Grid { t0, step }),
        );
        assert!(
            matches!(result, Err(ReplayError::InvalidTimeBase)),
            "{t0} {step}"
        );
    }
}

#[test]
fn recorded_times_pass_through_bit_for_bit() {
    let times = [
        -0.0,
        0.1,
        0.300_000_000_000_000_04,
        1.000_000_000_000_000_2,
        7.5,
        1e6 + 0.1,
    ];
    let (progress, seen) = recorded(&times, DuplicateTime::Refuse);
    assert_eq!(progress.unwrap(), Progress::Done { frames: 6 });
    let bits: Vec<u64> = seen.iter().map(|t| t.to_bits()).collect();
    let expected: Vec<u64> = times.iter().map(|t| t.to_bits()).collect();
    assert_eq!(bits, expected);
}

#[test]
fn equal_times_are_refused_by_default_and_kept_as_separate_frames_under_allow() {
    let (progress, seen) = recorded(&[0.0, 1.0, 1.0, 2.0], DuplicateTime::Refuse);
    assert!(matches!(
        progress,
        Err(ReplayError::Source(SourceError::DuplicateTime { row: 3 }))
    ));
    assert_eq!(seen, [0.0, 1.0], "refused before the engine saw the row");
    // -0.0 equals 0.0.
    let (progress, _) = recorded(&[0.0, -0.0], DuplicateTime::Refuse);
    assert!(matches!(
        progress,
        Err(ReplayError::Source(SourceError::DuplicateTime { row: 2 }))
    ));

    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let source = columnar(&map, &frames(&[0.0, 1.0, 1.0, 1.0, 2.0]));
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.duplicate_time = DuplicateTime::Allow;
    let (mut engine, trace) = Replay::new(engine, map, source, options)
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(trace.times(), [0.0, 1.0, 1.0, 1.0, 2.0]);
    assert_eq!(trace.status(), [0, 0, 0, 0, 0]);
    // Five accepted transitions: the next is the sixth.
    let slots = engine.frame_slots().unwrap();
    let values = frames(&[3.0])[0].1.clone();
    assert_eq!(
        engine
            .execute_slots(&slots, 3.0, &values)
            .unwrap()
            .sequence(),
        6
    );
}

#[test]
fn a_time_regression_is_refused_before_the_engine() {
    let (progress, seen) = recorded(&[0.0, 2.0, 1.0], DuplicateTime::Allow);
    assert!(matches!(
        progress,
        Err(ReplayError::Source(SourceError::TimeRegression { row: 3 }))
    ));
    assert_eq!(seen, [0.0, 2.0]);
}

#[test]
fn a_missing_or_infinite_time_is_refused_before_the_engine() {
    let (progress, seen) = recorded(&[0.0, f64::NAN], DuplicateTime::Refuse);
    assert!(matches!(
        progress,
        Err(ReplayError::Source(SourceError::MissingTime { row: 2 }))
    ));
    assert_eq!(seen, [0.0]);
    let (progress, _) = recorded(&[f64::INFINITY], DuplicateTime::Refuse);
    assert!(matches!(
        progress,
        Err(ReplayError::Source(SourceError::NonFiniteTime { row: 1 }))
    ));

    // NaT in an epoch-nanosecond column.
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let time = TimeColumn::EpochNanos {
        values: vec![0, i64::MIN],
        origin: 0,
    };
    let columns = (0..map.len())
        .map(|s| (map.column(s).to_owned(), Column::Real(vec![290.0, 290.0])))
        .collect();
    let source = ColumnarSource::new(&map, time, columns, &ReplayLimits::default()).unwrap();
    let mut replay =
        Replay::new(engine, map, source, ReplayOptions::new(TimeBase::Recorded)).unwrap();
    assert!(matches!(
        replay.advance(10),
        Err(ReplayError::Source(SourceError::MissingTime { row: 2 }))
    ));
    assert!(matches!(replay.advance(10), Err(ReplayError::Failed)));

    // A blank, NA or NaN time cell in a CSV.
    for cell in ["", "NA", "NaN", "nan"] {
        let engine = load(FIXTURE);
        let map = path_map(&engine, &MissingPolicy::Refuse);
        let header = format!("t,{},{}\n", map.column(0), map.column(1));
        let text = format!("{header}0,1,2\n{cell},1,2\n");
        let source = CsvSource::new(
            &map,
            text.as_bytes(),
            CsvFormat::seconds("t"),
            &ReplayLimits::default(),
        )
        .unwrap();
        let mut replay =
            Replay::new(engine, map, source, ReplayOptions::new(TimeBase::Recorded)).unwrap();
        assert!(
            matches!(
                replay.advance(10),
                Err(ReplayError::Source(SourceError::MissingTime { row: 2 }))
            ),
            "{cell:?}"
        );
    }
}

#[test]
fn nanosecond_offsets_convert_by_the_fixed_split() {
    // (offset in ns, expected f64 bits)
    let cases: [(i128, u64); 7] = [
        (0, 0),
        (1, 0x3e11_2e0b_e826_d695), // 1e-9
        (1_500_000_000, 1.5_f64.to_bits()),
        (-1_500_000_000, (-1.5_f64).to_bits()), // -2 + 0.5
        (-1, 0xbe11_2e0b_e000_0000),            // -1 + 0.999999999, not -1e-9's bits
        (86_400_000_000_123, 0x40f5_1800_0000_2104), // 86400 + 123e-9
        (-86_400_000_000_000, (-86_400.0_f64).to_bits()),
    ];
    for (offset, bits) in cases {
        assert_eq!(seconds_from_nanos(offset).to_bits(), bits, "{offset}");
    }
    // The columnar epoch-nanosecond column uses the same rule against its integer origin.
    let origin = 1_794_096_000_000_000_000_i64;
    let values = vec![origin - 1_500_000_000, origin, origin + 86_400_000_000_123];
    let column = TimeColumn::EpochNanos { values, origin };
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let columns = (0..map.len())
        .map(|s| (map.column(s).to_owned(), Column::Real(vec![290.0; 3])))
        .collect();
    let source = ColumnarSource::new(&map, column, columns, &ReplayLimits::default()).unwrap();
    let (_, trace) = Replay::new(engine, map, source, ReplayOptions::new(TimeBase::Recorded))
        .unwrap()
        .run()
        .unwrap();
    let bits: Vec<u64> = trace.times().iter().map(|t| t.to_bits()).collect();
    assert_eq!(bits, [(-1.5_f64).to_bits(), 0, 0x40f5_1800_0000_2104]);
}

#[test]
fn rfc_3339_instants_parse_with_explicit_offsets_negative_ones_included() {
    let origin = TimeOrigin::parse("2026-11-08T00:00:00Z").unwrap();
    assert_eq!(origin.unix_nanos(), 1_794_096_000 * 1_000_000_000);
    assert_eq!(origin.as_str(), "2026-11-08T00:00:00Z");
    let seconds = |s: &str| origin.seconds_of(s);
    assert_eq!(seconds("2026-11-08T00:00:00Z"), Some(0.0));
    assert_eq!(seconds("2026-11-07T19:00:00-05:00"), Some(0.0));
    assert_eq!(seconds("2026-11-08T05:30:00+05:30"), Some(0.0));
    assert_eq!(seconds("2026-11-07T23:59:59.5Z"), Some(-0.5));
    assert_eq!(seconds("2026-11-07T18:59:59.75-05:00"), Some(-0.25));
    assert_eq!(
        seconds("2026-11-09T00:00:00.000000123Z").map(f64::to_bits),
        Some(0x40f5_1800_0000_2104)
    );
    assert_eq!(seconds("2026-11-08T00:00:00.1Z"), Some(0.1));
    assert_eq!(seconds("2026-11-08T12:00:00-00:00"), Some(43_200.0));
    // Calendar edges against an epoch origin.
    let epoch = TimeOrigin::parse("1970-01-01T00:00:00Z").unwrap();
    assert_eq!(epoch.unix_nanos(), 0);
    assert_eq!(
        epoch.seconds_of("2024-02-29T00:00:00Z"),
        Some(1_709_164_800.0)
    );
    assert_eq!(
        epoch.seconds_of("0001-01-01T00:00:00Z"),
        Some(-62_135_596_800.0)
    );
    assert_eq!(
        epoch.seconds_of("9999-12-31T23:59:59Z"),
        Some(253_402_300_799.0)
    );
    assert_eq!(
        epoch
            .seconds_of("1969-12-31T23:59:59.999999999Z")
            .map(f64::to_bits),
        Some(0xbe11_2e0b_e000_0000)
    );
    // Outside the subset.
    for bad in [
        "2023-02-29T00:00:00Z",
        "2026-11-08T24:00:00Z",
        "2026-11-08T23:60:00Z",
        "2026-11-08T23:59:60Z",
        "2026-13-01T00:00:00Z",
        "2026-11-00T00:00:00Z",
        "2026-11-08t00:00:00Z",
        "2026-11-08T00:00:00z",
        "2026-11-08 00:00:00Z",
        "2026-11-08T00:00:00",
        "2026-11-08T00:00:00+0500",
        "2026-11-08T00:00:00+24:00",
        "2026-11-08T00:00:00+05:60",
        "2026-11-08T00:00:00.Z",
        "2026-11-08T00:00:00.1234567890Z",
        "2026-11-08T00:00:00ZZ",
        "+2026-11-08T00:00:00Z",
        "2026-11-08T00:00:00 Z",
        "",
    ] {
        assert_eq!(seconds(bad), None, "{bad:?}");
    }
    assert!(matches!(
        TimeOrigin::parse("2026-11-08"),
        Err(SourceError::BadInstant { row: 0 })
    ));
    // A CSV instant outside the subset is a typed error naming its row.
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let header = format!("t,{},{}\n", map.column(0), map.column(1));
    let text = format!("{header}2026-11-08T00:00:00Z,1,2\n2026-11-08T00:00:01+01,1,2\n");
    let format = CsvFormat {
        time: oce_replay::CsvTime::Instant { origin },
        ..CsvFormat::seconds("t")
    };
    let source = CsvSource::new(&map, text.as_bytes(), format, &ReplayLimits::default()).unwrap();
    let mut replay =
        Replay::new(engine, map, source, ReplayOptions::new(TimeBase::Recorded)).unwrap();
    assert!(matches!(
        replay.advance(10),
        Err(ReplayError::Source(SourceError::BadInstant { row: 2 }))
    ));
}

#[test]
fn the_origin_is_header_metadata_and_must_agree_with_the_source() {
    let origin = TimeOrigin::parse("2026-11-08T00:00:00Z").unwrap();
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let header = format!("t,{},{}\n", map.column(0), map.column(1));
    let text = format!("{header}2026-11-08T00:00:00Z,1,2\n");
    let format = CsvFormat {
        time: oce_replay::CsvTime::Instant {
            origin: origin.clone(),
        },
        ..CsvFormat::seconds("t")
    };
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.origin = Some(TimeOrigin::parse("2026-11-08T01:00:00+01:00").unwrap());
    let source = CsvSource::new(
        &map,
        text.as_bytes(),
        format.clone(),
        &ReplayLimits::default(),
    )
    .unwrap();
    // Same instant, different text: the header keeps one spelling, so they must be identical.
    assert!(matches!(
        Replay::new(engine, map.clone(), source, options),
        Err(ReplayError::OriginConflict)
    ));

    let source = CsvSource::new(&map, text.as_bytes(), format, &ReplayLimits::default()).unwrap();
    let (_, trace) = Replay::new(
        load(FIXTURE),
        map.clone(),
        source,
        ReplayOptions::new(TimeBase::Recorded),
    )
    .unwrap()
    .run()
    .unwrap();
    assert_eq!(trace.meta().origin.as_ref(), Some(&origin));
    assert_eq!(
        trace.times(),
        [0.0],
        "the engine sees model seconds, never the wall clock"
    );

    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.origin = Some(origin.clone());
    let source = columnar(&map, &frames(&[0.0]));
    let (_, trace) = Replay::new(load(FIXTURE), map, source, options)
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(trace.meta().origin.as_ref(), Some(&origin));
}

fn grid_trace(
    times: &[f64],
    tzon: &[f64],
    missing: MissingPolicy,
    duplicate: DuplicateTime,
) -> oce_replay::Trace {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &missing);
    let frames: Frames = times
        .iter()
        .zip(tzon)
        .map(|(&t, &z)| (t, vec![Value::Real(295.0), Value::Real(z)]))
        .collect();
    let source = columnar(&map, &frames);
    let mut options = ReplayOptions::new(TimeBase::Grid { t0: 0.0, step: 5.0 });
    options.on_refusal = oce_replay::OnRefusal::RecordAndContinue;
    options.duplicate_time = duplicate;
    options.trace.inputs = true;
    Replay::new(engine, map, source, options)
        .unwrap()
        .run()
        .unwrap()
        .1
}

fn tzon_column(trace: &oce_replay::Trace) -> Vec<f64> {
    match &trace.inputs().unwrap()[1].1 {
        oce_replay::TraceColumn::Real(v) => v.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn on_a_grid_a_sample_is_current_only_for_its_tick_interval_and_carrying_it_is_a_hold() {
    // Samples at 0 and 10 (and 12, 13); ticks 0, 5, 10.
    let times = [0.0, 10.0];
    let trace = grid_trace(
        &times,
        &[290.0, 291.0],
        MissingPolicy::Refuse,
        DuplicateTime::Refuse,
    );
    assert_eq!(trace.times(), [0.0, 5.0, 10.0]);
    assert_eq!(trace.status(), [0, 3, 0], "tick 5 has no sample in (0, 5]");

    let trace = grid_trace(
        &times,
        &[290.0, 291.0],
        MissingPolicy::HoldLast { max_age: 5.0 },
        DuplicateTime::Refuse,
    );
    assert_eq!(trace.status(), [0, 1, 0]);
    assert_eq!(trace.substitution_mask().unwrap(), [0, 0b11, 0]);
    assert_eq!(tzon_column(&trace), [290.0, 290.0, 291.0]);

    // max_age below the gap: refused.
    let trace = grid_trace(
        &times,
        &[290.0, 291.0],
        MissingPolicy::HoldLast { max_age: 4.0 },
        DuplicateTime::Refuse,
    );
    assert_eq!(trace.status(), [0, 3, 0]);

    // The latest sample in the interval wins: 7 and 9 fall in (5, 10]; tick 10 takes 9's value.
    let trace = grid_trace(
        &[0.0, 7.0, 9.0],
        &[290.0, 291.0, 292.0],
        MissingPolicy::HoldLast { max_age: 5.0 },
        DuplicateTime::Refuse,
    );
    assert_eq!(
        trace.times(),
        [0.0, 5.0],
        "ticks stop at the last sample (9)"
    );
    let trace = grid_trace(
        &[0.0, 7.0, 10.0],
        &[290.0, 291.0, 292.0],
        MissingPolicy::Refuse,
        DuplicateTime::Refuse,
    );
    assert_eq!(trace.status(), [0, 3, 0]);
    assert_eq!(tzon_column(&trace), [290.0, 0.0, 292.0]);

    // Equal-time samples under Allow: the later row is the latest sample.
    let trace = grid_trace(
        &[0.0, 5.0, 5.0],
        &[290.0, 291.0, 292.0],
        MissingPolicy::Refuse,
        DuplicateTime::Allow,
    );
    assert_eq!(tzon_column(&trace), [290.0, 292.0]);
}

#[test]
fn on_a_grid_a_step_list_is_a_signal_and_never_a_hold() {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let steps = vec![
        (map.column(0).to_owned(), vec![(0.0, Value::Real(295.0))]),
        (
            map.column(1).to_owned(),
            vec![(2.0, Value::Real(290.0)), (12.0, Value::Real(291.0))],
        ),
    ];
    let source =
        StepSource::with_instants(&map, steps, vec![30.0], &ReplayLimits::default()).unwrap();
    let mut options = ReplayOptions::new(TimeBase::Grid { t0: 0.0, step: 5.0 });
    options.on_refusal = oce_replay::OnRefusal::RecordAndContinue;
    options.trace.inputs = true;
    let (_, trace) = Replay::new(engine, map, source, options)
        .unwrap()
        .run()
        .unwrap();
    assert_eq!(trace.times(), [0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0]);
    // Tick 0 precedes TZon's first step: missing. Afterwards it is always current.
    assert_eq!(trace.status(), [3, 0, 0, 0, 0, 0, 0]);
    assert!(trace.substitution_mask().is_none());
    assert_eq!(
        tzon_column(&trace),
        [0.0, 290.0, 290.0, 291.0, 291.0, 291.0, 291.0]
    );
}
