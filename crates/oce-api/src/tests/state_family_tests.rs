//! End-to-end durable continuation across the state-layout families.

use std::cell::Cell;
use std::sync::Arc;

use oce_blocks::{Diagnostics, PortKind};
use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, EnumClassId, ModelGraph, ParamTable,
    Value, ValueType,
};

use crate::{Engine, EngineCheckpoint, EngineStateSnapshot};

#[derive(Default)]
struct WarningCount(Cell<usize>);

impl Diagnostics for WarningCount {
    fn warn(&self, _source: &str, _message: &str, _t: f64) {
        self.0.set(self.0.get() + 1);
    }
}

fn model(class_path: &str, params: ParamTable) -> ModelGraph {
    let block = (oce_blocks::lookup(class_path).unwrap().make)(&params);
    let signature = block.resolved_signature();
    let mut graph = ModelGraph::new();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for (index, kind) in signature.inputs.iter().enumerate() {
        let id = ConnectorId(graph.connectors.len() as u32);
        graph.connectors.push(
            Connector::new(id, BlockId(0), Dir::In, value_type(*kind), index as u32)
                .with_iri(format!("urn:test:{class_path}:input:{index}")),
        );
        graph.external_inputs.push(id);
        inputs.push(id);
    }
    for (index, kind) in signature.outputs.iter().enumerate() {
        let id = ConnectorId(graph.connectors.len() as u32);
        graph.connectors.push(
            Connector::new(id, BlockId(0), Dir::Out, value_type(*kind), index as u32)
                .with_iri(format!("urn:test:{class_path}:output:{index}")),
        );
        outputs.push(id);
    }
    graph.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from(class_path),
        inputs,
        outputs,
        params,
        decl_order: 0,
        instance_iri: Some(Arc::from(format!("urn:test:{class_path}:block"))),
    });
    graph
}

fn value_type(kind: PortKind) -> ValueType {
    match kind {
        PortKind::Real => ValueType::Real,
        PortKind::Integer => ValueType::Integer,
        PortKind::Boolean => ValueType::Boolean,
    }
}

fn params(values: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

fn stage_inputs(engine: &mut Engine, graph: &ModelGraph, boolean: bool) {
    for connector in &graph.connectors {
        if connector.dir != Dir::In {
            continue;
        }
        let value = match connector.value_type {
            ValueType::Real => Value::Real(1.0),
            ValueType::Integer => Value::Integer(1),
            ValueType::Boolean => Value::Boolean(boolean),
            ValueType::String => Value::String(Arc::from("value")),
            ValueType::Enum(class) => Value::Enum { class, ordinal: 1 },
        };
        engine
            .set_input(connector.iri.as_deref().unwrap(), value)
            .unwrap();
    }
}

fn assert_same_state(left: &Engine, right: &Engine, class_path: &str) {
    assert_eq!(left.state.words, right.state.words, "{class_path}");
    assert_eq!(
        left.state.t.to_bits(),
        right.state.t.to_bits(),
        "{class_path}"
    );
    assert_eq!(
        left.prev_t.map(f64::to_bits),
        right.prev_t.map(f64::to_bits),
        "{class_path}"
    );
    assert_eq!(left.state.values.len(), right.state.values.len());
    assert!(
        left.state
            .values
            .iter()
            .zip(&right.state.values)
            .all(|(left, right)| left.bit_eq(right)),
        "{class_path}"
    );
}

fn assert_family_round_trip(class_path: &str, parameters: ParamTable, capture_tick: usize) {
    let graph = model(class_path, parameters);
    let mut uninterrupted = Engine::in_memory();
    uninterrupted
        .build_model_in_memory(graph.clone(), Some("urn:test:family-model"))
        .unwrap();
    for tick in 0..=capture_tick {
        stage_inputs(&mut uninterrupted, &graph, tick % 2 == 1);
        uninterrupted.tick(tick as f64 * 0.1).unwrap();
    }
    if class_path == "CDL.Integers.Stage" {
        assert!(f64::from_bits(uninterrupted.state.words[1]) > uninterrupted.state.t);
    }
    if class_path == "CDL.Reals.MovingAverage" {
        assert_ne!(uninterrupted.state.words[3], 0, "ring did not wrap");
        assert_eq!(uninterrupted.state.words[5], 1, "warn-once bit was not set");
    }
    if class_path == "CDL.Utilities.SunRiseSet" {
        assert!(f64::from_bits(uninterrupted.state.words[0]) > uninterrupted.state.t);
        assert!(f64::from_bits(uninterrupted.state.words[1]) > uninterrupted.state.t);
    }
    let snapshot = uninterrupted.state_snapshot().unwrap();
    let decoded = EngineStateSnapshot::from_bytes(snapshot.as_bytes()).unwrap();
    let mut restored = Engine::in_memory();
    restored
        .build_model_in_memory(graph.clone(), Some("urn:test:family-model"))
        .unwrap();
    restored.restore_state(&decoded).unwrap();
    assert_same_state(&uninterrupted, &restored, class_path);

    let next = (capture_tick + 1) as f64 * 0.1;
    stage_inputs(&mut uninterrupted, &graph, true);
    stage_inputs(&mut restored, &graph, true);
    uninterrupted.tick(next).unwrap();
    restored.tick(next).unwrap();
    assert_same_state(&uninterrupted, &restored, class_path);
}

#[test]
fn integrator_and_pid_state_continue_bit_exactly() {
    assert_family_round_trip("CDL.Reals.IntegratorWithReset", ParamTable::default(), 2);
    assert_family_round_trip(
        "CDL.Reals.PID",
        params(&[(
            "controllerType",
            Value::Enum {
                class: EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 2,
            },
        )]),
        2,
    );
}

#[test]
fn timer_and_hold_state_continue_bit_exactly() {
    assert_family_round_trip("CDL.Logical.Timer", ParamTable::default(), 2);
    assert_family_round_trip(
        "CDL.Logical.TrueDelay",
        params(&[("delayTime", Value::Real(1.0))]),
        2,
    );
}

#[test]
fn edge_and_latch_state_continue_bit_exactly() {
    assert_family_round_trip("CDL.Logical.Edge", ParamTable::default(), 2);
    assert_family_round_trip("CDL.Logical.Latch", ParamTable::default(), 2);
}

#[test]
fn sampled_clock_state_continues_bit_exactly() {
    assert_family_round_trip(
        "CDL.Discrete.UnitDelay",
        params(&[("samplePeriod", Value::Real(0.1))]),
        2,
    );
}

#[test]
fn triggered_moving_mean_state_continues_bit_exactly() {
    assert_family_round_trip(
        "CDL.Discrete.TriggeredMovingMean",
        params(&[("n", Value::Integer(3))]),
        2,
    );
}

#[test]
fn integer_stage_future_deadline_continues_bit_exactly() {
    assert_family_round_trip(
        "CDL.Integers.Stage",
        params(&[
            ("n", Value::Integer(2)),
            ("holdDuration", Value::Real(10.0)),
        ]),
        2,
    );
}

#[test]
fn wrapped_moving_average_state_continues_bit_exactly() {
    assert_family_round_trip(
        "CDL.Reals.MovingAverage",
        params(&[("delta", Value::Real(100.0))]),
        70,
    );
}

#[test]
fn initialized_sun_events_continue_bit_exactly() {
    assert_family_round_trip(
        "CDL.Utilities.SunRiseSet",
        params(&[
            ("lat", Value::Real(0.0)),
            ("lon", Value::Real(0.0)),
            ("timZon", Value::Real(0.0)),
        ]),
        2,
    );
}

#[test]
fn moving_average_warn_once_state_survives_restore() {
    let graph = model(
        "CDL.Reals.MovingAverage",
        params(&[("delta", Value::Real(100.0))]),
    );
    let mut source = Engine::in_memory();
    source.build_model_in_memory(graph.clone(), None).unwrap();
    for tick in 0..=70 {
        stage_inputs(&mut source, &graph, false);
        source.tick(tick as f64 * 0.1).unwrap();
    }
    assert_eq!(source.state.words[5], 1);
    let checkpoint = source.checkpoint().unwrap();

    let mut preserved = Engine::in_memory();
    preserved
        .build_model_in_memory(graph.clone(), None)
        .unwrap();
    preserved.restore_checkpoint(&checkpoint).unwrap();
    let mut cleared_image = (*checkpoint.image).clone();
    cleared_image.words[5] = 0;
    let mut cleared = Engine::in_memory();
    cleared.build_model_in_memory(graph.clone(), None).unwrap();
    cleared
        .restore_checkpoint(&EngineCheckpoint {
            image: Arc::new(cleared_image),
        })
        .unwrap();

    stage_inputs(&mut preserved, &graph, false);
    stage_inputs(&mut cleared, &graph, false);
    let preserved_warnings = WarningCount::default();
    let cleared_warnings = WarningCount::default();
    preserved.tick_with(7.1, &preserved_warnings).unwrap();
    cleared.tick_with(7.1, &cleared_warnings).unwrap();
    assert_eq!(preserved_warnings.0.get(), 0);
    assert_eq!(cleared_warnings.0.get(), 1);
}

#[test]
fn sampled_and_triggered_state_contracts_continue_bit_exactly() {
    for class_path in [
        "CDL.Discrete.Sampler",
        "CDL.Discrete.ZeroOrderHold",
        "CDL.Discrete.FirstOrderHold",
    ] {
        assert_family_round_trip(class_path, params(&[("samplePeriod", Value::Real(0.1))]), 2);
    }
    for class_path in ["CDL.Discrete.TriggeredSampler", "CDL.Discrete.TriggeredMax"] {
        assert_family_round_trip(class_path, ParamTable::default(), 2);
    }
    assert_family_round_trip(
        "CDL.Logical.Sources.SampleTrigger",
        params(&[("period", Value::Real(0.1))]),
        2,
    );
}

#[test]
fn integer_edge_state_contracts_continue_bit_exactly() {
    for class_path in ["CDL.Integers.Change", "CDL.Integers.OnCounter"] {
        assert_family_round_trip(class_path, ParamTable::default(), 2);
    }
}

#[test]
fn logical_timing_latch_and_proof_state_contracts_continue_bit_exactly() {
    for class_path in [
        "CDL.Logical.Pre",
        "CDL.Logical.FallingEdge",
        "CDL.Logical.Change",
        "CDL.Logical.Toggle",
        "CDL.Logical.TimerAccumulating",
        "CDL.Logical.TrueFalseHold",
        "CDL.Logical.TrueHoldWithReset",
        "CDL.Logical.VariablePulse",
        "CDL.Logical.Proof",
    ] {
        let parameters = match class_path {
            "CDL.Logical.TrueFalseHold" | "CDL.Logical.TrueHoldWithReset" => {
                params(&[("trueHoldDuration", Value::Real(1.0))])
            }
            "CDL.Logical.VariablePulse" => params(&[("period", Value::Real(1.0))]),
            "CDL.Logical.Proof" => params(&[
                ("debounce", Value::Real(1.0)),
                ("feedbackDelay", Value::Real(1.0)),
            ]),
            _ => ParamTable::default(),
        };
        assert_family_round_trip(class_path, parameters, 2);
    }
}

#[test]
fn real_filter_comparator_and_controller_state_contracts_continue_bit_exactly() {
    for class_path in [
        "CDL.Reals.Greater",
        "CDL.Reals.GreaterThreshold",
        "CDL.Reals.Less",
        "CDL.Reals.LessThreshold",
    ] {
        assert_family_round_trip(class_path, params(&[("h", Value::Real(0.1))]), 2);
    }
    assert_family_round_trip(
        "CDL.Reals.Hysteresis",
        params(&[("uHigh", Value::Real(1.0)), ("uLow", Value::Real(0.0))]),
        2,
    );
    for class_path in [
        "CDL.Reals.Derivative",
        "CDL.Reals.LimitSlewRate",
        "CDL.Reals.Ramp",
    ] {
        let parameters = match class_path {
            "CDL.Reals.LimitSlewRate" | "CDL.Reals.Ramp" => {
                params(&[("raisingSlewRate", Value::Real(1.0))])
            }
            _ => ParamTable::default(),
        };
        assert_family_round_trip(class_path, parameters, 2);
    }
    assert_family_round_trip(
        "CDL.Reals.PIDWithReset",
        params(&[(
            "controllerType",
            Value::Enum {
                class: EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 2,
            },
        )]),
        2,
    );
}
