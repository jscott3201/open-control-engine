//! Engine refusals about the run, not the frame, stop the run whatever `OnRefusal` says; and the
//! classification of facade errors.

use oce_api::{Engine, OcError, Value};

use super::*;
use crate::{ColumnKind, ColumnMap, ColumnSpec, ColumnarSource, MissingPolicy, TimeColumn};

fn engine() -> Engine {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oce-cxf/tests/fixtures/g36/generic_time_suppression.jsonld"
    );
    let mut engine = Engine::new();
    engine.load_cxf(&std::fs::read(path).unwrap()).unwrap();
    engine
}

#[test]
fn a_run_refusal_stops_the_run_even_under_record_and_continue() {
    let engine = engine();
    let definitions = engine.input_definitions().unwrap();
    let specs: Vec<ColumnSpec> = definitions
        .iter()
        .map(|d| ColumnSpec::new(d.path.clone(), ColumnKind::Real))
        .collect();
    let map = InputMap::resolve(
        &definitions,
        &specs,
        &ColumnMap::by_name(&specs),
        &MissingPolicy::Refuse,
        &[],
    )
    .unwrap();
    let columns = specs
        .iter()
        .map(|s| (s.name.clone(), crate::Column::Real(vec![293.0; 4])))
        .collect();
    let time = TimeColumn::Seconds(vec![0.0, 1.0, 2.0, 3.0]);
    let source = ColumnarSource::new(&map, time, columns, &ReplayLimits::default()).unwrap();
    let mut options = ReplayOptions::new(TimeBase::Recorded);
    options.on_refusal = OnRefusal::RecordAndContinue;
    let mut replay = Replay::new(engine, map, source, options).unwrap();
    assert_eq!(replay.advance(1).unwrap(), Progress::More { done: 1 });
    // A pending parameter edit makes every later frame refuse with a state error.
    replay.engine.halt().unwrap();
    let (path, value, _) = replay.engine.params().iter().next().unwrap();
    replay.engine.set_param(&path, value).unwrap();
    let progress = replay.advance(10).unwrap();
    let Progress::Stopped { frame: 1, cause } = progress else {
        panic!("{progress:?}");
    };
    assert!(matches!(
        cause,
        RefusalCause::Engine {
            refusal: EngineRefusal::Run,
            ..
        }
    ));
    assert_eq!(replay.trace().status(), [0, 4]);
    assert_eq!(
        replay.advance(10).unwrap(),
        Progress::Stopped { frame: 1, cause }
    );
}

#[test]
fn facade_errors_classify_into_frame_and_run_refusals() {
    let frame = [
        (OcError::InputType("p".into()), EngineRefusal::InputType),
        (
            OcError::InputNonFinite("p".into()),
            EngineRefusal::InputNonFinite,
        ),
        (OcError::InputDomain("p".into()), EngineRefusal::InputDomain),
    ];
    for (error, refusal) in frame {
        assert_eq!(EngineRefusal::of(&error), refusal);
    }
    for error in [
        OcError::FrameSequenceExhausted,
        OcError::StalePreparedFrame,
        OcError::FrameMissingInput("p".into()),
    ] {
        assert_eq!(EngineRefusal::of(&error), EngineRefusal::Run, "{error}");
    }
    let _ = Value::Real(0.0);
}
