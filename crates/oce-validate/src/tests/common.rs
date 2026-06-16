//! Shared builders and re-exports for the deep-gate test matrix. The matrix is split across the
//! `structural`, `unification`, and `determinism` submodules to honor the 700-LOC/file cap; every
//! submodule pulls these in with `use super::common::*`.
//!
//! Tests compare `attrs.as_real()` fields or the `Vec<oce_diag::Diagnostic>` stream directly —
//! never `assert_eq!` on a whole `ModelGraph`/`Connector` (neither derives `PartialEq`).

pub(super) use std::sync::Arc;

pub(super) use oce_diag::{DiagCode, Severity};
pub(super) use oce_model::{
    Attrs, BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, IntAttrs, ModelGraph,
    ParamTable, RealAttrs, ValueType,
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

pub(super) fn conn_edge(from: u32, to: u32) -> Connection {
    Connection {
        from: ConnectorId(from),
        to: ConnectorId(to),
    }
}

/// Codes present in a diagnostic stream.
pub(super) fn codes(diags: &[oce_diag::Diagnostic]) -> Vec<DiagCode> {
    diags.iter().map(|d| d.code).collect()
}
