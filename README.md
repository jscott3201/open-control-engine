# Open Control Engine

A high-performance, **embeddable Rust control engine** that natively executes the OBC / LBL
**Control Description Language (CDL)** for smart-building equipment control sequences.

CDL is a declarative, object-oriented language — a strict subset of Modelica — that expresses
building control logic as block diagrams. Its determinism contract (CDL §7.16: synchronous data
flow + single assignment) means identical inputs and parameters yield identical outputs, which
makes the Open Control Engine a valid **executable specification** for commissioning and
continuous functional verification.

- North-star spec: the OBC/LBL CDL specification — <https://obc.lbl.gov/specification/cdl.html>
  (full index: <https://obc.lbl.gov/specification/index.html>).

> **Status:** the architecture specification is the design of record and is **complete**; the code
> in this repository is the **M0 scaffold** — the Cargo workspace, the public type/trait surfaces
> for the execution core and the storage seam, CI, and git hooks are in place, with method bodies
> filled in across subsequent milestones (M1: CXF ingest + a trivial sequence end-to-end; M2:
> ASHRAE Guideline 36 block-library breadth + a conformance harness; M3: the optional selene-db
> adapter).

## The architectural spine: the CDL §7.17 non-computational seam

CDL §7.17 states that point lists, trends, display units, tags, and all Brick / Haystack /
ASHRAE 223P semantics **do not affect the computation of a control signal**. That single rule is
the cleanest seam in the system, and the engine is built around it:

- An **execution core** — a small, deterministic, in-memory dataflow machine that sees *only*
  blocks, typed connections, and values. This is the hot path. It has **zero** dependency on any
  database.
- A **storage layer behind a trait** — everything the evaluator must *not* read (equipment
  topology, points, instance structure, parameters, trends, semantic triples) plus durable
  persistence and retrieval — reached only through selene-free port traits, with the database as
  an **optional, swappable, default-off** backend.

A downstream project can embed the engine for *load → flatten → validate → schedule → tick →
simulate* with no database at all.

## Embeddability posture

The engine is, by design:

- **Library-only** — no `main`, no daemon, no server, no network listener. The host owns process
  lifecycle, transport, TLS, authN/Z, multi-tenancy, off-host durability, and metrics export.
- **Synchronous, in-process** — every public method is a blocking synchronous call. **No async
  runtime** is pulled at any layer.
- `#![forbid(unsafe_code)]` in every crate.
- **edition 2024, rust 1.95.0** (pinned in `rust-toolchain.toml`), `resolver = "3"`.
- **Deterministic on the tick** — a frozen, topologically-sorted schedule evaluated over flat
  arrays: no graph walks, no hashing, no allocation, no I/O, and no store access on the hot path.

## The crate map (`oce-*`)

The dependency direction is intentional and acyclic, organized around the seam above.

**Execution core (Group A — no store, no database):**

| Crate | Responsibility |
| --- | --- |
| `oce-model` | Pure value/connector/instance/connection types; the `Value` enum (Real/Integer/Boolean/String/Enum) and the flattened model graph — the shared executable truth. |
| `oce-expr` | The CDL §7.7.2 binding-expression parser/evaluator (closed-world; pure, total). |
| `oce-blocks` | The `Block` trait and the native CDL elementary-block library (stateless `[A]` / stateful `[S]`). |
| `oce-flatten` | Elaboration / CXF-path resolution (CXF arrives pre-flattened; full `.mo` flattening is deferred). |
| `oce-validate` | Loader conformance: subset rejection, single-assignment, type/attribute unification. |
| `oce-graph` | The deterministic scheduler/executor: direct-feedthrough DAG, algebraic-loop rejection, own Kahn topological sort, the tick loop. |
| `oce-cxf` | CXF (Control eXchange Format) JSON-LD import/export → the model graph. |
| `oce-semantics` | Vendor-annotation parsing → effective (non-computational) point/trend/semantic metadata. |

**Storage ports (the seam — traits only, no database types):**

| Crate | Responsibility |
| --- | --- |
| `oce-store` | **The seam.** The `ModelStore` / `PointStore` / `SemanticStore` traits + DTOs. No database types. |
| `oce-store-mem` | The default in-memory backend, so the engine runs with no database. |
| `oce-store-selene` | The optional selene-db adapter — the **only** crate that may name a database type, reached only behind the `selene` feature (arrives at a later milestone). |

**Verification, externals & host facade (Group C):**

| Crate | Responsibility |
| --- | --- |
| `oce-conformance` | The funnel-style tolerance-band / golden-trace conformance harness. |
| `oce-extension` | The FMI / extension-block boundary (v1 surfaces extension blocks as unresolved externals). |
| `oce-docs` | Sequence-spec (Word/HTML) and point-list document export. |
| `oce-api` | The embeddable host facade: `Engine<S: Store = MemStore>` — the single public surface. Published under the umbrella name **`open-control-engine`**. |

## Build & feature flags

The **default build is the engine only — no database, no async runtime**:

```bash
cargo build --workspace
```

The optional `selene` feature wires the `oce-store-selene` adapter. (At the current milestone the
adapter is an empty stub, so this still links no database; the selene-db dependency arrives with
the adapter at a later milestone.)

```bash
cargo build --workspace --features selene
```

`oce-api` exposes the features:

- `default = ["mem"]` — wires the in-memory store as the default `Store` backend.
- `selene` — wires the optional selene-db adapter (`dep:oce-store-selene`).

## Development

Install the shared git hooks once after cloning (fast format/lint/seam gates on commit and push):

```bash
bash scripts/install-hooks.sh
```

The local gates mirror CI:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
bash .github/scripts/check-seam.sh           # Group A / store ports stay selene-free
bash .github/scripts/check-default-no-db.sh   # default build links no selene-db / tokio / async-std
```

Escape hatches for the hooks: `git commit/push --no-verify` (once) or
`export OCE_SKIP_HOOKS=1` (whole shell session).

Changes land via pull requests into the `development` branch, behind the CI gate in
`.github/workflows/ci.yml`.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
