//! `Engine::resume` re-instantiates the blocks under the model-wide constructed budget.
//!
//! No parameter a host may edit at rest sizes a block's buffers today, so a public edit cannot
//! take a loaded model past `oce_graph::MAX_CONSTRUCTED_WORDS`. The budget on the resume path is
//! therefore pinned from inside the crate: the test grows the tables of a halted engine's model
//! directly, as a future editable table would, and requires resume to refuse as the load does.

use std::sync::Arc;

use oce_graph::{BuildError, MAX_CONSTRUCTED_WORDS};
use oce_model::{
    BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value, ValueType,
};

use crate::{Engine, OcError, RunMode};

/// The cell whose name alone makes a `TimeTable` hold 524,288 rows of two columns.
const WIDE_CELL: &str = "table_524288_2";
/// What one such block reports: its 1,048,576 cells and two words of per-column buffers.
const WIDE_TABLE_WORDS: u64 = 1_048_578;

/// `count` `CDL.Reals.Sources.TimeTable` blocks, each with a two-row table and one output.
fn time_tables(count: u32) -> ModelGraph {
    let mut model = ModelGraph::new();
    for index in 0..count {
        let cell = |name: &str, value: f64| (Arc::from(name), Value::Real(value));
        model.blocks.push(BlockInstance {
            id: BlockId(index),
            class_iri: Arc::from("CDL.Reals.Sources.TimeTable"),
            inputs: Vec::new(),
            outputs: vec![ConnectorId(index)],
            params: ParamTable {
                values: vec![
                    cell("table_1_1", 0.0),
                    cell("table_1_2", 1.0),
                    cell("table_2_1", 10.0),
                    cell("table_2_2", 2.0),
                ],
            },
            decl_order: index,
            instance_iri: Some(Arc::from(format!("urn:test:table{index}"))),
        });
        model.connectors.push(
            Connector::new(
                ConnectorId(index),
                BlockId(index),
                Dir::Out,
                ValueType::Real,
                0,
            )
            .with_iri(format!("urn:test:table{index}.y")),
        );
    }
    model
}

/// Add or remove the wide cell on every block of the halted engine's model, as an edit at rest.
fn set_wide_cell(engine: &mut Engine, present: bool) {
    let model = Arc::make_mut(&mut engine.model);
    for block in &mut model.blocks {
        block.params.values.retain(|(name, _)| &**name != WIDE_CELL);
        if present {
            block
                .params
                .values
                .push((Arc::from(WIDE_CELL), Value::Real(0.0)));
        }
    }
    engine.params_dirty = true;
}

#[test]
fn resume_refuses_blocks_past_the_constructed_budget_and_stays_halted_until_corrected() {
    let mut engine = Engine::new();
    engine
        .build_model_in_memory(time_tables(5), Some("urn:test:tables"))
        .expect("five small tables load");
    engine.halt().unwrap();

    // Five tables of 1,048,578 words: the fourth takes the total past the budget.
    set_wide_cell(&mut engine, true);
    let refusal = engine.resume().expect_err("past the constructed budget");
    match refusal {
        OcError::Build(BuildError::ConstructedTooLarge {
            block,
            requested,
            limit,
        }) => {
            assert_eq!(block.0, "urn:test:table3");
            assert_eq!(requested, 4 * WIDE_TABLE_WORDS);
            assert_eq!(limit, MAX_CONSTRUCTED_WORDS);
        }
        other => panic!("expected ConstructedTooLarge, got {other:?}"),
    }
    assert_eq!(
        engine.mode(),
        RunMode::Halted,
        "a refused resume stays halted"
    );
    assert!(engine.params_dirty, "the refused edits are still pending");

    // With the tables small again the same engine resumes.
    set_wide_cell(&mut engine, false);
    engine.resume().expect("the corrected model resumes");
    assert_eq!(engine.mode(), RunMode::Running);
}
