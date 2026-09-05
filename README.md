# Open Control Engine

**Open Control Engine runs building control sequences written in the OBC / LBL
[Control Description Language (CDL)](https://obc.lbl.gov/specification/cdl.html), as a Rust
library you embed in your own application.**

[![License: Apache-2.0 OR MIT](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](#license)
[![Rust 1.97.0 MSRV · edition 2024](https://img.shields.io/badge/rust-1.97.0%20MSRV%20%C2%B7%20edition%202024-orange.svg)](docs/architecture.md#embeddability-posture)
[![database-free](https://img.shields.io/badge/storage-database--free-success.svg)](docs/architecture.md)
[![documentation site](https://img.shields.io/badge/docs-jscott3201.github.io-informational.svg)](https://jscott3201.github.io/open-control-engine/)

You hand it a control sequence as a CXF document (CDL's JSON-LD exchange format). It parses,
validates, and compiles that sequence into a frozen topological schedule, then ticks the schedule
deterministically — the same document, parameters and input sequence produce bit-identical outputs,
run to run and across x86_64 and arm64, checked against committed goldens on both architectures on
every pull request.

It is a library and nothing else: no `main`, no daemon, no network listener, no async runtime, and
no database. Everything non-computational — points, trends, tags, durability — sits behind a
storage port your application implements. That split is not a design preference; it is CDL §7.17,
which states that such metadata does not affect the computation of a control signal.

Today it loads and simulates **46 ASHRAE Guideline 36 sequence fixtures** against a registry of
**133 CDL block classes**. It is **pre-1.0 and not published to crates.io**.

Read the [product contract](docs/product-contract.md) for the versioned executable-CXF/HostTick
boundary, current limitations, host obligations and future acceptance outcomes.

---

## Who this is for

- **BAS and OEM product teams** who need a sequence runtime inside a controller or supervisory
  product, without adopting a database or a runtime framework along with it.
- **Commissioning and FDD tool authors** who need the same sequence to produce the same numbers
  today that it produced last quarter, as evidence rather than as a hope.
- **CDL researchers and toolchain authors** who want an independent executable implementation to
  compare against.

If you are looking for a finished building-automation product, this is not that. It is the engine
such a product would be built on.

---

## Quickstart

The facade package is **`oce-api`**. `open-control-engine` is a reserved umbrella name for a future
release, **not** a current alias — nothing is on crates.io yet, so depend on it via git. Pin a
revision appropriate to your release process rather than following a moving branch:

```toml
[dependencies]
oce-api = { git = "https://github.com/jscott3201/open-control-engine", rev = "<commit-sha>" }
```

Load a CDL sequence from CXF and simulate it:

Every point is named by an authored `@id` from the CXF document, expanded against the document's
`@context` to canonical absolute form at ingest — the declared boundary input's `@id` for a
boundary-driven point, the connector's own otherwise — so the same key names the same point
across loads of the same document, including a document re-serialized between compact and
expanded spellings. The document's declared boundary-output names (root `S231:hasOutput`) read
as aliases for their driving connectors on `get_output`, `watch`, and `CollectSpec::Named`;
internal connector paths, like the three below, remain valid output identities alongside them.

```rust
use oce_api::{CollectSpec, Engine, InputSource, SimSpec, Value};

const ECONOMIZER: &str = "http://example.org#g36.ahu_economizer";
const ECONOMIZER_ENABLED: &str = "http://example.org#g36.ahu_economizer.enableLatch.y";
const DAMPER_COMMAND: &str = "http://example.org#g36.ahu_economizer.damperSwitch.y";
const OA_TEMPERATURE_DELTA: &str = "http://example.org#g36.ahu_economizer.returnMinusOutdoor.y";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // An engine with the default in-memory store — no database.
    let mut engine = Engine::in_memory();

    // Parse, validate, and freeze the schedule.
    let cxf_bytes = std::fs::read("crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld")?;
    engine.load_cxf(&cxf_bytes)?;

    // Simulate: feed inputs per tick, collect named outputs.
    let metrics = engine.simulate(&SimSpec {
        t_start: 0.0,
        t_stop: 4.0,
        step: 1.0,
        inputs: InputSource::Closure(Box::new(|t| {
            vec![
                (format!("{ECONOMIZER}.return_air_temp"), Value::Real(24.0)),
                (
                    format!("{ECONOMIZER}.outdoor_air_temp"),
                    Value::Real(18.0 + t),
                ),
                (format!("{ECONOMIZER}.operating_mode"), Value::Integer(1)),
            ]
        })),
        collect: CollectSpec::Named {
            points: vec![
                ECONOMIZER_ENABLED.to_string(),
                DAMPER_COMMAND.to_string(),
                OA_TEMPERATURE_DELTA.to_string(),
            ],
            stride: 1,
        },
    })?;

    println!("times: {:?}", metrics.trace.times());
    for (index, name) in metrics.trace.columns().iter().enumerate() {
        println!(
            "{name}: {:?}",
            metrics.trace.column(index).unwrap_or_default()
        );
    }
    Ok(())
}
```

To run the engine's own tests from a clone:

```bash
git clone https://github.com/jscott3201/open-control-engine
cd open-control-engine
cargo nextest run -p oce-api -p oce-blocks -p oce-expr # the per-PR engine subset
bash .agents/gate.sh                            # the gate script CI runs; see its closing report
                                                # for the per-PR checks it cannot cover locally
```

---

## What it does today

- Imports CXF via the CDL §7.1 resolver, and **exports** back to CXF under a round-trip contract
  where re-importing the emitted bytes renders bit-identically — Reals compared by IEEE-754 bits,
  not by epsilon. See [CXF round trip](docs/cxf-round-trip.md).
- Executes a registry of **133 CDL elementary block classes** plus 3 reserved internal lowering
  classes, enumerable at runtime via `oce_blocks::catalog()` with per-class ports, parameter rules,
  and honest parameter defaults. See [CDL coverage](docs/cdl-coverage.md).
- Runs **46 G36 conformance fixtures** end to end through the frozen facade, each with a committed
  whole-sequence golden trace.
- Commits computed outputs through the storage port after a real-time step, with host-supplied
  timestamps — the seam never invents time.

## What it does not do

Stating this plainly is more useful than a feature list.

- **It does not parse or flatten Modelica `.mo` sources.** It executes the block graph a CXF
  document hands it. `oce-flatten` is a reserved seam that returns the model unchanged.
- **It is not general ASHRAE G36 support.** The supported set is explicitly
  *selected-explicit-cxf-variants-supported*: pre-flattened CXF at specific parameterizations, not
  other G36 composites. [What "supported" means](docs/cdl-coverage.md).
- **Its external-reference evidence is four cases, not broad engine coverage.**
  `CDL.Logical.Nand` has one exhaustive Boolean case, `CDL.Logical.Toggle` has one stateful Boolean
  event schedule, and `CDL.Reals.Line` has one finite matrix covering four limit modes and five
  input regions. One composed G36 Reliefs leaf has a seven-state exact-bit case at its declared
  outputs. No complete G36 sequence or general numeric tolerance has been checked that way, and the
  global Tier-3 report remains skipped —
  [read the full accounting](docs/verification-evidence.md).
- **`CDL.Logical.Pre` is a host-tick delay, not Modelica event iteration.** Under the fixed
  [HostTick v1 profile](docs/execution-profile.md), every successful `Engine::tick` call advances
  `Pre` once, including repeated calls at the same timestamp. Exact Modelica/OpenModelica `Pre`
  equivalence is outside the conformance claim.
- **It has no Python bindings**, no daemon, no scheduler, and no database.
- **`halt()` does not stop execution.** It only opens the tune-at-rest window in which
  `set_param` is accepted; ticks, real-time steps, and simulations continue if the host calls them.
- **Deferred loader signatures have been removed.** `load_from_semantic` and `load_modelica`
  are no longer callable; prepare supported CXF externally and use `load_cxf`.
  See [facade migration](docs/facade-migration.md) for the pre-release source break.
- **Assertion events and `AssertLevel` are Warning-only.** `AssertLevel::default()` is now
  `Warning`; the never-emitted `AssertLevel::Error` variant was removed. This adds no escalation
  or safety policy. See [facade migration](docs/facade-migration.md).

---

## Before you drive equipment

The engine deliberately implements **no fail-safe policy of its own**, and that is a decision your
host layer has to answer for:

- **Staging is status-agnostic.** A sample is converted from its value regardless of `PointStatus`
  — `Fault`, `Stale`, and `Uninitialized` all stage exactly like `Ok`.
- **A missing sample is not an error.** The connector holds its previous value indefinitely. A dead
  sensor is indistinguishable from a steady one, for as long as it stays dead.

Staleness limits, fault reactions, and safe-state fallback belong in the host above the engine.
**[Host responsibilities](docs/host-responsibilities.md)** is the checklist; read it before wiring
anything to a physical output.

---

## Architecture

CDL §7.17 states that point lists, trends, display units, tags, and Brick / Haystack / ASHRAE 223P
semantics do not affect the computation of a control signal. That one rule is the seam the whole
system is built around: an execution core that sees only blocks, typed connections, and values, and
a storage port for everything else.

![The execution core sees only blocks, connections, and values and has no database; everything non-computational sits behind the oce-store port](docs/diagrams/architecture-seam.svg)

Full layer-by-layer detail, the crate map, and the platform and MSRV policy are in
**[Architecture](docs/architecture.md)**.

---

## How it is verified

Six evidence layers in this repository are called "tests", and they prove different things.
One of them proves nothing about correctness at all — the 46 fixture goldens are **engine
self-output**, a determinism snapshot that catches drift, not wrongness.

Correctness is bounded separately by 412 provenance records generated by a tool held off the
workspace and **forbidden from depending on the block library**, with CI enforcing that
code-dependency firewall. Of the 410 signal goldens, 390 check CDL / Buildings source semantics and
20 G36 signals across three `Pre`-dependent fixtures check the HostTick v1 profile instead. In all,
389 are compared bit-exactly, including all 132 G36 sequence goldens, and the 21 transcendental,
psychrometric, and solar Real goldens use a documented 1e-12 aligned-tolerance band.

Two global report tiers are **not wired**, and no complete G36 sequence here has been executed against an
external Modelica / Buildings toolchain. The separate OpenModelica evidence covers exhaustive
Boolean Nand, one stateful Boolean Toggle schedule, one finite exact-bit Line matrix, and one
seven-state exact-bit case for a composed G36 Reliefs leaf.

**[Verification and evidence](docs/verification-evidence.md)** sets out what each layer proves, what
it cannot, and which checks are not running.

---

## Documentation

The `docs/` pages below are also published as a site:
**[jscott3201.github.io/open-control-engine](https://jscott3201.github.io/open-control-engine/)**.
`TESTING.md` and `SECURITY.md` are not on it; they are repository-only. The site is built from
`main`, so it trails `development` by a release — where the two differ, the Markdown in this
repository is the newer copy.

| Page | For |
| --- | --- |
| [Architecture](docs/architecture.md) | Layers, the §7.17 seam, the crate map, platform and MSRV |
| [Execution profile](docs/execution-profile.md) | HostTick semantics and the `CDL.Logical.Pre` conformance boundary |
| [Verification and evidence](docs/verification-evidence.md) | What has been proven, and what has not |
| [CDL coverage](docs/cdl-coverage.md) | Which classes and sequences run, and what "supported" means |
| [CXF round trip](docs/cxf-round-trip.md) | Export guarantees, and where it silently drops things |
| [CXF composite subset](docs/cxf-composite-subset.md) | Normative contract for external CXF emitters |
| [Host responsibilities](docs/host-responsibilities.md) | What you must implement before driving equipment |
| [CI and the gate](docs/ci-and-the-gate.md) | What runs when, and what a green check proves |
| [Benchmarks](docs/benchmarks.md) | Measured tick throughput, per run |
| [Testing standard](TESTING.md) | The bar every change is held to |
| [Security](SECURITY.md) | Reporting, threat model, and known limits |

---

## Contributing

Changes land via pull requests into `development`. Install the shared git hooks once after cloning
with `bash scripts/install-hooks.sh`, and run `bash .agents/gate.sh` before opening a PR — that
script is the single source of truth for what CI runs.

Read **[CONTRIBUTING.md](CONTRIBUTING.md)** first, and **[TESTING.md](TESTING.md)** before writing a
test. Notable changes are in **[CHANGELOG.md](CHANGELOG.md)**.

One thing worth knowing up front: the per-PR gate runs engine tests for `oce-api`, `oce-blocks`, and
`oce-expr` only. A change confined to another crate can show every check green having run none of
its own tests. [CI and the gate](docs/ci-and-the-gate.md) explains the split.

---

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you shall be dual-licensed as above, without any additional terms or
conditions.

`third_party/` vendors upstream Modelica Buildings CDL sources and modelica-json CXF translations
verbatim, under their own license — see
[`third_party/modelica-buildings-cdl/README.md`](third_party/modelica-buildings-cdl/README.md). That
tree sits outside every crate root, so `cargo package` never ships it.
