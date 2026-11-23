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

## Events on a Boolean input

A block whose CDL source has `when trigger then ...` (or `reinit`) on a Boolean input applies the
event at the frame of the event (issue #768; design note
[Reset timing at the event](design/reset-timing.md)):

| Boundary | HostTick v1 behavior |
| --- | --- |
| Event instant | Frame `k > 0` is the event instant of every Boolean event condition true at frame `k` and false at frame `k - 1`. |
| Event frame's outputs | The values after the event, computed from frame `k`'s inputs: the frame emits from the reinitialised or re-assigned state. |
| Integrators | The reset frame's update stores the reset value with no Euler step (that step covers `[t(k-1), t(k)]`, before the event); from frame `k + 1` on, the state is what the engine stored before this rule. |
| Frame 0 | Initialisation: no `when` fires unless its condition holds `initial()`. An event input true at frame 0 becomes its `pre` value for frame 1, so the first event is its next false-to-true transition. This holds for every level, and for every input with no pulse or sample upstream; an input fed by a pulse or a sample follows the initial-frame rule of [Pulses on an event input](#pulses-on-an-event-input), under which a level still never fires at frame 0 (issue #800, decision 3 as revised). |
| Feedthrough graph | An input the event reads at emission feeds through to the output, so the block waits for its producer, and feedback from the output into it is an `algebraic-loop`. |

The blocks this concerns are `Reals.PIDWithReset` (PI and PID modes: `trigger` feeds through; the
reset back-solves the integrator to `y_reset - (P + D)` from the frame's terms, and the frame emits
through the normal path, `P + D + (y_reset - (P + D))`, which can differ from `y_reset` by an ulp),
`Reals.IntegratorWithReset` (`trigger` and `y_reset_in` feed through; `u` stays a loop cut),
`Integers.OnCounter` (`trigger` and `reset` feed through), and `Discrete.TriggeredSampler` and
`Discrete.TriggeredMax` (no event at frame 0; both already fed through). `PIDWithReset` in the P
and PD modes has no integrator, ignores `trigger` and does not feed it through. `Logical.Change`, `Edge` and
`FallingEdge` keep their initialisation value at frame 0, true when `u` differs from
`pre_u_start`; a downstream `when` ignores that pulse either way. Every other class with a `when`
in its source already matched this rule.

**Checked against OpenModelica** (issues #670 and #787). OpenModelica 1.25.1 ran each of the five
classes from a pilot model at 1 s frames with held inputs, once with the trigger true from the
initial instant and once with it rising at frame 1: at every frame of the ten scenarios its output
equals the engine's, exactly (`crates/oce-conformance/tests/open_modelica_event_triggers.rs`; the
integrators with their integral held, `u = 0` and `Ti = 1e20` s). It fires no `when` at the initial
instant and shows each event at its own frame, and an event at frame 0 or an event shown one frame
late differs from it in every class. With a nonzero input, at a uniform frame step with inputs held
between frames, `IntegratorWithReset` trails OpenModelica by exactly one frame of the input held at
the last reset or at the start (`open_modelica_integrator_ramp.rs`), and the reset frame's skipped
Euler step is what keeps that lag the same after a reset as after the start. At uneven steps the
difference after the reset (or start) at frame `r` is `k * [sum over j of u(j) * (dt(j) - dt(j+1)) -
u(r) * dt(r+1)]`, the sum over the frames after `r`. Upstream's `CoolingCoilValve` on the 78
synthetic Building 33 frames resets its integrator at all nine changes of the enable, the engine at
eight; at each engine reset they agree to 1e-12
(`crates/leverage/oce-verify/tests/commissioning/building_33_openmodelica_frames.rs`). The ninth is
the limit below.

Modelica settles feedback into an event input by event iteration, which HostTick v1 does not run.
The engine refuses such a model with `algebraic-loop`, and the record of the event input names the
remedy: a `Logical.Pre` on the path into a Boolean event input, or a `Discrete.UnitDelay` on the
path into `IntegratorWithReset.y_reset_in` (a delay of exactly one frame only when its
`samplePeriod` equals the host frame period and frames land on its sample grid; CDL has no other
Real one-frame delay). The delay makes the model load and moves that input's events one frame
later. It does not reproduce the engine before issue #768, which applied each event one frame late
itself. The exact property, pinned by `crates/oce-api/tests/event_input_feedback.rs`: with every
event input of the block delayed and none true at frame 0, `OnCounter` matches the previous
engine count for count; the two integrating blocks match it in the frame of each reset (and, under
steady inputs, in every output up to that frame) but still differ in the reset value, which is
taken from the inputs of the frame the delayed event fires at, and by the integration step across
the event, which the reset frame does not take. An event input true at frame 0 is the exception:
the previous engine ignored it, and behind a `Pre` (`pre_u_start = false`) it is an event at
frame 1.

Because `PIDWithReset.trigger` feeds through only with an integrator, its `controllerType` decides
the block's schedule shape, and `resume` does not reschedule: `Engine::set_param` refuses, with
`ParamStructural`, a value that (with the block's other pending edits) changes the scheduled
instance's feedthrough, the inputs it reads when it fires or an input's event role. That refuses
a switch between {P, PD} and {PI, PID}, and between P and PD; PI and PID stay interchangeable.
P↔PD is refused even on a model without feedback into `trigger`, where it would be harmless: P
reads `trigger` when it fires and PD never does, so the two schedules differ, and one rule judged
on the block alone keeps a halted edit consistent with what a fresh load of the edited model
would schedule.

`TriggeredSampler` seeds its previous-trigger word `true` in `init_state`, so no false-to-true
transition can be seen at frame 0, whatever `trigger` is there; the frame-0 update stores frame
0's `trigger`. The word stays a canonical Boolean in every state, and `state_len` is unchanged.

## Pulses on an event input

A pulse that Modelica holds true for part of one instant (`Logical.Change`, `Integers.Change`,
`Edge`, `FallingEdge`, `Logical.Sources.SampleTrigger`, and logic over them) is true for a whole
frame here, and a frame can follow another where it was true. The settled-value rule (issue #800;
design note [Event inputs fed by one-frame pulses](design/pulse-event-inputs.md), option (g),
ruled by the owner on 2026-11-22) decides when an event input fires on such a signal:

| Boundary | HostTick v1 behavior |
| --- | --- |
| Cone | The stateless Boolean logic upstream of an event input (`And`, `Or`, `Not`, `Nand`, `Nor`, `Xor`, `MultiAnd`, `MultiOr`, `Logical.Switch`, the Boolean routing classes), classified at load, from the wiring, down to its leaves: a *sample* (a `SampleTrigger` output), a *primitive pulse* (a `Change`, `Integers.Change`, `Edge` or `FallingEdge` that nothing upstream of it, through any chain of blocks, connects to a pulse, a sample or a `Pre`), a *fallback leaf* (a `Pre`, any other edge block, or any other block such a chain reaches) and a *level* (everything else). |
| Three values | `v`, the frame value; `w`, the cone with every pulse and sample false; `s`, the cone with every pulse false and every sample at its frame value. They are what Modelica's event iteration at the frame's instant takes: before, during, settled. |
| Event | At a frame after the first, the input fires iff `(v(k) and not w(k-1)) or (not v(k) and s(k))`; the block stores `w(k)` where it stored the previous trigger. A direct pulse or sample fires on every frame it is true, `Not` of a pulse fires at the pulse's frame, and `Not` of a sample never fires, as in Modelica. Where no pulse or sample is upstream, `w = s = v` and the rule is the rising edge above, bit for bit. |
| Fallback | A cone with a fallback leaf keeps the rising edge: a `Pre` settles in Modelica's second event iteration and a frame later here, and an edge block of a pulse can change more than once in one instant. The `Pre` remedy for feedback into an event input is such a cone. A block with two event inputs (`OnCounter`, `Latch`, `Toggle`) keeps the rising edge on both when either input's cone falls back, frame 0 included. So does every event input of a block whose *data* inputs (`TriggeredSampler.u`, `TriggeredMax.u`, `TriggeredMovingMean.u`, `IntegratorWithReset.u` and `.y_reset_in`, `PIDWithReset.u_s` and `.u_m`, `TimerAccumulating.u`) a pulse, a sample or a `Pre` reaches through any chain of blocks: data cones are not traced, and such an input can read another value in Modelica's second iteration (a converted pulse, already false) or another block's second-iteration event. |
| Frame 0 | A fourth value, `i`: the cone at initialisation, every pulse at its frame value (the initialisation pulse of a `Change` whose input starts different from `pre_u_start`) and every sample false. At the initial frame the input goes `i -> w -> s` and fires on `(w and not i) or (s and not w)`, at most once: a sample instant at the start fires, and so does a negated initialisation pulse (`Not(Change(x))` with `x(0) = true`); a pulse, a level and a fallback cone never do (issue #768's "no event at frame 0 for a trigger already true" holds for every level). The body of the step reads that step's values (`OnCounter`'s `reset`). `TriggeredMovingMean` samples once for `initial()` and once more when the input fires there. `Latch` and `Toggle` run their initial equations on `i`, then their `when` on each step. This is decision 3 as revised by the owner on 2026-11-22 after two pinned-class probes (`InitNegatedPilot`, `InitMixturesPilot`). |

The consumers are `OnCounter.trigger` and `.reset`, `TriggeredSampler`, `TriggeredMax` and
`TriggeredMovingMean` `.trigger`, `IntegratorWithReset.trigger`, `PIDWithReset.trigger` (PI and
PID), `Latch.u` and `.clr`, `Toggle.u` and `.clr`, and `TimerAccumulating.reset`. A block with two
of them runs its `when` once per step that fires, with that step's values: `OnCounter` takes
`reset`'s value in the iteration that fired, so a trigger in the first iteration and a reset rising
in the second count once and then reset; `Latch` (`when {clr, u} then y = not clr and u`) and
`Toggle` (its `when`/`elsewhen` chain) clear in the first step and set in the second when `clr`
is a pulse and `u` rises after it, as Modelica does. `clr`'s value before a frame is not stored
(the state layout is unchanged): `clr` true at a frame is read as rising, which is exact unless
`clr`'s cone holds a negated sample (true between instants, false at one), where a latch set at an
instant is cleared at the next frame and Modelica keeps it (`DVG-000008`).

`SampleTrigger` emits `true` at frame 0 only when the start is one of its instants (within
`1e-9 · period`, the tolerance of its index; Modelica rounds its first instant to 1e-6 s). A frame
that spans several of its instants fires once; the first time that happens it warns, once per
instance: the state word records it (an unwarned word is the index, `-1` before the first
instant; a warned one `-(index + 2)`, for an index of at least 1), so a snapshot carries it and a
`resume` (which re-seeds state) re-arms it.

**What a host must keep.** At most one instant of a `SampleTrigger` per frame interval (a frame
step at or below its `period`), and at most one change of a `Change`'s input per frame interval,
which frame sampling gives anyway. Everything inside a frame interval is shown at the frame's
time with the frame's values: an instant between frames is sampled with the frame's inputs, not
the instant's (a `TriggeredSampler` behind `SampleTrigger(1.4 s)` at 1 s frames samples `u` at
2 s for the instant at 1.4 s).

The plan costs nothing where no event input has a pulse or sample upstream. Elsewhere load adds
one linear pass over the wiring (and the blob decoder repeats it: no blob byte carries it), and
each frame evaluates every logic block of an active cone twice more, once per block however many
consumers share it, into buffers sized at load.

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
OCE carries no build token. A snapshot names the engine version that wrote it and restores only on
that version, which is not a build identity. State compatibility does not authenticate a build or
authorize actuation.

## Conformance boundary

**Pulses on an event input follow the settled-value rule above**, checked against OpenModelica
1.25.1 running the pinned Buildings classes
(`crates/oce-conformance/tests/open_modelica_pulse_events.rs`, nine pilot runs): every consumer class
behind `Change` of a signal that changes on consecutive frames, `Not(Change)`, `Or(Change, level)`,
`SampleTrigger` at 0.5, 1, 1.4 and 2 s from an instant at the start and from an off-grid start,
`Not(SampleTrigger)`, G36 `TrimAndRespond`'s hold latch (`Latch` with `u` a sample and `clr`
`And(Not(hold), sample)`), `Latch` and `Toggle` whose `clr` is `Change` of the same or another
signal or `Not(Edge)`, every consumer at frame 0 behind a negated initialisation pulse
(`Not(Change(x))`, `Not(Edge(x))`) and behind `Not(SampleTrigger)` on and off the grid, mixed cones
at frame 0 (`Or`, `And`, `Xor` of an initialisation pulse and a sample, a three-leaf mixture, a
level), and `Latch` and `Toggle` reading the initialisation pulse, equal OpenModelica
at every frame (bit for bit; a timer within 1e-6 s and a mean within 1e-12, OpenModelica's printed
precision). Building 33's re-enable one frame after a disable resets as OpenModelica does
(`building_33_openmodelica_reenable.rs`). The two divergences registered before the rule,
`DVG-000001` and `DVG-000002`, are resolved. Ten remain, registered
(`crates/oce-conformance/tests/fixtures/known_divergence/register.json`) and documented here as
the owner ruled (decision 2); in each the engine gives what it gave before this rule or differs
from OpenModelica at fewer frames, but for the single `DVG-000008` frames below and
`DVG-000006`'s frame 1:

- `DVG-000003`, `DVG-000006`: a cone with a `Pre` keeps the rising edge. `OnCounter` behind
  `Or(Change(u), Pre(b))` counts one event where OpenModelica counts three; behind
  `Pre(SampleTrigger)`, one where OpenModelica counts one per instant.
- `DVG-000004`: several instants of a `SampleTrigger` in one frame fire once (0.5 s at 1 s frames:
  10 counts against 19), with the once-only warning above.
- `DVG-000005`, `DVG-000011`: an event between frames is shown at the next frame with that
  frame's values (`SampleTrigger(1.4 s)` at 1 s frames samples `u(2 s)` for the instant at
  1.4 s; a `TimerAccumulating` reset by a change at 3 s on frames at 0.25 + k s restarts at 3.25 s).
- `DVG-000007`: a fallback cone at frame 0 (`Or(Not(Change(x1)), Pre(false))`) never fires there;
  OpenModelica's initialisation pulse does.
- `DVG-000008`: `Latch` and `Toggle` whose `clr` holds a negated sample (above). For `Toggle`
  single frames can differ where the engine before this rule matched OpenModelica (never a whole
  column, never for `Latch`).
- `DVG-000012`: a `Pre` on one event input of a two-input consumer: the block keeps the rising
  edge on both, and the `Pre` shows its value a frame after OpenModelica's `pre` settles.
- `DVG-000009`: a data input carrying a converted pulse (`BooleanToReal(Change(a))` into
  `TriggeredSampler.u`, `IntegratorWithReset.y_reset_in` or `PIDWithReset.u_m`, the trigger
  `Not(Change(a))`) puts the block on the rising edge, a frame after OpenModelica's second
  iteration, which reads the pulse already false.
- `DVG-000010`: OpenModelica reinitialises `IntegratorWithReset` and `PIDWithReset` when a negated
  sample's `sample()` falls after its instant, in continuous time, though no when-only class
  behind the same signal fires there; the engine fires none of them.

Not compared, and not matched by any event rule: a pulse read for its duration (`TrueDelay`,
`Timer`, `TrueFalseHold`, `TrueHoldWithReset`, a `MovingAverage` of a converted pulse) lasts a
frame here and no time in Modelica; and events at different times inside one frame are treated as
simultaneous, so `And(Change(a), b)` with `b` rising after `a` changes inside the frame fires here
and not in Modelica. No host cadence removes that, only frames short enough that no two sources
change inside one.

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
