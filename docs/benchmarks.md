# Benchmarks

Measured tick throughput for the Open Control Engine, recorded per run with the commit, host and
method that produced it.

## Read this first

**Historical throughput numbers are not gated.** Nothing in CI or in `.agents/gate.sh` re-measures those runs, so they
are a record of what was observed, not a promise about `HEAD`. A performance figure that no test
enforces drifts silently — the same failure mode that got a git SHA deleted from every provenance
record in [PR #204](https://github.com/jscott3201/open-control-engine/pull/204), and the reason
the numbers live here rather than in `README.md`, where they would be read as a standing claim.

Treat a run below as evidence about **that commit on that host**. To make a claim about a
different commit, measure the current contract with the compiled harness described below. The
retired execution profiles are not interchangeable with complete-frame work.

The complete-frame harness below now runs structural allocation assertions and emits non-gating
latency observations. Historical throughput runs retain their original method and limits.

## Complete-frame observations

2026-09-19, implementation working tree based on `64e7ba83ff78a6d07750502d0f3f037b8b39f519`.
Apple M5, aarch64-apple-darwin, macOS 27.0 (26A5425a), Rust 1.97.1. Default features, locked
dependencies; dev/debug and release (workspace thin LTO, one codegen unit). These are local
observations, not hosted x86_64/arm64 qualification, equipment cadence or universal speed claims.

The historical version of [`frame_observations.rs`](../crates/oce-api/tests/frame_observations.rs)
used five representative fixtures. Load was excluded. Constant schema-valid synthetic inputs were staged once for legacy tick;
native frames supply the same complete values each time. Equal model time 0.0 deliberately tests
repeat transitions. After 64 warmup calls, five 2,048-call batch means are measured, alternating
commit/tick order. Commit timing excludes preparation and outer plan-batch allocation but includes
plan and result destruction. End-to-end preparation+commit is also reported. Legacy tick includes
its MemStore snapshot where inputs are bound, uses no-op diagnostics and produces no retained frame,
so this is a cost comparison between different contracts, not an equivalent-work speedup.
Only this test was scheduled during explicit measurements; other machine activity was not controlled.

Median of five batch means, nanoseconds per call:

| Case | Debug tick | Debug commit | Debug prepare+commit | Release tick | Release commit | Release prepare+commit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Add arithmetic | 211 | 390 | 1,357 | 34 | 76 | 171 |
| Sampled delay | 345 | 518 | 1,473 | 42 | 79 | 158 |
| Zero-input Pre feedback | 282 | 471 | 533 | 35 | 68 | 69 |
| 213-block G36 controller | 18,631 | 17,714 | 26,550 | 2,578 | 2,557 | 3,246 |
| Assert, zero boundary outputs | 214 | 466 | 993 | 28 | 79 | 126 |

Commit batch-mean ranges (debug / release ns): arithmetic 388–399 / 63–78;
delay 488–526 / 78–86; feedback 419–473 / 63–69; controller 17,625–17,951 / 2,538–2,637;
assertion 453–475 / 72–91. These short samples expose host drift, not tail latency or confidence
intervals. No timing value or ratio is an assertion; the functional determinism tests are separate.

The same debug/release harness asserts the exact allocation formula over 128 preparation+commit
repetitions per fixture and zero outstanding allocations after drop, with a 1,024-byte positive
control. It counts the synchronous thread only. N is logical inputs, T fan-out targets, B boundary
outputs and D emitted warnings (these cases have at most one):

| Case | N / T / B / D | Preparation allocations / bytes | Commit allocations / bytes |
| --- | --- | --- | --- |
| Add | 2 / 2 / 1 / 0 | 4 / 120 | 2 / 66 |
| Delay | 2 / 2 / 1 / 0 | 4 / 120 | 2 / 59 |
| Pre | 0 / 0 / 2 / 0 | 0 / 0 | 3 / 114 |
| Controller | 14 / 43 / 10 / 0 | 16 / 956 | 11 / 1,136 |
| Assert | 1 / 2 / 0 / 1 | 3 / 64 | 3 / 262 |

Preparation uses N+2 buffers for nonempty N. Commit capture uses B+1 buffers for nonempty B, with
`B * size_of::<(String, Value)>() + sum(path_bytes)` bytes; zero B allocates none. Each warning
copies its source/message and grows a Vec geometrically (one warning uses four event slots).
Value cloning preserves bits and shares immutable String payloads. This budget is structural:
no shadow RunState, whole-engine copy or rollback. Existing evaluator allocations, notably wide
Sort, remain a separate baseline cost; this fixture census does not erase them.

The shared evaluation-core extraction based on `8ea3e8f38d580868179bfa985b443b71ca3b86c1`
re-ran this exact census in debug and release on aarch64-apple-darwin/Rust 1.97.1: all five
preparation/commit counts and byte budgets above remain unchanged. The latency test also ran,
but these observations are not a speed gate or a before/after performance claim. The historical
timing table above is not re-blessed. The frame-only contraction removes the legacy comparison;
the current harness measures commit and preparation-plus-commit only, retaining the allocation
formula checks. All accepted frames now retain warnings. No current latency claim is inferred from
the historical tick columns.

Run observations explicitly with `--success-output immediate --test-threads 1` on the focused
`frame_observations` nextest binary. It is also included in the existing oce-api matrix test set;
normal success-output suppression hides passing timing logs, but allocation checks still execute.
Use `--locked --profile ci --no-tests=fail` for debug and
`--locked --profile ci-release --cargo-profile release --no-tests=fail` for release. The full
repository gate remains [`.agents/gate.sh`](../.agents/gate.sh), not this measurement selection.

## What the historical throughput runs measured

The now-retired steady-state `Engine::tick()` on real G36 fixtures, through the then-public facade:
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
  usually matters more than the mean, and one property that governs it — whether the evaluator
  thread allocates during a block tick — is gated separately and much more strictly, by
  `crates/oce-blocks/tests/tick_allocation_census.rs` (registry-wide, with a positive control),
   which runs per-PR. Current facade frame allocation and Store-noninterference checks also run
   per-PR in `frame_observations.rs` and `frame_purity.rs`. Historical throughput is not gated.
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

These numbers were measured before #230, which added a working-clone `@context` expansion pass to
ingest; load cost moved roughly +9–12% there, tick cost not at all.

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
`flatten`, §7.10 attribute unification, structural validation, then the build tail (registry,
schedule, state, outputs, io, params, store recovery). That is not a JSON parse, so a plain parser
MB/s intuition does not apply. The measured revision re-ran pure validation in the build tail.
Current `load_cxf` validates once before entering `build_validated_model_in_memory`
(`crates/oce-api/src/engine.rs:231-244`), so this historical table includes work the current path no
longer performs.

**Observation — cost is linear in block count.** Across a 28× size range the per-block cost holds
at roughly 11 ns (11.3 / 11.9 / 10.6 / 11.8 ns for the four largest). `vav_single_zone` is the
exception at ~18 ns per block, and it is the expected one: at 8 blocks the fixed per-tick overhead
(finite/monotonic time checks, output refresh) stops being amortised. No superlinear term is
visible, so a sequence twice the size costs about twice as much.

**Caveat on "CDL class refs".** That column counts CDL type references in the fixture's JSON-LD.
It is a proxy for scheduled block count, not the count itself, so the per-block figures are
indicative rather than exact. The linearity across 28× is the load-bearing part and does not
depend on the proxy being tight.

**In deployment terms.** `cooling_only_controller` is the largest fixture *document* in the corpus
(409 KiB; 213 blocks and 268 connections after import) and ticks in ~2.5 µs. Building control
sequences run at a 1 Hz cadence or slower.

> **Correction, 2026-07-31.** This paragraph previously called it "the largest sequence in the
> fixture corpus (213 instances, 377 edges)". Both halves were wrong: `relief_fan_group` imports
> to 226 blocks, more than this fixture's 213 (pinned at
> `crates/oce-cxf/tests/resolve_g36_relief_fan_group.rs:145-146`), and 377 was the pre-import
> `isConnectedTo` count, not the 268 connections the tick loop actually runs. The measured timing
> above is unchanged and was not re-run — only the description of what was measured is corrected.

## Reproducing a run

Use the repository's compiled `frame_observations` nextest binary for current measurements, with
the focused settings above. It submits a complete frame on every iteration and separately reports
commit-only and preparation-plus-commit cost. There is no sparse staging or implicit hold-last
benchmark profile. Historical tables retain the method and limits of their recorded revisions;
they are not predictions for the frame-only facade. No older timing table has been regenerated.

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
