# 12 — M2 (conformance / oracle + validation) execution plan

**Status:** design-of-record for M2. Produced by the M2 design workflow (`wf_ebf5776b-ded`:
5 parallel design lenses — conformance-harness, oracle-goldens, block-breadth-G36, param-validation,
sequencing-exit — → synthesis → adversarial completeness critic) on 2026-06-19, then reconciled by
the lead by **independently source-verifying the critic's four highest-impact findings** and folding
the corrections back in. Critic verdict was `needs_revision`; the load-bearing gaps it raised are
real (verified against `oce-api/tests/public-api.txt`, `oce-api/Cargo.toml`, `oce-conformance/Cargo.toml`,
`oce-blocks/src/reals.rs`) and are addressed below. Authoritative over doc 09 §2's M2 sketch where
they differ. Mirrors the M0/M1 cadence: each PR is one independently reviewable + mergeable slice,
green CI, adversarial review addressed before merge.

> This file is the *plan*. The normative conformance spec stays in `07-conformance-and-verification.md`;
> the block spec in `03-block-library.md`; the type/value spec in `02`; the frozen API surface in
> `08` §3/§11 and the blessed `oce-api`/`oce-store` `public-api.txt` baselines. Where this plan pins
> a previously-open detail, the owning doc must be updated **in the PR that implements it** (M1 precedent).

> **OWNER DECISIONS — RESOLVED 2026-06-19 (§7 carries the full set + lead defaults for the rest):**
> - **Modelica/Buildings/OM toolchain — DEFERRED ("revisit later"; owner has no Modelica access, support uncertain).** Consequence: the **Tier-B external-reference half of exit #2 is a deferred tail**; **Tier-A closed-form is the primary, authoritative oracle for the entire M2 set** (incl. a from-spec forward-Euler recurrence for the integrating blocks). 5 of 6 exits stay fully closeable now (#1/#3/#4/#5/#6); #2 closes for closed-form-able signals, external-Buildings validation pending. Same bucket: `funnel-xcheck` vs the `lbl-srg/funnel` tool (M2-AD-7) — the in-repo `build_bounds` hand-computed-extrema goldens carry the load until the tool is available.
> - **Exit-#1 fixture = representative hand-authored CXF** (toolchain-derivation reserved only for #2 reference traces). Keeps the A-lane block scope tight.
> - **G36 scope = BOTH AHU sequences (SAT-reset + economizer) + single-zone VAV.** Economizer is now **committed**, so A2 (Hysteresis) and A4 (timing/latch `[S]`) move from stretch to **must-have / critical-path**, and the integer mode-logic + IntegerToReal chain is required.
> - **Param-rule severity (Lane D) = per-class safety case** — shall-error for hazard-bearing clamps (e.g. inverted Limiter on a damper/valve), should-warning only where a deterministic degrade is genuinely safe, each with a one-line safety rationale; owner ratifies the table. No blanket default.

> **STATUS UPDATE — 2026-06-20 (post-merge reconciliation; verified vs `development` tip).** Lane A core
> is MERGED: **A0 (#31), A1 (#32), A2 (#33), A3 (#34), A5 (#35), A6 PID/PIDWithReset-only (#36)**; and
> **B1 (#30, funnel L1 band + CombiTimeTable CSV — the plan's "riskiest, land-earliest" PR) is also
> MERGED.** Registry on `development` = **55 CDL class paths**. The resource-pole critical path
> **A0→A1→A5→A6 is COMPLETE.** Three 2026-06-20 lead decisions now supersede the original §4 A6 row:
> (1) **A6 was SPLIT** — PID/PIDWithReset = #36 (merged); Derivative/MovingAverage/LimitSlewRate + a
> shared `dynamics.rs` helper = **A6b MERGED (#37, `eb6d783`, registry → 58)**; Utilities.Assert = **A6c**
> (A0-only dep). (2) The **PID/derivative discretization is pinned** (OD-PID — mixed forward/implicit
> Euler). (3) **CDL.Routing + the Multi\* family are BLOCKED** on a new vector-port enabler
> (M2-PR-VEC / OD-VEC) because `Value` carries no vector variant. Outstanding unstarted leaves
> (verified absent on `development`): **A4** (timing/latch), **M2-PR-00** (DiagCodes), **C1** (D6
> oracle), **D1** (param rules).

---

## 1. M2 scope (the six exit criteria, verbatim intent from `09` §181-215)

1. **#1 — load+simulate:** a documented G36 subset (**≥1 AHU supervisory + ≥1 VAV-box** sequence) loads
   from CXF and simulates end-to-end, store-agnostic.
2. **#2 — golden traces:** those sequences pass the `oce-conformance` **funnel L1 band** vs reference
   outputs (Buildings/Spawn/modelica-json), with **explicit per-signal abs/rel/time tolerances recorded**.
3. **#3 — per-block bar:** every M2 block ships a unit test + documented `[A]/[S]` class + correct
   `feeds_through()` + an Assert/Diagnostics path **where the block defines one**.
4. **#4 — tiered report:** the harness distinguishes **shall (hard) vs should (advisory)** and emits a
   **Tier 0–4** report.
5. **#5 — determinism re-confirmed:** bit-identical traces across the broadened block set (CDL §7.16).
6. **#6 — store-agnostic + D6 oracle:** still no first-party DB; the D6 schedule cross-check uses a
   **self-contained in-tree** topo oracle (no external graph-algo crate).

---

## 2. Four lanes + one shared precursor

| Lane | Crate(s) | What it delivers | Closes |
|------|----------|------------------|--------|
| **A — block breadth** | oce-blocks (+oce-model enum, oce-graph caller, oce-cxf reg) | Grow registry 13 → ~60–80 elementary blocks for ONE AHU + ONE VAV sequence | #1, #3 |
| **B — conformance** | oce-conformance (+oce-api read-only, oce-model, oce-diag) | funnel L1 band, CombiTimeTable CSV, masking, facade-bound trace driver, Tier 0–4 report | #2, #4, #5 |
| **C — D6 oracle** | oce-graph (tests/ only) | independent topo-sort oracle vs production Kahn schedule, bit-for-bit | #6 (D6 clause) |
| **D — param validation** | oce-validate (+oce-blocks rules, oce-diag) | WI-6/F-002: required-param + numeric-positivity (`SampleTrigger.period>0`, …) | *no exit* (hardening) |
| **shared** | oce-diag | two additive `DiagCode` variants, landed once so B + D don't race the enum | enables #4, D |

Two cross-lane fan-in PRs (E1 load+simulate, E2 golden traces) are the only synchronization points.

---

## 3. Architecture decisions (reconciled; CRITIC-FIX marks a folded-in correction)

| # | Decision |
|---|----------|
| **M2-AD-1** | **Facade-only binding (WI-5 acyclic law).** `oce-conformance` depends on `oce-api` + `oce-model` + `oce-store` (ports) + `oce-diag` only — **never `oce-graph`/`oce-blocks`**. The driver reaches the engine exclusively via the frozen facade: `load_cxf`, `simulate`+`CollectSpec`+`OutputTrace`, and `InputSource::Closure`; for non-uniform `EventAligned` cadences the driver **owns its own tick loop** via the frozen `step_realtime`+`set_input`+`get_output`. **No `oce-api` re-bless for the happy path** (verified: `OutputTrace::columns/times/column`, `simulate`, `step_realtime`, `set_input`, `get_output`, `InputSource::Closure` are all on the M1 frozen surface). |
| **M2-AD-2** **(CRITIC-FIX)** | **Facade param-read gap.** The frozen surface exposes `ParamTable`/`IoInventory`/`PointInfo` as *types* and enumerates *I/O points* — but there is **NO `Engine` method returning a given block's resolved param table** (verified absent in `public-api.txt`). So the `EventAligned` driver **cannot** read a Discrete block's `period` "through the facade param table" as the synthesis claimed. **Resolution: source sample/trigger instants from the verify config** (the `ReferenceSpec`/`TickCadence` already carries the cadence), NOT a new accessor. Adding `Engine::block_params()` would re-bless `oce-api` → **OPEN OWNER DECISION OD-A** (config-sourced, no re-bless [recommended] vs accessor + re-bless). |
| **M2-AD-3** **(CRITIC-FIX)** | **Facade-only CI canary wording.** `oce-api` itself depends on `oce-graph` **and** `oce-blocks` (verified in its `Cargo.toml`), so a `cargo tree -i oce-graph` rooted at the workspace **will** show `oce-conformance` as a reverse-dep *through* `oce-api`. The canary must assert **no DIRECT dependency edge** `oce-conformance → {oce-graph, oce-blocks}` (parse `oce-conformance/Cargo.toml` `[dependencies]`, or `cargo metadata` direct-deps), **not** absence from the transitive tree. As originally worded the check could never pass. |
| **M2-AD-4** | **Funnel L1 = static tolerance-envelope, NOT DTW/sliding-window** (the "funnel" name is a documented misnomer, `07` §3.3 C-2). Per reference point build a rectangle `hw=atolx+rtolx·range_x+ltolx·|x|`, `hh=atoly+rtoly·range_y+ltoly·|y|` (additive); construct monotone-in-x upper/lower piecewise-linear bound curves by corner-selection on local-derivative sign, with segment-intersection + interior-point dropping at extrema; `error=max(0,y−yU)−min(0,y−yL)≥0` at every test-x **and** every bound breakpoint; clamp (no extrapolation) outside range. **`atolx` is purely horizontal rectangle-widening — no timestamp resync/snap/resample.** `compare`/`build_bounds` are pure + deterministic (no FP-sum reorder, no rayon). |
| **M2-AD-5** | **Two-tier verification model** (the conceptual heart of M2). **Bit-exact** (`Value::bit_eq`) for self-consistency (Tier-2 vs prior-self, determinism #5) — our engine is its own deterministic truth. **Tolerance-banded** (funnel L1) for oracle-equivalence (#2) — float Modelica output is *not* bit-reproducible against us, so equivalence is "within declared tolerances." Every test documents which guarantee it asserts. |
| **M2-AD-6** | **Oracle = two tiers by exactness.** **Tier-A (primary, in-repo, no external tool):** a closed-form Rust generator (`tools/golden-gen`, off the cargo-test path) for all exact-math blocks (Reals algebraic/comparator/Hysteresis, Integers, Logical, Discrete UnitDelay/Sampler/Edge/SampleTrigger, Routing, Sources, Constants, Types enums). **Anti-tautology mandate:** derived from CDL spec text, **NOT** from `oce-blocks`, and authored by a different PR/agent than the block impl; a sample is spot-validated against Buildings. **Tier-B (secondary, off-CI):** Buildings + OpenModelica via modelica-json, reserved for the ~6–8 integrating `[S]` blocks (PID/IntegratorWithReset/Derivative/MovingAverage/LimitSlewRate/Timer-TrueDelay) + the two G36 sequences. Committed CSV goldens with sibling `prov.json` + a crate-root `oracle.lock` pinning the toolchain. Spawn deferred to a later differential milestone. **RESOLVED 2026-06-19: Tier-B is a DEFERRED TAIL (no Modelica access). Tier-A closed-form is the PRIMARY authoritative oracle for the whole M2 set** — including a from-spec **discrete recurrence** for the integrating blocks — **forward-Euler** for IntegratorWithReset + the PID integral term; **implicit/backward Euler** `xD=(xD+(dt/T)·e)/(1+dt/T)` for the PID D-term, standalone Derivative, and the LimitSlewRate `(u−y)/Td` lag; the **bounded checkpoint-ring difference** for MovingAverage — matching the schemes mandated in `03` R-REALS-2 / R-DYN-1 (proves the engine computes the *documented discrete scheme* correctly). **The Tier-A oracle MUST mirror the implicit-Euler filter, NOT explicit Euler, or it re-encodes the diverging round-1 scheme (see OD-PID).** Tier-B later proves that scheme is acceptably close to Buildings' *continuous* reference; until then `oracle.lock`/`prov.json` for Tier-B artifacts stay as skeleton/runbook. |
| **M2-AD-7** **(CRITIC-FIX)** | **Funnel external cross-check is MANDATORY, not optional.** The funnel band's worst failure is *undetectable from inside* (a too-loose/mis-built band silently PASSES a real control regression, and every dependent test trusts it — the M1-C1 "green CI on a wrong result" lesson at harness scope). So the `funnel-xcheck` job (cross-checking `compare()` against the upstream `lbl-srg/funnel` binary on the same pairs) is **the required external oracle for `compare()`** (CI-flag-gated, the only subprocess path, never in default `cargo test`). Lead process-evolution call under the safety-critical directive. **RESOLVED 2026-06-19: wiring is gated on `funnel` tool availability (same "revisit later" bucket as the Modelica toolchain).** Until then the **mandatory in-repo substitute** is the `build_bounds` isolation suite — hand-computed-extrema goldens compared bit-exactly — which carries the load with zero external dependency; the `lbl-srg/funnel` cross-check is added when the tool is available. **FP-residue golden discipline (A5/A6 lesson):** determinism/oracle goldens for integrating blocks MUST carry genuine IEEE rounding residue — inputs/params chosen so accumulated values are **non-dyadic** (dyadic-only goldens are tautological) — and with the per-tick increment magnitude **near the accumulator magnitude** so **add-rounding** (not merely product-rounding) is exercised; a tiny increment against a large accumulator is absorbed with no add-rounding and silently hides ordering/precision bugs. |
| **M2-AD-8** **(CRITIC scope-fix)** | **PR-A0 Block-trait Context threading is bigger than "thread a Context."** `step_algebraic/emit_from_state/update_state` are called in **both** `eval_tick` **and** `allocate_state/init_values` (pre-tick source firing calls `step_algebraic(&[],0.0,..)` — no Diagnostics sink exists at allocate time). A0 must update every call site and define **where the `&dyn Diagnostics` sink is owned** at the `oce-graph`→`oce-api` boundary, and how Assert events reach `StepReport.asserts`. |
| **M2-AD-9** **(CRITIC owner-decision)** | **Assert delivery path.** `StepReport.asserts: Vec<AssertEvent>` is **already** on the frozen surface (returned empty in M1) — `step_realtime` can populate it with no re-bless. But `simulate()` returns `SimMetrics`, **not** `StepReport`; where do Assert events from a `simulate()` run go? Adding asserts to `SimMetrics` is a frozen-surface change → **OPEN OWNER DECISION OD-B**. |
| **M2-AD-10** **(CRITIC-FIX)** | **cargo-machete discipline.** `oce-conformance`'s `[dependencies]` is currently **empty** (verified). B1 must declare `oce-model` **only in the same PR that uses `Value`/`ValueKind`** (the CSV path), or `cargo-machete` (a per-PR `ci.yml` gate) fails the declared-but-unused dep. Each per-PR dep-edge addition is explicit: B1 adds `oce-model`; B3 adds `oce-api`; B2 adds `oce-diag`. |
| **M2-AD-11** | **Param-dependent `kind()`/`state_len()` is a NOVEL pattern.** `Greater` is today an `[A]`-only unit struct (`pub struct Greater;`, no `h` field — verified). Making comparators-with-hysteresis and PID modes report `kind()`/`state_len()` that branch on a resolved param is feasible (the arena queries them on the resolved instance) but **unused by all 13 existing blocks** — it gets its own design note + a golden proving `h==0` allocates **0** state words and `h>0` allocates 1. |
| **M2-AD-12** **(2026-06-20)** | **Shared first-order-decay dynamics helper.** The identical `(input−state)/T` implicit/backward-Euler step appears in PID's D term (`T=Td/Nd`), standalone `Reals.Derivative` (`T`), and `Reals.LimitSlewRate`'s `(u−y)/Td` lag. It lives once in **`crates/oce-blocks/src/dynamics.rs`** (`first_order_filter_implicit`, `forward_euler_accumulate`, `tick_dt`, `PREV_T_UNSET`) so the discretization + first-tick `dt=0` handling are defined once and bit-reproducibly (R-DYN-1). The integrator family (`IntegratorWithReset`, PID's integral term) keeps **forward** Euler (no decay term → unconditionally stable). **Status:** the helper is in the in-flight A6b PR (pid.rs/reals_dynamic.rs already import it), pending review/merge. |

---

## 4. PR sequence (16 PRs; mirrors the M1-PR cadence)

> Risk is the lead's review-intensity dial. Deps are hard ordering. `[CRITIC]` notes a folded fix.

| ID | Title | Crate(s) | Depends on | Risk |
|----|-------|----------|------------|------|
| **M2-PR-00** | oce-diag: add `MissingRequiredParameter` + `ParameterOutOfRange` `DiagCode` variants (additive, `#[non_exhaustive]`), `as_str` arms, `Severity::Error` mapping — landed once so B + D don't race | oce-diag | none | low |
| **M2-PR-A0** | oce-blocks: per-tick `Context` (`t` + `&dyn Diagnostics`) into Block step methods + `read_int`/`int_param`/qualified-name enum-param accessor. `[CRITIC]` update **all** call sites incl. `init_values` pre-tick source firing; define sink ownership at the graph→api boundary | oce-blocks, oce-graph | none | medium |
| **M2-PR-A1** | oce-blocks: Reals algebraic core (Multiply, Divide, AddParameter, Abs, Min, Max, Line, +CDL.Constants). Landmines: Divide-by-zero IEEE (no panic), Line clamp outside `[x1,x2]` | oce-blocks | A0 | medium |
| **M2-PR-A2** | oce-blocks: Reals comparators stateful (Greater `h>0`, Less, GreaterThreshold, LessThreshold, Hysteresis). `[CRITIC]` `Greater` is `[A]`-only today → adds `h` field + param-dependent `kind()`/`state_len()` (novel) + **deliberate Greater golden re-bless, and any oce-graph schedule test using Greater** | oce-blocks | A1 | medium |
| **M2-PR-A3** | oce-blocks: Logical combinational (Or, Nand, Nor, Xor, Switch) + Conversions (BooleanToReal/Integer, IntegerToReal, RealToInteger round-half-up `−2.5→−2`) + Integers family + Logical.Sources.Constant | oce-blocks | A0 | medium |
| **M2-PR-A4** | oce-blocks: Logical timing/edge/latch `[S]` (FallingEdge, Change, Latch, Toggle, Timer, TrueDelay, TrueFalseHold, TrueHoldWithReset, Integers.OnCounter/Change). Densest feedthrough-classification surface. **MUST-HAVE** (economizer committed — was stretch) | oce-blocks | A3 | high |
| **M2-PR-A5** | oce-blocks: IntegratorWithReset (standalone, golden-tested **prerequisite for PID**); `y` CUT w.r.t. `u` | oce-blocks | A1 | medium |
| **M2-PR-A6** **(MERGED #36)** | oce-blocks: **PID + PIDWithReset** limited-controller core only (anti-windup, reverseActing, P/PI/PD/PID via `SimpleController`). **Mixed discretization (OD-PID):** forward-Euler integral term, **implicit/backward-Euler** first-order derivative filter `xD=(xD+(dt/T)·e)/(1+dt/T)`, `T=Td/Nd`. PID gets its own per-signal tolerance entry | oce-blocks, oce-model | A5 | high |
| **M2-PR-A6b** **(MERGED — #37, `eb6d783`)** | oce-blocks: **Derivative + MovingAverage + LimitSlewRate** + a shared first-order-decay helper **`crates/oce-blocks/src/dynamics.rs`** (`first_order_filter_implicit`, reused by PID's D term, Derivative, and LimitSlewRate's `(u−y)/Td` lag — all the same implicit-Euler `(input−state)/T` form). **MovingAverage** = Buildings `(μ−delay(μ,δ))/δ` over a **fixed-capacity `(t,μ)` checkpoint ring** (no hot-path alloc, R-IMPL-8; on overflow: graceful correct degrade — warn + drop-oldest + divide by actual retained span; **no CXF param**). See OD-MA | oce-blocks | A6 | high |
| **M2-PR-A6c** | oce-blocks: **Utilities.Assert** (the canonical Diagnostics-sink block, no signal output). Depends only on the A0 Ctx/Diagnostics seam → independent of A5/A6 | oce-blocks | A0 | low |
| **M2-PR-VEC** **(DEFERRED — off E1 path; Option iii)** | oce-blocks: `Block`-declared `PortShape{kind,width,dims?}` + per-instance `resolve_ports(params)` (build-time arity); width-`W` port → `W` row-major scalar `ConnectorId`s (reuses the flatten-to-scalar contract). **NO `Value::Vector` variant** (would violate R-IMPL-8). **Unblocks Routing + Multi\*.** Confirmed off the E1 critical path (research wf_cdfedf61-690). See OD-VEC | oce-blocks (+ oce-graph if needed) | A0 | high |
| **M2-PR-Routing** **(BLOCKED on VEC)** | oce-blocks: static `CDL.Routing` (15 vector-ported classes: `{Real,Integer,Boolean}×{ScalarReplicator,VectorReplicator,ExtractSignal,Extractor,VectorFilter}`) + the Multi\* family (`Reals.MultiSum/MultiMax/MultiMin`, `Logical.MultiAnd/MultiOr`, `Integers.MultiSum`, `MatrixGain/MatrixMax/MatrixMin`). `Value` is scalar-only today | oce-blocks, oce-model, oce-graph | VEC, A1/A3 | medium |
| **M2-PR-Aσ** **(CRITIC gap)** | oce-blocks: **Reals.Sources** beyond Constant (Ramp, Sine, Pulse, TimeTable) **iff** G1's generator + G36 fixtures need them. Resolve the G1↔A-lane source mismatch (`[CRITIC]` G1 enumerates Ramp/Sine/Pulse/TimeTable but no A-lane PR builds them) — either build here or strike from G1/D1 | oce-blocks | A0 | medium |
| **M2-PR-B1** | oce-conformance: funnel L1 band + CombiTimeTable CSV I/O (**THE RISKIEST PR**). Replace placeholder `ToleranceBand`/`ConformanceResult`/`unimplemented!()` `compare()` with 6-field `Tolerances`/`Series`/`FunnelResult`. `[CRITIC]` declare `oce-model` only with the CSV path in-PR (machete) | oce-conformance, oce-model | none | high |
| **M2-PR-B2** | oce-conformance: indicator masking + serde_json `VerifyConfig` + Tier 0–4 report enum + **Tier-0 shall/should** via `oce-diag` Severity (closes **#4**) | oce-conformance, oce-diag | B1, 00 | low |
| **M2-PR-B3** | oce-conformance: golden-trace driver bound THROUGH the frozen facade (`load_cxf` + `simulate`/`OutputTrace` fast path; driver-owned `step_realtime` tick loop for `EventAligned`). `[CRITIC]` canary checks **no DIRECT dep edge** (M2-AD-3); periods sourced from config (M2-AD-2) | oce-conformance, oce-api (ro) | B1, B2 | high |
| **M2-PR-B4** | oce-conformance: Tier-1 per-block goldens (one-block CXF micro-models via `load_cxf`) vs the **Tier-A closed-form oracle** + oracle.lock skeleton. `[CRITIC]` budget the ~60–80 single-block CXF fixtures as a real sub-deliverable (each needs declared external boundary I/O so it is schedulable). **Tier-B Buildings cross-check goldens = DEFERRED TAIL (no toolchain)** | oce-conformance, oce-blocks (fixtures) | B3, A6, G1 | medium |
| **M2-PR-G1** | oracle: closed-form generator (`tools/golden-gen`, off cargo-test) + Tier-A goldens + `prov.json` schema + oracle.lock skeleton. `[CRITIC]` either depend on A6 (covers PID/Integrator) or scope G1 explicitly to A1–A3 exact-math with a later A4–A6 pass — the `G1→A3` edge contradicts "ALL exact-math blocks" | tools/golden-gen, oce-conformance | B1, A3 | medium |
| **M2-PR-C1** | oce-graph: D6 Kahn-vs-reference-oracle topo cross-check (`tests/topo_oracle.rs`, structurally-different algorithm, re-implemented tie-break, no external crate, no runtime dep). Independent leaf — lands day 1. Closes **#6** D6 clause | oce-graph (tests/) | none | low |
| **M2-PR-D1** | oce-validate: block-param validation WI-6/F-002 (`check_block_params` reading raw `blk.params`, declarative `&'static [ParamRule]`, emits the two new DiagCodes). `[CRITIC]` D1 owns its negative fixtures (don't double-own with E1). Limiter inverted-range severity = **OD-D** | oce-validate, oce-blocks, oce-diag, oce-cxf | 00 | low |
| **M2-PR-E1** | G36 fixtures: **representative hand-authored** CXF for SAT-reset AHU **+ economizer AHU** + single-zone VAV; load + simulate end-to-end (**CLOSES #1**). The critical-path tail; gated by the LAST of {A4, A6b-merge} to land (A6c is a short feeder) | oce-cxf (fixtures), oce-api (tests) | A1, A2, A3, A4, A5, A6, A6b, A6c (+ Routing⇒VEC iff a sequence uses a vector splitter) | high |
| **M2-PR-E2** | G36 traces: Tier-2 **bit-exact determinism re-confirm (CLOSES #5)** + funnel L1 band vs the **closed-form/derivable** reference where available. **CLOSES #2's in-repo half**; the **Tier-B Buildings-reference comparison for the integrating loops/whole sequences is the DEFERRED tail** (pending toolchain) | oce-conformance, oce-api (tests) | E1, B3, B4, G1 | high |

---

## 5. Exit-criteria map

- **#1** ← E1 (representative hand-authored CXF, both AHU + VAV; enabled by all of Lane A + the M1 ingest path). **Fully closeable now.**
- **#2** ← E2. **SPLIT (toolchain deferred):** the in-repo half (funnel L1 band vs the Tier-A closed-form/derivable oracle, B1+B3+B4+G1) closes now; the **external-reference clause (vs Buildings/OM) for the integrating loops + whole G36 sequences is the DEFERRED tail** pending OD-toolchain. Per-signal tolerances are still recorded now.
- **#3** ← incrementally across A1–A6 (per-block), Assert path in A6; aggregate-verified by B4 Tier-1. **Fully closeable now.**
- **#4** ← B2, on the DiagCode/Severity substrate from PR-00. **Fully closeable now.**
- **#5** ← E2 (Tier-2 bit-exact whole-sequence) + per-block determinism goldens across A1–A6 (+ early A5/A6 canary, OD-det-canary). **Fully closeable now.**
- **#6** ← D6 clause by C1; store-agnostic clause is a standing invariant (existing default-no-db +
  cargo-deny gates, no new PR); facade-only binding guarded by B3's direct-dep-edge canary (M2-AD-3). **Fully closeable now.**

---

## 6. Critical path + riskiest PR

**Two co-equal long poles converged at E1/E2; the resource pole is now COMPLETE (merged 2026-06-20):**
- **Resource pole (Lane A):** A0(#31) → A1(#32) → A5(#35, Integrator) → A6(#36, PID) **— COMPLETE.**
  Remaining Lane-A tail to E1: **A6b** (Derivative/MovingAverage/LimitSlewRate — pushed, in review) and
  **A4** (timing/latch `[S]`, the outstanding economizer must-have).
- **Difficulty pole (Lane B):** B1(#30) **— MERGED (max soak achieved on the riskiest PR)** → B3 → E2.
- **Vector pole (new):** M2-PR-VEC → M2-PR-Routing — **RESOLVED off the E1 critical path** (research
  wf_cdfedf61-690: no committed `nZon=1` G36 sequence needs a vector splitter / Multi\*; every suspect
  degenerates to scalar at N=1). Deferrable additive track after E1/E2; not an E1 dependency.
- **Unblocked leaves, dispatch any time:** A4 (A3 merged), A6c (A0 merged), M2-PR-00 (no deps; gates
  B2 + D1), C1 (D6), D1 (after M2-PR-00), G1 (B1 + A3 merged).
- **Critical path to E1 now:** max(A6b-merge, A4) → E1 → E2.

**Riskiest = M2-PR-B1 (funnel band + CSV).** Load-bearing for #2 and #5; the monotone-envelope
construction at local extrema (corner selection + segment-intersection + interior-point dropping) is
the subtlest code in the crate (`07` risk #5); its worst failure is *undetectable from inside*.
Mandatory (not optional) mitigations: land earliest; unit-test `build_bounds` in **isolation** against
hand-computed extrema goldens; **MANDATORY** `funnel-xcheck` vs `lbl-srg/funnel` (M2-AD-7); ryu-class
shortest-round-trip f64 formatter with byte-exact CSV goldens (the Tier-2 bit-exact backstop).

---

## 7. OWNER DECISIONS — RESOLVED (2026-06-19) + LEAD DEFAULTS

**Resolved by the owner:**
- **OD-toolchain → DEFERRED ("revisit later"; no Modelica access, support uncertain).** Tier-B
  external-reference comparison (Buildings/OM) is a deferred tail; **Tier-A closed-form is the
  primary authoritative oracle for the whole M2 set** (M2-AD-6). Exit #2 splits: in-repo half closes
  now, external-Buildings half pending. `funnel-xcheck` (M2-AD-7) is in the same bucket — the
  `build_bounds` isolation goldens are the mandatory in-repo substitute meanwhile.
- **OD-#1-fixture → REPRESENTATIVE HAND-AUTHORED CXF** for exit #1 (using only the M2 block set);
  toolchain-derivation reserved for #2 reference traces only.
- **OD-scope → BOTH AHU sequences (SAT-reset + economizer) + single-zone VAV.** Economizer is
  committed → A2 (Hysteresis) + A4 (timing/latch) + the integer mode-logic/IntegerToReal chain are
  **must-have / critical-path**, not stretch. Owner still ratifies the exact `.mo` classes for the
  oracle.lock pin **when the toolchain question is revisited**.
- **OD-D / OD-severity-table → RESOLVED 2026-06-21, PER-CLASS SAFETY CASE.** **Shall-error:**
  `SampleTrigger.period > 0`, Discrete `period > 0` when such a class is present,
  `Reals.Limiter.uMin <= uMax`, `PID/PIDWithReset.Nd > 0`,
  `Derivative/LimitSlewRate/PID/PIDWithReset.Td > 0`, `MovingAverage.delta > 0`, and all
  required-parameter presence. **Should-warning:**
  `Reals.Limiter.uMin == uMax` (safe deterministic clamp-to-constant).

**Lead defaults (proceeding unless owner flags otherwise):**
- **OD-2 (Tier-A vs Tier-B lean).** Closed-form for ALL exact-math blocks; Tier-B (deferred) only
  ever for the ~6–8 integrating blocks + 2 sequences. Anti-tautology guard mandatory (generator
  derived from CDL spec text, not `oce-blocks`, different author).
- **OD-3 (Tier-A band tightness).** ~1-ULP rtol vs the closed-form oracle + separate Tier-2
  bit-exact-vs-self; `atoly=rtoly=0` only for Bool/Int outputs.
- **OD-PID (UPDATED 2026-06-20, as merged #36).** Discretization is **mixed**: the **integral term uses
  forward-Euler** (no decay term → unconditionally stable for any `dt≥0`, matches A5 IntegratorWithReset);
  the **first-order derivative filter uses implicit (backward) Euler** `xD(k+1)=(xD(k)+(dt/T)·e)/(1+dt/T)`,
  `T=Td/Nd`. *Rationale:* round 1 shipped **explicit** Euler for the D filter and it **diverges for
  `dt≥2T`**; since `T=Td/Nd` is intrinsically tiny (default `Nd=10`) that is essentially every realistic
  HVAC tick, so round 2 switched to implicit Euler (A-stable: `1/(1+dt/T)∈(0,1]`). **Implicit was chosen
  over exact ZOH** (`exp(−dt/T)`) deliberately: `exp()` is **not bit-reproducible across platforms/libm**
  and would break the Tier-2 bit-exact determinism goldens (M2-AD-5). First tick uses `dt=0`
  (PREV_T_UNSET sentinel); EventAligned pins the tick grid; PID gets its own per-signal tolerance entry.
  *Acknowledge: PID's float accumulator over a model-dependent tick union makes the Tier-2 golden fragile
  to unrelated model edits — isolate it.*
- **OD-MA (MovingAverage horizon-delay — RESOLVED by owner 2026-06-20).** Buildings
  `MovingAverage = (μ−delay(μ,δ))/δ`, `μ=∫u`, needs unbounded history on a variable tick grid, but the
  engine forbids a fixed internal step, arena `state_len` is fixed at init (`dt` unknown then), and
  R-IMPL-8 bars hot-path alloc. **Resolution:** a **fixed-capacity `(t,μ)` checkpoint ring**
  (`const MA_CAPACITY=64`, no engine/CXF param → CDL I/O unchanged). On **overflow** (>64 checkpoints
  needed to span `δ`, i.e. fine `dt` + long `δ`) the block applies a **GRACEFUL CORRECT DEGRADE**:
  Diagnostics **warns + drops the oldest** checkpoint AND **divides by the actual retained span
  `t − oldest_retained_t`** (not `δ`), yielding a **correct mean over the shorter available window**.
  *(This corrects round-1 A6b, which divided by `δ` while the numerator spanned `<δ` → a silent ~47%
  under-report; see the A6b review.)* Owner answers: **(a)** fixed-64 accepted; **(b)** overflow is a
  **should-warning** (graceful, not shall-error); **(c)** the degrade is a correct shorter-window mean,
  so the Tier-A/G1 oracle models it as a true mean over the retained span. **Must be bit-pinned by a
  non-constant post-startup overflow golden** (A6b round-2, landed bit-exact `1.3425…` over retained
  `[0.18,0.5]`).
  **Round-3 refinements (lead, from the A6b round-2 verification panel):**
  **(d) denom floor = division-safety guard, NOT a physical-horizon floor.** The windowed denom is floored at
  **`MIN_DELTA` (1e-5)** — the same constant `delta_eff()` floors the param at — *not* a literal `1e-3`. Round-2
  used `.max(1e-3)`, which for a **legal** `δ ∈ (1e-5, 1e-3)` (registry applies no `δ` clamp) under-scaled the
  non-overflow mean by `δ/1e-3` (≈2× low at `δ=5e-4`) — a silent wrong answer. Since every legal `δ ≥ MIN_DELTA`,
  flooring at `MIN_DELTA` makes the non-overflow denom exactly `δ` (correct) and only guards the overflow
  sub-case where `t−oldest_retained_t → 0`. Pinned by a non-overflow golden at `δ=5e-4`.
  **(e) overflow warning = once-per-instance.** A chronic capacity-exceedance must not emit a Diagnostics event
  *every tick* (log spam). A dedicated **`MA_WARNED` meta word** latches the first overflow warning →
  `MA_META_WORDS 5→6`, **`state_len` 133→134** (internal only; `oce-api` surface unaffected — oce-blocks isn't
  re-exported). Pinned by a ≥3-consecutive-overflow-tick golden asserting at-most-one warning + correct
  per-tick degrade.
- **OD-VEC (vector-port enabler — RESEARCH-RESOLVED 2026-06-20, wf_cdfedf61-690; owner ratification
  pending).** `Value` on `development` has only Real/Boolean/Integer/String/Enum — **NO vector variant**.
  **CDL.Routing** (all vector ports) + the **Multi\*** family stay unbuildable until M2-PR-VEC. Research
  (repo-spec-dispositive + authoritative Buildings `.mo`, HIGH confidence, both sequences):
  - **(1) VEC is OFF the E1 critical path.** The committed E1 fixture is a representative hand-authored
    **single-AHU / single-zone (`nZon=1`)** CXF (OD-scope). At `N=1` every vector suspect degenerates to
    scalar: zone-request aggregation (MultiMax/MultiSum) → pass-through or a fixed scalar Add/Max chain;
    setpoint/mode fan-out (`*ScalarReplicator`) → a single wire; trim-and-respond is intrinsically a
    **scalar** reset loop (Integer comparators + Reals.Add/Limiter + Discrete Sampler/UnitDelay);
    economizer/freeze/mode = scalar mode arithmetic. Buildings itself scalarized single-zone — `SupplyTemperature.mo`
    revision note: *"replaced multiAnd block with and3 block to avoid vector related implementation."*
    ⇒ **VEC → Routing deferrable, off the E1 path** (the schedule relief OD-VEC anticipated).
	  - **(2) Representation = OPTION iii (recommended, supersedes the `Value::Vector` framing): NO new `Value`
	    variant.** Add a Block-declared **`PortShape{kind, width, dims?}`** + a per-instance `resolve_ports(params)`
	    query resolved **once at BUILD** for param-dependent arity (ScalarReplicator `nout`, Multi `n`, matrix
	    dims); keep static `BlockSignature` for fixed arity. A width-`W` vector port expands to `W` consecutive
	    scalar `ConnectorId`s row-major — exactly the **flatten-to-scalar** contract `oce-flatten`/`oce-cxf`
	    already perform (`01` §3 inv 4). `Value`/tick/gather/`driver_of` unchanged. **Why not a `Value::Vector`
	    variant (options i/ii): R-IMPL-8 violation** — gather (`tick.rs` clones each input per tick) + emit would
	    heap-clone a `Vec`/slice **every tick**, breaking the zero-alloc tick.
	    **As-built 2026-06-27:** `oce-blocks` adds `PortShape` and `Block::resolved_signature()`; fixed-width
	    blocks borrow their static `BlockSignature`, while parameter-width blocks store resolved scalar
	    `PortKind` vectors on the immutable block instance. `oce-validate` checks the resolved signature.
	    The first consumers are `CDL.Logical.MultiAnd`, `CDL.Logical.MultiOr`, and `CDL.Integers.MultiSum`
	    (`nin`-dependent inputs, empty vector source behavior, `k_i` flattened gain elements). Native block
	    constructors clamp resolved vector width to a load-time cap before validation, so malformed hand-built
	    graphs cannot force unbounded allocation; the tick path remains scalar and allocation-free.
	  - **(3) Re-bless: MOOT under Option iii** (no `Value` variant). ⚠ **Gate-gap finding (independent of VEC):**
    the armed frozen-surface gate is **blind to re-exported foreign-type variants** — `Value` enters `oce-api`
    via `pub use oce_model::{…Value…}`, and `cargo public-api` prints it as one line without expanding
    variants, so a `Value::Vector` add would pass the gate **silently**. Don't trust the gate for `Value`-shape
    changes.
  - **(4)** `Value` **and** `ValueType` are **NOT `#[non_exhaustive]`** (`oce-model` lib.rs) → any future
    variant is breaking. Recommend marking both `#[non_exhaustive]` **before 1.0** (cross-crate-only: internal
    workspace exhaustiveness preserved; external consumers get additive evolution) per the embeddability mandate.
  - **Owner decisions (2026-06-20):** (a) E1 `nZon=1` single-AHU/single-zone scope **stands** (existing
    OD-scope; the mechanism keeping VEC off the critical path). (b) The VEC **representation decision is
    DEFERRED** — not locked now; Option iii remains the standing lead recommendation, to be ratified when
    M2-PR-VEC is scheduled (after E1/E2). (c) `#[non_exhaustive]` on `Value`/`ValueType` **APPROVED** as a
    small dedicated pre-1.0 PR (see work item).
- **OD-Dscope (Lane D inclusion).** Include D1 in M2 **and** the same-epic tune-at-rest activation
  (populate `ParamAttrs.min/max`, fire the inert `OcError::ParamRange`) — closes both halves of
  F-002 + private todo `019ede28-aa9b`. New shall-errors are a behavior change for models shipping
  `period=0` today → changelog + golden.
- **OD-A (facade param read).** Source Discrete periods from the verify config — **no `oce-api`
  re-bless.**
- **OD-B (Assert via `simulate()`).** Use the already-frozen `step_realtime`/`StepReport.asserts`
  path; defer `simulate()` assert-collection (would re-bless `SimMetrics`) unless a fixture needs it.
- **OD-det-canary.** Ship a cheap run-twice-byte-compare determinism gate **in A5/A6 themselves**,
  not only at the terminal E2.

---

## 8. Critic findings ledger

**Resolved in this plan:** facade-param-read gap (M2-AD-2 + OD-A); impossible cargo-tree canary
(M2-AD-3); cargo-machete declared-but-unused (M2-AD-10); A0 call-site ripple incl. init pre-tick
(M2-AD-8); param-dependent kind/state_len novelty + Greater re-bless blast radius (M2-AD-11, PR-A2);
funnel xcheck promoted mandatory (M2-AD-7); missing Reals.Sources PR (PR-Aσ); G1↔A-lane scope
mismatch (PR-G1 note); D1/E1 double-owned negative fixtures (PR-D1 note); single-block CXF fixtures
as a real sub-deliverable (PR-B4 note).

**Resolved by owner (§7):** OD-toolchain (deferred → Tier-A primary, exit-#2 external half is the
tail); OD-#1-fixture (representative hand-authored); OD-scope (both AHU + single-zone VAV → economizer
committed); OD-D/severity-table (per-class safety case).

**Lead defaults adopted (§7), revisit only if owner flags:** OD-2/OD-3 (Tier-A lean + band tightness),
OD-PID (mixed forward/implicit-Euler + isolate Tier-2 fragility), OD-MA (MovingAverage fixed-capacity ring), OD-VEC (vector-port enabler), OD-Dscope (include D1 + tune-at-rest), OD-A/OD-B
(no re-bless), OD-det-canary (early A5/A6 determinism gate).

**Revisit-later (toolchain-gated, not blocking the unblocked work):** Tier-B Buildings/OM cross-check
goldens (B4 tail), the `funnel-xcheck` external oracle (M2-AD-7), exit-#2's external-reference clause
(E2 tail), and the exact `.mo` class pins for oracle.lock.

---

## 9. M2-PR-A0 as-designed (workflow `wf_2ff711d1`, critic verdict APPROVE — source-verified)

A0 threads what M1 already reserved — it invents almost nothing. The `Diagnostics` trait already
exists (`oce-blocks/src/lib.rs:74-76`); `StepReport.asserts: Vec<AssertEvent>`, `AssertEvent`, and
`AssertLevel` (Default = Error) are **already on the frozen surface** (`oce-api/src/sim.rs:247-279`,
left populated-empty in M1 "for exactly this"). **Net result: ZERO `oce-api` re-bless.**

**Trait change (`Ctx`-first, uniform across all three compute methods — locked):**
```rust
// oce-blocks/src/lib.rs, after :76 (private fields; t()/warn() are the block-facing API)
pub struct Ctx<'a> { t: Time, diag: &'a dyn Diagnostics }
pub struct NoopDiagnostics;            // zero-size; impl Diagnostics { fn warn(..) {} }
// step_algebraic/emit_from_state/update_state: `_t: Time` → `_ctx: &Ctx` (Ctx first after &self)
```
Ripples to all 13 impls (reals.rs ×7, logical.rs And/Not/Pre/Edge/SampleTrigger, discrete.rs
UnitDelay). The only body-text change in the mechanical commit is SampleTrigger
`self.sample_index(t)` → `self.sample_index(ctx.t())` (`logical.rs:256/260`, identical f64).

**Sink ownership + init + Assert delivery:**
- Sink owned by the `oce-api` Engine layer, borrowed downstream; add `diagnostics: &'a dyn
  oce_blocks::Diagnostics` to `EvalContext` (`tick.rs:20-29`). Split `Engine::tick` → public `tick()`
  (injects `&NoopDiagnostics`, signature unchanged) + private `tick_with(t, diag)`.
- **Init-time fire** (`tick.rs:88` pre-tick source firing): pass a concrete `&oce_blocks::NoopDiagnostics`
  literal — **no `Option`, no smear, no BUILD-signature change**. (Only no-input `[A]` source today is
  Constant, which never warns → deterministic-drop contract holds.)
- **Assert delivery:** private `AssertCollector { events: RefCell<Vec<AssertEvent>> }` (RefCell because
  `Diagnostics::warn` takes `&self`); `step_realtime` (`sim.rs:358`) stack-allocates it and replaces
  `asserts: Vec::new()` (`sim.rs:366`) with `collector.events.into_inner()`. **`simulate()` stays
  asserts-free** via the public `tick()` → `NoopDiagnostics` path; `SimMetrics` gains no field (OD-B).

**Accessors (additive, off the frozen surface):** `read_int(inputs,i)->i64` (pub(crate)); `int_param(...)
->i64` (no Int→Real arm); **new `pub enum oce_model::SimpleController { P, Pi, Pd, Pid }` +
`from_qualified(&str)->Option<_>`** (lives in `oce-model` per `_spec/02 §2.2` / `_spec/03 §4.9`; oce-model
has no public-api gate → free); `controller_type_param` handles both grounded `Value::Enum{ordinal: u32}`
(1-based) and a defensive `Value::String` → `from_qualified` fallback.

**Ripple discipline — ONE PR, TWO commits:** commit 1 = mechanical `Ctx` sweep (behavior-preserving;
whole suite stays byte-green; only harness-plumbing edits); commit 2 = behavioral (AssertCollector +
accessors + all new tests).

**Critic CI-real checklist (fold into the PR or clippy/nextest fails):** (a) add `Ctx, NoopDiagnostics`
to `oce-blocks/src/tests.rs` `use super::{...}`; (b) per block module, drop the now-unused `Time`
import and add `Ctx` where the module no longer names `Time` (reals.rs, And/Not, Pre/Edge, UnitDelay;
SampleTrigger keeps `Time`) — the clippy gate rejects unused imports; (c) match `Value::Enum.ordinal`
as `u32` (not i64); (d) capture `let diag = ctx.diagnostics;` **before** the `&mut *ctx.state` RunState
destructure (`tick.rs:116-123`) to avoid a borrow conflict.

**Deferred to A2/A6 (not A0):** Assert instance-path source attribution + per-assert level (both
`oce-blocks`-side, never `oce-api`). A0 lands the seam with Error-default level.
