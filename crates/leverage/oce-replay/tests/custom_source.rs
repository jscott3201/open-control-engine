//! A caller's own `FrameSource` gets the same rules as the three adapters: NaN is missing whatever
//! the source lends (decisions 2 and 9), a wrongly typed cell reaches the engine and is refused
//! there as `InputType`, a row of the wrong width is a typed `RowWidth` error, and a boxed or
//! borrowed source drives a run.

#[path = "support/fixtures.rs"]
mod fixtures;

use fixtures::{load, path_map};
use oce_api::{NonFiniteInputPolicy, Value};
use oce_replay::{
    EngineRefusal, FrameSource, MapId, MissingPolicy, OnRefusal, RefusalCause, Replay, ReplayError,
    ReplayOptions, Row, SourceError, TimeBase,
};

const FIXTURE: &str = "generic_time_suppression";

/// Rows given in full, lent one at a time.
struct Scripted {
    map: MapId,
    rows: Vec<(f64, Vec<Option<Value>>)>,
    at: usize,
}

impl FrameSource for Scripted {
    fn map_id(&self) -> &MapId {
        &self.map
    }

    fn next_row(&mut self) -> Result<Option<Row<'_>>, SourceError> {
        let Some((t, cells)) = self.rows.get(self.at) else {
            return Ok(None);
        };
        self.at += 1;
        Ok(Some((*t, cells.as_slice())))
    }
}

fn run(
    rows: Vec<(f64, Vec<Option<Value>>)>,
    non_finite: NonFiniteInputPolicy,
) -> Result<oce_replay::Trace, ReplayError> {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let source: Box<dyn FrameSource> = Box::new(Scripted {
        map: map.id().clone(),
        rows,
        at: 0,
    });
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.on_refusal = OnRefusal::RecordAndContinue;
    options.non_finite = non_finite;
    Replay::new(engine, map, source, options)?
        .run()
        .map(|(_, t)| t)
}

fn real(x: f64) -> Option<Value> {
    Some(Value::Real(x))
}

#[test]
fn a_custom_sources_nan_is_missing_under_both_policies() {
    for non_finite in [NonFiniteInputPolicy::Admit, NonFiniteInputPolicy::Refuse] {
        let trace = run(
            vec![
                (0.0, vec![real(295.0), real(293.0)]),
                (1.0, vec![real(295.0), real(f64::NAN)]),
                (2.0, vec![real(-f64::NAN), real(293.0)]),
            ],
            non_finite,
        )
        .unwrap();
        assert_eq!(trace.status(), [0, 3, 3], "{non_finite:?}");
        assert!(
            trace
                .refusals()
                .iter()
                .all(|r| matches!(r.cause, RefusalCause::MissingInput { .. })),
            "NaN never reaches the engine"
        );
    }
}

#[test]
fn a_wrongly_typed_cell_is_refused_by_the_engine_as_input_type() {
    let trace = run(
        vec![
            (0.0, vec![real(295.0), Some(Value::Integer(293))]),
            (1.0, vec![real(295.0), real(293.0)]),
        ],
        NonFiniteInputPolicy::Refuse,
    )
    .unwrap();
    assert_eq!(trace.status(), [4, 0]);
    assert!(matches!(
        trace.refusals()[0].cause,
        RefusalCause::Engine {
            refusal: EngineRefusal::InputType,
            ..
        }
    ));
}

#[test]
fn a_row_of_the_wrong_width_is_a_typed_error() {
    let error = run(
        vec![
            (0.0, vec![real(295.0), real(293.0)]),
            (1.0, vec![real(295.0)]),
        ],
        NonFiniteInputPolicy::Refuse,
    )
    .unwrap_err();
    assert!(
        matches!(error, ReplayError::Source(SourceError::RowWidth { row: 2 })),
        "{error}"
    );
}

#[test]
fn a_borrowed_source_drives_a_run_and_stays_with_its_owner() {
    let engine = load(FIXTURE);
    let map = path_map(&engine, &MissingPolicy::Refuse);
    let mut owned = Scripted {
        map: map.id().clone(),
        rows: vec![(0.0, vec![real(295.0), real(293.0)])],
        at: 0,
    };
    let (_, trace) = Replay::new(
        engine,
        map,
        &mut owned,
        ReplayOptions::new(TimeBase::Recorded),
    )
    .unwrap()
    .run()
    .unwrap();
    assert_eq!(trace.status(), [0]);
    assert_eq!(owned.at, 1);
}
