# Host responsibilities

For anyone about to wire this engine to real equipment. It answers one question: what safety
behavior must **you** implement, because the engine deliberately does not?

This engine executes a control sequence. It does not supervise the equipment that sequence drives,
and it does not judge the quality of the data it is fed. Those are your job, and the engine will not
warn you if you skip them.

## Staging is status-agnostic

A sample is converted from its value regardless of `PointStatus`. `Fault`, `Stale`, `Uninitialized`
and `Override` all stage exactly like `Ok`.

The conversion function destructures the sample and discards both quality fields:
`crates/oce-api/src/engine.rs:381-386` binds `status: _` and `at_unix_nanos: _`, then dispatches
purely on the value and the target type. The five statuses are defined at
`crates/oce-store/src/lib.rs:81-92`; nothing in the engine reads them. The behavior is pinned by
`store_backed_input_staging_is_status_agnostic`
(`crates/oce-api/src/tests/store_backed_inputs.rs:75`), which ticks the same fixture once per status
and asserts identical staging.

This is a design decision, not an oversight — point quality is metadata for the application and BMS
layer, and an engine that silently reinterpreted a faulted reading would be harder to reason about
than one that never looks. But it means a faulted sensor reading drives your sequence exactly as a
healthy one does.

## A missing sample is not an error

If no sample is available for a bound input, the connector keeps its current value and the tick
proceeds. There is no diagnostic. Before the first sample ever arrives, that held value is the
type's `zero_value()` — so an input that has never been written reads as `0`, `0.0` or `false`, not
as "unknown".

The hold is explicit: `crates/oce-api/src/engine.rs:356-360` continues past a missing sample with
the comment "Deliberate hold-last: no store sample means no overwrite of the current state value",
and the policy is documented at `engine.rs:336-342`. `missing_store_sample_holds_prior_input_value`
(`crates/oce-api/src/tests/store_backed_inputs.rs:99`) pins it.

**A dead sensor and a steady sensor are indistinguishable to the engine, forever.** Nothing in the
engine will ever notice that a point stopped updating.

## The engine implements no fail-safe policy of its own

Taken together, the two behaviors above mean the engine has no concept of degraded operation. It
will keep computing and keep writing outputs from held, stale, faulted values indefinitely. If your
plant needs to fail safe, the logic that makes it fail safe lives above the engine, in your host
layer. At minimum, implement all of the following:

1. **Per-point staleness limits.** `PointSample` carries `at_unix_nanos`
   (`crates/oce-store/src/lib.rs:97-104`), and the engine throws it away at staging. Track sample age
   yourself and define, per point, how old is too old.
2. **A status reaction policy.** Decide what `Fault`, `Stale`, `Uninitialized` and `Override` mean
   for each input, and act on them before or instead of ticking. The engine will not.
3. **A defined safe state, and a path to it.** Know what output set is safe for the equipment, and
   drive it from the host when the input contract is violated — do not expect the sequence to
   produce it.
4. **Plausibility checks on inputs.** Range, rate-of-change and cross-sensor consistency, applied
   before staging.
5. **Equipment protection below the engine.** Any interlock you are relying on to prevent physical
   damage — freeze protection, high-limit cutouts, minimum off-times enforced in hardware — must
   exist in the host layer or in the equipment itself. The engine executes the sequence you gave it
   and nothing else; a sequence that omits an interlock has no interlock.
6. **Write-failure handling.** `Engine::step_realtime` is not transactional: if the batched store
   write fails, the tick has already completed and model time and outputs have advanced, and they
   are not rolled back (`crates/oce-api/src/sim.rs:457-458`).

## Time is host-supplied

The engine never reads a wall clock. `std::time::Instant` appears only as a monotonic timer for
latency metrics, never as a time source for the model (`crates/oce-api/src/sim.rs:6`). Model time
arrives as a `f64` argument you pass in, and it must be monotonic — a decrease returns
`OcError::TimeRegression` (`crates/oce-api/src/error.rs:64-71`).

For real-time stepping you must first configure the UNIX epoch corresponding to model `t = 0`, via
`Engine::set_realtime_epoch_unix_nanos` (`crates/oce-api/src/sim.rs:360-373`). If you never do,
`step_realtime` returns `OcError::RealtimeEpochUnset` before ticking rather than silently stamping
samples at 1970 (`crates/oce-api/src/sim.rs:457-461`, variant at `crates/oce-api/src/error.rs:72-74`,
pinned by `host_epoch_is_required_and_exact_mapping_handles_signed_model_time` at
`crates/oce-api/src/tests/realtime_write_back_tests.rs:79`). The epoch-plus-offset mapping is
explicitly range-checked, so a non-finite or out-of-range instant fails with
`OcError::RealtimeInstantUnrepresentable` rather than clamping, wrapping or panicking
(`crates/oce-api/src/sim.rs:265-279`).

Supply time from a source you trust to be monotonic. The engine cannot detect a clock that jumped.

Do not interleave horizon simulation with real-time stepping if you rely on the monotonic-time
guard across that boundary. After preflight succeeds, `simulate` deliberately clears the prior tick
time, so a following `step_realtime` cannot detect regression relative to a real-time step that
happened before the simulation.

`simulate` is a run restart, not a continuation. Before restarting, it resolves recorded columns,
fixed inputs, and the first list returned by an input closure. A refusal there leaves the prior run
unchanged, including its monotonic-time guard and state words. After preflight succeeds, the engine
clears the prior tick time and re-seeds stateful blocks to their authored start values. It leaves
connector values alone, so it is narrower than the `resume` re-seed described below, which replaces
the whole run state and only when parameters are dirty.

An input closure remains dynamic after the first tick. If a later call returns an unknown point or
a wrong-typed value, completed ticks stay in effect and any valid pairs before the failing pair stay
staged. A simulation is not transactional after execution begins.

Store-backed inputs are staged inside each tick, not during simulation preflight. A snapshot error
or wrong-typed store sample on the first tick therefore returns after the run clock and state words
have reset, even though no block evaluated. Model time and the output snapshot still describe the
prior run, while any store inputs staged before the failing sample remain in the connector image.

Two consequences to plan for. Splitting a horizon across two calls does not continue the
trajectory: simulating `0..10` then `11..20` is not the same as simulating `0..20`, because the
second call restarts from the seed. And a what-if interleaved into a live run resets that engine's
stateful blocks, which for held and sampled values means a jump rather than an advance —
snapshot-and-restore is the surface that would make this safe, and it does not exist yet
([#143](https://github.com/jscott3201/open-control-engine/issues/143)).

Connector values that `simulate` does not overwrite carry into the horizon: `InputSource` writes
the slots it names on every step, and the rest hold whatever was there. Whether a value staged
through `set_input` reaches a given block depends on how that input is fed — a store-bound point is
re-staged from the snapshot on any tick the snapshot has a sample for, and an input driven by
another block inside the model is read from its driver rather than from its own slot.

## Lifecycle names are not equipment controls

`Engine::halt()` does not stop ticks, real-time steps, simulations, or output writes. It changes
only the parameter-edit permission mode: `set_param` is accepted while halted. The host must stop
calling execution methods if it intends execution to stop.

A `halt` / `set_param` / `resume` cycle is also a run restart, not live tuning. When parameters are
dirty, `resume` rebuilds blocks, allocates all state again, refreshes outputs, and clears the prior
model time. Every stateful block—including integrators, latches, timers, and filters—is re-seeded,
and monotonic-time history is lost. Plan parameter edits as a new run.

The stable API also contains two loaders that do not work yet: `load_from_semantic` and
`load_modelica` always return `OcError::Load`. Use `load_cxf` for working ingest. Likewise, the
public `AssertLevel::Error` variant is never emitted today; the sole assertion collector produces
`Warning`, so hosts must not depend on receiving `Error` for escalation.

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
merged in order with later bindings winning; a remote context reference, `@base`, `@vocab`, and
prefix bindings that are not absolute IRIs are refused at load as non-subset constructs rather
than silently ignored, so the canonical-key guarantee holds for every document that loads at
all.

The document's declared boundary-output names (root `S231:hasOutput`) are a second read-only
identity space: each resolves on `get_output`, `watch`, and `CollectSpec::Named` as an alias for
its driving internal connector's slot, and `Topology.boundary_outputs` enumerates the
`(path, driver_path)` pairs. Declared names stay out of `point_list`, `to_map`, `IoSummary`,
and the durable store batch — a declared name and its driver are two keys over one value, and
only the driver's path carries samples. `set_input` never accepts a declared output name. An
undriven declared output resolves nowhere; its load-time `undriven-boundary-output` warning is
its only representation.

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

## The one hardening gap

Stated plainly, because the alternative is that you assume it is handled.

| Bound | Limit | Defined at | Behavior when exceeded |
| --- | --- | --- | --- |
| Expression parse and AST nesting | 64 | `crates/oce-expr/src/lib.rs:125` | typed `NestingTooDeep` error |
| Expression size | 4096 nodes | `crates/oce-expr/src/lib.rs:132` | typed `ExpressionTooLarge` error |
| Composite **nesting** (`containsBlock` lowering) | 64 | `crates/oce-cxf/src/resolve/composite.rs:22`, checked at `:231` | `MalformedDocument` diagnostic |
| Composite **boundary resolution** (`isConnectedTo` hops) | **none** | `crates/oce-cxf/src/resolve/composite.rs:427-474` | **unbounded recursion** |

The last row is the gap, and the distinction between the last two rows is the part to get right.
Composite *nesting* — how deeply composites contain other composites — is bounded at 64 and rejects
cleanly. Composite *boundary resolution* is a different walk: `resolve_target` and `follow_boundary`
are mutually recursive and take one stack frame per `isConnectedTo` hop, with no depth counter. A
`seen` set (`composite.rs:459-464`) catches authored cycles and routes them to the ordinary
dangling-reference diagnostic, so a cycle terminates. A long *acyclic* chain of boundary connectors
does not: it recurses once per hop until the stack is exhausted.

The gap is in `oce-cxf`, not `oce-expr`. The expression bounds are real and typed; they do not cover
this. The repo states the gap here, in `architecture.md`, and in `../TESTING.md:40-43` rather than
leaving it to be discovered.

## Treat untrusted CXF as untrusted input

A CXF document is a program. Loading one from a source you do not control is running code you did
not write, through a resolver with one known unbounded recursion. If you must:

- Bound document size and connector-chain length before handing bytes to the loader.
- Load in a process or thread whose loss you can absorb, with a stack you have sized deliberately.
- Never load an untrusted document in the same process that is actively commanding equipment.

The rest of the ingest path is bounded and returns typed diagnostics rather than panicking, and
`../TESTING.md` requires new ingest code to assert the specific `DiagCode` or error variant rather
than "an error occurred". That standard is why this one gap is written down instead of assumed away.

One more thing worth knowing: the tests cited on this page live in `oce-api`, and the per-PR gate
does not run `oce-api`'s tests. They execute on the release gate. See
[`ci-and-the-gate.md`](ci-and-the-gate.md) for what a green check actually covers.
