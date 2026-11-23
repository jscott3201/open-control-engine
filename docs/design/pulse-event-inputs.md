# Event inputs fed by one-frame pulses

**Design note, written before any code (issue #800), revised after its first review, and ruled
on.** The owner ruled on 2026-11-22: write a design note first for an engine fix, "the pulse/level
rule, or better if the note finds one"; nothing is built before the note is reviewed, and the two
divergences stay pinned in the register meanwhile. This note states what Modelica does with a pulse
on an event input and what the engine does, for every registered pulse producer and event
consumer; measures the shapes against OpenModelica, the engine and a brute-force simulator;
evaluates the options; and measures the blast radius. The owner ruled on all six of its
[decisions](#decisions) on 2026-11-22, each for the option this note recommended: **build option
(g), the corrected rule**. Calls C1 to C8 are this note's own. The
[execution profile](../execution-profile.md) stays the authority for shipped behaviour until the
implementation PR changes it.

**What the review changed.** The first draft recommended a "settled value" rule, kept below as
option (f). An independent review showed that it invents events in three families of shapes, and
that two statements it rested on were false: a `sample()` pulse is not "true only in the first
event iteration", and a `Pre` or an edge block of a pulse does not merely show a real event late.
This revision reran every counterexample (section 1.3 and
[Evidence that can be rerun](#evidence-that-can-be-rerun)) and recommended option (g), the corrected
rule, which the owner chose.

Paths below are repository-relative. `CDL/` abbreviates the vendored
`third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/`; `blocks/` abbreviates
`crates/oce-blocks/src/`; `pulse-event-inputs/` abbreviates `docs/design/pulse-event-inputs/`.
Engine line numbers are at `development` 128f7a8d.

## How the evidence was gathered

- **Sources.** The 39 vendored CDL classes with `when`, `edge(`, `change(`, `sample(` or `pre(`
  (the set [Reset timing at the event](reset-timing.md) read), G36 `Generic.TrimAndRespond`, and
  the engine implementation of each class with a Boolean event or a pulse output.
- **Modelica semantics.** Modelica Language Specification 3.6: a when-equation is "activated only
  at the instant when the scalar expression or any of the elements of the vector expression becomes
  true" (§8.3.5); `edge(b)` "expands into (b and not pre(b))", `change(v)` "expands into
  (v <> pre(v))" (§3.7.5); after each event iteration `pre(v)` takes the value of `v`, and
  iteration repeats until nothing changes (§8.6). What OpenModelica does with `sample()` is
  measured, not read: it is true in **every** event iteration of its instant and false in
  continuous time, where no `when` sees it fall. `when not sample(0, 1)` never fires at the
  instants 0 to 9 s; `when sample(0, 1) or lvl` fires at each instant and when `lvl` rises between
  them.
- **OpenModelica.** Four probe models in `pulse-event-inputs/om/`, with their results. They are
  scratch quality and **transcriptions**: each block transcribes the vendored CDL equations it
  names, or uses Modelica built-ins, and no library is loaded. They are not conformance fixtures.
  They ran in the pinned OpenModelica 1.25.1 image (the digest in `om/run.sh`, the one every other
  OpenModelica case of the repository pins), one container at a time, no network, cvode at
  tolerance 1e-6, output every 1 s; `om/fires.py` prints `om/results.txt` from them. The last row
  of a run, at its stop time, is not an event instant and is ignored (`notSample` shows one fire
  there).
- **Simulator.** `pulse-event-inputs/simulate.py` (standard library, `python3 -I`,
  deterministic; output in `pulse-event-inputs/results.txt`) models the snapped Modelica event
  iteration and each rule, and runs both over every Boolean function of a set of leaves (all
  2^(2^n) "cones"), every input sequence of five or six frames, and the named shapes of section
  1.3. Its docstring states its model of `sample()`, `pre()`, `change()` and `edge()` with the
  specification and `.mo` lines each encodes. On all twenty named shapes its Modelica column equals
  the OpenModelica runs.
- **Engine probe.** A scratch example (not committed) loaded one CXF document holding the first
  draft's shapes through `oce_api::Engine::load_cxf` at 1 s frames; its counts equal the
  simulator's "today" column. A second one ran the `multizone_vav_freeze_protection` fixture over
  its 111 trace frames and watched the Edge pulse into its latch (section 4).
- **Static scan.** A scratch script (`python3 -I`, not committed) walked every JSON-LD graph in the
  tree (214 under `crates/`, 46 under `third_party/`) and in `open-control-library` at its
  `development` tip fe7060a (140), from each pulse producer through Boolean logic and composite
  boundaries to the first consuming input (section 4).

## 1. The semantics

### 1.1 Pulse producers

| Class | Modelica | Engine | True on two consecutive frames? |
| --- | --- | --- | --- |
| `Logical.Change` | `y = change(u)`, `u(fixed = true, start = pre_u_start)` (`Logical/Change.mo:6-15`) | `u ≠` the previous frame's `u` (`blocks/logical_latch.rs:112`) | Yes, when `u` changes on two consecutive frames |
| `Integers.Change` | `y = change(u)`, `up = u > pre(u)`, `down = u < pre(u)` (`Integers/Change.mo:6-23`) | the same comparisons with the previous frame's `u` (`blocks/integers_edge.rs:175-178`) | Yes, all three outputs |
| `Logical.Edge` | `y = edge(u)` (`Edge.mo:13-17`) | `u ∧ ¬` previous `u` (`blocks/logical.rs:434`) | No: true at `k` needs `u` false at `k − 1`, so it was false there |
| `Logical.FallingEdge` | `y = edge(not u)` (`FallingEdge.mo:14-21`) | previous `u ∧ ¬u` (`blocks/logical_latch.rs:61`) | No, by the same argument |
| `Logical.Sources.SampleTrigger` | `y = sample(t0, period)`, `t0` from the start time and `shift`, rounded to 1e-6 s (`Sources/SampleTrigger.mo:24-30`) | true on each frame that reaches a new index `floor((t − phase) / period + 1e-9)`, and at frame 0 whenever `t(0) ≥ phase`, on the grid or not (`blocks/logical.rs:502-559`) | Yes, whenever two adjacent frame intervals each hold an instant: always at a frame step at or above `period`, sometimes above `period / 2` (at 1 s frames and `period = 1.4` s it is true at 2, 3, 5, 6, 7 and 9 s), never at or below `period / 2`. A frame holding several instants is one `true` |

Other sources of consecutive `true`: pulses combined through `Or` or `And`, and pulses built by
hand from `Pre`. `Xor(u, Pre(u))` is a hand-built `Change` and can be true on consecutive frames;
`And(u, Not(Pre(u)))` is a hand-built `Edge` and cannot. `Logical.Sources.Pulse`, `Timer.passed`,
`TrueDelay.y` and friends are levels: they hold a value for a duration in Modelica too.

A `sample()` and a `change()` are both instantaneous, but they end differently inside the event
iteration at their instant: `change(u)` falls in the iteration after `u` changes, when `pre(u)`
catches up; `sample()` stays true through every iteration. The first draft missed that difference;
it is why section 1.4 needs three values per frame.

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
| `Logical.TimerAccumulating.reset` | `when reset then entryTime = time; passed = t <= 0; yAcc = 0`, the first branch (`Logical/TimerAccumulating.mo:42-45`) | rising edge (`blocks/logical_timing.rs:151, 166`) | event (decision 6) |
| `Logical.Latch.u` | `when {clr, u} then y = not clr and u` (`Logical/Latch.mo:17-19`) | rising edge sets (`blocks/logical_latch.rs:132`) | event |
| `Logical.Toggle.u` | `change(u) and pre(u) == false`, that is `edge(u)` (`Logical/Toggle.mo:23-42`) | rising edge toggles (`blocks/logical_latch.rs:191`) | event |
| `Logical.Latch.clr`, `Logical.Toggle.clr` | `when {clr, …}` / `elsewhen clr`, and every other branch is guarded by `not clr` | read as a level: `¬clr ∧ …` | level-equivalent (below) |
| `Logical.Edge.u`, `FallingEdge.u`, `Change.u` | `pre(u)` | the previous frame's `u` | the producer's own memory |
| `Logical.Timer.u`, `TimerAccumulating.u`, `TrueDelay.u`, `TrueFalseHold.u`, `Reals.Ramp.active` | `when u` together with `elsewhen not u`, timers and the level itself (`Timer.mo:31-49`, `TimerAccumulating.mo:46-65`, `TrueDelay.mo:34-46`, `TrueFalseHold.mo:48-61`, `Ramp.mo:43-52`) | edges and the level | duration consumer |
| `Logical.TrueHoldWithReset.u`, `.clr` | none: the class has no vendored source, and its `clr` input is this engine's own extension (`blocks/logical_timing.rs:433-449`) | the level | engine-local |

`Latch.clr` and `Toggle.clr` give the same output whether read as events or as a level: at every
event of either input a true `clr` forces `y` false, a rising `clr` is such an event, and while
`clr` is false nothing reads it. `TrueHoldWithReset` has no Modelica text to check a claim
against, so this note makes none for it. Duration consumers read how long the input stays true; a
Modelica pulse lasts no time, so `TrueDelay(delayTime > 0)` of a pulse is always false in Modelica
and can be true in the engine. No event rule repairs that (section 6).

`TimerAccumulating.reset` merges consecutive pulses as the others do: fed by `Change(u)` with `u`
changing at 2, 3, 5, 7, 8 and 9 s, OpenModelica sets `y` back to 0 at all six (`timAcc.y` in
`om/results.txt`), the engine at 2, 5 and 7 only.

**Frame 0 per class.** OpenModelica fires no `when` at t = 0 on a `Change` or `Edge` true at
initialisation (`InitPulse`), and fires `when sample(0, p)` at t = 0 (`nSam`, and `c1` through
the transcribed `SampleTrigger`). `Latch(u = Change(x))` and `Toggle(u = Change(x))` with `x` true
from the start show `y = 1` at t = 0 (`latX`, `togX`): their initial equations read `u` at
initialisation, where `change(x)` is true. The result file prints `change(x)` false at t = 0
because its rows show the values after the initial event iteration. The engine already matches
both blocks there.

### 1.3 The shapes, measured

`u` changes at 2, 3, 5, 7, 8 and 9 s (the `OnCounterChangePilot` pattern), `lvl` is true on
[4, 8) s, `lvl2` from 3.5 s (the engine sees it at 4 s), `b` on [2, 7) s; frames are 1 s apart, and
each shape drives a counter. The table counts events at frames 1 to 9; frame 0 is section 1.2.
**OM** is OpenModelica (`om/results.txt`) and equals the simulator's snapped Modelica on every
row but 12, which the simulator cannot represent. The rule columns are the simulator's
(`results.txt`), and "today" also equals the engine probe. "L" marks events shown one frame after
Modelica's; "fb" marks a cone that (g) sends to today's rule; bold marks invented events.

| # | Shape into the counter | OM | Today | (a) | (a′) | (c) | (f) | (g) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `Change(u)` | 6 | 3 | 6 | 6 | 6 | 6 | 6 |
| 2 | `Not(Change(u))` | 6 | 2 L | 2 L | 2 L | 2 L | 6 | 6 |
| 3 | `And(Change(u), lvl)` | 2 | 2 | 2 | 2 | 2 | 2 | 2 |
| 4 | `Or(Change(u), lvl)` | 4 | 1 | 1 | 1 | 1 | 4 | 4 |
| 5 | `Or(Change(u), Change(u′))`, `u′ = u` | 6 | 3 | 6 | 6 | 3 | 6 | 6 |
| 6 | `Xor(u, Pre(u))` | 6 | 3 | 3 | 3 | 3 | 3 | 3 fb |
| 7 | `And(u, Not(Pre(u)))` | 3 | 3 | 3 | 3 | 3 | 3 | 3 fb |
| 8 | `Edge(Not(Change(u)))` | 6 | 2 L | 2 L | 2 L | 2 L | 2 L | 2 L fb |
| 9 | `Edge(Change(u))` | 6 | 3 | 3 | 3 | 3 | 3 | 3 fb |
| 10 | `Pre(Change(u))` | 6 | 3 L | 5 L | 3 L | 3 L | 5 L | 3 L fb |
| 11 | `Or(Change(u), lvl2)` | 3 | 1 | 1 | 1 | 1 | 3 | 3 |
| 12 | `SampleTrigger(0.5 s)` | 18 | 0 | 9 | 9 | 9 | 9 | 9 |
| 13 | `SampleTrigger(1 s)` | 9 | 0 | 9 | 9 | 9 | 9 | 9 |
| 14 | `SampleTrigger(1.4 s)` | 6 | 3 | 6 | 6 | 6 | 6 | 6 |
| 15 | `SampleTrigger(2 s)` | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| 16 | `Not(SampleTrigger(1 s))` | 0 | 0 | 0 | 0 | 0 | **9** | 0 |
| 17 | `Or(Change(u), Pre(b))` | 3 at 2, 8, 9 | 1 | 1 | 1 | 1 | **3 at 2, 3, 9** | 1 fb |
| 18 | `Not(Or(Change(u), Pre(u)))` | 3 at 3, 7, 9 | 1 | 1 | 1 | 1 | **4 at 2, 4, 5, 8** | 1 fb |
| 19 | `Or(Change(u), Pre(Change(u)))` | 6 | 1 | **8** | 1 | 1 | **8** | 1 fb |
| 20 | `Change(Edge(u))` | 3 | 3 | **6** | 3 | 3 | **6** | 3 fb |
| 21 | `And(Change(u), Edge(Not(Change(u))))` | 0 | 0 | 0 | 0 | 0 | 0 | 0 fb |

(a) is the reviewer's first rule; (a′) is the same with only primitive pulse leaves, and today's
rule for any cone holding a `Pre` or an edge block of a pulse; (c) fires on every true frame only
when the input is wired straight to a primitive producer; (f) is the first draft's rule and (g)
the corrected one (section 2). Notes:

- **Row 1** is `DVG-000002`. `DVG-000001` (Building 33's `Change(enable)` into
  `PIDWithReset.trigger`) is the same shape.
- **Row 12** is several instants in one frame (OM counts the 18 instants in (0, 9] s): no rule
  that evaluates once per frame can count them.
- **Rows 12 to 15 at frame 0.** OpenModelica also fires each at t = 0; decision 3.
- **Row 16** is what the first draft's handling of samples gets wrong. Its `TriggeredSampler`
  form, `TriggeredSampler(trigger = Not(SampleTrigger(1 s)))`, never samples in OpenModelica
  (`samplerNotSample`); (f) would sample at frames 1 to 9. Today's rule gets row 16 right only
  because a 1 s sample is true on every frame: on `Not(SampleTrigger(2 s))` it fires at every frame
  between instants, where OpenModelica never fires.
- **Rows 17 and 18**: OpenModelica's `Pre(x)` takes `x`'s new value in the second iteration of the
  instant, the engine's one frame later, so (f) evaluates the cone against a stale level.
- **Rows 19 and 20**: a non-primitive pulse (`Pre` of a pulse, an edge block of a pulse) is a late
  or extra copy, and firing on it as a pulse counts the event twice.
- **Row 21** is what an edge block computed from three values would get wrong (decision 5): at a
  change, `Change(u)` is true in the first iteration and `Edge(Not(Change(u)))` in the second,
  never together, but their three-value frame values are true together at every change.
- **TriggeredSampler** behind `SampleTrigger`, `u = 10 + k` at frame `k`, `y_start = 7`:
  OpenModelica gives 10, 11, 12, … at `period = 1` s and 10, 10, 12, 12, 14, … at `period = 2` s.
  The engine gives 7 at every frame at `period = 1` s, and 7, 7, 12, 12, 14, … at `period = 2` s
  (`crates/oce-conformance/tests/sample_trigger_event_inputs.rs`). Under (g) the first becomes 7,
  11, 12, …; with decision 3's frame-0 ruling both equal OpenModelica.

### 1.4 What a pulse is on a frame

Under HostTick v1 every event inside `(t(k−1), t(k)]` is shown at frame `k`, with frame `k`'s
values. Call that the snapped instant. Take a cone built by stateless logic from levels,
**primitive pulses** (a `Change`, `Integers.Change`, `Edge` or `FallingEdge` of a net with no
pulse, no sample and no `Pre` behind it) and **samples** (`SampleTrigger` outputs). Across
Modelica's event iteration at the snapped instant it takes three values:

- **before**, the continuous-time value just before the instant: every pulse and every sample
  false, the levels at their old values. Levels are constant between frames, so this is
  `w(k−1)`, where `w(k)` is the cone with pulses and samples false and the levels at frame `k`;
- **during**, the first iteration: pulses and samples at their frame values, levels new. This is
  the engine's frame value `v(k)`;
- **settled**, every later iteration: pulses false again (their `pre` has caught up), samples
  still at their frame value, levels new. Call it `s(k)`.

A `when` on the cone fires at the snapped instant if and only if

    fire(k) = (¬w(k−1) ∧ v(k)) ∨ (¬v(k) ∧ s(k))

a rise from *before* to *during*, or from *during* to *settled*. Three values rise at most once,
so one event per frame. The block stores `w(k)` where it stores the previous trigger today.
Without a pulse or sample upstream `w = s = v`, and the formula is today's rising edge. A pulse fed
straight in fires on every frame it is true; a sample does too; `Not` of a sample never fires, as
in OpenModelica.

The three values are exact only for that leaf set. A `Pre` changes value in the second iteration
(rows 17, 18) and an edge block of a pulse in the second or third (rows 8, 9, 19, 20), so the
iteration has more steps than three values can hold, and the engine shows `Pre`'s settled value a
frame late. The corrected rule therefore applies the formula only to cones whose leaves are all
levels, primitive pulses and samples, and keeps today's rising edge for every other cone.

## 2. Options

Counts are cones out of all Boolean functions of the leaf set, over every input sequence of five
or six frames (`results.txt`). **Invents** means a fire that no Modelica event of its frame or the
frame before accounts for, in the snapped model.

| | Rule | Rows of 1.3 it gets right (frames 1 to 9) | Invents (snapped) | Verdict |
| --- | --- | --- | --- | --- |
| today | rising edge `v(k) ∧ ¬v(k−1)` | 3, 7, 15, 16, 20, 21 | 44,800 of 65,536 cones of {Change(a), S, a, b} (negated or mixed samples); 170 of 256 of {Change(a), Pre(Change(a)), b}; 155 of 256 of {Change(a), Change(Edge(a)), b}; none on primitive pulses alone | the baseline |
| (a) | the reviewer's labels: pulse producers' outputs, `And` with a pulse, `Or` of pulses, `Pre` of a pulse; a pulse-labelled input fires on every true frame | 1, 3, 5, 7, 13 to 16, 21 | as today with samples; more than today with `Pre` of a pulse (198 against 170) and with an edge block of a pulse (186 against 155): rows 19, 20 | unsound beyond today |
| (a′) | (a) with primitive pulse leaves only; today's rule for a cone with any `Pre` or edge block of a pulse | 1, 3, 5, 7, 13 to 16, 20, 21 | as today, on every leaf set | sound relative to today; merges `Not` and `Or`-with-a-level |
| (b) | pulse producers carry an event count on the port | row 12 for `OnCounter` only | depends on a count algebra Modelica does not define | rejected |
| (c) | every true frame, only when wired straight to a primitive producer | 1, 3, 7, 13 to 16, 20, 21 | as today | sound relative to today; nothing combined |
| (d) | Modelica's event iteration inside the frame | 1 to 11 and 13 to 21, by argument | none, by construction | a new profile |
| (e) | no engine change | as today | as today | ships both divergences |
| (f) | the first draft's settled value | 1 to 5, 7, 11, 13 to 15, 21 | **44,800** of 65,536 with a sample; **156** of 256 with a `Pre` of a level, where today invents in none; **180** of 256 with a `Pre` or an edge block of a pulse | **unsound**: rows 16 to 20 |
| (g) | the corrected rule: section 1.4's formula where every leaf is a level, a primitive pulse or a sample; today's rule for every other cone | 1 to 5, 7, 11, 13 to 16, 20, 21 | **none** on {Change(a), S, a, b}, {Change(a), S, a}, {Change(a), Change(b), a, b}, {Edge(a), FallingEdge(b), a, b}, where it is exact in every cone; as today in fallback cones | **chosen** (decision 1) |

**(a) Pulse/level labels** (the reviewer's rule, issue #800 comment of 2026-11-22). Load labels
each Boolean net; a pulse-labelled input fires when true, any other on the rising edge. It fixes
the direct, `And` and `Or`-of-pulses shapes. As written it labels `Pre` of a pulse a pulse, and
treats an edge block's output as a pulse whatever feeds it, which counts one event twice
(rows 19, 20). Restricted to primitive leaves, with today's rule for any cone that also reads a
`Pre` or an edge block of a pulse, it becomes (a′): it never invents beyond today, and still merges
`Not(Change)` and `Or(Change, level)` (rows 2, 4, 11) and keeps today's negated-sample errors.
(Without that fallback it invents beyond today too: 158 against 155 cones of {Change(a),
Change(Edge(a)), b}, where a cone that is a pulse by its primitive leaf also reads the stale one.)

**Where these counts differ from the review's.** The review counted invented events for today and
(a) on its two non-primitive leaf sets as 80 and 108 (`Pre(Change(a))`) and 40 and 71
(`Change(Edge(a))`); this note's are 170 and 198, and 155 and 186. The counts for (f) agree (180),
as do every count on the primitive leaf sets. The definitions behind this note's numbers, all in
`simulate.py`: the leaf sets are exactly {Change(a), Pre(Change(a)), b} and {Change(a),
Change(Edge(a)), b}; every host sequence of six frames is enumerated from a settled frame 0; and a
fire is *invented* only if no Modelica event of its own frame or of the frame before is left to
match it, matching each fire first to the previous frame's unmatched event (so a fire one frame
late is late, not invented). The review's leaf sets or matching may differ; the conclusions do
not: (a) as written invents beyond today on both, and (f) invents on both.

**(b) Event count on the port.** A `Change` would emit "how many changes in the frame", a
`SampleTrigger` how many instants. Only `OnCounter` can use a count; a `TriggeredSampler` asked to
sample three times in one frame has one `u`. CDL ports are Boolean, so the count is a hidden side
channel with an algebra (`And` of counts?) Modelica does not define, and it changes what the
producers emit, which the 11 library fault rules of section 4 read. Rejected.

**(c) Direct wiring only.** The smallest change that closes both registered divergences and the
`SampleTrigger` freeze. A pass-through, composite boundary or vector element between producer and
consumer counts as direct; `Or(Edge(a), Edge(b))` does not, and neither does an edge block of a
pulse.

**(d) Event iteration inside the frame.** Exact for every single-frame shape, including `Pre` and
edge blocks inside the instant. It changes what `Pre` means (a micro-step, not a frame), which is
the documented HostTick v1 projection and the remedy the algebraic-loop refusal names; the
[execution profile](../execution-profile.md#snapshots) says a same-time event-iteration profile
"must carry profile identity in the manifest and codec. It must not consume HostTick v1 snapshots
as semantically equivalent state." A second profile, a fixed-point search with a bound, and a
per-frame cost that depends on the model. Out of scope for M3.

**(e) No engine change.** Keep the rising edge, register the shapes, and state the host
obligation: for `SampleTrigger`, any two consecutive frame steps must sum to at most `period`
(uniform frames: step ≤ `period / 2`); for `Change`, no source may change on two consecutive
frames, which a host cannot promise for a trended binary point. `DVG-000001` (a reset Building
33's valve misses by 0.036 on a 0–1 signal) stays shipped behaviour.

**(f) The first draft's settled value.** It evaluated `s` with every pulse, every sample and every
`Pre`- or edge-of-pulse leaf false, stored `s`, and fired on `(v(k) ∧ ¬s(k−1)) ∨ (¬v(k) ∧ s(k))`.
Three errors, each shown by a row and a leaf set. It drops a sample from the settled value, so it
fires `Not(SampleTrigger)` on every frame (row 16; 44,800 cones). It reads a `Pre` of a level at
its frame value, a frame staler than Modelica's second iteration (rows 17, 18; 156 of 256 cones of
{Change(a), Pre(a), b} and of {Change(a), a, Pre(b)}, where today invents in none). It fires on
non-primitive pulses as if they were new events (rows 19, 20; 180 of 256 cones). Kept here because
the draft PR proposed it.

**(g) The corrected rule.** At load, for each event input, walk the stateless Boolean logic
upstream of it (`And`, `Or`, `Not`, `Nand`, `Nor`, `Xor`, `MultiAnd`, `MultiOr`,
`Logical.Switch`, the five Boolean routing classes, `PassThrough.Boolean`, composite boundaries,
vector elements one by one) to its leaves, and classify each leaf syntactically:

- a **sample**: a `SampleTrigger` output;
- a **primitive pulse**: an output of `Change`, `Integers.Change`, `Edge` or `FallingEdge` whose
  own input cone holds no pulse, no sample and no `Pre`;
- a **fallback leaf**: any `Pre`, and any edge block fed by a pulse, a sample or a `Pre`;
- a **level**: everything else (host inputs, constants, comparators, other stateful blocks,
  conversions).

A cone with a fallback leaf fires by today's rule. Any other cone holding a pulse or a sample fires
by section 1.4's formula and stores `w(k)`. A cone of levels alone is today's rule, unchanged.

- *Sound against the snapped model, exact where it applies.* On every cone of the four primitive
  leaf sets above it fires exactly when, and as often as, snapped Modelica at every frame
  (0 invented, 0 missed, 0 late). In fallback cones it is today's rule, errors included, so it adds
  none there. It is **not** sound against unsnapped Modelica (section 6).
- *Interaction with #768/#789.* Event inputs already feed through; a cone's levels are upstream of
  the input through feedthrough logic, so the shadow evaluation adds no scheduling edge and no
  algebraic loop. `Pre` stays a cut. The `Pre` remedy for feedback into an event input puts a `Pre`
  in the cone, so such an input keeps today's rule and today's documented one-frame delay. The
  `set_param` schedule-shape guard gains no case: the classification is syntactic and reads no
  parameter value (a constant is an opaque level leaf, never folded; `SampleTrigger.period`
  changes when it pulses, not what it is; conditional instances are pruned at load;
  `PIDWithReset.controllerType` is already guarded).
- *Host obligation.* At most one instant per frame interval: for `SampleTrigger`, frame step ≤
  `period` (today it is `period / 2`). For `Change`, at most one change of its input per frame
  interval, which is what frame sampling sees anyway.
- *Cost and hostile input.* Load: one syntactic pass over the Boolean nets, linear in the model,
  with an explicit stack instead of recursion, so a long `Not` chain cannot overflow it under
  `no_std`. Per frame: `w` and `s` for each net in a cone that reaches an event input, evaluated
  once per net, not once per consumer, so fan-out stays linear. `w = s` unless the cone holds a
  sample, so the second evaluation happens only there. Nothing allocates per frame. Most models
  have no cone at all.
- *no_std and firmware.* The shadow pass lives beside the frame driver in `oce_graph::frame` and
  the classification in `oce-graph`'s load path, both `no_std + alloc`; the firmware image grows by
  that code, to be measured against its budget in the implementation PR.
- *Formats.* The blob is unchanged (C6). The state layout is unchanged: the previous-trigger word
  now holds `w`, still a canonical Boolean, with the same bits wherever no pulse or sample is
  upstream. Every `state_contract_revision` stays 1; the CXF document and its content id are
  untouched. The workspace is 0.6.0 after the v0.5.0 tag, so rule 10's bump is done unless v0.6.0
  is tagged before the implementation lands.

## 3. The rule to build

**Build (g)** (decision 1, owner ruling of 2026-11-22). It closes `DVG-000001`, `DVG-000002` and the
`SampleTrigger` freeze; it fixes `Not` of a pulse, `Or` of a pulse and a level, negated samples and
the other mixtures of primitive pulses, samples and levels (rows 2, 4, 5, 11, 16); in the snapped
model it is exact where it applies and nowhere invents an event today's rule does not.

**These shapes fall back to today's rule** (decision 5), with today's errors (rows 6 to 10 and 17 to
21):

- any cone with a `Pre` in it, including the `Pre` that breaks feedback into an event input;
- `Xor(u, Pre(u))` and every other pulse built by hand from `Pre`;
- any cone with an edge block (`Change`, `Integers.Change`, `Edge`, `FallingEdge`) fed by a pulse, a
  sample or a `Pre`.

None of them occurs in the tracked or library documents (section 4). (a′), considered and not
chosen, would have given static labels and no shadow evaluation at the same soundness relative to
today, leaving rows 2, 4 and 11 and negated samples as today. (a) as first written and (f) invent
events.

## Decisions

Decisions 1 to 6 are **owner rulings of 2026-11-22**, each for the option this note recommended,
marked (i); the options not chosen stay below as the record of what was considered. Decisions C1 to
C8 are **this note's own calls**; none of them decides what fires, and the owner can overrule any of
them. The implementation PR follows all fourteen.

1. **Which rule** (owner ruling, 2026-11-22: **(i)**). (i) **(g), the corrected rule**: the gains
   above; every cone it cannot handle keeps today's behaviour. Not chosen: (ii) (a′), primitive
   pulse labels: closes both register entries and the freeze with a static label and no shadow
   evaluation; `Not`, `Or`-with-a-level and negated samples stay as today. (iii) (c), direct wiring
   only: closes both register entries and the freeze, nothing combined. (iv) (e), no engine change:
   document the cadence obligation and keep both divergences open.
2. **The shapes no rule matches** (owner ruling, 2026-11-22: **(i)**; nothing is refused): several
   events in one frame (row 12); the fallback cones of decision 5; duration consumers fed by a pulse
   (`Timer`, `TrueDelay`, `TrueFalseHold`, `TrueHoldWithReset`, `MovingAverage` of a converted
   pulse); and snapping (section 6). (i) **Document them in the execution profile, and warn once per
   `SampleTrigger` instance, the first time a frame spans more than one of its instants**: a
   host-cadence violation the engine can see, reported once, not every frame. Not chosen: (ii)
   document only. (iii) Also warn at load for each fallback cone and each pulse into a duration
   consumer: every `Pre` remedy would warn, and so would the 11 library fault rules that count
   one-frame pulses with a moving average. (iv) Refuse them at load: the `Pre` remedy and those 11
   rules would stop loading.
3. **Frame 0, per class** (owner ruling, 2026-11-22: **(i)**). (i) **Fire only a sample's instant at
   frame 0:**
   - `OnCounter`, `TriggeredSampler`, `TriggeredMax`, `IntegratorWithReset`, `PIDWithReset` and
     `TimerAccumulating.reset` evaluate the formula at frame 0 with *before* = `w(0)` (pulses and
     samples false, frame-0 levels), every `Change`, `Integers.Change`, `Edge` and `FallingEdge`
     leaf false in *during* and *settled* (their frame-0 value is initialisation, which fires
     nothing in OpenModelica), and each sample leaf true only if `t(0)` is one of its instants. Only
     a sample fires at frame 0; `Not(SampleTrigger)` does not, and neither does `Not(Change(u))`
     with `u(0) ≠ pre_u_start`, which a gate on the first term alone would let fire through its
     second. A fallback cone keeps today's "no event at frame 0".
   - `TriggeredMovingMean` samples at frame 0 as today (`when {initial(), trigger}`).
   - `Latch` and `Toggle` keep their frame 0 as today: their initial equations read `u`'s frame-0
     value, the initialisation pulse of a `Change` included, as OpenModelica confirms.
   - `SampleTrigger`'s own frame-0 output becomes true only when `t(0)` is one of its instants.
     Today it is true at any `t(0) ≥ phase`, so `Pre(SampleTrigger)` fires at frame 1 after an
     off-grid start, where Modelica has no instant.
   - Every class stores `w(0)` after frame 0.

   Not chosen: (ii) keep #768's "no event at frame 0" for every class and `SampleTrigger`'s
   frame-0 output as it is. A sampler behind a `SampleTrigger` then shows `y_start` until the
   second instant, and the off-grid `Pre(SampleTrigger)` event at frame 1 stays; both are
   documented.
4. **`Latch.u` and `Toggle.u`, so G36 `TrimAndRespond`, in M3** (owner ruling, 2026-11-22: **(i)**),
   with decision 3's per-class frame 0. (i) **Include them with the other event inputs.** No tracked
   trace moves: all three fixtures that hold the block run `have_hol = false`, where `clr` equals
   `u` and the latch is false whatever the rule, at frame steps of 1 s and 60 s against a 120 s
   `samplePeriod`, starting on the sample grid. With `have_hol = true` at a frame step above 60 s,
   the latch re-latches at each sample as OpenModelica does (`lat.y` in `om/results.txt`). Not
   chosen: (ii) leave them on the rising edge until a G36 run with `have_hol = true` exists as
   evidence.
5. **Cones with a `Pre`, or an edge block fed by a pulse, a sample or a `Pre`** (owner ruling,
   2026-11-22: **(i)**). (i) **Fall back to today's rising edge**: no new error, and rows 6 to 10
   and 17 to 21 stay as today. Not chosen: (ii) compute edge blocks from the three values (`Edge` of
   a cone as `(v ∧ ¬w(k−1)) ∨ (s ∧ ¬v)`, and likewise `Change` and `FallingEdge`): rows 8, 9 and 20
   and `Change(Change(u))` come out right, but row 21 invents an event at every change (6 against
   OpenModelica's 0), because a first-iteration and a second-iteration pulse are true on the same
   frame; and `Pre` cones still need the fallback, since the engine shows `Pre`'s settled value a
   frame late. Only option (d) does better.
6. **`TimerAccumulating.reset`** (owner ruling, 2026-11-22: **(i)**). (i) **In**: it is a
   `when reset` event input, and OpenModelica resets on each of consecutive `Change` pulses where
   the engine resets on the first only. Not chosen: (ii) out: keep it on the rising edge and
   document it beside the duration consumers.

- **C1 (this note's call). The cone and its leaves** are as listed under (g), classified by a linear
  syntactic walk with an explicit stack. "No pulse behind it" is a structural property of the graph,
  never a satisfiability question; a constant is an opaque level leaf.
- **C2 (this note's call). Event inputs are declared per class.** Today `Block::event_input_role` is
  declared only on `OnCounter`, `PIDWithReset` and `IntegratorWithReset`, for the algebraic-loop
  diagnostic. The consumers of C4 need their own declaration of which inputs take the formula and
  which frame-0 rule of decision 3 they follow.
- **C3 (this note's call). The grid test.** "`t(0)` is an instant" uses the block's existing index
  function, which rounds within `1e-9 · period`, early only; Modelica rounds `t0` itself to 1e-6 s.
  The two agree at `t(0) = 0` with `shift` a multiple of 1e-6 s, the case every test and fixture
  uses; the execution profile says so.
- **C4 (this note's call, following decisions 1, 4 and 6). Consumers that take the formula:**
  `OnCounter.trigger` and `.reset`, `TriggeredSampler`, `TriggeredMax` and `TriggeredMovingMean`
  `.trigger`, `IntegratorWithReset.trigger`, `PIDWithReset.trigger` (PI and PID), and, by decisions
  4 and 6, `Latch.u`, `Toggle.u` and `TimerAccumulating.reset`. Unchanged: `Latch.clr`, `Toggle.clr`
  (level-equivalent, section 1.2), `TrueHoldWithReset` (engine-local) and the duration consumers.
- **C5 (this note's call). The stored word holds `w(k)`**, the cone with pulses and samples false:
  one word, as today.
- **C6 (this note's call). No blob bytes change.** The classification is derived from the wiring by
  one `oce-graph` function that both the loader and the blob decoder run, as the decoder already
  re-derives and checks emit-before order. Blobs and snapshots written by a 0.6.0 build before the
  change load under the new rule; the implementation PR says so, as GitHub #789 did. A restored
  pre-change snapshot holds `v(k)` where `w(k)` is now expected. The two differ only when a pulse or
  sample was true at the snapshot's frame, and then the first frame after restore shows at most one
  event merged, or one shown a frame late as the old rule would have shown it; none is counted
  twice. If v0.6.0 is tagged first, the PR bumps the version and adds the refusal test rule 10 asks
  for.
- **C7 (this note's call). The warning of decision 2** is a `Warning` diagnostic in the frame's
  receipt, once per `SampleTrigger` instance per load.
- **C8 (this note's call). One PR**, closing #800, with the register entries resolved in it.

### Implementation acceptance

What the implementation PR owes, derived from the rulings; section 5 has the full test plan.

- **The rule, per class.** Each consumer of C4 fires by section 1.4's formula on the cones (g)
  applies to and by today's rising edge on every fallback cone (decision 5), with all eight
  `(w(k−1), v(k), s(k))` combinations tested by hand-derived vectors.
- **Frame 0, per class** (decision 3), with a mutant per class that must die: the when-only classes
  fire at frame 0 only on a sample instant (`Not(SampleTrigger)` and `Not(Change(u))` with
  `u(0) ≠ pre_u_start` do not; the mutant that gates only the first term fires on the second);
  `TriggeredMovingMean` samples as today; `Latch` and `Toggle` read the initialisation pulse;
  `SampleTrigger`'s own frame-0 output is true only on its grid (the mutant that drops the grid test
  lets `Pre(SampleTrigger)` fire at frame 1 after an off-grid start).
- **The warning** (decision 2): a test that a frame spanning more than one instant of a
  `SampleTrigger` emits one `Warning`, that later such frames emit none for that instance (C7), and
  that a frame spanning one instant emits none. The execution profile documents every shape of
  decision 2; nothing is refused at load.
- **OpenModelica pilots against the pinned Buildings classes**, not transcriptions, run by
  `tools/openmodelica-event-reference/events/regenerate.sh` with provenance: the shapes of section
  1.3, `SampleTrigger` into `OnCounter` and `TriggeredSampler` (at frame 0 too),
  `Not(SampleTrigger)` into `TriggeredSampler`, `Latch` and `Toggle` at frame 0, `TimerAccumulating`
  reset by consecutive pulses, and `TrimAndRespond`'s latch with a hold. The engine equals
  OpenModelica at every frame on the rows (g) claims; every other row is registered as a known
  divergence.
- **The register.** `DVG-000001` and `DVG-000002` move to `resolved`, kept with their evidence
  files, with a disposition naming the PR and the passing comparison; the tests that pin today's
  merging (`open_modelica_event_pulses.rs`, `sample_trigger_event_inputs.rs`,
  `building_33_openmodelica_reenable.rs`, `building_33_openmodelica_frames.rs`) are rewritten.
- **Corpus byte identity.** The 46 fixture goldens, 422 provenance goldens, 46 G36 traces, the blobs
  and their run digests and the replay traces are byte-identical (section 4); Building 33's oracle
  moves from 382 s on and nothing else.
- **Firmware size.** The growth of the firmware image from the classification and the shadow pass is
  measured and stated against its budget.
- **Snapshot restore.** The PR states what C6 says a pre-change snapshot does at the first frame
  after restore, and tests it between two consecutive pulses.

## 4. Blast radius

Measured by the static scan and the second probe before any implementation, and corrected after
the implementation ran them (two rows, marked; see [As landed](#as-landed)).

| Corpus | Pulse into a consumer | Moves under (g) |
| --- | --- | --- |
| Building 33 (`crates/leverage/oce-verify/tests/fixtures/commissioning/building33/`) | `Change(enable)` → `PIDWithReset.trigger`, direct | Yes: `valve_oracle.txt` at 387 and 392 s (382 s reads 0 under both rules), `derive_valve.py`, and the two OpenModelica tests' claims ("eight of nine" resets becomes nine of nine); `yCooVal_reenable_probe.engine.csv` stays as the resolved divergence's evidence (corrected) |
| G36 `cooling_only_controller`, `generic_time_suppression` (and `oce-author`'s emitted `generic_time_suppression.cxf.json`) | `Edge` → two `TriggeredSampler.trigger`, direct; `Edge` → `Latch.clr` | No: an `Edge` is never true on consecutive frames, so `w ≡ s ≡ false` and today's edge give the same bits; `clr` is unchanged (C4) |
| G36 `multizone_vav_freeze_protection` | `Or(tim.passed, Edge(endStaTwo))` → `lat.u`; `Edge` → `lat.clr`, `lat2.clr` | No: the probe ran the 111 trace frames and `endStaTwo` never pulses |
| G36 `multizone_vav_supply_fan`, `multizone_vav_supply_temperature`, `trim_and_respond_have_hol_false` | `SampleTrigger(120 s)` → `Latch.u`, direct; `And(Not(false), SampleTrigger)` → `Latch.clr` | No output: `have_hol = false`, frame steps of 1, 1 and 60 s, and runs that start at t = 0 on the grid. Their blob run digests move (next row) |
| The other 40 G36 fixtures, the CDL fixtures, `tools/golden-gen` goldens | none | No |
| `crates/oce-api/tests/fixtures/library_rules/AHU-0004/rule.cxf.jsonld` and its vectors, `crates/leverage/oce-verify/tests/fixtures/canonical/AHU-0004.json` | `Integers.Change` → `BooleanToInteger` → `IntegerToReal` → `MovingAverage` | No: no event input |
| Compiled-schedule blobs, `blob_frame_digests.tsv`, `class_signatures.tsv` | — | Blob bytes and `class_signatures.tsv`: no (C6; no `state_len` changes). `blob_frame_digests.tsv`: **three lines move** (corrected), because those digests fold the state words after every frame, not only the outputs: `multizone_vav_supply_fan`, `multizone_vav_supply_temperature` and `trim_and_respond_have_hol_false`, whose `Latch` now stores `w` (false for a direct sample) where it stored `v`, and whose `SampleTrigger` word carries the warning mark |
| `oce-replay` traces (`controller_minute_day`, `controller_grid_hold`) | `cooling_only_controller`'s `Edge` → `TriggeredSampler` | No |
| `open-control-library` at fe7060a: 140 documents | 11 fault rules (AHU-0004, CHW-0009, FCU-0001, HP-0002, HW-0001, HW-0011 ×2, PMP-0004, RTU-0001, TOWER-0003, VAV-0005, VFD-0004 ×2): `Change`, `Integers.Change` or `Edge` → `BooleanToInteger` → `IntegerToReal` → `MovingAverage` → `MultiplyByParameter` → `GreaterThreshold` | No: no event input; they count one-frame pulses, which their cards document, and a consecutive change counts twice, as Modelica counts events |
| Engine tests that pin today's merging | `open_modelica_event_pulses.rs`, `sample_trigger_event_inputs.rs`, `building_33_openmodelica_reenable.rs`, `building_33_openmodelica_frames.rs` | Rewritten to the new rule; `DVG-000001` and `DVG-000002` move to `resolved` |

No tracked or library document has a cone that falls back, so decision 5 moves nothing in the
corpus. The 46 fixture goldens, 422 provenance goldens, every G36 trace and every blob output are
byte-identical; the implementation PR ran them. Docs that change with it:
the execution profile's "Events on a Boolean input" and "Conformance boundary",
`docs/cdl-coverage.md`, `docs/verification-evidence.md`, `TESTING.md`'s line on
`open_modelica_event_pulses.rs`, the Building 33 README, the rustdoc of `SampleTrigger` and of the
consumers that cite issue #800 (`blocks/discrete.rs`, `blocks/integers_edge.rs`, `blocks/pid.rs`,
`blocks/reals_integrator.rs`), and `CHANGELOG.md`.

## 5. Test and evidence plan

Per [TESTING.md](../../TESTING.md):

- **The formula, exhaustively.** For each consumer in C4, all eight `(w(k−1), v(k), s(k))`
  combinations at frames 1 and 2, with expectations derived by hand from section 1.4.
- **The simulator as an oracle.** `simulate.py`'s brute force over the four primitive leaf sets,
  turned into committed engine vectors for a sample of cones (every `Not`, `And`, `Or` and `Xor`
  shape of two leaves at least), with the snapped Modelica count per frame as the expectation.
- **The shapes.** Every row of section 1.3 as a committed engine vector, fallback rows included and
  pinned to today's values; plus `Nand`, `Nor`, `Logical.Switch` with a pulse on each data input
  and on the selector, `MultiAnd` and `MultiOr` of mixed widths, a pulse through a composite
  boundary and through a vector element, one pulse wired to two consumers, and a `Pre` remedy on a
  feedback path into each consumer (today's behaviour, unchanged).
- **Edge cases.** A pulse true on every frame; a pulse at frame 1; a pulse and a level rise at the
  same frame; a level falling at the frame of a pulse (row 4 at 8 s); every frame-0 case of
  decision 3, per class, `Not(SampleTrigger)` and `Not(Change(u))` with `u(0) ≠ pre_u_start`
  included; a `SampleTrigger` exactly on, a hair before and a hair after its frame-0 instant; frame
  steps above `period` (the decision 2 warning, once); repeated equal timestamps; snapshot and
  restore between two consecutive pulses; `set_param` on `SampleTrigger.period` leaving the
  classification as it was; a 100,000-block `Not` chain loading without stack growth.
- **Unchanged where nothing is upstream.** The whole G36 corpus, the CDL fixtures, the blob round
  trip and the replay traces byte-identical: the evidence that `w = s = v` everywhere else.
- **OpenModelica pilots**, added to `tools/openmodelica-event-reference/events/` and run by
  `regenerate.sh` against the pinned Buildings and MSL commits, not transcriptions: rows 1 to 21
  into `OnCounter`; `SampleTrigger` at 0.5, 1, 1.4 and 2 s into `OnCounter` and `TriggeredSampler`;
  `Not(SampleTrigger)` into `TriggeredSampler`; `Latch` and `Toggle` at frame 0;
  `TimerAccumulating` reset by consecutive pulses; and `TrimAndRespond`'s latch with a hold. The
  engine must equal OpenModelica at every frame on the rows (g) claims; every other row is
  registered as a known divergence with its evidence. `DVG-000001` and `DVG-000002` move to
  `resolved` with a disposition naming the PR and the passing comparison, their evidence files
  kept.
- **Mutants that must die:** firing on `v` alone (a held level re-fires); storing `v` instead of
  `w` (consecutive pulses merge again); dropping the `¬v ∧ s` term (row 2); evaluating `s` with
  samples false (row 16 fires); using last frame's levels in `s` (row 4 at 8 s); applying the
  formula to a cone with a `Pre` (rows 17, 18) or with an edge block of a pulse (rows 19, 20);
  gating only the first term at frame 0 (`Not(Change(u))` fires at frame 0); taking the frame-0
  grid test without its tolerance.
- **Determinism.** The new vectors and the unchanged corpus repeat bit for bit in the four native
  cells; the PR touches `oce-blocks` and `oce-graph` and runs the determinism lane.
- **Unverified by any tool, said plainly.** The OpenModelica runs of this note use transcribed
  equations. The simulator's exactness results cover four leaf sets of up to four leaves and
  sequences of five or six frames, not cones of any size; the argument of section 1.4 covers the
  rest. `Toggle`, `TriggeredMax`, `TriggeredMovingMean` and the reset integrators were not run in
  OpenModelica behind a pulse; the simulator models every consumer as a counter's `when`. `Integer`
  pulses (`Integers.Change` of a `BooleanToInteger` of a pulse) are untraced.

## 6. What this does not fix

- **Snapping.** Every rule here, today's included, treats the events inside a frame as if they all
  happened at `t(k)`. Unsnapped Modelica can differ: `And(Change(a), b)` with `b` rising after `a`
  changes inside the same frame fires in the engine and never in Modelica. (g) is sound only
  against the snapped model, and it adds such cases. Over {Change(a), Change(b), a, b}, with every
  order of two changes inside a frame and 5-frame sequences, today and (g) each invent an event on
  some sequence in the same 12,474 cones, and (g) invents on a sequence where today does not in
  11,786 of them (`results.txt`). No host cadence removes this; only frames short enough that no
  two sources change inside one frame do. The review counted the second figure as 8,635 cones and
  the invented (cone, sequence) pairs as 798,720 for (f) against 563,200 for today; this note's
  counts are 11,786 cones and 11,222,912 against 6,340,928 pairs. Both count the same 12,474
  cones in which both rules invent, and both find (g) inventing more than today. This note's
  enumeration: every sequence of five frames of `a` and `b` from a settled frame 0; in each frame
  where both change, three orders (`a` first, `b` first, together), each frame's choice
  independent; and the matching above, which counts a late fire as late, not invented.
- **Several events in one frame** count once: a `SampleTrigger` at half the frame step counts 9 of
  OpenModelica's 18 instants (row 12), and a source that changes twice between frames is not seen.
  The host obligation is one instant per frame interval.
- **`Pre` and edge blocks inside one instant** (rows 6 to 10, 17 to 21) keep today's errors under
  the fallback. Modelica's `Pre` is an event-iteration delay, the engine's a frame; only a new
  profile (option (d)) fixes them.
- **Duration consumers** read a pulse as lasting a frame: `TrueDelay`, `Timer`, `TrueFalseHold`,
  `TrueHoldWithReset` and a `MovingAverage` of a converted pulse see one frame step of `true` where
  Modelica sees none. The library's 11 counting rules rely on exactly that and keep working.
- **What the engine shows on a pulse output** is unchanged: `Change` emits `true` for the whole
  frame, where OpenModelica's last row at the instant shows `false`.

## Evidence that can be rerun

- `python3 -I docs/design/pulse-event-inputs/simulate.py` prints
  `docs/design/pulse-event-inputs/results.txt` (about 15 s, standard library only).
- `sh docs/design/pulse-event-inputs/om/run.sh <dir>` reruns the four OpenModelica probes into a
  directory, and `python3 -I docs/design/pulse-event-inputs/om/fires.py <dir>` prints what
  `om/results.txt` holds.

## As landed

GitHub #PRNUM implements option (g) with the six rulings of 2026-11-22 in one PR (C8). Where the
implementation found the note wrong, read it more narrowly or had to decide something it left
open, the record is here; the decided text above is unchanged, except section 4's two corrected
rows.

- **Taint is any chain of inputs: a conservative reading of decision 5.** Section 3 sends to the
  rising edge "any cone with an edge block fed by a pulse, a sample or a `Pre`". The plan marks
  every block that a pulse, sample or `Pre` reaches through any chain of inputs, Boolean or not
  (`oce_graph::events`, one breadth-first pass over a reader table built once). A leaf of a cone
  that is so marked is a fallback leaf, so `Integers.Change(BooleanToInteger(Change(u)))`, and a
  level such as `GreaterThreshold(BooleanToReal(Change(u)))`, send their cone to the rising edge
  instead of being read as a primitive pulse or a level. A primitive pulse is an edge block whose
  own input is untainted. No tracked or library cone is affected (section 4).
- **`TriggeredMovingMean` samples twice at a sample instant at the start: a refinement of decision
  3 found by the pinned-class pilots.** Decision 3 says it "samples as today", once, for
  `initial()`. `CDL/Discrete/TriggeredMovingMean.mo:36` is `when {initial(),trigger} then`; with
  `trigger = sample(...)` true at the start, OpenModelica fires the `when` for `initial()` and again
  in the start's event iteration (the second sample is taken when `trigger` becomes true after
  initialisation: `s and not w` at the initial frame). The engine now does the same, and the
  sample-trigger pilot agrees (2.8333 at 1 s, which one sample at the start does not give); it is
  not registered as a divergence. A `Change` or `Edge` leaf true from the start, whose pulse is
  initialisation's, adds no second sample, and an off-grid start adds none. The other classes,
  checked against their `.mo` and the pilots: `TriggeredMax` applies `max(pre(y), abs(u))` to its
  initial `y = u` at an instant at the start, so a negative `u` gives `abs(u)` (OpenModelica 2.5
  for `u = -2.5`); #789 pinned `y(0) = u(0)` for a level trigger true at the start, which is no
  event, so the two do not contradict. `OnCounter` counts 1, `TriggeredSampler` samples,
  `IntegratorWithReset` and `PIDWithReset` reset, `Toggle` and `Latch` read `true` and
  `TimerAccumulating`'s reset shows 0 either way, each as OpenModelica does.
- **The stored word is read only by the event test.** Each consumer stores `w` in the word that
  held the previous input: `OnCounter` words 1 and 2 (trigger, reset), `TriggeredSampler`,
  `TriggeredMax` and `TriggeredMovingMean` word 1, `IntegratorWithReset` word 2, `PIDWithReset`'s
  trigger word (PI and PID only), `Latch` and `Toggle` word 1, `TimerAccumulating` word 3. In
  every class that word is read only by `fire(...)`, the event test, never by an output, by
  `event_input_role` or by another state word's update; each validator still requires `0` or `1`
  there, which `w` satisfies.
- **`SampleTrigger`'s warning mark lives in its state word.** C7's once-per-instance warning must
  survive capture and restore, and the class has one word and no spare (a second would change
  `state_len`, which the blob's class signature fixes); `MovingAverage`'s coalesced-warning word
  is the precedent for a warning mark in state. The word holds the last instant index `k` as
  before (`-1` before the first instant), or `-(k + 2)` once the span warning fired (`k >= 1`, so
  the two ranges cannot meet). The index is recovered exactly, so sampling is the same with or
  without the mark (`sample_trigger_event_inputs.rs`); `validate_state` accepts exactly the index the
  state's model time gives, warned or not, and refuses with the typed state error any other
  index and a warned index below 1 (the word `-2`); decoding and encoding use no unchecked
  arithmetic, `i64::MIN` and `i64::MAX` included. A snapshot captured after the warning restores as already warned, one
  before it as not yet; a 0.6.0 snapshot from before this change holds a plain index and restores
  as not yet warned (`crates/oce-api/tests/settled_event_state.rs`, with the snapshot captured at
  56f93247 committed as `fixtures/state_before_settled_rule.hex`). `set_param` on `period` or
  `shift` is accepted (it changes neither the plan nor the signal); `resume` re-seeds the word.
- **Section 4 was wrong about the blob run digests.** `blob_frame_digests.tsv` folds the state
  words of every frame as well as the outputs, so three lines move (the corrected row). The proof
  that no output moved is now committed: `blob_frame_output_digests.tsv`, the outputs alone over
  the same 160-frame schedule (steps 0.75, 1, 15, 60 and 300 s), recorded at 56f93247 and equal
  on this change for all 46 fixtures (`blob_execution.rs`). Blob bytes, initial-state CRCs,
  content ids, class signatures, the replay traces and the strict-bit and determinism fixtures do
  not move.
- **A pre-change snapshot (C6).** Its stored words hold the frame value `v` where `w` is now
  expected. Between two consecutive pulses that is a stored `true` where `w = false`: the pulse
  after the restore merges with the one before it, as the old rule did, and none is counted twice
  (`[1, 1, 2]` against `[3, 3, 4]` from the start in the test above); from the next frame the
  words are this build's.
- **The decoder.** `decode` derives the plan from the wiring right after `build_order`, the last
  of its validation steps, so a hostile blob is refused before any plan buffer exists. The plan is
  thirteen buffers, each sized once from the model's counts, allocated only when some block
  declares settled inputs and some block is a pulse or a sample; a 1x, 2x and 4x generated model
  allocates thirteen at each size, with bytes exactly linear (`blob_allocations.rs`), and a
  100,000-deep `Not` chain derives without recursion.
- **The hold latch.** Decision 4's evidence ran on transcriptions; the sample-trigger pilot now
  runs `TrimAndRespond`'s latch on the pinned `Latch`, `And`, `Not` and `SampleTrigger`, with a
  table column standing for `truHol.y`, and the engine agrees at every frame (it latches at the
  two held instants, which the rising edge never did).
- **Firmware.** The `blob-frames` image grows by 5,736 octets of `.text` on `thumbv7em-none-eabihf`
  (146,396 to 152,132, budget 161,760) and 6,976 on `riscv32imac-unknown-none-elf` (171,820 to
  178,796, budget 189,664), with about 2.1 KB of `.rodata`; no budget changes.
