//! Every limit is accepted at the limit and refused one past it with its typed error, and a
//! request above a documented maximum is refused (design note, test plan: bounds; decisions 8
//! and 19).
//!
//! Each cap is exercised at a lowered value (a caller may lower any cap), and the cheap ones at
//! their documented defaults too: 4,096 CSV columns and 256-byte cells.

#[path = "support/fixtures.rs"]
mod fixtures;

use fixtures::{Frames, columnar, load, path_map};
use oce_api::Value;
use oce_replay::{
    Column, ColumnarSource, CsvFormat, CsvSource, LimitError, MissingPolicy, Progress, Replay,
    ReplayError, ReplayLimits, ReplayOptions, SourceError, StepSource, TimeBase, TimeColumn,
};

const FIXTURE: &str = "generic_time_suppression";

fn frames(n: usize) -> Frames {
    (0..n)
        .map(|k| (k as f64, vec![Value::Real(295.0), Value::Real(293.0)]))
        .collect()
}

#[test]
fn the_documented_maxima_are_the_defaults() {
    let max = ReplayLimits::default();
    assert_eq!(max, ReplayLimits::MAX);
    assert_eq!(max.max_rows, 16_777_216);
    assert_eq!(max.max_frames, 16_777_216);
    assert_eq!(max.max_columns, 4_096);
    assert_eq!(max.max_cell_bytes, 256);
    assert_eq!(max.max_source_bytes, 1 << 30);
    assert_eq!(max.max_steps, 1_000_000);
    assert_eq!(max.max_trace_bytes, 1 << 30);
    assert!(max.validate().is_ok());
    let zero = ReplayLimits {
        max_rows: 0,
        max_columns: 0,
        max_cell_bytes: 0,
        max_source_bytes: 0,
        max_steps: 0,
        max_frames: 0,
        max_trace_bytes: 0,
    };
    assert!(zero.validate().is_ok(), "lowering is always allowed");
}

#[test]
fn raising_any_limit_above_its_maximum_is_refused_everywhere() {
    let raised: [(&str, ReplayLimits); 7] = [
        (
            "max_rows",
            ReplayLimits {
                max_rows: (1 << 24) + 1,
                ..ReplayLimits::MAX
            },
        ),
        (
            "max_columns",
            ReplayLimits {
                max_columns: 4_097,
                ..ReplayLimits::MAX
            },
        ),
        (
            "max_cell_bytes",
            ReplayLimits {
                max_cell_bytes: 257,
                ..ReplayLimits::MAX
            },
        ),
        (
            "max_source_bytes",
            ReplayLimits {
                max_source_bytes: (1 << 30) + 1,
                ..ReplayLimits::MAX
            },
        ),
        (
            "max_steps",
            ReplayLimits {
                max_steps: 1_000_001,
                ..ReplayLimits::MAX
            },
        ),
        (
            "max_frames",
            ReplayLimits {
                max_frames: (1 << 24) + 1,
                ..ReplayLimits::MAX
            },
        ),
        (
            "max_trace_bytes",
            ReplayLimits {
                max_trace_bytes: (1 << 30) + 1,
                ..ReplayLimits::MAX
            },
        ),
    ];
    for (name, limits) in raised {
        let expected =
            |e: &LimitError| matches!(e, LimitError::AboveMaximum { limit, .. } if *limit == name);
        assert!(limits.validate().as_ref().is_err_and(expected), "{name}");
        let engine = load(FIXTURE);
        let map = path_map(&engine, &MissingPolicy::Refuse);
        let time = TimeColumn::Seconds(vec![0.0]);
        let cols = || {
            (0..2)
                .map(|s| (map.column(s).to_owned(), Column::Real(vec![1.0])))
                .collect()
        };
        assert!(
            matches!(ColumnarSource::new(&map, time.clone(), cols(), &limits), Err(SourceError::Limit(ref e)) if expected(e))
        );
        assert!(
            matches!(StepSource::new(&map, Vec::new(), &limits), Err(SourceError::Limit(ref e)) if expected(e))
        );
        assert!(
            matches!(CsvSource::new(&map, &b"t\n"[..], CsvFormat::seconds("t"), &limits), Err(SourceError::Limit(ref e)) if expected(e))
        );
        let source = ColumnarSource::new(&map, time, cols(), &ReplayLimits::default()).unwrap();
        let mut options = ReplayOptions::new(TimeBase::Recorded);
        options.limits = limits;
        assert!(
            matches!(Replay::new(engine, map, source, options), Err(ReplayError::Limit(ref e)) if expected(e)),
            "{name}"
        );
    }
}

fn csv_text(header: &str, rows: usize) -> String {
    let mut text = format!("{header}\n");
    for k in 0..rows {
        text.push_str(&format!("{k},295,293\n"));
    }
    text
}

fn csv(text: &str, limits: ReplayLimits) -> Result<Vec<f64>, SourceError> {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let header_names = format!("t,{},{}", map.column(0), map.column(1));
    let text = text.replacen("HEADER", &header_names, 1);
    let source = CsvSource::new(&map, text.as_bytes(), CsvFormat::seconds("t"), &limits)?;
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.limits = ReplayLimits {
        max_source_bytes: ReplayLimits::MAX.max_source_bytes,
        ..limits
    };
    let mut replay = Replay::new(engine, map, source, options).unwrap();
    loop {
        match replay.advance(1_000) {
            Ok(Progress::More { .. }) => {}
            Ok(_) => return Ok(replay.trace().times().to_vec()),
            Err(ReplayError::Source(e)) => return Err(e),
            Err(other) => panic!("{other}"),
        }
    }
}

#[test]
fn csv_rows_are_accepted_at_the_cap_and_refused_one_past() {
    let limits = ReplayLimits {
        max_rows: 5,
        ..ReplayLimits::MAX
    };
    assert_eq!(csv(&csv_text("HEADER", 5), limits).unwrap().len(), 5);
    assert_eq!(
        csv(&csv_text("HEADER", 6), limits),
        Err(SourceError::TooManyRows { limit: 5 })
    );
}

#[test]
fn csv_bytes_are_accepted_at_the_cap_and_refused_one_past() {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let text = csv_text("HEADER", 40).replacen(
        "HEADER",
        &format!("t,{},{}", map.column(0), map.column(1)),
        1,
    );
    let size = text.len() as u64;
    let at = ReplayLimits {
        max_source_bytes: size,
        ..ReplayLimits::MAX
    };
    assert_eq!(csv(&text, at).unwrap().len(), 40);
    let below = ReplayLimits {
        max_source_bytes: size - 1,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        csv(&text, below),
        Err(SourceError::TooManyBytes { limit: size - 1 })
    );
    // A cap below the header is refused at construction.
    let tiny = ReplayLimits {
        max_source_bytes: 4,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        csv(&text, tiny),
        Err(SourceError::TooManyBytes { limit: 4 })
    );
}

#[test]
fn csv_columns_are_accepted_at_the_default_cap_and_refused_one_past() {
    for (extra, ok) in [(4_096 - 3, true), (4_096 - 2, false)] {
        let mut header = "HEADER".to_owned();
        let mut row = "0,295,293".to_owned();
        for i in 0..extra {
            header.push_str(&format!(",x{i}"));
            row.push_str(",0");
        }
        let text = format!("{header}\n{row}\n");
        let result = csv(&text, ReplayLimits::MAX);
        if ok {
            assert_eq!(result.unwrap().len(), 1);
        } else {
            assert_eq!(result, Err(SourceError::TooManyColumns { limit: 4_096 }));
        }
    }
    let lowered = ReplayLimits {
        max_columns: 3,
        ..ReplayLimits::MAX
    };
    assert!(csv(&csv_text("HEADER", 1), lowered).is_ok());
    let lowered = ReplayLimits {
        max_columns: 2,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        csv(&csv_text("HEADER", 1), lowered),
        Err(SourceError::TooManyColumns { limit: 2 })
    );
}

#[test]
fn csv_cells_are_accepted_at_the_default_cap_and_refused_one_past() {
    let cell = |n: usize| format!("{}1", "0".repeat(n - 1));
    let text = format!("HEADER,note\n0,{},293,x\n", cell(256));
    assert_eq!(csv(&text, ReplayLimits::MAX).unwrap().len(), 1);
    let text = format!("HEADER,note\n0,{},293,x\n", cell(257));
    assert_eq!(
        csv(&text, ReplayLimits::MAX),
        Err(SourceError::CellTooLong {
            row: 1,
            field: 1,
            limit: 256
        })
    );
    // An ignored column's cell and a header cell are capped too.
    let text = format!("HEADER,note\n0,295,293,{}\n", "y".repeat(257));
    assert_eq!(
        csv(&text, ReplayLimits::MAX),
        Err(SourceError::CellTooLong {
            row: 1,
            field: 3,
            limit: 256
        })
    );
    let text = format!("HEADER,{}\n0,295,293,x\n", "n".repeat(257));
    assert_eq!(
        csv(&text, ReplayLimits::MAX),
        Err(SourceError::CellTooLong {
            row: 0,
            field: 3,
            limit: 256
        })
    );
}

#[test]
fn columnar_rows_columns_and_bytes_are_accepted_at_the_cap_and_refused_one_past() {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let make = |rows: usize, limits: ReplayLimits| {
        let time = TimeColumn::Seconds((0..rows).map(|k| k as f64).collect());
        let columns = (0..2)
            .map(|s| (map.column(s).to_owned(), Column::Real(vec![1.0; rows])))
            .collect();
        ColumnarSource::new(&map, time, columns, &limits).map(|s| s.rows())
    };
    let rows = ReplayLimits {
        max_rows: 10,
        ..ReplayLimits::MAX
    };
    assert_eq!(make(10, rows), Ok(10));
    assert_eq!(make(11, rows), Err(SourceError::TooManyRows { limit: 10 }));
    let columns = ReplayLimits {
        max_columns: 3,
        ..ReplayLimits::MAX
    };
    assert_eq!(make(1, columns), Ok(1));
    let columns = ReplayLimits {
        max_columns: 2,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        make(1, columns),
        Err(SourceError::TooManyColumns { limit: 2 })
    );
    // 10 rows × (8 time + 8 + 8) = 240 bytes.
    let bytes = ReplayLimits {
        max_source_bytes: 240,
        ..ReplayLimits::MAX
    };
    assert_eq!(make(10, bytes), Ok(10));
    let bytes = ReplayLimits {
        max_source_bytes: 239,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        make(10, bytes),
        Err(SourceError::TooManyBytes { limit: 239 })
    );
}

#[test]
fn step_lists_rows_and_columns_are_accepted_at_the_cap_and_refused_one_past() {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let make = |steps: usize, offset: f64, limits: ReplayLimits| {
        let lists = (0..2)
            .map(|s| {
                let list = (0..steps)
                    .map(|k| (k as f64 + offset * s as f64, Value::Real(1.0)))
                    .collect();
                (map.column(s).to_owned(), list)
            })
            .collect();
        StepSource::new(&map, lists, &limits).map(|s| s.rows())
    };
    let steps = ReplayLimits {
        max_steps: 4,
        ..ReplayLimits::MAX
    };
    assert_eq!(make(4, 0.0, steps), Ok(4));
    assert!(matches!(
        make(5, 0.0, steps),
        Err(SourceError::TooManySteps { limit: 4, .. })
    ));
    // Two lists of 4 offset by half a second: a union of 8 rows.
    let rows = ReplayLimits {
        max_rows: 8,
        ..ReplayLimits::MAX
    };
    assert_eq!(make(4, 0.5, rows), Ok(8));
    let rows = ReplayLimits {
        max_rows: 7,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        make(4, 0.5, rows),
        Err(SourceError::TooManyRows { limit: 7 })
    );
    let columns = ReplayLimits {
        max_columns: 2,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        make(1, 0.0, columns),
        Err(SourceError::TooManyColumns { limit: 2 })
    );
}

fn run_frames(n: usize, limits: ReplayLimits, time: TimeBase) -> Result<Progress, ReplayError> {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let source = columnar(&map, &frames(n));
    let mut options = ReplayOptions::new(time);
    options.limits = limits;
    // A grid tick between samples is refused (no hold); keep going so the frame count decides.
    options.on_refusal = oce_replay::OnRefusal::RecordAndContinue;
    let mut replay = Replay::new(engine, map, source, options).unwrap();
    loop {
        match replay.advance(3)? {
            Progress::More { .. } => {}
            other => return Ok(other),
        }
    }
}

#[test]
fn frames_are_accepted_at_the_cap_and_refused_one_past_on_either_time_base() {
    let limits = ReplayLimits {
        max_frames: 7,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        run_frames(7, limits, TimeBase::Recorded).unwrap(),
        Progress::Done { frames: 7 }
    );
    assert!(matches!(
        run_frames(8, limits, TimeBase::Recorded),
        Err(ReplayError::TooManyFrames { limit: 7 })
    ));
    // A grid at 1 s over samples at 0..=6 has 7 ticks; at 0.5 s it has 13.
    let grid = TimeBase::Grid { t0: 0.0, step: 1.0 };
    assert_eq!(
        run_frames(7, limits, grid).unwrap(),
        Progress::Done { frames: 7 }
    );
    let grid = TimeBase::Grid { t0: 0.0, step: 0.5 };
    assert!(matches!(
        run_frames(7, limits, grid),
        Err(ReplayError::TooManyFrames { limit: 7 })
    ));
}

#[test]
fn trace_bytes_are_accepted_at_the_cap_and_refused_one_past() {
    // generic_time_suppression: one Boolean output, no warnings: 8 + 1 + 1 = 10 bytes per frame.
    let at = ReplayLimits {
        max_trace_bytes: 100,
        ..ReplayLimits::MAX
    };
    assert_eq!(
        run_frames(10, at, TimeBase::Recorded).unwrap(),
        Progress::Done { frames: 10 }
    );
    let below = ReplayLimits {
        max_trace_bytes: 99,
        ..ReplayLimits::MAX
    };
    assert!(matches!(
        run_frames(10, below, TimeBase::Recorded),
        Err(ReplayError::TraceTooLarge { limit: 99 })
    ));
}

#[test]
fn snapshots_count_toward_the_trace_byte_cap() {
    let run = |cap: u64| {
        let engine = load(FIXTURE);
        let map = path_map(&engine, &MissingPolicy::Refuse);
        let source = columnar(&map, &frames(2));
        let mut options = ReplayOptions::new(TimeBase::Recorded);
        options.trace.snapshot_at = vec![0];
        options.limits = ReplayLimits {
            max_trace_bytes: cap,
            ..ReplayLimits::MAX
        };
        Replay::new(engine, map, source, options)
            .unwrap()
            .run()
            .map(|(_, t)| t)
    };
    let trace = run(ReplayLimits::MAX.max_trace_bytes).unwrap();
    let snapshot = trace.snapshots()[0].1.as_bytes().len() as u64;
    let total = 2 * 10 + 16 + snapshot;
    assert_eq!(trace.byte_size(), total);
    assert_eq!(run(total).unwrap().byte_size(), total);
    assert!(matches!(
        run(total - 1),
        Err(ReplayError::TraceTooLarge { .. })
    ));
}
