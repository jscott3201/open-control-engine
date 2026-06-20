//! BUILD tests for `oce-graph`: DAG construction, the deterministic Kahn sort, algebraic-loop
//! rejection, and cross-checks against a self-contained reference oracle (no external graph crate
//! enters the dependency tree — the oracle is a naïve in-tree re-implementation).

pub(super) use std::cell::RefCell;
pub(super) use std::sync::Arc;

pub(super) use oce_blocks::{
    Block, BlockKind, BlockSignature, Ctx, Diagnostics, NoopDiagnostics, PortKind, lookup,
};
pub(super) use oce_model::{
    BlockId, BlockInstance, Connection, Connector, ConnectorId, Dir, Model, ModelGraph, ParamTable,
    Value, ValueType,
};

pub(super) use crate::{
    BuildError, EvalContext, FeedthroughDag, RunState, Schedule, allocate_state,
    build_feedthrough_dag, compile, eval_tick,
};

// ---- test fixtures --------------------------------------------------------------------------

/// Feedthrough spec for a [`TestBlock`]: either a uniform answer for every port pair, or an
/// explicit set of `(in_idx, out_idx)` pairs that feed through (all others cut).
enum Ft {
    Uniform(bool),
    Pairs(Vec<(usize, usize)>),
}

/// A configurable test block. BUILD only ever queries `feeds_through` and `kind`, so a fixed dummy
/// `signature` is sufficient.
struct TestBlock {
    ft: Ft,
    stateful: bool,
}

impl Block for TestBlock {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "test.TestBlock",
            inputs: &[],
            outputs: &[],
            stateful: false,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        if self.stateful {
            BlockKind::Stateful
        } else {
            BlockKind::Algebraic
        }
    }
    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool {
        match &self.ft {
            Ft::Uniform(b) => *b,
            Ft::Pairs(pairs) => pairs.contains(&(in_idx, out_idx)),
        }
    }
}

#[derive(Default)]
pub(super) struct CapturingDiagnostics {
    pub(super) events: RefCell<Vec<(String, String, f64)>>,
}

impl Diagnostics for CapturingDiagnostics {
    fn warn(&self, source: &str, message: &str, t: f64) {
        self.events
            .borrow_mut()
            .push((source.to_string(), message.to_string(), t));
    }
}

pub(super) struct WarningSource;

impl Block for WarningSource {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "test.WarningSource",
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
        ctx.warn("test.WarningSource", "init and tick warning");
    }
}

/// Builds a flattened [`ModelGraph`] with the dense, declaration-order id assignment the engine
/// relies on (block id == decl order; connector id == global decl order), plus the parallel
/// `Vec<Box<dyn Block>>` indexed by `BlockId.0`.
#[derive(Default)]
pub(super) struct ModelBuilder {
    pub(super) model: ModelGraph,
    pub(super) blocks: Vec<Box<dyn Block>>,
    pub(super) next_conn: u32,
}

impl ModelBuilder {
    /// Add a block with a *uniform* feedthrough answer.
    pub(super) fn block(
        &mut self,
        class: &str,
        n_in: usize,
        n_out: usize,
        feeds: bool,
        stateful: bool,
    ) -> (BlockId, Vec<ConnectorId>, Vec<ConnectorId>) {
        self.add(class, n_in, n_out, stateful, Ft::Uniform(feeds))
    }

    /// Add a block with *per-pair* feedthrough (the `(in,out)` pairs that feed through). Used to
    /// build mixed-feedthrough multi-output blocks the uniform fixture cannot express.
    pub(super) fn block_mixed(
        &mut self,
        class: &str,
        n_in: usize,
        n_out: usize,
        stateful: bool,
        pairs: &[(usize, usize)],
    ) -> (BlockId, Vec<ConnectorId>, Vec<ConnectorId>) {
        self.add(class, n_in, n_out, stateful, Ft::Pairs(pairs.to_vec()))
    }

    fn add(
        &mut self,
        class: &str,
        n_in: usize,
        n_out: usize,
        stateful: bool,
        ft: Ft,
    ) -> (BlockId, Vec<ConnectorId>, Vec<ConnectorId>) {
        let id = BlockId(self.model.blocks.len() as u32);
        let push_conn = |this: &mut Self, dir: Dir| {
            let cid = ConnectorId(this.next_conn);
            this.model.connectors.push(Connector::new(
                cid,
                id,
                dir,
                ValueType::Real,
                this.next_conn,
            ));
            this.next_conn += 1;
            cid
        };
        let inputs: Vec<ConnectorId> = (0..n_in).map(|_| push_conn(self, Dir::In)).collect();
        let outputs: Vec<ConnectorId> = (0..n_out).map(|_| push_conn(self, Dir::Out)).collect();
        self.model.blocks.push(BlockInstance {
            id,
            class_iri: Arc::from(class),
            inputs: inputs.clone(),
            outputs: outputs.clone(),
            params: ParamTable::default(),
            decl_order: id.0,
            instance_iri: None,
        });
        self.blocks.push(Box::new(TestBlock { ft, stateful }));
        (id, inputs, outputs)
    }

    /// Add a block from a real `oce-blocks` impl, deriving the connector arity and value types from
    /// its signature (used by the TICK tests, which need real `step`/`emit`/`update` bodies).
    pub(super) fn block_real(
        &mut self,
        blk: Box<dyn Block>,
    ) -> (BlockId, Vec<ConnectorId>, Vec<ConnectorId>) {
        let sig = blk.signature(); // &'static — does not borrow blk
        let id = BlockId(self.model.blocks.len() as u32);
        let push = |this: &mut Self, kind: PortKind, dir: Dir| {
            let cid = ConnectorId(this.next_conn);
            let value_type = match kind {
                PortKind::Real => ValueType::Real,
                PortKind::Integer => ValueType::Integer,
                PortKind::Boolean => ValueType::Boolean,
            };
            this.model
                .connectors
                .push(Connector::new(cid, id, dir, value_type, this.next_conn));
            this.next_conn += 1;
            cid
        };
        let inputs: Vec<ConnectorId> = sig.inputs.iter().map(|&k| push(self, k, Dir::In)).collect();
        let outputs: Vec<ConnectorId> = sig
            .outputs
            .iter()
            .map(|&k| push(self, k, Dir::Out))
            .collect();
        self.model.blocks.push(BlockInstance {
            id,
            class_iri: Arc::from(sig.class_path),
            inputs: inputs.clone(),
            outputs: outputs.clone(),
            params: ParamTable::default(),
            decl_order: id.0,
            instance_iri: None,
        });
        self.blocks.push(blk);
        (id, inputs, outputs)
    }

    pub(super) fn connect(&mut self, from: ConnectorId, to: ConnectorId) {
        self.model.connections.push(Connection { from, to });
    }
}

/// Construct a real block via the `oce-blocks` registry from a class path + named parameters.
pub(super) fn make(class: &str, params: &[(&str, Value)]) -> Box<dyn Block> {
    let table = ParamTable {
        values: params
            .iter()
            .map(|(n, v)| (Arc::from(*n), v.clone()))
            .collect(),
    };
    let entry = lookup(class).unwrap_or_else(|| panic!("no registry entry for {class}"));
    (entry.make)(&table)
}

/// Evaluate one tick (scopes the [`EvalContext`] so `state` is readable afterwards).
pub(super) fn tick_once(
    model: &Model,
    schedule: &Schedule,
    blocks: &[Box<dyn Block>],
    state: &mut RunState,
    t: f64,
) {
    let diag = NoopDiagnostics;
    let mut ctx = EvalContext {
        model,
        schedule,
        blocks,
        diagnostics: &diag,
        state,
    };
    eval_tick(&mut ctx, t);
}

pub(super) fn tick_with_diag(
    model: &Model,
    schedule: &Schedule,
    blocks: &[Box<dyn Block>],
    state: &mut RunState,
    t: f64,
    diagnostics: &dyn Diagnostics,
) {
    let mut ctx = EvalContext {
        model,
        schedule,
        blocks,
        diagnostics,
        state,
    };
    eval_tick(&mut ctx, t);
}

// ---- reference oracle (D6 (c), in-tree, no external graph crate) ---------------------------

/// An independent, deliberately naïve reference topo sort: each step scans **all** vertices and
/// emits the in-degree-0 vertex with the smallest declaration key. Same *semantics* as
/// [`crate::topo_sort`]'s binary heap, a completely different *implementation* (O(V²) linear scan
/// vs heap) — so agreement cross-checks the heap's tie-break and determinism. Returns `None` on a
/// cycle (fewer than `n` vertices emitted).
pub(super) fn reference_order(dag: &FeedthroughDag, model: &Model) -> Option<Vec<ConnectorId>> {
    let n = dag.n;
    let mut indeg = dag.indeg.clone();
    let mut emitted = vec![false; n];
    let mut order = Vec::with_capacity(n);
    for _ in 0..n {
        let mut best: Option<(u32, u32, u32)> = None; // (block decl, connector decl, id)
        for (v, &d) in indeg.iter().enumerate() {
            if emitted[v] || d != 0 {
                continue;
            }
            let conn = &model.connectors[v];
            let key = (
                model.blocks[conn.block.0 as usize].decl_order,
                conn.decl_order,
                v as u32,
            );
            if best.is_none_or(|b| key < b) {
                best = Some(key);
            }
        }
        let Some((_, _, id)) = best else { break };
        emitted[id as usize] = true;
        order.push(ConnectorId(id));
        for &w in &dag.succ[id as usize] {
            indeg[w.0 as usize] -= 1;
        }
    }
    (order.len() == n).then_some(order)
}

/// True iff `order` is a valid linearization of `dag`: every vertex appears exactly once and every
/// edge `u → w` places `u` strictly before `w`.
pub(super) fn is_valid_topo_order(dag: &FeedthroughDag, order: &[ConnectorId]) -> bool {
    if order.len() != dag.n {
        return false;
    }
    let mut pos = vec![usize::MAX; dag.n];
    for (i, c) in order.iter().enumerate() {
        pos[c.0 as usize] = i;
    }
    if pos.contains(&usize::MAX) {
        return false;
    }
    for (u, succ) in dag.succ.iter().enumerate() {
        if succ.iter().any(|w| pos[u] >= pos[w.0 as usize]) {
            return false;
        }
    }
    true
}

// ---- tests ----------------------------------------------------------------------------------
