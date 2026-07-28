# Open Control Engine

**A high-performance, embeddable Rust control engine that natively executes the OBC / LBL
[Control Description Language (CDL)](https://obc.lbl.gov/specification/cdl.html) for smart-building
equipment control sequences.**

[![License: Apache-2.0 OR MIT](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](#license)
[![Rust 1.95 · edition 2024](https://img.shields.io/badge/rust-1.95%20%C2%B7%20edition%202024-orange.svg)](rust-toolchain.toml)
[![database-free](https://img.shields.io/badge/storage-database--free-success.svg)](#the-architectural-spine-the-cdl-717-non-computational-seam)

CDL is a declarative, object-oriented language — a strict subset of Modelica — that expresses
building control logic as block diagrams. Its determinism contract (CDL §7.16: synchronous
data flow + single assignment) means identical inputs and parameters yield identical outputs. The
engine is built to be an **executable specification** on that basis — the same control sequence,
run bit-for-bit reproducibly, for commissioning and continuous functional verification.
Reproducibility is enforced and tested here, and so is agreement with the CDL / Buildings
*source semantics* — 407 independently derived Tier-A goldens, behind a CI-enforced firewall.
What has not been done is executing any sequence against an external Modelica / Buildings
*toolchain* (Tier 3, skipped). See the goldens note below.

It is designed to be the **core engine under the hood** of larger building-control products: the
public API and core semantics stay small, embeddable, and stable.

---

## Status

The architecture specification is the design of record. It lives in a working directory that is
not part of this repository, so nothing below can be checked against it from a clone — the
implementation is progressing milestone by milestone, each gated by extensive tests.

| Milestone | Scope | State |
| --- | --- | --- |
| **M0** | Deterministic execution core — workspace, types, scheduler, tick loop | ✅ done |
| **M1** | CXF ingest, database-free, end-to-end *load → simulate* | ✅ done |
| **M2** | CDL block-library breadth (G36) + conformance/oracle + parameter validation | ✅ done (in-repo) |
| **M3** | Durability through the `oce-store` port (reference adapter) | 🟡 reference adapter landed (verification-only) |
| **M4** | Docs/point-list export, model-from-semantic, hardening, and `oce-py` PyO3 bindings (MVP) | ⬜ planned |
| **M5** | Python free-threading (`gil_used = false`) + free-threaded wheels + first PyPI publish | ⬜ planned |

Today the engine loads **46 ASHRAE Guideline 36 sequence fixtures** from CXF and simulates them
end-to-end through the frozen facade, against a registry of **136 block classes**: 133 CDL
elementary blocks — 52 `Reals`, 26 `Logical`, 24 `Integers`, 15 `Routing`, 7 `Discrete`, 4
`Conversions`, 3 `Psychrometrics`, 2 `Utilities` — plus 3 reserved internal lowering classes
that native pass-through import synthesizes (a namespace authored CXF cannot spell). The
registry is publicly enumerable at runtime via `oce_blocks::catalog()`, which carries per-class
ports, parameter rules, and honest parameter defaults (a required parameter reports *no*
default rather than an internal fallback). The workspace suite is over **1400 tests** plus
doctests.

Read that as breadth of the fixture corpus, not as general G36 support — the repo's own catalog
(`tools/reference-catalog/Buildings.Controls.OBC.ASHRAE.G36.catalog.json`) is explicit that the
supported set is `selected-explicit-cxf-variants-supported` and does "not imply arbitrary ASHRAE
G36 composite support." Concretely: **43 pre-specialized runtime variants over 31 distinct
canonical class paths**, plus 3 hand-authored fixture-only fragments. They are **pre-flattened
CXF** at specific parameterizations. The engine executes the block graph CXF hands it; it does not
parse or flatten Modelica `.mo` sources (see `oce-flatten`). A canonical class path is promoted to
supported-runtime only once a source-proven composite importer, parameter variants, fixture,
provenance, golden trace, and oracle evidence all exist.

CXF is bidirectional. `oce-cxf` imports via the §7.1 resolver and **exports** under a round-trip
contract (RT-2): for a graph inside the export subset, re-importing the emitted bytes renders
bit-identically to what was exported, Reals compared by IEEE-754 bits rather than by epsilon.
Export covers the flat, ground, single-root, scalar-parameter subset, and emits the §7.4.1
connector attributes that fall in the canonical subset — `unit`, `quantity`, `displayUnit`, and
finite `min`/`max`. On a **surviving** block, a connector carrying `nominal` or `unbounded` is
rejected rather than silently dropped; on a deferred block, connector validation is skipped
entirely along with the block itself. Export takes no registry dependency, which leaves two documented ways for an `Ok` export
to produce bytes that fail re-import, both reachable only from a hand-built graph: a block whose
declared port arity contradicts the class its `class_path` names (`MalformedDocument`), and an
unregistered class path (`ClassNotFound`). Every graph the resolver produces is correct by
construction on both axes.

CDL's direct input→output connect (a boundary input wired straight to a boundary output) is
native: import lowers each such connect to a reserved internal identity block
(`urn:oce:lowering#PassThrough.{Real,Integer,Boolean}`), and export elides those blocks back to
the bare boundary edge. Re-import re-synthesizes them, so the RT-2 round trip holds by render
identity even though the emitted document lists fewer `containsBlock` entries than the graph
holds blocks — an elision, not a deferral, so it produces no warning.

Enum-carrying blocks are deferred rather than fatal **as long as something survives**: the block
and its transitive downstream consumers are omitted so the enum-free remainder still exports, but
a graph whose every block defers leaves no runtime composite to emit and is rejected with
`ExportUnsupported`. **`export()` discards those
warnings** — a caller using it alone cannot distinguish a complete export from one that dropped a
large fraction of the graph. The G36 corpus pins cones of 83 blocks of 213 (39 %) and 63 of 226
(28 %); rejection fires only on *total* deferral, so in principle all but one block can vanish
from a successful export. Use `export_with_report` to tell the two apart: an empty `warnings` list is what certifies
the round trip covered the whole input. Export is reached through `oce-cxf` directly or through
the facade: `Engine::export_cxf()` returns the same byte-deterministic document with the
deferral warnings intact, and `ExportReport::content_id()` mints a documented
**non-cryptographic** integrity tag (`cxf:fnv1a128:<hex>`) over exactly those bytes — a
different identity from `LoadReport::model_id`, and one that names a *partial* document
whenever warnings are present. A read-only `Engine::topology()` snapshot (blocks, typed
connections, external inputs, pass-through pairs) rounds out the host-introspection surface.

**What the goldens do and do not prove.** Each of the 46 G36 fixtures carries a whole-sequence
trace and a `.prov.json` recording its provenance, and every one of them records `"tier": "2"` —
**engine self-output, a determinism snapshot and explicitly not a correctness oracle**
(`depends_on_oce_blocks: true`). They catch drift, not wrongness.

A structural independence layer runs on every PR: the `modelica-json` translations of the 44
upstream G36 classes are vendored at a pinned commit pair and an in-repo audit
(`fixture_structural_oracle`) structurally diffs the fixtures against them — 30 EXACT and
1 EXACT-XFOLD over the 31 comparable fixtures, after hierarchy flattening and two-sided
conditional resolution. It compares graph structure, not numerics, so it bounds fixture
fidelity rather than block behavior.

Correctness is bounded by a separate, genuinely independent layer generated by `tools/golden-gen`
— a crate held off the workspace and **forbidden from depending on `oce-blocks`**, so its
references are re-derived from CDL / Buildings source semantics rather than from the
implementation under test. CI enforces that firewall, and every golden it emits records
`depends_on_oce_blocks: false`. It covers **407 Tier-A goldens: 275 CDL block- and type-level,
plus 132 G36 sequence outputs spanning all 46 fixtures**. Every one is compared **bit-exactly** —
the 275 through the per-block harness, the 132 through per-sequence oracle suites in
`crates/oce-conformance/tests/`, covering Real, Integer and Boolean outputs alike. The L1 funnel
band is an *additional* layer over the 102 Real G36 outputs, not the primary check; Boolean and
Integer outputs stay on exact comparison and are never routed through the type-blind funnel.
Most references are closed-form derivations; some, such as `TimeSuppression`, are explicit
per-tick recurrences.

**What is not wired is Tier 1 and Tier 3.** The report assembler executes Tiers 0, 2, and 4 and
explicitly skips both Tier 1 (per-block "same response" — that corpus exists, but is not wired
into the tier report) and Tier 3 (cross-implementation differential against an external
Modelica / Buildings toolchain). No sequence here has been executed against the normative
reference implementation; that is the deliberately deferred tail.

The project is **pre-1.0** and not yet published to crates.io.

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

**Input quality is the host's job, not the engine's.** Staging is deliberately status-agnostic: a
sample is converted from its value regardless of `PointStatus`, so `Fault`, `Stale`, and
`Uninitialized` all stage exactly like `Ok`. A missing sample is not an error either — the
connector holds its previous value indefinitely (before the first sample, the type's
`zero_value()`). The engine therefore implements **no fail-safe policy of its own**. An embedder
driving real equipment must enforce staleness limits, fault reactions, and safe-state fallback in
the host layer above the engine.

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
  arrays: no graph walks, no hashing, and no store access in the graph evaluator. Two carve-outs,
  both real:
  - **The evaluator is not allocation-free for every block.** The schedule and state arrays are
    preallocated and the gather scratch is reused, so most blocks tick without allocating. Two
    exceptions, both measured: `Reals.Sort` heap-allocates **two `Vec`s on every tick,
    unconditionally, with valid inputs** (`reals_matrix.rs:387`, `:401`), and `Reals.Log` /
    `Reals.Log10` `format!` a diagnostic string when handed a non-positive input. Size a real-time
    loop against the blocks your sequence actually uses, not against a blanket guarantee.
  - **`Engine::tick` is store-free only when the model declares no store-backed inputs.**
    Otherwise it takes one `store.snapshot()` per tick plus one read per staged input. With the
    default `MemStore` that is exactly one boxed allocation; the `PointStore` trait places **no**
    allocation bound on a third-party backend's `snapshot()`.

---

## Quickstart

> The facade crate is **`oce-api`** — that is the package name today, and the name the workspace
> publish uses. Releasing it under the umbrella name **`open-control-engine`** is a planned change,
> not a current alias. Until the first crates.io release, depend on it via git:

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
| `oce-expr` | The CDL §7.7.2 binding-expression parser/evaluator (closed-world, pure). Total over the value domain; **not** over arbitrary nesting depth — neither the parser nor the evaluator carries a depth guard, so sufficiently deep input can exhaust the thread stack. Treat untrusted expression input as a hardening gap, not a solved problem. |
| `oce-blocks` | The `Block` trait and the native CDL elementary-block library (stateless `[A]` / stateful `[S]`), publicly enumerable via `catalog()` — ports, parameter rules, and honest parameter defaults per class. |
| `oce-flatten` | **Reserved seam; an identity passthrough today.** `oce-cxf` owns lowering because CXF arrives pre-flattened, so this crate currently returns the model unchanged. Full `.mo` flattening is deferred. |
| `oce-validate` | Loader conformance: subset rejection, single-assignment, type/attribute unification, parameter rules. |
| `oce-graph` | The deterministic scheduler/executor: direct-feedthrough DAG, algebraic-loop rejection, own Kahn topological sort, the tick loop. |
| `oce-cxf` | CXF (Control eXchange Format) JSON-LD ↔ the model graph, both directions. Import is the §7.1 resolver; export emits the flat/ground/scalar subset under the RT-2 round-trip contract, deferring enum-carrying blocks with warnings rather than failing. `export_with_report` surfaces those warnings; plain `export` discards them. Direct boundary input→output connects lower to reserved pass-through identities on import and elide exactly on export. |
| `oce-semantics` | **Reserved seam; annotation parsing is deferred.** The intended role is vendor-annotation parsing → effective (non-computational) point/trend/semantic metadata. No `__cdl` / `__CDL` annotation parsing is implemented today. |
| `oce-diag` | The shared diagnostic vocabulary (`Severity` / `DiagCode` / `Diagnostic`) across the ingest path. |

**Storage ports (the seam — traits only, no database types):**

| Crate | Responsibility |
| --- | --- |
| `oce-store` | **The seam.** The `ModelStore` / `PointStore` / `SemanticStore` traits + DTOs. No database types. |
| `oce-store-mem` | The default in-memory backend, so the engine runs with no database. |
| `oce-reference-wal-adapter` | **Verification-only, `publish = false`.** A `std::fs` WAL + atomic snapshot adapter that exists to prove the frozen seam can carry real durability without a first-party database. Not a supported backend — durable/queryable adapters stay app-side. |

**Verification, externals & host facade:**

| Crate | Responsibility |
| --- | --- |
| `oce-conformance` | The funnel-style tolerance-band / golden-trace conformance harness. |
| `oce-extension` | **Reserved seam; nothing consumes it yet.** The intended role is the FMI / extension-block boundary. No crate depends on it today, the CXF resolver has no extension-block branch (an unknown class is a hard `ClassNotFound`), and `DiagCode::MissingFmuPath` is never constructed. Do not plan FMI integration against this row. |
| `oce-docs` | **Reserved seam, not implemented.** The sequence-spec (Word/HTML) and point-list export surface is declared; every exporter is deferred to M4 and `point_list_html` currently panics with `unimplemented!`. |
| `oce-api` | The embeddable host facade: `Engine<S: Store = MemStore>` — the single public surface, spanning load, tick, simulate, parameters, IO inventory, CXF export with content id, and a read-only topology view. Package name is `oce-api`; the `open-control-engine` umbrella name is planned for first publish. |

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

CI is **dev-light / release-heavy**. The per-PR gate into `development` runs fmt, clippy
`-D warnings`, build, rustdoc `-D warnings`, the file-size cap, the no-secret scan, the
database-free check, `cargo-machete`, the golden-generator anti-tautology firewall, the gate-script
behavior fixtures, and a determinism subset covering `oce-blocks` and `oce-expr` on two
architectures in debug and release codegen. One job runs `.agents/gate.sh` itself, so the commands
above gate a PR whether or not each is also wired as its own job.

**No PR into `development` runs the full test suite.** It runs on `development → main` release
gates, on a daily cron against `development`, and on manual dispatch. Read that in the dangerous
direction: a change confined to `oce-cxf`, `oce-store`, `oce-api` or `oce-diag` can show every check
green having run none of its own tests. Run `bash .agents/gate.sh full` before claiming otherwise.

**Open your PR non-draft.** Every job in `ci.yml` is conditioned on
`github.event.pull_request.draft == false`, so a draft PR runs no gates at all — not a subset,
none.

---

## Build & develop

```bash
cargo build --workspace        # the engine only — no database, no async runtime
```

`oce-api` declares one feature today, `default = ["mem"]`. It is a marker rather than a switch:
`mem = []` gates nothing, and `oce-store-mem` is an unconditional dependency, so disabling default
features does not remove the in-memory backend. What actually makes it the default is the type
parameter — `Engine<S: Store = MemStore>`. Durable/queryable backends are the consuming
application's responsibility, authored app-side as an adapter behind the `oce-store` port.

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
