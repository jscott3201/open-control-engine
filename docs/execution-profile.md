# Execution profile

Open Control Engine currently has one execution profile: **HostTick v1**. It is fixed, not selected
through an API option. A future profile with different state-transition semantics would require a
separate compatibility and snapshot contract.

## HostTick v1

Each successful `Engine::tick(t_now)` call is one state transition:

1. The caller supplies a finite, monotonic non-decreasing `t_now` in seconds. Any explicit or
   store-backed inputs for the call are staged before block evaluation.
2. The engine evaluates the frozen schedule once. Algebraic blocks compute from current inputs;
   stateful blocks compute from call-entry state and any feedthrough inputs their contracts use.
3. After all emissions, every stateful block updates once from current-call inputs.
4. The engine refreshes the output snapshot returned by `tick` and read by `outputs`, `get_output`,
   and `watch`.

Repeating a timestamp does not repeat an observation of the same transition. Every successful call
advances state again. Time-dependent blocks see zero elapsed time, but call-based state still
changes. The engine performs no hidden same-time evaluation, event queue processing, rollback, or
fixed-point search.

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

The `Outputs` returned by `tick`, and later reads through `outputs`, `get_output`, or `watch`, expose
only the completed tick call. There are no intermediate event-iteration rows. A later call at the
same timestamp replaces the visible output snapshot. `step_realtime` likewise evaluates one
HostTick transition before attempting its output write; a write failure does not roll that
transition back. It publishes no internal iterations because none occur.

Do not drive event iteration by repeatedly calling `tick` with the same timestamp unless repeated
HostTick state transitions are the intended behavior. Those calls also update every other stateful
block, not only `Pre`.

## Snapshots

A state snapshot stores both sides of the call boundary: connector values contain the currently
visible `Pre` output, and the block's Boolean state word contains the value to emit on the next
successful call. Capture and restore do not evaluate the model. After restore, a call at the restored
timestamp is a new HostTick transition.

HostTick v1 is part of execution-state ABI revision 1 even though the profile name is not a separate
wire field. A future same-time event-iteration profile must use a distinct execution-state ABI
revision or a newly revised manifest and codec with profile identity. It must not consume HostTick v1
snapshots as semantically equivalent state.

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
