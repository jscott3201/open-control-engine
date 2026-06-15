//! BUILD step 1 — the direct-feedthrough DAG (`01-execution-model.md` §4).
//!
//! The dependency graph is **not** the connection graph; it is the *direct-feedthrough* graph over
//! connector vertices (CDL §7.16). It has two edge sources (`01` §4.1): connection edges (producer
//! output → consumer input) and block-internal input → output edges emitted **only** where the
//! block's output algebraically depends on that input — queried through the [`Block`] feedthrough
//! oracle. A loop-breaker `[S]` block (`Pre`, `UnitDelay`, an integrator) reports
//! `feeds_through == false` on its state-bearing path, emits no internal edge, and thereby *cuts*
//! the graph so the otherwise-cyclic feedback is legal (`01` §4.2/§4.3).

use oce_blocks::Block;
use oce_model::{ConnectorId, Model};

/// The direct-feedthrough dependency graph over connector vertices (`01` §4.4), held as adjacency
/// lists in dense `ConnectorId` space (0-based). An edge `u → v` means **`u` must be finalized
/// before `v`**, so `v` depends on `u`.
pub struct FeedthroughDag {
    /// Number of connector vertices (equals `model.connectors.len()`).
    pub n: usize,
    /// `succ[v]` = the out-neighbours of `v` (the vertices that directly depend on `v`).
    pub succ: Vec<Vec<ConnectorId>>,
    /// In-degree of each vertex; consumed (and decremented) by the Kahn sort (`01` §6).
    pub indeg: Vec<u32>,
}

impl FeedthroughDag {
    /// An empty graph sized for `n` connector vertices.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        Self {
            n,
            succ: vec![Vec::new(); n],
            indeg: vec![0; n],
        }
    }

    /// Add a dependency edge `from → to` (so `to` depends on `from`); bumps `to`'s in-degree.
    pub fn add_edge(&mut self, from: ConnectorId, to: ConnectorId) {
        self.succ[from.0 as usize].push(to);
        self.indeg[to.0 as usize] += 1;
    }
}

/// BUILD step 1: construct the direct-feedthrough DAG from the flattened [`Model`] and the
/// instantiated, parameter-resolved block impls (`blocks[i]` is the impl for `BlockId(i)`; index
/// equals `BlockId.0`).
///
/// Both edge passes iterate in **deterministic order** — connections in their stored order, blocks
/// in `BlockId` order, ports in declared connector order — so the adjacency lists are themselves
/// order-stable, which the Kahn sort relies on (`01` §4.4). It is infallible at M0: the model is
/// assumed flattened and validated (dense, in-range ids); structural validation of the model is an
/// ingest/validate concern (`04` / `oce-validate`), not BUILD.
#[must_use]
pub fn build_feedthrough_dag(model: &Model, blocks: &[Box<dyn Block>]) -> FeedthroughDag {
    debug_assert_eq!(
        blocks.len(),
        model.blocks.len(),
        "blocks must be 1:1 with model.blocks, indexed by BlockId.0"
    );
    let mut dag = FeedthroughDag::with_capacity(model.connectors.len());

    // (a) Connection edges: producer output → consumer input. The consumer depends on the producer.
    for c in &model.connections {
        dag.add_edge(c.from, c.to);
    }

    // (b) Block-internal input → output edges, only where the output feeds through from the input.
    for blk in &model.blocks {
        let blk_impl = blocks[blk.id.0 as usize].as_ref();
        for (i, &u) in blk.inputs.iter().enumerate() {
            for (j, &y) in blk.outputs.iter().enumerate() {
                if blk_impl.feeds_through(i, j) {
                    dag.add_edge(u, y);
                }
            }
        }
    }

    dag
}
