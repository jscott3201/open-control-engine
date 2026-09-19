# Complete generation-atomic input and output frames

## Status and authority

This is the normative detail of PC-031 in [product contract revision 5](product-contract.md).
**The contract is ratified; the complete-frame runtime API is not implemented.** Acceptance of
PC-031 is contract-and-evidence delivery only. PC-032 through PC-035 remain future implementation
and migration outcomes. No new public types, methods, selectors, wire formats or stable-product
claims follow from this document. Execution maintainers own these semantics; host policy remains
with the host integrator.

The requirements below describe the future native complete-frame path, not a reinterpretation of
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

M02-PR02 owns the concrete preparation/reference representation and typed refusal surface;
M02-PR03 owns accepted-result correlation. M03 owns typed identity/compatibility layers and canonical
state/replay representation, including continuation/rewind correlation. Representation choices
remain open, but cannot weaken these reload and correlation semantics. No snapshot fields, catalog
identities or state revisions change here, and hosts are not asked to parse private snapshot bytes
or build another replay format to fill this gap.

## Prevalidation and refusal matrix

All ordinary refusal conditions MUST be validated before any execution/replay mutation. The whole
candidate is resolved and checked before any input prefix is staged. There is no evaluation to
discover an ordinary input error. Typed error names and deterministic precedence when several
conditions fail are for M02-PR02 to specify and test; no concrete enum variants are prescribed here.

| Condition | Required outcome before mutation |
| --- | --- |
| No successfully loaded executable | Refuse; an empty engine is not an executable with zero inputs. |
| Unknown or non-input identity | Refuse; do not ignore extra values or treat output aliases/internal driven points as boundary inputs. |
| Duplicate boundary input | Refuse even if both values are bit-identical; do not use first-wins or last-wins. Fan-out is not a duplicate. |
| Missing required input | Refuse; no hold-last, zero/false seed or Store fill-in. Apply only explicit executable omission semantics. |
| Wrong type or declared domain | Refuse the complete candidate, including any otherwise valid prefix. |
| NaN or either infinite model time | Refuse. |
| Finite time below the preceding accepted model time | Refuse. |
| Time outside loaded-block representability | Refuse under the existing model-time limits, not by executing first. |
| Stale loaded-executable or IO compatibility context | Refuse frames and references from a superseded successful load; no implicit rebinding. |
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
own documented limits, but none proves availability of complete native frames.

M02-PR02/03 implement preparation and transition; M02-PR04/05 separately address shared convenience
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

Future implementation acceptance needs the complete refusal matrix on fresh and advanced stateful
runs, fan-out/domain and explicit-omission cases, Store noninterference, same-byte and changed-IO reload
rejection, equal-time sequence correlation, retained immutable outputs/diagnostics, and unchanged
execution/replay image on every ordinary refusal. This remains PC-032/033 work, not passing evidence
supplied by this slice. No external Modelica oracle exists for this OCE-specific frame boundary;
the existing HostTick conformance limits and later cross-platform/replay qualification still apply.
