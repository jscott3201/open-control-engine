# Host responsibilities

For anyone about to wire this engine to real equipment. It answers one question: what safety
behavior must **you** implement, because the engine deliberately does not?

This engine executes a control sequence. It does not supervise the equipment that sequence drives,
and it does not judge the quality of the data it is fed. Those are your job, and the engine will not
warn you if you skip them.

See the [product contract](product-contract.md) for numbered host obligations and the bounded
requirement-to-evidence map; engine boundary tests are not host-compliance evidence.

> **Production hosts: select the strict non-finite input policy.** Call
> `engine.set_non_finite_input_policy(NonFiniteInputPolicy::Refuse)` (on a target,
> `executable.set_non_finite_input_policy(…)`) before your first frame. The default is still `Admit`, which passes a NaN or infinite sensor value to the blocks; it becomes
> `Refuse` at the next native-evidence refresh (GitHub issue #363). See
> [Refuse non-finite Real inputs](#refuse-non-finite-real-inputs).

## Native complete frames and the host delivery boundary

The [complete-frame contract](complete-frame-contract.md) supplies current typed, read-only
preparation and an engine-local load/rebuild fence (PC-032). `execute_frame` consumes a prepared
submission and returns one immutable `CompletedFrame` after one atomic HostTick transition (PC-033).
PC-034 retains the shared private evaluation core. PC-035 removes the weaker legacy profiles;
preparation followed by consuming execution, and its slot-indexed twin over the same core driver
(`execute_slots`, issue #444), are the only public state-advancing execution surfaces.

Completeness means every executable boundary input exactly once, except omission explicitly
defined by the executable schema. It does not establish sensor coherence, quality, freshness or
plausibility. Preparation neither fills gaps from stored samples nor infers host defaults.
Its reload fence does not authorize deployments or commands. Persistence, authentication,
authorization, deployment fencing, scheduling/wall-clock mapping, NO_EVAL, safe states, equipment
interlocks and actuation stay host-owned. NO_EVAL means not executing.

Native execution calls no storage port or host callback. Persisting or delivering a retained result
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

Library, Studio and Edge retain their roles; Runtime is only an additive future M05-PR03
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
`FrameMissingInput` before mutation, even if the connector had a prior value or your store holds a
sample. There is no implicit hold-last, type seed, sparse write or stored-sample fill-in.

Frames carry typed values, not `PointStatus` or wall-clock freshness. A host that deliberately
resubmits stale or faulted observations can still obtain a valid computational result. Validate
quality and observation coherence before preparation. The engine neither invents missing data nor
qualifies a host's substitution policy.

## Refuse non-finite Real inputs

A NaN or infinite input sample is a hazard that no block masks reliably. One NaN can latch `PID`,
`Derivative` or `MovingAverage` state until reload, and `Line` or `LimitSlewRate` can turn it into
a finite, believable command. Block maths deliberately propagates non-finite values, as CDL does,
so the protection belongs at the boundary.

`Engine::set_non_finite_input_policy` selects what preparation does with a NaN (either sign, any
payload) or infinite value on a Real boundary input:

| Policy | Unbounded Real input | Bounded Real input | Default |
| --- | --- | --- | --- |
| `NonFiniteInputPolicy::Admit` | Accepted and passed to blocks unchanged | `InputDomain` (no value compares with NaN; finite bounds exclude the infinity beyond them) | **Yes, for now** |
| `NonFiniteInputPolicy::Refuse` | `InputNonFinite(path)` | `InputNonFinite(path)` | From the next native-evidence refresh |

**Production hosts are recommended to select `Refuse`.** `Admit` is the default only because the
bound strict-bit conformance evidence feeds NaN and ±∞ through the facade to pin CDL's non-finite
propagation. At the next native-evidence refresh the default becomes `Refuse`, the conformance
driver opts out explicitly, and the evidence is re-captured (GitHub issue #363). A host that
already selects `Refuse` sees no change then; a host relying on `Admit` must select it explicitly.

Under `Refuse`, `InputNonFinite` names the canonical input. Per input it is checked after
`InputType` and before `InputDomain`, so a NaN on a bounded input reports `InputNonFinite`. Nothing
is staged, model time does not advance and no sequence number is consumed: the engine is exactly
as it was before the attempt. Integer, Boolean and enum inputs are unaffected. Execution rechecks
the plan against the policy in force, so a plan prepared under `Admit` cannot carry a non-finite
value past a later switch to `Refuse`. Changing the policy changes nothing else: not the run
state, the time, the sequence, the snapshot bytes or an outstanding plan's validity.

The policy is host configuration, like `cxf_byte_limit`. It persists across loads, parameter
resumes and restores, and no snapshot, checkpoint or replay record carries it. It decides only
which frames are accepted; an accepted frame evolves state identically under either policy, so
replay and restore stay deterministic without it. A replay record captured under `Admit` can
carry a non-finite input; replaying it on a `Refuse` engine is refused at preparation, so replay
such a record on an engine that selects `Admit`. Restored state is not inspected: the policy
governs host inputs, not values a block already holds.

Producing the replacement value is the host's obligation, and the engine does not choose one:

1. **Substitute a finite value** that your fault policy defines for that point — a last good
   sample within its staleness limit, a configured fallback, or a value that drives the sequence
   toward its safe state — and record that you did.
2. **Or do not execute the frame** and drive the equipment's safe state yourself.

Do not substitute a bound or a type seed merely to get a frame accepted; a plausible value that
the sensor never reported is the masking this refusal exists to prevent.

### On a `no_std` target

A target that runs a [compiled-schedule blob](compiled-schedule-blob.md#executing-a-blob) through
`oce_graph::blob::Executable` gets the same frame driver and the same policy, with the same
default, `Admit`; select `Refuse` with `Executable::set_non_finite_input_policy` before the first
frame. `FrameRefusal::InputNonFinite` names the input slot rather than the path. Three duties that
the facade's names carry fall to the host there:

- **Map names to slots once.** Resolve each point's path with `input_slot` / `output_slot` at
  startup and keep the slot; a slot is only meaningful for the blob it came from.
- **Supply every slot every frame.** The entry takes one value per input slot and keeps none from
  the previous frame; reusing a stale value in your own buffer is the hold-last substitution the
  engine refuses to make for you.
- **Keep state words with their context.** `state_words` are raw, with no portability tag,
  manifest or checksum; store them with the blob and model time they came from, and authenticate
  them like any other state.

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
a decrease returns `OcError::TimeRegression`. Its magnitude must not exceed
`MAX_FRAME_TIME_MAGNITUDE` (`f64::MAX / 4`, about 4.49e307), or the frame is refused with
`OcError::TimeOutOfRange` for every model, and restore refuses a snapshot or checkpoint whose clock
is beyond it. Any single difference of two accepted times (a step, or the span since a timer
started) is then at most `f64::MAX / 2`. Accumulated elapsed time (a sum of rounded steps) can round
upward past the exact span; the remaining factor of two keeps it finite for fewer than about 3e15
accumulated steps. Gains and other model maths applied to time are not bounded. UNIX epoch
seconds, milliseconds and nanoseconds are all far inside the limit. A
large finite step (a clock jump from -1e9 to 1e18) is still accepted, and integrators take it in
one step: keep time contiguous yourself. Loaded blocks also enforce representability. The
host owns cadence and any mapping between model seconds and external UNIX timestamps. Realtime
epoch configuration and realtime/storage orchestration are not facade capabilities.

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

## Durability is the host's

The engine is pure: it owns no storage backend, `oce-api` does not depend on `oce-store`, and no
load, frame, snapshot or restore calls a storage port. Persisting anything is a separate host
operation. A load hands you the persistable model as plain data:

```rust,ignore
use oce_store::host::persist_loaded_model;

let (report, loaded) = engine.load_cxf(&cxf_bytes)?; // engine state is committed here
// `your_store` is your adapter behind the port, or `oce_store::MemStore`.
let input_handles = persist_loaded_model(&your_store, &loaded.model, loaded.input_points())?;
your_store.commit()?;                                 // the commit boundary is yours too
```

`persist_loaded_model` runs the sequence the engine used to run at load, in this order, stopping at
the first error: `Durable::recover` (so the save lands after recovered state, never under it),
`ModelStore::save_model`, then `PointStore::resolve_points` for every input point, refusing a
backend that returns a different number of handles with `StoreError::Validation`. The three steps
are also public on their own (`oce_store::host::{recover, save_model, validate_inputs}`) for a host
that interleaves its own work. The returned handles are in `input_points()` order; they are yours to
bind to your point layer or to drop, and the engine never reads them. The helper takes plain data only — `loaded.model` is the
`ResolvedModel` the port takes unchanged (both crates share the `oce_model::resolved_model` DTOs)
and `loaded.input_points()` yields `(canonical path, value type)` pairs — so `oce-store` never
depends on `oce-api`.

What it does not do is yours: it never calls `commit`, and it rolls nothing back. The engine has
already committed the load when you persist, so a failure here leaves the engine loaded and
running with the store partly written; whether to retry, delete the model or refuse to start is
your policy. Nothing you persist is read back by a frame.

## Persist engine state outside the engine

`Engine::state_snapshot` returns the engine-owned canonical bytes needed to continue a run. It does
not write them anywhere. `Engine::checkpoint`, `state_snapshot`, `restore_checkpoint`, and
`restore_state` call no storage port. The [state contract](state-compatibility.md) defines the
same-loaded-executable guarantee, typed refusals and portability limits. The host owns durable
storage and an authenticated sealed envelope binding **exact snapshot bytes** to its model, its
approved compiled-build/deployment qualifier, freshness and generation. Verify that envelope
**before** `EngineStateSnapshot::from_bytes`, then pass only the OCE bytes. The engine refuses a
snapshot another engine version wrote (`EngineStateError::EngineVersionMismatch`, at
`from_bytes`); that is all it checks about the bytes' origin. OCE emits, accepts, stores and
enforces no build token: an equal engine version, ABI/manifest, package or catalog fact does not
authenticate a build.

Capture only after a model has loaded successfully and while no parameter edits are pending. A
durable capture also requires authored stable identities and registered state contracts for every
stateful block. The decoder enforces a 64 MiB limit, validates canonical ordering and manifest
self-consistency, and checks an integrity trailer. Capture runs this same bounded decoder before
returning a successful artifact. Format and execution ABI are revision 1 and stay 1 until 1.0.0;
the header also names the engine version that wrote the snapshot, and only that version restores
it, so plan a version upgrade as a cold start or a restart on the prior qualified build. To key
stored snapshots and compiled-schedule blobs by that version, record the `major`, `minor` and
`patch` fields of `oce_api::ENGINE_VERSION` in the envelope beside each one. `EngineVersion` has
no `Display`, `Hash`, `Ord` or serde implementation, so the host spells out the three numbers
itself. They are the value both headers carry and the `expected` of an `EngineVersionMismatch`, so
before decoding the host can tell which stored bytes this build will refuse by version. A
matching version predicts only that check: the trailer, the fingerprint and the restore checks
can still refuse the bytes. Two builds of one version are not told apart: the sealed envelope is
still what binds bytes to the model, the build and the deployment that wrote them
([wire revision](state-compatibility.md#wire-revision-and-integrity)).
Class-specific block-state invariants are checked during restore, when a target engine is available.
The trailer detects accidental corruption; it is not an authenticity or freshness proof. Protect
snapshot bytes according to the trust boundary of the host that consumes them.

Durable continuation has a narrow restore window:

1. Stop command delivery and fence the old process/actuator owner. An OCE halt is not this fence.
2. Read bounded host-envelope bytes using the host's durable-publication/recovery policy. Reject
   partial writes; authenticate the exact payload and build/deployment qualifier, freshness,
   rollback/replay policy and deployment generation before decoding. Package version or an FNV tag
   is insufficient. Refuse a mismatched build/deployment here, outside OCE.
3. Parse only approved OCE bytes with `EngineStateSnapshot::from_bytes`. `EngineVersionMismatch`
   means another engine version wrote them: do not edit the header; restore on that version or
   cold start. Inspect `portability()` for placement, without treating it as build or numerical
   qualification.
4. Load the exact compatible executable/parameters into a fresh target. Load has no external
   effects; anything your host persisted around a load is yours to reconcile.
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
is atomic and leaves engine state unchanged. Durable restore also refuses after the target
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

When the engine build changes, work through this host fallback checklist:

- [ ] Preserve exact original bytes and refusal cause; do not migrate or rewrite them.
- [ ] Fence external actuator ownership/commands and select NO_EVAL while deciding.
- [ ] Authenticate exact bytes, full build qualifier, freshness, generation and replay order before
  decoding. Same Cargo version, descriptor equality and integrity checks are insufficient.
- [ ] Choose explicitly approved cold requalification or separately qualified external rollback;
  never auto-fallback or cross-build restore.
- [ ] For cold start, qualify CXF, parameters, complete IO domains, cadence and seeded behavior;
  reconcile anything your host persisted around the load and external state separately.
- [ ] For rollback, bind the prior binary to its own authenticated state and configuration; reject
  any mismatch before dispatch. No current decoder participates.
- [ ] Verify field fencing, quality policy, model-time mapping and exclusive command authority before
  delivery. Successful load, restore, replay or local tests alone authorize no field effect.

Current/current is the only supported pairing; equal package strings are not build authority.
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

## Real parameters are always finite

The engine refuses every non-finite Real parameter value (`NaN`, `+inf`, `-inf`) at every level of a
CXF document — root and nested composite declarations as well as leaf block parameters, and each
element of a Real array parameter — for every block class (issue #362). Load-time grounding is also
strict about expression intermediates: no expression node may produce a non-finite Real, so a
function cannot absorb one into a finite result. `integer(1/0)`, `sign(0/0)`, `min(1/0, 3.0)`,
`max(p, 2.0)` or `p > 0` with a non-finite `p`, and `Modelica.Constants.inf * 2` are all refused.
Connector attribute bounds are not parameters and keep their existing handling.

At load, `load_cxf` fails with `OcError::Cxf` carrying one `non-finite-parameter` error per offending
declaration, whose subject is the declaring instance and whose message names the parameter. That
applies whether the value came from a typed literal such as `"NaN"` or `"INF"` or from an
expression. Dependents that read the refused declaration add the existing `grounding-failed`
unknown-identifier cascade. Behind the resolver, the `oce-validate` load gate is a backstop: it
re-checks every block parameter on the flattened graph and would refuse a non-finite value under the
same code (`OcError::Validate`, subject = the block instance). No public load path reaches it with
one today, because grounding refuses first. When it does fire, *all* of that block's class-level
parameter rules are suppressed, not just range rules: its
`missing-required-parameter`, `parameter-kind-mismatch` and array/matrix/time-table shape
diagnostics surface only once the value is fixed. The load fails either way.

At rest, `set_param` returns `OcError::ParamRange` and leaves the parameter unchanged. A crafted
state snapshot cannot reintroduce a non-finite value: snapshots are compared bit-exactly against the
loaded, already-checked parameters and never restore parameter values.

No registered class takes a non-finite parameter, so there is no opt-out. The retained
`Modelica.Constants.inf` alias is `f64::MAX`, which is finite and accepted. Arithmetic on it is
ordinary finite arithmetic, not Modelica's IEEE infinity (issue #374): `inf/inf` is `1`, `inf-inf`
is `0`, `inf+1` is `f64::MAX`, and only an overflow such as `inf*2` produces a non-finite value that
the load refuses. Use the alias for comparison or pass-through, not as an arithmetic operand.

`integer(x)` refuses a Real whose floor lies outside the Integer range `[-2^63, 2^63)` (issue #374)
instead of saturating: `integer(1e300)` used to load as `i64::MAX` with no diagnostic. The load
fails with `grounding-failed` on the binding's node, for parameters and connector bounds alike;
`integer(-9223372036854775808.0)` is still exactly `i64::MIN`. The runtime
`CDL.Conversions.RealToInteger` block deliberately keeps its saturating conversion (NaN → 0, ±inf →
`i64::MAX`/`i64::MIN`): a running control loop must emit an output every tick, whereas a load can
refuse. A host that cannot tolerate saturation at run time keeps the block's Real input in range. Finite extremes, signed
zero and subnormals are accepted unless a class rule bounds them. The gate covers parameters only:
a non-finite input *signal* is still the host's to keep out, and it propagates through the blocks
under their documented IEEE behavior.

Executable ingest uses `load_cxf`; the never-working semantic/Modelica loaders have been removed.
Hosts prepare supported CXF outside the engine. Likewise, hosts decode CSV/table input outside the
facade and provide complete `prepare_frame` observations. `AssertLevel` contains only `Warning`,
which is now also its default; assertion reports neither escalate nor stop equipment. The collector
preserves the block's diagnostic source (currently the Assert class path, not an instance identity).

`point_list(None)` still returns the engine's own effective inventory. Its existing argument remains,
but device filtering is outside support: every `Some` returns `OcError::Load` directly, without
changing the run. The engine has no equipment index to delegate to.

## CXF point identities are the authored `@id`s

Every point path — on the host-visible `IoInventory` and in the plain-data `PointDto` projection a
load returns in `LoadedModel::model` — is an authored `@id` from the source CXF document, expanded against
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
subset. A term whose name is itself spelled as an IRI or a compact IRI (a `:` anywhere but first or
last, or a `/` anywhere) must bind its own expansion (issue #591). A term matches a token before any
other reading, so `"S231:ExtensionBlock": "http://example.org#Foo"` would let the document redirect
a standard spelling; it is refused as `malformed-document`, the error JSON-LD 1.1 calls an invalid
IRI mapping. Such a term must still bind its own expansion once every map of a list is merged,
because a later map may rebind the prefix it was read through; that shape, which JSON-LD accepts,
is refused as a non-subset construct. A direct `@context` on an `@graph` node, one of its
identity/type reference objects, or a modeled value/term object is also refused: context bindings
are document-level only, and the engine never applies a scoped context to one semantic value. The
canonical-key guarantee therefore holds for every document that loads at all.

A reference object (each value of a structural edge, `hasInput`, `hasOutput`, `hasParameter`,
`hasConstant`, `containsBlock`, `hasInstance` or `isConnectedTo`, and of `isOfDataType`) is read as
its `@id` alone (issue #588). One that carries any other key, such as `@type`, `@index` or a
property like `S231:value`, is refused as a non-subset construct, subject the node that holds it.
In JSON-LD such an object is an embedded node object that states more about the node it names;
flattened CXF states that on the node's own `@graph` entry, and the engine would otherwise drop it
without a word.

Property keys are expanded against the same `@context` (issue #383). A modeled property — `min`,
`max`, `nominal`, `unit`, `quantity`, `displayUnit`, `value`, the structural edges, and the rest
of the `S231:` terms the importer reads — is the same property whether it is keyed compactly
(`S231:min`), by its full IRI in the S231 namespace, by another prefix bound to that namespace, or
by a declared term.

A unit and a display unit have further spellings, the ones modelica-json and `CXF-Core.jsonld`
write (issue #631, [unit spellings](design/unit-spellings.md), owner rulings of 2026-11-12). Each
is matched after the same expansion, on every node that carries one:

- **`unit`** is also `S231:hasUnit`, and `hasUnit` in either QUDT schema namespace:
  `http://qudt.org/schema/qudt#`, as the modelica-json translations bind `qudt`, and
  `http://qudt.org/schema/qudt/`, as QUDT and CXF-Core do. The compact key `qudt:hasUnit` is that
  property only through a `qudt` prefix bound to one of the two.
  **A `hasUnit` key that reading does not reach is refused, never left unread** (issue #675, owner
  ruling of 2026-11-20). It is matched on the expanded IRI, on every `@graph` node, whether the
  document writes a compact key, a full IRI or a `@context` term:
  - `qudt:hasUnit` in a document that binds no `qudt` prefix is `malformed-document`;
  - `hasUnit` in a near miss of either schema namespace is `vocabulary-namespace-mismatch`: the
    `https:` spelling, the scheme or the host in another letter case, a default or empty port,
    and a missing, repeated or wrong separator (`https://qudt.org/schema/qudt#hasUnit`,
    `http://QUDT.org/…`, `http://qudt.org:80/…`, `http://qudt.org/schema/qudthasUnit`,
    `http://qudt.org/schema/qudt/#hasUnit`);
  - `hasUnit` in the QUDT unit vocabulary (`http://qudt.org/vocab/unit#` or `…/unit/`), or a near
    miss of it, is `vocabulary-namespace-mismatch` too: `qudt` bound to the `unit:` IRI, or the
    key written `unit:hasUnit`.

  The message names the key and the namespace as written, each in at most 64 bytes with format
  and control characters escaped, and the two namespaces the property is read in. Bind `qudt` to
  one of them, as every modelica-json translation does, or write `S231:unit`. Under a prefix
  bound to a namespace that is no QUDT one and no near miss of one, `hasUnit` is that
  vocabulary's property and passes through unread, as any other vocabulary's key does:
  `http://example.org/qudt#`, and also another host (`http://www.qudt.org/schema/qudt#`), a path
  in another letter case and a versioned IRI (`http://qudt.org/2.1/schema/qudt#`), none of which
  RFC 3986 reads as the same namespace. So does a key that differs in its prefix, its local name
  or its letter case (`QUDT:hasUnit` with no such binding, `qudt:HasUnit`).
- **`displayUnit`** is also `S231:hasDisplayUnit`.
- **`quantity` has no other spelling.** `qudt:hasQuantityKind` is not read: modelica-json writes
  `PressureDifference` as the kind `Pressure`, so reading it would report the wrong quantity. The
  key stays in the pass-through map.

A unit read under any of these keys reaches the §7.10 unit gate, the point list, the content id
and the export exactly as one read under `S231:unit` does.

A unit's shape decides how it is read, not its key. A string or a typed literal is the unit
string, verbatim. **An IRI node** for a unit or a display unit, under any key that names it and
the engine's own `S231:unit` included, is expanded against the document `@context` and read
through a closed table:

- a QUDT unit IRI, in `http://qudt.org/vocab/unit#` (modelica-json) or
  `http://qudt.org/vocab/unit/` (QUDT), reads as the Modelica unit string of one of 25 entries:
  `K` as `K`, `PA` as `Pa`, `DEG_C` as `degC`, `UNITLESS` as `1`, `M3-PER-SEC` as `m3/s`. The
  table is modelica-json's own, inverted, and is in the engine: nothing is dereferenced, and
  nothing registers an entry at run time (`oce_api::QUDT_UNITS`, `oce_api::unit_of_iri`);
- an IRI in the S231 namespace reads as the text after the namespace, the form modelica-json
  writes for a unit its table lacks (`http://data.ashrae.org/S231#s/K` is `s/K`). The text is not
  empty and holds no whitespace and no format or control character;
- **any other IRI is refused** as `non-subset-construct`, subject the node, with the IRI quoted in
  at most 64 bytes: a relative reference (`{"@id": "degC"}`), an `@id` that holds whitespace, a
  `unit:` prefix the document binds to another vocabulary or not at all, a name the table lacks,
  and the two entries left out of it. Those two
  are `unit:M3-PER-MIN`, which modelica-json writes for `cfm` and which is cubic metres a minute,
  and `unit:IN_H2O_60DEG_F`, which it writes for `inH2O` and which is the inch of water at 60 °F.
  Write such a unit as a string.

So `{"@id": "unit:K"}` and `"K"` are one unit to the gate and to the host, under any key. A
`quantity` is not read through any table: its IRI node is its `@id`, verbatim and unexpanded. A
parameter's unit goes through the same reading where the parameter is read, though it is used
for nothing.
Export writes every unit back as a bare string under `S231:unit` or `S231:displayUnit`
([the unit spellings read](cxf-round-trip.md#the-unit-spellings-read-and-the-one-written)), so a
round trip normalises the spelling.

A parameter's `min`, `max`, `unit`, `quantity` and `displayUnit` are never used for control, but
their shape is checked where the parameter is read (GitHub #594, issue #559): a bound is one
number, typed literal or expression string (modelica-json writes a bound over other parameters as
an expression, `"yMin"`), and a term has the shape every term has (below). No string among the
bounds may be empty or whitespace-only. JSON `null` reads as absent. A malformed attribute is
refused in one of two ways:

- **`malformed-document`, subject the parameter**, once per malformed attribute however often the
  parameter is read: a bound that is a list, a boolean or a blank string, or a typed literal with
  a blank `@value`; and every malformed term. Only a parameter that is read gets it; one pruned by
  a false conditional guard, or listed by no node, is not checked.
- **A parse failure of the whole document** (`CxfError::Json`; no code and no subject): a bound
  that is an object but no typed literal with a string `@value` and `@type` (`{}`, `{"@id": …}`,
  `{"@value": "30"}`, `{"@value": [1], …}`, `{"@list": …}`), or a list holding one. Such a bound
  matches no shape the document reader knows, so the document fails to parse wherever the
  parameter is, read, pruned or listed by no node, as a malformed `S231:value` does.

The value of a well-shaped attribute is still not read.

A term (`unit`, `quantity`, `displayUnit`) on any node that carries one, a connector, a boundary or
a parameter, is a string, a typed literal holding exactly `@value` and `@type`, or an IRI node
holding exactly `@id`, and it is not empty or whitespace-only: not the string, the `@value` or the
`@id` (GitHub #602, owner ruling of 2026-11-12). Anything else is refused as `malformed-document`,
subject the node: another JSON shape, an object carrying both `@id` and `@value` (with or without
`@type`), an object carrying any other key (`@language`, `@index`, `qudt:symbol`), and a blank term.
On a connector a blank term would otherwise be a declared unit to the §7.10 gate: unequal to any
other, so a cluster an absent unit lets load is refused as a mismatch, and alone, propagated to
the peers that declare none and reported in the point list as a unit. An Integer or Boolean
connector permits no term, whatever its shape, and is refused for carrying one: CDL §7.4.1.2 gives
an Integer only `min` and `max` (owner ruling of 2026-11-12; before, a well-shaped term on an
Integer connector was dropped without a word). The check runs where the node is read, as a
parameter's does: a connector or parameter of a block pruned by a false conditional guard, or a
node no other node lists, is not checked.

Nor may a term carry a Unicode format or control character (general category `Cf` or `Cc`, the
class a typing token is held to, below: a zero-width space, a byte-order mark, U+2060, U+180E, a
soft hyphen, a bidirectional control, a C0 or C1 control) anywhere in the string, the `@value` or
the `@id` (GitHub #646, issue #629, owner ruling of
2026-11-12). Such a term is refused as `malformed-document`, subject the node, once. The message
names the first such code point (`U+200B, a format character (Cf)`) and quotes the term excerpted,
with each such character written `\u{…}`, never as it is. No reader can see such a character:
before, `"K\u{200B}"` loaded and reached the §7.10 gate and the point list as a unit that looks
like `K` and equals no other. The shape refusals above come first, then the blank one, so a term of
whitespace alone is blank even where that whitespace is a control character too (a tab, a line
feed, a carriage return, U+0085). The same characters around or inside a visible term are this
refusal (`"K\t"`): a term, unlike a typing token, has no surrounding-whitespace rule that would
trim them away, and a term may carry surrounding spaces. On an Integer or Boolean connector the
term is refused for being there, once, whatever it carries. A typed literal's `@type` is the
datatype, not the term, and is not judged. Characters outside `Cf` and `Cc` are not this rule's:
a no-break space or a line separator inside a term, a variation selector, a Hangul filler and a
private-use character still load. The authoring importer refuses the same characters, decided by
the same table (`oce_api::is_format_or_control`), as `wrong-shape` naming the key.

The importer reads the S231 vocabulary in one namespace, `http://data.ashrae.org/S231#`: the one
the exporter binds, the OpenBuildingControl (OBC) specification lists (OBC §11.3) and
modelica-json emits (GitHub #527; the evidence is on the
[CDL coverage page](cdl-coverage.md#the-standard-this-page-measures-against)). A document either
binds `S231` to exactly that IRI or binds no `S231` at all, in which case its compact keys are
read in the same namespace. Every other spelling is refused with `vocabulary-namespace-mismatch`,
and the message names the IRI found and the IRI expected:

- `S231` bound to any other IRI. An IRI is compared as a string, so the `https:` spelling of the
  same host and path is another IRI and is refused too.
- Any prefix bound to `http://data.ashrae.org/S231P#`. The engine wrote that IRI through release
  0.4.0. No importer, alias or second spelling reads a document of that layout: export it again
  with a current engine or tool.
- A modeled term keyed by its full IRI in that namespace or in the `https:` spelling of either
  namespace, and a `@type` or `isOfDataType` value in the retired namespace.
- A `@type` or `isOfDataType` value in the `https:` spelling of either namespace, whatever the
  term (GitHub #620; the class `ExtensionBlock` since GitHub #582). The vocabulary is read only in
  the published namespace, so such a spelling would otherwise be read by its trailing term, or for
  `ExtensionBlock` as an ordinary class (see
  [what the engine does not execute](cdl-coverage.md#what-the-engine-does-not-execute)).

In a list of context maps only the binding in force is judged (a later map replaces an earlier
one). Another prefix bound to the published IRI is the same namespace under another name and is
accepted. A node that writes one property in two spellings
(an explicit `null` or `[]` compact key counts; `S231:unit` beside `qudt:hasUnit` is two
spellings of `unit`, and is refused even when the two values agree) and a key that names a modeled
property only after trimming surrounding whitespace (`"S231:hasUnit "`, `"qudt:hasUnit "`), or
is a refused `hasUnit` key only then (`"qudt:hasUnit "` with no `qudt` binding), are refused as
`malformed-document`, as is any key — modeled or
not — written more than once on one `@graph` node, since JSON parsing would silently keep only one
of the values. None of these is silently
dropped. Keys that name no modeled property pass through unchanged. A `@type` value or an
`isOfDataType` reference that names an S231 term only after trimming surrounding whitespace
(`"S231:ExtensionBlock "`) is refused the same way (issue #587): it would be kept as written and
read as no S231 type, and a composite never reads its type, so a root typed that way loaded its
children.

A `@type` value or an `isOfDataType` reference in the published namespace must name a term of the
S231 vocabulary the engine reads, on any node (GitHub #620, owner ruling of 2026-11-12); anything
else is refused as `malformed-document`, subject the node, the token and term quoted as
excerpts. The one exception is an S231-spelled `Buildings.Controls.OBC.CDL.` block class the
registry does not implement, which is refused as `class-not-found` (`no registered block class
for …`), as the same class is in any other spelling. The vocabulary is one set: the classes the
vendored `CXF-Core.jsonld` declares, the terms it uses without declaring them, the terms the
engine writes, both endings of CXF-Core's `DataType`/`Datatype` classes, the registry's 133 CDL
block classes written `Buildings.Controls.OBC.CDL.…`, and the 18 CDL and G36 enumeration and
constant package types by their full path (the
[CDL coverage page](cdl-coverage.md#the-standard-this-page-measures-against) lists them). An S231 enumeration reference is matched by its full path, so
`S231:Buildings.Controls.OBC.CDL.Types.ZeroTime` is read and `S231:ZeroTime` is refused. A term a
newer CXF-Core adds is refused until the engine's table holds it. A class in another namespace (a
leaf's `http://example.org#Buildings.Controls.OBC.CDL.Reals.Add`, a composite's own class) is not
judged by this rule: a leaf's must still be a registered class (`class-not-found`).

A `@type` value or an `isOfDataType` reference in any namespace that carries a Unicode format or
control character (general category `Cf` or `Cc`: a zero-width space, a byte-order mark, a soft
hyphen, a C0 or C1 control) inside its surrounding whitespace is refused as
`malformed-document`, subject the node (GitHub #641, owner ruling of 2026-11-12). The message names
the first such code point (`U+200B, a format character (Cf)`) and quotes the token with each such
character written `\u{…}`, never as it is. The check comes before every other on the token:
surrounding whitespace alone, a tab or line break included, is still the whitespace near miss
above, and a token with both is this refusal, once.

The document's declared boundary-output names (root `S231:hasOutput`) are a second read-only
identity space: each resolves on `get_output` and `watch` as an alias for
its driving internal connector's slot, and `Topology.boundary_outputs` enumerates the
`(path, driver_path)` pairs, each with the output's exact type and its `quantity`, `unit` and
`displayUnit` after that unification. `Topology.boundary_inputs` lists the executable boundary
inputs in the order and with the type and effective bounds of `Engine::input_definitions`, each
with its declaration's three labels, and `Topology.model_id` is the root identity
`LoadReport::model_id` reports. Together these are the boundary facts the
[canonical content id](design/content-id.md) encodes, so two loaded models with equal topologies
have equal ids (the converse does not hold: a topology also carries the namespace and the
loader's order). Declared names stay out of `point_list` and `IoSummary`; committed
frames enumerate only the executable boundary. Their unit, quantity, and bounds are one §7.10 contract:
conflicts refuse at load and one-sided values propagate to the unset peer. Frame preparation never accepts
a declared output name. Because the driver's connector supplies host point metadata, a declared
alias can supply a previously unset driver unit, quantity, or bound. That changes the driver's
`IoInventory` and `point_list(None)` row for an unchanged input document; propagated unit and
quantity also reach the durable `PointDto`. `IoSummary` remains a count-only surface and does not
change when metadata propagates. Hosts that retain point metadata outside the engine must
refresh it after loading with this rule. An undriven declared output resolves nowhere; its load-time
`undriven-boundary-output` warning is its only representation. The input side mirrors it: a root
`hasInput` declaration that no connection reads is absent from `input_definitions()` (every listed
input must be supplied, and nothing would read this one), resolves nowhere, and is reported only by
an `unused-boundary-input` load warning whose subject is the declared IRI. A host that drives frames
from its own point list should check the load receipt for that code and reject or reconcile the
model before preparing frames. The check covers the root interface only; an unread input of a
nested composite's own interface is internal wiring and does not warn.

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
| Serialized CXF at both facade load entry points | 8 MiB (8,388,608 bytes) by default; hosts may configure a smaller limit | `crates/oce-api/src/admission.rs` | `OcError::CxfTooLarge { actual_bytes, limit_bytes }` before JSON deserialization or engine mutation |
| Expression parse and AST nesting | 64 | `crates/oce-expr/src/lib.rs` | typed `NestingTooDeep` error |
| Expression size | 4096 nodes | `crates/oce-expr/src/lib.rs` | typed `ExpressionTooLarge` error |
| Composite **nesting** (`containsBlock` lowering) | 64 | `crates/oce-cxf/src/resolve/composite.rs` | `MalformedDocument` diagnostic |
| Composite boundary path | 64 non-top `isConnectedTo` hops | `crates/oce-cxf/src/resolve/composite.rs` | `MalformedDocument` diagnostic |
| Composite boundary work | 65,536 target examinations and 8 MiB of aggregate target-IRI bytes per document | `crates/oce-cxf/src/resolve/composite.rs` | `MalformedDocument` diagnostic |
| Diagnostic text of one CXF read | 8 MiB (8,388,608 bytes) of messages and subjects together, a subject counted for each diagnostic | `crates/oce-cxf/src/diagnostic_list.rs` | the list ends; one closing `resource-limit-exceeded` error counts what is not listed |
| Constructed blocks, model-wide | 4,194,304 words (32 MiB) held by the blocks outside their state: tables, gain matrices, resolved port kinds | `crates/oce-graph/src/constructed.rs` | `OcError::Build(BuildError::ConstructedTooLarge { block, requested, limit })` at instantiation, recorded as `resource-limit-exceeded`; `BlobError::ConstructedTooLarge` from the blob decoder |
| Run state, model-wide | 4,194,304 words (32 MiB) | `crates/oce-graph/src/lib.rs` | `OcError::Build(BuildError::StateTooLarge { block, requested, limit })`, recorded as `resource-limit-exceeded`; `BlobError::StateTooLarge` from the blob decoder |
| What `@context` expansion adds to one document | 8 MiB (8,388,608 bytes) over every expanded token, an expansion counted as its length less the token's | `crates/oce-cxf/src/expansion_budget.rs` | `resource-limit-exceeded` on the node the first token that does not fit is written on; nothing after it is read |

Composite nesting and boundary traversal are different walks and have separate limits. Boundary
traversal is iterative, so an accepted path does not consume one call-stack frame per hop. Its work
budgets also bound shallow fan-out and repeated long IRIs that a depth limit alone would miss.
Direct leaf wiring is outside those budgets. Below the limits the walk keeps target order and
duplicate paths intact for single-assignment validation.

### A refusal list is a bounded answer

A diagnostic copies document text: its subject is a node's `@id`, and most messages name that
`@id` again. So the CXF reader holds the diagnostics of one read, a refusal list or the warnings
of a load, to 8 MiB of text (issue #695). What you can rely on:

- **The bound.** The messages and subjects of the listed diagnostics are at most 8,388,608
  bytes together. A subject is counted for every diagnostic that carries it, whether or not the
  diagnostics share one allocation of it, so the bound is what you pay when you copy, print,
  log or bind each diagnostic with its subject. `load_cxf_with_receipt`, which copies every
  record's subject, holds no more than the bound again for the import stage.
- **What that is in memory.** A returned list holds its text and one record for each
  diagnostic, with no spare capacity in a message. Measured: 1,073 bytes beside 7.9 MB of text
  for sixteen large diagnostics, and 64 bytes a diagnostic, 1.26 times the text, for 33,421 of
  about 250 bytes. Where diagnostics share a subject the list holds less than its text. While
  a load runs, the reader holds that list beside the document it parsed: peaks of 6.1 to
  11.7 MB for the refused documents of 0.27 to 0.57 MB that the cost tests measure.
- **Past the bound.** Diagnostics are listed in the order the reader raises them. The first one
  that does not fit ends the list: it and every later one are counted, never listed, even where
  a later one would have fit. The list then ends with one `resource-limit-exceeded` error with
  no subject. Its message starts with the count and names the first diagnostic left out, by its
  code and at most 64 bytes of its subject with format and control characters escaped:
  `385 more diagnostic(s) are not listed, the first of them `malformed-document` on
  `urn:long:xxx…`: a diagnostic list holds at most 8388608 bytes of message and subject text`.
  One diagnostic larger than the whole bound therefore leaves that error alone, and it still
  says what was refused and where. The diagnostics before it are in the reader's usual order.
- **Where the closing error is.** Last in `CxfError::Validation` and in
  `OcError::diagnostics()`. A receipt sorts its records by stage, subject, code and severity,
  and a record with no subject sorts before every record with one, so in
  `load_cxf_with_receipt` it is the first `Import` record.
- **It is always a refusal, for warnings too.** The closing error is an error whatever was
  left out, so a document that raises nothing but warnings is refused once they pass the
  bound: the bound is the one reason a document with no error of its own does not load. A load
  never succeeds with warnings missing. Measured at the edge on the reader
  (`oce_cxf::import_cxf`, which has no admission limit): 127 unused boundary inputs with 64 KiB
  `@id`s written out, a document of 16.7 MB, load with 127 warnings, and 128 are refused.
  Through the facade that document is over the admission limit, and the same inputs written
  through a prefix are refused first for what expansion adds
  ([below](#expansion-adds-a-bounded-amount-to-a-document)): 8 MiB of warnings about distinct
  nodes takes 16 MiB of `@id`s, each written on its node and in a reference.
- **A list within the bound is whole.** Every diagnostic the reader raises is listed, with its
  message and subject as they are: nothing is shortened, cut, merged or dropped. Every tracked
  document and every document of `open-control-library` is far inside the bound (the largest
  list among them is 196 diagnostics and 29,460 bytes, from a document of 201,721 bytes).
- **Work.** Where one node can raise a diagnostic per item of a list the document sets (its
  keys, its `@type` tokens, its references, its `hasInstance` members, its repeated children, a
  composite's repeated declarations), a finding past the bound is counted and nothing is built
  for it. Other findings past the bound are built and dropped, so they cost their copying but
  hold nothing (issue #711). That is bounded memory, not bounded time: a document that asks for
  very many such findings is refused after as long as building them takes, where without the
  bound it failed on allocation first.

The bound covers what `oce-cxf` raises while it reads. The later load stages (unification,
validation, the schedule) return lists of their own, which this bound does not hold: a receipt
copies their subjects too (issue #708). An export's diagnostics are not held to it either.

### Expansion adds a bounded amount to a document

A token read through a `@context` prefix is as long as the IRI the prefix binds: `p:T` under a
64 KiB binding is 64 KiB once expanded. The document sets that length and the number of tokens,
and pays three bytes for each, so nothing ties what the reader holds to the document's size
unless the reader counts it: 1,024 such tokens, a document of 78,157 bytes, loaded and held
202 MB (issue #709). What you can rely on:

- **The bound.** Expanding a document's tokens adds at most 8,388,608 bytes to it. An
  expansion is counted as its length less the length of the token as written, over every node
  `@id`, every reference (`hasInput`, `hasOutput`, `hasParameter`, `hasConstant`,
  `containsBlock`, `hasInstance`, `isConnectedTo`), every `@type` and `isOfDataType`, every
  `unit` and `displayUnit` IRI node, and every property key that ends in `hasUnit` and is read
  through a prefix. So the identity and typing text the reader works on is at most the
  document's own plus 8 MiB.
- **What adds nothing.** A token written as an absolute IRI is not expanded. A token that
  expands to no more bytes than it is written in adds nothing. Reading a token (to classify it,
  to compare it with a namespace, to find it relative) copies nothing and is not counted, so a
  property key, a relative reference or a `@context` term through a long binding costs what the
  token costs, never the binding again.
- **Past the bound.** The first expansion that does not fit is not made. The document is
  refused with one `resource-limit-exceeded` error whose subject is the node the token is
  written on (the token as written, for a node's own `@id`; `@graph[i]` for a key on a node with
  no `@id`), and whose message quotes at most 64 bytes of the token, escaped, and states the
  sizes: ``@type `p:T` expands to 65547 bytes through the document's @context, 65544 more than
  it is written in; expansion had added 8324709 bytes, and may add at most 8388608 to one
  document``. Nothing after that token is read, so the list holds that error and whatever was
  raised before it, in the reader's usual sorted order.
- **What that is in memory.** A refused document costs what it added before the refusal: the
  issue's document now peaks at 8.6 MB. A document that fills the bound exactly loads, and
  peaks at 17.3 MB for 137 KB of bytes, because lowering copies the expanded working copy once.
  That is the worst case of this bound: a fixed 16 MiB or so beside what the document's own
  bytes cost, whatever the document.
- **No real document is near it.** Over the 257 tracked documents that hold an `@graph` and the
  139 CXF documents of `open-control-library`, the most expansion adds to one document is
  66,275 bytes (the vendored `CXF-Core.jsonld`, 682,615 bytes), 126 times under the bound; the
  longest IRI any of them binds is 51 bytes, and expansion never adds more than 0.31 bytes for
  each byte of a document. A document at the 8 MiB admission limit would have to more than
  double under expansion to be refused.

The bound is on what expansion adds, not on the length of a binding: one long namespace IRI
read by a few tokens loads, as it did.

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
Below the cap the pipeline refuses the known amplification shapes with typed diagnostics (issue
#365): a `containsBlock` child reached twice (`shared-instance`), array parameters past a
1,048,576-element per-document budget, run state past 4,194,304 words, since issue #700 what
the constructed blocks hold outside their state (tables, gain matrices, resolved port kinds)
past 4,194,304 words, and since issue #709 `@context` expansion that adds more than 8 MiB to a
document (all four `resource-limit-exceeded`), and binding evaluation past
2^24 deterministic work units per expression or 2^26 per document (`grounding-failed`). Those limits bound growth, not absolute footprint — a maximal admissible
document can still need hundreds of MiB — so isolation stays your obligation.
Receipt admission refusals use the existing `Import` stage, with byte counts only and no input
content, source error or structured diagnostics in the `OcError`; `OperationFailure` still exposes
that original error through its normal source chain. Legacy malformed-input errors remain unchanged.

## Load replacement leaves no external effect

Ordinary returned load failures preserve the prior **in-memory executable/run image**: model and
identity, blocks/schedule, state words and connector/output values, IO/parameters,
mode/dirty flags, prior time, semantic warnings, loaded state and
durable-restore readiness. Successful reload replaces model-bound caches and state, resets time and
parameter lifecycle, and opens the fresh durable-restore window. Refresh all model-local/ephemeral
references after success; admission policy persists and accepted-frame sequence does not reset.

Load calls no storage port, so a refused load has no external effect to compensate: the engine
either commits the new model and returns a `LoadedModel`, or returns an error and keeps the prior
image. If your host persists the `LoadedModel`, it does so after the engine has committed, and the
ordering, transaction and compensation of that write are yours. Panic, process death, allocation
failure, and concurrent host effects are outside the ordinary returned-error guarantee.

`reload_tests` compares the complete owned image for fresh, advanced and halted/dirty runs across
real import, unification, validation and schedule refusals; the private build tail covers
instantiation and projection. Semantics currently returns metadata without a refusal path; no
injected failures are claimed for that stage.

The structural ingest paths above are bounded and return typed diagnostics rather than panicking.
`../TESTING.md` requires new ingest code to assert the specific `DiagCode` or error variant rather
than "an error occurred."

The tests cited on this page live in `oce-api` and run per PR on x86_64 and arm64 in debug and
release codegen (`bash .agents/gate.sh determinism` reproduces both on one host). The full workspace and doctests still wait for the
nightly release gate. See
[`ci-and-the-gate.md`](ci-and-the-gate.md) for the exact split.
