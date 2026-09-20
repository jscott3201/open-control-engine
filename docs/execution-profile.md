# Execution profile

The [generated authority summary](authority-claims.md) distinguishes compiled state revisions from
the review-only profile name and semantics; it does not define a new profile identity.

Open Control Engine currently has one execution profile: **HostTick v1**. It is fixed, not selected
through an API option. A future profile with different state-transition semantics would require a
separate compatibility and snapshot contract.

The [complete-frame contract](complete-frame-contract.md) implements read-only preparation and
one native atomic transition with an immutable output/diagnostic result around this same profile,
not a second evaluator or profile selector. Preparation does not execute; consuming `execute_frame`
is the only public state-advancing execution surface. Legacy execution profiles were removed.

The frame path enters one private infallible evaluation/refresh core after preflight and staging.
The core closes durable-restore readiness, evaluates once, updates
the previous time and refreshes latest outputs. It selects no diagnostic policy and performs no
Store IO, restart, frame-sequence update or projection. Host loops select cadence and capture traces
from completed receipts or explicitly labelled latest-state inspections.

## HostTick v1

Each successful `Engine::execute_frame(prepared)` call is one state transition:

1. Preparation snapshots every required typed boundary input and finite nondecreasing model time
   in seconds. Execution rechecks readiness, incarnation, time and sequence capacity before staging.
2. The engine evaluates the frozen schedule once. Algebraic blocks compute from current inputs;
   stateful blocks compute from call-entry state and any feedthrough inputs their contracts use.
3. After all emissions, every stateful block updates once from current-call inputs.
4. The engine refreshes latest-state inspection and returns immutable `CompletedFrame` boundary
   outputs and warnings, correlated by an engine-lifetime accepted-frame sequence.

Repeating a timestamp does not repeat an observation of the same transition. Every successful call
advances state again. Time-dependent blocks see zero elapsed time, but call-based state still
changes. The engine performs no hidden same-time evaluation, event queue processing, rollback, or
fixed-point search.

Ordinary refusal preserves the full execution image, including connector values, clocks, words,
restore readiness and sequence. Previously retained completed diagnostics are unchanged. There
is no sparse staging prefix, Store determinant or post-write error path.

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

`CompletedFrame` retains only the executable root boundary outputs in lexical identity order and
the Warning diagnostics emitted by that native transition. It is independent of the mutable latest
view. Its engine-lifetime sequence increases only on accepted frames, never on refusal,
and survives reload/resume/restore without rewinding. The private context fence
is not serialized and is not host deployment authority. See the frame contract for preflight precedence.

Do not drive event iteration by repeatedly executing frames with the same timestamp unless repeated
HostTick state transitions are the intended behavior. Those calls also update every other stateful
block, not only `Pre`.

## Snapshots

A state snapshot stores both sides of the call boundary: connector values contain the currently
visible `Pre` output, and the block's Boolean state word contains the value to emit on the next
successful call. Capture and restore do not evaluate the model. After restore, a call at the restored
timestamp is a new HostTick transition.

HostTick v1 is part of execution-state ABI revision 2 even though the profile name is not a separate
wire field. A future same-time event-iteration profile must use a distinct execution-state ABI
revision or a newly revised manifest and codec with profile identity. It must not consume HostTick v1
snapshots as semantically equivalent state.

ABI revision 2 preserves HostTick v1 and adds the exact executable IO acceptance domain to the
state manifest, alongside format revision 2. See the [state compatibility contract](state-compatibility.md).
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
`oce-blocks`, but they do not claim Modelica `Pre` event-iteration equivalence.
