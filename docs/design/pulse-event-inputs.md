# Event inputs fed by one-frame pulses

**Design note, written before any code (issue #800).** The owner ruled on 2026-11-22: write a
design note first for an engine fix, "the pulse/level rule, or better if the note finds one";
nothing is built before the note is reviewed, and the two divergences stay pinned in the register
meanwhile. This note states what Modelica does with a pulse on an event input and what the engine
does, for every registered pulse producer and event consumer; measures the shapes against
OpenModelica and the engine; evaluates six options; recommends one; lists the
[decisions](#decisions) the owner is asked to rule on and the calls this note proposes to make
itself; and measures the blast radius. The [execution profile](../execution-profile.md) stays the
authority for shipped behaviour until an implementation PR changes it.

Paths below are repository-relative. `CDL/` abbreviates the vendored
`third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/`; `blocks/` abbreviates
`crates/oce-blocks/src/`. Engine line numbers are at `development` 128f7a8d.

## How the evidence was gathered

- **Sources.** The 39 vendored CDL classes with `when`, `edge(`, `change(`, `sample(` or `pre(`
  (the set [Reset timing at the event](reset-timing.md) read), G36 `Generic.TrimAndRespond`, and
  the engine implementation of each class with a Boolean event or a pulse output.
- **Modelica semantics.** Modelica Language Specification 3.6: a when-equation is "activated only
  at the instant when the scalar expression or any of the elements of the vector expression becomes
  true" (§8.3.5); `edge(b)` "expands into (b and not pre(b))", `change(v)` "expands into
  (v <> pre(v))" (§3.7.5); after each event iteration `pre(v)` takes the value of `v`, and
  iteration repeats until nothing changes (§8.6). `sample(start, interval)` is true only in the
  first event iteration at each instant `start + i * interval`. So a pulse is true for part of one
  instant and false before and after it.
- **OpenModelica.** Three scratch models (not committed) ran in the pinned OpenModelica 1.25.1
  image recorded by the GitHub #796 provenance files, one container at a time, no network, cvode at
  tolerance 1e-6, output every 1 s. They load no library: each CDL class involved is transcribed
  from the vendored source (`Change.mo:4-15`, `Edge.mo:4-17`, `Pre.mo:13-17`, `Latch.mo:14-19`,
  `OnCounter.mo:16-25` with `reset` false, `TriggeredSampler.mo:16-22`,
  `Sources/SampleTrigger.mo:4-30` with `Buildings.Utilities.Math.Functions.round` written out).
  Section 1.3 gives their results. Rerunning them against the pinned Buildings classes is owed by
  the implementation (section 5).
- **Engine probe.** A scratch example (not committed) loaded one CXF document holding every shape
  of section 1.3 through `oce_api::Engine::load_cxf` and ran frames at t = 0, 1, …, 9 s. A second
  one ran the `multizone_vav_freeze_protection` fixture over its 111 trace frames and watched the
  Edge pulse into its latch (section 4).
- **Static scan.** A scratch script (`python3 -I`, not committed) walked every JSON-LD graph in the
  tree (214 under `crates/`, 46 under `third_party/`) and in `open-control-library` at its
  `development` tip fe7060a (140), from each pulse producer's output through Boolean logic and
  composite boundaries to the first consuming input (section 4).

## 1. The semantics

### 1.1 Pulse producers

| Class | Modelica | Engine | True on two consecutive frames? |
| --- | --- | --- | --- |
| `Logical.Change` | `y = change(u)`, `u(fixed = true, start = pre_u_start)` (`Logical/Change.mo:6-15`) | `u ≠` the previous frame's `u` (`blocks/logical_latch.rs:112`) | Yes, when `u` changes on two consecutive frames |
| `Integers.Change` | `y = change(u)`, `up = u > pre(u)`, `down = u < pre(u)` (`Integers/Change.mo:6-23`) | the same comparisons with the previous frame's `u` (`blocks/integers_edge.rs:175-178`) | Yes, all three outputs |
| `Logical.Edge` | `y = edge(u)` (`Edge.mo:13-17`) | `u ∧ ¬` previous `u` (`blocks/logical.rs:434`) | No: true at `k` needs `u` false at `k − 1`, so it was false there |
| `Logical.FallingEdge` | `y = edge(not u)` (`FallingEdge.mo:14-21`) | previous `u ∧ ¬u` (`blocks/logical_latch.rs:61`) | No, by the same argument |
| `Logical.Sources.SampleTrigger` | `y = sample(t0, period)`, `t0` from the start time and `shift` (`Sources/SampleTrigger.mo:24-30`) | true on each frame that reaches a new index `floor((t − phase) / period + 1e-9)` (`blocks/logical.rs:502-559`) | Yes, whenever two adjacent frame intervals each hold an instant: always when the frame step is at or above `period`, sometimes when it is above `period / 2` (at 1 s frames and `period = 1.4` s it is true at 2, 3, 5, 6, 7 and 9 s), never at or below `period / 2`. A frame holding several instants is one `true` |

Two more sources of consecutive `true`, neither a class: pulses combined through `Or` or `And`
(`Or(Edge(a), Edge(b))` with `a` and `b` rising on consecutive frames), and pulses built by hand
from `Pre`. `Xor(u, Pre(u))` is a hand-built `Change` and can be true on consecutive frames;
`And(u, Not(Pre(u)))` is a hand-built `Edge` and cannot. The pulse-shaped levels are not pulses:
`Logical.Sources.Pulse`, `Timer.passed`, `TrueDelay.y` and friends hold a value for a duration in
Modelica too.

### 1.2 Event consumers

Every Boolean input that a `when` reads, with what the engine does with it:

| Input | Modelica | Engine | Kind |
| --- | --- | --- | --- |
| `Integers.OnCounter.trigger`, `.reset` | `when {trigger, reset}` (`Integers/OnCounter.mo:20-25`) | rising edge of each (`blocks/integers_edge.rs:65-67`) | event |
| `Discrete.TriggeredSampler.trigger` | `when trigger` (`Discrete/TriggeredSampler.mo:20-22`) | rising edge (`blocks/discrete.rs:203`) | event |
| `Discrete.TriggeredMax.trigger` | `when trigger` (`Discrete/TriggeredMax.mo:18-22`) | rising edge (`blocks/discrete.rs:282`) | event |
| `Discrete.TriggeredMovingMean.trigger` | `when {initial(), trigger}` (`Discrete/TriggeredMovingMean.mo:36-53`) | rising edge, or no sample yet (`blocks/discrete.rs:377`) | event |
| `Reals.IntegratorWithReset.trigger` | `when trigger then reinit(...)` (`Reals/IntegratorWithReset.mo:27-29`) | rising edge (`blocks/reals_integrator.rs:33`, stored at `:144`) | event |
| `Reals.PIDWithReset.trigger` (PI, PID) | its integrator's `trigger` | rising edge (`blocks/pid.rs:238`, stored at `:346`) | event |
| `Logical.Latch.u` | `when {clr, u} then y = not clr and u` (`Logical/Latch.mo:17-19`) | rising edge sets (`blocks/logical_latch.rs:132`) | event |
| `Logical.Toggle.u` | `change(u) and pre(u) == false`, that is `edge(u)` (`Logical/Toggle.mo:23-42`) | rising edge toggles (`blocks/logical_latch.rs:191`) | event |
| `Logical.Latch.clr`, `Logical.Toggle.clr` | `when {clr, …}` / `elsewhen clr`, and every other branch is guarded by `not clr` | read as a level: `¬clr ∧ …` | level-equivalent (below) |
| `Logical.Edge.u`, `FallingEdge.u`, `Change.u` | `pre(u)` | the previous frame's `u` | the producer's own memory |
| `Logical.Timer.u`, `TimerAccumulating.u` and `.reset`, `TrueDelay.u`, `TrueFalseHold.u`, `TrueHoldWithReset.u`, `Reals.Ramp.active` | `when u` together with `elsewhen not u`, timers and the level itself (`Timer.mo:31-49`, `TimerAccumulating.mo:42-65`, `TrueDelay.mo:34-46`, `TrueFalseHold.mo:48-61`, `Ramp.mo:43-52`; `TrueHoldWithReset` has no vendored source) | edges and the level | duration consumer |

`Latch.clr` and `Toggle.clr` give the same output whether read as events or as a level: at every
event of either input a true `clr` forces `y` false, a rising `clr` is such an event, and while
`clr` is false nothing reads it, so a level reading of a pulse or of a level `clr` reproduces
Modelica. Duration consumers read how long the input stays true; a Modelica pulse lasts no time,
so `TrueDelay(delayTime > 0)` of a pulse is always false in Modelica and can be true in the engine.
No event rule repairs that (section 6).

### 1.3 The shapes, measured

`u` changes at 2, 3, 5, 7, 8 and 9 s (the `OnCounterChangePilot` pattern), `lvl` is true on
[4, 8) s and `lvl2` from 3.5 s, frames are 1 s apart, and each shape drives a counter (`OnCounter`,
no reset). The table gives the count after the 9 s frame. **OM** is the OpenModelica run.
**Today** is the engine probe. **(a)**, **(c)** and **(f)** are the options of section 2, derived by
hand from the engine's frame values (not run); "late" means each event lands one frame after
Modelica's.

| # | Shape into the counter | OM | Today | (a) | (c) | (f) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | `Change(u)` | 6 | 3 | 6 | 6 | 6 |
| 2 | `Not(Change(u))` | 6 | 2, late | 2, late | 2, late | 6 |
| 3 | `And(Change(u), lvl)` | 2 | 2 | 2 | 2 | 2 |
| 4 | `Or(Change(u), lvl)` | 4 | 1 | 1 | 1 | 4 |
| 5 | `Or(Change(u), Change'(u))` | 6 | 3 | 6 | 3 | 6 |
| 6 | `Xor(u, Pre(u))` | 6 | 3 | 3 | 3 | 3 |
| 7 | `And(u, Not(Pre(u)))` | 3 | 3 | 3 | 3 | 3 |
| 8 | `Edge(Not(Change(u)))` | 6 | 2, late | 2, late | 2, late | 2, late |
| 9 | `Edge(Change(u))` | 6 | 3 | 3 | 3 | 3 |
| 10 | `Pre(Change(u))` | 6 | 3, late | 5, late | 3, late | 5, late |
| 11 | `Or(Change(u), lvl2)` | 3 | 1 | 1 | 1 | 3 |
| 12 | `SampleTrigger(0.5 s)` | 19 | 0 | 9 | 9 | 9 |
| 13 | `SampleTrigger(1 s)` | 10 | 0 | 9 | 9 | 9 |
| 14 | `SampleTrigger(1.4 s)` | 7 | 3 | 6 | 6 | 6 |
| 15 | `SampleTrigger(2 s)` | 5 | 4 | 4 | 4 | 4 |

Rows 12 to 15 count the `when` firing at t = 0 in OpenModelica; under decision 3's recommended
option each of (a), (c) and (f) gains that one. Notes on the rows:

- **Row 1** is `DVG-000002` (OpenModelica 3, 3, 4, 5, 5, 6, 6, 7, 8, 9 from `y_start = 3`; the
  engine 3, 3, 4, 4, 4, 5, 5, 6, 6, 6). `DVG-000001` (Building 33's `Change(enable)` into
  `PIDWithReset.trigger`) is the same shape.
- **Rows 2, 4 and 11** are what the reviewer's rule (a) leaves: OpenModelica fires
  `when not change(u)` at every change instant, in the second event iteration, when `change(u)`
  has fallen; and fires `when change(u) or lvl` at each change while `lvl` is false and once when
  `lvl` rises, but not at 8 s, where `lvl` falls at the instant `u` changes.
- **Rows 6, 8, 9 and 10** need `Pre` or an edge block to act within one instant, which the engine
  does not do: OpenModelica's `Pre` is a delay of one event iteration, the engine's one frame
  ([execution profile](../execution-profile.md#cdllogicalpre)). Row 10 under (a) and (f) counts
  each event one frame late; the 9 s change lands at 10 s.
- **Row 12** is several instants in one frame: only option (b) could count them, and only for
  `OnCounter`.
- **TriggeredSampler** behind `SampleTrigger`, `u = 10 + k` at frame `k`, `y_start = 7`:
  OpenModelica gives 10, 11, 12, … at `period = 1` s and 10, 10, 12, 12, 14, … at `period = 2` s.
  The engine gives 7 at every frame at `period = 1` s, and 7, 7, 12, 12, 14, … at `period = 2` s
  (`crates/oce-conformance/tests/sample_trigger_event_inputs.rs`). Under (f) the first becomes 7,
  11, 12, …; with decision 3's recommended option both equal OpenModelica.
- **Frame 0.** OpenModelica fires `when sample(0, p)` at t = 0, in the event iteration that follows
  initialisation, and through the transcribed CDL `SampleTrigger` too. It fires no `when` at t = 0
  on a `Change` or `Edge` whose input is true from the start. The #768 rule ("no `when` fires at
  frame 0") is right for the second and wrong for the first; that is decision 3.
- **Latch.** OpenModelica's `Latch(u = sample(0, 0.5), clr = sample(0, 0.5) and not hold)`, with
  `hold` from 6.5 s, is false until the 7 s sample and true after it: each sample is an event. That
  is `TrimAndRespond`'s latch with `have_hol = true`.

### 1.4 What a pulse is on a frame

Under HostTick v1 every event inside `(t(k−1), t(k)]` is shown at frame `k`, with frame `k`'s
values. Call that the snapped instant. A Boolean built by stateless logic from pulses and levels
then takes three values across Modelica's event iteration at the snapped instant: **before** (every
pulse false, every level at its old value), **during** (the pulses true, the levels at their new
values) and **settled** (every pulse false again, the levels at their new values). Levels are
constant between frames, so *before* at frame `k` is *settled* at frame `k − 1`. The engine's
frame value `v(k)` is *during*. Writing `s(k)` for *settled*, a `when` on that Boolean fires at the
snapped instant if and only if

    fire(k) = (v(k) ∧ ¬s(k−1)) ∨ (¬v(k) ∧ s(k))

the first term a rise from *before* to *during*, the second from *during* to *settled*. Three
values can rise at most once, so one event per frame. For a Boolean with no pulse upstream
`s = v`, and the formula is today's rising edge `v(k) ∧ ¬v(k−1)`. For a pulse fed straight in,
`s ≡ false`, and it fires on every frame the pulse is true. Every row of section 1.3 that (f)
matches follows from this formula; the rows it misses have a `Pre` or an edge block inside the
instant, where the iteration has more than three steps.

## 2. Options

| | Rule | Matches (rows of 1.3) | Still differs | Can it fire an event the snapped Modelica model does not? |
| --- | --- | --- | --- | --- |
| (a) | Label each Boolean net at load: pulse producers' outputs, `And` with a pulse input, `Or` whose inputs are all pulses, `Pre` of a pulse; everything else a level. An input fed by a pulse fires on every true frame | 1, 3, 5, 7; 13 to 15 with decision 3 | 2, 4, 6, 8, 9, 10 (late), 11, 12 | No |
| (b) | Pulse producers carry an event count, or a distinct event kind, on the port | 1, 5 (with a count algebra), 12 for `OnCounter` only | Everything through logic needs new semantics for counts; values of `u` between instants are unknowable | Depends on the algebra |
| (c) | Fire on every true frame only when the input is wired straight to a pulse producer | 1, 3, 7; 13 to 15 with decision 3 | 2, 4, 5, 6, 8, 9, 10, 11, 12 | No |
| (d) | Run Modelica's event iteration inside the frame (micro-steps until no `pre` value changes) | 1 to 11 and 13 to 15, by argument (not built) | 12; `Pre` stops being a frame delay | No, by construction |
| (e) | No engine change: a load diagnostic and a host cadence obligation | 3 and 7, as today | all the others, documented | — (today's behaviour) |
| (f) | **Settled value**: each event input fires by section 1.4's formula, with `s` computed from the stateless logic between it and the pulse producers | 1 to 5, 7, 11; 13 to 15 with decision 3 | 6, 8, 9, 10 (late), 12 | No |

**(a) Pulse/level labels** (the reviewer's rule, issue #800 comment of 2026-11-22). Load labels
each Boolean net; a consumer whose input is labelled pulse fires when it is true, otherwise on the
rising edge. *Sound:* a pulse-labelled net true at frame `k` means a producer pulsed in
`(t(k−1), t(k)]`, and `And` with a level keeps that (it is true only where the pulse is), `Or` of
pulses too; a level-labelled net keeps today's rule, which fires only on a real rise. It cannot
fire an event the snapped model lacks. *Misses* the mixtures: `Or` with a level (rows 4, 11), `Not`
(row 2), `Xor`, `Nand`, `Nor`, a `Logical.Switch` mixing the two. It is the special case of (f)
where the settled value is either constant false or the value itself.

**(b) Event count on the port.** A `Change` would emit "how many changes in the frame", a
`SampleTrigger` how many instants. Only `OnCounter` can use a count; a `TriggeredSampler` asked to
sample three times in one frame has one `u`. CDL ports are Boolean, so the count is a hidden side
channel through every logic block, with an algebra (`And` of counts?) Modelica does not define. It
would change what the producers emit, and the 11 library fault rules that count one-frame pulses
with a moving average (section 4) read that emission. Rejected.

**(c) Direct wiring only.** The smallest change that closes both registered divergences and the
`SampleTrigger` → `TriggeredSampler` freeze. A pass-through, composite boundary or vector element
between producer and consumer would have to count as direct, and `Or(Edge(a), Edge(b))` does not.
Sound, for the same reason as (a). It leaves the combined pulses issue #800 names.

**(d) Event iteration inside the frame.** Exact for every single-frame shape, including `Pre` and
edge blocks inside the instant. It changes what `Pre` means (a micro-step, not a frame), which is
the documented HostTick v1 projection and the remedy the algebraic-loop refusal names; the
[execution profile](../execution-profile.md#snapshots) says a same-time event-iteration profile
"must carry profile identity in the manifest and codec. It must not consume HostTick v1 snapshots
as semantically equivalent state." A second profile, a fixed-point search with a bound, and a
per-frame cost that depends on the model. Out of scope for M3; recorded as the direction if exact
`Pre` ever matters.

**(e) No engine change.** Keep the rising-edge rule, register the shapes, and state the host
obligation: for `SampleTrigger`, any two consecutive frame steps must sum to at most `period`
(uniform frames: step ≤ `period / 2`); for `Change`, no source may change on two consecutive
frames, which a host cannot promise for a trended binary point. A load diagnostic could name each
pulse that reaches an event input. Cheap, honest, and leaves `DVG-000001` (a reset Building 33's
valve misses by 0.036 on a 0–1 signal) as shipped behaviour.

**(f) Settled value** (this note's proposal, a generalisation of (a)). At load, for each event
input, find its *cone*: the stateless Boolean logic upstream of it (`And`, `Or`, `Not`, `Nand`,
`Nor`, `Xor`, `MultiAnd`, `MultiOr`, `Logical.Switch`, the five Boolean routing classes,
`PassThrough.Boolean`, composite boundaries, vector elements one by one), stopping at *pulse
leaves* (the outputs of `Change`, `Integers.Change`, `Edge`, `FallingEdge`, `SampleTrigger`, and a
`Pre` whose input is itself a pure pulse) and *level leaves* (everything else). Each frame, the
settled value `s` of each net in a cone is the cone's logic evaluated with every pulse leaf false
and every level leaf at its frame value. The consumer fires by section 1.4's formula and stores
`s(k)` where it stores the previous trigger today. Where no pulse leaf is upstream, `s = v` and
nothing changes.

- *Sound, and exact where it claims to be.* For a cone of stateless logic the three values are the
  whole event iteration at the snapped instant, so (f) fires exactly when the snapped model does,
  at most once per frame. Pulse leaves that are not primitive (`Pre` of a pulse, an edge block of
  a pulse-carrying input) are real events shown up to one frame late, never invented ones.
- *Interaction with #768/#789.* No event at frame 0 is kept, except as decision 3 rules. Event
  inputs already feed through; a cone's level leaves are upstream of the event input through
  feedthrough logic, so the shadow evaluation adds no scheduling edge and no algebraic loop. `Pre`
  stays a cut, and the `Pre` remedy still "delays the input's events by one frame", now without
  merging consecutive ones (row 10). The `set_param` schedule-shape guard needs no new case: no
  cone depends on a parameter value (`SampleTrigger.period` changes when it pulses, not that it is
  a pulse; conditional instances are pruned at load; `PIDWithReset.controllerType` is already
  guarded).
- *Host obligation.* At most one instant per frame interval: for `SampleTrigger`, frame step ≤
  `period` (today it is `period / 2`). For `Change`, at most one change of its input per frame
  interval, which is what frame sampling sees anyway.
- *Cost.* Load: one pass over the Boolean nets. Per frame: one Boolean operation per block in a
  cone that reaches an event input, evaluated once per net (not once per consumer, so a hostile
  fan-out stays linear in the model), into a scratch buffer sized at load. Nothing allocates per
  frame. Cones are empty in most models.
- *no_std and firmware.* The shadow pass lives in `oce_graph::frame` beside the driver and the cone
  derivation in `oce-graph`'s load path, both `no_std + alloc`; the firmware image grows by that
  code, to be measured against its budget in the implementation PR.
- *Formats.* The blob is unchanged if the decoder derives the cones from the wiring it already
  validates (decision C6); the state layout is unchanged (the previous-trigger word now holds `s`,
  still a canonical Boolean, and the same bits wherever no pulse is upstream); every
  `state_contract_revision` stays 1; the CXF document and its content id are untouched, since this
  is engine semantics, not document content. The workspace is 0.6.0 after the v0.5.0 tag, so
  rule 10's bump is done unless v0.6.0 is tagged before the implementation lands.

## 3. Recommendation

**Build (f).** It closes `DVG-000001` and `DVG-000002`, ends the `SampleTrigger` freeze, and fixes
the mixtures (rows 2, 4, 11) that (a) keeps, for the same per-frame cost class, the same soundness
argument and no format or state-layout change. It changes nothing where no pulse reaches an event
input, which is almost every tracked model (section 4). (a) is the fallback if the owner wants the
smallest explanation: it is (f) with the cone collapsed to a label, and an implementation of (f)
contains it. (c) is too narrow for the combined pulses the issue names, (b) changes what the
library's counting rules read, (d) is a new profile, and (e) ships a known 0.036 valve error.

## Decisions

Decisions 1 to 4 are **owner rulings asked for**, each with its options, the recommended one
first. Decisions C1 to C7 are **calls this note proposes to make itself**; the owner can overrule
any of them.

1. **Which rule** (owner). (i) **(f), the settled value.** (ii) (a), static pulse/level labels:
   the same for direct and `And`/`Or`-of-pulse shapes, keeps today's merging for `Or` with a level
   and for `Not`. (iii) (c), direct wiring only: closes the two registered divergences and the
   `SampleTrigger` freeze, nothing combined. (iv) (e), no engine change: document the cadence
   obligation, keep both divergences open.
2. **The shapes the rule cannot match** (owner): several events in one frame (row 12); pulses
   through `Pre` or an edge block inside one instant (rows 6, 8, 9, 10); duration consumers fed by
   a pulse (`Timer`, `TrueDelay`, `TrueFalseHold`, `TrueHoldWithReset`, `MovingAverage` of a
   converted pulse). (i) **Document them in the execution profile, and emit a frame warning when a
   `SampleTrigger` frame spans more than one sample instant** (a host-cadence violation the engine
   can see). (ii) Document only. (iii) Also warn at load for each unmatched shape: the 11 library
   fault rules that count one-frame pulses with a moving average would warn on every load.
   (iv) Refuse them at load: those 11 rules would no longer load.
3. **A `SampleTrigger` instant at frame 0** (owner). (i) **Fire it**, as OpenModelica does: at
   frame 0 the cone is evaluated with each `SampleTrigger` leaf true only if `t(0)` is on its
   sample grid (within the block's `1e-9 · period`) and every `Change`, `Edge` and `FallingEdge`
   leaf false (their frame-0 value is initialisation, which fires nothing); `TriggeredSampler`
   behind a `SampleTrigger` then samples at frame 0. (ii) Keep #768's "no event at frame 0" for
   every input: such a sampler shows `y_start` until the second instant.
4. **G36 `TrimAndRespond` and `Latch`/`Toggle` in M3** (owner). (i) **Apply the rule to
   `Latch.u` and `Toggle.u` with the other event inputs now.** No tracked trace moves: all three
   fixtures that hold the block run `have_hol = false`, where `clr` equals `u` and the latch is
   false whatever the rule, at frame steps of 1 s and 60 s against a 120 s `samplePeriod`. With
   `have_hol = true` at a frame step above 60 s, the latch would now re-latch at each sample as
   Modelica does. (ii) Apply it to the six `when`-only classes (`OnCounter`, the three triggered
   samplers, the two reset integrators) and leave `Latch` and `Toggle` on the rising edge until a
   G36 run with `have_hol = true` is evidence.

- **C1. The cone** is exactly the stateless Boolean classes listed under (f), with composite
  boundaries and the lowering's pass-throughs transparent and vector ports element by element.
  Integer and Real paths (`BooleanToInteger` of a pulse, comparators) are level leaves.
- **C2. `Pre` is a cut.** `Pre` of a pure pulse (`s ≡ false` statically) is a pure pulse one frame
  late; `Pre` of anything else is a level leaf. That needs no second state word, so `state_len`
  and the class signature hash do not change.
- **C3. Edge blocks keep their rule.** `Edge`, `FallingEdge`, `Change` and `Integers.Change` whose
  input carries a pulse still compare with the previous frame's value (rows 8, 9). Making them use
  `s` gets row 9 right and row 8 wrong (it would never fire where today fires one frame late);
  the exact answer needs (d).
- **C4. Consumers that change:** `OnCounter.trigger` and `.reset`, `TriggeredSampler`,
  `TriggeredMax` and `TriggeredMovingMean` `.trigger`, `IntegratorWithReset.trigger`,
  `PIDWithReset.trigger` (PI and PID), and, per decision 4, `Latch.u` and `Toggle.u`. Unchanged:
  `Latch.clr`, `Toggle.clr`, `TrueHoldWithReset.clr` (level-equivalent, section 1.2) and the
  duration consumers.
- **C5. The stored word holds `s(k)`.** Each consumer's previous-trigger word stores the settled
  value instead of the frame value; frame 0 stores `s(0)`, which keeps decision 5 of
  [Reset timing](reset-timing.md#decisions) for levels.
- **C6. The blob carries no new bytes.** The cones are derived from the wiring by one `oce-graph`
  function that both the loader and the blob decoder run, as the decoder already re-derives and
  checks emit-before order. Blobs and snapshots written by a 0.6.0 build before the change load
  under the new rule; the implementation PR says so, as GitHub #789 did. If v0.6.0 is tagged first,
  the PR bumps the version and adds the refusal test rule 10 asks for.
- **C7. One PR**, `fix(graph): …` or `fix(blocks): …`, closing #800, with the register entries
  resolved in it.

## 4. Blast radius

Measured by the static scan and the second probe; no implementation was run.

| Corpus | Pulse into a consumer | Moves under (f) |
| --- | --- | --- |
| Building 33 (`crates/leverage/oce-verify/tests/fixtures/commissioning/building33/`) | `Change(enable)` → `PIDWithReset.trigger`, direct | Yes: `valve_oracle.txt` from the appended re-enable at 382 s on, `derive_valve.py`, `yCooVal_reenable_probe.engine.csv` (resetting at 382 s, as OpenModelica does), and the two OpenModelica tests' claims ("eight of nine" resets becomes nine of nine) |
| G36 `cooling_only_controller`, `generic_time_suppression` (and `oce-author`'s emitted `generic_time_suppression.cxf.json`) | `Edge` → two `TriggeredSampler.trigger`, direct; `Edge` → `Latch.clr` | No: an `Edge` is never true on consecutive frames, so `s ≡ false` and today's edge give the same bits; `clr` is unchanged (C4) |
| G36 `multizone_vav_freeze_protection` | `Or(tim.passed, Edge(endStaTwo))` → `lat.u`; `Edge` → `lat.clr`, `lat2.clr` | No: the probe ran the 111 trace frames and `endStaTwo` never pulses |
| G36 `multizone_vav_supply_fan`, `multizone_vav_supply_temperature`, `trim_and_respond_have_hol_false` | `SampleTrigger(120 s)` → `Latch.u`, direct; `And(Not(false), SampleTrigger)` → `Latch.clr` | No: `have_hol = false` and frame steps of 1, 1 and 60 s |
| The other 40 G36 fixtures, the CDL fixtures, `tools/golden-gen` goldens | none | No |
| Compiled-schedule blobs, `blob_frame_digests.tsv`, `class_signatures.tsv` | — | No (C6; no trace moves; no `state_len` changes) |
| `oce-replay` traces (`controller_minute_day`, `controller_grid_hold`) | `cooling_only_controller`'s `Edge` → `TriggeredSampler` | No |
| `open-control-library` at fe7060a: 140 documents | 11 fault rules (AHU-0004, CHW-0009, FCU-0001, HP-0002, HW-0001, HW-0011 ×2, PMP-0004, RTU-0001, TOWER-0003, VAV-0005, VFD-0004 ×2): `Change` or `Edge` → `BooleanToInteger` → `IntegerToReal` → `MovingAverage` → `MultiplyByParameter` → `GreaterThreshold` | No: no event input; they count one-frame pulses, which their cards document and which counts a consecutive change twice, as Modelica counts events |
| Engine tests that pin today's merging | `open_modelica_event_pulses.rs`, `sample_trigger_event_inputs.rs`, `building_33_openmodelica_reenable.rs`, `building_33_openmodelica_frames.rs` | Rewritten to the new rule; `DVG-000001` and `DVG-000002` move to `resolved` |

So the 46 fixture goldens and 422 provenance goldens are expected byte-identical, and no G36 trace
or blob moves; the implementation PR proves that by running them. Docs that change with it: the
execution profile's "Events on a Boolean input" and "Conformance boundary", `docs/cdl-coverage.md`,
`docs/verification-evidence.md`, `TESTING.md`'s line on `open_modelica_event_pulses.rs`, the
Building 33 README, the rustdoc of `SampleTrigger` and of the consumers that cite issue #800
(`blocks/discrete.rs`, `blocks/integers_edge.rs`, `blocks/pid.rs`, `blocks/reals_integrator.rs`),
and `CHANGELOG.md`. Under decision 4 (ii) the `Latch` and `Toggle` rows drop out; under decision
1 (ii) or (iii) the moves are the same in the corpus, since no tracked model holds row 2, 4 or 11.

## 5. Test and evidence plan

Per [TESTING.md](../../TESTING.md):

- **The formula, exhaustively.** For each consumer in C4, all eight `(s(k−1), v(k), s(k))`
  combinations at frames 1 and 2, with expectations derived by hand from section 1.4.
- **The shapes.** Every row of section 1.3 as a committed engine vector, with the hand-derived count
  per frame; plus `Nand`, `Nor`, `Xor`, `Logical.Switch` with a pulse on each data input and on the
  selector, `MultiAnd`/`MultiOr` with mixed widths, a pulse through a composite boundary and through
  a vector element, `Pre(Change(u))` (one frame late, not merged), `Pre(Or(Change(u), lvl))` (a
  level leaf), and the same pulse wired to two consumers.
- **Edge cases.** A pulse true on every frame; a pulse at frame 1 (the first possible event); a
  pulse and a level rise at the same frame; a level falling at the frame of a pulse (row 4 at 8 s);
  a `SampleTrigger` exactly on, a hair before and a hair after the frame-0 grid point (decision 3);
  frame steps above `period` (the decision 2 warning); repeated equal timestamps; snapshot and
  restore between two consecutive pulses; `set_param` on `SampleTrigger.period` keeping the cone.
- **Unchanged where no pulse is upstream.** The whole G36 corpus, the CDL fixtures, the blob round
  trip and the replay traces byte-identical, which is the evidence that `s = v` everywhere else.
- **OpenModelica pilots**, added to `tools/openmodelica-event-reference/events/` and run by
  `regenerate.sh` against the pinned Buildings and MSL commits (not transcriptions): the shapes of
  rows 1 to 11 into `OnCounter`, `SampleTrigger` at 0.5, 1, 1.4 and 2 s into `OnCounter` and
  `TriggeredSampler`, and `TrimAndRespond`'s latch with a hold. The engine must equal OpenModelica
  at every frame on the rows (f) claims, and the others are registered as known divergences with
  their evidence, not left as prose. `DVG-000001` and `DVG-000002` move to `resolved` with a
  disposition naming the PR and the passing comparison, their evidence files kept.
- **Mutants that must die:** firing on `v` alone (a held level re-fires); storing `v` instead of
  `s` (consecutive pulses merge again); dropping the `¬v ∧ s` term (row 2); computing `s` with the
  previous frame's levels (row 4 at 8 s); labelling `Pre` of a mixed cone a pulse (it fires on every
  frame the level holds); taking the frame-0 grid test without the epsilon.
- **Determinism.** The new vectors and the unchanged corpus repeat bit for bit in the four native
  cells; the PR touches `oce-blocks` and `oce-graph` and runs the determinism lane.
- **Unverified by any tool, said plainly.** The snapping itself: events at different instants inside
  one frame (for example `And` of two `SampleTrigger`s of different periods) are treated as
  simultaneous, by today's engine and by every option. The exactness argument of section 1.4 is a
  proof sketch, checked by OpenModelica on one input pattern. This note's OpenModelica runs used
  transcribed equations. `Toggle`, `TriggeredMax`, `TriggeredMovingMean` and the reset integrators
  behind a pulse were not run in OpenModelica. `Integer` pulses (`Integers.Change` of a
  `BooleanToInteger` of a pulse) are untraced. Whether OpenModelica's `Change` output at the
  initial instant is false (one scratch run printed false) was not examined; decision 3 does not
  depend on it, because the consuming `when` fires either way only on `sample`.

## 6. What this does not fix

- **Several events in one frame** count once: a `SampleTrigger` at half the frame step counts 9 of
  OpenModelica's 19 instants (row 12), and a source that changes twice between frames is not seen
  at all. The host obligation is one instant per frame interval.
- **`Pre` and edge blocks inside one instant** (rows 6, 8, 9; row 10 one frame late): Modelica's
  `Pre` is an event-iteration delay, the engine's a frame. Only a new profile (option (d)) fixes
  them.
- **Duration consumers** read a pulse as lasting a frame: `TrueDelay`, `Timer`, `TrueFalseHold`,
  `TrueHoldWithReset` and a `MovingAverage` of a converted pulse see one frame step of `true` where
  Modelica sees none. The library's 11 counting rules rely on exactly that and keep working.
- **What the engine shows on a pulse output** is unchanged: `Change` emits `true` for the whole
  frame, where OpenModelica's last row at the instant shows `false`.
- **Frame 0 for levels**: a trigger true from the start is still not an event (issue #768,
  confirmed by OpenModelica in GitHub #796).
