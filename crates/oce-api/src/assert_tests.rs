//! Warning records from complete-frame transitions, including mixed native producers.

use std::sync::Arc;

use oce_blocks::{Block, BlockKind, BlockSignature, Ctx};
use oce_graph::{allocate_state, compile};
use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value, ValueType,
};
use oce_store_mem::MemStore;

use super::{AssertLevel, Engine};

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

    let mut eng = Engine::in_memory();
    eng.model = Arc::new(model);
    eng.schedule = schedule;
    eng.blocks = blocks;
    eng.state = state;
    eng.loaded = true;
    eng
}

#[test]
fn completed_frame_delivers_the_producer_warning_without_output_points() {
    let mut eng = loaded_warning_engine();
    let plan = eng.prepare_frame(3.0, &[]).unwrap();
    let report = eng.execute_frame(plan).unwrap();
    assert_eq!(report.diagnostics().len(), 1);
    let event = &report.diagnostics()[0];
    assert_eq!(event.block, "test.WarningBlock");
    assert_eq!(event.message, "assertion tripped");
    assert_eq!(event.t.to_bits(), 3.0f64.to_bits());
    assert_eq!(event.level, AssertLevel::Warning);
    assert!(report.outputs().is_empty());
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
    let mut events = Vec::new();
    for (t, u) in steps {
        let plan = eng
            .prepare_frame(*t, &[("conn#0", Value::Boolean(*u))])
            .unwrap();
        let report = eng.execute_frame(plan).unwrap();
        assert!(report.outputs().is_empty());
        events.extend(
            report
                .diagnostics()
                .iter()
                .cloned()
                .map(|e| (e.block, e.message, e.t.to_bits(), e.level)),
        );
    }
    events
}

#[test]
fn false_inputs_emit_on_each_frame_and_true_inputs_are_silent() {
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
        let mut events = Vec::new();
        for time in [0.0, 0.0, 1.0] {
            let plan = engine
                .prepare_frame(
                    time,
                    &[
                        ("conn#0", Value::Boolean(false)),
                        ("conn#1", Value::Real(0.0)),
                        ("conn#2", Value::Real(0.0)),
                    ],
                )
                .unwrap();
            let report = engine.execute_frame(plan).unwrap();
            assert_eq!(report.diagnostics().len(), 2);
            assert_eq!(report.diagnostics()[0].message, "false remains advisory");
            assert!(
                report.diagnostics()[1]
                    .message
                    .starts_with("Atan2: inputs u1 and u2")
            );
            events.extend(
                report
                    .diagnostics()
                    .iter()
                    .cloned()
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
