# Architecture

This page is for an engineer deciding whether to embed the Open Control Engine in a product. It
answers four questions: what the layers are, where the seam between them falls, what each of the 17
crates owns, and what this engine will never do for you.

## The organising idea: the CDL §7.17 non-computational seam

CDL §7.17 states that point lists, trends, display units, tags, and all Brick / Haystack / ASHRAE
223P semantics **do not affect the computation of a control signal**. That is the cleanest seam
available in this problem domain, and the engine is built along it rather than around it.

![Architecture diagram. A host application drives the engine through the oce-api facade. Below it, a
deterministic execution core labelled Group A contains oce-model, oce-expr, oce-blocks, oce-flatten,
oce-validate, oce-graph, oce-cxf, oce-semantics and oce-diag; it sees only blocks, connections and
values, evaluates a frozen Kahn schedule over flat arrays, and has no database. A horizontal line
marked "CDL §7.17 seam" separates it from the storage side, where the oce-store port publishes the
ModelStore, PointStore and SemanticStore traits, served either by the in-memory default oce-store-mem
or by an app-side adapter over a graph DB, SQL or time-series backend. The library ships no
first-party database.](diagrams/architecture-seam.svg)

Above the seam sits an **execution core**: a small, deterministic, in-memory dataflow machine that
sees *only* blocks, typed connections, and values. This is the hot path, and it has zero dependency
on any database.

Below it sits **storage behind a trait**. Everything the evaluator must not read — equipment
topology, points, instance structure, parameters, trends, semantic triples — plus durable
persistence is reached only through the `oce-store` port traits
(`crates/oce-store/src/lib.rs:580`). The library ships **no first-party database**. Durable or
queryable backends are app-side adapters behind the port, with an in-memory default
(`oce-store-mem`) so that a downstream project can embed the engine for *load → flatten → validate →
schedule → tick → simulate* with no database at all.

The seam also fixes where responsibility for input quality lives, and it is not here. Staging is
deliberately status-agnostic: a sample is converted from its value regardless of `PointStatus`, so
`Fault`, `Stale`, and `Uninitialized` all stage exactly like `Ok`. A missing sample is not an error
either — the connector holds its previous value indefinitely, and before the first sample it holds
the type's `zero_value()`. The engine therefore implements **no fail-safe policy of its own**.
Staleness limits, fault reactions, and safe-state fallback belong to the host layer above it; see
[host-responsibilities.md](host-responsibilities.md).

## The pipeline: build once, tick many

![Pipeline diagram. A build phase runs once per load and off the hot path: load ingests CXF, flatten
elaborates, validate applies the conformance gate, and schedule performs a Kahn topological sort.
The frozen schedule then feeds a tick phase on the hot path, where tick advances one step and
simulate runs to t_stop, performing no I/O over preallocated schedule and
state.](diagrams/pipeline.svg)

Everything expensive and everything fallible happens in BUILD. Parsing, elaboration, conformance
rejection, algebraic-loop rejection, and topological sorting all run once per load. What survives is
a frozen schedule over flat arrays, and TICK walks it: no graph traversal, no hashing, no store
access in the graph evaluator.

## Embeddability posture

The engine is, by design:

- **Library-only.** No `main`, no `[[bin]]`, no daemon, no server, no network listener — verified by
  the absence of any binary target or `std::net` use in `crates/`. The host owns process lifecycle,
  transport, TLS, authN/Z, multi-tenancy, off-host durability, and metrics export.
- **Synchronous and in-process.** Every public method is a blocking synchronous call, and no async
  runtime is pulled at any layer. The check is the dependency set below, not a promise.
- **`#![forbid(unsafe_code)]` in all 17 crates**, belt and braces: each `crates/*/src/lib.rs`
  carries the crate attribute, *and* the workspace sets `unsafe_code = "forbid"` under
  `[workspace.lints.rust]` (`Cargo.toml:49`).
- **Edition 2024**, `resolver = "3"`.

The whole external dependency surface on the embed path is four crates: `serde`, `serde_json`,
`thiserror`, and `libm`. Two more — `regex` and `ryu` — exist only inside `oce-conformance`, which
nothing else in the workspace depends on, so a host embedding `oce-api` never links them.

### Two Rust versions, and they are not the same number

These are different values on purpose, and conflating them has already cost this repo one reverted
change (PR #209).

| | Value | Where | What it means to you |
| --- | --- | --- | --- |
| **MSRV** | `1.97.0` | `Cargo.toml:42` (`rust-version`) | The floor a consumer needs. Build the engine with any toolchain at or above this. |
| **Pin** | `1.97.1` | `rust-toolchain.toml` (`channel`) | What CI and local development actually build with, so the two agree exactly. |

The MSRV cannot be raised to match the pin casually: the release gate's `cargo public-api` surface
checks shell out to a pinned nightly that identifies as `rustc 1.97.0-nightly`, and Cargo enforces
`rust-version` against it. `Cargo.toml` carries the full explanation inline at the declaration.

## Determinism, and the two allocation carve-outs

The tick is deterministic: a frozen, topologically-sorted schedule evaluated over flat arrays, with
no graph walks, no hashing, and no store access in the graph evaluator. Two carve-outs, both real,
both stated here rather than in a footnote.

**The evaluator is not allocation-free for every block.** The schedule and state arrays are
preallocated and the gather scratch is reused, so most blocks tick without allocating.
`CDL.Reals.Sort` uses fixed stack buffers through `nin = 64` (`SORT_STACK_WIDTH`,
`crates/oce-blocks/src/reals_matrix.rs:340`), then falls back to two heap-allocated vectors for
wider inputs. `CDL.Reals.Log` and `CDL.Reals.Log10` use static warning messages, so warning emission
allocates nothing block-side — but a diagnostic sink may still allocate when it records an event,
including the `step_realtime` collector. Size a real-time loop against the blocks and the diagnostic
sink your sequence actually uses, not against a blanket guarantee.

**`Engine::tick` is store-free only when the model declares no store-backed inputs.** When it
declares none, the staging path returns immediately (`crates/oce-api/src/engine.rs:258`). Otherwise
the tick takes one `store.snapshot()` plus one read per staged input. With the default `MemStore`
that snapshot is exactly one boxed allocation (`crates/oce-store-mem/src/lib.rs:131` returns
`Box<dyn PointSnapshot>` over an `Arc` clone); the `PointStore` trait places **no** allocation bound
on a third-party backend's `snapshot()`.

Whether a tick allocates at all is gated per-PR, registry-wide and with a positive control, by
`crates/oce-blocks/tests/tick_allocation_census.rs`. The facade has a narrower guard in
`crates/oce-api/tests/tick_purity_tests.rs`. Throughput figures live in
[`docs/benchmarks.md`](benchmarks.md), recorded per run with the commit and host that produced them,
because nothing re-measures them in CI.

## The crate map

Seventeen crates. The dependency direction is acyclic and organized around the seam above.

**Execution core (Group A — no store, no database):**

| Crate | Responsibility |
| --- | --- |
| `oce-model` | Pure value/connector/instance/connection types; the `Value` enum (Real/Integer/Boolean/String/Enum) and the flattened model graph — the shared executable truth. |
| `oce-expr` | The CDL §7.7.2 binding-expression parser and evaluator (closed-world, pure). Bounded on structure: input deeper than `MAX_NESTING_DEPTH` (64) or wider than `MAX_EXPR_NODES` (4096) is a typed error, not a stack overflow (`crates/oce-expr/src/lib.rs:125`, `:132`). |
| `oce-blocks` | The `Block` trait and the native CDL elementary-block library, publicly enumerable at runtime via `catalog()` (`crates/oce-blocks/src/catalog.rs:155`) with ports, parameter rules, and honest parameter defaults per class. |
| `oce-flatten` | **Reserved seam; an identity passthrough today.** `oce-cxf` owns lowering because CXF arrives pre-flattened, so `flatten()` returns the model unchanged (`crates/oce-flatten/src/lib.rs:53`). Full `.mo` flattening is deferred. It is on the `oce-api` path, so the seam is wired even though it does nothing. |
| `oce-validate` | Loader conformance: subset rejection, single-assignment, type and attribute unification, parameter rules. |
| `oce-graph` | The deterministic scheduler and executor: direct-feedthrough DAG, algebraic-loop rejection, its own Kahn topological sort, the tick loop. |
| `oce-cxf` | CXF (Control eXchange Format) JSON-LD ↔ model graph, both directions. Composite nesting is bounded at `MAX_COMPOSITE_NESTING_DEPTH` (64) (`crates/oce-cxf/src/resolve/composite.rs:22`); composite *boundary* resolution recurses per `isConnectedTo` hop and is **not yet depth-bounded**, so treat untrusted documents accordingly. The accept/reject contract is written out in [cxf-composite-subset.md](cxf-composite-subset.md). |
| `oce-semantics` | **Reserved seam; annotation parsing is deferred.** The intended role is vendor-annotation parsing into effective non-computational point/trend/semantic metadata. No `__cdl` / `__CDL` annotation parsing exists today. |
| `oce-diag` | The shared diagnostic vocabulary (`Severity` / `DiagCode` / `Diagnostic`) across the ingest path. Zero dependencies. |

**Storage ports (the seam — traits only, no database types):**

| Crate | Responsibility |
| --- | --- |
| `oce-store` | **The seam.** The `ModelStore` / `PointStore` / `SemanticStore` / `Durable` traits plus DTOs, unified by the `Store` supertrait. No database types. |
| `oce-store-mem` | The default in-memory backend, so the engine runs with no database. |
| `oce-reference-wal-adapter` | **Verification-only, `publish = false`.** A `std::fs` WAL and atomic-snapshot adapter that exists to prove the frozen seam can carry real durability without a first-party database. Not a supported backend. |

**Verification, externals, and the host facade:**

| Crate | Responsibility |
| --- | --- |
| `oce-conformance` | The funnel-style tolerance-band and golden-trace conformance harness. Standalone: no other crate depends on it. Read [`TESTING.md`](../TESTING.md) for what it does and does not check. |
| `oce-bless` | **Test-support only, `publish = false`.** The single definition of the repo's environment-variable truthiness policy, so golden-regeneration switches cannot drift apart across crates. |
| `oce-extension` | **Reserved seam; nothing consumes it.** The intended role is the FMI / extension-block boundary. No crate depends on it, the CXF resolver has no extension-block branch (an unknown class is a hard `ClassNotFound`), and `DiagCode::MissingFmuPath` (`crates/oce-diag/src/lib.rs:167`) is declared but never constructed. **Do not plan FMI integration against this crate.** |
| `oce-docs` | **Reserved seam, not implemented.** The sequence-spec and point-list export surface is declared; `point_list_html` panics with `unimplemented!` (`crates/oce-docs/src/lib.rs:17`). Nothing depends on it. |
| `oce-api` | The embeddable host facade: `Engine<S: Store = MemStore>` (`crates/oce-api/src/engine.rs:38`) — the single public surface, spanning load, tick, simulate, parameters, IO inventory, key-selected output reads (`watch`), CXF export with content id, and a read-only topology view. |

Four of those — `oce-flatten`, `oce-semantics`, `oce-extension`, `oce-docs` — are reserved seams
rather than working components. They are named here so that nobody plans a feature against a crate
that does nothing yet.

### One thing that looks like a feature flag and is not

`oce-api` declares `default = ["mem"]` (`crates/oce-api/Cargo.toml:23`), but `mem = []` gates
nothing and `oce-store-mem` is an unconditional dependency, so disabling default features does not
remove the in-memory backend. What actually makes `MemStore` the default is the type parameter in
`Engine<S: Store = MemStore>`. To use a different backend, name it: `Engine<MyAdapter>`.

## What this engine will never do

Each of these is a design commitment, not a gap waiting to be closed.

- **No first-party database.** Persistence lives behind the `oce-store` port, authored app-side.
- **No daemon, no server, no network listener.** It is a library; the host owns the process.
- **No async runtime** at any layer, pulled by any crate.
- **No fail-safe policy of its own.** It does not decide what a stale sample means, what a faulted
  point should do, or what safe state looks like for your equipment. Those decisions are yours, and
  they are enumerated in [host-responsibilities.md](host-responsibilities.md).

For what the engine has and has not been verified against — including the tiers that are not wired —
see [`TESTING.md`](../TESTING.md) and the verification section of the [README](../README.md).
