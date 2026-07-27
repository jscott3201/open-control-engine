# Open Control Engine

**A high-performance, embeddable Rust control engine that natively executes the OBC / LBL
[Control Description Language (CDL)](https://obc.lbl.gov/specification/cdl.html) for smart-building
equipment control sequences.**

[![License: Apache-2.0 OR MIT](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](#license)
[![Rust 1.95 · edition 2024](https://img.shields.io/badge/rust-1.95%20%C2%B7%20edition%202024-orange.svg)](rust-toolchain.toml)
[![database-free](https://img.shields.io/badge/storage-database--free-success.svg)](#the-architectural-spine-the-cdl-717-non-computational-seam)

CDL is a declarative, object-oriented language — a strict subset of Modelica — that expresses
building control logic as block diagrams. Its determinism contract (CDL §7.16: synchronous
data flow + single assignment) means identical inputs and parameters yield identical outputs,
which makes the Open Control Engine a valid **executable specification** for commissioning and
continuous functional verification — the same control sequence, run bit-for-bit reproducibly.

It is designed to be the **core engine under the hood** of larger building-control products: the
public API and core semantics stay small, embeddable, and stable.

---

## Status

The architecture specification is the design of record and is complete; the implementation is
progressing milestone by milestone, each gated by extensive tests.

| Milestone | Scope | State |
| --- | --- | --- |
| **M0** | Deterministic execution core — workspace, types, scheduler, tick loop | ✅ done |
| **M1** | CXF ingest, database-free, end-to-end *load → simulate* | ✅ done |
| **M2** | CDL block-library breadth (G36) + conformance/oracle + parameter validation | ✅ done (in-repo) |
| **M3** | Durability through the `oce-store` port (reference adapter) | ⬜ planned |
| **M4** | Docs/point-list export, model-from-semantic, hardening, and `oce-py` PyO3 bindings (MVP) | ⬜ planned |
| **M5** | Python free-threading (`gil_used = false`) + free-threaded wheels + first PyPI publish | ⬜ planned |

Today the engine loads representative **ASHRAE Guideline 36** sequences (AHU supply-air-temperature
reset, AHU economizer, single-zone VAV) from CXF and simulates them end-to-end through the frozen
facade, against **~70 CDL elementary blocks**, with a self-contained schedule cross-check oracle,
whole-sequence **bit-exact determinism goldens**, an **independent closed-form conformance oracle**,
and block-parameter validation. External Modelica/Buildings reference cross-checks are a deliberately
deferred tail. The project is **pre-1.0** and not yet published to crates.io.

---

## The architectural spine: the CDL §7.17 non-computational seam

CDL §7.17 states that point lists, trends, display units, tags, and all Brick / Haystack /
ASHRAE 223P semantics **do not affect the computation of a control signal**. That single rule is the
cleanest seam in the system, and the engine is built around it.

![Architecture: the execution core sees only blocks, connections, and values and has no database; everything non-computational sits behind the oce-store port](docs/diagrams/architecture-seam.svg)

- An **execution core** — a small, deterministic, in-memory dataflow machine that sees *only*
  blocks, typed connections, and values. This is the hot path. It has **zero** dependency on any
  database.
- A **storage layer behind a trait** — everything the evaluator must *not* read (equipment topology,
  points, instance structure, parameters, trends, semantic triples) plus durable persistence — is
  reached only through the `oce-store` port traits. The library ships **no first-party database**;
  durable/queryable backends are **app-side adapters** behind the port, with an in-memory default
  (`oce-store-mem`).

A downstream project can embed the engine for *load → flatten → validate → schedule → tick →
simulate* with no database at all.

![Pipeline: load, flatten, validate, and schedule run once per load; tick and simulate run on the deterministic hot path](docs/diagrams/pipeline.svg)

---

## Embeddability posture

The engine is, by design:

- **Library-only** — no `main`, no daemon, no server, no network listener. The host owns process
  lifecycle, transport, TLS, authN/Z, multi-tenancy, off-host durability, and metrics export.
- **Synchronous, in-process** — every public method is a blocking synchronous call. **No async
  runtime** is pulled at any layer.
- **`#![forbid(unsafe_code)]`** in every crate.
- **edition 2024, Rust 1.95.0** (pinned in [`rust-toolchain.toml`](rust-toolchain.toml)),
  `resolver = "3"`.
- **Deterministic on the tick** — a frozen, topologically-sorted schedule evaluated over flat
  arrays: no graph walks, no hashing, no allocation, no I/O, and no store access on the hot path.

---

## Quickstart

> The facade crate is **`oce-api`**, published under the umbrella name **`open-control-engine`**.
> Until the first crates.io release, depend on it via git:

```toml
[dependencies]
oce-api = { git = "https://github.com/jscott3201/open-control-engine" }
```

A minimal embed — load a CDL sequence from CXF and simulate it (illustrative sketch):

```rust
use oce_api::{CollectSpec, Engine, InputSource, SimSpec, Value};

// 1. An engine with the default in-memory store — no database.
let mut engine = Engine::in_memory();

// 2. Load a CDL sequence from CXF (JSON-LD): parse, validate, freeze the schedule.
engine.load_cxf(cxf_bytes)?;

// 3. Simulate: feed inputs per tick, collect named outputs.
let metrics = engine.simulate(&SimSpec {
    t_start: 0.0,
    t_stop: 4.0,
    step: 1.0,
    inputs: InputSource::Closure(Box::new(|t| {
        vec![("zone_temp".to_string(), Value::Real(22.0 + t))]
    })),
    collect: CollectSpec::Named { points: vec!["sat_setpoint".to_string()], stride: 1 },
})?;
```

---

## The crate map (`oce-*`)

The dependency direction is intentional and acyclic, organized around the seam above.

**Execution core (Group A — no store, no database):**

| Crate | Responsibility |
| --- | --- |
| `oce-model` | Pure value/connector/instance/connection types; the `Value` enum (Real/Integer/Boolean/String/Enum) and the flattened model graph — the shared executable truth. |
| `oce-expr` | The CDL §7.7.2 binding-expression parser/evaluator (closed-world; pure, total). |
| `oce-blocks` | The `Block` trait and the native CDL elementary-block library (stateless `[A]` / stateful `[S]`). |
| `oce-flatten` | Elaboration / CXF-path resolution (CXF arrives pre-flattened; full `.mo` flattening is deferred). |
| `oce-validate` | Loader conformance: subset rejection, single-assignment, type/attribute unification, parameter rules. |
| `oce-graph` | The deterministic scheduler/executor: direct-feedthrough DAG, algebraic-loop rejection, own Kahn topological sort, the tick loop. |
| `oce-cxf` | CXF (Control eXchange Format) JSON-LD import/export ↔ the model graph. |
| `oce-semantics` | Vendor-annotation parsing → effective (non-computational) point/trend/semantic metadata. |
| `oce-diag` | The shared diagnostic vocabulary (`Severity` / `DiagCode` / `Diagnostic`) across the ingest path. |

**Storage ports (the seam — traits only, no database types):**

| Crate | Responsibility |
| --- | --- |
| `oce-store` | **The seam.** The `ModelStore` / `PointStore` / `SemanticStore` traits + DTOs. No database types. |
| `oce-store-mem` | The default in-memory backend, so the engine runs with no database. |

**Verification, externals & host facade:**

| Crate | Responsibility |
| --- | --- |
| `oce-conformance` | The funnel-style tolerance-band / golden-trace conformance harness. |
| `oce-extension` | The FMI / extension-block boundary (v1 surfaces extension blocks as unresolved externals). |
| `oce-docs` | Sequence-spec (Word/HTML) and point-list document export. |
| `oce-api` | The embeddable host facade: `Engine<S: Store = MemStore>` — the single public surface. Published as **`open-control-engine`**. |

---

## Testing

This engine controls real equipment, so **a wrong result is a physical hazard** — testing is a
first-class deliverable, not an afterthought. Every change ships extensive edge-case tests, **golden
tests** (checked-in expected outputs compared bit-exactly), **oracle cross-checks** (results compared
against independently-derived references), and **determinism goldens**. See
[`TESTING.md`](TESTING.md) for the full standard.

[cargo-nextest](https://nexte.st/) is the test runner:

```bash
cargo nextest run        # unit + integration tests
cargo test --doc         # doctests (nextest does not run these)
```

To run the gate the way CI runs it:

```bash
bash .agents/gate.sh        # the per-PR gate
bash .agents/gate.sh full   # adds the workspace suite and doctests
```

CI is **dev-light / release-heavy**: per-PR gates into `development` run fmt / clippy / build /
rustdoc / file-size / no-secret / database-free checks plus a determinism subset covering
`oce-blocks` and `oce-expr` on two architectures; the **full test suite runs on
`development → main` release gates**.

---

## Build & develop

```bash
cargo build --workspace        # the engine only — no database, no async runtime
```

`oce-api` exposes one feature today: `default = ["mem"]`, which wires the in-memory store as the
default `Store` backend. Durable/queryable backends are the consuming application's responsibility,
authored app-side as an adapter behind the `oce-store` port.

Install the shared git hooks once after cloning (fast format/lint/no-DB gates on commit and push):

```bash
bash scripts/install-hooks.sh
```

Changes land via pull requests into the `development` branch, behind the CI gate in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml). Contributions are welcome — see
[`CONTRIBUTING.md`](CONTRIBUTING.md), and [`CHANGELOG.md`](CHANGELOG.md) for notable changes.

---

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you shall be dual-licensed as above, without any additional terms or
conditions.
