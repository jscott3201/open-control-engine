//! The structural rule's signature probes are charged to the constructed budget (issue #722).
//!
//! `check_port_types` constructs each block to read its resolved signature and drops it. Each
//! construction is charged to one `oce_blocks::ConstructedBudget` before the next: exactly the
//! budget is probed without a finding, and the first block that takes the total past it is one
//! `resource-limit-exceeded` error on that block, after which no block is constructed or
//! compared. The graphs bind lone corner cells, which the `TimeTable` constructor sizes from (the
//! parameter rules refuse such a table too, and still do).

use super::common::*;
use oce_blocks::MAX_CONSTRUCTED_WORDS;
use oce_diag::Diagnostic;

/// A table of 524,287 rows reports 1,048,576 words: four of them are exactly the budget.
const QUARTER_BUDGET: &str = "table_524287_2";
/// A one-row table: four words.
const SMALLEST: &str = "table_1_2";
const TABLE: &str = "CDL.Reals.Sources.TimeTable";

/// One `TimeTable` per cell, each binding that one corner cell, with one Real output.
fn tables(cells: &[&str]) -> ModelGraph {
    let mut model = ModelGraph::default();
    for (index, cell) in cells.iter().enumerate() {
        let id = u32::try_from(index).unwrap();
        model
            .connectors
            .push(conn(id, id, Dir::Out, ValueType::Real));
        model.blocks.push(block_with_params(
            id,
            TABLE,
            &[],
            &[id],
            vec![rp(cell, 0.0)],
        ));
    }
    model
}

/// Every diagnostic `validate` gives `model`, errors and warnings alike.
fn diagnostics(model: &ModelGraph) -> Vec<Diagnostic> {
    match validate(model) {
        Ok(warnings) => warnings,
        Err(error) => error.diagnostics,
    }
}

/// The budget refusals among `model`'s diagnostics.
fn budget_refusals(model: &ModelGraph) -> Vec<Diagnostic> {
    diagnostics(model)
        .into_iter()
        .filter(|d| d.code == DiagCode::ResourceLimitExceeded)
        .collect()
}

fn crossing_message(requested: usize) -> String {
    format!(
        "constructed blocks too large: {requested} word(s) held by the blocks through this block \
         exceed the supported maximum (4194304 words). Remedy: reduce table, matrix or vector \
         sizes, or split the model."
    )
}

#[test]
fn exactly_the_budget_is_probed_and_one_word_more_is_refused_on_the_block_that_crosses_it() {
    assert_eq!(MAX_CONSTRUCTED_WORDS, 1 << 22);
    let at_budget = tables(&[QUARTER_BUDGET; 4]);
    assert_eq!(budget_refusals(&at_budget), Vec::new());
    // The parameter rules still refuse each unfilled table, once.
    assert_eq!(
        codes(&diagnostics(&at_budget)),
        vec![DiagCode::ParameterOutOfRange; 4]
    );

    for cells in [
        [
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            SMALLEST,
        ],
        [
            SMALLEST,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
            QUARTER_BUDGET,
        ],
    ] {
        let refusals = budget_refusals(&tables(&cells));
        assert_eq!(refusals.len(), 1, "{cells:?}: {refusals:?}");
        let refusal = &refusals[0];
        assert_eq!(refusal.severity, Severity::Error);
        // Counted in declaration order: the fifth block crosses in both orders.
        assert_eq!(refusal.subject.as_deref(), Some("block#4"), "{cells:?}");
        assert_eq!(refusal.message, crossing_message(MAX_CONSTRUCTED_WORDS + 4));
    }
}

#[test]
fn the_refusal_names_the_authored_instance_and_holds_the_saturated_request() {
    let mut model = tables(&[QUARTER_BUDGET; 5]);
    model.blocks[4].instance_iri = Some(Arc::from("urn:test:M.t4"));
    let refusals = budget_refusals(&model);
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].subject.as_deref(), Some("urn:test:M.t4"));
    assert_eq!(
        refusals[0].message,
        crossing_message(MAX_CONSTRUCTED_WORDS + (1 << 20))
    );
}

#[test]
fn no_block_after_the_crossing_is_compared_and_every_block_before_it_is() {
    // A `Constant` declaring two outputs is an interface mismatch wherever it stands. Before the
    // crossing it is reported; after it, nothing about it is, because it is never constructed.
    let mismatched = |model: &mut ModelGraph| {
        let id = u32::try_from(model.blocks.len()).unwrap();
        let first = u32::try_from(model.connectors.len()).unwrap();
        for offset in 0..2 {
            model
                .connectors
                .push(conn(first + offset, id, Dir::Out, ValueType::Real));
        }
        let mut constant = constant_block(id, &[first, first + 1]);
        constant.decl_order = id;
        model.blocks.push(constant);
    };
    let interface = |model: &ModelGraph| {
        diagnostics(model)
            .into_iter()
            .filter(|d| d.message.starts_with("block interface mismatch"))
            .count()
    };

    let mut after = tables(&[QUARTER_BUDGET; 5]);
    mismatched(&mut after);
    assert_eq!(interface(&after), 0);
    assert_eq!(budget_refusals(&after).len(), 1);

    let mut before = ModelGraph::default();
    mismatched(&mut before);
    let tail = tables(&[QUARTER_BUDGET; 5]);
    for mut table in tail.blocks {
        let id = u32::try_from(before.blocks.len()).unwrap();
        let connector = u32::try_from(before.connectors.len()).unwrap();
        before
            .connectors
            .push(conn(connector, id, Dir::Out, ValueType::Real));
        table.id = BlockId(id);
        table.decl_order = id;
        table.outputs = vec![ConnectorId(connector)];
        before.blocks.push(table);
    }
    assert_eq!(interface(&before), 1);
    let refusals = budget_refusals(&before);
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].subject.as_deref(), Some("block#5"));
}

#[test]
fn an_unregistered_class_is_not_constructed_and_charges_nothing() {
    // Four quarter tables with an unregistered block among them: exactly the budget, no refusal
    // (the facade reports the unknown class; the validator skips it, R-IMPL-2).
    let mut model = tables(&[QUARTER_BUDGET; 4]);
    let id = u32::try_from(model.blocks.len()).unwrap();
    model
        .blocks
        .insert(2, block(id, "CDL.Reals.NoSuchClass", &[], &[]));
    for (index, block) in model.blocks.iter_mut().enumerate() {
        block.id = BlockId(u32::try_from(index).unwrap());
        block.decl_order = block.id.0;
    }
    for connector in &mut model.connectors {
        if connector.block.0 >= 2 {
            connector.block = BlockId(connector.block.0 + 1);
        }
    }
    assert_eq!(budget_refusals(&model), Vec::new());
}

#[test]
fn the_probe_refusal_is_the_same_on_every_run() {
    let model = tables(&[QUARTER_BUDGET; 6]);
    let first = budget_refusals(&model);
    for _ in 0..3 {
        assert_eq!(budget_refusals(&model), first);
    }
    assert_eq!(first[0].subject.as_deref(), Some("block#4"));
}

#[test]
fn blocks_whose_declared_ports_do_not_match_their_class_are_charged_too() {
    // Each of five quarter-budget tables declares two outputs where its class has one (review of
    // GitHub #788): the probe is charged before the interface is compared, so the fifth crosses
    // the budget, and the four before it are each an interface mismatch.
    let mut model = tables(&[QUARTER_BUDGET; 5]);
    for index in 0..5 {
        let id = u32::try_from(model.connectors.len()).unwrap();
        let block = u32::try_from(index).unwrap();
        model
            .connectors
            .push(conn(id, block, Dir::Out, ValueType::Real));
        model.blocks[index].outputs.push(ConnectorId(id));
    }
    let refusals = budget_refusals(&model);
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert_eq!(refusals[0].subject.as_deref(), Some("block#4"));
    assert_eq!(
        refusals[0].message,
        crossing_message(MAX_CONSTRUCTED_WORDS + (1 << 20))
    );
    let mismatches = diagnostics(&model)
        .into_iter()
        .filter(|d| d.message.starts_with("block interface mismatch"))
        .count();
    assert_eq!(mismatches, 4);
}
