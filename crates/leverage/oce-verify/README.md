# oce-verify

Verification over vectors: a vector set is a strict-JSON file of scenarios (a schedule of inputs
over one clock, and expectations over windows of it) to check against one loaded control sequence.
Design note: [`docs/design/vectors.md`](../../../docs/design/vectors.md), which is the
specification of everything here.

This is the first of four parts (issue #519): **the format**. The crate reads a vector set, checks
every rule that needs no engine, writes its canonical form and names it with a `spec_id`. It loads
no engine, replays nothing and writes no receipt; binding a set to a sequence, the run and the
receipt, mutants and the `oce verify` verb build on it.

It is a leverage crate: among `oce-*` packages it may depend on `oce-api` and `oce-replay` only,
and today it uses neither. It opens no file, runs no clock and is not published.

```rust
use oce_verify::{VectorSet, VerifyLimits};

let set = VectorSet::from_json_bytes(&bytes, &VerifyLimits::default())?;
for scenario in set.scenarios() {
    println!("{}: {} expectations", scenario.name(), scenario.expectations().len());
}
println!("{} ticks, {}", set.clock().tick_count(), set.spec_id());
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
| `scenarios[]` | yes | `name` (not empty, unique), optional `description`, optional `params`, `inputs`, `expect` |
| `inputs` | yes | input name to a constant, or to a step list `[{ "t", "value" }]` from `t = 0` |
| `expect[]` | yes | a window (`output`, `from_s`, `to_s`, `equals`) or an instant (`diagnostic { block, message? }`, `at_s`) |

The format stays `v1` until 1.0.0 and changes in place. Keys it does not have yet (`within`, `of`,
`when`, `mutants`) are `unknown-key`: refused, never ignored, so no set is ever checked with part
of it skipped.

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

Each is inclusive: at the bound a set loads, one unit past it is refused with its own code, never
truncated. The three profile bounds are fixed. A `VerifyLimits` field can be lowered, to zero
included, and never raised: a value above its maximum is read as the maximum
(`VerifyLimits::effective`).

Memory and work are linear in the document: nothing is materialised per tick (a clock at the tick
bound costs what a clock of two ticks costs), a refusal stops where it is found, and
`tests/load_cost.rs` pins the factor with `allocation-counter`: at most 24 bytes held per document
byte for the densest shape the format has (a map of one-byte constants), 4 for step lists and
windows.

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

Every refusal is one `VectorErrorKind` with a stable kebab-case `code()`; `VECTOR_ERROR_CODES`
lists them, a build fails unless the list and the kinds agree, and `tests/error_codes.rs` pins the
list and produces each code from a document. A `VectorError` also says where: a line and column
for a refusal found in the text, a path such as `scenarios[2].inputs.clg_vlv_cmd[1].t` for a rule.
The code is the contract; the sentence is not.

| Code | Refuses |
| --- | --- |
| `not-utf8`, `byte-order-mark`, `json-syntax` | bytes that are not one strict JSON document |
| `input-too-large`, `nesting-too-deep`, `list-too-long`, `string-too-long` | a read bound |
| `duplicate-key`, `unknown-key`, `missing-key`, `wrong-type` | a closed object's shape |
| `non-finite-number`, `integer-out-of-range`, `control-character` | a lexical refusal; the second also an integer time beyond ±2^53 |
| `unknown-schema` | a `schema` other than `oce/vectors/v1` |
| `invalid-clock`, `horizon-off-cadence`, `too-many-ticks` | the clock |
| `too-many-scenarios`, `too-many-expectations`, `too-many-parameters` | a set bound |
| `duplicate-scenario`, `empty-name` | scenario names; an empty name in any name position |
| `step-list-empty`, `step-list-not-from-zero`, `step-times-not-increasing`, `step-after-horizon` | a step list |
| `invalid-window`, `window-outside-clock`, `window-uncovered` | a window |
| `ambiguous-expectation` | an expectation that is not exactly one of the two forms |
| `instant-off-cadence` | a diagnostic `at_s` that is not a tick |

The first fourteen are spelled as the authoring wire form's refusals are, so one word names one
refusal across the tools. The codes of the `mutants` key arrive with the key.

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
switch, and say in the pull request why it moved (a shape, or a doc comment in `src/dto.rs`).

## Fixtures and goldens

- The two in-repo library rules' `vectors.json` files are read in place from
  `crates/oce-api/tests/fixtures/library_rules/` (`tests/in_repo_rules.rs`).
- `tests/fixtures/canonical/*.json` are the canonical forms of those two files and of
  `tests/fixtures/sets/every_field.json`, and the `spec_id` literals in `tests/canonical_form.rs`
  are their FNV-1a-128 hashes. They were derived outside this crate and **are never blessed**:
  [`tests/fixtures/README.md`](tests/fixtures/README.md) says how to derive them again.
- `tests/fixtures/note/worked_example.json` is the JSON block of the design note's "The vector
  format" section. `tests/closed_objects.rs` reads the note and requires the two to be equal byte
  for byte, so the note is on the classifier's test-read list (`scripts/ci/classify-changes.sh`)
  and an edit to it runs this suite; when that block changes, copy it again.
