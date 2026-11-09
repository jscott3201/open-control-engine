//! `validate_block_parameters` reports exactly what whole-model validation reports for the same
//! block (GitHub #440), over the constructor fuzz's own hostile tables.
//!
//! Every registered class is driven with the shared generator in
//! `crates/oce-blocks/tests/support/hostile_tables.rs`: the empty table, every name the class's
//! rules and defaults mention (in- and out-of-range array members such as `k_0`, `K_3_1`, `k_`,
//! junk names) alone with NaN, `-0.0`, `i64` extremes, a width one past the cap, `true` and an
//! empty string, then seeded random tables with duplicates and every value kind. For each table
//! the standalone entry and a full `validate` of a one-block model wired to the block's resolved
//! ports must return the same result — the same diagnostics, codes, subjects, messages and order,
//! warnings included. Where the resolved arity is too large to wire, the block is validated
//! port-less and its error diagnostics, less the arity refusal that alone produces, must agree.

#[allow(dead_code)]
#[path = "../../oce-blocks/tests/support/hostile_tables.rs"]
mod hostile_tables;

use std::sync::Arc;

use hostile_tables::{XorShift, env_u64, hostile_tables};
use oce_blocks::PortKind;
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{
    Attrs, BlockId, BlockInstance, Connector, ConnectorId, Dir, ModelGraph, ParamTable, ValueType,
};
use oce_validate::{validate, validate_block_parameters};

/// Resolved arities above this are compared port-less (errors only).
const MAX_WIRED_PORTS: usize = 4096;

fn value_type(kind: PortKind) -> ValueType {
    match kind {
        PortKind::Real => ValueType::Real,
        PortKind::Integer => ValueType::Integer,
        PortKind::Boolean => ValueType::Boolean,
    }
}

/// One block of `class_path` carrying `params`, its inputs declared as external boundary inputs
/// (legal in-degree 0) and its outputs unconnected, or port-less when `kinds` is `None`.
fn model(
    class_path: &str,
    params: &ParamTable,
    kinds: Option<(&[PortKind], &[PortKind])>,
) -> ModelGraph {
    let mut graph = ModelGraph::default();
    let mut block = BlockInstance {
        id: BlockId(0),
        class_iri: Arc::from(class_path),
        inputs: Vec::new(),
        outputs: Vec::new(),
        params: params.clone(),
        decl_order: 0,
        instance_iri: None,
    };
    if let Some((inputs, outputs)) = kinds {
        for (dir, side) in [(Dir::In, inputs), (Dir::Out, outputs)] {
            for kind in side {
                let id = ConnectorId(graph.connectors.len() as u32);
                graph.connectors.push(Connector {
                    id,
                    block: BlockId(0),
                    dir,
                    value_type: value_type(*kind),
                    attrs: Attrs::default_for(value_type(*kind)),
                    decl_order: id.0,
                    iri: None,
                });
                match dir {
                    Dir::In => {
                        block.inputs.push(id);
                        graph.external_inputs.push(id);
                    }
                    Dir::Out => block.outputs.push(id),
                }
            }
        }
    }
    graph.blocks.push(block);
    graph
}

fn outcome(
    result: Result<Vec<Diagnostic>, oce_validate::ValidationError>,
) -> (bool, Vec<Diagnostic>) {
    match result {
        Ok(warnings) => (true, warnings),
        Err(error) => (false, error.diagnostics),
    }
}

#[test]
fn standalone_parameter_checks_equal_whole_model_validation_on_hostile_tables() {
    let cases = env_u64("OCE_PARAM_AGREEMENT_CASES", 60);
    let mut rng = XorShift(env_u64("OCE_PARAM_FUZZ_SEED", 0x0CEB_0440_A6EE_0001).max(1));
    let (mut wired, mut portless) = (0usize, 0usize);
    for entry in oce_blocks::catalog() {
        for params in hostile_tables(entry.class_path, cases, &mut rng) {
            let standalone = outcome(validate_block_parameters(entry.class_path, &params));
            let block = (oce_blocks::lookup(entry.class_path).unwrap().make)(&params);
            let signature = block.resolved_signature();
            if signature.inputs.len() <= MAX_WIRED_PORTS
                && signature.outputs.len() <= MAX_WIRED_PORTS
            {
                let graph = model(
                    entry.class_path,
                    &params,
                    Some((&signature.inputs, &signature.outputs)),
                );
                let whole = outcome(validate(&graph));
                assert_eq!(
                    standalone, whole,
                    "{}: {:?}",
                    entry.class_path, params.values
                );
                wired += 1;
            } else {
                let graph = model(entry.class_path, &params, None);
                let (_, whole) = outcome(validate(&graph));
                let whole_errors: Vec<Diagnostic> = whole
                    .into_iter()
                    .filter(|d| d.is_error() && d.code != DiagCode::MalformedDocument)
                    .collect();
                let standalone_errors: Vec<Diagnostic> = if standalone.0 {
                    Vec::new()
                } else {
                    standalone.1
                };
                assert_eq!(
                    standalone_errors, whole_errors,
                    "{}: {:?}",
                    entry.class_path, params.values
                );
                portless += 1;
            }
        }
    }
    assert!(wired > 10_000, "only {wired} tables compared wired");
    assert!(
        portless > 0,
        "no oversized table reached the port-less comparison"
    );
}
