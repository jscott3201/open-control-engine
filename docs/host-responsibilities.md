# Host responsibilities

For anyone about to wire this engine to real equipment. It answers one question: what safety
behavior must **you** implement, because the engine deliberately does not?

This engine executes a control sequence. It does not supervise the equipment that sequence drives,
and it does not judge the quality of the data it is fed. Those are your job, and the engine will not
warn you if you skip them.

See the [product contract](product-contract.md) for numbered host obligations and the bounded
requirement-to-evidence map; engine boundary tests are not host-compliance evidence.

## Native complete frames and the host delivery boundary

The [complete-frame contract](complete-frame-contract.md) supplies current typed, read-only
preparation and an engine-local load/rebuild fence (PC-032). `execute_frame` consumes a prepared
submission and returns one immutable `CompletedFrame` after one atomic HostTick transition (PC-033).
PC-034 retains the shared private evaluation core. PC-035 removes the weaker legacy profiles;
preparation followed by consuming execution is the only public state-advancing execution surface.

Completeness means every executable boundary input exactly once, except omission explicitly
defined by the executable schema. It does not establish sensor coherence, quality, freshness or
plausibility. Preparation neither fills gaps from Store samples nor infers host defaults.
Its reload fence does not authorize deployments or commands. Persistence, authentication,
authorization, deployment fencing, scheduling/wall-clock mapping, NO_EVAL, safe states, equipment
interlocks and actuation stay host-owned. NO_EVAL means not executing.

Native execution calls no Store method or host callback. Persisting or delivering a retained result
is a separate host operation; a later delivery failure does not turn the accepted frame into a refusal.
Do not blindly resubmit at the same time: that produces another state transition and sequence.
The accepted-frame sequence correlates only within one Engine lifetime; reload, resume and restore
do not reset it. It is deliberately absent from snapshot bytes and supplies no durable replay position,
lease, authentication or equipment authority. Retained results remain unchanged across those operations.
The result owns Warning-only diagnostics; it adds no escalation, interlock or safe-state decision.

Execution has no write-back or post-write error path. Hosts retain the completed receipt and own
any subsequent persistence or actuation attempt. Do not repeat execution merely to retry delivery:
even equal model time advances state again. `get_output` and `watch` inspect latest state, not a
retained receipt; load, restore and resume can replace that state without executing a frame.

Library, Studio, Edge and Sim retain their roles; Runtime is only an additive future M05-PR03
consumer/host qualification candidate. BOPTEST is Runtime host evidence, not OCE equivalence.
No host services or second evaluator/snapshot/replay stack move into OCE.

## Canonical per-frame replay

`CompletedFrame::replay_record()` captures accepted inputs, exact completed outputs/warnings,
model-time bits, public compatibility facts and placement without a second execution. The
[v1 record contract](replay-record.md) defines independent decoding and read-only eligibility and
comparison. Capture can refuse its 64 MiB bound after successful execution; that does not undo the
frame or permit blind resubmission. No bytes are persisted by OCE.

Authenticate exact bytes and qualify the same build/deployment, executable/parameters and prior
state outside OCE **before decoding**. Then check `ReplayRecord::check_compatible`, prepare its
canonical inputs, execute once on an isolated compatible engine and verify the completed result.
A post-execution mismatch is not rollback or an equipment interlock. The content tag and public
descriptor are non-authoritative; no OCE build token is created or enforced.

The host owns the ordered sequence envelope, missing/duplicate policy and optional start/end
`EngineStateSnapshot` sidecars. The engine-lifetime sequence is not durable position and is absent
from records. Drop/process one record at a time for bounded retention. State continuation and
placement still obey the existing manifest/restore-window rules; portable does not mean universally
qualified mathematics, and conformance tolerances are not replay acceptance policy.

## Complete values are not sensor-quality evidence

Every executable boundary input is required exactly once. A missing determinant returns
`FrameMissingInput` before mutation, even if the connector had a prior value or the Store holds a
sample. There is no implicit hold-last, type seed, sparse write or Store fill-in.

Frames carry typed values, not `PointStatus` or wall-clock freshness. A host that deliberately
resubmits stale or faulted observations can still obtain a valid computational result. Validate
quality and observation coherence before preparation. The engine neither invents missing data nor
qualifies a host's substitution policy.

## The engine implements no fail-safe policy of its own

Completeness and type/domain checks do not implement degraded-operation or equipment protection.
If your plant needs to fail safe, that policy lives in the host or equipment. At minimum:

1. **Per-point staleness limits.** `PointSample` carries `at_unix_nanos`
   in the storage port; frame execution does not read it. Track sample age
   yourself and define, per point, how old is too old.
2. **A status reaction policy.** Decide what `Fault`, `Stale`, `Uninitialized` and `Override` mean
   for each input, and act on them before or instead of executing. The engine will not.
3. **A defined safe state, and a path to it.** Know what output set is safe for the equipment, and
   drive it from the host when the input contract is violated — do not expect the sequence to
   produce it.
4. **Plausibility checks on inputs.** Range, rate-of-change and cross-sensor consistency, applied
    before frame preparation.
5. **Equipment protection below the engine.** Any interlock you are relying on to prevent physical
   damage — freeze protection, high-limit cutouts, minimum off-times enforced in hardware — must
   exist in the host layer or in the equipment itself. The engine executes the sequence you gave it
   and nothing else; a sequence that omits an interlock has no interlock.
6. **Delivery-failure handling.** Persisting or delivering a `CompletedFrame` is your operation.
   Its success, rollback, retry policy and actuator acknowledgment are not engine guarantees.

## Time is host-supplied

The engine never reads a wall clock. Model time is host-supplied finite nondecreasing `f64` seconds;
a decrease returns `OcError::TimeRegression`. Loaded blocks also enforce representability. The
host owns cadence and any mapping between model seconds and external UNIX timestamps. Realtime
epoch configuration and realtime/Store orchestration are not facade capabilities.

Supply time from a source you trust to be monotonic. The engine cannot detect a clock that jumped.

## One call is one HostTick transition

The engine uses the fixed [HostTick v1 execution profile](execution-profile.md). Every successful
`execute_frame` call advances state once, even when its model time equals the previous timestamp. Equal time
means zero elapsed time to timers and integrators; it does not make the call observational. In
particular, `CDL.Logical.Pre` emits its stored Boolean and latches current input once per call.

Do not execute repeatedly at one timestamp to imitate Modelica event iteration. The engine does
not search for a fixed point, and every stateful block updates on each call. A `Pre`-cut Boolean loop
that cannot converge under Modelica may continue changing on every tick call without a diagnostic.
The completed result retains that transition's boundary outputs and diagnostics; latest-state
inspection can subsequently change. A host simulation loop is a sequence of complete submissions,
not an implicit restart. Split schedules continue the existing state. Earlier successful frames
remain committed if a later frame refuses, but that refusal stages no prefix. Use a fresh load for
a fresh run, or an explicit compatible process-local checkpoint for branching/rewind. There is no
whole-horizon transaction.

## Persist engine state outside the store port

`Engine::state_snapshot` returns the engine-owned canonical bytes needed to continue a run. It does
not write them anywhere. `Engine::checkpoint`, `state_snapshot`, `restore_checkpoint`, and
`restore_state` call no `Store` method. The [state contract](state-compatibility.md) defines the
same-loaded-executable guarantee, typed refusals and portability limits. The host owns durable
storage and an authenticated sealed envelope binding **exact snapshot bytes** to its approved
compiled-build/deployment qualifier, freshness and generation. Verify that envelope **before**
`EngineStateSnapshot::from_bytes`, then pass only the OCE bytes. OCE emits, accepts, stores and
enforces no build token; equal ABI/manifest, package or catalog facts do not authenticate a build.

Capture only after a model has loaded successfully and while no parameter edits are pending. A
durable capture also requires authored stable identities and registered state contracts for every
stateful block. The decoder enforces a 64 MiB limit, validates canonical ordering and manifest
self-consistency, and checks an integrity trailer. Capture runs this same bounded decoder before
returning a successful artifact. Format and execution ABI are now revision 2; revision-1 bytes
refuse without migration because their manifest omitted executable input-domain/unit facts.
Class-specific block-state invariants are checked during restore, when a target engine is available.
The trailer detects accidental corruption; it is not an authenticity or freshness proof. Protect
snapshot bytes according to the trust boundary of the host that consumes them.

Durable continuation has a narrow restore window:

1. Stop command delivery and fence the old process/actuator owner. An OCE halt is not this fence.
2. Read bounded host-envelope bytes using the host's durable-publication/recovery policy. Reject
   partial writes; authenticate the exact payload and build/deployment qualifier, freshness,
   rollback/replay policy and deployment generation before decoding. Package version or an FNV tag
   is insufficient. Refuse a mismatched build/deployment here, outside OCE.
3. Parse only approved OCE bytes with `EngineStateSnapshot::from_bytes`. Inspect `portability()`
   for placement, without treating it as build or numerical qualification.
4. Load the exact compatible executable/parameters into a fresh target. Load can have Store
   effects even if it fails; apply the separate compensation policy below.
5. Call `restore_state` before any accepted frame, dirty-parameter resume or either earlier restore.
   On refusal do not patch bytes, retry on an advanced target or bypass admission. Select a
   qualified target, approved cold start or rollback under host policy.
6. Reconcile external point values, quality, timestamps, histories and backend transaction state.
   Re-establish model-time/wall-clock mapping and choose the first complete, quality-approved
   observation set. An equal-time retry executes again; OCE has no durable delivery acknowledgment
   or engine-owned replay sequence position.
7. Acquire the new generation's exclusive actuator authority/lease and verify fencing at the
   delivery boundary before resuming writes. Record snapshot/command acknowledgments externally;
   successful restore alone authorizes nothing.

The target model must have the same executable manifest: block classes and parameters, port
bindings, connector types, schedule, state-slot layout, enum descriptors, external inputs, and
boundary outputs, computation units/quantities and effective complete-frame acceptance bounds.
The diagnostic model id may differ; executable compatibility may not. A refusal
is atomic and leaves engine and store state unchanged. Durable restore also refuses after the target
crosses a mutation boundary, even if that mutation was otherwise harmless
as defined by the state contract.

Snapshots for models that use the unchanged 15-class libm-dependent set are target-bound. They restore
only on the same architecture and operating system; `restore_state` returns
`EngineStateError::TargetDomainMismatch` before commit on another target. Other models carry
`StatePortability::Portable`, meaning no target restriction in that policy, not universal exactness.
The finite 21-signal Linux receipt does not widen it to arbitrary inputs, full closure or macOS.
Treat a target-domain refusal as a placement failure, not corrupt state; bind placement metadata
to the authenticated envelope rather than rewriting the OCE bytes.

Snapshots restore absolute model time and the prior-tick monotonicity guard. They do not carry the
real-time UNIX epoch, backend point history, point status or timestamps, backend transaction state,
or host safety policy. The current connector image, including staged input values, is part of the
snapshot. Restore the other state outside the engine. Use `EngineCheckpoint` instead when branching
or rewinding within one process; it is opaque and has no persistence format.

For `CDL.Logical.Pre`, a snapshot preserves both the currently visible output connector and the
Boolean memory that will be emitted on the next HostTick call. Restore does not evaluate the block.
A call at the restored timestamp advances it again.

For candidate changes, use the [release compatibility and host fallback checklist](release-compatibility.md#host-fallback-checklist).
Current/current is the only supported pairing; equal 0.1.0 package strings are not build authority.
No N-1 state/replay migration is supplied. Cold requalification starts fresh; external rollback uses
the prior qualified binary with its own authenticated state, never a current-engine transplant.

## Lifecycle names are not equipment controls

`Engine::halt()` does not stop complete-frame execution or host-owned output writes. It changes
only the parameter-edit permission mode: `set_param` is accepted while halted. The host must stop
calling execution methods if it intends execution to stop.

A `halt` / `set_param` / `resume` cycle is also a run restart, not live tuning. When parameters are
dirty, `resume` rebuilds blocks, allocates all state again, refreshes outputs, and clears the prior
model time. Every stateful block—including integrators, latches, timers, and filters—is re-seeded,
and monotonic-time history is lost. Plan parameter edits as a new run.

Executable ingest uses `load_cxf`; the never-working semantic/Modelica loaders have been removed.
Hosts prepare supported CXF outside the engine. Likewise, hosts decode CSV/table input outside the
facade and provide complete `prepare_frame` observations. `AssertLevel` contains only `Warning`,
which is now also its default; assertion reports neither escalate nor stop equipment. The collector
preserves the block's diagnostic source (currently the Assert class path, not an instance identity).
See [facade migration](facade-migration.md) for the intentional pre-release source/default break.

`point_list(None)` still returns the engine's own effective inventory. Its existing argument remains,
but device filtering is outside support: every `Some` returns `OcError::Load` directly, without
querying the store or changing the run. Wiring a capable `SemanticStore` does not enable that filter.

## CXF point identities are the authored `@id`s

Every point path — on the host-visible `IoInventory` and in the durable `PointDto` projection sent
through the PointStore port — is an authored `@id` from the source CXF document, expanded against
the document's `@context` to canonical absolute form at ingest: for a connector driven by a
composite boundary input it is the declared boundary input's `@id` (one host point fans out to
every internal consumer, which is why the G36 corpus's 3020 connectors surface as 2895 points),
and for every other connector it is the connector's own node's `@id`. CXF ingest rejects a
connector node without an `@id`, so a document-loaded point can never receive a positional
identity. Because keys are canonical, a document re-serialized between compact and expanded
spellings keeps its point paths; a relative `@id` that no `@context` can canonicalize is refused
at load with a typed `relative-iri` diagnostic rather than admitted under a spelling-dependent
key. The supported `@context` form is an inline prefix map — a single map, or a list of maps
merged in order with later bindings winning; a remote context reference, `@base`, `@import`,
`@vocab`, prefix bindings that are not absolute IRIs, and term definitions that use another active
prefix are refused at load as non-subset constructs rather than silently ignored. The last case is
a nested compact IRI; it includes an absolute-looking value such as `urn:oce:names#` when the same
context also declares `urn` as a term. Recursive context-term expansion is outside the supported
subset. A direct `@context` on an `@graph` node, one of its identity/type reference objects, or a
modeled value/term object is also refused: context bindings are document-level only, and the engine
never applies a scoped context to one semantic value. The canonical-key guarantee therefore holds
for every document that loads at all.

The document's declared boundary-output names (root `S231:hasOutput`) are a second read-only
identity space: each resolves on `get_output` and `watch` as an alias for
its driving internal connector's slot, and `Topology.boundary_outputs` enumerates the
`(path, driver_path)` pairs. Declared names stay out of `point_list` and `IoSummary`; committed
frames enumerate only the executable boundary. Their unit, quantity, and bounds are one §7.10 contract:
conflicts refuse at load and one-sided values propagate to the unset peer. Frame preparation never accepts
a declared output name. Because the driver's connector supplies host point metadata, a declared
alias can supply a previously unset driver unit, quantity, or bound. That changes the driver's
`IoInventory` and `point_list(None)` row for an unchanged input document; propagated unit and
quantity also reach the durable `PointDto`. `IoSummary` remains a count-only surface and does not
change when metadata propagates. Hosts that retain point metadata outside the store port must
refresh it after loading with this rule. An undriven declared output resolves nowhere; its load-time
`undriven-boundary-output` warning is its only representation.

A related contract for emitters and durable stores: array order is load-bearing wherever the
resolver reads an array — `@graph` node position, `containsBlock` order, each instance's port and
parameter lists, `isConnectedTo` order. The one carve-out is the boundary-input elision vector
(`external_inputs`) and the pass-through pair list: both are re-keyed on the boundary port's own
`@graph` node position instead of inheriting the order of that port's `isConnectedTo` array
(`crates/oce-cxf/src/resolve/mod.rs`, Step 9). Neither array order nor node position is a stable
identity: key by authored name, never by position.

Point histories persisted under the earlier positional `conn#<N>` keys are disposable, not
migratable: an index is not traceable to an authored connector after the document that produced it
changes.

## Ingest resource bounds

| Bound | Limit | Defined at | Behavior when exceeded |
| --- | --- | --- | --- |
| Serialized CXF at both facade load entry points | 8 MiB (8,388,608 bytes) by default; hosts may configure a smaller limit | `crates/oce-api/src/admission.rs` | `OcError::CxfTooLarge { actual_bytes, limit_bytes }` before JSON deserialization or Store calls |
| Expression parse and AST nesting | 64 | `crates/oce-expr/src/lib.rs` | typed `NestingTooDeep` error |
| Expression size | 4096 nodes | `crates/oce-expr/src/lib.rs` | typed `ExpressionTooLarge` error |
| Composite **nesting** (`containsBlock` lowering) | 64 | `crates/oce-cxf/src/resolve/composite.rs` | `MalformedDocument` diagnostic |
| Composite boundary path | 64 non-top `isConnectedTo` hops | `crates/oce-cxf/src/resolve/composite.rs` | `MalformedDocument` diagnostic |
| Composite boundary work | 65,536 target examinations and 8 MiB of aggregate target-IRI bytes per document | `crates/oce-cxf/src/resolve/composite.rs` | `MalformedDocument` diagnostic |

Composite nesting and boundary traversal are different walks and have separate limits. Boundary
traversal is iterative, so an accepted path does not consume one call-stack frame per hop. Its work
budgets also bound shallow fan-out and repeated long IRIs that a depth limit alone would miss.
Direct leaf wiring is outside those budgets. Below the limits the walk keeps target order and
duplicate paths intact for single-assignment validation.

## Treat untrusted CXF as untrusted input

A CXF document is a program. Loading one from a source you do not control is running code you did
not write. If you must:

- Bound transport/buffering before handing bytes to the loader. Both `load_cxf` and
  `load_cxf_with_receipt` enforce `Engine::cxf_byte_limit()` before deserialization, defaulting to
  `MAX_CXF_BYTES` (8 MiB). `set_cxf_byte_limit` accepts `0..=MAX_CXF_BYTES`; a larger request
  returns `CxfByteLimitTooLarge` without changing configuration. Zero refuses nonempty input;
  empty input still fails normal JSON parsing. Policy persists across reload and is not snapshot state.
- Load in a process or thread whose loss you can absorb when your threat model requires isolation.
- Never load an untrusted document in the same process that is actively commanding equipment.

The byte cap does not bound peak memory, CPU time, expansion, or total graph complexity. It is not
a sandbox, cancellation/deadline guarantee, or evidence that every below-cap document is safe.
Receipt admission refusals use the existing `Import` stage, with byte counts only and no input
content, source error or structured diagnostics in the `OcError`; `OperationFailure` still exposes
that original error through its normal source chain. Legacy malformed-input errors remain unchanged.

## Load replacement and the Store compensation boundary

Ordinary returned load failures preserve the prior **in-memory executable/run image**: model and
identity, blocks/schedule, state words and connector/output values, IO/parameters,
mode/dirty flags, prior time, semantic warnings, loaded state and
durable-restore readiness. Successful reload replaces model-bound caches and state, resets time and
parameter lifecycle, and opens the fresh durable-restore window. Refresh all model-local/ephemeral
references after success; admission policy persists and accepted-frame sequence does not reset.

This is not a Store transaction. `recover`, `save_model`, and `resolve_points` execute before the
in-memory commit and can have effects even when a later operation (or that call itself) fails.
The port has no abort/compensation hook. A saved candidate model and allocated point handles can
remain after refusal. The host/adapter owns compensation, isolation and re-establishing a usable
backend; **old external handle validity is not promised**. Load-time handles are validated but
not retained for execution. Do not blindly resume equipment control after a backend refusal. Panic, process death,
allocation failure, and concurrent host effects are outside the ordinary returned-error guarantee.

`reload_tests` compares the complete owned image for fresh, advanced and halted/dirty runs across
real import, unification, validation, schedule and injected Store refusals; the private build tail
covers instantiation and projection. Flatten is currently an infallible identity shim and semantics
currently returns metadata without a refusal path; no injected failures are claimed for those stages.
The separate residual-effects test demonstrates the MemStore compensation boundary.

The structural ingest paths above are bounded and return typed diagnostics rather than panicking.
`../TESTING.md` requires new ingest code to assert the specific `DiagCode` or error variant rather
than "an error occurred."

The tests cited on this page live in `oce-api` and run per PR on x86_64 and arm64 under debug and
release codegen. The full workspace and doctests still wait for the release gate. See
[`ci-and-the-gate.md`](ci-and-the-gate.md) for the exact split.
