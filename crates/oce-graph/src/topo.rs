//! BUILD steps 2–3 — algebraic-loop **hard rejection** (`01` §5) and the deterministic Kahn
//! topological sort (`01` §6). Per **D6** these are own Kahn implementations with a
//! declaration-order tie-break: **selene-free** (no `selene-algorithms`, no `rayon`, no external
//! graph crate), `O(V + E)`, run entirely on flat arrays.
//!
//! Two sorts run in BUILD:
//! 1. **Connector-level** sort over the [`FeedthroughDag`] → `connector_order` (retained for trace
//!    tooling) and connector-level algebraic-loop rejection with fine-grained diagnostics.
//! 2. **Block-level** sort over the *emit-before* graph → the frozen `order` the tick iterates.
//!    Block A precedes block B iff an output of A drives an input that B **reads at emit time**
//!    (every input for `[A]` blocks; only feedthrough inputs for `[S]` blocks — a loop-breaker's
//!    cut input is read only by the deferred state update, never at emit). Because the engine fires
//!    a block *atomically* (one `emit` call produces all its outputs, `01` §9), a cycle in this
//!    graph is a **block-granularity algebraic loop** — unschedulable by single-pass firing even
//!    when the connector graph is acyclic (a stateful multi-output block whose state-only output
//!    feeds a path back to one of its feedthrough inputs). It is rejected, not silently mis-ordered.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use oce_blocks::{Block, BlockKind};
use oce_model::{BlockId, ConnectorId, Model};

use crate::build::FeedthroughDag;
use crate::{BuildError, ConnectorPath, Schedule};

/// Tie-break key for a connector: its owning block's declaration order, then the connector's own
/// declaration order (`01` §6.2). Combined with the unique `ConnectorId` final tie-break in the
/// ready set, this is a strict total order — the foundation of the byte-identical schedule (D6).
fn decl_key(model: &Model, c: ConnectorId) -> (u32, u32) {
    let conn = &model.connectors[c.0 as usize];
    (
        model.blocks[conn.block.0 as usize].decl_order,
        conn.decl_order,
    )
}

/// Min-first ready set: pops the entry with the smallest `(key, id)`. A binary heap — **not** a
/// FIFO — so that among all simultaneously-ready vertices the sort always pops the lowest key
/// first. That single choice makes the schedule deterministic across equivalent models (§6.2
/// req 1); a FIFO would leak edge-insertion order into the schedule. `id` (the dense, unique vertex
/// index) is the final tie-break, so the order is total even when `key` values collide.
struct MinReady<K: Ord> {
    heap: BinaryHeap<Reverse<(K, u32)>>,
}

impl<K: Ord> MinReady<K> {
    fn with_capacity(n: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(n),
        }
    }

    fn push(&mut self, id: u32, key: K) {
        self.heap.push(Reverse((key, id)));
    }

    fn pop_min(&mut self) -> Option<u32> {
        self.heap.pop().map(|Reverse((_, id))| id)
    }
}

/// BUILD steps 2–3: produce the frozen [`Schedule`]. Runs the connector-level Kahn sort (for
/// `connector_order` + connector-level loop rejection) then the block-level Kahn sort (for the
/// tick `order` + block-granularity loop rejection).
///
/// # Errors
/// [`BuildError::AlgebraicLoop`] if the connector feedthrough graph has a cycle no loop-breaker
/// cuts; [`BuildError::BlockAlgebraicLoop`] if the connector graph is acyclic but atomic block
/// firing still cannot schedule it (mixed-feedthrough multi-output feedback).
pub fn topo_sort(
    dag: &FeedthroughDag,
    model: &Model,
    blocks: &[Box<dyn Block>],
) -> Result<Schedule, BuildError> {
    let connector_order = connector_order_kahn(dag, model)?;
    let order = block_order_kahn(model, blocks)?;
    // `driver_of` (the gather/alias map) is a separate BUILD step filled by `compile`; the sort
    // itself produces only the orderings.
    Ok(Schedule {
        order,
        connector_order,
        driver_of: Vec::new(),
    })
}

/// Connector-level Kahn sort over the direct-feedthrough DAG. Returns the connector evaluation
/// order, or [`BuildError::AlgebraicLoop`] if a feedthrough cycle remains (emitted `< n`).
fn connector_order_kahn(
    dag: &FeedthroughDag,
    model: &Model,
) -> Result<Vec<ConnectorId>, BuildError> {
    let mut indeg = dag.indeg.clone();
    let mut ready = MinReady::with_capacity(dag.n);
    for (v, &d) in indeg.iter().enumerate() {
        if d == 0 {
            ready.push(v as u32, decl_key(model, ConnectorId(v as u32)));
        }
    }

    let mut order = Vec::with_capacity(dag.n);
    while let Some(v) = ready.pop_min() {
        order.push(ConnectorId(v));
        for &w in &dag.succ[v as usize] {
            let d = &mut indeg[w.0 as usize];
            *d -= 1;
            if *d == 0 {
                ready.push(w.0, decl_key(model, w));
            }
        }
    }

    if order.len() < dag.n {
        let in_residual: Vec<bool> = indeg.iter().map(|&d| d > 0).collect();
        let succ: Vec<Vec<u32>> = dag
            .succ
            .iter()
            .map(|s| s.iter().map(|c| c.0).collect())
            .collect();
        let members = first_cycle(dag.n, &succ, &in_residual, |v| {
            (decl_key(model, ConnectorId(v)), v)
        })
        .into_iter()
        .map(|v| connector_path(model, ConnectorId(v)))
        .collect();
        return Err(BuildError::AlgebraicLoop { members });
    }

    Ok(order)
}

/// Block-level Kahn sort over the *emit-before* graph (block A → block B iff an output of A drives
/// an input that B reads at emit time). Returns the atomic-firing block order, or
/// [`BuildError::BlockAlgebraicLoop`] if the graph is cyclic (unschedulable by single-pass firing).
fn block_order_kahn(model: &Model, blocks: &[Box<dyn Block>]) -> Result<Vec<BlockId>, BuildError> {
    let nb = model.blocks.len();
    let mut succ: Vec<Vec<u32>> = vec![Vec::new(); nb];
    let mut indeg = vec![0u32; nb];
    for conn in &model.connections {
        if !emit_consumes(model, blocks, conn.to) {
            continue; // a fully-cut input creates no emit-before dependency (loop-breaker path).
        }
        let producer = model.connectors[conn.from.0 as usize].block.0;
        let consumer = model.connectors[conn.to.0 as usize].block.0;
        succ[producer as usize].push(consumer);
        indeg[consumer as usize] += 1;
    }

    let mut ready = MinReady::with_capacity(nb);
    for (b, &d) in indeg.iter().enumerate() {
        if d == 0 {
            ready.push(b as u32, model.blocks[b].decl_order);
        }
    }

    let mut order = Vec::with_capacity(nb);
    while let Some(b) = ready.pop_min() {
        order.push(BlockId(b));
        for &c in &succ[b as usize] {
            indeg[c as usize] -= 1;
            if indeg[c as usize] == 0 {
                ready.push(c, model.blocks[c as usize].decl_order);
            }
        }
    }

    if order.len() < nb {
        let in_residual: Vec<bool> = indeg.iter().map(|&d| d > 0).collect();
        let members = first_cycle(nb, &succ, &in_residual, |b| {
            (model.blocks[b as usize].decl_order, b)
        })
        .into_iter()
        .map(|b| block_path(model, BlockId(b)))
        .collect();
        return Err(BuildError::BlockAlgebraicLoop { members });
    }

    Ok(order)
}

/// Does block `B` (the block owning input connector `input`) read `input`'s value when emitting its
/// outputs? `[A]` blocks read every input at emit time (including output-less sinks). `[S]` blocks
/// read only inputs that feed through to some output — a fully-cut input (a loop-breaker's) is read
/// only by the deferred state update, never at emit, so it imposes no emit-before ordering.
fn emit_consumes(model: &Model, blocks: &[Box<dyn Block>], input: ConnectorId) -> bool {
    let b = model.connectors[input.0 as usize].block.0 as usize;
    let blk_impl = blocks[b].as_ref();
    if blk_impl.kind() == BlockKind::Algebraic {
        return true;
    }
    let blk = &model.blocks[b];
    match blk.inputs.iter().position(|&c| c == input) {
        Some(i) => (0..blk.outputs.len()).any(|j| blk_impl.feeds_through(i, j)),
        None => false,
    }
}

/// Find one concrete directed cycle within the residual induced subgraph of `(n, succ)`,
/// deterministically: DFS roots and successors are visited in `key` order, so the same graph always
/// reports the same cycle (§5 req 4). A residual subgraph (every member retains an unmet in-edge
/// from another member) always contains a reachable cycle, so the back-edge search succeeds. Shared
/// by both the connector-level and block-level loop diagnostics.
fn first_cycle<K: Ord>(
    n: usize,
    succ: &[Vec<u32>],
    in_set: &[bool],
    key: impl Fn(u32) -> K,
) -> Vec<u32> {
    // Residual successors of each residual vertex, sorted by key for a deterministic walk.
    let succ_sorted: Vec<Vec<u32>> = (0..n)
        .map(|v| {
            if !in_set[v] {
                return Vec::new();
            }
            let mut s: Vec<u32> = succ[v]
                .iter()
                .copied()
                .filter(|&w| in_set[w as usize])
                .collect();
            s.sort_by_key(|&w| key(w));
            s
        })
        .collect();

    let mut roots: Vec<u32> = (0..n as u32).filter(|&v| in_set[v as usize]).collect();
    roots.sort_by_key(|&v| key(v));

    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;
    let mut color = vec![WHITE; n];
    let mut pos_in_stack = vec![usize::MAX; n];

    for &root in &roots {
        if color[root as usize] != WHITE {
            continue;
        }
        // Iterative DFS (no recursion: large tangles must not overflow the stack). Each frame is
        // (vertex, next-successor index); `pos_in_stack` maps a gray vertex to its frame index.
        let mut stack: Vec<(u32, usize)> = Vec::new();
        color[root as usize] = GRAY;
        pos_in_stack[root as usize] = 0;
        stack.push((root, 0));
        while let Some(&(u, idx)) = stack.last() {
            let su = &succ_sorted[u as usize];
            if idx < su.len() {
                stack.last_mut().unwrap().1 = idx + 1;
                let w = su[idx] as usize;
                match color[w] {
                    WHITE => {
                        color[w] = GRAY;
                        pos_in_stack[w] = stack.len();
                        stack.push((w as u32, 0));
                    }
                    GRAY => {
                        // Back edge u → w: the cycle is the live stack from w through u.
                        let start = pos_in_stack[w];
                        return stack[start..].iter().map(|&(x, _)| x).collect();
                    }
                    _ => {}
                }
            } else {
                color[u as usize] = BLACK;
                pos_in_stack[u as usize] = usize::MAX;
                stack.pop();
            }
        }
    }

    // Unreachable for a genuine residual set; return the residual as a defensive, non-panicking
    // fallback so BUILD never aborts without a diagnostic.
    roots
}

/// Render a connector as a dotted path for diagnostics. Hand-built unnamed graphs are rendered from
/// the block class IRI and dense ids:
/// `<class-iri>#b<block>.<dir>#c<conn>`.
fn connector_path(model: &Model, c: ConnectorId) -> ConnectorPath {
    let conn = &model.connectors[c.0 as usize];
    let class = &model.blocks[conn.block.0 as usize].class_iri;
    let dir = if matches!(conn.dir, oce_model::Dir::In) {
        "in"
    } else {
        "out"
    };
    let block = conn.block.0;
    let cid = c.0;
    ConnectorPath(format!("{class}#b{block}.{dir}#c{cid}"))
}

/// Render a block as a dotted path for block-granularity diagnostics: `<class-iri>#b<block>`.
fn block_path(model: &Model, b: BlockId) -> ConnectorPath {
    let class = &model.blocks[b.0 as usize].class_iri;
    let id = b.0;
    ConnectorPath(format!("{class}#b{id}"))
}
