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
