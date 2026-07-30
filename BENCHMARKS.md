# Benchmarks

Measured tick throughput for the Open Control Engine, recorded per run with the commit, host and
method that produced it.

## Read this first

**These numbers are not gated.** Nothing in CI or in `.agents/gate.sh` re-measures them, so they
are a record of what was observed, not a promise about `HEAD`. A performance figure that no test
enforces drifts silently — the same failure mode that got a git SHA deleted from every provenance
record in [PR #204](https://github.com/jscott3201/open-control-engine/pull/204), and the reason
the numbers live here rather than in `README.md`, where they would be read as a standing claim.

Treat a run below as evidence about **that commit on that host**. To make a claim about a
different commit, re-run it — the method is fully specified, and the harness is reproduced in this
file so anyone can.

A gated harness is tracked work. Until it lands, this file is updated by hand.

## What is measured

Steady-state cost of `Engine::tick()` on real G36 fixtures, through the public facade only:
`Engine::in_memory()` → `load_cxf()` → `tick()`.

- **Load is excluded from the tick figure** and reported separately. Loading happens once;
  ticking happens forever, so blending them would flatter the result and describe neither.
- Ticks are run for a fixed wall-clock window after a warmup, so first-touch page faults and any
  lazily initialised state land in the warmup rather than the measurement.
- The clock is read in batches, so the timing call is not itself the workload.
- Time advances monotonically. The engine rejects time regression, so `t` only ever increases
  across warmup and measurement.

## What is *not* measured

Stated explicitly, because the gap between these and the numbers below is where a wrong
conclusion would come from.

- **Load / parse / resolve throughput** beyond the single `load_ms` column.
- **Tail latency.** These are means over millions of ticks. For equipment control the tail
  usually matters more than the mean, and the property that governs it — whether a tick allocates
  at all — is gated separately and much more strictly, by
  `crates/oce-blocks/tests/tick_allocation_census.rs` (registry-wide, with a positive control) and
  `crates/oce-api/tests/tick_purity_tests.rs`. Those run per-PR; this file does not.
- **Multi-core or concurrent engines.** Single engine, single thread.
- **Any architecture other than the one in the run header.** CI runs a determinism matrix across
  x86_64 and arm64 precisely because one machine does not speak for both.
- **Store-backed input staging under a real durable store.** Runs below use the in-memory store.

## Runs

### 2026-07-30 · `b5b19e7` · Apple M5 (10 cores) · rustc 1.95.0 · macOS 26.6 · `--release`

Warmup 20,000 ticks · 2.0 s measurement window · `dt` = 1.0 simulated second per tick.

| fixture | CDL class refs | ns/tick | ticks/sec |
| --- | ---: | ---: | ---: |
| `cooling_only_controller` | 222 | 2,508 | 398,801 |
| `multizone_vav_relief_fan_group` | 228 | 2,706 | 369,558 |
| `multizone_vav_supply_fan` | 71 | 755 | 1,325,349 |
| `ahu_economizer` | 12 | 141 | 7,095,155 |
| `vav_single_zone` | 8 | 144 | 6,932,518 |

Repeated back to back; the two runs agreed within ~2% on the large fixtures and ~6% on the
smallest. Measured on an otherwise idle machine — an earlier attempt taken while a parallel build
was running produced numbers that were not reproducible, which is why the method above insists on
it.

### Load, same commit and host — 60 iterations per fixture

Reported as first / median / min in one process, because a single first-call figure carries
process-start and page-cache cost.

| fixture | KiB | first ms | **median ms** | min ms | first÷med | MiB/s at median |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `cooling_only_controller` | 409 | 7.84 | **2.62** | 2.48 | 3.0× | 154 |
| `multizone_vav_relief_fan_group` | 366 | 2.55 | **2.12** | 2.02 | 1.2× | 169 |
| `multizone_vav_supply_fan` | 112 | 0.89 | **0.68** | 0.66 | 1.3× | 159 |
| `ahu_economizer` | 16 | 0.14 | **0.11** | 0.10 | 1.3× | 144 |
| `vav_single_zone` | 14 | 0.11 | **0.09** | 0.08 | 1.3× | 150 |

**Correction to an earlier revision of this file.** It reported `11.0 ms` for
`cooling_only_controller` and placed the "runs agreed within ~2%" sentence where it read as
covering that column too. Both were wrong. That figure was a **single first call in a cold
process** — it is the first fixture measured, so it absorbed process start and page-cache misses.
Measured properly it is **2.62 ms**, a 4.2× overstatement. A second process invocation shows the
same fixture's first/median ratio collapse from 3.0× to 1.3× once the page cache is warm, while
every other fixture sat at 1.1–1.5× in both runs. The tick figures above were unaffected: they were
always taken after a 20,000-tick warmup, which is exactly the discipline the load column lacked.

**Observation — load throughput is flat.** 144–169 MiB/s across a 29× size range, through the whole
`load_cxf` pipeline: `import_cxf` (JSON-LD parse **and** resolve to a flat ground ModelGraph),
`flatten`, `unify_and_validate` (§7.10 unification, which mutates the graph), then the build tail
(registry, schedule, state, outputs, io, params, store recovery). That is not a JSON parse, so a
plain parser MB/s intuition does not apply. Note the build tail deliberately re-runs pure
`validate` — see `crates/oce-api/src/engine.rs:190-192` for why — so validation happens twice per
load.

**Observation — cost is linear in block count.** Across a 28× size range the per-block cost holds
at roughly 11 ns (11.3 / 11.9 / 10.6 / 11.8 ns for the four largest). `vav_single_zone` is the
exception at ~18 ns per block, and it is the expected one: at 8 blocks the fixed per-tick overhead
(finite/monotonic time checks, output refresh) stops being amortised. No superlinear term is
visible, so a sequence twice the size costs about twice as much.

**Caveat on "CDL class refs".** That column counts CDL type references in the fixture's JSON-LD.
It is a proxy for scheduled block count, not the count itself, so the per-block figures are
indicative rather than exact. The linearity across 28× is the load-bearing part and does not
depend on the proxy being tight.

**In deployment terms.** `cooling_only_controller` is the largest sequence in the fixture corpus
(213 instances, 377 edges) and ticks in ~2.5 µs. Building control sequences run at a 1 Hz cadence
or slower.

## Reproducing a run

The harness deliberately lives **outside** the repository, in a scratch directory. It is not a
crate target, so it ships nothing, adds no dependency to the workspace, and cannot perturb the
nextest test count or `cargo package`. Where a permanent harness should live is the open design
question in the tracked follow-up.

```bash
mkdir -p /tmp/tickbench/src && cd /tmp/tickbench
cat > Cargo.toml <<'EOF'
[workspace]

[package]
name = "tickbench"
version = "0.0.0"
edition = "2024"

[dependencies]
oce-api = { path = "/ABSOLUTE/PATH/TO/open-control/crates/oce-api" }

[profile.release]
debug = true
EOF
```

`src/main.rs` — adjust the `include_str!` paths to your checkout:

```rust
use std::time::Instant;

use oce_api::Engine;

const FIXTURES: &[(&str, &str)] = &[(
    "cooling_only_controller",
    include_str!("/ABSOLUTE/PATH/TO/open-control/crates/oce-cxf/tests/fixtures/g36/cooling_only_controller.jsonld"),
)];

const MEASURE_SECS: f64 = 2.0;
const WARMUP_TICKS: u64 = 20_000;
const DT: f64 = 1.0;

fn main() {
    for (name, cxf) in FIXTURES {
        let load_start = Instant::now();
        let mut engine = Engine::in_memory();
        if let Err(e) = engine.load_cxf(cxf.as_bytes()) {
            println!("{name}: load failed: {e:?}");
            continue;
        }
        let load_ms = load_start.elapsed().as_secs_f64() * 1e3;

        let mut t = 0.0_f64;
        for _ in 0..WARMUP_TICKS {
            t += DT;
            if engine.tick(t).is_err() {
                break;
            }
        }

        let start = Instant::now();
        let mut ticks: u64 = 0;
        loop {
            t += DT;
            if engine.tick(t).is_err() {
                break;
            }
            ticks += 1;
            if ticks % 4096 == 0 && start.elapsed().as_secs_f64() >= MEASURE_SECS {
                break;
            }
        }
        let secs = start.elapsed().as_secs_f64();
        println!(
            "{name}: load {load_ms:.1} ms · {:.0} ns/tick · {:.0} ticks/sec",
            secs * 1e9 / ticks as f64,
            ticks as f64 / secs
        );
    }
}
```

```bash
cargo build --release && ./target/release/tickbench
```

Run it on an **idle** machine, and run it at least twice — a figure that does not reproduce is not
a measurement.

**To measure load rather than ticks**, loop the `Engine::in_memory()` + `load_cxf` pair on its own
(60 iterations is plenty) and report **first / median / min**, not a single call. The first call in
a cold process absorbs process start and page-cache misses; on the largest fixture that inflated the
figure by 3× and produced the erroneous `11.0 ms` corrected above. Reporting only a median hides the
cold cost from anyone who cares about startup, and reporting only a first call is simply wrong —
report both. Time `Engine::in_memory()` inside the measured region and let the engine drop outside
it, since teardown is not part of load.

## Adding a run

Append a new `### <date> · <short SHA> · <host> · <toolchain> · <profile>` section above the
previous ones, newest first. Never edit an older run to match a newer one: the value of this file
is the trend, and a rewritten history has no trend in it. If a run regresses, record it and say
so — that is the entire point of keeping the record.
