use std::sync::Arc;

use oce_blocks::{Block, BlockKind, BlockSignature, Ctx};
use oce_graph::{allocate_state, compile};
use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value, ValueType,
};
use oce_store_mem::MemStore;

use super::{AssertLevel, CollectSpec, Engine, InputSource, Outputs, SimSpec};

struct WarningBlock;

impl Block for WarningBlock {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "test.WarningBlock",
            inputs: &[],
            outputs: &[],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(
        &self,
        ctx: &Ctx<'_>,
        _inputs: &[Value],
        _emit: &mut dyn FnMut(usize, Value),
    ) {
        ctx.warn("test.WarningBlock", "assertion tripped");
    }
}

fn loaded_warning_engine() -> Engine<MemStore> {
    let mut model = ModelGraph::new();
    model.blocks.push(BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from("test.WarningBlock"),
        inputs: Vec::new(),
        outputs: Vec::new(),
        params: ParamTable::default(),
        decl_order: 0,
        instance_iri: None,
    });
    let blocks: Vec<Box<dyn Block>> = vec![Box::new(WarningBlock)];
    let schedule = compile(&model, &blocks).expect("warning source schedule");
    let state = allocate_state(&model, &blocks);
    let outputs = Outputs::build(&model, &state);

    let mut eng = Engine::in_memory();
    eng.model = Arc::new(model);
    eng.schedule = schedule;
    eng.blocks = blocks;
    eng.state = state;
    eng.outputs = outputs;
    eng
}

fn sim_spec(t_start: f64, t_stop: f64, step: f64, collect: CollectSpec) -> SimSpec {
    SimSpec {
        t_start,
        t_stop,
        step,
        inputs: InputSource::None,
        collect,
    }
}

#[test]
fn step_realtime_delivers_assert_diagnostics_and_simulate_drops_them() {
    let mut eng = loaded_warning_engine();
    // Verification-only model time is anchored explicitly at the UNIX epoch.
    eng.set_realtime_epoch_unix_nanos(0);
    let report = eng.step_realtime(3.0).unwrap();
    assert_eq!(report.asserts.len(), 1);
    let event = &report.asserts[0];
    assert_eq!(event.block, "test.WarningBlock");
    assert_eq!(event.message, "assertion tripped");
    assert_eq!(event.t.to_bits(), 3.0f64.to_bits());
    assert_eq!(event.level, AssertLevel::Warning);
    assert_eq!(report.written, 0);

    let mut sim_engine = loaded_warning_engine();
    let metrics = sim_engine
        .simulate(&sim_spec(3.0, 3.0, 1.0, CollectSpec::None))
        .expect("simulate keeps warning sink no-op");
    assert_eq!(metrics.ticks, 1);
    assert!(
        metrics.trace.rows() == 0,
        "simulate remains timing/trace-only and exposes no assert stream"
    );
}

fn assert_model(message: &str) -> ModelGraph {
    let mut model = ModelGraph::new();
    let block = BlockId(0);
    let input = ConnectorId(0);
    model
        .connectors
        .push(Connector::new(input, block, Dir::In, ValueType::Boolean, 0));
    model.external_inputs.push(input);
    model.blocks.push(BlockInstance {
        id: block,
        class_iri: Arc::from("CDL.Utilities.Assert"),
        inputs: vec![input],
        outputs: Vec::new(),
        params: ParamTable {
            values: vec![(Arc::from("message"), Value::String(Arc::from(message)))],
        },
        decl_order: 0,
        instance_iri: None,
    });
    model
}

fn loaded_assert_engine(message: &str) -> Engine<MemStore> {
    let mut eng = Engine::in_memory();
    eng.build_model_in_memory(assert_model(message), None)
        .expect("zero-output Assert model builds");
    eng
}

fn assert_trace(steps: &[(f64, bool)]) -> Vec<(String, String, u64, AssertLevel)> {
    let mut eng = loaded_assert_engine("freezestat tripped");
    // Determinism harness: choose the UNIX epoch explicitly; no production clock is implied.
    eng.set_realtime_epoch_unix_nanos(0);
    assert!(
        eng.outputs().is_empty(),
        "Assert declares no output connectors"
    );
    let mut events = Vec::new();
    for (t, u) in steps {
        eng.set_input("conn#0", Value::Boolean(*u))
            .expect("boundary input is stageable");
        let report = eng.step_realtime(*t).expect("assert tick succeeds");
        assert_eq!(report.written, 0);
        events.extend(
            report
                .asserts
                .into_iter()
                .map(|e| (e.block, e.message, e.t.to_bits(), e.level)),
        );
    }
    events
}

#[test]
fn utilities_assert_delivers_warning_events_through_step_realtime() {
    let events = assert_trace(&[
        (0.0, true),
        (1.0, false),
        (2.0, false),
        (3.0, true),
        (4.0, false),
    ]);
    assert_eq!(
        events,
        vec![
            (
                "CDL.Utilities.Assert".to_string(),
                "freezestat tripped".to_string(),
                1.0f64.to_bits(),
                AssertLevel::Warning,
            ),
            (
                "CDL.Utilities.Assert".to_string(),
                "freezestat tripped".to_string(),
                2.0f64.to_bits(),
                AssertLevel::Warning,
            ),
            (
                "CDL.Utilities.Assert".to_string(),
                "freezestat tripped".to_string(),
                4.0f64.to_bits(),
                AssertLevel::Warning,
            ),
        ],
        "Assert.mo uses a stateless warning assert, so false input emits every evaluation"
    );
}

#[test]
fn utilities_assert_warns_on_first_tick_false_and_is_deterministic() {
    let first_tick = assert_trace(&[(0.0, false)]);
    assert_eq!(
        first_tick,
        vec![(
            "CDL.Utilities.Assert".to_string(),
            "freezestat tripped".to_string(),
            0.0f64.to_bits(),
            AssertLevel::Warning,
        )]
    );

    let inputs = [(0.0, false), (0.25, true), (0.5, false), (1.0, false)];
    assert_eq!(assert_trace(&inputs), assert_trace(&inputs));
}

#[test]
fn mixed_native_block_and_assert_warnings_repeat_without_escalation() {
    fn run() -> Vec<(String, u64, AssertLevel)> {
        let mut model = assert_model("false remains advisory");
        let block = BlockId(1);
        for (id, direction) in [(1, Dir::In), (2, Dir::In), (3, Dir::Out)] {
            model.connectors.push(Connector::new(
                ConnectorId(id),
                block,
                direction,
                ValueType::Real,
                if id == 2 { 1 } else { 0 },
            ));
        }
        model
            .external_inputs
            .extend([ConnectorId(1), ConnectorId(2)]);
        model.blocks.push(BlockInstance {
            id: block,
            class_iri: Arc::from("CDL.Reals.Atan2"),
            inputs: vec![ConnectorId(1), ConnectorId(2)],
            outputs: vec![ConnectorId(3)],
            params: ParamTable::default(),
            decl_order: 1,
            instance_iri: None,
        });
        let mut engine = Engine::in_memory();
        engine.build_model_in_memory(model, None).unwrap();
        engine.set_realtime_epoch_unix_nanos(0);
        let mut events = Vec::new();
        for time in [0.0, 0.0, 1.0] {
            engine.set_input("conn#0", Value::Boolean(false)).unwrap();
            engine.set_input("conn#1", Value::Real(0.0)).unwrap();
            engine.set_input("conn#2", Value::Real(0.0)).unwrap();
            let report = engine.step_realtime(time).unwrap();
            assert_eq!(report.written, 1);
            assert_eq!(report.asserts.len(), 2);
            assert_eq!(report.asserts[0].message, "false remains advisory");
            assert!(
                report.asserts[1]
                    .message
                    .starts_with("Atan2: inputs u1 and u2")
            );
            events.extend(
                report
                    .asserts
                    .into_iter()
                    .map(|event| (event.block, event.t.to_bits(), event.level)),
            );
        }
        assert!(
            engine
                .get_output("conn#3")
                .unwrap()
                .bit_eq(&Value::Real(0.0))
        );
        events
    }
    let expected: Vec<_> = [0.0_f64, 0.0, 1.0]
        .into_iter()
        .flat_map(|time| {
            ["CDL.Utilities.Assert", "CDL.Reals.Atan2"]
                .map(|class| (class.to_owned(), time.to_bits(), AssertLevel::Warning))
        })
        .collect();
    assert_eq!(run(), expected);
    assert_eq!(run(), expected);
}
