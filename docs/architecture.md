# Architecture

This page is for an engineer deciding whether to embed the Open Control Engine in a product. It
answers four questions: what the layers are, where the seam between them falls, what each of the 13
crates owns, and what this engine will never do for you.

The [product contract](product-contract.md) owns the aggregate requirement/evidence map and delegates
domain semantics without turning future acceptance outcomes into current behavior.

## The organising idea: the non-computational metadata seam

The OpenBuildingControl (OBC) specification's metadata section (OBC §7.17; clause 5.13 in the table
of contents of ANSI/ASHRAE Standard 231-2026) says of that metadata that **none of it affects the
computation of a control signal**. The engine applies that rule to point lists, trends, display
units, tags, and all Brick / Haystack / ASHRAE 223P semantics. That is the cleanest seam available
in this problem domain, and the engine is built along it rather than around it.

![Architecture diagram. A host application drives the engine through the oce-api facade, which is
pure: plain data in, plain data out. Below it, a deterministic execution core labelled Group A
contains oce-model, oce-expr, oce-blocks, oce-validate, oce-graph, oce-cxf, oce-semantics and
oce-diag; it sees only blocks, connections and values, evaluates a frozen Kahn schedule over flat
arrays, and has no database. A line marked "metadata seam" separates it from the storage side,
where the oce-store port publishes the ModelStore, PointStore and SemanticStore traits, served
either by its in-memory MemStore (the default mem feature) or by an app-side adapter over a graph
DB, SQL or time-series backend. Only the host reaches the storage side; the facade never does. The library
ships no first-party database.](diagrams/architecture-seam.svg)

Above the seam sits an **execution core**: a small, deterministic, in-memory dataflow machine that
sees *only* blocks, typed connections, and values. This is the hot path, and it has zero dependency
on any database.

Beside it sits **storage behind a trait**, reached by the host and never by the engine. Everything
the evaluator must not read — equipment topology, points, instance structure, parameters, trends,
semantic triples — plus durable persistence goes through the `oce-store` port traits
(`ModelStore`, `PointStore`, `SemanticStore`, `Durable` and the composite `Store` in
`crates/oce-store/src/lib.rs`). The library ships **no first-party database**. Durable or queryable
backends are app-side adapters behind the port. `oce-store` also carries an in-memory backend,
`MemStore`, behind its default `mem` feature, and the host-side post-load helper
`oce_store::host::persist_loaded_model`.

The facade is **pure**: `Engine` is a plain struct with no storage backend, and `oce-api` has no
dependency on `oce-store` (asserted by `.github/scripts/check-default-no-db.sh`
and `crates/oce-api/tests/frame_purity.rs`). `Engine::load_cxf` returns a plain-data `LoadedModel`
beside its `LoadReport` — the resolved-model projection, the IO inventory and the load diagnostics —
and the host decides whether and where to persist it. The resolved-model DTOs live in `oce-model`
(`oce_model::resolved_model`) so the facade can return them and the store port can persist them
without either crate depending on the other. A downstream project embeds the engine for *load →
validate → schedule → prepare frame → execute frame* with no storage crate in the build at all.

The seam also fixes where responsibility for input quality lives: the host supplies every typed
boundary input exactly once. Missing values refuse; stored samples and prior values cannot fill gaps.
Typed completeness does not establish sensor quality or freshness, so the engine still implements
**no fail-safe policy of its own**.
Staleness limits, fault reactions, and safe-state fallback belong to the host layer above it; see
[host-responsibilities.md](host-responsibilities.md).

## The pipeline: build once, tick many

![Pipeline diagram. A build phase runs once per load and off the hot path: load ingests and lowers
CXF, validate applies the conformance gate, and schedule performs a Kahn topological sort.
The host prepares complete typed observations, then consumes them in one HostTick transition with
an owned completed result and no storage I/O.](diagrams/pipeline.svg)

Parsing, elaboration, conformance
rejection, algebraic-loop rejection, and topological sorting all run once per load. What survives is
a frozen schedule over flat arrays, and TICK walks it: no graph traversal, no hashing, no store
access in the graph evaluator.

The current runtime is the fixed [HostTick v1 execution profile](execution-profile.md): every
successful frame execution evaluates this schedule once and advances state once, including at a repeated
timestamp. It does not implement Modelica same-time event iteration for `CDL.Logical.Pre`.

The frame around that evaluation lives once, in the core: `oce_graph::frame` checks the time and
every input slot (type, non-finite policy, domain), keeps the accepted-frame sequence, stages the
values and evaluates ([one frame driver, two callers](execution-profile.md#one-frame-driver-two-callers)).
The `oce-api` facade is a thin layer over it that resolves names, holds the prepared plan and
returns an owned `CompletedFrame`; a `no_std` target runs the same driver by slot through the
[compiled-schedule blob](compiled-schedule-blob.md#executing-a-blob) entry,
`oce_graph::blob::Executable`.

## Embeddability posture

The engine is, by design:

- **Library-only.** No `main`, no `[[bin]]`, no daemon, no server, no network listener in the engine
  crates — verified by the absence of any binary target or `std::net` use in `crates/` outside
  `crates/leverage/`. The no-binaries rule covers the engine crates only: leverage crates may ship
  binaries above the facade, never a daemon, server or listener (authoring note decision 9). The
  first is the `oce` command-line tool (`crates/leverage/oce-cli`; see the
  [authoring layer](design/authoring-layer.md)). The host owns process
  lifecycle, transport, TLS, authN/Z, multi-tenancy, off-host durability, and metrics export.
- **Synchronous and in-process.** Every public method is a blocking synchronous call, and no async
  runtime is pulled at any layer. The check is the dependency set below, not a promise.
- **`#![forbid(unsafe_code)]` in all 18 workspace members**, belt and braces: each
  `crates/*/src/lib.rs`, each `crates/leverage/*/src/lib.rs`, the `oce` binary root
  (`crates/leverage/oce-cli/src/main.rs`) and the firmware image's lib and bin roots carry the crate
  attribute,
  *and* the workspace sets `unsafe_code = "forbid"` under `[workspace.lints.rust]` (root
  `Cargo.toml`).
- **Edition 2024**, `resolver = "3"`.
- **A `no_std + alloc` execution core.** `oce-model`, `oce-expr`, `oce-blocks` and `oce-graph` build
  without the standard library; see [below](#no_std--alloc-the-core-four).

The whole external dependency surface on the embed path is five direct crates: `serde`,
`serde_json`, `thiserror`, `libm`, and `sha2`. `sha2` sits on `oce-api` only, with
`default-features = false`, for the [canonical content id](design/content-id.md), and brings its
small RustCrypto tree (`digest`, `block-buffer`, `crypto-common`, `generic-array`, `typenum`,
`cpufeatures` with `libc` on some targets, `cfg-if`); the core four never link it. Two more —
`regex` and `ryu` — exist only inside `oce-conformance`, which
nothing else in the workspace depends on, so a host embedding `oce-api` never links them; and
`csv-core` exists only inside the leverage crate `oce-replay`, `schemars` only inside
`oce-author` and `oce-verify` and `clap` only inside `oce-cli`, all above the facade; `oce-vocab`
adds none.

### `no_std + alloc`: the core four

The four execution-core crates — `oce-model`, `oce-expr`, `oce-blocks` and `oce-graph` — each have
`default = ["std"]`, `std = ["alloc", …]` and `alloc = []`. Built with
`--no-default-features --features alloc` they are `#![no_std]` and need only `core` and `alloc`:
a global allocator for `Vec`, `String`, `Box`, `Arc` and `BTreeMap`, and nothing else — no operating
system, threads, clock, files or I/O. `alloc` is the floor: a build with neither feature refuses with
a `compile_error!`. The default build is unchanged, and every crate that depends on one of the four
names `features = ["std"]` on that edge.

| Crate | Built without `std` | What differs from the default build |
| --- | --- | --- |
| `oce-model` | `no_std + alloc` | Nothing. `serde` is linked with `alloc` and `derive` only. |
| `oce-expr` | `no_std + alloc` | The evaluation-fuel meter is one program-wide slot instead of thread-local: evaluate from one execution context at a time, or serialize evaluations across cores and interrupt handlers yourself. |
| `oce-blocks` | `no_std + alloc` | `catalog()` is absent: its cache is a `std::sync::OnceLock`, and the catalog is host metadata a target does not read. `class_signature_hash` and `class_is_target_bound`, which a blob decoder needs, are present. |
| `oce-graph` | `no_std + alloc` | Nothing: the [compiled-schedule blob](compiled-schedule-blob.md) decoder, the HostTick v1 frame driver and the blob execution entry are part of the `no_std` build; its closure holds no `oce-cxf`, `oce-api`, `serde_json` or `std` (`scripts/ci/check-no-std-entry-closure.sh`). |

Errors derive `core::error::Error` through `thiserror` with its default features off. Every `f64`
operation that is not in `core` on Rust 1.97 (`floor`, `ceil`, `trunc`, `round`, `sqrt`) goes
through `libm`, in the default build too, so the two builds share one numeric path; those five are
exact in IEEE 754, and `crates/oce-blocks/tests/libm_std_parity.rs` checks them against the `std`
methods bit for bit. Each of those calls goes through the crate's `math` module — the one place
that names `libm` — and clippy refuses any `std`-only float method elsewhere in the four crates' `src/` trees, unit
tests included (`tests/` files are separate crates outside the lint, where the `std` methods serve
as the oracle)
([TESTING.md, math discipline](../TESTING.md#conventions), issue #399). The four use no hash
collections: clippy refuses `std`'s hash types there and `crates/oce-api/tests/core_crates_hash_free.rs`
refuses any hash collection or hash crate. In the `std` crates no hash iteration order reaches an
output, which `crates/oce-api/tests/hash_order_independence.rs` checks across independently seeded
hashers ([state compatibility](state-compatibility.md#portability-domains-and-evidence-limits)).

Everything else stays `std`: `oce-cxf`, `oce-validate`, `oce-semantics`, `oce-diag`, `oce-store` and
the `oce-api` facade, plus the verification-only `oce-conformance` and `oce-bless`. The
[bare-metal cell](ci-and-the-gate.md#the-bare-metal-cell) in `pr-gate.yml` builds and lints the
four on `thumbv7em-none-eabihf` (Cortex-M4F) and `riscv32imac-unknown-none-elf` (RV32IMAC), two
bare-metal targets with no `std`, and on `wasm32-unknown-unknown`; fails on any `std` feature or
host-only crate in their dependency graphs; runs the `oce-expr` suite without `std`; and links them
into a firmware image ([`firmware/link`](../firmware/link/), `publish = false`) for both bare-metal
targets, held to a per-target size budget. The image decodes a checked-in blob golden with
`oce_graph::blob::Executable` and runs the shared 160-frame schedule; the nightly `qemu.yml` runs it
under emulation and requires its per-frame run digests to equal the host's
([the nightly emulation run](ci-and-the-gate.md#the-nightly-emulation-run)). `bash .agents/gate.sh`
runs the cell (`bash .agents/gate.sh nostd` runs it alone). What a target integrator supplies is
what the image supplies: a global allocator and a reset entry; the engine asks for nothing else.

### Two Rust versions, and they are not the same number

These are different values on purpose, and conflating them has already cost this repo one reverted
change (PR #209).

| | Value | Where | What it means to you |
| --- | --- | --- | --- |
| **MSRV** | `1.97.0` | root `Cargo.toml` (`rust-version`) | The floor a consumer needs. Build the engine with any toolchain at or above this. |
| **Pin** | `1.97.1` | `rust-toolchain.toml` (`channel`) | What CI and local development actually build with, so the two agree exactly. |

The pin is a patch release of the MSRV, so the declared floor is the lowest version every gate
actually builds against. Nothing in CI builds below the pin; lowering the MSRV would need a CI leg
that does.

## Determinism, and the two allocation carve-outs

The tick is deterministic: a frozen, topologically-sorted schedule evaluated over flat arrays, with
no graph walks, no hashing, and no store access in the graph evaluator. Two carve-outs, both real,
both stated here rather than in a footnote.

**The evaluator is not allocation-free for every block.** The schedule and state arrays are
preallocated and the gather scratch is reused, so most blocks tick without allocating.
`CDL.Reals.Sort` uses fixed stack buffers through `nin = 64` (`SORT_STACK_WIDTH`,
`crates/oce-blocks/src/reals_matrix.rs`), then falls back to two heap-allocated vectors for
wider inputs. `CDL.Reals.Log` and `CDL.Reals.Log10` use static warning messages, so warning emission
allocates nothing block-side — but a diagnostic sink may still allocate when it records an event,
including the completed-frame warning collector. Size a host loop against the blocks and the diagnostic
sink your sequence actually uses, not against a blanket guarantee.

**The engine is storage-free.** Preparation owns the complete typed observations and resolved
targets; execution consumes the plan and produces an owned receipt. Their structural allocation
budget scales with inputs, fan-out, boundary outputs and emitted warnings. It is not zero allocation
or a whole-state clone. Load calls no storage port either: it returns the resolved model as plain
data and the host persists it if it wants durability.

Whether a block tick allocates on the evaluator thread is gated per-PR, registry-wide and with a
positive control, by `crates/oce-blocks/tests/tick_allocation_census.rs`. The blob path (decode
once, then frames through the core frame driver) is censused by
`crates/oce-graph/tests/blob_execution.rs` against the `no_std + alloc` build: zero allocations per
frame over every golden; decoding allocates what it returns. No current block delegates
work to a worker thread; such an implementation would need a companion guard for worker allocation.
The facade's corpus-wide purity and storage-isolation guard is
`crates/oce-api/tests/frame_purity.rs`; frame allocation
formulas are checked by `frame_observations.rs`. Throughput figures
live in [`docs/benchmarks.md`](benchmarks.md), recorded per run with the commit and host that produced
them, because nothing re-measures them in CI.

## The crate map

Twelve engine crates, plus the unpublished firmware image `oce-firmware-link` (`firmware/link`),
which links the `no_std` core into a bare-metal binary, and five unpublished leverage crates above
the facade, `oce-replay` (`crates/leverage/oce-replay`), `oce-author` (`crates/leverage/oce-author`),
`oce-cli` (`crates/leverage/oce-cli`), `oce-vocab` (`crates/leverage/oce-vocab`) and `oce-verify`
(`crates/leverage/oce-verify`): 18 workspace members, 10 publishable and eight private. The
dependency direction is acyclic and organized around the seam above.
Their responsibilities below do not imply equal support or publication status: publication is
read from each crate's `publish` flag and checked by the
[package validator](ci-and-the-gate.md#the-package-validator), and a publishable implementation
crate is registry closure for the facade, not an independently supported host API.

**Execution core (Group A — no store, no database):**

| Crate | Responsibility |
| --- | --- |
| `oce-model` | Pure value/connector/instance/connection types; the `Value` enum (Real/Integer/Boolean/String/Enum) and the flattened model graph — the shared executable truth. Also the plain-data resolved-model DTOs (`resolved_model`) that `oce-api` returns from a load and `oce-store` re-exports, so neither of those crates depends on the other. |
| `oce-expr` | The binding-expression parser and evaluator for parameter assignments (OBC §7.7.2; closed-world, pure). Bounded on structure: input deeper than `MAX_NESTING_DEPTH` (64) or wider than `MAX_EXPR_NODES` (4096) is a typed error, not a stack overflow (`crates/oce-expr/src/lib.rs`). Depth counts real nesting only; a left-associative chain such as `a+b+…` is bounded by the node cap and evaluated iteratively. |
| `oce-blocks` | The `Block` trait and the native CDL elementary-block library, publicly enumerable at runtime via `catalog()` (`crates/oce-blocks/src/catalog.rs`; `std` builds only) with ports, parameter rules, honest parameter defaults, the class signature hash and the target-bound flag per class. |
| `oce-validate` | Loader conformance: subset rejection, single-assignment, type and attribute unification, parameter rules. |
| `oce-graph` | The deterministic scheduler and executor: direct-feedthrough DAG, algebraic-loop rejection, its own Kahn topological sort, the tick loop, and the slot-indexed HostTick v1 frame driver around it (`oce_graph::frame`) that the facade and the blob entry both call. Also the `OCEB` [compiled-schedule blob](compiled-schedule-blob.md) codec (`oce_graph::blob`): the decoder and the execution entry (`blob::Executable`) a `no_std + alloc` target runs, and the byte writer the facade encodes with. |
| `oce-cxf` | CXF (Control eXchange Format) JSON-LD ↔ model graph, both directions. The resolver owns all lowering: composites are flattened away, parameters grounded and array parameters normalized to per-element scalars, so nothing elaborates the graph after it. Full `.mo` flattening is not implemented. Composite nesting is bounded at 64. Boundary lowering is iterative and separately bounded at 64 non-top `isConnectedTo` hops per path, 65,536 target examinations, and 8 MiB of aggregate target-IRI bytes per document. The accept/reject contract is written out in [cxf-composite-subset.md](cxf-composite-subset.md). |
| `oce-semantics` | **Active effective-point metadata resolver.** Derives metadata from connector attributes, direction, value type and defaults; this is unrelated to the removed semantic-template loader. Raw `__cdl` / `__CDL` annotation parsing remains deferred. It stays a wired, publishable `oce-api` implementation dependency, not an independently supported API. |
| `oce-diag` | The shared diagnostic vocabulary (`Severity` / `DiagCode` / `Diagnostic`) across the ingest path. Zero dependencies. |

**Storage ports (the seam — traits only, no database types; reached by the host, never by `oce-api`):**

| Crate | Responsibility |
| --- | --- |
| `oce-store` | **The seam.** The `ModelStore` / `PointStore` / `SemanticStore` / `Durable` traits plus DTOs, unified by the `Store` supertrait; the resolved-model DTOs are re-exported from `oce-model`. No database types. Also the host-side post-load helper (`host`: recover → save the model → validate the input points) and, behind the default `mem` feature, the in-memory `MemStore` backend; `default-features = false` builds the ports, DTOs and helper alone. Depends on `oce-model`, never on `oce-api`, and `oce-api` never depends on it. |

**Verification, externals, and the host facade:**

| Crate | Responsibility |
| --- | --- |
| `oce-conformance` | **Verification-only, `publish = false`.** The funnel-style tolerance-band and golden-trace conformance harness. Standalone: no other crate depends on it. Read [`TESTING.md`](../TESTING.md) for what it does and does not check. |
| `oce-bless` | **Test-support only, `publish = false`.** The single definition of the repo's environment-variable truthiness policy, so golden-regeneration switches cannot drift apart across crates. |
| `oce-api` | The primary embeddable host facade: a pure `Engine` (no type parameter, no storage dependency), spanning load with a plain-data `LoadedModel` result, complete-frame preparation/execution, parameters, IO inventory, latest-state non-receipt reads (`get_output`/`watch`), CXF export with content id, compiled-schedule blob encoding (`Engine::compile_blob`), the raw state words in the blob entry's layout (`Engine::state_words`, a read-only verification view, not a continuation format; continue state with `state_snapshot` or `checkpoint`), and a read-only topology view. Its catalog metadata is `oce_api::catalog()`, with the actively supported `oce_blocks::catalog()` as a separate companion surface; see the [product contract](product-contract.md). |

**Leverage crates (above the facade; `oce-api` only, see [`crates/leverage/`](../crates/leverage/README.md)):**

| Crate | Responsibility |
| --- | --- |
| `oce-replay` | **Leverage, `publish = false`.** Historical replay ([design note](design/replay.md)): frame sources by slot (owned columns, a bounded CSV reader, step lists), the missing-sample policy (refuse and stop by default; every hold, fallback, skip and refusal recorded), a columnar bit-exact trace with a documented size per frame, and a resumable cursor over the facade's slot entry (`Engine::execute_slots`). It is host policy applied offline, never engine policy; `oce-bless` is its one other `oce-*` edge, as a dev-dependency. |
| `oce-author` | **Leverage, `publish = false`.** The authoring layer's library ([design note](design/authoring-layer.md), [crate README](../crates/leverage/oce-author/README.md)): the typed authoring model and builder, the strict-JSON wire form (`from_wire_str` / `to_wire_string`: closed objects, five value shapes, bounded reads, one canonical spelling) with its `schemars`-derived JSON Schema (`WIRE_SCHEMA`, `schema/sequence.schema.json`) pinned byte for byte, the author-side (stage 1) validator, which judges a sequence against `oce_api::class_metadata` and `oce_api::class_ports`, and the text form (`parse_text` / `print_text` / `fmt_text`: a hand-written parser with typed refusals and a canonical, idempotent printer). It never evaluates, schedules or loads an engine; `oce-bless` is its one other `oce-*` edge, as a dev-dependency. |
| `oce-cli` | **Leverage, `publish = false`; binary `oce`.** The command-line tool ([crate README](../crates/leverage/oce-cli/README.md)): `emit`, `validate`, `fmt`, `catalog` and `vocab` over `oce-author`, `oce-vocab` and the facade's catalog (`class_metadata`, `class_ports`), with `clap` argument parsing, bounded reads, a documented JSON report on stdout and the exit rule 0 yes / 1 no / 2 error. `catalog` serves the block vocabulary beside the engine's facts (each parameter's and port's meaning, declared unit and quantity, each parameter's range rendered from the catalog's rules, the typed notes without their `evidence`, the upstream links), and `vocab` the text form's words, the unit scale factors and every code. It loads only the validator's throwaway engines and opens no listener; it depends on `oce-api` and the leverage crates `oce-author` and `oce-vocab`, with `oce-bless` as a dev-dependency. |
| `oce-vocab` | **Leverage, `publish = false`.** The block vocabulary ([design note](design/vocabulary.md), [crate README](../crates/leverage/oce-vocab/README.md)): reference text for the 133 authorable classes as two checked-in strict-JSON files, `generated.json` (connector and parameter descriptions with declared units and quantities, projected from the vendored modelica-json `CXF-Core.jsonld` through the catalog's own names by a regenerate-and-diff test) and `curated.json` (typed notes, and descriptions only where upstream has none), merged and checked against the catalog (`vocabulary()`) and joined with it per class (`class_vocab()`), where each parameter's range is rendered from the catalog's rules (`parameter_range`); and the closed unit scale-factor table (`SCALE_FACTORS`: `min`, `h`, `d`). It is not engine behaviour: nothing on the load path, the emitter, the blob or a content id reads it, and it never copies a default or a range. It depends on `oce-api` only, with `oce-bless` and `sha2` as dev-dependencies. |
| `oce-verify` | **Leverage, `publish = false`.** Verification over vectors ([design note](design/vectors.md), [crate README](../crates/leverage/oce-verify/README.md)): the one vector format, `oce/vectors/v1` (a strict bounded reader, `VectorSet::from_json_bytes`, with closed objects, so a key the format does not define is refused and never ignored, and the lowerable `VerifyLimits`; the rules a shape cannot say checked at load: a clock whose cadence is decided by multiplication, step lists from `t = 0`, windows that hold at least one tick, a diagnostic instant on a tick; the canonical form and `spec_id`; closed `VectorErrorKind` codes; the `schemars`-derived JSON Schema, `schema/vectors.schema.json`, pinned byte for byte), and the run, `run_vectors`: a set bound once to a CXF subject (every name by `oce-replay`'s one name rule, every value typed by the port or parameter it lands on), each scenario replayed on a freshly loaded engine through `oce-replay` on the set's grid with its parameter edits applied in passes, compared exactly (a Real by its bits), and the receipt (`oce/receipt/v1`: the header shared with the commissioning verifier, the closed `ReceiptBody`, exact value spellings, `receipt_id` over the canonical bytes without `timings`), with closed `VerifyErrorKind` codes for every bind and run refusal. It opens no file and measures nothing; among `oce-*` packages it depends on `oce-api` and `oce-replay` only, with `oce-bless` as a dev-dependency. |

Every crate listed here does work today. Reserved placeholder crates were removed rather than kept
as empty seams, so nobody plans a feature against a crate that does nothing.

### Durability is the host's

`oce-api` declares no cargo features and has no storage dependency, so there is no build that links
a store into the engine. A host that wants durability takes the `LoadedModel` a load returns and
persists it through whichever adapter it chose behind the `oce-store` port — in one call,
`oce_store::host::persist_loaded_model(&store, &loaded.model, loaded.input_points())`; the engine
never calls that adapter, on load or on a frame. What the host owns is listed in
[host-responsibilities.md](host-responsibilities.md#durability-is-the-hosts).

## What this engine will never do

Each of these is a design commitment, not a gap waiting to be closed.

- **No first-party database.** Persistence lives behind the `oce-store` port, authored app-side and
  driven by the host; the engine never calls it.
- **No daemon, no server, no network listener.** It is a library; the host owns the process.
- **No async runtime** at any layer, pulled by any crate.
- **No fail-safe policy of its own.** It does not decide what a stale sample means, what a faulted
  point should do, or what safe state looks like for your equipment. Those decisions are yours, and
  they are enumerated in [host-responsibilities.md](host-responsibilities.md).

For what the engine has and has not been verified against — including the global report tiers that
are not wired — see [`TESTING.md`](../TESTING.md) and the verification section of the
[README](../README.md).
