# Vectors, the verify replayer and receipts

**Design note, written before any `oce-verify` code (M3 items 4 and 5, tracking issue #456; issue
#508).** It fixes the vector format, how a vector set is bound to a loaded sequence and replayed
through [`oce-replay`](replay.md), the vector receipt under the header it shares with the
[commissioning verifier](commissioning.md), mutant mode, the `oce verify` verb and the fixtures.
Every decision below was made while drafting, checked by an independent review and ruled on by
the owner on 2026-11-09 (see [Review rulings](#review-rulings)); the note is amended to match.
Decisions 1 to 19 answer the format half of the issue's outline in its order, decisions 20 to 37
the replayer half (outline item *n* is decision 19 + *n*). Nothing here is built. *(2026-11-09:
part 1 of 4, the format, landed in GitHub #541. 2026-11-10: part 2 of 4, binding, the run and the
receipt, landed in GitHub #558.)*

**The version rule, in one sentence.** The format is `oce/vectors/v1` and stays `v1` until 1.0.0: a
change is made in place, in the PR that rewrites every in-repo fixture and golden, and nothing
reads an older shape (owner ruling, 2026-11-09; #456, #509).

## What this is, and is not

`oce-verify` is a leverage crate (`crates/leverage/oce-verify`) above the facade. Among `oce-*`
packages it depends on `oce-api` and `oce-replay` only, as the commissioning note states. This note
covers its **exact mode over vectors**: a vector set is a strict-JSON file of scenarios (inputs
over a clock, expectations over windows) checked against one loaded sequence, the **subject**.

- It **never changes the engine**. Parameters are set only through the facade's halt,
  `set_param`, resume path. The one facade addition M3 makes for it is #460's read-only
  `FrameSlots::output_types()`, a core PR of its own.
- It **opens no file and runs no clock**. The library takes bytes; the CLI resolves paths.
- A receipt is **evidence, not a gate**: it never decides whether a sequence may load or run
  (#456, "Notes for issue bodies").
- **Not here:** the registry index and `oce fetch` (#515), the release-gate replay of the
  library's corpus (#516), the library re-stamp and "ids agree", all milestone M3b; tolerance mode,
  the funnel port and file-backed series (tracking items 6 and 7); the conversion of the library's
  corpus, which the library does once in its own repository
  (jscott3201/open-control-library#105; its bundle contract, #106 there, cites this note and does
  not restate it).

## What exists today, checked against the tree

**The old shape.** `crates/oce-api/tests/fixtures/library_rules/AHU-0016/vectors.json` and
`AHU-0004/vectors.json` carry `"schema": "cxf-library/vectors/v1"`: 2 files, 16 scenarios (7 and
9), 21 expectation windows (8 and 13), 8 step-list inputs, every `equals` a Boolean. Their one
reader is test-local, in `crates/oce-api/tests/library_rule_state_continuation.rs`.

**The library's corpus**, counted read-only on a fresh clone at commit `124427b` (the issue's own
counts came from a stale clone): 139 `vectors.json` files; 1,781 scenarios (1,760 fault, 21
routine); 4,563 expectation windows (4,463 and 100); 915 step-list inputs (906 and 9) holding
9,182 steps. No file writes a `tolerance` key. 45 windows carry a non-Boolean `equals`, all in one
routine (`airflow-setpoint`); no fault vector has one. Every step list starts at `t = 0`; 33 step
times are off the clock's grid, all in fault vectors; 5 are after the horizon (`HP-0002` and
`HW-0001`). No window ends after the horizon and none is empty of ticks. The largest file is 184
KB, with at most 28 scenarios, 12 expectations per scenario and 241 steps per list. By the
orchestrator's static check, not repeated here, every scenario supplies every boundary input of
its graph and names no other, so a complete frame can be built for all of them.

**The library's documents.** They use `urn:cxf-library:<id>#` as instance namespace and class
base, a root `@type` of `S231:Block` and untyped parameter nodes. 92 of the 137 rule documents
carry a retired root name (`ahu_fc_050` for AHU-0016), and so do the two copies vendored here. The
sequence name is inside the canonical content id, so those ids change when the library renames
them; #517 tracks re-vendoring the two rules after it converts them. The two routine graphs use
`S231:hasInstance`, which the importer refuses, and the library hand-writes those.

**`oce-replay` as built** (GitHub #462). `StepSource` rows are the union of step times and any
declared instants; `TimeBase::Grid { t0, step }` ticks at `t0 + k * step` and stops once a tick
passes the last source row; `Trace::check_coverage` counts executed rows only; a `DiagnosticRow` is
`frame`, `block`, `message`, `time`, `level`; an engine refusal is the closed `EngineRefusal` class.

**Run-time warnings carry no code.** `oce_api::AssertEvent` is `block`, `message`, `t` and `level`
(`crates/oce-api/src/observations.rs`), and `AssertLevel` has one variant, `Warning`. Its rustdoc
calls `block` the "Producer-supplied diagnostic source, currently class-level rather than instance
identity": the class path (`CDL.Utilities.Assert`, `CDL.Reals.MovingAverage`), never the instance
that warned.
`message` is free text, the authored `message` parameter for `CDL.Utilities.Assert` and an
engine-written sentence otherwise. Only load-time diagnostics carry a code
(`oce_diag::DiagCode::as_str`, kebab-case).

**The CLI** (GitHub #491). `Exit` is `Yes` 0, `No` 1, `Error` 2. Stdout carries one document, the
command's product or the `oce-report/1` report, and `Outcome::document` is built only with
`Exit::Yes`. `codes.rs` already spells the strict-JSON refusals (`duplicate-key`, `unknown-key`,
`non-finite-number` and the rest).

**The sketch.** The owner's Milestone Work Plan, kept out of the tree, carries an earlier sketch
of a "vector format v2". No clone has it; "the sketch" below means that draft as issue #508
describes it, and this note replaces it.

Where this contradicts the issue's outline, the code wins:
[Where the tree overrules the outline](#where-the-tree-overrules-the-outline) lists each case.

## The vector format

Strict JSON, one object, closed at every level. The worked file below shows every field of the
first cut. It is a shape built around the AHU-0016 rule, not a fixture: that rule has no `Assert`
block, so the `diagnostic` line shows the form only.

```json
{
  "schema": "oce/vectors/v1",
  "clock": { "step_s": 60, "horizon_s": 1800 },
  "params": {},
  "mutants": [
    { "path": "mutants/threshold_inclusive.cxf.json",
      "description": "Both thresholds read >= 5 instead of > 5." }
  ],
  "scenarios": [
    {
      "name": "transient_overlap",
      "description": "Overlap shorter than the delay never alarms.",
      "inputs": {
        "htg_vlv_cmd": 20.0,
        "clg_vlv_cmd": [ { "t": 0, "value": 30.0 }, { "t": 600, "value": 0.0 } ]
      },
      "expect": [
        { "output": "yFault", "from_s": 0, "to_s": 1800, "equals": false }
      ]
    },
    {
      "name": "short_delay",
      "params": { "persist.delayTime": 300.0 },
      "inputs": { "htg_vlv_cmd": 20.0, "clg_vlv_cmd": 30.0 },
      "expect": [
        { "output": "yFault", "from_s": 0, "to_s": 240, "equals": false },
        { "output": "yFault", "from_s": 360, "to_s": 1800, "equals": true },
        { "diagnostic": { "block": "CDL.Utilities.Assert", "message": "Both valves open." },
          "at_s": 360 }
      ]
    }
  ]
}
```

| Field | Required | Meaning |
| --- | --- | --- |
| `schema` | yes | exactly `oce/vectors/v1`; anything else is `unknown-schema` |
| `clock.step_s`, `clock.horizon_s` | yes | the set's grid: ticks `k * step_s` for `k = 0..=N`, where `N * step_s == horizon_s` |
| `params` | no | parameter path → value, applied to every scenario before its first frame |
| `mutants[]` | no | `path` (relative to this file) and an optional `description`; the key is added in place by part 3 and is `unknown-key` before it |
| `scenarios[]` | yes | `name` (not empty, unique in the set), optional `description`, optional `params`, `inputs`, `expect` |
| `inputs` | yes | input name → a constant, or a step list `[{ "t", "value" }]` starting at `t = 0` |
| `expect[]` | yes | either a window (`output`, `from_s`, `to_s`, `equals`) or an instant (`diagnostic`, `at_s`) |

**Clock.** `step_s` is finite and positive, `horizon_s` finite and not negative. A time `x` is
**on the cadence** when an integer `k >= 0` exists with `(k as f64) * step_s == x` under IEEE-754
double multiplication. `k` is found by rounding `x / step_s` to the nearest integer `q` and
testing `q - 1`, `q` and `q + 1`; inside the tick bound at most one of them can hold. A quotient
is never floored: `floor(4.3 / 0.1)` is 42, while `43 * 0.1 == 4.3` exactly. `horizon_s` must be
on the cadence and its `k` is `N`; tick `k` is `(k as f64) * step_s` for `k = 0..=N`, by multiply.
`N + 1` may not exceed 16,777,216 ticks, and a quotient past that bound is refused before any
candidate is tested. *(Amended 2026-11-10, with GitHub #541:)* a time is a binary64, so in every
time position (`step_s`, `horizon_s`, a step's `t`, `from_s`, `to_s`, `at_s`) an integer token
must lie within ±2^53, the range in which every integer is a binary64 exactly; one beyond it is
`integer-out-of-range` at load, so an integer token never names a time other than the one it
spells. A larger time is written as a real (`1e16`), which rounds to the nearest binary64 as
every real does.

**Inputs.** A constant is a one-step list at `t = 0`. In a step list, `t` is finite, strictly
increasing, starts at exactly `0` and never exceeds `horizon_s`; a step time need not be a tick,
and takes effect at the first tick at or after it (zero-order hold). Every engine input has
exactly one entry.

**Names.** *(Amended 2026-11-10, with GitHub #541:)* no name is empty: an empty scenario name,
input name, parameter path, output name or `diagnostic.block` is `empty-name` at load, so an
empty name never reaches binding, where a suffix rule would have to give it a meaning. A
`description` and a diagnostic `message` may be empty.

**Windows.** `0 <= from_s <= to_s <= horizon_s`, inclusive at both ends. Tick `k` is in the window
when `from_s <= k * step_s <= to_s`, compared on the tick's value. Every tick in the window is
checked, and a window that contains no tick is refused.

**Diagnostic at an instant.** `at_s` must be on the cadence, by the rule above, and not after the
horizon. The expectation holds when the frame at that tick emitted at least one warning whose
`block` equals the given class path and, when `message` is present, whose `message` equals it byte
for byte. *(Amended 2026-11-10, owner ruling with GitHub #558:)* matching is one to one: an
expectation takes one warning and a warning is taken by at most one expectation, in the order
[part 2's readings](#part-2s-readings) give (reading 7), and every warning no expectation took is
unmatched.

**Bounds.** Fixed by the strict profile: nesting 8, strings 4,096 bytes, lists 65,536 entries
(which bounds a step list). Set by `VerifyLimits`, each lowerable and never raisable: 4 MiB per
file, 16,777,216 ticks, 4,096 scenarios, 4,096 expectations per scenario, 256 parameters per map,
256 mutants. Each is a typed error at its boundary, at load.

**Canonical form.** `to_canonical_json` sorts keys by their UTF-8 bytes at every depth and writes
two-space indentation, LF, one final line feed, and ASCII only: any other character is `\u` and
four lowercase hex digits, astral characters as surrogate pairs, as `oce-author`'s writer does.
The loaded set remembers whether each number was an integer token or a real.

- An integer token is written as a decimal integer. `-0` is the integer zero and is written `0`.
- A real is written with the shortest digits that read back to the same bits: in plain decimal
  with at least one fractional digit when it is zero or `1e-4 <= |x| < 1e16`, otherwise as
  `d[.ddd]e[-]N` with a lowercase `e`, no `+` and no leading zero in the exponent. `-0.0` is
  written `-0.0`. *(Amended 2026-11-10:)* "shortest" alone does not pick one string: `3e-324`
  through `7e-324` all read back to the smallest subnormal. Of the shortest strings that read
  back, the writer takes the one closest to the exact binary value, here `5e-324` (the value is
  4.94…e-324), and likewise `1e-323` and `1.5e-323` for the next two subnormals. So: `0.1`, `60.0`, `0.0001` (the lower threshold) and `9.999999999999999e-5`
  (the binary64 just below it), `9999999999999998.0` (the binary64 just below the upper
  threshold) and `1e16`, `1.2345678901234568e17`, `5e-324` (the smallest subnormal) and
  `1.7976931348623157e308`. The rule is normative as written: it was confirmed against the
  implementation in the format PR (part 1 of 4, GitHub #541), where the layout is decided by
  `oce-verify`'s own code from the decimal exponent of the shortest digits (plain decimal for an
  exponent from -4 to 15), so no formatter's threshold decides a hashed byte. Goldens derived
  outside the crate pin the bytes at both thresholds, at `-0`, `-0.0` and a subnormal, and a test
  compares the result with Rust's `{:?}` for an `f64`, which follows the same rule, so a
  toolchain change cannot move the spelling unseen.

`spec_id` is `spec:fnv1a128:<32 hex>` over those bytes. A file need not be canonical to load.

## Binding and running

Binding happens once per set, against a throwaway engine loaded from the subject bytes, before any
scenario runs. Any failure is an error (exit 2, no receipt), never a failed scenario.

1. **Load.** `Engine::load_cxf(subject)`, then `Engine::frame_slots()`. A refusal is
   `subject-load-failed` carrying the engine's `DiagCode` spellings. Load warnings do not refuse:
   their codes are listed in the body under `load_warnings` (`oce validate` is where a warning is
   an error finding).
2. **Names.** Input names resolve against `Engine::input_definitions()` by the replay rule: the
   exact canonical path, or a suffix matching exactly one path at a `.` boundary. Output names
   resolve the same way against `FrameSlots::outputs()`, and parameter paths against
   `Engine::params()`.
3. **Values.** Each JSON value is typed by the declared type of what it lands on (decision 7): an
   input by `InputDefinition::value_type`, a parameter by its declared type, an `equals` literal by
   `FrameSlots::output_types()`. That last accessor is #460; today `outputs()` is paths only and a
   type exists only after the first accepted frame, which is why #460 moves into M3. *(2026-11-09:
   the accessor landed in GitHub #525.)*
4. **Parameters.** Each scenario's effective edits are resolved and ordered (decision 14).
   *(Amended 2026-11-10, owner ruling with GitHub #558:)* and tried, on a freshly loaded engine
   per scenario, by the passes of decision 14, so `param-refused` is found here, before any frame
   of any scenario runs (reading 8).
5. **Mutants.** Every listed mutant is loaded and validated ([Mutant mode](#mutant-mode)).
6. **Frames.** `(N + 1) * scenarios * (1 + mutants)` must fit `VerifyLimits::max_total_frames`.

Each scenario then runs on a **freshly loaded engine**: load, apply its parameter edits, then one
`Replay` over a `StepSource` built with
`StepSource::with_instants(map, steps, vec![horizon_s], limits)` and
`ReplayOptions::new(TimeBase::Grid { t0: 0.0, step: step_s })`. The replay defaults stand:
`MissingPolicy::Refuse`, `OnRefusal::Stop`, `DuplicateTime::Refuse`,
`NonFiniteInputPolicy::Refuse`, outputs only in the trace. The `horizon_s` instant is what makes
the grid run to the horizon when no input changes late; the step-time bound above is what stops it
running past.

**After a scenario's run, in this order:**

1. `Progress::Stopped` with an `EngineRefusal::Run` cause is the error `run-refused` (exit 2).
2. `Progress::Stopped` with any other cause makes the scenario `fail`, with one `frame-refused`
   failure at the refused tick. Instants before that tick are compared and counted; instants at
   and after it are not counted. **Coverage is not checked for this scenario**: under
   `OnRefusal::Stop` the trace ends at the refused row, so `Trace::check_coverage` would turn
   every later window into an error when the verdict is a failure.
3. Otherwise every tick executed. `Trace::check_coverage` is run over the windows; load already
   proved each contains a tick, so a miss here is the error `window-uncovered`.
4. Every tick in every window, and every diagnostic instant, is compared.

## The receipt

The header is the commissioning note's, unchanged. The tables are normative; the examples after
them show shape, not a prediction: no engine was run for this note, ids are elided, and keys are
in reading order where the canonical bytes sort them. `to_canonical_json` on a receipt follows the
vector file's rules. Arrays keep the order each row states.

**Header.** Every field is always present.

| Field | Type | Content |
| --- | --- | --- |
| `schema` | string | `oce/receipt/v1` |
| `kind` | string | `vectors` |
| `content_id` | string | the subject's `oce:canon1:` id as loaded, before any parameter edit |
| `engine_version` | string | the facade's Cargo version (decision 33) |
| `spec_id` | string | `spec:fnv1a128:<32 hex>` of the vector set |
| `comparison` | object | `{ "mode": "exact" }`, the envelope fields absent |
| `passed` | boolean | `body.status == "pass"` |
| `receipt_id` | string | `receipt:fnv1a128:<32 hex>` over the canonical receipt with `timings` and `receipt_id` removed |
| `timings` | object | `load_ms`, `run_ms`, `compare_ms`: integers, zero unless the caller measured |
| `body` | object | below |

**Body.**

| Field | Type | Present | Content, order and bound |
| --- | --- | --- | --- |
| `status` | string | always | `pass`, `fail` or `no_data` (decision 26) |
| `clock` | object | always | `step_s`, `horizon_s`, as the set writes them |
| `load_warnings` | array of string | always | `DiagCode` spellings in load order, at most 64 |
| `load_warnings_total` | integer | always | the count before the cap |
| `totals.scenarios` | object | always | `total`, `pass`, `fail`, `no_data` |
| `totals.frames` | object | always | `executed`, `substituted`, `refused`, summed over scenarios |
| `totals.instants` | object | always | `checked`, `passed`, `failed`, `masked`, summed over scenarios |
| `totals.failures`, `totals.warnings` | integer | always | the sums of `failures_total` and `warnings_total` |
| `scenarios` | array | always | one object per scenario, in file order, at most 4,096 |
| `mutants` | object | when the set lists a mutant | below |

**Scenario.**

| Field | Type | Present | Content, order and bound |
| --- | --- | --- | --- |
| `name` | string | always | as written |
| `status` | string | always | `fail` when a frame was refused or `instants.failed > 0`; else `no_data` when `instants.checked == 0`; else `pass` |
| `params` | array | always | the effective edits as `{ "path", "value" }`, `path` the canonical parameter path, sorted by path bytes; `[]` when there are none; at most 256 |
| `content_id` | string | when `params` is not empty | `Engine::content_id` after the edits: the id of what ran |
| `frames` | object | always | `executed`, `substituted`, `refused` |
| `expectations` | integer | always | the number of `expect` entries |
| `instants` | object | always | `checked`, `passed`, `failed`, `masked`; `checked == passed + failed` |
| `failures` | array | always | by tick, then by expectation index; at most `max_recorded_failures` (16); when a frame was refused, the last slot is kept for its `frame-refused` record *(amended 2026-11-10, owner ruling with GitHub #558)* |
| `failures_total` | integer | always | the count before the cap |
| `warnings` | array | always | unmatched run-time warnings as `{ "t", "block", "message" }`, in row then emission order; at most 16 |
| `warnings_total` | integer | always | the count before the cap |

**What counts as an instant.** One window tick is one instant, and so is one diagnostic
expectation at its `at_s`: a scenario with only diagnostic expectations is not `no_data`. A
refused frame is not an instant; it is counted in `frames.refused` and `failures_total`. `masked`
is zero until `when` exists.

**Failure record**, by `code` (a failure makes exit 1, never exit 2):

| `code` | Fields |
| --- | --- |
| `value-mismatch` | `expectation` (index), `output` (canonical path), `t`, `expected`, `actual` |
| `diagnostic-missing` | `expectation` (index), `t`, `block`, and `message` when the expectation gave one |
| `frame-refused` | `t`, `refusal` (the kebab-case `EngineRefusal` class, decision 25) |

**Mutants.**

| Field | Type | Present | Content, order and bound |
| --- | --- | --- | --- |
| `status` | string | always | `scored`, or `not_run` when the subject did not pass |
| `total`, `caught`, `survived` | integer | always | `total` is the number listed; the other two are zero under `not_run` |
| `entries` | array | always | one object per listed mutant, in file order, at most 256 |
| `entries[].path` | string | always | as the set writes it |
| `entries[].digest` | string | always | `fnv1a128:<32 hex>` of the bytes supplied |
| `entries[].content_id` | string | always | the mutant's id (every listed mutant loaded, or there is no receipt) |
| `entries[].status` | string | always | `caught`, `survived` or `not_run` |
| `entries[].scenario`, `entries[].failure` | string, failure record | when `caught` | the first failing scenario in file order and its first failure |

There is no `invalid` status: an invalid mutant is an error and no receipt exists
([Mutant mode](#mutant-mode)).

**Values** (`expected`, `actual`, a `params` value). A Boolean is `true` or `false`, an Integer a
JSON integer, a finite Real other than negative zero a JSON number in the canonical real spelling.
Negative zero, NaN and the infinities are the string `"0x"` plus the 16 lowercase hex digits of
the bits. No trace column is a String, so a string in a value position is always bits.

A pass over a one-scenario set:

```json
{
  "schema": "oce/receipt/v1",
  "kind": "vectors",
  "content_id": "oce:canon1:5d0c…",
  "engine_version": "0.5.0",
  "spec_id": "spec:fnv1a128:7b1e…",
  "comparison": { "mode": "exact" },
  "passed": true,
  "receipt_id": "receipt:fnv1a128:c4a9…",
  "timings": { "load_ms": 0, "run_ms": 0, "compare_ms": 0 },
  "body": {
    "status": "pass",
    "clock": { "step_s": 60, "horizon_s": 1800 },
    "load_warnings": [], "load_warnings_total": 0,
    "totals": {
      "scenarios": { "total": 1, "pass": 1, "fail": 0, "no_data": 0 },
      "frames": { "executed": 31, "substituted": 0, "refused": 0 },
      "instants": { "checked": 31, "passed": 31, "failed": 0, "masked": 0 },
      "failures": 0, "warnings": 0
    },
    "scenarios": [ {
      "name": "transient_overlap", "status": "pass", "params": [],
      "frames": { "executed": 31, "substituted": 0, "refused": 0 },
      "expectations": 1,
      "instants": { "checked": 31, "passed": 31, "failed": 0, "masked": 0 },
      "failures": [], "failures_total": 0, "warnings": [], "warnings_total": 0
    } ]
  }
}
```

A failing scenario (the first window of AHU-0016's `both_open` hand-edited to end at 900 s, where
the in-repo test asserts the delay fires):

```json
{
  "name": "both_open", "status": "fail", "params": [],
  "frames": { "executed": 31, "substituted": 0, "refused": 0 },
  "expectations": 2,
  "instants": { "checked": 31, "passed": 30, "failed": 1, "masked": 0 },
  "failures": [ {
    "expectation": 0, "output": "urn:cxf-library:ahu-fc-050#ahu_fc_050.yFault",
    "t": 900.0, "expected": false, "actual": true, "code": "value-mismatch"
  } ],
  "failures_total": 1, "warnings": [], "warnings_total": 0
}
```

The `mutants` section:

```json
"mutants": {
  "status": "scored", "total": 3, "caught": 2, "survived": 1,
  "entries": [
    { "path": "mutants/threshold_inclusive.cxf.json", "digest": "fnv1a128:19f3…",
      "content_id": "oce:canon1:a2e7…", "status": "caught", "scenario": "threshold_edge",
      "failure": { "expectation": 0, "output": "urn:cxf-library:ahu-fc-050#ahu_fc_050.yFault",
                   "t": 900.0, "expected": false, "actual": true, "code": "value-mismatch" } },
    { "path": "mutants/late_alarm.cxf.json", "digest": "fnv1a128:60bd…",
      "content_id": "oce:canon1:0f48…", "status": "survived" }
  ]
}
```

## Mutant mode

A **mutant** is a CXF document that should fail the vectors: a deliberate defect in the subject.
The set names its mutants; the caller supplies each one's bytes.

- **Validated at bind**, every one, before any scenario of the subject runs. A mutant is valid
  when it loads, its input paths and types equal the subject's (`Engine::input_definitions()`),
  its output paths and types equal the subject's (`FrameSlots::outputs()` and `output_types()`,
  #460), and its content id differs from the subject's. Input bounds are not compared.
- **An invalid mutant is an error, in one place.** `run_vectors` returns `Err` naming the first
  invalid mutant in file order with `mutant-load-failed`, `mutant-boundary-differs` or
  `mutant-identical`. No receipt exists, so `--out` writes nothing; the CLI prints the report and
  exits 2, whatever the subject's verdict would have been.
- **Scored only when the subject passes.** Against a failing subject the section is
  `"status": "not_run"`: a mutant that fails the scenario the subject fails proves nothing.
- **Caught** when at least one scenario fails on it; a mutant's run stops at its first failing
  scenario, which the receipt names with its first failure. **Survived** otherwise. A frame-class
  refusal fails the scenario for a mutant as for the subject; a run-class refusal is an error.
- The score is the integer pair `caught` and `total`, with `survived` beside them. There is no
  ratio and no float.

## Error codes

Every refusal is one `VectorErrorKind` (load) or `VerifyErrorKind` (bind and run), both
`#[non_exhaustive]`, each with a stable kebab-case `code()` and a list constant pinned by a test,
as `ImportErrorKind` is. A code is the contract; a message is not. Every row is exit 2 with no
receipt.

| Code | Stage | Refuses |
| --- | --- | --- |
| `not-utf8`, `byte-order-mark`, `json-syntax` | load | bytes that are not one strict JSON document |
| `input-too-large`, `nesting-too-deep`, `list-too-long`, `string-too-long` | load | a read bound (a step list past 65,536 steps is `list-too-long`) |
| `duplicate-key`, `unknown-key`, `missing-key`, `wrong-type` | load | a closed object's shape; `within`, `when` and, before part 3, `mutants` are `unknown-key` |
| `non-finite-number`, `integer-out-of-range`, `control-character` | load | a lexical refusal; `integer-out-of-range` is also an integer token beyond ±2^53 in a time position *(amended 2026-11-10)* |
| `unknown-schema` | load | a `schema` other than `oce/vectors/v1`, the two older tags included |
| `invalid-clock` | load | `step_s` not finite and positive, `horizon_s` not finite or negative |
| `horizon-off-cadence`, `too-many-ticks` | load | a horizon no integer `k` reaches; more than 16,777,216 ticks |
| `too-many-scenarios`, `too-many-expectations`, `too-many-mutants`, `too-many-parameters` | load | a set bound |
| `duplicate-scenario`, `empty-name` | load | scenario names; `empty-name` is also an empty input name, parameter path, output name or `diagnostic.block` *(amended 2026-11-10)* |
| `step-list-empty`, `step-list-not-from-zero`, `step-times-not-increasing`, `step-after-horizon` | load | a step list |
| `invalid-window`, `window-outside-clock`, `window-uncovered` | load | a window (the last also after a run with no refused frame) |
| `ambiguous-expectation` | load | an expectation that is not exactly one of the two forms |
| `instant-off-cadence` | load | a diagnostic `at_s` that is not a tick of the clock |
| `mutant-path-invalid`, `duplicate-mutant` | load | a mutant path that is absolute, has a `..`, `.` or empty segment or a backslash, or repeats |
| `subject-load-failed` | bind | a subject the engine refuses to load |
| `unknown-input`, `ambiguous-input`, `duplicate-input`, `input-not-supplied` | bind | input names |
| `unknown-output`, `ambiguous-output` | bind | output names |
| `param-path-unknown`, `param-path-ambiguous`, `param-path-duplicate` | bind | a parameter key that matches nothing, matches several, or resolves to the parameter another key of the same map names |
| `param-refused` | bind | an edit the engine still refuses when a pass applies nothing (decision 14), with the `OcError` class: `param-range`, `param-type` or `param-structural` *(amended 2026-11-10, reading 2)* |
| `value-type-mismatch`, `value-not-representable` | bind | a JSON value its port or parameter type cannot take |
| `enum-port-unsupported`, `unsupported-port-type` | bind | an Enum boundary input or output (until #465), a String port |
| `too-many-frames` | bind | the total frame budget |
| `mutant-missing`, `mutant-unlisted`, `too-many-mutant-bytes` | bind | mutant bytes not supplied, supplied without a listing, or past the total byte bound |
| `mutant-load-failed`, `mutant-boundary-differs`, `mutant-identical` | bind | an invalid mutant |
| `run-refused` | run | an `EngineRefusal::Run` refusal: about the run, not a frame |
| `replay-failed` | run | any other `ReplayError`, with `cause` from a closed kebab-case table `oce-verify` owns and pins (`trace-too-large`, `output-set-changed`, `grid-not-increasing`, `engine-setup`, `source`), never a Rust variant name; `engine-setup` also covers every replay error the table does not name *(amended 2026-11-10, reading 1)* |

The first four rows reuse the spellings `oce-cli` already prints for the wire form, so `oce vocab`
has one word per refusal. The parameter codes are spelled `param-path-…` because
`unknown-parameter` and `duplicate-parameter` are already stage-1 finding codes, and
`unknown-param` and `duplicate-param` already belong to `oce catalog --param`. The CLI adds only
what is its own: its existing `not-found`, `not-a-file`, `read-failed`, `write-failed`, and a new
`unknown-subject-form`.

## Rust API sketch

```rust
// crates/leverage/oce-verify: oce-api and oce-replay only among oce-* packages.
pub const VECTORS_SCHEMA: &str = "oce/vectors/v1";
pub const RECEIPT_SCHEMA: &str = "oce/receipt/v1";
pub const VECTORS_JSON_SCHEMA: &str = include_str!("../schema/vectors.schema.json");

pub struct VectorSet { /* clock, params, mutants, scenarios; numbers keep their lexical kind */ }
impl VectorSet {
    /// Strict, bounded; opens nothing.
    pub fn from_json_bytes(bytes: &[u8], limits: &VerifyLimits) -> Result<Self, VectorError>;
    pub fn to_canonical_json(&self) -> String;
    pub fn spec_id(&self) -> String;                 // "spec:fnv1a128:<32 hex>"
}

/// The bytes a run reads, supplied by the caller. `mutants` pairs each listed path with its CXF.
pub struct VectorInputs<'a> { pub subject: &'a [u8], pub mutants: &'a [(&'a str, &'a [u8])] }
pub struct VerifyOptions { pub limits: VerifyLimits, pub timings: Option<Timings>,
                           pub keep_traces: bool,
                           pub chunk_frames: usize }  // frames per `Replay::advance`; 4,096
/// Every bound the format states that is not fixed by the strict profile. Lower-only.
pub struct VerifyLimits { pub max_bytes: u64, pub max_ticks: u64, pub max_scenarios: u32,
                          pub max_expectations: u32, pub max_parameters: u32,
                          pub max_mutants: u32, pub max_mutant_bytes: u64,
                          pub max_recorded_failures: u32, pub max_recorded_warnings: u32,
                          pub max_total_frames: u64, pub replay: oce_replay::ReplayLimits }

/// `Err` for every load, bind and run refusal, an invalid mutant included; then no receipt.
pub fn run_vectors(set: &VectorSet, inputs: &VectorInputs<'_>, options: &VerifyOptions)
    -> Result<VectorRun, VerifyError>;
pub struct VectorRun { pub receipt: Receipt, pub traces: Vec<(String, oce_replay::Trace)> }

pub struct Receipt { pub header: ReceiptHeader, pub body: ReceiptBody }
#[non_exhaustive]
pub enum ReceiptBody { Vectors(VectorBody) }         // the commissioning PR adds its variant
impl Receipt { pub fn to_canonical_json(&self) -> String; }
pub enum ScenarioStatus { Pass, Fail, NoData }
pub enum MutantStatus { Caught, Survived, NotRun }
```

`ReceiptHeader` is the commissioning sketch's. `run_vectors` blocks; inside, each scenario drives
`Replay::advance(chunk_frames)`, and a test proves chunk sizes 1, 7 and the whole run give one
`receipt_id`.

## CLI

```text
oce verify <VECTORS> --subject <FILE> [--subject-form cxf|text|wire] [--out <DIR>]
           [--require-mutants-caught] [--deny-warnings] [--timings]
```

One vector file and one subject per run; there is no batch mode.

**Forms.** The subject's form is, in order: `--subject-form` when given; CXF for a name ending in
`.jsonld` or `.cxf.json`; text for `.oce`; wire for any other `.json`; otherwise
`unknown-subject-form`. The `.cxf.json` test comes first because `oce-cli`'s `form_of` reads every
`.json` as the wire form, and `verify` does not use it. A text or wire subject goes through both
validation stages and `oce_author::emit` with `EmitOptions::default()`; there is no `--namespace`,
because the namespace is inside every path the receipt prints, and a caller who wants another runs
`oce emit` first. An error finding there is `subject-load-failed`, exit 2. A CXF subject goes to
the engine untouched and never through the importer (`oce import`, #514), so the library's two
hand-written routine graphs verify like any other. A mutant is always CXF, whatever its name.

**Reads.** The vector file is bounded at 4 MiB, the subject and each mutant at
`oce_api::MAX_CXF_BYTES` (8 MiB), and the mutants together at 64 MiB
(`VerifyLimits::max_mutant_bytes`), so 256 mutants are never 2 GiB in memory. Each size is checked
from metadata before a byte is read, as every `oce` command does.

**Output.** Stdout carries one document: the receipt when a verdict exists (exit 0 or 1), the
`oce-report/1` report otherwise (exit 2, with the refusal in `errors[]`). With `--out`, the
receipt is written atomically to `<DIR>/receipt.json` and stdout is the report, with `passed`,
`status`, `receipt_id` and `output`, as `emit -o` does. On exit 2 nothing is written.

| Exit | When |
| --- | --- |
| 0 | every scenario passed, no mutant survived under `--require-mutants-caught`, and no warning under `--deny-warnings` |
| 1 | a scenario failed, or the set is `no_data`; under their flags, a surviving mutant or a warning |
| 2 | usage, an unreadable or refused vector file, a subject that does not load, an unresolved name, an uncovered window, an invalid mutant, a run refusal, an I/O failure |

The two flags change the exit code only; no receipt byte depends on them. `--deny-warnings` counts
`load_warnings_total` and `totals.warnings`. `oce vocab` lists the new codes. Commissioning stays
`oce verify --commission <SPEC>`; #456 item 7's "`oce commission` subcommand" is reworded to
match.

## Fixtures

**What is rewritten, in the part 1 PR.** Not imported: there is no reader of the old shape.
*(Landed in GitHub #541.)*

- `crates/oce-api/tests/fixtures/library_rules/AHU-0016/vectors.json` and
  `AHU-0004/vectors.json`: the `schema` line only. Every other byte already satisfies the format
  above (`description` included), which is why `description` is a field.
- `crates/oce-api/tests/fixtures/library_rules/README.md`: the two `vectors.json` SHA-256 rows,
  and the sentence that the files are byte-for-byte library copies, which stays true of the two
  `rule.cxf.jsonld` files only.
- `crates/oce-api/tests/library_rule_state_continuation.rs`: the `schema` assertion, and the
  `#`-boundary arm of its name rule. An engine crate cannot depend on a leverage crate, so the
  test keeps its own small reader of the new shape.
- No file under `tools/golden-gen/goldens` and no `.trace` golden reads a vector file; none moves.
  `oce-conformance` has no vector format and is not touched.

**Reference runs.** The two in-repo rules, 16 scenarios: 217 frames and 216 checked instants for
AHU-0016, 441 frames for AHU-0004. Their pass receipts are goldens. That they pass is inferred from
`library_fixtures_meet_their_own_vectors_uninterrupted`, which checks every window today; it was
not run for this note.

**What moves a golden receipt.** `engine_version` is inside the hashed part, so every workspace
version bump re-blesses every golden receipt, as it already does the blob goldens. The owner ruled
on 2026-11-09 that `development` moves to 0.5.0 in #509 and bumps when a format changes; the
examples here show 0.5.0.

**Re-vendoring.** Both vendored rules carry a retired root name, which is inside every canonical
path and the content id. When #517 re-vendors them, the golden receipts (their `content_id`,
`spec_id` and output paths) and the mutants, which are hand edits of the rule's CXF, are re-made in
that PR. The paths in this note's examples are today's.

**Fail and `no_data` receipts.** Hand-edited copies kept beside the goldens: the 900 s edit shown
above, and a set whose one scenario has an empty `expect`.

**Mutants.** None exist as files anywhere. Three are written for AHU-0016, in
`crates/leverage/oce-verify/tests/fixtures/AHU-0016/mutants/`, each a hand edit of the rule's CXF
with a provenance line in its `description`:

| Mutant | Edit | Expected |
| --- | --- | --- |
| `threshold_inclusive` | `htgThr` and `clgThr` read `u >= 5` (a `Not` over `Reals.LessThreshold`) | caught by `threshold_edge` |
| `no_persistence` | `persist.delayTime` 900 → 0 | caught by `both_open` |
| `late_alarm` | `persist.delayTime` 900 → 930 | survives: no window asserts the tick at 900 s |

The expectations are read off the rule and its vectors, not run; part 3's goldens are the
evidence. `late_alarm` survives by design of the library's own contract ("leave ≥ one step of
margin around an `alarm_delay` edge"), so the fixture shows a score reporting a real gap. Two
constraints from the tree shape the layout. Mutant paths cannot leave the vector file's directory
(decision 28), so that directory holds its own `vectors.json`: the AHU-0016 scenarios plus the
`mutants` list, held equal to the `oce-api` copy's `clock` and `scenarios` by a test, with the
subject read in place. And the files are named `.cxf.json`, not `.jsonld`: `oce-cxf`'s
`parameter_grounding_corpus` test pins the count of every tracked `.jsonld` file in the workspace.

## Decisions

**1. Schema tag `oce/vectors/v1`, one format, changed in place.** Beside `oce/receipt/v1` and
`oce/commissioning-spec/v1`. `cxf-library/vectors/v1` and every other tag are `unknown-schema`.
Rejected: `v2` and an importer or dual-read path (the ruling); no tag (a foreign file could not be
refused by name).

**2. One strict reader in `oce-verify`, shared with the commissioning spec loader.** The authoring
wire profile and bounds, with the same code spellings. Rejected: depending on `oce-author` for its
reader (the leverage rule would allow the edge, but the scan in `wire/scan.rs` and the writer in
`json.rs` are private, and the edge would pull the authoring model into the verifier); `serde_json`
alone (it keeps the last duplicate key and has no bounds); a shared strict-JSON leverage crate now
(the right end state, since `oce-author` already has two readers, but it is a refactor of that
crate and its own issue). Why: hostile input is the default assumption.

**3. One set-level clock on `TimeBase::Grid`, with cadence decided by multiplication.** As
[the format](#the-vector-format) states: a time is on the cadence when some integer `k` gives it
exactly under double multiplication, found by rounding the quotient and testing `k - 1`, `k` and
`k + 1`. The horizon is added through `StepSource::with_instants`. Rejected: `floor` of the
quotient (the first draft; it refuses `{ step 0.1, horizon 4.3 }`, which is on the cadence);
`Recorded` time (the 33 off-grid step times in the library would become frames the old runs never
had); a clock per scenario (no vector needs one); accumulating ticks (drift). Why: this is the old
verifier's schedule, and one rule answers "is a tick" for the horizon, a window and `at_s`.

**4. Step lists are closed `{ "t", "value" }` objects from `t = 0`, never past the horizon.**
Rejected: `[[t, v]]` pairs (positional, against the closed-object profile, and 9,182 library lines
to rewrite); a list starting later (a missing input and a refused frame); a step after the horizon
(under `Grid` it would extend the run past the horizon; the 5 such steps in the library, in
`HP-0002` and `HW-0001`, are edited there).

**5. Constants and step lists only; no series or CSV input in the first cut.** An object in an
input position is `wrong-type`. File-backed series arrive in place with the commissioning sources
(tracking item 7), which must build the union of times (commissioning decision 15). Rejected: mixed
sources now (a `Replay` takes one `FrameSource`, and nothing defines how they combine; no existing
vector uses one).

**6. One name rule for inputs, outputs and parameters.** The exact canonical path, or a unique
`.`-boundary suffix; ambiguous, unknown, doubled and unsupplied names are typed errors. Part 2
makes the rule one public function in `oce-replay`, because `resolve_target` is private to
`InputMap::resolve` today and knows inputs only. Rejected: a copy of the rule in `oce-verify` (two
implementations of one contract); the old reader's `#` boundary (both in-repo rules resolve at a
`.` boundary, and the issue's static check says all 647 library point names do).

**7. Values are typed by the declared port or parameter type, at bind.** A Real takes any JSON
number: an integer token is taken when it converts to binary64 exactly, and `-0` as an integer
token is the integer zero, so `+0.0`; negative zero is written `-0.0`. An Integer takes an integer
token only (`1.0` is refused). A Boolean takes `true` or `false`, never `0` or `1`. An Enum
boundary port, input or output, is `enum-port-unsupported` until #465. Output types come from
`FrameSlots::output_types()` (#460), not from the first accepted frame. Rejected: typing by the
JSON token (the corpus writes `0` and `1.0` for Real ports); coercing `1.0` to an Integer (a
silent conversion); typing `equals` after the run from the trace's columns (the first draft: a
type error would surface after frames ran, and a mutant's output types could not be compared at
all). *(Ruled 2026-11-09: #460 moves into M3.)*

**8. `equals` is exact for every type; a Real compares by bits.** The parsed number's bits against
the output's, so `0` on a Real output is `+0.0` and does not equal `-0.0`, and no finite literal
equals a NaN. The library contract's "optional `tolerance` (default 1e-9)" is dropped: no vector
writes the key, so the slack was never visible in a file. A window that needs slack is rewritten to
`within`. All 21 in-repo windows are Boolean and unaffected; the library has 45 windows with a
non-Boolean `equals`, all in one routine (`airflow-setpoint`), so that one file is where the
library decides window by window. Rejected: a default tolerance (a hidden band under a receipt
that says `exact`); a `tolerance` key beside `equals` (two tolerance spellings). *(Ruled
2026-11-09: accepted, with decision 7's `-0` rule.)*

**9. `within` uses the commissioning tolerance names, and is refused until it exists.** `atoly`,
`rtoly`, `ltoly`, `atolx`, `rtolx`, `ltolx`, one type shared with tolerance mode. Until the funnel
port (tracking item 6) the key is `unknown-key`, never ignored; that PR adds `within` and `of` in
place and amends this note with their window semantics. Rejected: the sketch's `atol`, `rtol`,
`ttol_s` (a second spelling of one type); accepting and ignoring the key (`oce-verify` may not ship
a `todo!` or a silent pass). *(Ruled 2026-11-09: accepted.)*

**10. `when` names Boolean signals, and waits with `within`.** A list of input or output names,
ANDed, masking instants that are counted as `masked` and never passed (commissioning decision 2).
`unknown-key` until item 6, when the trace also starts retaining inputs. Rejected: the sketch's
time-window indicator (time gating is already `from_s` and `to_s`; one meaning across both specs).

**11. Windows are inclusive, inside the clock, and must contain a tick.** Refused at load from the
clock alone. `Trace::check_coverage` (replay decision 7) runs again after a scenario in which no
frame was refused; a scenario with a refused frame fails instead and is not coverage-checked
([Binding and running](#binding-and-running)). Rejected: a pass over a window no tick reached; a
window running past the horizon (it would check fewer ticks than it states); checking coverage
after a refused frame (it would turn decision 26's failure into an exit 2).

**12. A diagnostic expectation matches `block` and, optionally, `message` at a tick.** That is all
that exists to match: no code, and `block` is the class, not the instance. Two instances of one
class are told apart only by `message`. Matching an engine-written message pins its wording, so
the receipt reports the text and a vector should match `message` only for `CDL.Utilities.Assert`,
whose text the author wrote. An unexpected warning is reported and never fails a scenario; the old
verifier ignored warnings, so the rewritten corpus keeps its verdicts. Because the rows are capped
at 16 per scenario, the body carries `warnings_total` per scenario and `totals.warnings` for the
set, and `--deny-warnings` turns any of them into exit 1 without changing a receipt byte. A stable
run-time code and an instance path on `AssertEvent` are a facade change, a named follow-up not
designed here. Rejected: the sketch's code match (nothing to match); failing on any warning by
default (`Reals.MovingAverage` warns on close ticks, and whether a corpus vector triggers it is
unknown). *(Ruled 2026-11-09: accepted, with the totals and the exit-only flag.)*

**13. Expected state is deferred.** No encoding exists: a snapshot is opaque bytes, and a digest of
them would move with every state-format change. The follow-up issue is "vectors: assert named
state", scoped to values a host can already read. Rejected: a snapshot digest in v1. *(Ruled
2026-11-09: deferred; tracking item 4 loses the words "expected state".)*

**14. `params` at set and scenario level, resolved, ordered and echoed.**

- **Resolution.** Every key resolves by the name rule to one canonical parameter path. Two keys of
  one map that resolve to the same parameter are `param-path-duplicate`.
- **Override.** A scenario's map overrides the set's by **resolved path**, whatever either
  spelling was. The effective edits of a scenario are the union, one value per path.
- **Order.** `set_param` checks class rules against the table as it stands
  (`crates/oce-api/src/params/class_rules.rs`: `Reals.Hysteresis` requires `uLow <= uHigh`), so
  the result depends on the order of edits, and the canonical form sorts keys. The edits are
  therefore applied inside one halt, in passes: each pass tries the remaining edits in ascending
  canonical-path byte order; a refused edit leaves the table unchanged and waits for the next
  pass; a pass that applies nothing ends it, and the first edit still waiting is `param-refused`.
  Then `resume`. The order is a function of the resolved paths and values alone.
- **Limitation.** The passes can refuse a table that only another order of edits reaches: an edit
  applied in an early pass can make a later one invalid where the reverse order would not. That
  is `param-refused` too. It is accepted and documented, not worked around: such a vector splits
  the edits across scenarios, or the subject is emitted with the values. *(Ruled 2026-11-09.)*
- **Echo.** The receipt lists the effective edits by canonical path. `Engine::content_id` moves
  with any applied parameter bit, so such a scenario also records the `content_id` it ran under;
  the header's is the subject's as loaded.

At most 256 parameters per map, which bounds the passes *(ruled 2026-11-09)*. Rejected: file order
(the canonical form has none); one pass in path order (lowering both hysteresis thresholds would
be refused: `uHigh` sorts first and would drop below the old `uLow`); override by spelling
(`persist.delayTime` and its full path would both apply); the edited id in the header (a set can
have several; the registry keys on the document); set-level parameters only (a threshold-edge
scenario wants its own).

**15. Each scenario runs on a freshly loaded engine.** The old contract. Rejected: one engine for
the set (state leaks between scenarios); restoring a post-load snapshot (a later optimization that
would put the restore path under every verdict).

**16. Canonical JSON with sorted keys and one number spelling; `spec_id` over it.** As
[Canonical form](#the-vector-format): keys by UTF-8 bytes, lowercase `\u` escapes, integers as
integers with `-0` written `0`, reals shortest round-trip with the exponent form outside
`1e-4 <= |x| < 1e16`. FNV-1a-128 (commissioning decision 20) is implemented in `oce-verify` with
pinned test vectors, since the facade exposes no FNV function. The authoring wire writer keeps
built key order; this one sorts, as the `oce-report/1` report does, because a hash input needs one
order that no builder chooses. `spec_id` covers the mutant paths; the receipt adds each mutant's
digest. Rejected: "shortest round-trip" with no stated threshold (two correct writers could
disagree on `1e16`).

**17. A pinned JSON Schema.** `crates/leverage/oce-verify/schema/vectors.schema.json`, derived
with `schemars` from the reader's types and regenerated by a diff test, as
`sequence.schema.json` is. `schemars` is already in the lockfile. Rejected: a hand-written schema
(it drifts from the reader); none (agents write vectors).

**18. Closed error kinds with stable codes.** As [Error codes](#error-codes).

**19. The in-repo rules are M3's replay evidence, in the leverage lane.** An `oce-verify` test
reads the two rule directories in place and compares golden receipts, so every leverage and engine
PR runs it. The library's sets replay in the library's repository, and against this one in the
release gate under #516 (M3b). *(Ruled 2026-11-09: accepted.)*

**20. `oce-verify` takes bytes for the subject and every mutant.** It opens no file (commissioning
decision 25). The CLI, which already links `oce-author`, emits `.oce` and wire subjects first.

**21. One `Receipt` with a closed `ReceiptBody` enum, and `run_vectors`.** This replaces the
commissioning sketch's `body: CommissioningBody`. Rejected: two receipt types (two writers of one
header).

**22. The vector body is the field table.** [The receipt](#the-receipt) is normative: every field,
its type, when it is present, its order and its bound. Counts are of instants, because "zero
checked" must mean zero ticks compared, and a diagnostic expectation is one instant.

**23. A failure record is shaped by its code.** `value-mismatch` carries `expectation`, `output`,
`t`, `expected`, `actual`; `diagnostic-missing` carries `expectation`, `t`, `block` and the
expected `message`; `frame-refused` carries `t` and `refusal` and no `expectation`, since no
expectation was reached. In tick order, then expectation order; at most 16 per scenario with
`failures_total` kept. The outline's `scenario` field is the enclosing object. Rejected: one flat
shape with nulls; an unbounded list.

**24. Exact numbers in a receipt.** As [Values](#the-receipt). Rejected: plain JSON numbers
throughout (a parse can lose `-0.0` or a NaN payload, as the diff note warns).

**25. "Engine diagnostic codes" are three different things.** A load refusal or load warning
carries `DiagCode` spellings (a warning is reported, never a refusal). A refused frame carries the
`EngineRefusal` class, which has a numeric `tag()` and no name today, so `oce-verify` owns the
kebab-case table (`input-type`, `input-non-finite`, `input-domain`, `non-finite-time`,
`time-out-of-range`, `time-regression`, `non-finite-time-step`, `model-time-unrepresentable`,
`run`), held complete by an exhaustive match. A run-time warning carries `block` and `message`.

**26. Verdicts and exits.** A scenario is `fail` when a frame was refused or an instant failed,
else `no_data` when no instant was checked, else `pass`. The set is `fail` when any scenario
fails; otherwise `no_data` when it has no scenarios or any scenario is `no_data`; otherwise
`pass`. `fail` and `no_data` are exit 1. A frame-class refusal fails its scenario with one
`frame-refused` failure; instants before it count, later ones do not, and coverage is not checked
for that scenario. A `run`-class refusal, a subject that does not load, an unresolved name, an
uncovered window, an invalid mutant and an I/O failure are exit 2 with no receipt. Rejected: every
refusal as exit 2 (a mutant that narrows an input bound would then be an error, not a catch).
*(Ruled 2026-11-09: accepted for vectors. The commissioning note sends an engine refusal to exit
2; whether tolerance mode follows this rule is that work's to settle.)*

**27. A mutant has the subject's boundary and a different content id, or the run is an error.** As
[Mutant mode](#mutant-mode): validated at bind, output types through #460, and an invalid mutant
makes `run_vectors` return `Err`. Equal ids promise equal outputs for one build, so an identical
mutant proves nothing. Rejected: counting an unloadable mutant as caught (it never ran); an
`invalid` status inside a receipt (the first draft: the body would count what the exit code says
cannot be reported, and `--out` would have to write a receipt on exit 2); a first-frame probe for
output types.

**28. Mutants are listed in the vector file by relative path.** `{ "path", "description"? }`,
`/`-separated, inside the file's directory. Only the CLI resolves them; `oce-verify` sees CXF
bytes. The `mutants` key is added to the format in place by part 3; until then the reader refuses
it as `unknown-key`, so no set that lists mutants is ever replayed with them ignored. Rejected:
the sketch's `.ocs` extension (neither text nor CXF); a `..` segment (a fetched vector file must
not direct a read outside its own tree); mutants passed only on the command line (`spec_id` would
not cover which mutants a stamp refers to).

**29. The catch rate is two integers.** `caught` and `total`, with per-mutant status and first
failing scenario. Rejected: a float (not byte-stable); scoring against a failing subject.

**30. The score never gates `passed`.** `--require-mutants-caught` changes the exit code only.
*(Ruled 2026-11-09: accepted.)*

**31. `oce-verify` measures nothing.** `timings` is always present, and zero unless the caller
passes durations. Rejected: omitting it (a header whose shape depends on a flag); measuring in the
library (a clock in a crate that runs none). *(Ruled 2026-11-09: accepted.)*

**32. Exact mode is `{ "mode": "exact" }`.** No envelope fields.

**33. `engine_version` is `CompatibilityDescriptor::current(None).oce_api_version()`, and it is
hashed.** The facade's Cargo version, which is the workspace version: the first PR after a release
tag that changes a format's bytes or meaning bumps it on `development` to the next milestone's
version (#509); by that accessor's own words not a build identity. It stays inside `receipt_id`, so a version bump
re-blesses every golden receipt. Rejected: leaving it out of the hashed part (two engine versions
could then issue one receipt id).

**34. `oce verify <VECTORS> --subject <FILE>`.** As [CLI](#cli): one vector file and one subject
per run, the form rule that reads `.cxf.json` as CXF, default emit options for an authored
subject, bounded reads. The plan's `oce verify --all` is not designed and no batch mode is asked
for; the library loops over its bundles in its own gate script. One thing changes in `oce-cli`: an
`Outcome` must carry a document with exit 1, and the crate README's rule that every exit 1 prints
the report gains this exception. Rejected: the receipt always nested inside a report (the stored
artefact and the stdout bytes would differ); a separate `oce commission` verb; reusing `form_of`
(it reads `rule.cxf.json` as the wire form). *(Ruled 2026-11-09: accepted, with the `.cxf.json`
rule.)*

**35. `VerifyLimits` holds every bound that the strict profile does not fix.** As the sketch:
bytes, ticks, scenarios, expectations, parameters, mutants, mutant bytes, recorded failures and
warnings, and `max_total_frames` 16,777,216 across scenarios and mutants. Lower-only, each a typed
error pinned at its boundary; the tick bound is checked at load, before any engine exists.

**36. `oce check` does not return in part 4.** The receipt defines no load report. What binding
needs (inputs with types and bounds, outputs, parameters) is what `oce check` should print, so it
is cut as its own follow-up issue after part 4, and the authoring note's amendment is dated then.
The content id it was also to print has its own verb, `oce id` (#513).

**37. `oce/receipt/v1` for both kinds.** Changed in place until 1.0.0, pinned by golden (#456,
"Receipts drive agents").

## Where the tree overrules the outline

- **`description`.** The outline's field list omits it; both in-repo files and the library
  contract carry it per scenario. It is an optional field.
- **Steps after the horizon.** The outline infers that `Grid` stops at the last sample time.
  Confirmed in `grid_step`, and the converse matters too: a step after the horizon extends the
  run. Hence `step-after-horizon`.
- **Output name resolution.** There is no replay rule for outputs to reuse: `resolve_target` is
  private and input-only (decision 6).
- **`block` is a class.** The outline's "matching `block` exactly" cannot pick an instance
  (decision 12).
- **`oce-author`'s reader.** Not only outside the stated dependency set: private, and one of two
  strict readers in that crate (decision 2).
- **Sorted keys.** The canonical writer the outline points at does not sort (decision 16).
- **Enum class paths.** `oce_api::enumeration_class_path` exists (GitHub #488), while
  `oce-replay`'s docs and canonical bytes still say the facade has no lookup. That is #465's
  remaining half.
- **A Real `>=`.** The catalog has `Reals.Greater`, `GreaterThreshold`, `Less` and
  `LessThreshold` only, so the outline's "`>` made `>=`" mutant is two blocks, not a class swap.
- **Mutant location.** The outline puts mutants under `oce-verify`'s fixtures and names them by a
  path relative to a vector file that lives under `oce-api`'s. Both cannot hold without `..`
  ([Fixtures](#fixtures)).
- **Stdout on exit 1.** `Outcome::document` implies exit 0 today (decision 34).
- **Output types.** The outline held that #460 was not needed because expectations could be typed
  "from the trace's columns or `Engine::point_list`". Nothing types an output before the first
  accepted frame, and binding and mutant validation both need it then (decisions 7 and 27).
- **`.cxf.json`.** `oce-cli`'s `form_of` reads every `.json` as the wire form (decision 34).
- **Parameter edits are order-dependent.** `set_param` checks class rules against the current
  table, which the outline's "applied through halt, `set_param`, resume" leaves open (decision
  14).
- **Numeric `equals`.** The outline's "31 Real `equals` windows" came from a stale clone. At
  `124427b` 45 windows carry a non-Boolean `equals`, all in one routine; no fault vector has one.

## Dependencies and sequencing

1. **#509** (the six format revisions reset to 1, `development` to 0.5.0) is ahead of this work on
   the tracking checklist. Nothing here embeds one of the six: the receipt carries
   `oce/receipt/v1`, the `oce:canon1:` id and the facade's Cargo version; the trace is
   `TRACE_VERSION = 1`; the verifier takes no state snapshot. Golden receipts are blessed after it,
   because they hash the version.
2. **#460 moves into M3**, as a core PR before part 2: `FrameSlots::output_types()` in slot order.
   Part 2's binding and part 3's boundary comparison both read it. It was headed for M6 until the
   ruling of 2026-11-09.
3. **Part 1 of 4, the format.** The `oce-verify` scaffold, strict reader, canonical writer,
   `spec_id`, limits, error codes, pinned schema, and the in-repo rewrite. `mutants` is not yet a
   key. It touches `crates/oce-api/tests` and `Cargo.lock`, so it runs the determinism lane.
   *(Landed in GitHub #541.)*
4. **Part 2 of 4, replayer and receipts.** Binding, the public name rule in `oce-replay`,
   parameter edits, the run, exact comparison, the header and vector body, `receipt_id`, golden
   receipts for pass, fail and `no_data`, chunk-size independence.
   *(Landed in GitHub #558.)*
5. **Part 3 of 4, mutant mode.** The `mutants` key in place, validation, the score, the receipt
   section, the AHU-0016 fixtures. Between parts 2 and 3 a set that lists mutants is refused, not
   replayed without them.
6. **Part 4 of 4, `oce verify`.** The verb, codes, `oce vocab`, golden transcripts, README and
   CHANGELOG.
7. **Follow-ups, cut with the four:** `oce check`; "vectors: assert named state"; a run-time
   warning code and instance path on `AssertEvent` (facade); a shared strict-JSON reader.
8. **#465** (replay's Enum columns) lands before an Enum boundary port can appear in a vector;
   until then the typed refusal stands, so it blocks nothing.
9. **Tracking items 6 and 7** add `within`, `of`, `when` and file-backed series in place.
10. **Beside this work, not blocked by it:** `oce id` (#513) and `oce import` (#514); #517
    re-vendors the two rules after the library renames them, and re-makes the receipts and mutants
    with them. **After it, in M3b:** the registry index and `oce fetch` note (#515) and the
    release-gate replay of the library's corpus (#516); library side,
    jscott3201/open-control-library#105 to #108.

## Test plan

- **Reader.** Each bound accepted at the limit and refused one past; one refusal per code in the
  table, pinned by code; load, write, load is a fixed point; the two older schema tags refuse;
  canonical numbers at `1e-4`, `1e16`, `-0` and `-0.0`.
- **Clock and windows.** `{ step 0.1, horizon 4.3 }` accepted; a horizon one ulp off the cadence;
  a window touching a tick at each bound; a window between two ticks; a step time on, between and
  after ticks; `at_s` on and off the cadence; the tick bound.
- **Binding.** Exact path, unique suffix, ambiguous, unknown, doubled and unsupplied names for
  inputs, outputs and parameters; every value-typing rule, `-0`, `1.0` on an Integer port and an
  integer token past 2^53 on a Real port included; an `equals` of the wrong type refused before a
  frame runs.
- **Parameters.** Two spellings of one path; a scenario overriding the set by another spelling;
  raising and lowering both hysteresis thresholds; an edit no order admits; the receipt's order.
- **Verdicts.** Golden receipts for pass, fail and `no_data`; a diagnostic-only scenario; a
  refused frame mid-scenario giving exit 1 with later windows uncounted; `receipt_id` equal across
  runs and chunk sizes and unchanged by `timings`; `-0.0` and a NaN output in a failure.
- **Mutants.** Caught, survived and `not_run` with their goldens; an identical mutant, a boundary
  that differs by one path and by one output type, and an unloadable mutant each an error with no
  receipt; the byte bound.
- **CLI.** A transcript per exit class; `--out` atomic write and nothing written on exit 2;
  `rule.cxf.json` read as CXF; a mutant path with `..` refused; both exit-only flags leaving the
  receipt bytes unchanged.
- **Isolation.** The 46 fixture goldens and 422 provenance goldens stay byte-identical.

## Out of scope

- The registry and `oce fetch` (#515), the release-gate replay of the library's corpus (#516), the
  library re-stamp and "ids agree" (M3b); a batch mode such as `oce verify --all`.
- Tolerance mode, the funnel port, `within`, `of`, `when`, series and CSV inputs (items 6 and 7).
- Expected state; a run-time warning code; any change to `oce-api` other than #460.
- Writing traces to disk (`Trace::canonical_bytes` is not a published format) and a resumable
  `run_vectors` for the browser (M4).
- The old-id column of the content-id pin (`restamp.tsv` then, `pin.tsv` now) and the "library
  bundle v2" wording in the authoring, importer and content-id notes: the issue lists them, and
  they belong to #509 and M3b.

## Review rulings

**Before the draft.** The owner ruled on 2026-11-09, and the note states these without reopening
them:

- Every format and schema stays at revision 1 until 1.0.0, with no backwards compatibility. The
  vector format is one format, `oce/vectors/v1`, defined in place: no importer, no dual-read path,
  no mapping table. The in-repo fixtures and their reader are rewritten; the library converts its
  own corpus once.
- The six formats numbered above 1 are reset by #509.
- M3 closes on evidence this repository controls; the registry, `oce fetch`, the library re-stamp
  and "ids agree" move to M3b.
- Reference mutants are written in this repository. The standard is cited as ANSI/ASHRAE
  231-2026.

**The independent review** of the draft (2026-11-09) found six gaps, closed in the
note as it stands: output types had no bind-time source; the receipt had examples but no field
table; coverage was checked after a refused frame, contradicting decision 26; an invalid mutant
was both a receipt count and an exit 2; the clock rule floored a quotient and refused valid
horizons; parameter edits had no order. It agreed with the draft's ten recommendations, amended
three and added an eleventh.

**The owner's rulings** (2026-11-09, "accept all"): the eleven questions the draft put, each
decided as recommended, the review's amendments included.

1. **Real `equals` is exact, by bits** (decision 8). The unwritten `1e-9` slack is dropped. `-0.0`
   is not `0`; an integer token `-0` is the integer zero, and negative zero is written `-0.0`
   (decision 7). The library's 45 non-Boolean windows need exact literals or wait for `within`.
2. **Tolerance names are the commissioning note's** (`atoly`, `rtoly`, `ltoly`, `atolx`, `rtolx`,
   `ltolx`); `within` and `when` are unknown keys until the funnel port (decisions 9 and 10).
3. **Expected state is deferred** to a named follow-up; tracking item 4 loses the words
   (decision 13).
4. **Unexpected warnings are reported, not failed**, at load and at run time, with a set-level
   total beside the capped rows and an exit-only `--deny-warnings` flag (decisions 12 and 25).
5. **A frame-class engine refusal fails its scenario** (exit 1) with the refusal class; a
   run-class refusal is exit 2 (decision 26).
6. **The mutant score never gates `passed`**; `--require-mutants-caught` changes the exit code
   only (decision 30).
7. **Three hand-written AHU-0016 mutants, one a deliberate survivor**, under
   `crates/leverage/oce-verify/tests/fixtures/`, within standing rule 7 ([Fixtures](#fixtures)).
8. **Timings are zero by default**; `oce-verify` measures nothing and the CLI measures only under
   `--timings` (decision 31).
9. **`oce verify <VECTORS> --subject <FILE> [--out <DIR>]`**, the receipt on stdout, refusals as
   the `oce-report/1` report; a subject named `*.jsonld` or `*.cxf.json` is CXF, tested before the
   `.json`-is-wire rule; commissioning stays `oce verify --commission <SPEC>` (decision 34).
10. **The in-repo replay gate is an `oce-verify` test in the leverage lane**, reading the two rule
    directories in place; the library's corpus is M3b's (decision 19).
11. **#460 moves into M3**: `FrameSlots::output_types()` in slot order, a core PR ahead of the
    first `oce-verify` code, so outputs are typed before any frame runs (decisions 7 and 27).

**Three points the draft flagged**, ruled by the orchestrator within what the owner accepted
(2026-11-09):

- Pass-ordered parameter edits may refuse a table that only another order reaches. Kept, as a
  documented limitation with the code `param-refused` (decision 14).
- The canonical real spelling's thresholds were written from memory of Rust's formatter. The
  paragraph is marked to be confirmed against the implementation, and the code's behaviour is
  written back into this note in that PR (decision 16). *(Confirmed in GitHub #541: both
  thresholds and the exponent spelling stand as written; the paragraph now says so.)*
- 256 parameters per map stands.

### Part 2's readings

*(Added 2026-11-10, with GitHub #558, issue #520.)* Building the run (part 2) needed ten
readings where this note was silent. The owner ruled on readings 2, 7 and 8 and on the
refusal-slot question below; the independent review of GitHub #558 found the other seven
consistent with the note. They are normative from here, so parts 3 and 4 inherit them.

1. **`replay-failed` causes.** The table's five causes stand. `source` is `ReplayError::Source`.
   `engine-setup` is everything `Replay::new` refuses, every facade setup call that fails after a
   successful load (a scenario's fresh load, `content_id`, `input_definitions`, `frame_slots`, the
   parameter `halt` and `resume`, a `set_param` refusal that is not about the edit's value), and
   every replay error the table does not name.
2. **`param-refused`'s class** *(confirmed by the owner).* The closed table `param-range`
   (`OcError::ParamRange`: outside the declared bounds, or a class rule such as `uLow <= uHigh`),
   `param-type` (`OcError::ParamType`) and `param-structural` (`OcError::ParamStructural`), carried
   as the refusal's cause. Any other refusal of `set_param` is `replay-failed` with `engine-setup`.
3. **A frame replay refused for a missing input** (`RefusalCause::MissingInput`) has no
   `EngineRefusal` class for a `frame-refused` record. It cannot happen: every vector input is a
   step list from `t = 0`, which is always current. If it did, it is `replay-failed` with the cause
   `source`, never a verdict.
4. **One scenario's ticks past the replay limits.** A scenario whose `N + 1` ticks pass the replay
   limits' `max_frames` is `too-many-frames` at bind, beside the total budget, since
   `ReplayError::TooManyFrames` has no cause in the table.
5. **`window-uncovered` is also a bind-and-run code**: the one word the two code lists share, for
   one refusal whichever stage finds it.
6. **`VectorInputs` has no `mutants` field until mutant mode.** Supplying a mutant without a
   listing is `mutant-unlisted`, a part-3 code, so part 3 adds the field in place.
7. **Warnings match one to one** *(ruled by the owner).* A diagnostic expectation takes one
   warning, and a warning is taken by at most one expectation. Every warning no expectation took is
   unmatched: it is listed, counts in `warnings_total`, and `--deny-warnings` counts it. The
   pairing order is fixed. Tick by tick, within a tick, first every expectation that names a
   `message`, in expectation order, then every block-only expectation, in expectation order. Each
   takes the earliest warning of its tick, in emission order, that it matches and that no
   expectation has taken. Pairing the message-bearing expectations first matches as many
   expectations as any pairing can. When fewer warnings can be matched than there are
   expectations, the later expectations of each group fail with `diagnostic-missing`. So one
   expectation at a tick where two instances of a class warn leaves the second warning unmatched,
   and two expectations take one each.
8. **`param-refused` is found at bind** *(ruled by the owner).* Binding tries every scenario's
   effective edits, in file order, on a freshly loaded engine by decision 14's passes, before any
   frame of any scenario runs. The run applies the same edits to the scenario's own fresh engine
   by the same passes. For part 3: a mutant's run applies the same edits; a mutant whose class
   rules refuse an edit the subject admits is for part 3 to rule on.
9. **Bind order.** Load, then the boundary ports (an Enum or String port, judged before any name
   because it is a property of the subject alone), then the set's `params`, then each scenario in
   file order (its inputs, the inputs it does not supply, its expectations, its `params`), then
   the parameter trial (reading 8), then the frame budget. The first refusal wins.
10. **API beyond the sketch.** `Receipt::computed_receipt_id`, the closed `ReceiptValue`
    (Real, Integer or Boolean) a receipt records, `ENGINE_REFUSAL_CLASSES` and
    `engine_refusal_class`, `MAX_LOAD_WARNINGS`, and `VerifyError`'s `path`, `candidates`, `cause`
    and `diagnostic_codes`. A `chunk_frames` of zero is read as one.

**The refusal's slot** *(ruled by the owner).* A `frame-refused` record is always kept. When a
frame was refused, the failure list keeps its last slot for that record, so earlier mismatches
never push it out; `failures_total` is unchanged. With a cap of zero no record is listed.

Two points the review raised are left as they are and tracked: the receipt does not record the
caps it was written under (#563), and nothing bounds the total work of a run, instants and subject
loads included (#561, which part 4 waits for).
