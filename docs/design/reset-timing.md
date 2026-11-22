# Reset timing at the event

**Design note, written before any code (GitHub #768).** The owner ruled on 2026-11-22: "fix both
to match Modelica". A reset of `CDL.Reals.PIDWithReset` takes effect at the frame where the
trigger rises, not one frame later, and no reset fires at frame 0 when the trigger is already true
at start. This note says how far each difference extends across the registered classes, states the
rule once for the HostTick v1 profile, names the references that check it outside the engine,
measures the blast radius, and lists what the ruling leaves open. The
[execution profile](../execution-profile.md) stays the authority for shipped behaviour until the
implementation PR changes it.

Paths below are repository-relative. `CDL/` abbreviates the vendored
`third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/`; `blocks/` abbreviates
`crates/oce-blocks/src/`. Engine line numbers are at `development` 84db2d76.

## How the evidence was gathered

- **Sources.** Every vendored CDL class containing `when`, `reinit`, `pre(`, `initial()`, `edge(`,
  `change(` or `sample(` (39 files) was read, and each class with an event on a Boolean input was
  paired with its engine implementation.
- **Modelica semantics.** Modelica Language Specification 3.6. §8.3.5: a when-equation is
  "activated only at the instant when the scalar expression or any of the elements of the vector
  expression becomes true". §8.6: when-clauses "are active during initialization, if and only if
  they are explicitly enabled with initial()", "only in one of the two forms when initial() then
  or when {…, initial(), …} then"; for an inactive one, "the equation v = pre(v) is added for
  initialization"; for a discrete-time variable with `fixed = true`, "the equation
  pre(vd) = startExpression is added to the initialization equations"; after initialisation
  "pre(vi) := vi must be set and an event iteration at the initial time must follow". §3.7.5:
  `edge(b)` "expands into (b and not pre(b))", `change(v)` "expands into (v <> pre(v))", "the
  first value of pre(y) is determined in the initialization phase". §8.3.6: "reinit can only be
  used in the body of a when-equation".
- **Probe.** A scratch program (not committed) loaded one-block and two-block CXF documents through
  `oce_api::Engine::load_cxf` and ran frames at t = 0, 1, 2, … s. The "engine" columns below are
  its output at 84db2d76. A scratch implementation of the rule in section 2 was then run through
  the same program (it reproduced every "rule" column) and through the `oce-blocks`, `oce-graph`,
  `oce-api`, `oce-conformance` and `oce-verify` suites (2,758 tests) to measure section 4. Nothing
  from either is committed.

## 1. Current behaviour, per class

| Class | Modelica at the event | Modelica at initialisation | Engine at the trigger's frame | Engine at frame 0, trigger true | Differs |
| --- | --- | --- | --- | --- | --- |
| `Reals.PIDWithReset` (PI, PID) | `I` is `IntegratorWithReset` with `reinit(y, y_reset_in)`, `y_reset_in = y_reset − (P + D)` (`CDL/Reals/PIDWithReset.mo:67-70, 160, 186, 213, 226`), so `y = lim(y_reset)` at the instant | `when` inactive; `I.y = xi_start` (`IntegratorWithReset.mo:22-23`) | emits from the pre-reset `I` (`blocks/pid.rs:259-266`); the reset is written by `update_state` (`:293-303`); `trigger` does not feed through (`:424-425`). Probe, `e = 1`, `y_reset = 5`, rising at t = 2: **1, 1, 2, 5, 6**; rule **1, 1, 5, 5, 6** | resets: the trigger word starts false (`:444`). Probe, true from t = 0: **1, 5, 6, 7**; rule **1, 1, 2, 3** | both |
| `Reals.IntegratorWithReset` | `reinit(y, y_reset_in)` (`IntegratorWithReset.mo:27-29`) | `y = y_start` (`:22-23`), no reinit | emits the prior accumulator (`blocks/reals_integrator.rs:84`), resets in update (`:95`), feeds through from nothing (`:63-65`). Probe, `u = 1`, `y_reset_in = 10`, rising at t = 2: **0, 0, 1, 10, 11**; rule **0, 0, 10, 10, 11** | resets (`:74`; the doc comment at `:29-32` calls this a deliberate choice). Probe: **0, 10, 11, 12**; rule **0, 0, 1, 2** | both |
| `Integers.OnCounter` | `y = if reset then y_start else pre(y) + 1` at the instant (`CDL/Integers/OnCounter.mo:20-25`) | `pre(y) = y_start` (`:16-17`), `when` inactive | emits the prior count (`blocks/integers_edge.rs:83`); neither input feeds through (`:61-63`). Probe, trigger rising at 1 and 3, reset at 5: **0, 0, 1, 1, 2, 2, 0**; rule **0, 1, 1, 2, 2, 0, 0** | no event (`:95`, `has_history`). Matches | event only |
| `Discrete.TriggeredSampler` | `y = u` (`CDL/Discrete/TriggeredSampler.mo:20-22`) | `y = y_start` (`:16-17`); no `pre(trigger)` initial equation | same frame (feeds through, `blocks/discrete.rs:218-220`). Matches | samples `u` (`:228`; doc `:179-180` claims "the CDL source initializes pre(trigger) to false", which the source does not). Probe, `y_start = 7`, `u = 3, 4, 5, 6`, trigger T, T, F, T: **3, 3, 3, 6**; rule **7, 7, 7, 6** | frame 0 only |
| `Discrete.TriggeredMax` | `y = max(pre(y), abs(u))` (`TriggeredMax.mo:18-22`) | `y = u` (`:14-15`) | same frame (`blocks/discrete.rs:296-298`). Matches | applies the event to the initial value, `max(u, abs(u))` (`:272-273`). Probe, `u = −2`: **2, 2, 2**; rule **−2, −2, −2**; `u = −0.0`: **0.0**; rule **−0.0** | frame 0, when `u < 0` or `u` is `−0.0` |
| `Discrete.TriggeredMovingMean` | sample, `when {initial(), trigger}` (`TriggeredMovingMean.mo:36-53`) | the same clause, active: one sample | samples at frame 0 once (`blocks/discrete.rs:365-369`). Probe, `n = 2`: **2, 2, 2, 5** | as Modelica | no |
| `Logical.Latch` | `y = not clr and u` (`Latch.mo:17-19`) | the same expression (`:14-15`) | `!clr && ((u && !pre_u) \|\| held)` (`blocks/logical_latch.rs:132`) | equals the initial equation | no |
| `Logical.Toggle` | toggles (`Toggle.mo:23-45`) | `when initial()`: `y = if clr then false else u` (`:20-22`) | `blocks/logical_latch.rs:191` | equals the initial clause | no |
| `Logical.Timer`, `Logical.TimerAccumulating` | `when u` / `when reset` (`Timer.mo:31-45`, `TimerAccumulating.mo:42-61`) | `pre(entryTime) = time`, `pre(passed) = t <= 0`, `yAcc = 0` (`Timer.mo:26-28`, `TimerAccumulating.mo:33-38`) | same frame (feed through) | treats a true input as an edge (`blocks/logical_timing.rs:57-75, 163-182`), which yields exactly the initial values: `y = 0`, `passed = t <= 0` | no |
| `Logical.TrueDelay`, `Logical.TrueFalseHold` | edge clauses | `when initial()` (`TrueDelay.mo:34-36`, `TrueFalseHold.mo:48-51`) | same frame | first-tick branches (`blocks/logical_timing.rs:259, 356`), by inspection, not probed | no |
| `Reals.Ramp` | `reinit(y_internal, u)`, `y = if active then y_internal else u` (`Ramp.mo:43-52`) | `y_internal = u` (`:40`) | same frame (`blocks/reals_ramp.rs:84-85`). Probe: **0, 0, 10, 10** | `y_internal = u`, as Modelica | no |
| `Logical.VariablePulse` | internal `when go then t0 = time` (`VariablePulse.mo:114-116`) | `pre(t0) = time` (`:110-111`) | anchors on a width change in the same frame (`blocks/logical_variable_pulse.rs:76`) | anchors `t0` at the first frame (`:69`), the same value. By inspection | no |
| `Logical.Change`, `Edge`, `FallingEdge`, `Integers.Change` | no `when`; `change(u)` / `edge(u)` equations | `pre(u) = pre_u_start` (`Change.mo:6-8` by `fixed = true`; `Edge.mo:13-14`; `FallingEdge.mo:17-18`; `Integers/Change.mo:6-8`) | current against prior (`blocks/logical_latch.rs:112`, `blocks/logical.rs:434`) | `y = (u ≠ pre_u_start)`. Probe, `Change`, `u` true, `pre_u_start` false: **true, false, …** | no (section 2) |

Out of scope, with reasons: `Logical.Pre` is the documented HostTick projection of `pre()`
([execution profile](../execution-profile.md#cdllogicalpre)). `Discrete.Sampler`, `ZeroOrderHold`,
`FirstOrderHold`, `UnitDelay`, `Logical.Sources.SampleTrigger`, `Sources.Pulse`, `Integers.Stage`
and `Reals.MovingAverage` fire on time events (`sample(…)`, `time >= …`), not on a Boolean input.
`Reals.PID` ties its integrator's trigger to a constant false (`PID.mo:62, 191`).
`Logical.TrueHoldWithReset` has no vendored source and a level-sensitive `clr`.

**How far each difference extends.** The next-frame event is in three classes: `PIDWithReset`,
`IntegratorWithReset` and `OnCounter`, the three stateful blocks whose event input is declared a
loop cut. The frame-0 event is in four: `PIDWithReset`, `IntegratorWithReset`, `TriggeredSampler`
and `TriggeredMax`, the four that start their previous-trigger memory at `false` without a
first-frame guard. In the Building 33 derivation the next-frame event shows at four frames, not two:
25, 237, 272 and 307 s (section 3). The frame-0 reset is not invisible when `y_reset = xi_start`:
the integrator is back-solved to `y_reset − (P + D)`, so it moves whenever `P + D ≠ 0` at frame 0.

## 2. The rule (HostTick v1)

**Event at the frame.** Frame `k > 0` at time `t_k` is the event instant of every Boolean event
condition that is true at frame `k` and was false at frame `k − 1`. The outputs of frame `k` are
the values after the event: a block whose `when` fires at frame `k` emits from the reinitialised
or re-assigned state, computed from frame `k`'s inputs. This is the value OpenModelica prints last
at that instant, which is what `extract_day.py` keeps. For the integrators, frame `k`'s state update
writes the reset value with no Euler step: under the documented tick convention the step taken at
frame `k` covers `[t_(k−1), t_k]`, which lies before the event, so the state trajectory from frame
`k + 1` on is the one the engine already computes. Only the emission at frame `k` changes, and with
it everything downstream of that emission.

**Frame 0 is initialisation.** At frame 0 a `when` fires only if its condition holds `initial()`
(`TriggeredMovingMean`, `Toggle`, `TrueDelay`, `TrueFalseHold`, the sampled family); every other
block emits its initial-equation values, or `pre(v)` where nothing defines `v`. The value an event
input has at frame 0 becomes its `pre` value for frame 1, so a trigger true at frame 0 and still
true at frame 1 is not an event at frame 1. Equations that are not when-clauses are active at
initialisation, with `pre(u)` given by the start value. So `Change` with `u` true and
`pre_u_start` false emits **true** at frame 0: `change(u) = (u <> pre(u)) = (true <> false)`, with
`pre(u) = pre_u_start` added because `u` is declared `fixed = true, start = pre_u_start`
(`Change.mo:6-8`, §8.6). Modelica then runs the initial event iteration with `pre(u) := u`, after
which the same equation gives false; HostTick has one frame at `t_0` and keeps the initialisation
value (decision 6). A downstream reset never sees that pulse as an event: at initialisation because
the `when` is inactive, and at frame 1 because the pulse has fallen.

**Evaluation order.** To emit the post-event value, the block must read this frame's event input at
emission, so the input must be declared direct feedthrough: `PIDWithReset` `trigger → y` in the
modes with an integrator; `IntegratorWithReset` `trigger → y` and `y_reset_in → y` (its `u` stays a
cut); `OnCounter` `trigger, reset → y`. `TriggeredSampler` and `TriggeredMax` already declare it.
This is required, not cosmetic: a non-feedthrough input is read only by the deferred update and
imposes no emit-before edge (`crates/oce-graph/src/topo.rs:186-198`), so reading it at emission
would see last frame's value or this frame's depending on schedule order. The change adds one
internal edge per input in `build_feedthrough_dag` (`crates/oce-graph/src/build.rs:84-92`) and one
emit-before constraint the blob decoder checks (`crates/oce-graph/src/blob/decode.rs:553-560`).
Consequences:

- **Feedback into an event input becomes a loop.** `y → GreaterThreshold → trigger` on a PI reset
  controller, a self-resetting `OnCounter`, or an integrator whose output reaches its own
  `trigger` or `y_reset_in`, loads today and would be refused with `algebraic-loop`. Modelica
  accepts these: the reinitialisation is settled by event iteration, which HostTick does not run
  (execution profile, "no hidden same-time evaluation"). The remedy is a `Logical.Pre` or
  `UnitDelay` on the feedback path, which reproduces today's one-frame delay explicitly. No tracked
  document is refused: no load in the 2,758-test run failed; only the three synthetic tests that pin
  the cut changed (section 4).
- **No new mixed-feedthrough class.** Each block has one output, and every input it reads at
  emission feeds through to it. The census stays at twelve: `PIDWithReset` in P mode reads
  `trigger` without feeding it through, as today. The text at `docs/cdl-coverage.md:502` that says
  the same feedback "loads" in the stateful modes becomes false.
- **Schedules can reorder.** A block now waits for its event input's producer. In the corpus one
  blob's schedule changes (`thermal_zones_control_loops`), and the new decoder refuses its old blob
  with `InvalidSchedule { fault: EmitBefore }`.

## 3. References outside the engine

| Reference | What it can show | What it cannot |
| --- | --- | --- |
| `derive_valve.py` (Building 33), updated so that a rising trigger after frame 0 sets `integral = Y_RESET − y_p` before the PI emits, and the update then skips its Euler step | The transcription under the rule, bit for bit, at all 74 frames, from code that shares nothing with the crate. A scratch copy moves four rows from the oracle, all to 0: 25 s (0.09), 237 s (0.045), 272 s (0.018) and 307 s (0.018); the other 70 are byte-identical | The rule against the continuous model: it is the same discretisation, written twice. The scenario's enable is false at frame 0, so it does not reach the frame-0 case |
| Hand derivation, PI `k = 1`, `Ti = 1`, `xi_start = 0`, `y_reset = 5`, `e = 1`, frames 1 s apart, unsaturated | Rising at t = 2: frame 2 emits `lim(5) = 5`, `I = 5 − 1 = 4` is held, frame 3 emits 5, frame 4 emits 6. True from t = 0 (`y_reset ≠ xi_start`): no reset, `I` = 0, 0, 1, 2, outputs 1, 1, 2, 3 | Anything beyond the recurrence |
| Hand derivation, `IntegratorWithReset` `k = 1`, `y_start = 0`, `u = 1`, `y_reset_in = 10` | Rising at t = 2: 0, 0, 10, 10, 11. True from t = 0: 0, 0, 1, 2 | As above |
| `tools/golden-gen` (its own workspace, `depends_on_oce_blocks: false`) | Re-derived per-block and G36 Tier-A references under the rule; `src/reals_pid.rs`, `src/reals_integrator.rs`, the discrete module and seven `sequences/*.rs` files encode today's convention and change with it | Independence from this note's reading of Modelica: it implements the same rule |
| OpenModelica day (`building_33_openmodelica.rs`) | That the fix does not degrade agreement with the continuous model: with and without the scratch fix the largest difference is 3.39934346310633e-3 at 67,375 s and the mean 1.4689685623085045e-4, identical to every printed digit. **The 0.0034 bound does not tighten** | Either difference: the enable is on throughout the day, so the only event is at frame 0, and the valve is 0 for the day's first instants with or without that reset. A check of the next-frame event against OpenModelica needs a run with re-enables (decision 9) |

## 4. Blast radius

Measured with the scratch implementation of all five classes; 86 of 2,758 tests failed before any
re-bless (`oce-blocks` 23, `oce-graph` 9, `oce-api` 31, `oce-conformance` 22, `oce-verify` 1).

| Artefact | Moves | Which |
| --- | --- | --- |
| G36 determinism traces, `crates/oce-conformance/tests/fixtures/golden/g36_traces/` | 7 of 46 `.csv`, with their 7 `.prov.json` | the seven fixtures that hold `PIDWithReset`: `cooling_only_controller`, `cooling_only_dampers`, `multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21`, `multizone_vav_economizer_limits_common`, `multizone_vav_supply_fan`, `multizone_vav_supply_signals`, `thermal_zones_control_loops`. The frame-0 change alone moves only the first and third; the next-frame change moves all seven. `generic_time_suppression` (`TriggeredSampler`) does not move |
| The 46 fixture goldens and 422 provenance goldens, `tools/golden-gen/goldens/` | yes; rule 2's "exists to regenerate them" applies | CDL: 7 scenarios (`PIDWithReset/pi_reset_recurrence`, `IntegratorWithReset` default and `gain`, `OnCounter` default and `trigger_initially_true`, `TriggeredSampler/trigger_initially_true`, `TriggeredMax/trigger_initially_true`), each `y.csv`, `y.prov.json` (its `reference_rule` text) and `reference.csv`, plus `MANIFEST.txt`. Unchanged: `OnCounter/held_reset`, the `TriggeredSampler` and `TriggeredMax` defaults, every `TriggeredMovingMean` scenario. G36: the same seven directories, 30 provenance files at most; the Tier-A oracle tests of six of them failed, and the exact count is whatever golden-gen re-derives |
| Compiled-schedule blobs, `crates/oce-api/tests/fixtures/blob/` | 3 of 46 `.oceb` and their `blob_summary/*.json`; 5 more run digests in `blob_frame_digests.tsv` (8 of 46 round trips) | `cooling_only_controller` and `generic_time_suppression` (the `TriggeredSampler` initial-state CRC, decision 7), `thermal_zones_control_loops` (schedule order). `class_signatures.tsv` does not move: the hash covers `state_len` and ports, not feedthrough or initial words |
| Building 33 | `valve_oracle.txt` (4 rows), `derive_valve.py`, README item 2 | the day receipt and the OpenModelica check pass unchanged |
| Tests pinning today's convention | rewritten, not deleted | e.g. `pid_tests` (4), `reals_integrator_tests` (6), `integers_edge_tests` (3), `discrete_tests` (6), `oce-graph` `pid_feedthrough`, `edge_counter_feedthrough`, `mixed_feedthrough_block_loop::reset_pid_trigger_feedback_compiles_outside_p_mode`, `oce-api` `reset_pid_block_loop::reset_pid_trigger_feedback_loads_in_every_mode_with_state` |
| `open-control-library` | nothing | no sequence document uses any of the five classes |

**Formats and revisions.** No `state_len` changes, and each `state_contract_revision` stays at 1
(rule 9). After any frame a state word means what it means today: the integrators store the reset
value for the next frame exactly as now. This is a behaviour change, not a format change, apart from
`TriggeredSampler`'s initial word. The workspace is already 0.6.0 since #720, after the v0.5.0
tag, so rule 10's bump is done; `EngineVersionMismatch` and `blob_old_layout.rs` already refuse
earlier artefacts, and no new refusal test is owed. **Strict bits:** no file under
`crates/oce-conformance/tests/strict_bits/` names these classes, and no strict-bit test failed.
The PR touches `oce-blocks` and runs the determinism lane, in all four native cells.

**Docs that change:** `docs/execution-profile.md` (a section beside `Pre`'s giving this rule);
`docs/cdl-coverage.md` (lines 502 and 519-524, and the remedy list); the module and type docs of
`blocks/pid.rs` (lines 3-5, 23-28, 398-403), `blocks/reals_integrator.rs` (1-3, 19-32),
`blocks/integers_edge.rs` (1-5, 32-39) and `blocks/discrete.rs` (176-184, 248-258); the Building
33 README and oracle; the golden-gen `reference_rule` texts; `CHANGELOG.md`.

## 5. Plan

**One PR**, `fix(blocks): …`, closing #768, with the five classes, because they share one rule,
one re-bless of the same goldens and one CHANGELOG paragraph per behaviour (event at the frame;
no event at frame 0; event inputs feed through). Splitting would re-bless
`cooling_only_controller` and the economizer controller twice. Proof row: `light`
(`bash .agents/gate.sh`, batched), as `scripts/ci/proof-row.sh` names it for these paths; the PR
itself owes fmt, clippy and rustdoc on the changed crates, the `oce-blocks`, `oce-graph`,
`oce-api`, `oce-conformance` and `oce-verify` suites, doctests, golden-gen hygiene, and the goldens
line naming the files above.

**Tests it owes (TESTING.md).** Edge cases per class: a rising trigger at frame 1 (the first
possible event), a held trigger, a trigger true from frame 0 with `y_reset ≠ xi_start` and with
`P + D ≠ 0`, a trigger rising in the same frame as a step in `u_s`, `u_m` or `y_reset_in`, a trigger
falling and rising on consecutive frames, a reset outside `[yMin, yMax]` (clamped at the event),
NaN and infinite `y_reset_in`, repeated equal timestamps, and snapshot and restore across a reset
frame. Graph tests: feedback into each new feedthrough input is refused with `algebraic-loop`, and
the same model with `Pre` loads and runs with today's timing. Goldens re-derived in golden-gen,
compared bit-exactly. Oracle: the updated `derive_valve.py` and the hand derivations above as
committed vectors. Determinism: the re-blessed traces repeat bit-exactly in all four cells.

**Risks.** A model with feedback into an event input that loads today is refused after the
upgrade; the CHANGELOG and the load diagnostic must name the `Pre` remedy. An emitted reset value
`P + D + (y_reset − (P + D))` can differ from `y_reset` by an ulp; decision 8 fixes which. The
`TriggeredSampler` first-frame marker must not leak into a capturable state (decision 7).

## Decisions for the owner

1. **Scope.** (a) Fix `PIDWithReset` and `IntegratorWithReset` only, the `when … reinit` pair;
   (b) also `OnCounter` (next-frame) and `TriggeredSampler` and `TriggeredMax` (frame 0); (c) (b)
   in two PRs. *Recommend (b) in one PR*: one rule, one re-bless, and the same Modelica reading
   that the ruling applies to `PIDWithReset`.
2. **Feedback into an event input.** (a) Refuse it with `algebraic-loop`, remedy `Pre`;
   (b) keep the event input a cut and accept the next-frame event only on a cycle, which makes a
   block's timing depend on its surroundings; (c) keep the cut everywhere and document the
   next-frame event as the profile's choice beside `Pre`. *Recommend (a)*: fail closed, the same
   policy as atomic firing, and no tracked document is refused.
3. **`OnCounter` as the integer loop cut.** Making it feed through removes the only integer
   counter that cuts its own feedback; a counter that resets itself through a threshold then needs
   `Pre`. *Recommend fixing it with the others* (no library sequence uses it), unless the owner
   wants it kept as a documented exception under (c) above.
4. **Inputs that change in the frame of the event.** The reset value is this frame's: for
   `IntegratorWithReset` this frame's `y_reset_in`, for `PIDWithReset` `y_reset` minus this frame's
   `P + D` (Modelica evaluates `reinit`'s expression at the instant). The frame's Euler step is
   discarded. *Recommend as stated.*
5. **A trigger true at frame 0.** It is latched as `pre(trigger)` for frame 1, so no reset fires at
   frame 1 either, and the first event is its next false-to-true transition. *Recommend yes*:
   this is §8.3.5's "becomes true" after §8.6's `pre(vi) := vi`.
6. **`Change`, `Edge`, `FallingEdge` at frame 0.** (a) Keep the initialisation value (true when
   `u` differs from `pre_u_start`), today's behaviour; (b) emit the value after Modelica's initial
   event iteration (false). *Recommend (a)*: it is the initialisation solution, the consuming
   `when` ignores it under the rule either way, and (b) would move the goldens of edge blocks
   whose input starts true (not measured here) for no reset-timing gain. Differential runs against OpenModelica then compare
   frame 0 with the initialisation row.
7. **`TriggeredSampler`'s first-frame marker.** It has no `prev_t` or `has_history` word. (a)
   Start its trigger word at a third value meaning "no frame yet" (no `state_len` change; moves the
   initial-state CRC of 2 blobs); (b) add a third word (`state_len` 2 → 3, moves its class
   signature hash and every blob holding it). *Recommend (a)*, with the validator accepting the
   marker only in a state no frame has touched; the implementation confirms whether capture can
   see such a state (`TriggeredMax`'s `has_history == 1` check, `blocks/state_contract.rs:33-37`,
   suggests it cannot).
8. **Bits of the emitted reset.** (a) Emit `P + D + (y_reset − (P + D))` through the normal path,
   the same bits the next frame emits when `P + D` is unchanged and what Modelica's `addPID`
   computes; (b) emit `lim(y_reset)` exactly. *Recommend (a).*
9. **Evidence for the next-frame change against OpenModelica.** (a) None beyond the
   hand-derived and Python references; (b) add an OpenModelica run of the 74-frame scenario
   (trend inputs, `CombiTimeTable` held) as a later evidence issue. *Recommend (b) as a follow-up
   issue*, not in the fix PR.
10. **`PIDWithReset` in P mode.** Keep `trigger` non-feedthrough there (the census stays at
    twelve, the P-mode refusal stays `block-algebraic-loop`), or declare it feedthrough in every
    mode. *Recommend keep*: out of scope.
