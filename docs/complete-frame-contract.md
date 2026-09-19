# Complete generation-atomic input and output frames

## Status and authority

This is the normative detail of PC-031 in [product contract revision 7](product-contract.md).
**Preparation (PC-032) and native complete-frame execution (PC-033) are implemented.** PC-034/035
remain future convenience migration/classification outcomes. The additive APIs below supply no
profile selector, wire format or stable-product guarantee. Execution maintainers own
these semantics; host policy remains with the host integrator.

The requirements below describe the native complete-frame path, not a reinterpretation of
`set_input`, `tick`, `simulate` or `step_realtime`. “Atomic” means an engine-owned transition under
the ordinary returned-refusal boundary below, not a distributed, persistent or actuator transaction.
The fixed [HostTick v1 profile](execution-profile.md#hosttick-v1) remains unchanged.

## Determinants and executable boundary

A transition is determined by the loaded executable (including effective parameters, schedule and
value domains), compatible entry execution state, the fixed execution profile, model time, and the
complete input values. Completeness closes the external-input determinant set; it does not make a
stateful transition independent of its prior state or imply unrestricted cross-build determinism.

- Every executable boundary input MUST be supplied exactly once unless the executable schema
  explicitly declares an engine-level default or optionality with a deterministic omission meaning.
  An absent declaration means required. An optional input, when supplied, is still subject to
  uniqueness and type validation. An executable with no boundary inputs admits an empty value set
  only after the other acceptance checks pass.
- Completeness is over canonical executable input identities, not every row marked input in today's
  point inventory, and not the union of all internal connector slots. One boundary identity may fan
  out to several consumers; the host supplies it once and the engine resolves all its targets.
  Internal driven connectors and read-only output aliases are not extra host determinants.
- Values MUST match the executable schema's types and declared domains, including enum identity
  and legal enum values where applicable, without lossy coercion through a Store carrier. Point
  metadata is not that schema: current `IoInventory` includes internal points, projects enums to
  `Int`, and omits strings. It cannot by itself establish the required executable input set.
- Type seeds, previously staged values, held Store samples, absent quality metadata and host
  substitutions MUST NOT become implicit defaults. Engine-level optionality is executable schema
  semantics, not permission for OCE to invent sensor quality, freshness or fallback policy. This
  contract adds no blanket finite-Real or plausibility rule for signal values; finite **time** is
  a separate requirement.
- Native preparation and transition MUST use only the supplied values and engine execution
  context, with no Store reads, writes or host callbacks that add hidden input determinants.
  A host or convenience adapter can obtain observations before submission; it owns their coherence
  and quality. “Complete” does not prove simultaneous sampling or physical validity.

## Compatibility and generation roles

These are semantic roles, not proposed public field or type names:

| Role | Meaning and limit |
| --- | --- |
| Loaded-executable context | The engine-local successful load incarnation and the executable it installed. Successful reload invalidates prior prepared frames and resolved references, even when reloading identical bytes or reusing the same point names. |
| Executable IO compatibility | Canonical boundary identities, direction, types/domains, fan-out and explicit omission semantics used to interpret inputs and outputs. Equality of authored model names or point counts is insufficient. |
| Accepted-transition correlation | An unambiguous association among an accepted input, its one transition, and its completed output/diagnostics within the compatible run. Equal timestamps identify different transitions; refused submissions consume no accepted-transition position. |
| Replay compatibility and position | The deterministic execution context and accepted transition ordering needed for reproduction or explicit refusal. This is not yet a canonical record encoding or a promise that an ephemeral load token is portable. |

A frame or resolved reference MUST be checked against the current loaded-executable and IO context
before mutation; a stale context refuses rather than silently rebinding names. Preparation alone
does not grant permission to commit after reload or after the time guard has advanced beyond the
submitted time. Ordinary failed load retains the prior engine-local context under the existing
in-memory replacement guarantee; external Store effects still have their separate compensation
boundary. Compatibility-changing reconfiguration cannot silently reuse old preparation.

This fence is **not** a deployment generation, lease, authentication token, sensor freshness test,
command authorization or restored actuator ownership. A host deployment identifier cannot replace
the engine check, and a passing engine check cannot replace host admission. Authored/source model,
exported-document and catalog identities retain their distinct roles. Current snapshot executable
compatibility is not proof that a reference belongs to the current successful load incarnation.

Preparation supplies the concrete reference representation and typed refusal surface;
native execution supplies accepted-result correlation. M03 owns typed identity/compatibility layers and canonical
state/replay representation, including continuation/rewind correlation. Future representation choices
cannot weaken these reload and correlation semantics. No snapshot fields, catalog
identities or state revisions change here, and hosts are not asked to parse private snapshot bytes
or build another replay format to fill this gap.

## Prevalidation and refusal matrix

All ordinary refusal conditions MUST be validated before any execution/replay mutation. The whole
candidate is resolved and checked before any input prefix is staged. There is no evaluation to
discover an ordinary input error. Typed error names and deterministic precedence for preparation
are specified below; execution retains this prevalidation boundary.

| Condition | Required outcome before mutation |
| --- | --- |
| No successfully loaded executable | Refuse; an empty engine is not an executable with zero inputs. |
| Pending parameter edits | Refuse until resume; halted alone does not prohibit execution. |
| Unknown or non-input identity | Refuse; do not ignore extra values or treat output aliases/internal driven points as boundary inputs. |
| Duplicate boundary input | Refuse even if both values are bit-identical; do not use first-wins or last-wins. Fan-out is not a duplicate. |
| Missing required input | Refuse; no hold-last, zero/false seed or Store fill-in. Apply only explicit executable omission semantics. |
| Wrong type or declared domain | Refuse the complete candidate, including any otherwise valid prefix. |
| NaN or either infinite model time | Refuse. |
| Finite time below the preceding accepted model time | Refuse. |
| Time outside loaded-block representability | Refuse under the existing model-time limits, not by executing first. |
| Stale loaded-executable or IO compatibility context | Refuse frames and references from a superseded successful load; no implicit rebinding. |
| Accepted-frame sequence exhausted | Refuse before staging; never wrap or reset the sequence. |
| All checks pass, including equal finite time | Accept exactly one HostTick v1 transition. |

For **every ordinary refused frame**, the entire observable engine execution/replay image MUST
remain unchanged: model time and monotonic guard; state words; connector image (including staged
inputs); visible outputs and output generation; completed-frame diagnostics; replay identity and
accepted-transition position. Refusal also preserves existing mutation/readiness boundaries, rather
than closing a fresh durable-restore window by staging a prefix. The refusal report describes the
attempt; it does not replace the last completed outcome or masquerade as a completed diagnostic frame.

This guarantee excludes panic, process death, allocation failure, cancellation and concurrency
outside current guarantees. It is not persistence, Store rollback, delivery acknowledgment or an
actuator guarantee. Native Store noninterference does not retroactively undo earlier host/adapter
operations. No scheduler, deadline guarantee or universal panic freedom is introduced.

## Accepted transition and immutable outcome

One successful complete frame MUST perform exactly one HostTick v1 transition: stage the validated
values, emit the frozen schedule once from entry state/current inputs, then update stateful blocks
once and expose the completed boundary outputs. Model time is finite nondecreasing seconds. An equal
finite timestamp is valid and advances again; it is neither a read nor an idempotent retry. There is
no hidden event iteration, convergence test or repeated evaluation to reach a fixed point.

The accepted output frame MUST be immutable and bound to that accepted input/transition and its
loaded-executable/IO compatibility context. It contains all completed executable boundary outputs
with their identities and types, plus the execution diagnostics belonging to that transition.
Later execution or reload cannot change a previously retained outcome. It is not merely an alias
to a mutable latest-output view, a subset of convenient point rows, or a Store write receipt.

Output and diagnostic ordering and compatibility/sequence semantics MUST support deterministic host
correlation, including two successes at the same time and an intervening refused attempt. Runtime
assertion diagnostics follow the existing **Warning-only** law: warnings do not reject or roll back
a transition, escalate to an Error level, or implement an interlock. Collection covers execution
diagnostics, not load/export receipt history, host quality assessments or persistence/write errors.
Diagnostic source semantics are preserved; no new instance-identity promise is inferred. Wall-clock
latency measurements are not deterministic replay identity.

Producing or retaining an output frame means computation completed, not that an adapter persisted it
or equipment received it. A future convenience write failure after commit is a delivery failure,
not an ordinary refused native frame. It cannot retroactively roll back or relabel the transition.

## Legacy paths and migration

The following are **current weaker/convenience paths**, not implementations of this contract:

| Path | Current acceptance and failure boundary |
| --- | --- |
| `set_input` followed by `tick` | Sparse named typed writes mutate immediately. Repeated writes are last-wins; omitted inputs retain entry values. A later setter or tick refusal does not undo earlier successful setters. |
| Store-backed `tick` | Available samples overwrite bound slots regardless of status/timestamp; missing samples hold the connector value. Snapshot acquisition failure stages no sample. A later type refusal can leave a valid prefix staged and close durable-restore readiness, while no block evaluates and model time/outputs do not advance. |
| `simulate` | Collection, constants and the first closure list preflight before restart, but these lists are not completeness/duplicate validation. Restart re-seeds words and clears the prior time guard, retaining connector values. Later closure refusal keeps earlier ticks and valid prefix staging. First-tick Store refusal can leave the restart applied before any evaluation. |
| `step_realtime` | Host epoch mapping validates first; then the tick applies before Store write-back. Write failure leaves the transition applied and can prevent collected assertions from being returned. Written count is an adapter receipt, not durability or actuation. |

Current `tick` and simulation use a no-op execution diagnostic sink; realtime collects Warning
reports. Existing snapshots/checkpoints, output views and facade metadata remain valid within their
own documented limits; they are not completed frames.

The native preparation/execution APIs are additive. M02-PR04/05 address shared convenience
semantics and legacy classification/guards. This document does not remove, retrofit or strengthen the
current APIs. Future migration requires explicit acceptance evidence and compatibility accounting
for any changed sparse, hold-last, duplicate, restart or post-tick-write behavior. Whole-horizon
rollback and Store transactionality are not implied by reusing a native transition core.

## Host and downstream boundaries

Quality, freshness, plausibility, missing-data reaction, NO_EVAL, safe-state selection, scheduling,
wall-clock mapping, persistence, authentication/authorization, deployment fencing, equipment
interlocks and actuation remain [host responsibilities](host-responsibilities.md). NO_EVAL means
not executing, not submitting a fabricated zero frame or calling `halt` as an equipment stop.

Library keeps artifact verification; Studio keeps authoring/translation and simulation integration;
Edge keeps deployment, observation admission, NO_EVAL and command boundaries; Sim keeps closed-loop
simulation and replay integration. Runtime is only an **additive future M05-PR03 consumer/host
qualification candidate**. BOPTEST results would be Runtime host evidence, not OCE equivalence or
reassignment of Sim. No downstream qualification is claimed here.

Tokio, Axum, SQL, HTTP/MCP, drivers, quality/staleness policy, leases, commands and fallback services
stay outside OCE. Consumers use the engine-owned evaluator, snapshot and eventual canonical replay
contracts; this work does not create a second evaluator, snapshot or replay stack.

## Evidence and remaining acceptance work

The passing [legacy boundary tests](../crates/oce-api/tests/legacy_frame_boundary.rs) deliberately
characterize the gaps, not a future API. A hand-authored two-input Add fixture and bit-exact output
golden expose omission/last-wins behavior. Typed refusals, before/after snapshot bytes, continuation
outputs and fresh-restore refusal distinguish unchanged execution from retained input mutation.
Independent arithmetic expectations and repeat runs guard against merely blessing engine output.
These stateless cases do not prove preservation of stateful words or any future frame identity.

Existing [Store tests](../crates/oce-api/src/tests/store_backed_inputs.rs),
[simulation staging tests](../crates/oce-api/src/tests/input_staging_tests.rs), and
[Pre profile tests](../crates/oce-api/src/tests/pre_execution_profile_tests.rs) provide complementary
current-behavior evidence. The product requirement table links the existing restart, Warning and
post-tick write-failure tests. The traceability checker checks links/statuses, not semantic compliance.

Preparation evidence below fulfills PC-032; the execution evidence below fulfills PC-033 with
equal-time correlation, retained immutable results and unchanged images on ordinary refusal.
Explicit omission semantics would require a future executable
schema change; none is invented here. The existing HostTick conformance limits and later
cross-platform/replay qualification still apply.

## Current preparation API

`Engine::input_definitions()` returns an owned `Vec<InputDefinition>` in lexical UTF-8 canonical
path order. Each row has `path`, exact native `value_type`, and inclusive `min`/`max` as optional
native `Value`s. This is the represented executable boundary (`external_inputs`), not the point
inventory or discarded source declarations with no executable consumer. Every row is required;
the current schema has no optionality/default declaration. Fan-out yields one row, with the
intersection of the boundary declaration's and all targets' bounds. Empty intersections accept no
value. Real zero-bound ties have deterministic bits; a NaN bound never disappears in intersection.
Integer bounds remain exact i64 values; absent Integer bounds use the documented i32 defaults,
while explicit bounds are retained. Enum bounds carry the class and legal ordinal endpoints.
No Store carrier, handle or public connector index is involved.

Keys are the expanded identities retained by ingest. The current resolver admits **no input
aliases**: compact names are expanded at ingest, not at submission, and elided child names are not
alternate setters. Output aliases are read-only. An injected-resolver-alias control checks that two
spellings mapped to one logical input cannot defeat duplicate detection. It does not establish a
public alias namespace. String/enum schema handling is total, but no current registry block offers
those signal ports; private detached probes do not claim broader CXF support.

`Engine::prepare_frame(time, &[(&str, Value)])` returns `PreparedInputFrame`. The plan owns exact
values and every resolved target; keys are borrowed only during the call. It is opaque and has no
serialization or public constructor. `Engine::execute_frame` consumes it. Editing a copied definition
does not alter validation. Neither preparation success nor preparation refusal stages a value,
evaluates, calls Store, replaces diagnostics,
changes watches/outputs/time, or closes durable-restore readiness. There is no new replay image.

First-cause refusal precedence is:

1. `State(NoLoadedModel)`, then `State(PendingParameterEdits)`.
2. `NonFiniteTime`, `TimeRegression`, then `ModelTimeUnrepresentable`, in that order.
3. The lexically first invalid submitted key: `FrameUnknownInput` or `FrameNotInput` (outputs and
   internally driven input points). These errors keep at most 64 UTF-8 bytes, cut at a character
   boundary, and the original key byte count. They do not clone an arbitrary-size submitted key.
4. `FrameDuplicateInput`, then `FrameMissingInput`, each naming the lowest canonical input path.
5. The first canonical input with a bad value: `InputType` before `InputDomain` for that input.

This precedence is independent of entry order. Refusals do not accumulate a report. Unbounded Real
values retain NaN payloads/infinities; a declared comparison bound has to hold, so any bound rejects
NaN and finite bounds reject the respective infinity. No blanket finite-signal or equipment policy
is introduced. Finite equal time is valid.

The internal `check_prepared_frame` seam checks readiness, incarnation, and current time eligibility
before execution can use resolved targets. It returns `StalePreparedFrame` for a
different engine or superseded load/rebuild, without rebinding names. Successful identical-byte
reload also invalidates. Failed load preserves the previous incarnation. Dirty resume fences before
effective model mutation, even for same-value edits; clean resume and compatible checkpoint/durable
restore alone retain the incarnation. Advancing the clock can still make a retained plan's time
ineligible. The fence is a retained process-local allocation identity, not a serialized counter,
model name, Store handle, deployment generation or authentication token.

### Preparation costs and evidence limits

For N logical inputs and T total fan-out targets, successful preparation allocates N+2 buffers for
nonempty N: one temporary N-slot reference array, one N-entry plan, and one target array per input.
Values clone without copying String bytes. Retained capacity is N plan entries plus T targets;
zero-input preparation allocates no buffers. Input lookup scans submitted key bytes; value checks
and target copies are proportional to N+T. The load-time schema sorts canonical keys once.
The cooling-only-controller census repeats preparation 128 times, checks exact allocation/byte
formulas and capacities, requires no retained allocation after drop, and has a counter positive
control. This is a synchronous allocation census, not a latency, throughput or general peak-memory
claim. Definition snapshots separately allocate owned metadata.

The [public refusal matrix](../crates/oce-api/tests/prepare_frame_preservation.rs) compares fresh and
advanced stateful snapshots, output/watch values, restore readiness and Store call counts. The
[public ordering tests](../crates/oce-api/tests/prepare_frame.rs) and
[private plan/lifecycle/domain/census tests](../crates/oce-api/src/frame_tests.rs) pin independently
authored expected values and checked-in bit/diagnostic goldens. The nonserialization compile-fail
example and exact facade baseline cover opacity. Removing duplicate detection, dropping a fan-out
tail, coercing integers via f64, changing signed-zero bits or omitting invalidation is detected by
the corresponding assertions. No external Modelica oracle exists for this OCE-specific policy.

## Current execution API

`Engine::execute_frame(PreparedInputFrame) -> Result<CompletedFrame, OcError>` consumes one plan
by value, even on refusal. The plan cannot be cloned, serialized or submitted twice. Preflight
precedence is unloaded, pending edits, stale incarnation, nonfinite time, regression, block-time
representability, then `FrameSequenceExhausted`. Preparation has already resolved and validated
every input and fan-out target. Execution rechecks the mutable conditions, without name rebinding.

After preflight, the implementation stages the entire plan, closes durable restore, calls the
existing infallible evaluator once with the Warning collector, updates `prev_t` and latest `Outputs`
once, increments the accepted-frame sequence and captures the result. All ordinary returned errors
are before this boundary. No Store operation, host callback, shadow RunState, undo log or rollback
is involved. Allocation failure remains excluded, including during result capture.

`CompletedFrame` is owned, Clone + Debug + Send + Sync, with private fields and read-only
`time()`, `sequence()`, `outputs()` and `diagnostics()` accessors. Outputs are `(String, Value)`
pairs; cloning a result gives an independent owned result. Real bits are copied, not normalized by
capture (individual block arithmetic retains its existing numerical policy). No public connector
indices, Store handles, mutable latest-view, schedule or serialized identity are returned.

The output set is the same disjoint union as `Topology.boundary_outputs`: represented elided
root declarations plus lowered pass-through outputs. It is sorted by canonical lexical UTF-8
identity, not source order. Distinct declarations sharing a driver stay distinct; internal driver
paths and the pass-through listing are not appended as duplicate aliases. Undriven source-only
declarations are absent under the existing ingest warning contract. Zero boundary outputs is valid,
including an Assert-only boundary with internal outputs. Inventory, trace and durable columns are
not the authority for this set.

Sequence starts at one and advances only on a successful native complete-frame commit. It never
decreases or resets in one Engine lifetime, including successful reload, dirty/clean resume,
simulation restart, checkpoint rewind and durable restore. Legacy tick/sim/realtime and refusals
consume no position. Thus equal-time commits remain distinct. The private retained `Arc<()>`
incarnation binds each result to its loaded executable/IO and fixed build/profile context without
exposing pointer identity. Sequence is correlation only: not replay position, snapshot generation,
durability, cross-process identity, deployment authority, lease, authentication or freshness.
Neither it nor the result is serialized into existing state/checkpoint bytes. No last-result cache
is added to Engine; the caller owns retained outcomes, which remain unchanged by later operations.

### Execution evidence and cost boundary

- [Public success goldens](../crates/oce-api/tests/execute_frame.rs): hand-derived Add, sampled
  delay and equal-time Pre feedback; fan-out, native pass-through bits, lexical/many-to-one
  boundaries and ordered Warning diagnostics. These OCE-specific scenarios are not a Modelica
  event-iteration oracle.
- [Public preservation matrix](../crates/oce-api/tests/execute_frame_preservation.rs) and
  [private preflight controls](../crates/oce-api/src/frame_commit_tests.rs): fresh/advanced
  connector/word/time/output/scratch images, snapshot and checkpoint bytes, watches, restore
  readiness, sequence and Store counts. Private injection exercises nonfinite/unrepresentable
  time and sequence exhaustion; opaque public plans cannot be forged. The instrumented block
  boundary detects even idempotent double updates. No postcommit ordinary failure is invented.
- [G36 controller](../crates/oce-api/tests/g36_cooling_only_controller.rs): two complete 1,441-row,
  ten-boundary-output runs against the existing independent Tier-A HostTick reference, not
  engine self-output or unrestricted Modelica equivalence.
- [Allocation and latency harness](../crates/oce-api/tests/frame_observations.rs): exact counters
  and positive control, 128 repetitions each of five fixtures. For B nonempty boundary outputs,
  result capture allocates B path buffers plus one pair vector; empty B allocates none. Warnings
  add their owned source/message buffers and a geometrically growing event vector. Staging clones
  only the prepared fan-out values; preparation's separate N/T formula remains above. Existing
  evaluator allocation exceptions remain, and latest Outputs still refreshes exactly as on tick.
  No whole-engine/state copy is charged as result capture. See [measured observations](benchmarks.md#complete-frame-observations)
  for debug/release timing scope; no universal speed or latency-ratio guarantee follows.

Compile-fail rustdoc pins nonserialization and single-use preparation. Exact public baselines and
shape guards cover the additive API. Hosted architecture qualification and actual downstream host
adoption remain separate; PC-034/035, stable release, persistence and equipment claims are not promoted.
