use std::sync::Arc;

use oce_blocks::{Block, BlockKind, BlockSignature, Ctx};
use oce_graph::{allocate_state, compile};
use oce_model::{BlockId, BlockInstance, ModelGraph, ParamTable, Value};
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
    let report = eng.step_realtime(3.0).unwrap();
    assert_eq!(report.asserts.len(), 1);
    let event = &report.asserts[0];
    assert_eq!(event.block, "test.WarningBlock");
    assert_eq!(event.message, "assertion tripped");
    assert_eq!(event.t.to_bits(), 3.0f64.to_bits());
    assert_eq!(event.level, AssertLevel::Error);
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
