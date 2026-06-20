//! Shared helpers for facade tests. Builds a **hand-built** flattened model with **no parser** and
//! asserts the engine's load → BUILD → tick contract end to end:
//!
//! - the `Engine` loads a hand-built `ModelGraph` and ticks it;
//! - a multi-tick run advances the canonical `Add`/`UnitDelay` feedback accumulator with a
//!   true **one-tick** delay (1, 2, 3, 4 — not the two-tick delay an inline emit-then-update gives);
//! - an injected feedthrough cycle is rejected with a typed [`BuildError::AlgebraicLoop`];
//! - determinism: two independent compiles of the same model produce **byte-identical**
//!   `order`/`connector_order`/`driver_of`;
//! - a `MemStore` model round-trip + no-op `commit`/`flush`/`recover` through the engine.

pub(super) use std::sync::Arc;

pub(super) use oce_graph::BuildError;
pub(super) use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, ModelGraph, ParamTable, Value,
    ValueType,
};
pub(super) use oce_store::{DomainKey, Durable, ModelStore, ResolvedModel};

pub(super) use crate::{
    CollectSpec, Engine, InputSource, IoClass, OcError, PhysicalKind, PointDirection, RunMode,
    SemanticQuery, SimSpec, TemplateRef,
};
pub(super) use oce_store_mem::MemStore;

/// Tiny hand-builder for a flattened [`ModelGraph`]: dense block/connector ids are assigned in
/// declaration order (so `decl_order == id`), which is exactly what the deterministic scheduler
/// tie-breaks on. Keeps the exit-criteria graphs readable without a parser.
pub(super) struct Mb {
    m: ModelGraph,
}

impl Mb {
    pub(super) fn new() -> Self {
        Self {
            m: ModelGraph::new(),
        }
    }

    /// Append a block with `ins` input connectors and `outs` output connectors (typed), plus its
    /// ground params; returns `(block, input connector ids, output connector ids)`.
    pub(super) fn block(
        &mut self,
        class: &str,
        ins: &[ValueType],
        outs: &[ValueType],
        params: Vec<(Arc<str>, Value)>,
    ) -> (BlockId, Vec<ConnectorId>, Vec<ConnectorId>) {
        let bid = BlockId(self.m.blocks.len() as u32);
        let push_conn = |m: &mut ModelGraph, dir: Dir, vt: ValueType| {
            let id = ConnectorId(m.connectors.len() as u32);
            m.connectors.push(Connector::new(id, bid, dir, vt, id.0));
            id
        };
        let in_ids: Vec<ConnectorId> = ins
            .iter()
            .map(|&vt| push_conn(&mut self.m, Dir::In, vt))
            .collect();
        let out_ids: Vec<ConnectorId> = outs
            .iter()
            .map(|&vt| push_conn(&mut self.m, Dir::Out, vt))
            .collect();
        self.m.blocks.push(BlockInstance {
            id: bid,
            class_iri: Arc::from(class),
            inputs: in_ids.clone(),
            outputs: out_ids.clone(),
            params: ParamTable { values: params },
            decl_order: bid.0,
            instance_iri: None,
        });
        (bid, in_ids, out_ids)
    }

    pub(super) fn connect(&mut self, from: ConnectorId, to: ConnectorId) {
        self.m.connections.push(Connection { from, to });
    }

    pub(super) fn finish(self) -> ModelGraph {
        self.m
    }
}

pub(super) fn rp(name: &str, v: f64) -> (Arc<str>, Value) {
    (Arc::from(name), Value::Real(v))
}

/// The canonical accumulator graph (6 blocks): a `Constant(1)` drives an `Add` that is closed into
/// a one-sample feedback loop by a `UnitDelay` (the accumulator `acc(k) = acc(k-1) + 1`), with a
/// `Constant(2.5)` + `Greater` comparator and a `Limiter[0,3]` reading the accumulator. Returns the
/// model plus the `(add_out, greater_out, limiter_out)` connectors to probe.
pub(super) fn build_accumulator_model() -> (ModelGraph, ConnectorId, ConnectorId, ConnectorId) {
    let mut mb = Mb::new();
    let (_, _, c1) = mb.block(
        "CDL.Reals.Sources.Constant",
        &[],
        &[ValueType::Real],
        vec![rp("k", 1.0)],
    );
    let (_, add_in, add_out) = mb.block(
        "CDL.Reals.Add",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let (_, ud_in, ud_out) = mb.block(
        "CDL.Discrete.UnitDelay",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let (_, _, c2) = mb.block(
        "CDL.Reals.Sources.Constant",
        &[],
        &[ValueType::Real],
        vec![rp("k", 2.5)],
    );
    let (_, gt_in, gt_out) = mb.block(
        "CDL.Reals.Greater",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Boolean],
        vec![],
    );
    let (_, lim_in, lim_out) = mb.block(
        "CDL.Reals.Limiter",
        &[ValueType::Real],
        &[ValueType::Real],
        vec![rp("uMin", 0.0), rp("uMax", 3.0)],
    );

    mb.connect(c1[0], add_in[0]); // Constant(1) → Add.u1
    mb.connect(ud_out[0], add_in[1]); // UnitDelay → Add.u2  (the feedback edge)
    mb.connect(add_out[0], ud_in[0]); // Add → UnitDelay      (closes the loop; UnitDelay cuts it)
    mb.connect(add_out[0], gt_in[0]); // Add → Greater.u1
    mb.connect(c2[0], gt_in[1]); // Constant(2.5) → Greater.u2
    mb.connect(add_out[0], lim_in[0]); // Add → Limiter

    (mb.finish(), add_out[0], gt_out[0], lim_out[0])
}

/// Two `Add` blocks driving each other: a feedthrough cycle with **no** state-holding loop-breaker
/// — the model an engine must reject (CDL §7.16), not silently solve.
pub(super) fn build_algebraic_loop_model() -> ModelGraph {
    let mut mb = Mb::new();
    let (_, a_in, a_out) = mb.block(
        "CDL.Reals.Add",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    let (_, b_in, b_out) = mb.block(
        "CDL.Reals.Add",
        &[ValueType::Real, ValueType::Real],
        &[ValueType::Real],
        vec![],
    );
    mb.connect(a_out[0], b_in[0]);
    mb.connect(b_out[0], a_in[0]);
    let mut model = mb.finish();
    model.external_inputs = vec![a_in[1], b_in[1]];
    model
}
