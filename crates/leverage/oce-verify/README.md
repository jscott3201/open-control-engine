# oce-verify

Verification over vectors: a vector set is a strict-JSON file of scenarios (a schedule of inputs
over one clock, and expectations over windows of it) to check against one loaded control sequence,
the **subject**. Design note: [`docs/design/vectors.md`](../../../docs/design/vectors.md), which is
the specification of everything here.

Three of four parts are in. **The format** (issue #519): the crate reads a vector set, checks
every rule that needs no engine, writes its canonical form and names it with a `spec_id`. **The
run** (issue #520): it binds a set to a subject, replays each scenario through `oce-replay`,
compares exactly and issues the receipt. **Mutant mode** (issue #521): it validates the
deliberately broken copies of the subject a set lists and scores which ones the vectors catch. The
`oce verify` verb builds on it.

It is a leverage crate: among `oce-*` packages it depends on `oce-api` and `oce-replay` only. It
opens no file, runs no clock, measures nothing and is not published.

```rust
use oce_verify::{VectorInputs, VectorSet, VerifyLimits, VerifyOptions, run_vectors};

let set = VectorSet::from_json_bytes(&vectors, &VerifyLimits::default())?;
let mutants = [("mutants/late_alarm.cxf.json", late_alarm.as_slice())];
let inputs = VectorInputs::new(&cxf).with_mutants(&mutants);
let run = run_vectors(&set, &inputs, &VerifyOptions::default())?;
println!("{}", run.receipt.to_canonical_json()); // `passed`, the body, `receipt_id`
```

## The format: `oce/vectors/v1`

```json
{
  "schema": "oce/vectors/v1",
  "clock": { "step_s": 60, "horizon_s": 1800 },
  "params": {},
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
        { "output": "yFault", "from_s": 360, "to_s": 1800, "equals": true },
        { "diagnostic": { "block": "CDL.Utilities.Assert", "message": "Both valves open." },
          "at_s": 360 }
      ]
    }
  ]
}
```

| Key | Required | Meaning |
| --- | --- | --- |
| `schema` | yes | exactly `oce/vectors/v1`; any other tag is `unknown-schema` |
| `clock.step_s`, `clock.horizon_s` | yes | ticks at `k * step_s` for `k = 0..=N`, where `N * step_s == horizon_s` exactly |
| `params` | no | parameter path to value, for every scenario; at most 256 entries |
| `mutants[]` | no | `path` (relative to this file: `/`-separated, no empty, `.` or `..` segment, no backslash, unique) and an optional `description`; at most 256 |
| `scenarios[]` | yes | `name` (not empty, unique), optional `description`, optional `params`, `inputs`, `expect` |
| `inputs` | yes | input name to a constant, or to a step list `[{ "t", "value" }]` from `t = 0` |
| `expect[]` | yes | a window (`output`, `from_s`, `to_s`, `equals`) or an instant (`diagnostic { block, message? }`, `at_s`) |

The format stays `v1` until 1.0.0 and changes in place. Keys it does not have yet (`within`, `of`,
`when`) are `unknown-key`: refused, never ignored, so no set is ever checked with part of it
skipped. `mutants` arrived in place with mutant mode; it belongs to the set and to no other
object.

**A value** is a number or a boolean, and keeps the kind it was written as: `1` is an integer
token, `1.0` and `1e0` are reals, `-0` is the integer zero and `-0.0` keeps its sign
([`Literal`](src/number.rs)). Nothing is typed here; which kind a port or parameter takes is
decided when the set is bound to a sequence. A string, `null`, a list or an object is never a
value, and an object is never an input (no series or file source in this format).

**Names** (inputs, outputs, parameter paths, a diagnostic's `block`) are kept as written and are
never empty; resolving them is the binder's work.

## What a load checks

`VectorSet::from_json_bytes` runs five stages and returns the first refusal:

1. **Bytes**: the length against `VerifyLimits::max_bytes` before a byte is examined, then UTF-8,
   then no byte-order mark.
2. **Strict JSON**, the whole text in one pass: syntax, the three profile bounds, repeated keys
   (compared after unescaping), control characters, non-finite numbers and integer tokens outside
   `i64`, at the first such place in document order.
3. **The tag**: a root `schema` string other than `oce/vectors/v1` is `unknown-schema` whatever
   else the document holds, so a foreign document is refused by name before its shape is judged.
4. **Shape**, in document order: every object is closed (`unknown-key`, `missing-key`), every
   value has the JSON type of its position (`wrong-type`; a list is never read as an object's
   members, and `null` is never a value), an expectation is exactly one of its two forms
   (`ambiguous-expectation`), the count bounds, and the integer range of a time.
5. **Rules**, in a fixed order (the clock, the set's `params`, then each scenario: its name, its
   `params`, its inputs in file order, its expectations in file order):
   - **the clock**: `step_s` positive, `horizon_s` not negative and **on the cadence**: some
     integer `k` gives `(k as f64) * step_s == horizon_s` under binary64 multiplication. `k` is
     found by rounding the quotient and testing `k - 1`, `k` and `k + 1`; a quotient is never
     floored (`floor(4.3 / 0.1)` is 42, while `43 * 0.1 == 4.3`). A quotient past the tick bound is
     `too-many-ticks` before any candidate is tested;
   - **step lists**: at least one step, the first at exactly `t = 0`, strictly increasing, none
     after the horizon. A step time need not be a tick;
   - **windows**: `0 <= from_s <= to_s <= horizon_s`, both ends included, holding at least one
     tick. Tick `k` is in the window when `from_s <= (k as f64) * step_s <= to_s`, decided from the
     tick's value: on a 0.1 s clock tick 3 is `0.30000000000000004`, so a window ending at `0.3`
     does not reach it. A loaded `Window` carries its first and last tick;
   - **instants**: `at_s` is a tick, by the cadence rule.

A time (`step_s`, `horizon_s`, `t`, `from_s`, `to_s`, `at_s`) is a binary64. An integer token
there must be within ±2^53 (`MAX_TIME_INTEGER`), the range in which every integer is a binary64,
so an integer token never names a time other than the one it spells; past it the refusal is
`integer-out-of-range`, and a larger time is written as a real. Negative zero reads as zero.

## Bounds

The values are the design note's ("Bounds", decision 35); the last column says what each is
measured against.

| Bound | Value | Refusal | Why this value |
| --- | --- | --- | --- |
| `MAX_DEPTH` | 8 levels | `nesting-too-deep` | the strict profile's: the format nests six levels, and two more let a misplaced value be reported as a wrong type, not as depth |
| `MAX_LIST_LEN` | 65,536 entries | `list-too-long` | the strict profile's; it is the bound on one input's steps (the library's longest list has 241) |
| `MAX_STRING_BYTES` | 4,096 bytes as written | `string-too-long` | the strict profile's |
| `VerifyLimits::max_bytes` | 4 MiB | `input-too-large` | the authoring forms' document bound; the library's largest vector file is 184 KB |
| `VerifyLimits::max_ticks` | 16,777,216 | `too-many-ticks` | `oce-replay`'s frame bound (`ReplayLimits::MAX.max_frames`), since one scenario is one replay; it is 2^24, far inside the range where a tick index is an exact binary64 |
| `VerifyLimits::max_scenarios` | 4,096 | `too-many-scenarios` | the bound on the receipt's scenario list; the library's largest set has 28 |
| `VerifyLimits::max_expectations` | 4,096 per scenario | `too-many-expectations` | the library's largest scenario has 12 |
| `VerifyLimits::max_parameters` | 256 per map | `too-many-parameters` | it bounds the passes that order parameter edits (note decision 14) |
| `VerifyLimits::max_mutants` | 256 | `too-many-mutants` | the bound on the receipt's mutant entries |
| `VerifyLimits::max_mutant_bytes` | 64 MiB, all mutants together | `too-many-mutant-bytes`, at bind, before any mutant loads | 256 mutants are never 2 GiB in memory; each one is also held to the engine's own `MAX_CXF_BYTES` |
| `VerifyLimits::max_total_frames` | 2^24 = 16,777,216 | `too-many-frames` | `(N + 1) * scenarios * (1 + mutants)` for the whole run, at bind's budget step; the library's largest set runs 23,337 |
| `VerifyLimits::max_total_instants` | 2^28 = 268,435,456 | `too-many-instants` | `(1 + mutants)` times every window's ticks and every diagnostic, at the budget step; the library's largest set compares 41,488 (the work-budget note) |
| `VerifyLimits::max_total_load_bytes` | 2^30 = 1,073,741,824 | `too-many-load-bytes` | the subject's and the supplied mutants' bytes times `1 + e + s` (`e` distinct edit lists, `s` scenarios), at the budget step; the library's largest is 847,757 |
| `VerifyLimits::max_total_frame_bytes` | 2^40 = 1,099,511,627,776 | `too-many-frame-bytes` | `(N + 1) * s` times the same bytes, a frame weighted by the document that runs it, at the budget step; the library's largest is 426,133,620 |
| `VerifyLimits::replay` | `ReplayLimits::MAX` | `too-many-frames` (its `max_frames` against one scenario's `N + 1`, at bind), else `replay-failed` | the limits each scenario's replay runs under |
| `VerifyLimits::max_recorded_failures` | 16 per scenario | none: the list stops, `failures_total` keeps the count | the note's failure-record cap |
| `VerifyLimits::max_recorded_warnings` | 16 per scenario | none: the list stops, `warnings_total` keeps the count | the note's warning-row cap |
| `MAX_LOAD_WARNINGS` | 64 | none: the list stops, `load_warnings_total` keeps the count | the note's load-warning cap |

Each is inclusive: at the bound a set loads or runs, one unit past it is refused with its own
code, never truncated; the three record caps stop a list and keep its total. The three profile
bounds are fixed. A `VerifyLimits` field can be lowered, to zero
included, and never raised: a value above its maximum is read as the maximum
(`VerifyLimits::effective`). The load bounds apply when a set is read; `run_vectors` applies the
run's, from `VerifyOptions::limits`.

**Loading** a set costs memory and work linear in the document: nothing is materialised per
tick (a clock at the tick bound costs what a clock of two ticks costs), a refusal stops where it
is found, and `tests/load_cost.rs` pins the factor with `allocation-counter`: at most 24 bytes
held per document byte for the densest shape the format has (a map of one-byte constants), 4 for
step lists and windows.

**Running** a set is not linear in the document. Its work grows with the frames it executes,
weighted by the bytes of the document that runs them, with the instants it compares and with the
loads it makes: one at bind, one per distinct edit list to try it, one per scenario to run it, on
the subject and on each mutant. The four budgets bound each of these as an upper bound, checked in
one step at bind before any load but the subject's first (the
[work-budget note](../../../docs/design/vector-work-budget.md)). At the defaults the worst a set
can ask for is about a minute in `release` on an Apple M5, against hours before #561; the
library's sets sit 700x to 6,500x under every default. Its memory is bounded: a run holds one
scenario's trace at a time (unless `keep_traces` asks for all of them), and each expectation keeps
at most the cap of its own earliest failure records, so the receipt's size is bounded by the caps,
not by the number of ticks checked.

## The canonical form and `spec_id`

`to_canonical_json` writes one spelling per set: keys sorted by their UTF-8 bytes at every depth,
two-space indentation, one entry per line, `{}` and `[]` for empty ones, line feeds, one final
line feed, ASCII only (`\"`, `\\`, `\n`, and `\u` with four lowercase hex digits for anything
else, an astral character as its surrogate pair). Lists keep file order. An absent optional key
stays absent, and an empty `params` stays `{}`.

Numbers:

- an integer token is written as a decimal integer; `-0` is the integer zero and is written `0`;
- a real is written with the shortest digits that read back to the same binary64: in plain
  decimal with at least one fractional digit when it is zero or `1e-4 <= |x| < 1e16`, otherwise as
  `d[.ddd]e[-]N` with a lowercase `e`, no `+` and no leading zero in the exponent. So `0.0001`,
  `9.999999999999999e-5`, `9999999999999998.0`, `1e16`, `-0.0`, `5e-324`,
  `1.7976931348623157e308`. "Shortest" alone does not pick one string (`3e-324` through `7e-324`
  all read back to the smallest subnormal): of the shortest strings that read back, the one
  written is the closest to the exact binary value, `5e-324`. The layout is decided in this crate
  from the decimal exponent of those digits; goldens pin the bytes at both thresholds and at the
  subnormals, a test compares the result with Rust's own `{:?}` spelling, which follows the same
  layout rule, and another checks with exact integers that the digits are the shortest that read
  back and the closest of those.

`spec_id` is `spec:fnv1a128:` and the 32 lowercase hex digits of FNV-1a-128 over those bytes. Two
documents that differ only in spelling have one id; two that differ in the kind of a number (`60`
and `60.0`) do not. It is an identity, not a cryptographic digest.

A document need not be canonical to load. Loading the canonical form and writing it again gives
the same bytes. The canonical form itself loads again provided it is within the bounds: a
character outside ASCII is six bytes per UTF-16 unit when written, so a string near 4,096 bytes of
such characters canonicalises past the string bound, and a compact document near 4 MiB gains its
indentation.

## Error codes

Every refusal of a load is one `VectorErrorKind` with a stable kebab-case `code()`;
`VECTOR_ERROR_CODES` lists them, a build fails unless the list and the kinds agree, and
`tests/error_codes.rs` pins the list and produces each code from a document. A `VectorError` also
says where: a line and column for a refusal found in the text, a path such as
`scenarios[2].inputs.clg_vlv_cmd[1].t` for a rule. The code is the contract; the sentence is not.

| Code | Refuses |
| --- | --- |
| `not-utf8`, `byte-order-mark`, `json-syntax` | bytes that are not one strict JSON document |
| `input-too-large`, `nesting-too-deep`, `list-too-long`, `string-too-long` | a read bound |
| `duplicate-key`, `unknown-key`, `missing-key`, `wrong-type` | a closed object's shape |
| `non-finite-number`, `integer-out-of-range`, `control-character` | a lexical refusal; the second also an integer time beyond ±2^53 |
| `unknown-schema` | a `schema` other than `oce/vectors/v1` |
| `invalid-clock`, `horizon-off-cadence`, `too-many-ticks` | the clock |
| `too-many-scenarios`, `too-many-expectations`, `too-many-mutants`, `too-many-parameters` | a set bound |
| `duplicate-scenario`, `empty-name` | scenario names; an empty name in any name position |
| `step-list-empty`, `step-list-not-from-zero`, `step-times-not-increasing`, `step-after-horizon` | a step list |
| `invalid-window`, `window-outside-clock`, `window-uncovered` | a window |
| `ambiguous-expectation` | an expectation that is not exactly one of the two forms |
| `instant-off-cadence` | a diagnostic `at_s` that is not a tick |
| `mutant-path-invalid`, `duplicate-mutant` | a mutant path that is absolute, has an empty, `.` or `..` segment or a backslash; a path listed twice |

The first fourteen are spelled as the authoring wire form's refusals are, so one word names one
refusal across the tools.

## Binding a set to a subject

`run_vectors(&set, &VectorInputs::new(&cxf), &options)` binds once, against a throwaway engine,
before any scenario runs, and every refusal is an `Err` with no receipt, never a failed scenario.
In order, the first refusal winning:

1. **Load** the subject (`Engine::load_cxf`). A refusal is `subject-load-failed`, carrying the
   engine's `DiagCode` spellings (`VerifyError::diagnostic_codes`). Load warnings never refuse;
   their codes go to the receipt.
2. **The boundary**: an Enum input or output is `enum-port-unsupported` (until #465), a String one
   `unsupported-port-type`. No CXF document the engine loads has either today; unit tests pin both.
3. **Names and values**: the set's `params`, then each scenario in file order (its inputs, the
   inputs it does not supply, its expectations, its `params`). A name resolves by replay's one
   rule, `oce_replay::resolve_path`: the exact canonical path, or the one path that ends with the
   name at a `.` boundary (`#` is not a boundary). Inputs resolve against
   `Engine::input_definitions()`, outputs against `FrameSlots::outputs()`, parameter keys against
   `Engine::params()` sorted by path, each list indexed once (`oce_replay::PathIndex`), so a name
   costs a binary search rather than a scan of the list (issue #571). Every value is typed by what
   it lands on, never by its token: a Real takes any number (an integer token when a binary64
   holds it exactly, `-0` as `+0.0`, `-0.0` with its sign), an Integer an integer token only
   (`1.0` is refused), a Boolean `true` or `false`, a String or Enum parameter nothing. A
   scenario's `params` override the set's by resolved path, whatever either spelling was. The
   set's `params` are held once; each scenario keeps only its own map.
4. **The work budgets** (the [work-budget note](../../../docs/design/vector-work-budget.md)), in
   this order, the first exceeded winning; each counts every mutant as if it ran every scenario:
   - frames, `(N + 1) * scenarios * (1 + mutants)`, against `max_total_frames`, after one
     scenario's `N + 1` against the replay limits' `max_frames`;
   - compared instants, `(1 + mutants)` times every window's ticks and every diagnostic, against
     `max_total_instants`;
   - bytes loaded, `D * (1 + e + s)`, against `max_total_load_bytes`, with `D` the subject's and
     every supplied mutant's bytes, `e` the distinct non-empty edit lists and `s` the scenarios;
   - byte-frames, `(N + 1) * s * D`, against `max_total_frame_bytes`.

   Each product is taken in `u128` with checked multiplication, so a hostile count refuses and
   never wraps. The step is arithmetic on steps 1 to 3, so it runs before every other load: an
   over-budget set is refused even when the subject would refuse its edits, and it loads no
   mutant.
5. **The parameter edits**: each distinct effective edit list is tried once, in the order of the
   first scenario that has it, on a freshly loaded engine by the passes the run uses (below), so
   an edit no pass admits is `param-refused` here, at the first scenario holding it, before any
   frame of any scenario runs. No class chains more than two parameters through its rules, so a
   list takes at most two passes (`tests/parameter_trials.rs` pins it against the catalog).
6. **The mutants** ([below](#mutant-mode)): each one supplied, within the byte bound, loaded,
   held to the subject's boundary and content id, and given every edit list.

## The run

Each scenario runs on a freshly loaded engine. Its parameter edits are applied inside one halt in
passes: each pass tries the edits still waiting in ascending path order, a refused edit waits for
the next pass, and a pass that applies nothing is `param-refused` with the facade's class
(`param-range`, `param-type`, `param-structural`). Binding has already tried the same edits by the
same passes, so a run never meets that refusal. So lowering both `Reals.Hysteresis` thresholds
takes two passes and raising them one. A table that only another order of edits reaches is refused
too: that limitation is the note's, kept on purpose.

Then one replay: a `StepSource` with the horizon as an instant, on `TimeBase::Grid { t0: 0, step:
step_s }`, with replay's defaults (refuse a missing sample, stop at a refused frame, refuse a
repeated time, refuse a non-finite input), in chunks of `VerifyOptions::chunk_frames` frames.
Every scenario runs exactly the clock's `N + 1` ticks unless a frame is refused, and chunk sizes
never change a receipt byte. After the run, in order:

1. a refusal about the run rather than the frame (`EngineRefusal::Run`) is the error
   `run-refused`;
2. any other refused frame fails the scenario with one `frame-refused` record at its tick, with
   the refusal's kebab-case class (`input-domain` and the eight others, `ENGINE_REFUSAL_CLASSES`).
   Instants before it are compared and counted; instants at and after it are not, and coverage is
   not checked;
3. otherwise every window must hold an executed tick (`window-uncovered` if one does not, which
   load already rules out);
4. every tick of every window and every diagnostic instant is compared. `equals` is exact for every
   type, a Real by its bits (`0` on a Real output is `+0.0` and does not equal `-0.0`; nothing
   equals a NaN). A diagnostic expectation matches the class path in `block` and, when given, the
   `message` byte for byte, at its tick, and is one instant. Matching is one to one: each
   expectation takes one warning, so where two instances of a class warn at one tick, one
   expectation leaves the second warning unmatched and two expectations take one each. Within a
   tick the expectations that name a message pair first, then the block-only ones, each in
   expectation order taking the earliest free warning it matches. Warnings no expectation took are
   listed (at most 16, with `warnings_total`) and never change a verdict.

A replay that ends in an error is `replay-failed` with a cause from a closed table
(`REPLAY_FAILURE_CAUSES`: `trace-too-large`, `output-set-changed`, `grid-not-increasing`,
`engine-setup`, `source`), never a Rust variant name.

| Code | Refuses |
| --- | --- |
| `subject-load-failed` | a subject the engine refuses to load |
| `unknown-input`, `ambiguous-input`, `duplicate-input`, `input-not-supplied` | input names (`VerifyError::candidates` lists an ambiguous name's matches) |
| `unknown-output`, `ambiguous-output` | output names |
| `param-path-unknown`, `param-path-ambiguous`, `param-path-duplicate` | parameter keys |
| `param-refused` | an edit no pass admits, with its class in `VerifyError::cause` |
| `value-type-mismatch`, `value-not-representable` | a value its port or parameter type cannot take |
| `enum-port-unsupported`, `unsupported-port-type` | an Enum or a String boundary port |
| `too-many-frames` | the frame budget, or one scenario's `N + 1` past the replay limits |
| `too-many-instants`, `too-many-load-bytes`, `too-many-frame-bytes` | the compared-instant, load-byte and byte-frame budgets |
| `mutant-missing`, `mutant-unlisted`, `duplicate-mutant` | mutant bytes not given, given for a path the set does not list, or given twice |
| `too-many-mutant-bytes` | the mutants' bytes together |
| `mutant-load-failed`, `mutant-boundary-differs`, `mutant-identical` | an invalid mutant: refused by the engine, another boundary, or the subject's content id |
| `mutant-param-refused` | a mutant that refuses an edit the subject admits, with its cause (`MUTANT_PARAM_REFUSAL_CAUSES`) |
| `run-refused` | a run-class engine refusal |
| `replay-failed` | any other replay error, with its cause |
| `window-uncovered` | a window with no executed tick after a run that refused no frame |

`VERIFY_ERROR_CODES` lists them and a build fails unless the list and the kinds agree;
`tests/run_error_codes.rs` pins the list and produces each code from a run, except the four no
run reaches today (the two port codes, `run-refused` and `window-uncovered`), which unit tests in
`src/bind_tests.rs` and `src/run_tests.rs` produce from the cause replay would hand back.

## Mutant mode

A **mutant** is a CXF document that should fail the vectors: a deliberate defect in the subject.
The set lists each by a path relative to the vector file; the caller resolves the paths and hands
the bytes in (`VectorInputs::with_mutants`), so `oce-verify` still opens no file. A caller that
reads files resolves each path, keeps it inside the vector file's directory and refuses two
listings that resolve to one file (owner ruling of 2026-11-11, for part 4's CLI): the format's
lexical path rule cannot see every alias.

**Validation**, at bind, after the work budgets and the subject's parameter trials and before any
frame runs. An invalid mutant is an error with no receipt; there is no `invalid` status. In this
order:

1. every path given is listed, and given once (`mutant-unlisted`, `duplicate-mutant`);
2. every listed mutant is given (`mutant-missing`);
3. the bytes together fit `max_mutant_bytes` (`too-many-mutant-bytes`);
4. each listed mutant in file order:
   - it loads (`mutant-load-failed`, with the engine's codes);
   - its input paths and types and its output paths and types are the subject's
     (`mutant-boundary-differs`, naming the first port that differs; input bounds are not
     compared);
   - its content id is not the subject's (`mutant-identical`);
   - it admits every distinct edit list of the scenarios, each tried once on a fresh engine of it,
     in the order of the first scenario that has it (`mutant-param-refused`, owner ruling of
     2026-11-11).

**Scoring**, after the subject's scenarios, and only when the subject passed:
- each mutant runs the scenarios in file order, on fresh engines with the scenarios' edits, as the
  subject did, and stops at the first that fails on it;
- that mutant is `caught`, and the receipt names the scenario and its first failure;
- a frame refused on a mutant fails the scenario as it would for the subject, so a mutant that
  narrows an input bound is caught, not an error;
- a mutant no scenario fails on `survived`;
- against a subject that failed or checked nothing, the section is `not_run`.

The score is `caught` and `total`, with `survived`, never a ratio, and it never changes `passed`.

**Cost.** Mutant mode multiplies a run's work, and the four budgets count it all:
- validation loads each mutant once, plus once per distinct edit list;
- scoring runs up to every scenario on every mutant: `(N + 1) * scenarios * mutants` more frames,
  and as many fresh loads, each weighted by the mutant's bytes in the load-byte and byte-frame
  budgets.

## The receipt

`Receipt { header, body: ReceiptBody::Vectors(VectorBody) }`, written by `to_canonical_json` under
the vector file's canonical rules. The header is the one the commissioning verifier shares:
`schema` (`oce/receipt/v1`), `kind` (`vectors`), the subject's `content_id` as loaded, the
`engine_version` (`oce_api::ENGINE_VERSION` as `major.minor.patch`, the version every blob and
state snapshot of the same build records, without the Cargo version's pre-release suffix; issue
#593), the set's `spec_id`, `comparison` (`{ "mode": "exact" }`), `passed`, `receipt_id` and
`timings`. The body holds the set's `status` (`pass`, `fail` or `no_data`), its
`clock` as written, the load-warning codes (at most 64, `load_warnings_total` beside them), the
totals, and one object per scenario: its verdict, its effective parameter edits by canonical path
with the `content_id` of what ran when it has any, frame and instant counts, failure records
(`value-mismatch`, `diagnostic-missing`, `frame-refused`, each shaped by its code, by tick then by
expectation, at most 16 with `failures_total`; when a frame was refused the last slot is kept for
its `frame-refused` record, so earlier mismatches never push it out) and unmatched warnings (at
most 16 with `warnings_total`). When the set lists a mutant, the body has a `mutants` section:
- its `status` (`scored`, or `not_run` when the subject did not pass);
- `total`, `caught` and `survived`;
- one entry per listed mutant in file order, each with its `path`, its `digest`
  (`fnv1a128:<32 hex>` of the bytes given, since `spec_id` covers only the path), its
  `content_id` and its `status`, plus, when caught, the `scenario` and its first `failure`.

Values are exact: a Boolean is `true` or `false`, an Integer a JSON integer, a finite Real other
than negative zero a number in the canonical real spelling, and negative zero, a NaN or an
infinity the string `"0x"` and the 16 lowercase hex digits of its bits (`"0x8000000000000000"`,
`"0x7ff8000000000000"`).

`receipt_id` is `receipt:fnv1a128:<32 hex>` over the canonical receipt with `timings` and
`receipt_id` removed, so two runs give one id whatever they measured; `engine_version` is inside
the hashed part, so a workspace version bump moves every receipt id. `oce-verify` measures nothing:
`timings` are zeros unless the caller passes `VerifyOptions::timings`.

## The schema

[`schema/vectors.schema.json`](schema/vectors.schema.json) (`VECTORS_JSON_SCHEMA`) is derived with
`schemars` from the reader's own shapes (`src/dto.rs`) and written canonically.
`tests/schema_pinned.rs` regenerates it and compares byte for byte, and walks it for keywords
outside the strict profile (`SCHEMA_PROFILE_KEYWORDS`): closed objects, two map shapes (`inputs`
and `params`), `anyOf` only over a value's shapes, an input's and an expectation's two forms, and
no `oneOf`, `const`, `format`, `default` or `null`. `tests/schema_conformance.rs` holds the schema
and the reader together: every document the reader loads conforms, and for every closed object the
schema's properties and required keys are exactly the keys the reader knows and requires.

A JSON Schema cannot say the cadence, step order, a window's ticks, the bounds or repeated keys,
so a document that satisfies the schema can still be refused. A document the reader loads always
satisfies the schema.

Regenerate with `OCE_BLESS_VERIFY=1 cargo nextest run -p oce-verify`, run it again without the
switch, and say in the pull request why it moved (a shape, or a doc comment in `src/dto.rs`). The
same switch rewrites the golden receipts.

## Fixtures and goldens

- The two in-repo library rules are read in place from
  `crates/oce-api/tests/fixtures/library_rules/`: their `vectors.json` files
  (`tests/in_repo_rules.rs`) and, as subjects, their `rule.cxf.jsonld` files.
- `tests/fixtures/receipts/*.receipt.json` are the golden receipts: the pass receipts of both
  rules, a fail receipt (`both_open_900.vectors.json`, AHU-0016's set with `both_open`'s first
  window ending at 900 s, where the delay fires) and a `no_data` receipt
  (`empty_expect.vectors.json`, one scenario expecting nothing). `tests/receipt_goldens.rs`
  compares them byte for byte and checks each `receipt_id` against a derivation of its own. They
  embed the engine version and the two rules' content ids, so a workspace version bump re-blesses
  them, and so does #517 when it re-vendors the rules. Regenerate with
  `OCE_BLESS_VERIFY=1 cargo nextest run -p oce-verify`, run it again without the switch, and say in
  the pull request which receipts moved and why.
- `tests/fixtures/receipts/AHU-0016.mutants.receipt.json` is the golden receipt of AHU-0016 with
  its three mutants.
- `tests/fixtures/AHU-0016/` holds the mutants:
  - `vectors.json` is AHU-0016's set plus the `mutants` list; `tests/mutant_mode.rs` holds its
    clock and scenarios equal to the facade's copy, which lists no mutant;
  - `mutants/` holds three hand edits of the rule's CXF, each with a provenance line in its
    listing's `description`. `threshold_inclusive` (both thresholds read `u >= 5`, a `Not` over
    `Reals.LessThreshold`) is caught by `threshold_edge`. `no_persistence` (`persist.delayTime`
    900 to 0) is caught by `both_open`. `late_alarm` (900 to 930) survives by design: no window
    asserts the tick at 900 s;
  - the directory holds its own `vectors.json` because a mutant path cannot leave the vector
    file's directory; the subject is read in place.
- `tests/fixtures/subjects/*.cxf.json` are small subjects written for the run's tests:
  - `signals`: inputs of every type, two pairs of names that share a last segment, a bounded
    input, an assertion, a divider and pass-through outputs;
  - `hysteresis`: one `Reals.Hysteresis`;
  - `twin_asserts`: two assertions on one input;
  - `threshold` and `threshold_integer`: a Real threshold, and a mutant of it through an Integer
    threshold.
- Every subject and mutant is named `.cxf.json`, not `.jsonld`, because `oce-cxf`'s
  `parameter_grounding_corpus` test pins the count of every tracked `.jsonld` file.
- `tests/fixtures/canonical/*.json` are the canonical forms of the two rules' vector files and of
  `tests/fixtures/sets/every_field.json`, and the `spec_id` literals in `tests/canonical_form.rs`
  are their FNV-1a-128 hashes. They were derived outside this crate and **are never blessed**:
  [`tests/fixtures/README.md`](tests/fixtures/README.md) says how to derive them again.
- `tests/fixtures/note/worked_example.json` is the JSON block of the design note's "The vector
  format" section. `tests/closed_objects.rs` reads the note and requires the two to be equal byte
  for byte, so the note is on the classifier's test-read list (`scripts/ci/classify-changes.sh`)
  and an edit to it runs this suite; when that block changes, copy it again.
