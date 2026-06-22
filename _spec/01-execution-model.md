# 01 — Execution Model: The Deterministic Dataflow Engine

> **Status:** spec section. Expands `00-overview-and-decisions.md` §8 row `01-execution-model.md`.
> Authored 2026-06-15 from the FRAME doc, `_research/cdl/01-language-core.md`,
> `_research/cdl/00-synthesis.md`, and `_research/00-integration-brief.md`.
> **Binding constraints inherited:** D1 (dual representation — `oce-model` is the executable truth
> on the tick), D6 (own Kahn topological sort in `oce-graph`, declaration-order tie-break,
> selene-free), D-OWNER-1 (the execution core has **zero** dependency on selene-db or any DB).
>
> **Scope.** This document specifies the deterministic dataflow engine that lives in `oce-graph`
> and operates entirely on the flat `oce-model`. It covers: the CDL §7.16 model of computation;
> the **BUILD phase** (direct-feedthrough DAG, algebraic-loop rejection, deterministic topological
> sort, state allocation); the **TICK phase** (stateful/stateless evaluation, time injection,
> host-chosen cadence); the sampling/clock + rising-edge mechanism for `CDL.Discrete`; and how
> loop-breaker blocks (`Pre`, integrators, samplers, `UnitDelay`) cut the DAG. It defines the core
> Rust types (`Schedule`, the eval loop, state slots).
>
> **This section is 100% selene-free.** No selene-db type, function, or concept appears anywhere in
> the engine described here. The single permitted touchpoint (a lock-free point read on the tick)
> is owned by `oce-api` / `oce-store`, NOT by `oce-graph`, and is described only as an interface
> obligation in §10.

---

## 1. The contract (CDL §7.16) and what the engine guarantees

CDL adopts the Modelica **synchronous data flow principle** plus the **single-assignment rule**.
The three principles (CDL §7.16, verbatim) translate to engine guarantees as follows:

1. *"All variables keep their actual values until these values are explicitly changed. Variable
   values can be accessed at any time instant."* → Every connector value **persists between ticks**.
   The engine never zeroes or invalidates a value; a value changes only when its driving block
   recomputes it in a tick.
2. *"Computation and communication at an event instant does not take time."* → A tick is
   **zero-time**: there is no intra-tick advance of `t`. All blocks in a single tick observe the
   same `t`. Signal propagation along connections is instantaneous within the tick.
3. *"Every input connector shall be connected to exactly one output connector."* → Single
   assignment. Enforced structurally by `oce-validate` (in-degree exactly 1 per input element, per
   CDL §7.10). The DAG builder in §4 **assumes** this precondition holds and is not responsible for
   re-checking it (it is a load error if violated).

Plus the acyclicity rule (CDL §7.16, verbatim):

> *"the dependency graph from inputs to outputs that directly depend on inputs shall be directed
> and acyclic. I.e., connections that form an algebraic loop are not allowed."*

And the loop-breaking rule (CDL §7.16, verbatim):

> *"To break an algebraic loop, one could place a delay block or an integrator in the loop, because
> the outputs of a delay or integrator does not depend directly on the input."*

**The determinism contract open-control sells:** identical inputs + identical parameters + identical
initial state ⇒ **bit-identical** outputs, independent of declaration order, connection order, or
host platform. This is what makes the engine a valid *executable specification* for commissioning
and verification (FRAME §1). Every design choice below — own deterministic Kahn sort (D6), frozen
schedule, allocation-stable tick, no hashing/IO/graph-walks on the hot path — exists to honor that
contract.

**Non-goal of this section.** Time-series persistence, semantic metadata, point lists, units, and
trends are **non-computational** (CDL §7.17) and the evaluator never reads them. They are out of
scope here and live in `oce-semantics` / `oce-store`.

---

## 2. The two phases

The engine has a strict two-phase lifecycle, mirroring the §7.16 separation between *structure*
(fixed at load) and *behavior* (evaluated per tick):

```
                  ┌─────────────────────── BUILD (once per model load, OFF the tick) ──────────────────────┐
  oce-model graph ─┤  1. direct-feedthrough DAG   2. algebraic-loop rejection                                │
  (flattened,     │  3. deterministic Kahn topo sort → frozen Schedule                                      │──► Schedule + State
   validated)     │  4. state allocation (seed [S] state slots from parameters)                             │     (immutable + mutable)
                  └────────────────────────────────────────────────────────────────────────────────────────┘
                  ┌─────────────────────── TICK (the hot path, host-chosen cadence) ──────────────────────┐
  inputs + t     ─┤  for each block in Schedule order:                                                      │──► outputs at t
  (+ prior state) │     [S] read prior state → emit output → update state                                   │     (+ next state)
                  │     [A] y = f(p, t, u)                                                                   │
                  └────────────────────────────────────────────────────────────────────────────────────────┘
```

- **BUILD** runs once after `oce-flatten` + `oce-validate` produce a monomorphic, single-assignment
  `oce-model` graph. It is allowed to allocate, hash, sort — it is *not* on the hot path.
- **TICK** is allocation-stable, branch-predictable, and runs the *frozen* schedule over flat arrays.
  No graph walks, no hashing, no allocation, no IO. The schedule is computed once and reused for
  every tick until the model is reloaded.

This is the structural realization of D1: `oce-model` (plus the `Schedule` and `RunState` derived in
BUILD) is the **executable truth on the tick**; the store, if present, is a durable projection
synced *off* the tick.

---

## 3. `oce-model` substrate the engine consumes (recap, normative for this section)

The engine operates on these `oce-model` concepts. They are produced by `oce-flatten`/`oce-cxf` and
validated by `oce-validate`; this section only specifies how `oce-graph` *uses* them. Type sketches
here are the engine-facing contract; the full type system lives in `02-type-system-and-values.md`.

```rust
// oce-model — pure types, NO selene-db, NO store.

/// Stable index newtypes (selene-free; these are NOT selene NodeIds — purely
/// in-memory, dense, 0-based positions in oce-model's own arenas).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct BlockId(pub u32);
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ConnectorId(pub u32);   // one per (block, connector, array-element)

/// Identifies an enumeration class in the flattened model's enum registry (see 02 §2.3).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct EnumClassId(pub u32);

/// A scalar CDL value. THE canonical definition lives in `02-type-system-and-values.md` §2.3
/// (oce-model owns it); reproduced here for engine-facing signature clarity, NOT redefined.
/// (Real=f64 per §7.4.1.1; Integer widened to i64 internally but honors the i32 range guarantee;
/// Boolean; String is metadata-only and never flows on the tick; Enum = 1-based ordinal + class.)
///
/// On the hot path the engine only ever observes the numeric/boolean subset (Real/Integer/Boolean);
/// `String` is metadata-only (§7.8 — no String connector) and `Enum` appears as a parameter, not a
/// signal — see 02 §2.3 and 03 §2.1 for the signal-value subset discussion.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Real(f64),
    Integer(i64),
    Boolean(bool),
    String(std::sync::Arc<str>),         // metadata/identifiers only — never a tick signal (§7.8)
    Enum { class: EnumClassId, ordinal: u32 },  // 1-based ordinal; EnumClassId lives in oce-model
}

/// Direction of a connector (String has no connector type per §7.8).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Dir { In, Out }

/// A resolved connector instance. Arrays are pre-flattened to one ConnectorId per
/// element (row-major), so the engine only ever sees scalars on the hot path.
pub struct Connector {
    pub id: ConnectorId,
    pub block: BlockId,
    pub dir: Dir,
    pub value_type: ValueType,   // Real | Int | Bool | Enum  (String is metadata-only)
    pub decl_order: u32,         // position in the source declaration (tie-break key, see §6)
}

/// A resolved, monomorphic block instance (elementary only on the tick — composites
/// are fully flattened away by oce-flatten before BUILD; §7.2/§7.15).
pub struct BlockInstance {
    pub id: BlockId,
    pub class_iri: String,       // join key to the native block impl in oce-blocks
    pub inputs:  Vec<ConnectorId>,
    pub outputs: Vec<ConnectorId>,
    pub params:  ParamTable,     // ground (or symbolic-resolved) parameter values
    pub decl_order: u32,         // declaration order of the instance (tie-break key, see §6)
}

/// A single-assignment connection: exactly one output drives one input element (§7.10).
pub struct Connection {
    pub from: ConnectorId,       // an Out connector
    pub to:   ConnectorId,       // an In  connector (in-degree exactly 1, validated upstream)
}

/// The flattened model the engine schedules and ticks. This is the **scheduling-relevant view** of
/// the canonical in-memory `oce_model::ModelGraph` (defined in full in `04-cxf-ingest.md` §7 — it
/// additionally carries `root`, `params`, and the load-bearing `iri_index: IRI → node id`, FRAME §2).
/// `Model` and `ModelGraph` name the **same** in-memory artifact (D1's executable truth on the tick);
/// `oce-graph` consumes `ModelGraph` and reads its scheduling fields (`blocks`/`connectors`/edges)
/// exactly as sketched here. The CXF importer (04) and store projection (06) name it `ModelGraph`;
/// this section uses the short `Model` for the engine-facing subset. (`Connection` ≡ `ModelGraph`'s
/// `Edge`: both are an output→input dataflow edge.) Do **not** treat `Model` as a third distinct type.
///
/// `oce_store::ResolvedModel` (06) is a SEPARATE, serializable **DTO projection** of `ModelGraph` for
/// durable storage — the store direction of D1, not the tick artifact. The scheduler never consumes
/// `ResolvedModel`; the conversion is `ModelGraph` (in-memory truth) → `ResolvedModel` (store DTO).
pub struct Model {
    pub blocks:      Vec<BlockInstance>,   // indexed by BlockId.0
    pub connectors:  Vec<Connector>,       // indexed by ConnectorId.0
    pub connections: Vec<Connection>,      // == ModelGraph::edges (output → input)
}
```

**Invariants the engine relies on (guaranteed by `oce-flatten`/`oce-validate`, not re-checked on the
hot path):**

1. The model is **monomorphic and fully flattened**: every `BlockInstance` is elementary (CDL §7.2);
   composites and `replaceable`/`redeclare`/`extends` are resolved and stripped before BUILD (CDL
   §7.14/§7.15; "they are not present in CXF").
2. **Single assignment** holds: every `In` connector (every array element) has in-degree exactly 1
   (CDL §7.10). Conditionally-removed inputs (CDL §7.7.4) have already been bound to a default
   constant block or removed.
3. Connection endpoints are **type-matched** (Real↔Real, Int↔Int, Bool↔Bool; no implicit coercion —
   explicit `CDL.Conversions` blocks; CDL §7.10).
4. Arrays are **flattened to scalar `ConnectorId`s** (row-major, 1-based CDL indices mapped to
   0-based internal positions; CDL §7.4.4). The hot path is scalar-only.

---

## 4. BUILD phase, step 1 — the direct-feedthrough DAG

The dependency graph is **not** the connection graph. It is the *direct-feedthrough* graph: the
graph of which block outputs must be computed before which other block inputs can be read **within a
single tick**. It has two edge sources.

### 4.1 The two edge kinds

1. **Connection edges (output → input), one per `Connection`.** A connection `connect(a.y, b.u)`
   means `b.u` cannot be read until `a.y` is produced. Edge: `a.y → b.u` (data flows along the
   connection; the consumer depends on the producer).

2. **Block-internal input → output edges, ONLY where the output algebraically depends on that
   input.** For each block, for each (input `u_i`, output `y_j`) pair, add an edge `u_i → y_j`
   **iff** `y_j` directly (algebraically, in zero time) depends on `u_i` in this tick. This is the
   crux of CDL §7.16: stateful loop-breaker blocks have outputs that depend on **prior state**, not
   the current input, so they emit **no** internal input→output edges and thereby cut the graph.

The DAG is built over **connector vertices** (`ConnectorId`), not block vertices, because
feedthrough is per-(input,output)-pair, not per-block. (A block can be transparent on one
input→output path and opaque on another — see the multi-port cases in §4.3.) Block-level evaluation
order is then recovered from the connector ordering in §6.

### 4.2 The feedthrough oracle (`oce-blocks` ⨯ `oce-graph` seam)

`oce-graph` cannot itself know whether a given block's output algebraically depends on a given input;
that is *per-block-class* knowledge owned by `oce-blocks`. The `Block` trait (full definition in
`03-block-library.md`) therefore exposes a **feedthrough matrix**:

```rust
// oce-blocks — the Block trait surface the scheduler needs (subset; see 03-block-library.md).
pub trait Block {
    /// Is this block stateless [A] (`y = f(p,t,u)`) or stateful [S] (`y = f(p,t,u,x)`)? (§7.2/§7.6)
    fn kind(&self) -> BlockKind;   // Algebraic | Stateful

    /// Direct-feedthrough oracle. Returns true iff output `out_idx` algebraically (zero-time,
    /// same-tick) depends on input `in_idx`. This is what cuts the DAG for loop-breakers.
    ///
    /// Contract:
    ///  * Pure [A] blocks: typically true for every (in,out) that participate in `y=f(...,u)`.
    ///  * Loop-breaker [S] blocks (Pre, integrators, samplers, UnitDelay): MUST return false for
    ///    the state-bearing path (output derives from prior state x(t), not current u).
    ///  * Parameter-dependence is irrelevant here (params are not connectors).
    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool;
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BlockKind { Algebraic, Stateful }
```

> **Design note (binding).** `feeds_through` is a **static, parameter-independent** property of the
> block *class* in v1. A block whose feedthrough structure changes with a parameter (e.g. a
> comparison block that is `[A]` when hysteresis `h=0` but `[S]` when `h>0`, per the block-library
> backlog) MUST expose its **post-flatten, parameter-resolved** feedthrough — i.e. the
> `Block` instance handed to BUILD already knows its `h` and reports `kind()`/`feeds_through()`
> accordingly. The oracle is queried on the **instantiated** block, never on the bare class.

### 4.3 Per-class feedthrough rules (normative examples)

| Block (class) | Kind | `feeds_through(in,out)` | DAG effect |
|---|---|---|---|
| `CDL.Reals.Add`, `Multiply`, `Limiter`, `Switch`, `Line`, `MultiSum`, all `CDL.Conversions.*`, `CDL.Logical.And/Or/Not`, `CDL.Routing.*` | **[A]** | `true` for every (in,out) on the math path | transparent — emits internal edges |
| `CDL.Reals.Sources.Constant`, `CDL.Integers.Sources.Constant`, `CDL.Logical.Sources.Constant` | **[A]** | n/a (no inputs) | source root — pure out-edge origin |
| `CDL.Reals.Switch` | **[A]** | `true` for all three inputs `(u1, u2, u3)` → `y` | transparent |
| `CDL.Logical.Pre` | **[S]** | **`false`** (output = prior input via 1-tick memory) | **CUTS** — canonical algebraic-loop breaker |
| `CDL.Discrete.UnitDelay`, `Sampler`, `ZeroOrderHold`, `FirstOrderHold`, `TriggeredSampler`, `TriggeredMax`, `TriggeredMovingMean` | **[S]** | **`false`** on the sampled/held path (output is the prior held sample) | **CUTS** |
| `CDL.Reals.IntegratorWithReset` | **[S]** | **`false`** w.r.t. the integrand `u` (output is the prior accumulated state); the `trigger`/`y_reset_in` inputs also do **not** feed through within the tick — reset takes effect via state update | **CUTS** (w.r.t. `u`) |
| `CDL.Integers.OnCounter` | **[S]** | **`false`** (output is purely accumulated count) | **CUTS** |
| `CDL.Reals.Derivative` (filtered), `MovingAverage`, `LimitSlewRate` | **[S]** | **`true`** on `y ← {u}` (the current input does feed through: a filtered derivative, a window that includes the current sample, and a rate-bounded tracker all change combinationally with `u` this tick) | **transparent** (stateful but NOT a cut) |
| `CDL.Reals.PID`, `PIDWithReset` | **[S]** | **`true`** on `y ← {u_s, u_m}` (the proportional term is instantaneous; only the integral/derivative are state). The `trigger`/`y_reset_in` reset inputs of `PIDWithReset` do not feed through (reset via state update). | **transparent** (stateful but NOT a cut) |
| `CDL.Reals.Greater/Less/*Threshold` with hysteresis `h>0`, `Hysteresis` | **[S]** | **`true`** (output can change combinationally with the input crossing a threshold; the prior bool only holds *inside* the band) | **transparent** (stateful but NOT a cut) |
| `CDL.Reals.Greater/Less/*Threshold` with `h=0` (fast path) | **[A]** | `true` | transparent |
| `CDL.Logical.Timer`, `TimerAccumulating`, `TrueDelay`, `TrueFalseHold`, `TrueHoldWithReset`, `Latch`, `Toggle`, `Edge`, `FallingEdge`, `Change` | **[S]** | **`true`** on `y ← {u, …}` (edge/timer/latch outputs are current-vs-prior or pass a current transition through — they carry state but feed through) | **transparent** (stateful but NOT a cut) |

> **Statefulness ≠ loop-cut (binding for v1).** A block being `[S]` does **not** make it a loop cut.
> A cut exists only on an output that is computed **entirely** from prior state `x(t)`, with **no**
> algebraic dependence on any current input. A block whose output has *both* a feedthrough term and a
> state term on the **same output** (e.g. PID's `P` term + integral state, or a hysteretic comparator
> whose output flips when the input crosses a threshold) genuinely **feeds through** and MUST report
> `feeds_through == true` for that (input, output) pair. The true cuts in v1 are therefore restricted
> to the genuinely state-only-output blocks: `Logical.Pre`,
> `Discrete.{UnitDelay, Sampler, ZeroOrderHold, FirstOrderHold, TriggeredSampler, TriggeredMax,
> TriggeredMovingMean}`, `Reals.IntegratorWithReset` (w.r.t. its integrand `u`), and
> `Integers.OnCounter`. This is identical to the loop-cut list in `03-block-library.md` §5; the two
> documents MUST stay in lockstep.

> **Why mis-cutting in either direction is unsafe.** The feedthrough oracle is the single input that
> decides which connection cycles are rejected as algebraic loops (§5). Reporting `feeds_through ==
> false` when the true answer is `true` **removes a real internal edge**: for a block that genuinely
> feeds through (PID's P-term, a threshold-crossing comparator), this can let the Kahn pass **accept**
> a model whose P-only / threshold feedback path is a true algebraic loop — i.e. it **under-rejects**,
> the dangerous direction §7.16 forbids. Reporting `feeds_through == true` when the true answer is
> `false` (under-cutting) is the opposite hazard: it can fabricate a cycle and falsely reject a valid
> model. The oracle MUST therefore report the block class's **true** post-flatten feedthrough
> structure — neither over- nor under-cut — matching the per-class table above and `03` §3/§5.

### 4.4 DAG construction algorithm

```rust
// oce-graph — BUILD step 1. Selene-free; operates only on oce-model + the oce-blocks oracle.

/// The direct-feedthrough dependency graph over connector vertices.
/// Stored as adjacency lists in dense ConnectorId space (0-based).
pub struct FeedthroughDag {
    pub n: usize,                          // number of connector vertices
    pub succ: Vec<Vec<ConnectorId>>,       // succ[v] = vertices that depend on v
    pub indeg: Vec<u32>,                   // in-degree of each vertex (for Kahn)
    pub edge_decl_key: Vec<EdgeKey>,       // stable tie-break key per emitted edge order
}

pub fn build_feedthrough_dag(
    model: &Model,
    blocks: &[Box<dyn Block>],   // instantiated, parameter-resolved; index == BlockId.0
) -> Result<FeedthroughDag, BuildError> {
    let mut dag = FeedthroughDag::with_capacity(model.connectors.len());

    // (a) Connection edges: output -> input, deterministic over connection order which is
    //     itself derived from declaration order (see §6) so the result is order-independent.
    for c in &model.connections {
        dag.add_edge(c.from, c.to);        // producer must precede consumer
    }

    // (b) Block-internal input -> output edges, only where feeds_through == true.
    for blk in &model.blocks {
        let impl_ = &blocks[blk.id.0 as usize];
        for (i, &u) in blk.inputs.iter().enumerate() {
            for (j, &y) in blk.outputs.iter().enumerate() {
                if impl_.feeds_through(i, j) {
                    dag.add_edge(u, y);    // input must be ready before this output is computed
                }
            }
        }
    }
    Ok(dag)
}
```

The DAG is iterated in **deterministic order** in both (a) and (b): connections in their resolved
declaration order, blocks in `BlockId` order (which equals declaration order, §6), inputs/outputs in
declared connector order. This makes the adjacency lists themselves order-stable, which the Kahn
sort in §6 relies on for its tie-break.

---

## 5. BUILD phase, step 2 — algebraic-loop detection and **hard rejection**

CDL §7.16 mandates that the direct-feedthrough graph "shall be directed and acyclic" and that
"connections that form an algebraic loop are not allowed." open-control implements this as a
**hard error at load** — there is no fixed-point iteration, no tearing, no implicit solver. The only
legal feedback is one routed through a loop-breaker block (§4.3), which cuts the graph by emitting no
feedthrough edge.

Detection is folded into the Kahn topological sort itself (§6): Kahn's algorithm processes vertices
in in-degree order; if it terminates having emitted fewer than `n` vertices, the un-emitted vertices
**are exactly the strongly-connected feedback set** — they form one or more algebraic loops.

```rust
#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error("algebraic loop detected: {} connectors form a cycle that is not broken by a \
             state-holding block (Pre/integrator/sampler/UnitDelay). CDL §7.16 forbids algebraic \
             loops. Cycle members: {members:?}", .members.len())]
    AlgebraicLoop { members: Vec<ConnectorPath> },   // human-readable dotted paths for diagnostics
    // ... other build errors
}
```

**Requirements (numbered, normative):**

1. After the Kahn pass, if `emitted_count < dag.n`, raise `BuildError::AlgebraicLoop`. This is a
   `shall`-level (error) diagnostic per the §7.6 two-tier diagnostic policy — **never** a warning,
   **never** auto-broken.
2. The error MUST report the **dotted instance/connector paths** of the cycle members (recovered
   from `oce-model` connector metadata), so a sequence author can locate the offending loop. Prefer
   reporting the **minimal cycle** (run a single DFS over the residual subgraph of un-emitted
   vertices to extract one concrete back-edge path) rather than the whole residual set, for
   actionable diagnostics.
3. The diagnostic MUST name the canonical remedy verbatim from §7.16: insert a delay block (`Pre`,
   `UnitDelay`) or an integrator into the loop, because their outputs do not depend directly on the
   input.
4. Detection MUST be deterministic: the *same* model always reports the *same* cycle members in the
   *same* order (driven by the declaration-order traversal of §6).

> **Why no tearing/iteration.** open-control is an *executable specification*, not a general
> equation solver. CDL deliberately forbids algebraic loops so that a single topological pass
> computes every signal exactly once per tick in zero time. Introducing iterative loop-solving would
> break the bit-stable determinism contract (iteration order, convergence tolerance, and platform FP
> would all leak into outputs) and is explicitly disallowed by §7.16. **Reject, do not solve.**

---

## 6. BUILD phase, step 3 — deterministic topological sort (D6, own Kahn)

Per **D6**, `oce-graph` owns its topological sort: **Kahn's algorithm with a declaration-order
tie-break**, implemented in-crate, **selene-free** (no `selene-algorithms`, no `rayon`,
no `GraphProjection`). It is cross-checked against a self-contained reference oracle only in CI (D6
recommendation (c)), never linked into the engine.

### 6.1 Why declaration-order tie-break

Kahn's algorithm repeatedly removes a source (in-degree 0) vertex. When multiple sources are ready
simultaneously, the choice is free for *correctness* but must be **fixed** for *determinism* (CDL
§7.10 requires the evaluation order to be independent of declaration/connection order in its
*observable result*, but the engine must still pick a single concrete order for bit-stable traces).
open-control breaks ties by **declaration order**: the `decl_order` field on blocks/connectors,
assigned at flatten time from the source/CXF ordering. This gives a total, reproducible order.

> **Subtlety.** "Independent of connection order" (§7.10) is about the *result* being invariant — and
> it is, because the DAG encodes the true data dependencies regardless of how connections were
> listed. The *schedule* (the chosen linearization among valid ones) is fixed by `decl_order` so that
> golden traces are bit-stable. Two models that differ only in the textual order of `connect`
> statements produce the **same DAG** and therefore the **same schedule**; two models that differ in
> *declaration* order of instances may produce different (but equally valid) schedules — which is why
> goldens are keyed to a specific flattened model, per the conformance strategy.

### 6.2 The sort

> **Public `oce-graph` API names (normative, single source of truth).** The frozen schedule type is
> `oce_graph::Schedule`; the public BUILD entry point that produces it is `oce_graph::compile(...)`
> (which internally runs `build_feedthrough_dag` + `topo_sort` + `allocate_state`); the public TICK
> entry point is `oce_graph::eval_tick(...)` operating on an `EvalContext` (§9). `06`/`08` refer to the
> schedule as `CompiledSchedule` and to the tick as `run_tick` — these are **aliases for the same
> `Schedule` type and `eval_tick` function**; all sections MUST use this one set. The tick entry point
> takes the mutable `RunState`, the frozen `Schedule`, the `ModelGraph`, the block impls, and the
> staged inputs/`t_now` (the `EvalContext` fields of §9) — never a 3-argument form.

```rust
// oce-graph — BUILD step 3. Own Kahn impl per D6. Deterministic. Selene-free.

/// The frozen per-tick evaluation order. Computed once in BUILD, reused every tick.
/// Public alias: `CompiledSchedule` (used in 06/08) == this `Schedule`.
pub struct Schedule {
    /// Blocks in topological evaluation order. Each block's inputs are guaranteed
    /// computed before the block fires. Stored as a flat Vec for cache-friendly iteration.
    pub order: Vec<BlockId>,
    /// Connector evaluation order (the underlying connector-level toposort) retained for
    /// trace tooling / debugging. The hot loop uses `order` (block granularity).
    pub connector_order: Vec<ConnectorId>,
}

pub fn topo_sort(
    dag: &FeedthroughDag,
    model: &Model,
) -> Result<Schedule, BuildError> {
    let mut indeg = dag.indeg.clone();
    // Ready set: a *binary heap / sorted structure keyed by decl_order*, NOT a FIFO queue,
    // so that among simultaneously-ready vertices we always pop the lowest decl_order first.
    let mut ready = DeclOrderHeap::new();
    for v in 0..dag.n {
        if indeg[v] == 0 {
            ready.push(ConnectorId(v as u32), decl_key(model, ConnectorId(v as u32)));
        }
    }

    let mut connector_order = Vec::with_capacity(dag.n);
    while let Some(v) = ready.pop_min() {          // deterministic: min decl_order among ready
        connector_order.push(v);
        for &w in &dag.succ[v.0 as usize] {
            indeg[w.0 as usize] -= 1;
            if indeg[w.0 as usize] == 0 {
                ready.push(w, decl_key(model, w));
            }
        }
    }

    if connector_order.len() < dag.n {
        // Residual vertices form one or more algebraic loops (§5).
        return Err(build_algebraic_loop_error(dag, &indeg, model));
    }

    // Collapse connector order to a stable block order: a block fires when the LAST of its
    // outputs becomes scheduled (all its inputs are by then ready). Preserve first-appearance
    // order, dedup by BlockId, tie-break by block decl_order.
    let order = collapse_to_block_order(&connector_order, model);
    Ok(Schedule { order, connector_order })
}

/// Tie-break key: the declaration order of the connector's owning block, then the connector's
/// own declaration order. Total, reproducible, platform-independent.
fn decl_key(model: &Model, c: ConnectorId) -> (u32, u32) {
    let conn = &model.connectors[c.0 as usize];
    (model.blocks[conn.block.0 as usize].decl_order, conn.decl_order)
}
```

**Requirements (numbered, normative):**

1. The ready-set MUST be ordered by `decl_key` (block declaration order, then connector declaration
   order) — a `BinaryHeap` with a reversed comparator or an explicitly sorted bucket — **not** a
   plain FIFO/LIFO. A FIFO would make the schedule depend on edge-insertion order, breaking
   reproducibility across equivalent models.
2. The sort MUST be `O(V + E)` (standard Kahn) and run entirely on the flat `FeedthroughDag` arrays.
   No external graph-library `GraphProjection`, no CSR build, no `rayon`. (D6.)
3. The resulting `Schedule.order` is **frozen**: stored in the engine handle and reused verbatim for
   every tick until model reload. It MUST NOT be recomputed per tick.
4. Block-level collapse: a `BlockInstance` is emitted into `Schedule.order` at the position where its
   *last* output connector is finalized in `connector_order`; this guarantees all of the block's
   inputs are computed before the block fires. Deduplicate by `BlockId`, preserving first stable
   position.
5. **CI cross-check (D6 (c), not in the engine):** a test harness builds a **self-contained reference
   oracle** (a small, in-tree topological-sort verifier with **no external graph-algorithm
   dependency**) and asserts the *set* of valid topological orders contains open-control's order, and
   that open-control's order is a valid linearization. This lives in test code under `#[cfg(test)]` of a
   test-only crate — and pulls **no** external graph crate into `oce-graph`'s dependency graph.

---

## 7. BUILD phase, step 4 — state allocation seeded from parameters

CDL has **no `start` attribute** and **no `initial equation`/`initial algorithm`** (CDL §7.4.1
omissions; §7.3 prohibitions). Initial state for every `[S]` block is therefore seeded **from the
block's parameters** (the synthesis: "seed it from initial-state **parameters** (no `start`
attribute exists)").

The engine allocates one **state slot** per `[S]` block, laid out as a flat, contiguous buffer
(`RunState`) for cache locality and allocation-stable ticks.

```rust
// oce-graph — BUILD step 4 + the mutable run-time state.

/// Opaque, fixed-size, per-instance persisted state x(t). Each [S] block class defines its own
/// layout (e.g. an integrator carries one f64; a Timer carries an f64 elapsed + a bool armed).
/// State is a flat byte/word region indexed by a per-block offset, so the tick touches no heap.
pub struct StateSlot {
    pub block: BlockId,
    pub offset: usize,    // start of this block's region within RunState.words
    pub len: usize,       // length of the region
}

/// All mutable execution state. ONE per running model instance. Owned by the Engine handle.
pub struct RunState {
    /// Flat connector-value array, indexed by ConnectorId.0. This is "the values that persist
    /// between ticks" (§7.16 principle 1). Never cleared between ticks.
    pub values: Vec<Value>,
    /// Flat per-[S]-block state words. Indexed via StateSlot offsets.
    pub words: Vec<u64>,            // reinterpreted per-block (f64::to_bits, bools, counters)
    /// Slot directory (immutable after BUILD).
    pub slots: Vec<StateSlot>,
    /// Current model time t (seconds). Advanced by the host, read by elementary blocks (§8).
    pub t: f64,
}

pub fn allocate_state(
    model: &Model,
    blocks: &mut [Box<dyn Block>],
) -> RunState {
    let mut words = Vec::new();
    let mut slots = Vec::new();
    for blk in &model.blocks {
        let impl_ = &mut blocks[blk.id.0 as usize];
        if impl_.kind() == BlockKind::Stateful {
            let len = impl_.state_len();             // class-defined word count
            let offset = words.len();
            words.resize(offset + len, 0);
            // SEED from parameters — NOT from a `start` attribute (none exists in CDL).
            impl_.init_state(&mut words[offset..offset + len], &blk.params);
            slots.push(StateSlot { block: blk.id, offset, len });
        }
    }
    let values = init_values(model);   // seed source/constant outputs; others 0 of their type
    RunState { values, words, slots, t: 0.0 }
}
```

**Requirements (numbered, normative):**

1. Every `[S]` block allocates a fixed-size state region at BUILD; sizes come from
   `Block::state_len()` and never change at run-time (arrays are fixed-size, CDL §7.4.4). The tick
   performs **zero allocation**.
2. Initial state MUST be derived **only** from the block's resolved parameters via
   `Block::init_state(&mut region, &params)`. There is no implicit `0` default unless the reference
   block's initial-state parameter defaults to a value yielding `0` (e.g. an integrator's
   `y_start` parameter). The seed value is part of the conformance contract (Tier 0 golden tests
   cover stateful blocks "with explicit initial-state parameters").
3. Connector `values` are initialized so that **constant/source** outputs hold their value before the
   first tick (CDL §7.16 principle 1 — values exist and persist), and all other connectors hold the
   type-appropriate zero (`Real(0.0)`, `Integer(0)`, `Boolean(false)`). Signal connectors carry only
   the Real/Integer/Boolean subset (§7.8 — no String/Enum connector). This matters because of the
   prior-state read order in §9 (a downstream block on the first tick may read a loop-breaker output
   before the loop-breaker has run).
4. `RunState` is the **sole mutable structure** on the tick. `Model`, the `Schedule`, the `StateSlot`
   directory, and the `Box<dyn Block>` *layouts* are immutable after BUILD. (Block *internal* mutable
   scratch, if any, must live inside the `words` region, not in the `Block` object, to keep the
   schedule shareable — see §11 threading note.)

---

## 8. Time: internal to elementary blocks, cadence chosen by the host

CDL has **no user-facing `time` variable and no time connector** (CDL §7.6/§7.8). Time `t` enters
**only** implicitly via the elementary-block mapping `(p, t, u(t), x(t)) → (y(t), x'(t))` (CDL §7.6).
Sources (`Constant` excepted), samplers, timers, and discrete blocks consume `t`; pure arithmetic /
logic / routing blocks ignore it.

**The host chooses the cadence.** CDL §7.16 specifies *principles* (synchronous data flow, zero-time
events, acyclic feedthrough) but "does **not** mandate a specific time step, scheduler, or
event-handling algorithm." open-control therefore:

1. Treats `t` as a `f64` seconds value carried in `RunState.t`, advanced **by the host** between
   ticks (the `oce-api` facade exposes the tick call; see `08-embeddable-api-and-performance.md`).
2. Passes `t` into every block's `step` (§9). `[A]` blocks that don't use it ignore it; time-aware
   blocks read it.
3. Imposes **no fixed step internally.** The engine is correct at any cadence the host picks (it is
   a synchronous-dataflow evaluator, not an ODE integrator).

**Reference cadence (default, advisory, not normative for the engine):** **60–120 s** for HVAC
supervisory control loops (the synthesis verification-cadence reference). This is a documentation
default surfaced to hosts and the conformance harness; the engine itself accepts any monotonic `t`.

```rust
// The host's per-tick contract on the Engine handle (defined in oce-api, shown here for the seam).
impl Engine {
    /// Advance to absolute model time `t_now` (seconds), apply staged inputs, evaluate one tick.
    /// `t_now` MUST be >= the previous tick's t (monotonic, non-decreasing). The host owns cadence.
    pub fn tick(&mut self, t_now: f64) -> Result<&Outputs, OcError> { /* see §9 */ }
}
```

**Requirements (numbered, normative):**

1. `t` MUST be monotonic non-decreasing across ticks. The engine MAY assert this in debug builds;
   a decreasing `t` is a host error (it would make timers/samplers ill-defined).
2. The engine MUST NOT embed a wall-clock or a fixed step. All time-derived behavior is a pure
   function of the `t` the host supplies and the block's prior state.
3. Two-source-of-truth ban: there is exactly one `t` per tick (in `RunState.t`); all blocks in a tick
   observe the identical value (zero-time, §7.16 principle 2). Blocks MUST NOT read any other clock.
4. Δt for integrating/filtering blocks is computed **inside** the block as `t_now − t_prev`, where
   `t_prev` is part of the block's persisted state (`words`). The scheduler does not compute Δt; it
   only supplies absolute `t`. This keeps integration semantics local to each block and conformant to
   the reference implementation (which derives step from the simulation clock). On the **first tick**
   there is no prior model time, so blocks persist a sentinel (`PREV_T_UNSET = u64::MAX`, a reserved
   raw-bits value distinct from any valid `f64` model time, set in `init_state`); while present the
   block uses **Δt = 0** (no integration/decay on the first tick) and overwrites it with `t_now` in
   `update_state`. The sentinel + Δt helpers live once in `oce-blocks/src/dynamics.rs` (`tick_dt`,
   `PREV_T_UNSET`), keeping first-tick semantics identical across PID, Derivative, IntegratorWithReset,
   LimitSlewRate, and MovingAverage.
5. **Bounded history under the no-fixed-step law (R-IMPL-8a in `03`).** Because requirement 2 forbids a
   fixed internal step and `state_len` is fixed at `init` (Δt unknown then), a block that needs a
   **time-horizon window** (`Reals.MovingAverage` over `delta`; `Discrete.TriggeredMovingMean`) cannot
   size unbounded history. Such blocks use a **fixed-capacity `(t, value)` checkpoint ring** in their
   state region (interpolating the delayed value). On overflow it does a **graceful correct degrade**:
   `Diagnostics` **warning** (emitted **once per instance**, not per overflowing tick) + drop-oldest +
   **divide by the actual retained span** (not the requested horizon) so the result stays a correct mean
   over the shorter window — never reallocating (R-IMPL-8), panicking, or under-scaling. The denom's
   division-safety floor is the param floor `MIN_DELTA`, **not** a coarser physical constant (a coarser
   floor silently under-scales a legal sub-floor horizon). The capacity is a fixed engine constant, not a
   CDL/CXF parameter (OD-MA, `12`).

---

## 9. TICK phase — the eval loop

A tick evaluates every block exactly once, in `Schedule.order`, observing the §7.16 semantics:
`[S]` blocks emit output from **prior** state then update state; `[A]` blocks compute
`y = f(p, t, u)`. Within the tick all reads/writes happen against the single flat `RunState`.

```rust
// oce-graph — the hot path. Allocation-free, IO-free, hashing-free, selene-free.

pub struct EvalContext<'a> {
    pub model: &'a Model,
    pub schedule: &'a Schedule,
    pub blocks: &'a mut [Box<dyn Block>],   // instantiated impls, indexed by BlockId.0
    pub state: &'a mut RunState,
}

/// Evaluate exactly one tick at absolute time `t_now`. Inputs to the graph (external/host-driven
/// connectors) must already be staged into `state.values` before calling.
pub fn eval_tick(ctx: &mut EvalContext, t_now: f64) {
    ctx.state.t = t_now;

    for &bid in &ctx.schedule.order {
        let blk = &ctx.model.blocks[bid.0 as usize];
        let impl_ = &mut ctx.blocks[bid.0 as usize];

        // 1. Gather inputs by reading the (already-computed-this-tick, or persisted-from-last-tick)
        //    connector values. Producers precede consumers in schedule order, EXCEPT across a
        //    loop-breaker cut, where the consumer legitimately reads last tick's value.
        let inputs = gather_inputs(blk, &ctx.state.values);   // &[Value], borrow from flat array

        match impl_.kind() {
            BlockKind::Algebraic => {
                // y = f(p, t, u). No state.
                impl_.step_algebraic(&inputs, t_now, &mut |out_idx, v| {
                    ctx.state.values[blk.outputs[out_idx].0 as usize] = v;
                });
            }
            BlockKind::Stateful => {
                // §7.16: emit output from PRIOR state, THEN update state.
                let slot = state_slot_for(ctx.state, bid);
                let region = &mut ctx.state.words[slot.offset..slot.offset + slot.len];
                // (a) output from prior state x(t): y(t) = g(p, t, u, x_prior)
                impl_.emit_from_state(&inputs, t_now, region, &mut |out_idx, v| {
                    ctx.state.values[blk.outputs[out_idx].0 as usize] = v;
                });
                // (b) update state: x' = h(p, t, u, x_prior)  (writes back into `region`)
                impl_.update_state(&inputs, t_now, region);
            }
        }
    }
}

/// After eval, propagate connector values along connections so consumers see producer outputs.
/// (Equivalently, fold the copy into gather_inputs — see note below.)
```

> **Connection propagation — copy vs alias (implementation choice, must be deterministic).** A
> connection `connect(a.y, b.u)` means `b.u` *is* `a.y`. Two equivalent realizations:
> (i) **alias** — `gather_inputs` for `b` reads the producer connector `a.y` directly via a
> precomputed `to → from` resolution table (built in BUILD), so no copy is needed and the input
> array is a gather of producer-output slots; or (ii) **copy** — after a producer fires, copy its
> output into each fanned-out input slot. v1 SHALL use **(i) alias/gather**: build a
> `Vec<ConnectorId>` `driver_of[input] = output` map in BUILD (single-assignment guarantees exactly
> one driver per input), and `gather_inputs` reads `state.values[driver_of[input]]`. This avoids
> per-tick copies and makes the value array hold only *outputs and external inputs* as the source of
> truth. Because producers precede consumers in `Schedule.order` (except across cuts), the gathered
> value is the current tick's for feedforward paths and last tick's for the broken-feedback path —
> exactly the §7.16 semantics.

**Requirements (numbered, normative):**

1. Each block is evaluated **exactly once per tick**, in `Schedule.order`. No block is skipped, none
   is evaluated twice (single-pass; no fixed-point iteration — §5).
2. For `[S]` blocks the **output is computed from prior state first** (`emit_from_state`), and **only
   then is state updated** (`update_state`). This ordering is what makes feedback well-defined
   (§7.16): a downstream consumer that closes the loop reads the loop-breaker's *prior*-state output,
   produced before the loop-breaker advances its state this tick.
3. For `[A]` blocks the output is a pure function of `(params, t, inputs)`; the block holds no state
   and must produce identical output for identical `(p, t, u)`.
4. The tick performs **zero heap allocation**, **zero hashing**, **zero IO**, and touches **no store**.
   The only memory written is `RunState.values` (outputs) and `RunState.words` (updated `[S]` state).
   This is the latency/determinism floor that D1 + the §0/§5 performance strategy require.
5. **Determinism:** for a fixed `Schedule`, fixed `params`, fixed initial `RunState`, and a fixed
   sequence of `(t, staged inputs)`, the sequence of outputs is **bit-identical** across runs and
   platforms. (FP determinism: the engine relies on IEEE-754 double semantics and avoids
   nondeterministic FP reorderings — no `rayon`, no SIMD autovectorization that reassociates,
   compile with stable FP flags. This is a Group-A correctness obligation, cross-checked by the
   conformance harness funnel.)
   Real-valued outputs also pin the IEEE-underspecified cases that affect `to_bits()`:
   every NaN emitted as a `Real` is canonicalized to `0x7ff8_0000_0000_0000`; Real min/max use the
   project convention matching IEEE-754-2019 `minimumNumber`/`maximumNumber` and arm64 FMIN/FMAX
   (`min(+0.0, -0.0) = -0.0`, `max(+0.0, -0.0) = +0.0`); and every Real emit boundary applies the
   NaN canonicalization backstop. Modelica treats `+0.0` and `-0.0` as equal values here, so this is
   an open-control bit-reproducibility convention, not an additional CDL semantic distinction.
6. **First-tick read of an un-fired loop-breaker:** because a broken-feedback consumer is scheduled
   *before* its loop-breaker producer in some linearizations, on tick 0 it reads the producer's
   *initialized* value (§7.4 seed). This is correct and matches the reference (the loop-breaker's
   output before its first update is its initial state).

---

## 10. The single permitted store touchpoint (interface obligation, NOT in oce-graph)

For completeness and to make the seam explicit: when a store is present, the host (`oce-api`) MAY,
**off the engine's critical evaluator**, stage external input values into `RunState.values` from a
point store before calling `eval_tick`, and read outputs after. Per the integration brief this is at
most **one** lock-free read per tick, performed by `oce-api`/`oce-store`, **never** by `oce-graph`.

- `oce-graph` exposes only: "give me a `&mut RunState` with external inputs already staged, and a
  `t_now`; I will produce outputs." It has **no** knowledge of where values come from or go.
- `oce-graph`'s `Cargo.toml` MUST NOT depend on `oce-store`, `oce-store-mem`, any store adapter, or any
  database crate. (Enforced in CI by a dependency-graph assertion.)
- All metadata (units, quantity, trend, tags, point types) is resolved at load and lives outside the
  evaluator (CDL §7.17). The evaluator reads **values only**.

This section names the seam so no implementer is tempted to reach into a store from the tick loop;
the actual store-agnostic `oce-store` port traits (and a non-normative reference adapter example) are
specified in `06-storage-abstraction-and-selene-adapter.md`.

---

## 11. Sampling, clock, and the rising-edge mechanism (`CDL.Discrete`)

Discrete/sampled behavior is **opt-in via blocks**, not a language clock (CDL §7.3 forbids Modelica
clocks/clocked state machines; §7.6 puts sampling in `CDL.Discrete` + triggers in
`CDL.Logical.Sources`). The engine provides the **mechanism** these blocks need: a periodic
`sample(start, period)` predicate and **rising-edge detection** on boolean trigger signals. Both are
realized *inside* the relevant blocks using `t` (§8) and per-block state (§7), **not** as a global
scheduler clock — this keeps the engine a pure synchronous-dataflow evaluator and keeps determinism
local.

### 11.1 `sample(start, period)` — periodic activation

A block like `CDL.Logical.Sources.SampleTrigger`, or the internal sampling of
`CDL.Discrete.Sampler`/`UnitDelay`/`ZeroOrderHold`, fires when the model time `t` crosses a sample
instant `start + k·period` for integer `k ≥ 0`. Because the host chooses the tick cadence (§8) and
need not align ticks to sample instants, the sample predicate is **edge-detected over `t`**:

```rust
// Inside a sampling block's state region: the last sample index already fired.
struct SamplerState { last_k: i64 }   // -1 before first sample

/// True on the tick during which a new sample boundary at or before t_now has been reached.
/// Handles cadence coarser OR finer than `period` (catches every boundary crossed since last tick;
/// if multiple boundaries were crossed in one coarse tick, fires once and snaps to the latest k —
/// the reference samples on the *current* value at the tick, see note).
fn sample_due(t_now: f64, start: f64, period: f64, s: &mut SamplerState) -> bool {
    if t_now + EPS < start { return false; }
    let k = ((t_now - start) / period).floor() as i64;   // current sample index
    if k > s.last_k { s.last_k = k; true } else { false }
}
```

**Requirements (numbered, normative):**

1. The sample predicate MUST be a pure function of `t_now`, the block's `(start, period)` parameters,
   and the persisted `last_k`. It MUST be robust to **arbitrary host cadence**: coarser than
   `period` (boundary may be crossed mid-interval — fire on the tick that first reaches/passes it),
   equal, or finer (fire only on the tick that crosses each new boundary, not every tick).
2. If a single coarse tick spans multiple sample boundaries, the block fires **once** and snaps
   `last_k` to the current `k` (it samples the *current* input at the tick — there is no
   sub-tick replay, consistent with zero-time event semantics and the host owning cadence). This is a
   documented **host-cadence behavior, OUT of the conformance guarantee** (matched in
   `07-conformance-and-verification.md` §13 risk #2): the conformance driver uses the `EventAligned`
   cadence (07 §5.4) configured to place a tick on **every** sample/trigger instant for any model with
   `CDL.Discrete.*` or `Logical.Sources.SampleTrigger`, so the engine lands on the oracle's grid and
   this snap-to-latest path is **never exercised** under conformance. The harness's `atolx` tolerance
   (event-timing slack) is a residual margin for a finer-stepped oracle, not a value re-pick — it
   cannot mask the value an intermediate-instant sample would have taken, which is exactly why
   conformance pins the cadence instead of relying on `atolx` here.
3. `EPS` comparisons MUST use a fixed, documented epsilon (e.g. relative to `period`) so boundary
   ticks are deterministic and don't flicker due to FP. The chosen epsilon is part of the conformance
   contract for discrete blocks.

### 11.2 Rising-edge detection — the trigger primitive

Triggered blocks (`CDL.Discrete.TriggeredSampler`, `TriggeredMax`, `TriggeredMovingMean`, and the
edge family `CDL.Logical.Edge`/`FallingEdge`/`Change`) act on a **rising edge** of a boolean input:
`false → true` between the prior tick and this tick. This is the second first-class discrete
primitive. It is implemented with a single persisted prior-value bit per trigger.

```rust
// Inside a triggered block's state region.
struct EdgeState { prev: bool }   // seeded from a parameter (typically false) at BUILD

/// Rising edge: true exactly on the tick where u transitions false -> true.
fn rising_edge(u: bool, s: &mut EdgeState) -> bool {
    let edge = u && !s.prev;
    s.prev = u;          // updated in the [S] update_state phase, AFTER emit (per §9 ordering)
    edge
}
```

**Requirements (numbered, normative):**

1. Edge detection is `[S]` (it owns `prev`). It MUST follow the §9 ordering: the edge for *this* tick
   is computed in `emit_from_state` against the *prior* `prev`; `prev` is written in `update_state`.
   The edge detector itself feeds through (`y ← {u}`); the **loop cut** in a triggered chain is the
   `Discrete.TriggeredSampler` it drives — whose *output* is the prior held sample (`feeds_through ==
   false`), which is what legally closes a feedback loop (§4.3, `03` §5).
2. The `prev` seed is a parameter (typically `false`), set in `init_state` (§7) — there is no `start`
   attribute. A trigger that is already `true` on tick 0 produces an edge on tick 0 iff the seeded
   `prev` is `false` (matching the reference's pre=false convention).
3. `Change` (Real/Int/Bool) and the `Edge`/`FallingEdge` variants reuse this primitive with the
   appropriate comparison; `FallingEdge` is `!u && prev`. These edge/change blocks are `[S]` (they
   own `prev`) but **feed through** on the current input (`y ← {u}`) — the edge is a function of the
   *current* `u` versus the prior `prev`, so they are **not** loop-breakers (see `03` §4.3 R-FT-3 and
   §5). The loop-cut among triggered blocks is the `Discrete.TriggeredSampler`/`TriggeredMax`/
   `TriggeredMovingMean` family (their output is the held prior sample), not the edge detectors.

### 11.3 Why this is NOT a scheduler-level clock

The sample/edge mechanism lives **inside blocks**, driven by the `t` the scheduler threads in. The
scheduler has **no** event queue, **no** clock domains, **no** rollback. This is deliberate:

- It preserves the single-pass, zero-allocation tick (§9).
- It keeps the §7.16 zero-time semantics (everything in a tick sees one `t`).
- It keeps the engine a synchronous-dataflow evaluator — the host owns cadence and the engine is
  correct at any cadence, with the conformance `atolx` band absorbing event-timing differences vs a
  finer-stepped oracle (synthesis E.1/E.5).

---

## 12. How loop-breakers cut the DAG (worked summary)

This ties §4–§6 together with the canonical CDL feedback pattern. Consider a feedback loop:
controller output `c.y` drives plant model surrogate `p.u`; plant output `p.y` feeds back to the
controller error input `c.u`. As pure algebra this is a cycle `c.y → p.u → p.y → c.u → c.y` and would
be **rejected** under §5.

CDL's legal forms (§7.16) and how the DAG handles them:

- **Insert `CDL.Logical.Pre` (or `CDL.Discrete.UnitDelay`) in the loop.** `Pre.feeds_through(in,out)
  == false`, so the internal edge `Pre.u → Pre.y` is **not** emitted (§4.2). The cycle in the
  feedthrough DAG is broken: the path becomes `c.y → p.u → p.y → Pre.u` (edge stops) and separately
  `Pre.y → c.u → c.y` (edge stops at `c.y`'s own block if the controller is itself a cut, or
  continues to `Pre.u` which is a leaf of the next tick). Result: **acyclic** → schedulable.
- **Insert an integrator** (`CDL.Reals.IntegratorWithReset`). Same mechanism: its output derives
  from prior accumulated state, not the current input this tick, so it reports `feeds_through ==
  false` and cuts the cycle.
- The loop-breaker's **prior-state-first** evaluation (§9 requirement 2) is what makes the broken
  loop produce the *correct* feedback value: at tick `n`, the controller reads the loop-breaker's
  output computed from state as of tick `n−1`, exactly the one-step-delayed feedback the §7.16 model
  prescribes.

**The complete rule, restated for implementers:** *A model is schedulable iff every directed cycle
of connections passes through at least one block that reports `feeds_through == false` on the
participating (input, output) pair. Such blocks are the loop-breakers: `Logical.Pre`,
`Discrete.{UnitDelay, Sampler, ZeroOrderHold, FirstOrderHold, TriggeredSampler, TriggeredMax,
TriggeredMovingMean}`, `Reals.IntegratorWithReset` (w.r.t. its integrand), and `Integers.OnCounter`.
Stateful-but-feedthrough blocks (`PID`, `Hysteresis` / `h>0` comparators, `Derivative`,
`MovingAverage`, `LimitSlewRate`, edge/timer/latch blocks) are **not** loop-breakers — a closed
control loop must route through one of the cut blocks above (see `03-block-library.md` §5). Any cycle
with no such cut is an algebraic loop and is hard-rejected at load.*

---

## 13. Consolidated engine requirement checklist (this section)

1. **Two phases.** BUILD once per load (off-tick); TICK is allocation/IO/hash/store-free.
2. **Direct-feedthrough DAG** over connector vertices: connection edges (out→in) + block-internal
   in→out edges **only where `Block::feeds_through(in,out)` is true** (§4).
3. **Feedthrough oracle owned by `oce-blocks`**, queried on the parameter-resolved instance; reports
   the block class's **true** feedthrough structure (statefulness ≠ cut). A cut exists only on an
   output computed entirely from prior state with no algebraic input dependence — neither over- nor
   under-cut (§4.2–§4.3, identical to `03` §3/§5).
4. **Algebraic loops → hard rejection** at load with dotted-path diagnostics and the §7.16 remedy;
   no tearing, no iteration (§5).
5. **Own Kahn topological sort** (D6), declaration-order tie-break, `O(V+E)`, selene-free, frozen
   into a `Schedule` reused every tick; CI-only cross-check vs a self-contained reference oracle (§6).
6. **State allocation** at BUILD: one fixed-size slot per `[S]` block, seeded from **parameters**
   (no `start` attribute); flat `RunState.words`; values persist between ticks (§7).
7. **Time** is `f64` seconds in `RunState.t`, **internal to elementary blocks**, monotonic,
   **host-chosen cadence** (reference 60–120 s), one `t` per tick, Δt computed inside blocks (§8).
8. **Tick eval loop:** each block once, in schedule order; `[S]` emits from prior state then updates;
   `[A]` is `y=f(p,t,u)`; alias/gather connection propagation; bit-deterministic (§9).
9. **No store on the tick.** `oce-graph` has zero store/selene dependency; the single off-tick point
   read/write is owned by `oce-api`/`oce-store` (§10).
10. **Sampling = `sample(start,period)` edge-detected over `t`** + **rising-edge detection** with a
    persisted `prev` bit, both *inside blocks*, no scheduler clock/event queue (§11).
11. **Loop-breakers cut the DAG** by reporting `feeds_through==false`; a model is schedulable iff
    every connection cycle passes through such a cut (§12).
12. **Determinism contract:** identical inputs+params+initial-state ⇒ bit-identical outputs across
    runs/platforms — the executable-specification property (§1, §9 req 5).

---

## 14. Cross-references

- `00-overview-and-decisions.md` — D1 (dual representation), D6 (own Kahn sort), D-OWNER-1
  (selene-free core), crate decomposition (`oce-model`, `oce-graph`, `oce-blocks`).
- `02-type-system-and-values.md` — `Value`/`ValueType`, attribute model, arrays, parameters vs
  constants, the §7.7.2 expression evaluator (referenced by `ParamTable` and `init_state`).
- `03-block-library.md` — the full `Block` trait (`kind`, `feeds_through`, `state_len`,
  `init_state`, `emit_from_state`, `update_state`, `step_algebraic`), the `[A]`/`[S]` taxonomy, and
  per-class feedthrough/state definitions referenced in §4.3 and §11.
- `04-cxf-ingest.md` / `oce-flatten` — produce the monomorphic, single-assignment, array-flattened
  `oce-model` the engine assumes (§3 invariants).
- `06-storage-abstraction-and-selene-adapter.md` — the store-agnostic `oce-store` port seam named in
  §10; any concrete database mapping lives only in that doc's non-normative reference appendix.
- `07-conformance-and-verification.md` — funnel comparison, `atolx` event-timing slack referenced in
  §11.1, Tier 0 stateful-block initial-state coverage referenced in §7.
- `08-embeddable-api-and-performance.md` — `Engine::tick`, dual modes, host cadence ownership (§8),
  the performance floor the allocation-stable tick (§9) targets.
