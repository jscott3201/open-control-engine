# Commissioning verifier (OBC §12)

**Design note, written before any verifier code.** It fixes what the commissioning verifier is, its
test-spec and point-mapping files, unit conversion, the envelope, indicators, time handling, the
report and the receipt. Decisions 1 to 4 were settled by the owner and decisions 5 to 10 are
technical recommendations the orchestrator accepted; both are stated here, not reopened. Decisions
11 to 26 were made while drafting and ruled on by the architect pass, which accepted fourteen and
replaced 17 and 21 (see [Review rulings](#review-rulings)). It answers GitHub #436 and drives
[the replay API](replay.md) (GitHub #435, merged as GitHub #443; the crate is #445).

The OpenBuildingControl (OBC) specification's §12.7.1 states the product: "a report that describes
whether the real implementation conforms to the CDL implementation within a user-specified error
tolerance." Every §12 citation in this note is a section of that specification's
[Verification chapter](https://obc.lbl.gov/specification/verification.html), as numbered on
2026-11-09. The table of contents of ANSI/ASHRAE 231-2026 lists no verification or conformance
clause.

## What the verifier is, and is not

It is the **tolerance mode of `oce-verify`** (M3), a leverage crate above the facade that depends on
`oce-api` and `oce-replay` only. Given a test spec, it loads the CDL sequence's CXF into an
`Engine`, applies the spec's parameters, feeds the trended inputs of a real controller (or a
simulation's inputs) through the replay API, and compares the engine's outputs with the trended
outputs, per OBC §12.4: "The commissioning agent then executes the CDL specification for the
trended input time series, and compares" whether the trended outputs and the computed ones "are
close to each other."

- It is **never the default comparison**. `oce-verify`'s default is exact; tolerance mode is chosen
  by the spec, and every receipt names the comparison mode and the envelope.
- It **never changes the engine**. It adds nothing to `oce-api`, sets parameters only through the
  facade's halt / `set_param` / resume path, and the engine keeps its
  [metadata seam](../architecture.md#the-organising-idea-the-non-computational-metadata-seam)
  (OBC §7.17; ANSI/ASHRAE 231-2026 §5.13).
- **Unit conversion and point mapping happen outside the engine**, in the verifier's source adapter,
  as §12.7 requires: conversion "will need to be done by the tool that reads the CSV files", because
  "CDL provides all required unit information, but this is not the case in general for a building
  automation system."
- It does **not** talk to a controller. Scenario 1's live run (`runController`, the `controller`
  BACnet block), modelica-json and OpenModelica invocation, plots and sequence diagrams (§12.5.4)
  are out of scope. Both §12.7 scenarios reduce here to Scenario 2: archived trends in, a verdict
  out.
- A commissioning receipt is evidence about a site, not a gate: it never decides whether a sequence
  may load or run.

## Test-spec file

A versioned **strict-JSON superset of §12.7 Listing 12.1** (decision 3). These LBL fields load
unchanged:

| Field | Type | Meaning here |
| --- | --- | --- |
| `references[]` | array | one test per entry |
| `.model` | string | the Modelica class, a label only |
| `.sequence` | string | the instance name; output and indicator keys are `<sequence>.<path>` |
| `.pointNameMapping` | string or array | a mapping file path, or the mapping inline |
| `.outputs` | object | key an output name or a glob, value partial tolerances |
| `.indicators` | object | key an output name or a glob, value indicator names (ANDed) |
| `.sampling` | number (s) | per-test comparison sampling |
| `tolerances` | object | `rtolx`, `rtoly`, `atolx`, `atoly` (Listing 12.1: `0.002, 0.002, 10, 0`) |
| `sampling` | number (s) | default sampling (Listing 12.1: 120) |

`generateJson`, `modelJsonDirectory`, `runController` and `controller` parse and are ignored with a
named `lbl-field-ignored` report finding; `controllerOutput` is honoured as the expected-output
source when no `expected` block is given (decision 18). An LBL file is not strict JSON (its examples
carry comments), so the loader strips `//` and `/* */` comments outside strings, then parses
strictly: duplicate keys, trailing commas, `NaN` and unknown fields are typed errors. The verifier
always writes strict JSON with a `schema` tag, so a stored spec is canonical.

**Added fields.** `schema` (`oce/commissioning-spec/v1`; absent means "LBL, read as v1"); per
reference: `cxf` (the sequence's CXF path; an LBL spec without it parses but refuses to run),
`parameters` (instance path → value in CDL units, replacing `reference_parameters.txt`), `inputs`
and `expected` (trend sources), `indicatorSources` (indicator columns from a file, §12.7.3's
`indicator.csv`), `window` (`start`, `stop` in model seconds), `envelope` (`funnel` or
`aligned-grid`), `funnelReference` (`engine` or `trend`); in any tolerance object `ltolx` and
`ltoly`; at top level `time.origin` (an ISO-8601 instant with an explicit offset).

**A trend source** names a file, its format and a column map from the file to device point names;
the point mapping then takes device names to CDL names and units. Formats are `csv` (a header row,
read by replay's `CsvSource`) and `combiTimeTable` (§12.5.1's `#1` text, the Buildings library's
trend format; `table` names the matrix when a file holds more than one). Time is a column of seconds
since `time.origin` or an ISO-8601 column (replay decision 17). `hold.maxAge` per source is the
replay `HoldLast` policy for that source's inputs; absent, a missing sample refuses the frame.

A key that contains `*` or `?` is a glob matched against the whole `<sequence>.<path>` name (`*`
any run of characters, `?` one character, nothing else special); any other key is an exact name.
Overrides apply in declaration order, later fields winning. A key that matches no output is an
error, `SpecError::UnmatchedKey`, whose message names the key, whether it was read as an exact name
or a glob, and the sequence's output names, so a typo cannot pass silently. There is no substring
or regular-expression matching: `u` selects only an output named `u`. Listing 12.1's
`"setPoiVAV1.TSup*"` is a glob and loads unchanged (decision 17).

### Worked spec: Building 33 cooling-coil valve (§12.6)

```json
{
  "schema": "oce/commissioning-spec/v1",
  "time": { "origin": "2018-06-07T00:00:00-07:00" },
  "tolerances": { "atolx": 1, "atoly": 0, "rtolx": 0, "rtoly": 0.01 },
  "sampling": 5,
  "references": [
    {
      "model": "Buildings.Utilities.Plotters.Examples.ControlsVerification_CoolingCoilValve",
      "sequence": "cooValSta",
      "cxf": "cxf/Buildings/Utilities/Plotters/Examples/BaseClasses/CoolingCoilValve.json",
      "pointNameMapping": "b33-points.json",
      "parameters": {},
      "envelope": "funnel",
      "funnelReference": "engine",
      "window": { "start": 0, "stop": 86400 },
      "inputs": [
        { "path": "data/Supply_Air_Temp.mos", "format": "combiTimeTable", "timeColumn": 1,
          "columns": { "3": "Supply Air Temp" }, "hold": { "maxAge": 10 } },
        { "path": "data/SA_Clg_Stpt.mos", "format": "combiTimeTable", "timeColumn": 1,
          "columns": { "3": "SA Clg Stpt" }, "hold": { "maxAge": 10 } },
        { "path": "data/OA_Temp.mos", "format": "combiTimeTable", "timeColumn": 1,
          "columns": { "3": "OA Temp" }, "hold": { "maxAge": 10 } },
        { "path": "data/VFD_Fan_Feedback.mos", "format": "combiTimeTable", "timeColumn": 1,
          "columns": { "3": "VFD Fan Feedback" }, "hold": { "maxAge": 10 } },
        { "path": "data/VFD_Fan_Enable.mos", "format": "combiTimeTable", "timeColumn": 1,
          "columns": { "3": "VFD Fan Enable" }, "hold": { "maxAge": 90000 } }
      ],
      "expected": [
        { "path": "data/Clg_Coil_Valve.mos", "format": "combiTimeTable", "timeColumn": 1,
          "columns": { "3": "Clg Coil Valve" } }
      ],
      "outputs": { "cooValSta.yCooVal": { "atolx": 1, "rtoly": 0.01 } },
      "indicators": { "cooValSta.yCoo*": [ "cooValSta.uFanSta" ] }
    }
  ]
}
```

The tolerances are §12.6's: "an absolute tolerance in time of 1 s and a relative tolerance in y of
1%", read as funnel's range-relative `rtoly` (1 % of the engine series' range, about 0.01 for a 0–1
valve signal), not `ltoly`, which collapses to the 1e-10 floor wherever the valve is shut. Listing
12.1's global `rtolx = 0.002` would be 173 s over a one-day window; the receipt carries the absolute
half-widths applied, so a reader sees that. The fan-enable trend logs one sample a day, hence its
25-hour hold. The indicator is a no-op on this data (the fan is enabled throughout); it is in the
example to show the shape, keyed by a glob where `outputs` uses the exact name. The sequence's
instance modifiers in the upstream model equal the class defaults, so `parameters` is empty: `k =
0.018` 1/K and `Ti = 30` s are derived inside the CXF from the recorded ALC gains (`k = alc_prop_k ·
9/5`, `Ti = k · interval / (alc_int_k · 9/5)`), as §12.6 prescribes. The upstream instance sets
`reverseActing = false`; §12.6's prose says `reverseAction = true`, written before CDL renamed and
inverted that parameter. The worked example follows the model's `reverseActing = false`, not the
prose.

## Point mapping

Listing 12.2's shape, unchanged: a list of `{ "cdl": {name, unit?, type}, "device": {name, unit?,
type} }`, by path or inline (decision 5). Types are `float`, `int` and `bool`; "some points may not
have a unit, but only have a type." Every CDL input of the sequence must be mapped, and every
expected output must map to a CDL output; an unmapped input, a duplicate name on either side or a
mapped name the engine does not have is a typed error before any frame. Names resolve to slots once
(replay's `InputMap`).

Conversion runs in the source adapter, device → CDL, on inputs, expected outputs and indicator
columns alike, so every comparison happens in CDL units and tolerances are CDL-unit quantities.
Parameters are never converted: §12.6's gain conversion is controller-specific engineering, so the
spec gives parameters in CDL units. The table is **closed**: an unlisted pair is
`UnitError::UnknownPair`, never a pass-through; a unit on one side only is `UnitError::OneSided`;
no unit on both sides means no conversion. Its first content, version `units/v1`:

| Device → CDL | Conversion (f64, fixed operation order) |
| --- | --- |
| `degF` → `K` | `(x - 32.0) * (5.0 / 9.0) + 273.15` |
| `degC` → `K` | `x + 273.15` |
| `degF` → `degC`, `K` → `degC` | `(x - 32.0) * (5.0 / 9.0)`, `x - 273.15` |
| `kPa` → `Pa` | `x * 1000.0` |
| `inH2O` → `Pa` | `x * 249.08891` (conventional inch of water, NIST SP 811) |
| `inH2O_60F` → `Pa` | `x * 248.84` (inch of water at 60 °F) |
| `cfm` → `m3/s` | `x * 0.00047194744832` (exact: 0.3048³ / 60) |
| `L/s` → `m3/s` | `x * 0.001` |
| `percent` or `%` → `1` | `x / 100.0` |
| identical spellings | identity, bits unchanged |

| Device type → CDL type | Rule |
| --- | --- |
| `bool` → `bool`; `bool` → `int` | identity; `false` 0, `true` 1 (Listing 12.2's `uOccSen`) |
| `float` or `int` → `bool` | exactly `0` is false, exactly `1` true; anything else refused |
| CSV text → `bool` | `true`/`false`, `active`/`inactive` (BACnet binary present value), `0`/`1` |
| `float` → `int` | integral and within `i32` only, else refused |

A refused cell is a typed `SourceError` naming file, row and column, never a silent threshold: the
upstream Modelica model turns fan enable into a Boolean with `GreaterThreshold(t = 0.5)`; the
verifier refuses a `0.5` instead (decision 19).

## Sources, sampling and time

**Frames.** The verifier merges its input sources on the union of their timestamps and hands replay
a `ColumnarSource` whose cells are missing (NaN for Real, a cleared validity bit otherwise)
wherever a file has no sample at that time, which replay reads as `None`. Replay's
`HoldLast { max_age }` (per source, from `hold.maxAge`) fills them and records every substitution in
the trace; with no hold a missing cell refuses the frame (decision 15). Time is
`TimeBase::Recorded`, so irregular trend steps reach the engine's timers and integrators unchanged.
`DuplicateTime::Allow` is always set: §12.2 allows "two equal time stamps to indicate when a value
switches", and each equal-time row is its own frame and its own trace row. Trend times are
converted to model seconds from `time.origin`, which the trace and receipt keep once as metadata.

**Comparison.** The funnel compares two curves without a shared grid: bounds built from the
reference are interpolated at the test series' times. The verifier only prepares the two series
(decision 14):

- `window` clips both to `[start, stop]`. Each series gets a sample exactly at `start` and `stop`,
  its zero-order-hold value there, so funnel's precondition ("reference and test data minimum x
  values" equal, and the maxima) holds by construction. A series with no sample at or before `start`
  makes that output `no_data`.
- Without `sampling`, both series are compared at their own times. With `sampling = s`, both are
  resampled by **zero-order hold** at `start + k * s` (by multiply, as replay's grid), taking the
  last sample at or before each instant, the last of an equal-time pair included.
- Zero-order hold, not linear interpolation, because the engine's outputs change only at frames and
  a trend log reports the last value it saw: interpolating would invent values neither controller
  produced and smear a late step that the time tolerance exists to judge.

**Roles.** By default the funnel's reference is the engine series: §12.7.2 compares "the output
variables generated by the controller … against the corresponding output variables generated by
the CDL simulation", and the CDL is the specification. The funnel tool's own example puts the trend
in the reference role (`funnel --reference trended.csv --test simulated.csv`), so
`funnelReference: "trend"` reproduces that; the envelope is not symmetric, and the receipt records
which (decision 11).

**Caps.** Every source is bounded by replay's limits. The comparison adds its own, because the
`aligned-grid` envelope is quadratic in samples: per output at most 1,048,576 compared points for
`funnel` and 32,768 for `aligned-grid` (one day at 5 s is 17,281), each a typed
`CompareError::TooManyPoints`, plus a work counter on loop removal (below), never wall-clock time
(decision 22).

## Envelope

**`funnel` (decision 1).** A faithful Rust port of LBNL funnel's tube construction, pinned to
`lbl-srg/funnel` commit `a12fa96` (`pyfunnel` 2.1.1), specified by its C source
(`tubeSize.c`, `algorithmRectangle.c`, `tube.c`) and proven against it:

1. **Rectangle per reference point.** Half-width `w_i = max(atolx, rtolx · range_x, ltolx · |x_i|)`
   and half-height `h_i = max(atoly, rtoly · range_y, ltoly · |y_i|)`: the terms combine by
   **maximum**, and `rtol` is relative to the reference's range ("relative to the range of `x` and
   `y` … mainly for compatibility with csv-compare"). A half-size that is zero becomes
   `rtol · magnitude` (`max(max, |min|)`) when `rtol` is non-zero, then at least `1e-10`.
2. **Normalization.** `x` and the half-widths are divided by `x`'s magnitude when it exceeds 1e-5.
3. **Corners.** Leading duplicate points are skipped. The upper curve starts at the first
   rectangle's top-left corner; at each interior point where the slope changes, the slope signs
   before and after choose the corners: rising or flat on both sides takes top-left, falling or flat
   takes top-right, a peak takes top-left then top-right, a valley top-right then top-left; corners
   on a flat run are dropped. It ends with the last rectangle's top-right corner (preceded by its
   top-left when the last slope rises). A vertical step uses the slope sentinel `±1e15`. The lower
   curve mirrors this with bottom corners.
4. **Loop removal.** Where consecutive corners run backwards in `x` (a local extremum with wide
   rectangles), the backward stretch is cut at the intersection of the segments around it, and
   points strictly inside the new envelope are dropped.
5. **Error.** Both curves are interpolated linearly at each test time, holding the last value
   beyond their end; the error is `max(0, y - y_up) - min(0, y - y_low)`, never negative.

Equality is funnel's `equ` (`|a - b| <= 1e-10 · max(1, |a|, |b|)`), reproduced exactly. The port
keeps funnel's behaviour where it differs from this repo's harness: `max` rather than a sum of
terms, and errors at test times only, where `oce-conformance` also checks bound breakpoints.

**Proof by differential test (decision 13).** `pyfunnel` writes every curve as `%.16g`, too few
digits to round-trip a double, so the claim is textual and stated as such: for each pinned case the
port's `reference`, `test`, `lowerBound`, `upperBound` and `errors` curves, printed with a `%.16g`
emulation, equal pyfunnel's five CSV files byte for byte. The cases are funnel's own test set (17
directories, `fail1`–`fail6`, `should_succeed1`, `success1`–`success10`, with their parameters), a
hostile set (constant series, a single point, equal times, huge and tiny `x` magnitude, every
tolerance kind alone) and one Building 33 day. A pinned script runs `pyfunnel==2.1.1` from its wheel
on x86_64 Linux (no fused multiply-add) and writes the files plus a provenance record (package
version, wheel SHA-256, funnel commit, platform); the script, its pins and the fixtures are
committed, and the fixtures are regenerated only by an issue that says so. The port's own outputs
are also golden-tested bit for bit.

**`aligned-grid`.** The conservative envelope `oce-conformance` uses today (`build_bounds`): a grid
of every sample and rectangle edge, min and max over covering rectangles, half-sizes as a **sum** of
terms. Wider than funnel and never leaky, but not what §12 references; a receipt names it, and it
is for conformance-style work.

**Discrete outputs.** Boolean and Integer outputs never enter an envelope (decision 8): the funnel
is type-blind, so a value between two levels could pass "within tolerance". They are compared
exactly at the comparison instants; a mismatch is a failure with exceedance `1` at that instant.
Enum outputs compare by ordinal, the same way.

## Indicators

§12.7.2 step 8, followed literally (decision 2): "set the value of the reference and the controller
output to zero whenever the indicator function is zero for that time stamp. This will exclude the
value from the verification." An indicator name is a sequence input or output (`<sequence>.<path>`,
read from the trace, which therefore always retains inputs) or a column of an `indicatorSources`
file (§12.7.3's `indicator.csv`). Several indicators for one output are ANDed; values are Boolean
or exactly `0`/`1`, held from each sample to the next, and false before the first.

Masked instants are zeroed in both series before the envelope is built, so the funnel spans the
step at a mask edge as LBL's tool does. They are counted as `masked`, excluded from the verdict and
from `compared`, and listed as intervals in the report ("indicator exclusions should be
documented"); an exceedance at a masked instant is reported separately and never fails the output
(decision 23). The conformance harness keeps its stricter segmentation ("it never builds a tolerance
band across an inactive gap"), named there as a deliberate deviation for conformance work.

## Report and receipt

Per output (decision 6): `status` (`pass`, `fail` or `no_data`), `compared` and `masked` counts,
the masked intervals, the worst exceedance and its time, the first exceedance time, the total time
in exceedance, the envelope and the absolute half-widths applied (min and max over the series).
**Zero compared points is `no_data`, never `pass`.** The run passes only when every output passes.
The exceedance curve is a trace column, `<output>.exceedance`, one `f64` per test instant; the
bound curves and pyfunnel-named CSVs are opt-in.

The receipt (decision 7) is plain data. Its header is shared with the M3 vector receipt and holds
only fields both have; the body is the verifier's; there is no mutant score. `receipt_id` is
FNV-1a-128 over the canonical receipt with `timings` removed, so two runs on the same inputs give
the same id (decision 20).

```json
{
  "schema": "oce/receipt/v1",
  "kind": "commissioning",
  "content_id": "oce:canon1:fbbea1ef32cb24797011163d09ead7ee5c17757a4aad551b6989c745740a07a5",
  "engine_version": "0.3.0",
  "spec_id": "spec:fnv1a128:9e4d2a7c1b0f8e6d5c4b3a2918070605",
  "comparison": { "mode": "tolerance", "envelope": "funnel", "envelope_version": "funnel@a12fa96" },
  "passed": false,
  "receipt_id": "receipt:fnv1a128:0f1e2d3c4b5a69788796a5b4c3d2e1f0",
  "timings": { "load_ms": 4, "run_ms": 61, "compare_ms": 9 },
  "body": {
    "data": "trended", "sequence": "cooValSta", "parameters": [],
    "time": { "base": "recorded", "origin": "2018-06-07T00:00:00-07:00",
              "window": [0, 86400], "sampling": 5 },
    "units": { "table": "units/v1", "columns": [
      { "device": "Supply Air Temp", "from": "degF", "to": "K" },
      { "device": "Clg Coil Valve", "from": "percent", "to": "1" } ] },
    "sources": [{ "path": "data/Supply_Air_Temp.mos", "digest": "fnv1a128:5a0b…", "rows": 75563 }],
    "frames": { "executed": 17284, "substituted": 4, "refused": 0 },
    "outputs": [ {
      "output": "cooValSta.yCooVal", "status": "fail", "funnel_reference": "engine",
      "tolerances": { "atolx": 1, "rtoly": 0.01 }, "half_width": [1, 1],
      "half_height": [0.01, 0.01], "compared": 17281, "masked": 0, "masked_intervals": [],
      "worst": { "t": 41235, "exceedance": 0.0412 }, "first_exceedance_t": 30410,
      "exceedance_time_s": 315, "masked_exceedance": [] } ],
    "findings": []
  }
}
```

The numbers above illustrate the shape; they are not a prediction. `data` is `trended` or
`synthetic` (decision 4). The vector receipt fills the same header with `kind: "vectors"`, `mode:
"exact"` or `"tolerance"`, and its own body (scenarios, pass counts, mutant score).

## Rust API sketch

```rust
// crates/leverage/oce-verify, tolerance mode: depends on oce-api and oce-replay only.
pub struct CommissioningSpec { /* schema, time, tolerances, sampling, references */ }
impl CommissioningSpec {
    /// Strips `//` and `/* */` outside strings, then parses strictly; bounded input.
    pub fn from_json_str(text: &str, limits: &SpecLimits) -> Result<Self, SpecError>;
    pub fn to_canonical_json(&self) -> String;               // strict, with `schema`
    pub fn spec_id(&self) -> String;                          // over canonical bytes
}
pub enum Envelope { Funnel, AlignedGrid }      pub enum FunnelReference { Engine, Trend }
pub fn convert_unit(from: &str, to: &str, x: f64) -> Result<f64, UnitError>; // closed `units/v1`

/// The bytes a run reads, supplied by the caller: the library opens no file.
pub struct SpecInputs { pub cxf: Vec<u8>, pub files: Vec<(String, Vec<u8>)> }
pub fn run_commissioning(spec: &CommissioningSpec, reference: usize, inputs: &SpecInputs,
                         options: &CommissionOptions) -> Result<CommissionRun, VerifyError>;
pub struct CommissionRun { pub receipt: Receipt, pub trace: oce_replay::Trace,
                           pub exceedance: Vec<(String, Vec<f64>)>, pub bounds: Option<Bounds> }
pub fn funnel(reference: Series<'_>, test: Series<'_>, tol: &Tolerances,
              limits: &CompareLimits) -> Result<FunnelResult, CompareError>;
pub enum OutputStatus { Pass, Fail, NoData }
pub struct Receipt { pub header: ReceiptHeader, pub body: ReceiptBody }
pub enum ReceiptBody { Vectors(VectorBody), Commissioning(CommissioningBody) }
pub struct ReceiptHeader { pub schema: String, pub kind: ReceiptKind, pub content_id: String,
                           pub engine_version: String, pub spec_id: String,
                           pub comparison: Comparison, pub passed: bool,
                           pub receipt_id: String, pub timings: Timings }
```

*(Amended 2026-11-10, with GitHub #558, issue #520:)* `Receipt.body` is `ReceiptBody`, the one closed enum the
[vectors note](vectors.md) decides (decision 21), not `CommissioningBody`: there is one `Receipt`
type and one writer of the shared header. `oce-verify` builds it with the `Vectors` variant only
(`#[non_exhaustive]`); the commissioning PR adds the `Commissioning` variant and its `ReceiptKind`
in place.

Every error is a typed variant (`SpecError`, `UnitError`, `SourceError`, `CompareError`, replay's
errors and the engine's `OcError` wrapped) naming the spec field, file, row or output; nothing
panics on hostile input.

## CLI

`oce verify --commission <spec> [--reference <n>] [--out <dir>] [--bounds] [--format human|json]`,
in the `oce` binary of the [authoring layer](authoring-layer.md#cli). The CLI reads the files the
spec names (paths relative to the spec, never absolute), runs every reference unless `--reference`
picks one, and writes the receipt, the trace and the exceedance columns to `--out`. Exit codes
follow the `oce` convention (0 yes, 1 no, 2 error): **0** every output passed; **1** not passed,
covering `fail` and `no_data` (the receipt says which); **2** a usage error, a refused spec, mapping
or unit, an unreadable source, an engine refusal or an I/O failure (decision 21).

## Building 33 data

The trend files exist upstream and are vendorable. `Buildings/Resources/Data/Utilities/Plotters/
Examples/ControlsVerification_CoolingCoilValve/` holds `Supply_Air_Temp.mos`, `SA_Clg_Stpt.mos`,
`OA_Temp.mos`, `VFD_Fan_Feedback.mos`, `VFD_Fan_Enable.mos`, `Clg_Coil_Valve.mos`, a `README.md`
("5s interval data trends … B33-AHU-2-ClVal-5s, 3 days of data starting at 2018-06-07 00:00:00 PDT")
and `csv2mos.py`, about 11.6 MB, with identical blobs at the repo's existing modelica-buildings pin
`a131864`. Each `.mos` is a CombiTimeTable with columns `Seconds_Elapsed, Unix_Time, Value`: 75,563
to 75,578 rows, 5 s steps to 376,910 s, then 600 s; no equal or decreasing times. Fan enable is a
change-of-value log of seven samples, all `1.0`. The sequence is
`Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve` (PIDWithReset, Line, two
Hysteresis, And, Min, Change, BooleanToReal, Multiply, Constant), outside today's vendored cone.

So the worked example uses real trends (decision 4). The implementing PR adds the six `.mos` files,
their README and the sequence's `.mo` verbatim under `third_party/modelica-buildings-cdl/` at the
existing pin, and its CXF translated at the recorded modelica-json pin, following the vendored
README's pin-advance process (manifest, license, `gate.sh full`) even though the pin does not move.
A synthetic trend generated from the engine, labelled `data: synthetic` in its receipts, is used
only where no recorded data exists: unit-level tests and the hostile set.

The verdict on Building 33 is not predicted here. §12.6 names two real residuals ("ALC EIKON uses a
discrete time step", "a proprietary algorithm for the anti-windup"), and the engine's HostTick
integration is a third difference from Dymola's continuous solver. The first run records whatever
verdict results; a failure is a finding to adjudicate, never a reason to widen the tolerance.

## Decisions

Decisions 1 to 4 are the owner's; 5 to 10 are recommendations accepted by the orchestrator. Both are
settled. Decisions 11 to 26 were ruled on by the architect pass; 17 and 21 are as it replaced them.

**1. Envelope: a faithful port of funnel, named `funnel`,** proven by a differential test against
pyfunnel, with the conservative grid envelope kept as a second named mode, `aligned-grid`.
Rejected: the conservative envelope as the default. Why: a verdict must match the tool the
specification cites. **2. Indicators: §12.7.2 semantics,** zeroing both series where the indicator
is zero and documenting the masked instants; the conformance harness keeps its segmenting as a named
deliberate deviation. Rejected: segmenting everywhere. Why: defensible against the specification.
**3. Spec: a versioned strict-JSON superset of §12.7:** LBL fields load unchanged, comments are
stripped on load, strict JSON with a `schema` tag is written, the added fields are those above.
Rejected: a strict-only loader; a separate LBL importer. Why: an LBL example should load.
**4. Worked example data: vendor Building 33 if it exists, else a labelled synthetic trend.** It
exists ([Building 33 data](#building-33-data)). Rejected: synthetic only. Why: real sensor noise
matters where it is available.

**5. Point mapping: Listing 12.2 by path or inline,** every CDL input mapped, conversion in the
verifier's source adapter with a closed table and a typed error on an unknown pair, never in the
engine. **6. Report: `pass` / `fail` / `no_data` per output,** compared and masked counts, worst and
first exceedance, zero compared is `no_data`, exceedance a trace column, bounds opt-in. **7. Receipt
header shared with the M3 vector receipt** (`kind`, content id, engine version, spec id, comparison
mode and envelope, pass, timings), verifier fields in the body, no mutant score, never a gate on
loading. **8. Booleans and Integers exact,** never through the funnel. **9. Copy and extend, never
depend:** `VerifyConfig` and the funnel code are copied into `oce-verify`; `oce-conformance` is
verification-only and keeps its copy. **10. Replay provides** `DuplicateTime::Allow`, the origin as
metadata, row and column caps and `sampling` for `Grid` sources; resampling and indicators are the
verifier's ([the replay note](replay.md) records all six).

**11. the engine series is the funnel reference by default;** `funnelReference: "trend"`
reproduces the funnel tool's own example, and the receipt records the role. Rejected: a fixed trend
reference. Why: the envelope is asymmetric and the CDL is the specification.

**12. funnel's tolerance arithmetic, unmodified** (maximum of terms, range-relative
`rtol`, the magnitude and `1e-10` fallbacks); `aligned-grid` keeps its sum. Rejected: harmonising
the two. Why: decision 1 is fidelity; receipts carry the absolute half-widths either way.

**13. the differential claim is `%.16g` text equality** on funnel's 17 cases, a hostile
set and a Building 33 day, captured once by a pinned script on x86_64 Linux. Rejected: bit equality
(pyfunnel cannot emit it); a numeric tolerance (it would hide a porting error).

**14. comparison sampling is zero-order hold of both series;** raw times without
`sampling`; `window` clips and pins the endpoints. Rejected: linear interpolation (invents values);
resampling inside replay (replay decision 5).

**15. frames at the union of input times, holds declared per source** and applied by
replay's `HoldLast`, so every substitution is in the trace. Rejected: the verifier holding values
itself (unrecorded); a designated clock source (drops samples replay must observe).

**16. an explicit `window` in model seconds;** a series not covering `start` is
`no_data`. Rejected: comparing whatever overlaps (a short trend silently shrinks the test).

**17. Exact names, or whole-string globs where the key has `*` or `?`.** Any other key is an exact
`<sequence>.<path>`; a key matching nothing is an error; no `regex` dependency, and the copied
`VerifyConfig` drops its regex matching. Rejected: unanchored regular expressions (the draft; an
agent or a person writing `u` must not silently select every output); a `glob:` prefix (Listing
12.1 would need editing, against decision 3).

**18. Scenario 1 fields parse and are ignored with a finding;** `controllerOutput` is the
expected source when `expected` is absent. Rejected: refusing them (LBL specs would not load);
acting on them (the verifier drives no controller).

**19. `units/v1` as listed, discrete encodings exact.** Rejected: one ambiguous `inH2O`;
a `0.5` Boolean threshold (a sensor fault would read as a state).

**20. FNV-1a-128 identities** for `spec_id`, source digests and `receipt_id`, over
canonical bytes, `timings` excluded. Rejected: a new hash dependency in the verifier. *Reconciled
2026-11-09 with the canonical content id (GitHub #470, [content-id note](content-id.md)):* the
content id a receipt carries is M3's `oce:canon1:` id, SHA-256 through `sha2` on `oce-api`, which
the verifier reads from `Engine::content_id` and adds no edge for; `spec_id` and `receipt_id` keep
FNV-1a-128, and the receipt names both algorithms by their prefixes.

**21. Exit codes: the `oce` convention.** 0 pass; 1 not passed (`fail` or `no_data`, the
receipt says which); 2 usage errors, refused specs, unreadable sources and I/O. Rejected: a separate
"no verdict" code and an exit 3 (the draft). Why: the owner fixed 0 yes, 1 no, 2 error for every
`oce` subcommand; "nothing was verified" is a `no_data` verdict in the receipt, not an exit code.

**22. comparison caps,** 1,048,576 points per output for `funnel`, 32,768 for
`aligned-grid`, a work counter on loop removal, each a typed error pinned at its boundary.

**23. masked exceedance is reported, never counted.** Rejected: dropping it (hides mask
edges); failing on it (contradicts §12.7.2's "exclude").

**24. parameters in CDL units through halt, `set_param` and resume** before the first
frame, echoed in the receipt, never converted.

**25. the library opens no file;** it takes bytes and the CLI resolves paths relative to
the spec, as replay does.

**26. vendor Building 33 verbatim and whole** (about 11.6 MB beside the 3.46 MB tree).
The implementing PR follows the vendored README's pin-advance process although the pin does not
move, and brings the `CoolingCoilValve` `.mo` and its CXF at the recorded modelica-json pin.
Rejected: a one-day slice (not verbatim, outside the manifest's fidelity check).

## Relation to other work

- **Replay (#435, merged as GitHub #443; crate #445).** Needs `Recorded` time,
  `DuplicateTime::Allow`, the header origin, `ColumnarSource` and `CsvSource` with caps, `HoldLast`
  with substitution bits, an input-retaining trace and `check_coverage`; the replay note provides
  all of them and leaves comparison, resampling, indicators and units here. The `combiTimeTable`
  reader lives in `oce-verify`.
- **M3 `oce-verify` and the vector format** ([design note](vectors.md)). Tolerance mode shares
  source, tolerance and receipt header types with its `within` / `of` / `when`: "a §12
  commissioning spec and a library vector set are the same machinery". The vector receipt's body
  is M3's.
- **M5 notebooks** read the receipt through `serde` and the trace and exceedance columns through
  `Trace.to_pandas()`; plots are theirs.
- **Conformance harness.** `oce-conformance` keeps `VerifyConfig`, `build_bounds` and segmented
  masking unchanged, and is never a dependency.
- **Implementation issues**, filed when the M3 tracking issue is cut: the funnel port and its
  differential fixtures; spec, mapping, units and sources; the run, report and receipt; the CLI
  subcommand; Building 33 vendoring and the worked example. Goldens use `oce-bless` as a
  dev-dependency (authoring note, decision 19).

## Test plan

- **Funnel port.** The differential cases, from the committed capture script and pins (fixtures
  regenerated only by an issue that says so); bit-exact goldens of the port's own curves; each
  tolerance kind alone; zero half-sizes taking the fallbacks; vertical steps; leading duplicates;
  loop removal at a sharp peak; mismatched endpoints and non-finite input refused with variants.
- **Spec.** Listing 12.1 verbatim, comments included, loads; load → write → load is byte-identical;
  duplicate keys, trailing commas, unknown fields, `NaN` and over-limit input refuse; exact keys,
  globs with `*` and `?` anchored at both ends, a key matching nothing refused with
  `UnmatchedKey` and its message, `u` not matching `uFan`, Listing 12.1's keys loading unchanged;
  override order.
- **Units.** Every pair golden-tested bit for bit at `-0.0`, extremes and known points (`32 degF`
  is exactly `273.15 K`); unknown and one-sided pairs refuse; `0.5`, `2` and `NaN` in a Boolean
  column refuse.
- **Sources, time, indicators.** Holds at `maxAge` exactly and one ulp beyond; equal timestamps as
  two frames; a negative origin offset; window endpoints; zeroing, mask edges, ANDed indicators, an
  all-masked output `no_data`, masked exceedance excluded.
- **Report, receipt, CLI.** Golden receipts for pass, fail and `no_data`; `receipt_id` identical
  across runs and replay chunk sizes; exit 0, 1 (`fail`, `no_data`) and 2 per outcome class.
- **Building 33.** One day end to end, its receipt a golden regenerated only through `oce-bless`.

## Out of scope

- Driving a real controller, BACnet I/O, modelica-json and OpenModelica (§12.7.2 steps 1–7).
- Plots, sequence diagrams (§12.5.4) and validation (§12.4 step 2).
- Converting parameters; unit pairs beyond `units/v1` (each needs a table PR with goldens).
- Any change to `oce-api`, `oce-conformance` or the replay crate's scope; gating on a receipt.
- The vector receipt's body and the meaning of id prefixes (M3).

## Review rulings

The architect pass on the draft (orchestrator, 2026-11-08) ruled:

- 11 accepted: the engine series is the funnel reference by default, `trend` opt-in.
- 12 accepted: funnel's tolerance arithmetic from `tubeSize.c` is the contract.
- 13 accepted: `%.16g` text equality; capture script and pins committed, regenerated only by issue.
- 14 accepted: zero-order hold for both series, never interpolation.
- 15 accepted: frames at the union of input times, per-source `hold.maxAge` as `HoldLast`.
- 16 accepted: an explicit comparison `window`.
- 17 rejected and replaced: exact `<sequence>.<path>` keys; a key with `*` or `?` is an anchored
  glob, so Listing 12.1 loads unchanged; no `regex`.
- 18 accepted: Scenario 1 fields ignored with a finding; `controllerOutput` is the expected output.
- 19 accepted: two inch-of-water units, exact discrete encodings.
- 20 accepted: FNV-1a-128 ids; M3's canonical id may replace the hash, and the receipt names it.
- 21 rejected and replaced: the `oce` convention, 0 pass, 1 not passed, 2 error; no exit 3.
- 22 accepted: 1,048,576 points per output for `funnel`, 32,768 for `aligned-grid`.
- 23 accepted: masked exceedance reported, never failing the output.
- 24 accepted: parameters in CDL units through `set_param`.
- 25 accepted: the library takes bytes and opens no file.
- 26 accepted: verbatim and whole, through the pin-advance process, with the `.mo` and its CXF.
- Conflicts accepted: the C source over the brief's formula; `aligned-grid` keeps the summed
  tolerances; the model's `reverseActing = false` over §12.6's prose.
