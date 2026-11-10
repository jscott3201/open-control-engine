# Execution profile

The [generated authority summary](authority-claims.md) distinguishes compiled state revisions from
the review-only profile name and semantics; it does not define a new profile identity.

Open Control Engine currently has one execution profile: **HostTick v1**. It is fixed, not selected
through an API option. A future profile with different state-transition semantics would require a
separate compatibility and snapshot contract.

The [complete-frame contract](complete-frame-contract.md) implements read-only preparation and
one native atomic transition with an immutable output/diagnostic result around this same profile,
not a second evaluator or profile selector. Preparation does not execute; consuming `execute_frame`
and its slot-indexed twin `execute_slots` (issue #444), which runs the same core driver, are the
only public state-advancing execution surfaces. Legacy execution profiles were removed.

## One frame driver, two callers

HostTick v1 is implemented once, in the core: `oce_graph::frame` (design note
[decision 11](design/compiled-schedule-blob.md#decisions), GitHub #401). Its `FrameDriver` checks
the time (finite, `|t| ≤ MAX_FRAME_TIME_MAGNITUDE`, not before the previous accepted time, a finite
step, representable by every stateful block), then one value per input slot and, slot by slot,
the exact type, the `NonFiniteInputPolicy` and the declared domain, then sequence capacity; every
refusal precedes any mutation. It then stages each value into every fan-out target, evaluates the
frozen schedule once, sets the previous time and advances the sequence. It is slot-indexed and
name-free, selects no diagnostic policy (the caller passes the sink) and performs no storage IO,
restart or projection.

Two callers share it:

| Caller | Build | Adds |
| --- | --- | --- |
| `oce_api::Engine::prepare_frame` / `execute_frame` | `std` | Name resolution and the unknown, duplicate and missing-key refusals, the prepared plan and its incarnation token, load and pending-edit preconditions, the durable-restore window, replay records, `String` warnings and the owned `CompletedFrame` |
| `oce_api::Engine::frame_slots` / `execute_slots` | `std` | The same facade, by slot: a `FrameSlots` map resolved once and bound to the incarnation token, the same preconditions and restore window, `FrameExtraInputs` for an over-long vector, reused warning records and a borrowed `SlotFrame` ([slot-indexed execution](complete-frame-contract.md#current-slot-indexed-execution-api)) |
| `oce_graph::blob::Executable::execute_frame` | `no_std + alloc` | Inputs and outputs by slot from the [compiled-schedule blob](compiled-schedule-blob.md#executing-a-blob)'s I/O table, and the state as raw words |

`crates/oce-api/src/tests/blob_dual_path_tests.rs` runs every G36 fixture's frames, and the same
bad frames, through the facade's named and slot entries, the core driver on the host's own
executable and the blob entry,
and requires identical outputs, refusals and run state after every frame;
`crates/oce-api/tests/frame_driver_ownership.rs` checks that no facade source holds a second copy
of the rules. Host loops select cadence and capture traces from completed receipts or explicitly
labelled latest-state inspections.

## HostTick v1

Each successful `Engine::execute_frame(prepared)` call is one state transition:

1. Preparation snapshots every required typed boundary input and finite nondecreasing model time
   in seconds. Execution rechecks readiness, incarnation, time and sequence capacity before staging.
2. The engine evaluates the frozen schedule once. Algebraic blocks compute from current inputs;
   stateful blocks compute from call-entry state and any feedthrough inputs their contracts use.
3. After all emissions, every stateful block updates once from current-call inputs.
4. The engine refreshes latest-state inspection and returns immutable `CompletedFrame` boundary
   outputs and warnings, correlated by an engine-lifetime accepted-frame sequence.

Step 2 fires each block atomically, so one emission produces all of a block's outputs. The
schedule is therefore an order of whole blocks. A model whose feedback leaves one output of a block
and comes back into an input that output does not depend on is CDL-legal, yet it has no such order.
The engine refuses it at load with `block-algebraic-loop`. It never runs such a model in a wrong
order. See
[Atomic block firing refuses some CDL-legal feedback](cdl-coverage.md#atomic-block-firing-refuses-some-cdl-legal-feedback)
for which classes can trigger it and the remedies.

Repeating a timestamp does not repeat an observation of the same transition. Every successful call
advances state again. Time-dependent blocks see zero elapsed time, but call-based state still
changes. The engine performs no hidden same-time evaluation, event queue processing, rollback, or
fixed-point search.

Ordinary refusal preserves the full execution image, including connector values, clocks, words,
restore readiness and sequence. Previously retained completed diagnostics are unchanged. There
is no sparse staging prefix, stored determinant or post-write error path.

## `CDL.Logical.Pre`

The upstream Buildings [`Pre` block](../third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Logical/Pre.mo)
defines `y = pre(u)` as a delay of one Modelica event iteration without advancing time. Event
iteration continues until `u == pre(u)`. HostTick v1 deliberately uses a different projection:

| Boundary | HostTick v1 behavior |
| --- | --- |
| State allocation | One Boolean memory word is seeded from `pre_u_start`. |
| Before the first tick | The output connector has its Boolean connector seed, `false`; allocating state does not execute the block. |
| First successful tick | `Pre` emits `pre_u_start`, then latches current `u`. |
| Later successful ticks | `Pre` emits the `u` latched by the preceding call, then latches current `u`. |
| Repeated `t_now` | Each call advances the memory once, even though model time is unchanged. |
| Feedthrough graph | `Pre` reports no direct feedthrough and cuts a scheduling cycle. |

Schedule acceptance is based on direct-feedthrough shape, not event convergence. A Boolean loop such
as `Pre -> Not -> Pre` is accepted and alternates once per tick call. That network has no same-time
Boolean fixed point, but HostTick v1 neither rejects it nor emits a non-convergence diagnostic.

## Host observation

`get_output` and `watch` are latest-state, non-receipt inspections. Load, restore and parameter
resume can replace their state without a frame. They may read internal connector points that are
not boundary outputs. There are no intermediate event-iteration rows or raw output-arena accessor.

`CompletedFrame` retains accepted inputs and executable root boundary outputs in lexical identity
order, plus the Warning diagnostics emitted by that native transition. It is independent of the mutable latest
view. Its engine-lifetime sequence increases only on accepted frames, never on refusal,
and survives reload/resume/restore without rewinding. The private context fence
is not serialized and is not host deployment authority. See the frame contract for preflight precedence.

The separate [canonical replay record](replay-record.md) captures this receipt without reevaluation.
Exact replay compares raw values and diagnostic fields, not tolerances. Hosts authenticate order,
build/executable context and optional state sidecars, then drive the same prepare/execute surface.
No sequence container, embedded snapshot, profile selector or second evaluator is introduced.

Do not drive event iteration by repeatedly executing frames with the same timestamp unless repeated
HostTick state transitions are the intended behavior. Those calls also update every other stateful
block, not only `Pre`.

## Snapshots

A state snapshot stores both sides of the call boundary: connector values contain the currently
visible `Pre` output, and the block's Boolean state word contains the value to emit on the next
successful call. Capture and restore do not evaluate the model. After restore, a call at the restored
timestamp is a new HostTick transition.

HostTick v1 is part of execution-state ABI revision 1 even though the profile name is not a separate
wire field. A future same-time event-iteration profile must carry profile identity in the manifest
and codec. It must not consume HostTick v1 snapshots as semantically equivalent state.

The state manifest holds the exact executable IO acceptance domain. The format and ABI revisions
both stay 1 until 1.0.0; see the [state compatibility contract](state-compatibility.md).
Build/deployment authentication is a mandatory host-envelope precondition before byte decoding;
OCE carries no build token. State compatibility does not authenticate a build or authorize actuation.

## Conformance boundary

Open Control Engine supports the `CDL.Logical.Pre` interface and the HostTick v1 behavior above. It
does **not** claim exact Modelica or OpenModelica same-time event-iteration equivalence for `Pre`, or
for a network whose result depends on that iteration. `Pre` is therefore excluded from expected-green
OpenModelica differential claims under this profile.

The boundary is pinned through the public facade in
`crates/oce-api/src/tests/pre_execution_profile_tests.rs`. The tests cover initialization, repeated
equal-time calls, all host output views, a non-convergent Boolean feedback loop, and snapshot/restore
continuation.

The Tier-A references for `Generic.TimeSuppression`, `CoolingOnly.Controller`, and
`ReliefFanGroup` likewise check HostTick v1. Their 20 signal records are independent of
`oce-blocks`, but they do not claim Modelica `Pre` event-iteration equivalence. Two standalone
Tier-A goldens under `tools/golden-gen/goldens/CDL/Logical/Pre/` pin the block alone under the same
profile: a default `pre_u_start=false` trace whose input is already true on the first tick, and a
`pre_u_start=true` trace whose input is false on the first tick, each followed by holds and
single-tick toggles. `crates/oce-conformance/tests/per_block_logical.rs` compares the engine with
both exactly. They carry `execution_profile: "HostTick v1"` and are not Modelica event-iteration
oracles either.
