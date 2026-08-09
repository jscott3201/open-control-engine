//! Shared builders and re-exports for the deep-gate test matrix. The matrix is split across the
//! `structural`, `unification`, and `determinism` submodules to honor the 700-LOC/file cap; every
//! submodule pulls these in with `use super::common::*`.
//!
//! Tests compare `attrs.as_real()` fields or the `Vec<oce_diag::Diagnostic>` stream directly —
//! never `assert_eq!` on a whole `ModelGraph`/`Connector` (neither derives `PartialEq`).

pub(super) use std::sync::Arc;

pub(super) use oce_diag::{DiagCode, Severity};
pub(super) use oce_model::{
    Attrs, BlockId, BlockInstance, BoundaryOutput, Connection, Connector, ConnectorId, Dir,
    IntAttrs, ModelGraph, ParamTable, RealAttrs, Value, ValueType,
};

pub(super) use crate::{unify_and_validate, unify_attributes, validate};

/// A connector with explicit `RealAttrs` (via the checked builder), id == decl_order, no IRI.
pub(super) fn real_conn(id: u32, block: u32, dir: Dir, attrs: RealAttrs) -> Connector {
    Connector::new(ConnectorId(id), BlockId(block), dir, ValueType::Real, id)
        .with_attrs(Attrs::Real(attrs))
        .expect("Real attrs match a Real connector")
}

/// A `Real` connector with only `unit` set (the common §7.10 case).
pub(super) fn real_unit(id: u32, block: u32, dir: Dir, unit: Option<&str>) -> Connector {
    real_conn(
        id,
        block,
        dir,
        RealAttrs {
            unit: unit.map(Arc::from),
            ..RealAttrs::default()
        },
    )
}

/// A typed connector with default attributes (for structural/type tests).
pub(super) fn conn(id: u32, block: u32, dir: Dir, vt: ValueType) -> Connector {
    Connector::new(ConnectorId(id), BlockId(block), dir, vt, id)
}

/// An `Integer` connector with explicit `IntAttrs` (via the checked builder), for min/max tests.
pub(super) fn int_conn(id: u32, block: u32, dir: Dir, attrs: IntAttrs) -> Connector {
    Connector::new(ConnectorId(id), BlockId(block), dir, ValueType::Integer, id)
        .with_attrs(Attrs::Integer(attrs))
        .expect("Integer attrs match an Integer connector")
}

/// A block instance with the given canonical class path and port connector ids.
pub(super) fn block(id: u32, class: &str, inputs: &[u32], outputs: &[u32]) -> BlockInstance {
    BlockInstance {
        id: BlockId(id),
        class_iri: Arc::from(class),
        inputs: inputs.iter().map(|&i| ConnectorId(i)).collect(),
        outputs: outputs.iter().map(|&i| ConnectorId(i)).collect(),
        params: ParamTable::default(),
        decl_order: id,
        instance_iri: None,
    }
}

/// A `CDL.Reals.Sources.Constant` with its required `k` supplied — the minimal valid source
/// block for tests that need a well-formed source and don't care about the value.
pub(super) fn constant_block(id: u32, outputs: &[u32]) -> BlockInstance {
    block_with_params(
        id,
        "CDL.Reals.Sources.Constant",
        &[],
        outputs,
        vec![(Arc::from("k"), Value::Real(1.0))],
    )
}

/// A block instance with explicit raw parameter values.
pub(super) fn block_with_params(
    id: u32,
    class: &str,
    inputs: &[u32],
    outputs: &[u32],
    params: Vec<(Arc<str>, Value)>,
) -> BlockInstance {
    BlockInstance {
        params: ParamTable { values: params },
        ..block(id, class, inputs, outputs)
    }
}

pub(super) fn rp(name: &str, value: f64) -> (Arc<str>, Value) {
    (Arc::from(name), Value::Real(value))
}

pub(super) fn one_block_model(
    class: &str,
    inputs: &[ValueType],
    outputs: &[ValueType],
    params: Vec<(Arc<str>, Value)>,
) -> ModelGraph {
    let mut connectors = Vec::with_capacity(inputs.len() + outputs.len());
    let mut input_ids = Vec::with_capacity(inputs.len());
    let mut output_ids = Vec::with_capacity(outputs.len());
    for (idx, value_type) in inputs.iter().copied().enumerate() {
        let id = idx as u32;
        connectors.push(conn(id, 0, Dir::In, value_type));
        input_ids.push(id);
    }
    for (offset, value_type) in outputs.iter().copied().enumerate() {
        let id = (inputs.len() + offset) as u32;
        connectors.push(conn(id, 0, Dir::Out, value_type));
        output_ids.push(id);
    }
    ModelGraph {
        blocks: vec![block_with_params(0, class, &input_ids, &output_ids, params)],
        connectors,
        connections: vec![],
        external_inputs: input_ids.into_iter().map(ConnectorId).collect(),
        boundary_outputs: vec![],
    }
}

pub(super) fn real_to_real_model(class: &str, params: Vec<(Arc<str>, Value)>) -> ModelGraph {
    one_block_model(class, &[ValueType::Real], &[ValueType::Real], params)
}

pub(super) fn conn_edge(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

pub(super) fn boundary_output(iri: &str, source: u32, attrs: Attrs) -> BoundaryOutput {
    BoundaryOutput {
        iri: Arc::from(iri),
        source: ConnectorId(source),
        attrs,
    }
}

/// Codes present in a diagnostic stream.
pub(super) fn codes(diags: &[oce_diag::Diagnostic]) -> Vec<DiagCode> {
    diags.iter().map(|d| d.code).collect()
}
