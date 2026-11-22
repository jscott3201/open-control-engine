//! The model-wide constructed budget on the engine's build tail, from a hand-built graph (issue
//! #700).
//!
//! No valid CXF document reaches `oce_graph::MAX_CONSTRUCTED_WORDS` any more: since issue #773 a
//! dense `MatrixGain` writes its cells, the document limits bound what cells cost, and since
//! issue #772 a table sized by a lone corner cell is refused by the load gate
//! (`tests/cxf_constructed_budget.rs` measures the largest a document reaches). The budget is
//! defence in depth for a host that builds a graph directly and for blobs. These tests pin it on
//! the load tail the CXF path shares (`build_validated_model_in_memory`, after the structural
//! gate): exactly the budget is built, one word more is a typed refusal naming the block that
//! crosses it in declaration order, the receipt records it at instantiation, and it comes before
//! the schedule's loop check and the state budget. The graphs bind lone corner cells, which the
//! constructor sizes from (a graph the load gate would refuse, as only a direct builder makes).

use std::sync::Arc;

use oce_graph::{BuildError, MAX_CONSTRUCTED_WORDS, MAX_STATE_WORDS};
use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value,
    ValueType,
};

use super::Engine;
use crate::diagnostics::{DiagnosticCapture, DiagnosticStage, OperationFailure};
use crate::error::OcError;

/// A table of 524,287 rows reports 1,048,576 words: four of them are exactly the budget.
const QUARTER_BUDGET: &str = "table_524287_2";
/// A one-row table: four words.
const SMALLEST: &str = "table_1_2";

/// Push one block of `class` with `params`, `ins` Real inputs and `outs` Real outputs, named
/// `urn:test:<name>`; returns its input and output connectors.
fn push(
    model: &mut ModelGraph,
    name: &str,
    class: &str,
    params: Vec<(Arc<str>, Value)>,
    ins: &[ValueType],
    outs: &[ValueType],
) -> (Vec<ConnectorId>, Vec<ConnectorId>) {
    let id = BlockId(u32::try_from(model.blocks.len()).unwrap());
    let mut connector = |dir, value_type| {
        let cid = ConnectorId(u32::try_from(model.connectors.len()).unwrap());
        model
            .connectors
            .push(Connector::new(cid, id, dir, value_type, cid.0));
        cid
    };
    let inputs: Vec<_> = ins.iter().map(|&t| connector(Dir::In, t)).collect();
    let outputs: Vec<_> = outs.iter().map(|&t| connector(Dir::Out, t)).collect();
    model.blocks.push(BlockInstance {
        id,
        class_iri: Arc::from(class),
        inputs: inputs.clone(),
        outputs: outputs.clone(),
        params: ParamTable { values: params },
        decl_order: id.0,
        instance_iri: Some(Arc::from(format!("urn:test:{name}"))),
    });
    (inputs, outputs)
}

/// One `Reals.Sources.TimeTable` per cell, each bound to that one corner cell only.
fn tables(model: &mut ModelGraph, cells: &[&str]) {
    for (index, cell) in cells.iter().enumerate() {
        let params = vec![(Arc::from(*cell), Value::Real(0.0))];
        push(
            model,
            &format!("table{index}"),
            "CDL.Reals.Sources.TimeTable",
            params,
            &[],
            &[ValueType::Real],
        );
    }
}

fn graph(cells: &[&str]) -> ModelGraph {
    let mut model = ModelGraph::new();
    tables(&mut model, cells);
    model
}

/// The engine's build tail on `model` (the structural gate passed): its result, and the
/// receipt of a refusal.
fn build(model: ModelGraph) -> (Result<(), OcError>, DiagnosticCapture) {
    let mut engine = Engine::new();
    let mut capture = DiagnosticCapture::new(true, DiagnosticStage::Validation);
    let result = engine
        .build_validated_model_in_memory(model, None, Vec::new(), &mut capture)
        .map(|_| ());
    (result, capture)
}

fn crossed(result: Result<(), OcError>) -> (String, u64, usize) {
    match result {
        Err(OcError::Build(BuildError::ConstructedTooLarge {
            block,
            requested,
            limit,
        })) => (block.0, requested, limit),
        other => panic!("expected ConstructedTooLarge, got {other:?}"),
    }
}

#[test]
fn exactly_the_budget_is_built_and_one_word_more_names_the_block_that_crosses_it() {
    let (at_budget, _) = build(graph(&[QUARTER_BUDGET; 4]));
    at_budget.expect("four quarter-budget tables are exactly the budget");

    let (past, _) = build(graph(&[
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        SMALLEST,
    ]));
    assert_eq!(
        crossed(past),
        (
            "urn:test:table4".to_owned(),
            MAX_CONSTRUCTED_WORDS as u64 + 4,
            MAX_CONSTRUCTED_WORDS
        )
    );
    // Counted in declaration order: with the small table first, the last large one crosses.
    let (past, _) = build(graph(&[
        SMALLEST,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
    ]));
    assert_eq!(
        crossed(past),
        (
            "urn:test:table4".to_owned(),
            MAX_CONSTRUCTED_WORDS as u64 + 4,
            MAX_CONSTRUCTED_WORDS
        )
    );
}

#[test]
fn the_receipt_records_the_refusal_as_a_resource_limit_at_instantiation() {
    let (result, capture) = build(graph(&[
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        QUARTER_BUDGET,
        SMALLEST,
    ]));
    let error = result.expect_err("past the budget");
    assert!(matches!(
        error,
        OcError::Build(BuildError::ConstructedTooLarge { .. })
    ));
    let failure = OperationFailure::new(error, capture);
    assert_eq!(failure.stage(), DiagnosticStage::Instantiation);
    let records = failure.diagnostics().records();
    let limits: Vec<_> = records
        .iter()
        .filter(|record| record.key().code() == "resource-limit-exceeded")
        .collect();
    assert_eq!(limits.len(), 1, "{records:?}");
    assert_eq!(limits[0].key().stage(), DiagnosticStage::Instantiation);
    assert_eq!(
        limits[0].message(),
        "constructed blocks too large: 4194308 word(s) held by the blocks through this block \
         exceed the supported maximum (4194304 words). Remedy: reduce table, matrix or vector \
         sizes, or split the model."
    );
}

#[test]
fn the_constructed_refusal_comes_before_the_loop_check_and_the_state_budget() {
    // Two `Abs` blocks feeding each other: an algebraic loop, reported as one while the tables
    // fit the budget, and never reached past it.
    let looped = |cells: &[&str]| {
        let mut model = graph(cells);
        let real = [ValueType::Real];
        let (a_in, a_out) = push(&mut model, "a", "CDL.Reals.Abs", Vec::new(), &real, &real);
        let (b_in, b_out) = push(&mut model, "b", "CDL.Reals.Abs", Vec::new(), &real, &real);
        model.connections.push(Connection {
            from: a_out[0],
            to: b_in[0],
        });
        model.connections.push(Connection {
            from: b_out[0],
            to: a_in[0],
        });
        model
    };
    let (within, _) = build(looped(&[SMALLEST]));
    assert!(
        matches!(
            within,
            Err(OcError::Build(BuildError::AlgebraicLoop { .. }))
        ),
        "{within:?}"
    );
    let (past, _) = build(looped(&[QUARTER_BUDGET; 5]));
    assert_eq!(crossed(past).0, "urn:test:table4");

    // A window one word past the state budget is reported only when the blocks fit.
    let with_mean = |cells: &[&str]| {
        let mut model = ModelGraph::new();
        let window = i64::try_from(MAX_STATE_WORDS).unwrap() - 3;
        push(
            &mut model,
            "mean",
            "CDL.Discrete.TriggeredMovingMean",
            vec![(Arc::from("n"), Value::Integer(window))],
            &[ValueType::Real, ValueType::Boolean],
            &[ValueType::Real],
        );
        let offset = model.blocks.len();
        let mut rest = graph(cells);
        for block in &mut rest.blocks {
            block.instance_iri = Some(Arc::from(format!("urn:test:table{}", block.id.0)));
        }
        let shift = u32::try_from(model.connectors.len()).unwrap();
        for (index, mut block) in rest.blocks.into_iter().enumerate() {
            block.id = BlockId(u32::try_from(offset + index).unwrap());
            block.decl_order = block.id.0;
            for connector in block.inputs.iter_mut().chain(&mut block.outputs) {
                connector.0 += shift;
            }
            model.blocks.push(block);
        }
        for mut connector in rest.connectors {
            connector.id.0 += shift;
            connector.block = BlockId(connector.block.0 + u32::try_from(offset).unwrap());
            model.connectors.push(connector);
        }
        model
    };
    let (state, _) = build(with_mean(&[SMALLEST]));
    assert!(
        matches!(state, Err(OcError::Build(BuildError::StateTooLarge { .. }))),
        "{state:?}"
    );
    let (both, _) = build(with_mean(&[QUARTER_BUDGET; 5]));
    assert_eq!(crossed(both).0, "urn:test:table4");
}

#[test]
fn the_structural_gate_in_front_refuses_a_lone_corner_before_any_block_is_built() {
    // The load tail behind `build_model_in_memory` and `load_cxf` runs the load gate first: a
    // lone corner cell leaves its table's rectangle unfilled, refused before construction.
    let mut engine = Engine::new();
    let refused = engine.build_model_in_memory(graph(&[QUARTER_BUDGET; 5]), None);
    assert!(matches!(refused, Err(OcError::Validate(_))), "{refused:?}");
}
