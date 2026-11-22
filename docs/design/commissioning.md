# Commissioning verifier (OBC §12)

**Design note, written before any verifier code.** It fixes what the commissioning verifier is, its
test-spec and point-mapping files, unit conversion, the envelope, indicators, time handling, the
report and the receipt. Decisions 1 to 4 were settled by the owner and decisions 5 to 10 are
technical recommendations the orchestrator accepted; both are stated here, not reopened. Decisions
11 to 26 were made while drafting and ruled on by the architect pass, which accepted fourteen and
replaced 17 and 21 (see [Review rulings](#review-rulings)). It answers GitHub #436 and drives
[the replay API](replay.md) (GitHub #435, merged as GitHub #443; the crate is #445).
*(2026-11-20, issue #657: decisions 27 to 42 fix the run's readings and the normative receipt
body. The owner ruled on six of them, 27, 28 and 38 to 41; the other ten stand as written; see
[Run readings](#run-readings).)*

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
source when no `expected` block is given (decision 18). *(Amended 2026-11-21, owner's ruling:)*
when a reference gives both, `expected` is the source, and its `controllerOutput` is reported as
one more `lbl-field-ignored` finding, at `references[i].controllerOutput`; it is not a refusal.
The loader strips `//` and `/* */`
comments outside strings, then parses strictly (decision 3): duplicate keys, trailing commas,
`NaN` and unknown fields are typed errors. The verifier always writes strict JSON with a `schema`
tag, so a stored spec is canonical. *(Corrected 2026-11-20: the sentence gave as its reason that
LBL's "examples carry comments". Listings 12.1 and 12.2 as published carry none; the loader's
rule stands.)*

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
replay `HoldLast` policy for that source's inputs; absent, a missing sample refuses the frame,
which is recorded while the run continues (decision 28).

A key starts with the reference's `sequence` and a `.`, byte for byte, and that prefix is
literal. A key whose rest contains `*` or `?` is a glob (`*` any run of characters, `?` one
character, nothing else special): the rest is matched, whole, against the last `.`-segment of
each output's canonical path, so the names a glob sees are `<sequence>.<last segment>`. Any other
key is an exact name, and its rest resolves by the replay name rule against the outputs.
*(Amended 2026-11-21, owner's ruling; the sentence read "Overrides apply in declaration order,
later fields winning":)* which override applies to an output **does not depend on the order of
the file**. An exact key beats a glob, field by field. Two keys of one kind, two exact names or
two globs, that select one output combine when they give different tolerance fields, or one
field the same value; when they give one field different values no order could decide between
them, and the selection is refused at bind as `conflicting-override`, naming both keys, the
output and the field. The top-level `tolerances` lie under every key. Indicator rules have no
such conflict: every rule that selects an output applies, ANDed, each name once. The canonical
form therefore sorts every object's keys, those of `outputs` and `indicators` included, so a
tool that reorders a spec changes neither its meaning nor its `spec_id`. A key that matches no
output is an error, `SpecError::UnmatchedKey`, whose message names the key, whether it was read
as an exact name or a glob, and the sequence's output names as a glob sees them, so a typo cannot
pass silently; an exact name that names more than one output by the replay name rule is
`ambiguous-output`.
There is no substring or regular-expression matching: `u` selects only an output named `u`.
Listing 12.1's `"setPoiVAV1.TSup*"` is a glob and loads unchanged (decision 17; the prefix and
the last segment are decision 27's, 2026-11-20). A `parameters` key and a mapping's `cdl.name`
carry no prefix.

**`window`** is required to run a reference, as `cxf` is: a reference without one loads, because
an LBL file has none, and is refused at bind as `reference-incomplete`, naming the missing field
(decision 16). When present it holds both `start` and `stop`, finite, with `start <= stop`.

### As landed: the spec and mapping readers

*(GitHub #704, issue #658.)* The two readers are in `crates/leverage/oce-verify`
(`src/commissioning/`): `CommissioningSpec::from_json_bytes` and `from_json_str`,
`PointMapping::from_json_bytes` and `from_json_str`, each with `to_canonical_json`, and
`CommissioningSpec::spec_id`. This note said the loader is "bounded" and gave no numbers, and
left a few shapes unsaid; this is what was built.

- **Bounds.** The two readers of the crate have one profile, the strict cursor's: nesting at
  most 8, a string at most 4,096 bytes as written, a list at most 65,536 entries, each refused
  as `nesting-too-deep`, `string-too-long` and `list-too-long`. `SpecLimits` holds the rest, each
  a default and a maximum that a caller may lower and never raise: 4 MiB per document, comments
  included (`input-too-large`, decided from the length); 256 entries in one reference's
  `parameters` (`too-many-parameters`, decision 29); 4,096 keys in one `outputs`, `indicators`
  or `columns` map (`too-many-keys`: the most outputs a run compares, and the most columns a
  replay source has); and 2^28 work units for one selection of a reference's keys against a
  sequence's outputs (`work-limit-exceeded`: one unit per key and output compared and one per
  step of the glob matcher, never time; a selection whose keys times outputs alone pass the
  limit is refused before anything is allocated, and what a selection holds is one bit per key
  and output). Two costs of a selection are outside that count, because they grow with the
  sequence's outputs alone, which come from the engine's own frame and not from the spec: the
  index an exact key is resolved through (`oce_replay::PathIndex`, one sort of the output
  paths, `n log n` comparisons of path bytes) and the paths an ambiguous exact name collects
  for its refusal. For scale: a million outputs that share one last segment, and one exact key
  naming it, are refused as `ambiguous-output` after 7.4 s in a debug build. Every bound is
  inclusive and pinned at its edge. A load holds at most 32 bytes
  per byte of the document, pinned on the densest shape there is, a long list of one-digit
  numbers inside a Scenario 1 field; so about 134 MB at the 4 MiB maximum.
- **Comments.** `//` runs to the next line feed or carriage return, or to the end of the text.
  `/*` runs to the next `*/`; block comments do not nest, `/*/` is an open comment, and one that
  is never closed is `unterminated-comment` at its `/*`. A marker inside a string is text. Each
  comment byte becomes a space and a line feed stays, so a refusal's line and byte column are
  those of the text as written. The pass reads each byte once.
- **Refusals.** `SpecError` carries a closed `SpecErrorKind`, its kebab-case code
  (`SPEC_ERROR_CODES`), and for everything found in the text the line and column of the value
  concerned; only `not-utf8`, `byte-order-mark` and `input-too-large` have no place. The
  strict-JSON codes, `unknown-schema`, `too-many-parameters`, `empty-name` and `invalid-window`
  are the vector reader's words; `invalid-tolerance` and `work-limit-exceeded` are the envelope
  comparison's. The format's own are `unterminated-comment`, `too-many-keys`, `unknown-value`
  (a `format`, `envelope`, `funnelReference` or point `type` outside its list), `path-invalid`,
  `invalid-time-origin`, `invalid-sampling`, `invalid-hold` and `duplicate-point`, and three
  found only when bind matches keys against a sequence's outputs: `unmatched-key`,
  `ambiguous-output` (the vector bind's word) and `conflicting-override`. A sentence is at most
  512 bytes of printable ASCII, whatever the document wrote.
- **What a value must be.** `references` is required, and in a reference `model` and
  `sequence`. No name is empty (`empty-name`): a `model` names a class, a `sequence` prefixes
  every key, and a point that has no unit writes no `unit` key, so a unit that is written is
  not empty either. Every other key is optional at load. A tolerance is not
  negative; `sampling` is above zero; `hold.maxAge` is not negative; a `window` holds `start`
  and `stop` with `start <= stop`; `time` holds `origin`. An integer token read as a real is
  held to 2^53, as a vector set's times are. A trend source holds `path`, `format`,
  `timeColumn` and `columns`; `table` and `hold` are optional. `timeColumn` is a position
  counted from 1 (an integer token) or a header name (a string), and `columns` maps a file
  column, written as a string, to a device point name. Which of the two a format takes is the
  source reader's rule (#660), decided with the file's bytes.
- **Paths.** `cxf`, `controllerOutput`, a `pointNameMapping` written as a string, and a
  source's `path` are held to the rule a vector set's mutant paths are held to
  (`path-invalid`).
- **The envelope** a reference names is read as `oce_verify::Envelope`, the type the comparison
  takes (GitHub #687), by its own spelling; `funnelReference` is `FunnelReference`, whose
  default is the engine series (decision 11). `PartialTolerances::resolved` gives the
  comparison's `Tolerances`, a name nothing gave read as zero.
- **The Scenario 1 fields.** `generateJson`, `modelJsonDirectory`, `runController` and
  `controller` each take any one strict JSON value. The value is never interpreted and is kept:
  the canonical form writes it and `spec_id` covers it, so a spec and its stored form have the
  same findings. `CommissioningSpec::findings` lists them in the order the receipt's `findings`
  row states, and `findings_for(reference)` gives what one reference's receipt carries. A
  `controllerOutput` beside an `expected` is listed with them (the owner's ruling of
  2026-11-21), among its reference's fields in name order: `controllerOutput`, `generateJson`,
  `runController`. It is still held to the path rule and still written.
- **The canonical form sorts every key** and always writes `schema`. As first built it kept
  the declaration order of `outputs` and `indicators` keys, because the note then made that
  order the meaning; the owner's ruling of 2026-11-21 took the meaning out of the order, so the
  two maps are sorted like every other object, in the model as in the canonical form.
- **Keys and precedence** (crate-private until bind, #662, calls them: `src/commissioning/`
  `glob.rs` and `selection.rs`). `SpecReference::select_output_keys` and
  `select_indicator_keys` take the sequence's output **canonical paths**, sorted as
  `FrameSlots::outputs()` lists them. A glob selects every output whose last `.`-segment its
  rest matches, whole; an exact key selects the one output its rest names by
  `oce_replay::resolve_path`, so `ahu.yFan` and the whole canonical path are exact names too;
  a `#` is no boundary in that rule, so `s.Root.y` selects `urn:x#top.Root.y` and does not
  select `urn:x#Root.y`.
  A key without the reference's prefix, a glob that matches nothing and an exact name that
  names nothing are `unmatched-key`, with the sentence decision 17 asks for; an exact name that
  names several outputs is `ambiguous-output`. Then, for each output: the top-level
  `tolerances`, the globs that select it laid over them, the exact keys laid over those
  (`OutputSelection::tolerances_for`). Two keys of one kind that give one field different
  values are `conflicting-override`; "the same value" is the number, so `10` and `10.0` agree.
  A same-kind conflict is refused even when an exact key would set the field: two globs that
  disagree about an output are refused whether or not an exact key also selects it.
  `IndicatorSelection::indicators_for` gives the indicators of every rule that selects an
  output, each once. The two selections are two types, and each borrows its reference, so one
  cannot be read as the other or against another reference.
- **The mapping.** A list of pairs, in file order, with no `schema` key. A name is kept as
  written; a unit, when written, is not empty, is kept as written and is the unit table's to
  judge (#659); a name twice on one side is `duplicate-point` at its second place. Two columns
  of one source that name one device point load: what that means is the source reader's to
  decide (#660).

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
(replay's `InputMap`). *(Amended 2026-11-20, decision 27:)* a `cdl.name` is written as Listing 12.2
writes it, with no sequence prefix, and resolves by the replay name rule: against the inputs for
a point an `inputs` source reads, against the outputs for a point an `expected` source reads. A
point an `indicatorSources` source reads is the one mapped name the engine need not have: its
`cdl.name` is the indicator's name.

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
| `degF_delta` → `K` | `x * (5.0 / 9.0)` (a temperature difference) |
| `degC_delta` → `K` | identity (a temperature difference) |
| `kPa` → `Pa` | `x * 1000.0` |
| `inH2O` → `Pa` | `x * 249.08891` (conventional inch of water: ISO 80000-3's conventional value, through NIST SP 811 footnote 12) |
| `inH2O_60F` → `Pa` | `x * 248.84` (inch of water at 60 °F) |
| `cfm` → `m3/s` | `x * 0.0004719474432` (exact: 0.3048³ / 60) |
| `L/s` → `m3/s` | `x / 1000.0` |
| `percent` or `%` → `1` | `x / 100.0` |
| identical spellings | identity, bits unchanged |

*(Amended 2026-11-21, owner rulings during GitHub #716's review, and #714:)* the two difference
rows are added; the cfm digits are the exact quotient (the table had written `0.00047194744832`,
1.08e-8 relative too large, #714); `L/s` divides by an exact `1000.0` rather than multiplying by
`0.001`, which is not exact in binary and rounded twice; and the inch of water cites the source
of its 1000 kg/m³ basis. **A temperature difference is its own unit.** A setpoint adjustment, a
deadband or an approach is a difference between two readings, and it converts without the
offset: 2 `degF` is 256.48 K, 2 `degF_delta` is 1.11 K. Listing 12.2's `setAdj` and `heaSetAdj`
are setpoint adjustments, so their device side needs `degF_delta`, not the `degF` the listing
writes. Where the sequence port's declared quantity is `TemperatureDifference`, an absolute pair
(`degF` → `K`, `degC` → `K`, `degF` → `degC`, `K` → `degC`) is refused at bind (#662) as
`UnitError::AbsoluteForDifference`. **`cdl.unit` must be the port's unit.** A mapping's
`cdl.unit` that disagrees with the unit the sequence port declares is refused at bind as
`unit-refused` (#662). The identity rule and every row trust `cdl.unit` to be the engine's unit,
so they are only safe together with that check.

| Device type → CDL type | Rule |
| --- | --- |
| `bool` → `bool`; `bool` → `int` | identity; `false` 0, `true` 1 (Listing 12.2's `uOccSen`) |
| `float` or `int` → `bool` | exactly `0` is false, exactly `1` true; anything else refused |
| CSV text → `bool` | `true`/`false`, `active`/`inactive` (BACnet binary present value), `0`/`1` |
| `float` → `int` | integral and within `i32` only, else refused |
| `int` → `float` | a whole number within `i32`, widened exactly, else refused *(amended 2026-11-21, owner ruling)* |

*(Amended 2026-11-21, owner's ruling on #660:)* until replay's `CsvSource` can hand a text cell
over, a `csv` Boolean is read by its Boolean parse, `true`, `false`, `1` and `0`; `active` and
`inactive` are refused ("As landed: the trend sources"). *(Amended again by GitHub #739, issue
#728:)* `CsvSource` hands a text cell over, and a `csv` file's `bool` device column is read as
text by the row above, so the row holds as written.

A refused cell is a typed `SourceError` naming file, row and column, never a silent threshold: the
upstream Modelica model turns fan enable into a Boolean with `GreaterThreshold(t = 0.5)`; the
verifier refuses a `0.5` instead (decision 19).

### As landed: the unit table and the type rules

*(GitHub #716, issue #659.)* `units/v1` is `convert_unit` in `crates/leverage/oce-verify`
(`src/commissioning/units.rs`, with `UnitError`, `UnitSide`, `UNITS_TABLE` and
`UNIT_ERROR_CODES`), and the type rules are `src/commissioning/type_rules.rs`, crate-internal
until the trend sources (#660) call them. Its bit goldens come from a script that shares no code
with the crate (`tests/fixtures/units/`). These are consequences of building it:

- **One pair is listed both ways.** `degC` → `K` and `K` → `degC` are both rows, so the reverse
  of a row is refused for every row but those two.
- **What bind is handed.** The port-quantity rule above is
  `UnitConversion::for_port_quantity(quantity)`, crate-internal, which #662 calls with the
  facade's `PointInfo::quantity` for each mapped point; it refuses only on the exact quantity
  `TemperatureDifference`. Nothing here sees a port, so the `cdl.unit` check is #662's alone.
- **What the quantity check does not see.** `for_port_quantity` matches the bare string
  `TemperatureDifference`, byte for byte, and nothing else. A quantity carried as an IRI
  (`q:TemperatureDifference`, or a full QUDT IRI the importer keeps as `TermAttr::Iri`) is not
  refused, and neither is a quantity left unread: modelica-json translations write it as
  `qudt:hasQuantityKind`, so `PointInfo::quantity` is `None` there. A `*_delta` unit mapped onto
  a port whose quantity is `ThermodynamicTemperature` is not refused here either. All three
  belong to the binder (#662), which sees the port; a test lists the first two spellings as
  deliberately passing. *(Superseded 2026-11-21 by the owner's rulings in "As landed: the run,
  the comparison and the receipt": a quantity matches by its local name, and a positive rule per
  quantity refuses a difference row on an absolute port too.)*
- **Choices the note left open.** An empty unit spelling is refused as `empty-unit`, the device
  side's first, before the one-sided rule. A finite reading whose conversion overflows `f64` is
  refused as `overflow` (a factor above one, beyond about ±1e305). A NaN, replay's missing Real
  cell, and an infinity pass through every pair with their bits unchanged, so no platform's NaN
  propagation reaches a result. Identical spellings are the identity whatever they spell (`psi`
  to `psi`). `-0.0` is zero for the Boolean and the Integer rules. A text cell is read by the
  Boolean text rule only in a device `bool` column. `bool` → `float` is not in the type table and
  is refused as `type-pair-refused`.
- **The bound on the temperature rows.** Errors are counted in ulps of the largest magnitude in
  the sum, M, not of the result: near the offset the result cancels, and `-459.67 degF` converts
  to -2⁻⁴⁴ K, not zero. The two Celsius–kelvin rows are within 1 ulp(M) of the exact conversion,
  and that is proven: half an ulp for rounding 273.15 to `f64`, half for the add. The two
  Fahrenheit rows are held to 2 ulp(M), and that bound is **measured, not proven**: the script
  checks it against the exact rational at every golden row and at 140,000 seeded readings per
  pair (worst 1.62 for `degF` → `K`, 1.22 for `degF` → `degC`), and the test at every golden row.

## Sources, sampling and time

**Frames.** The verifier merges its input sources on the union of their timestamps and hands replay
a `ColumnarSource` whose cells are missing (NaN for Real, a cleared validity bit otherwise)
wherever a file has no sample at that time, which replay reads as `None`. Replay's
`HoldLast { max_age }` (per source, from `hold.maxAge`) fills them and records every substitution in
the trace; with no hold a missing cell refuses the frame (decision 15). *(Amended 2026-11-20,
decision 28:)* a frame refused for a missing sample is recorded and the run continues; the
comparison instants from its time up to the next executed frame are **uncovered**, zeroed in
both series and out of the verdict, and an engine refusal of a submitted frame is an error with
no receipt. Time is
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
(decision 22). *(Amended 2026-11-20, decision 42:)* the verifier refuses both at bind, before the
replay runs, as its own `too-many-points`, and holds the run to 16,777,216 points over all its
outputs under the same code, so the comparison's `too-many-points` is never reached and
`compare-refused` never carries it.

### As landed: the trend sources

*(GitHub #729, issue #660.)* `TrendSources::read(spec, reference, inputs, limits)` in
`crates/leverage/oce-verify` (`src/commissioning/`: `combi_time_table.rs`, `trend_*.rs`,
`source_error.rs`) turns the bytes of a reference's files (`SpecInputs`, decision 25) into the
merged inputs (`TrendInputs`, which becomes a `ColumnarSource` bound to the run's `InputMap`),
the expected and indicator series (`TrendSeries`, finite samples in CDL units and types) and the
receipt's `sources`, `mapping` and `units.columns` records. No engine is loaded. Every refusal
is a `SourceError` with a kebab-case code (`SOURCE_ERROR_CODES`, 39), the file as the spec
writes its path, the data row counted from 1, the column as the spec addresses it, and the
lower layer's code as its cause (a `UnitError`, a mapping's `SpecError`, a cell rule's).

**Ruled by the owner on 2026-11-21** (the note was silent or contradicted itself):

1. **A `csv` Boolean.** The type table's "CSV text → bool" row lists `active`/`inactive`, but a
   `csv` file is read by replay's `CsvSource`, which hands a cell over typed and never as text.
   A `bool` device column is read by its Boolean parse, so `true`, `false`, `1` and `0` are
   read and `active`, `inactive` and every other text are `bad-cell`, with file, row and column.
   A replay issue (#728) asks for the text, after which the row holds again. *Lifted by GitHub
   #739:* a `bool` device column is one of `CsvSource`'s text columns
   (`CsvFormat::text_columns`, `CsvSource::text_cells`), read by `boolean_from_text`: `true`,
   `false`, `active`, `inactive`, `1` and `0` are read, byte for byte once replay has trimmed
   the cell's padding, and every other text is `cell-refused` caused by `not-boolean-text`, with
   file, row and column, where it was `bad-cell`. A `bool` device column mapped to a CDL `int`
   reads the same text as 0 and 1.
2. **A `csv` time column's kind** is fixed per source by its first data row's time cell: a
   number of seconds, or an RFC 3339 instant converted against `time.origin` by replay's fixed
   split. The two grammars share no text. A later cell of the other kind is `bad-time`; a
   column of instants in a spec with no origin is `time-origin-required`. A `combiTimeTable` is
   always seconds.
3. **An infinity** is refused at read in every role and both formats (`non-finite-number`),
   before any frame, so the engine's frame-class refusal of decision 28 never sees one from a
   trend. NaN, an empty cell, `NA` and `NaN` stay missing in a `csv` (replay's rule).
4. **`controllerOutput` with no `expected`** is read (decision 18), per OBC §12.7.3: a `csv`
   whose first header column is the time in seconds and whose every other header is a device
   point of the mapping, with no hold. A header that names no mapped device point is
   `unmapped-point`; a file with no header row `no-header`; a header of the time alone
   `no-point-columns`; a repeated header `duplicate-column`; an instant in its time column
   `bad-time`. Beside an `expected` it is not read and not one of the reference's files, as
   GitHub #704's `lbl-field-ignored` finding has it.

Two more, **ruled by the owner on 2026-11-21** in the review of GitHub #729:

5. **OBC §12.7.3's printed example files are refused today.** Replay's `CsvSource` neither
   trims a cell nor a header, and §12.7.3 prints its csv files padded: a header
   `time, uHea, uCoo, ...` and rows like `   0,    1, ...`. Read as they are printed, the time
   cell `   0` is `bad-time`, and a header ` uHea` names no mapped device point
   (`unmapped-point`) unless the mapping writes its space. Trimming spaces and tabs around csv
   cells and headers belongs in replay's reader, issue #733; until it lands, such a file is
   refused. A `combiTimeTable` is not affected: its separators include spaces. *Lifted by GitHub
   #739:* replay trims the ASCII spaces and tabs, and only those, around every csv cell and
   header name it reads unquoted, so the printed `reference_input.csv` reads as an input source
   and, in "the same format", as a `controllerOutput`, and the printed `indicator.csv` as an
   indicator source (`tests/commissioning/trend_padded_files.rs`, over the files as printed). A
   mapping that writes the space (` uHea`) now names no header. A quoted cell keeps its spaces.
6. **The files of a reference together are bounded**: `TrendLimits::max_total_bytes`, 1 GiB
   by default and lowerable, never raisable, is the summed length of the files handed in
   (the mapping file, each trend source and the `controllerOutput`, a file named twice counted
   once, the CXF not counted), checked from the lengths before any file is parsed, and refused
   past it as `too-many-total-bytes`.

**Decided here**, under the delegation of GitHub #704's review:

- **Addressing, per format.** A `combiTimeTable` source gives `timeColumn` as a position and
  each `columns` key as a position from 1 in canonical decimal (`"3"`; `"03"`, `"+3"` and
  `"3.0"` are refused); a `csv` source gives both as header names, compared byte for byte (a
  header named `3` is a name; replay trims a header name's padding first, so OBC §12.7.3's
  `time, uHea` header has a column `uHea`, GitHub #739). A
  `table` is a `combiTimeTable`'s, and the time column is never a point. Anything else is
  `column-reference-invalid`.
- **One device point, one column per role.** Two columns that name one device point, in one
  source or in two sources of one role, are `point-read-twice`: whichever the reader took, the
  other's samples would be dropped unseen. Two roles may read one point (an input that is also
  an indicator).
- **Equal times across files** (the issue's recommended reading). The frames are the instants of
  the union of the input sources' times. At an instant where a source has `k` rows, its `i`-th
  row fills the `i`-th frame there, and there are as many frames as the source with the most
  rows there has; a source with no `i`-th row is missing at that frame, so its hold fills it from
  its last observed sample, its own row at that instant. Two files with one row each at `t` give
  one frame; a file with two rows at `t` beside one with one give two. Pinned by test, by the
  fixture's independent derivation and by a replay of it.

**The `combiTimeTable` grammar** is §12.5.1's and no wider: the file starts with `#1`, after at
most a UTF-8 byte-order mark, which is skipped; `#` comments run to the line end anywhere a
separator may stand; a matrix is `double <name>(<rows>, <cols>)` at the start of a line
(`float` and other types refused, and an indented header, or one on the line of the previous
matrix's last value, is `table-syntax`: a deliberate narrowing, which real Modelica exports have
not been tested against, as no `#1` file is vendored under `third_party/` yet); values follow in
row order, wherever lines break, separated by spaces, tabs, line ends and comments with at most
one `,` or `;`, and after a matrix's last value a number is `size-mismatch` while any other text
is `table-syntax`; a number is the
C decimal form `[+-]? (D+ ('.' D*)? | '.' D+) ([eE] [+-]? D+)?`, with no `inf`, `nan` or
hexadecimal, converted by Rust's `f64` parser, which rounds correctly, so the same text gives
the same bits on every target. A file with several matrices needs `table` (`table-required`);
`table-not-found` and `duplicate-table` are refusals, and every matrix is held to the grammar
whether chosen or not. A declared size that the values do not fill, or overfill, is
`size-mismatch`. A `csv` number is read by Rust's `f64` grammar through `CsvSource`.

**Time.** Model seconds from `time.origin`; within a file, never decreasing (`time-regression`),
equal times kept; `-0.0` is read as `0.0`, so a time of `-0` and one of `0` in two files are one
instant. The origin is carried once, as `TrendInputs::origin` and the frame source's.

**Cells.** Each is converted exactly once: an infinity refused, then the point's cell rule (a
`0.5` in a Boolean column is `cell-refused`, cause `not-boolean`), then, for a CDL Real, the
unit table (an overflow is `unit-refused`, cause `overflow`). Two readings the note left open
follow from the rules as written and are pinned: a unit conversion other than the identity on a
CDL `int` or `bool` point is `unit-on-discrete-point`, since no row of `units/v1` preserves an
integer; and an unknown, one-sided or empty unit pair, a refused type pair and an unmapped point
are found from the spec and the mapping alone, before any file is parsed. A point of the mapping
that no source reads is not an error here: an input left unsupplied is refused at bind (#662).

**What #662 is handed.** The merged columns are named by the mapping's `cdl.name` and map onto
the inputs by the replay name rule (`TrendInputs::column_specs`, `column_map`); the run-level
policy is `Refuse` and each column of a source with `hold.maxAge` has a `HoldLast { max_age }`
override (`overrides`); `time_base()` is `Recorded` and `duplicate_time()` `Allow`. A merged
Real column holds NaN where its file has no sample at a frame, an Integer or Boolean column a
cleared validity flag. The order of `files` and of the input sources' declarations changes no
merged byte and no digest; only `sources[]` follows the declaration order, as the body table
says.

**Bounds** (`TrendLimits`, each a default and a maximum a caller may lower and never raise):

| Bound | Value | Code | Why this value |
| --- | --- | --- | --- |
| sources per reference | 256 | `too-many-sources` | decision 42's bound on the complete `sources` list |
| bytes of a reference's files together (mapping, sources, `controllerOutput`; each once; not the CXF) | 1 GiB | `too-many-total-bytes` | the owner's ruling of 2026-11-21; decided from the lengths before any file is parsed |
| bytes of one file | 1 GiB | `too-many-bytes` | replay's `max_source_bytes`, decided from the length; the total below is checked first, so under the default limits one file of 1 GiB and one byte is `too-many-total-bytes` |
| values one source keeps, and the merged inputs hold | 1 GiB | `too-many-bytes` | the same cap as a `ColumnarSource`'s bytes, checked from the counts before allocating |
| rows of one file, frames of the merge | 16,777,216 | `too-many-rows` | replay's `max_rows`; the merge counts its frames before allocating and stops at the first past the cap |
| columns of one file, of the merge (time included) | 4,096 | `too-many-columns` | replay's `max_columns`, the spec's `max_keys` |
| one csv cell, one number, one matrix name | 256 bytes | `cell-too-long` | replay's `max_cell_bytes` |

A line has no bound of its own: the `combiTimeTable` reader holds no line, only the number
being converted, and replay's reader holds one record of capped cells. **What a read costs.**
Reading one file holds at most about 25 bytes per byte of it by construction (a cell is at least
its separator, one byte, and is held as at most 16 bytes while read and 9 once merged); the
hostile-bytes test pins 48 per byte plus 64 KiB for one source, and a Building 33-shaped source
of 75,578 rows peaks at 32 bytes a row as `combiTimeTable` and 36 as `csv`. Every source's
values are held until the merge; a `csv` source's lists are shrunk to its rows once read, and an
expected or indicator series holds its own times (8 bytes a sample) beside its values, its read
consumed as it is built. **The merged inputs are not bounded per byte of input**: every merged
column is as long as the whole timeline, so they hold `frames × (8 + Σ widths)` bytes (8 a Real,
9 an Integer, 2 a Boolean), quadratic in the number of input sources whose times differ: 64
sources of 100 rows each, 44 KB of files, merge into 3.3 MB, and at 256 sources about 4 MB of
files reach the 1 GiB cap. That product is checked against `max_source_bytes` from the counts
before anything is allocated, and the test that pins it shows a refused merge allocating no
column. The walk over the sources is `O(R log S)` for `R` rows over `S` sources; filling the
merged columns is `O(frames × columns)` more, bounded by the same cap.

**A switch beside a source with no hold.** Under the equal-times rule, a source that writes a
switch as two rows at `t` (§12.2) makes two frames at `t`, and every other input source with no
row for the second is missing there: with no `hold`, that frame is refused for a missing sample
(decision 28's uncovered stretch). A spec whose sources record switches should declare `hold`
on every other input source, `{ "maxAge": 0 }` at least, which carries that source's own row at
`t` into the second frame at `t` and holds nothing older.

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

*(As landed, GitHub #687, issue #655:)* the envelope arithmetic is in `oce-verify` as a
library surface (`funnel`, `aligned_grid`, `Tolerances`, `Series`, `Curve`, `Envelope`,
`FunnelResult`, `CompareLimits`, `CompareError`, `format_g16`, `FUNNEL_VERSION`,
`ALIGNED_GRID_VERSION`); no vector file,
receipt or verb reaches it yet. The fixture is
`crates/leverage/oce-verify/tests/fixtures/funnel/` (its README has the pins, the capture and a
row per case). Six things are as the tree found them, not as this section said:

- **Two of funnel's 17 cases are refusals.** At the pinned commit funnel refuses `fail2` and
  `fail6` ("maximum x values are different", which its own test list expects) and writes no
  curve, so those two are compared as refusals: the port returns `mismatched-endpoints` at the
  same end. The other 15, and 45 of the 49 hostile cases, are compared as five files each; the
  remaining four hostile cases are refusals by the ends as well.
- **Upstream's committed `results/` are not the oracle.** None equals what the pinned script
  writes byte for byte: they predate the `%.16g` print format, the current tube and the endpoint
  refusal, and upstream's harness reads its inputs through pandas. The fixture README records the
  difference case by case.
- **The port refuses where funnel defines no result.** A local `x` tolerance above one can make
  funnel's loop removal read outside its arrays (found by a seeded search of random cases, checked
  with an AddressSanitizer build of the pinned source). The port returns `loop-removal-undefined`
  on exactly those inputs, and `non-finite-envelope` where a bound or an error overflows.
- **The work bound is 2^28 units** (decision 22 named a work counter and no number), one budget
  per comparison. The cost per reference point grows with the width of the `x` tolerance in
  samples: measured on a random walk, 9 units at one sample wide, 28 at 35, 70 at 300, 123 at
  1,000 and 205 at 3,000. A day at 5 s uses under 1 % of the bound at any of these. A series at
  the point cap has 256 units per point, so a trend of a million points under an `x` tolerance
  thousands of samples wide is refused `work-limit-exceeded` (the random walk at 5,000 samples;
  the review of GitHub #687 measured refusal from about 1,500 on its signals). A rising sawtooth
  under a local `x` tolerance costs about 0.7 units per point squared and is refused from about
  19,400 points.
- **Whether a series is inside its own funnel depends on the grid.** On a grid from `x = 0`
  with one sample per instant the construction does not leak: a series is inside its own band
  for every `x` tolerance from none to ten ticks. Three limits, each pinned as a hostile case
  that pyfunnel answers the same way and as a test (`tests/funnel_tick_grid.rs`):
  - *rounding, with no `y` tolerance.* The band is then only the `1e-10` floor high; with an `x`
    tolerance of exactly one tick a corner of the bound sits on a sample, and a step of a
    million or more rounds by more than the floor (`identical_large_step_no_y_tolerance`:
    outside by `2.3e-10`). Any `y` tolerance above the rounding removes it.
  - *several samples at one instant, with no `x` tolerance.* The rectangles are the `1e-10`
    floor wide, which funnel's equality cannot tell from zero once `x` is a few units large.
    Three samples that do not rise or fall in order leak from an `x` of about 4; a step written
    as two equal time stamps (§12.2) is inside while `x` is in the hundreds of thousands and
    leaks once it is in the millions. Any `x` tolerance removes both.
  - *`x` more than about 1e10 times its step*, such as epoch seconds in steps under 0.17 s:
    funnel's equality, `1e-10` relative, reads neighbouring instants as one. Model seconds from
    `time.origin` stay far below that.
- **pyfunnel's output depends on its platform.** Its arm64 macOS library contracts fused
  multiply-adds and prints 38 of the fixture's 306 files differently; the port pins the x86_64
  Linux result on every target, which is why the capture requires that platform.

The Building 33 day of the differential set follows in #664, captured with the same script once
the trends (#661) and an engine series (#663) exist. *(2026-11-21: moved to #755 with the worked
example. Landed with it, GitHub #765: see "As landed: the worked example".)*

**`aligned-grid`.** The conservative envelope `oce-conformance` uses today (`build_bounds`): a grid
of every sample and rectangle edge, min and max over covering rectangles, half-sizes as a **sum** of
terms. Wider than funnel and never leaky, but not what §12 references; a receipt names it, and it
is for conformance-style work.

**Discrete outputs.** Boolean and Integer outputs never enter an envelope (decision 8): the funnel
is type-blind, so a value between two levels could pass "within tolerance". They are compared
exactly at the comparison instants; a mismatch is a failure with exceedance `1` at that instant.
*(Amended 2026-11-20, decision 34:)* a sequence with an Enum boundary input or output is refused
at bind, `enum-port-unsupported`, until #465 gives replay a durable class for an Enum column.

**Where the verdict is taken (decision 32).** Under either envelope an instant's exceedance is
step 5's error at that test instant, from the envelope's two bound curves. `oce-conformance`'s
`compare` also checks the bounds' breakpoints between test samples, and the `aligned-grid` copy
reports those too; an `oce-verify` verdict reads only the errors at the test's own instants.

**No self-enclosure refusal here.** A vector window refuses a reference that lies outside its
own envelope ([vectors note](vectors.md), decision 41). A commissioning run does not: it
reports what the envelope the specification cites gives. So two identical series can fail here,
in the cases the funnel port measured ("as landed", above). By rounding: no `y` tolerance, an
`x` tolerance that puts a corner of a bound on a sample, and a step of about a million or more;
any `y` tolerance above the rounding removes it. With no `x` tolerance, where samples share an
instant: three that do not rise or fall in order, or a step written as two equal time stamps
(§12.2) once model seconds are in the millions, some weeks into a trend; any `x` tolerance
removes it, and Listing 12.1's is 10 s. And where `x` is more than about 1e10 times its step, which
model seconds from `time.origin` do not reach.

## Indicators

§12.7.2 step 8, followed literally (decision 2): "set the value of the reference and the controller
output to zero whenever the indicator function is zero for that time stamp. This will exclude the
value from the verification." An indicator name is a sequence input or output (`<sequence>.<path>`,
read from the trace, which therefore always retains inputs) or a column of an `indicatorSources`
file (§12.7.3's `indicator.csv`); decision 27 gives the rule for each kind of name. Several
indicators for one output are ANDed; values are Boolean
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

**Header.** The header's fields are defined once, in the table of the
[vectors note](vectors.md#the-receipt), for both kinds; this note adds no field to it. A
commissioning receipt has `kind` `commissioning`, the CXF's `content_id` as loaded (decision
30), the commissioning spec's `spec_id`, and a `comparison` whose mode is always `tolerance`.
This table says only what each `envelope_version` names (decision 32):

| `mode` | `envelope` | `envelope_version` | What the version names |
| --- | --- | --- | --- |
| `tolerance` | `funnel` | `funnel@a12fa96` | the `lbl-srg/funnel` commit the port is proven against |
| `tolerance` | `aligned-grid` | `aligned-grid@1` | this project's own construction, copied from `oce-conformance`'s `build_bounds` |

The tables below are normative (decision 31). `to_canonical_json` on the receipt follows the
vector file's rules, and arrays keep the order each row states.

**Body.**

| Field | Type | Present | Content, order and bound |
| --- | --- | --- | --- |
| `status` | string | always | `fail` when an output is `fail`; else `no_data` when `outputs` is empty or an output is `no_data`; else `pass`. The header's `passed` is `status == "pass"` |
| `reference` | integer | always | the zero-based index into the spec's `references[]` |
| `model`, `sequence` | string | always | as the reference writes them; labels only |
| `data` | string | always | `trended` or `synthetic` (decisions 4 and 41) |
| `parameters` | array | always | the edits as `{ "path", "value" }`, `path` the canonical parameter path, sorted by path bytes; `[]` when there are none; at most 256 |
| `content_id` | string | when `parameters` is not empty | `Engine::content_id` after the edits (decision 30) |
| `time` | object | always | `base` (always `recorded`); `origin`, when the spec gives one, as its canonical form spells it; `window` as `{ "start", "stop" }`; `sampling`, when one applies to the reference |
| `mapping` | object | when the mapping is a file | `{ "path", "digest" }`: the path as the spec writes it and `fnv1a128:<32 hex>` of the bytes supplied. An inline mapping is inside `spec_id` |
| `units.table` | string | always | `units/v1` |
| `units.columns` | array | always | one `{ "device", "cdl", "from", "to" }` (`cdl` is the mapping's `cdl.name` as written, never a canonical path) per point a source of the reference reads whose mapping gives a unit on both sides, identity pairs included, sorted by `device` bytes; at most 256 |
| `units.columns_total` | integer | always | the count before the cap |
| `sources` | array | always | one `{ "role", "path", "digest", "rows" }` per trend source of the reference: `role` is `inputs`, `expected` or `indicators`, in that order and in declaration order within each; `table` is added when the source names one; `digest` is `fnv1a128:<32 hex>` of the file's bytes; `rows` is the data rows of the table read: for `csv`, the rows after the header row; for `combiTimeTable`, the rows of the matrix read. Header, comment and blank lines are not rows. Complete, at most 256 |
| `frames` | object | always | `executed`, `substituted`, `refused`, by the trace's status byte. `refused` counts missing-sample refusals only: an engine refusal leaves no receipt (decision 28) |
| `load_warnings`, `load_warnings_total` | array of string, integer | always | `DiagCode` spellings in load order, at most 64; the count before the cap |
| `warnings`, `warnings_total` | array, integer | always | run-time warnings as `{ "t", "block", "message" }`, in row then emission order, at most 16; the count before the cap |
| `outputs` | array | always | one object per compared output (decision 40), sorted by canonical path bytes. Complete, at most 4,096 |
| `unverified_outputs`, `unverified_outputs_total` | array of string, integer | always | canonical paths of the boundary outputs no `expected` source supplies, sorted, at most 64; the count before the cap |
| `findings`, `findings_total` | array, integer | always | `{ "code", "at" }` per finding of the spec loader that concerns the top level or this reference, in the loader's order (the top-level fields sorted by name, then the reference's sorted by name, whatever the file's key order), at most 64; the count before the cap. The one code is `lbl-field-ignored`, and `at` is the field (`references[1].runController`) |

There is no mutant score and no gate (decision 7).

**Output.**

| Field | Type | Present | Content, order and bound |
| --- | --- | --- | --- |
| `output` | string | always | the canonical path |
| `type` | string | always | `real`, `integer` or `boolean`: a Real goes through the envelope, the other two are compared exactly (decision 8) |
| `status` | string | always | `no_data` when `compared == 0`; else `fail` when any compared instant has an exceedance above zero; else `pass` |
| `compared` | integer | always | the test instants compared. With `sampling` they are the instants `start + k * s` in the window. Without it they are the test series' own sample times in the window, with `start` and `stop` (decision 14): the test series is the trend's under `funnel_reference: "engine"` and the engine's under `"trend"`, and for a Boolean or Integer output it is the trend's, the engine's value held to each |
| `masked` | integer | always | the test instants an indicator excluded |
| `uncovered` | integer | always | the test instants in a refused stretch that no indicator masked (decision 28): from a frame refused for a missing sample up to the next executed frame, or to `stop` when none follows |
| `funnel_reference` | string | when `type` is `real` | `engine` or `trend`: which series the envelope is built around (decision 11). It is per output here because only a Real output has an envelope; a vector receipt carries one `funnel_reference` for its body, always `expectation` |
| `tolerances` | object | when `type` is `real` | the six names, every one present, after the overrides: `atolx`, `atoly`, `ltolx`, `ltoly`, `rtolx`, `rtoly` |
| `half_width`, `half_height` | object | when an envelope was built | `{ "min", "max" }` over the reference points, in model seconds and in the output's CDL unit, before `x` is normalised. Under `funnel`: the half-sizes of "Envelope" step 1 with its fallbacks, so never zero. Under `aligned-grid`: the sum of the three tolerance terms at each reference point, with no fallback, so zero when every tolerance of the axis is zero |
| `worst` | object | when `status` is `fail` | `{ "t", "exceedance" }`: the largest exceedance over the compared instants and the earliest instant that has it |
| `first_exceedance_t` | real | when `status` is `fail` | the earliest compared instant with an exceedance above zero |
| `exceedance_time_s` | real | always | the sum, in ascending time order, of `t[k + 1] - t[k]` over the compared instants `k` with an exceedance above zero, `t[k + 1]` the next test instant; the last instant adds nothing; `0.0` when there is none |
| `masked_intervals` | array | always | `{ "from", "to" }` in model seconds, the first and last instant of each run of masked instants, in time order, at most 64 |
| `masked_intervals_total` | integer | always | the count before the cap |
| `masked_exceedance` | array | always | `{ "t", "exceedance" }` for each masked instant whose error is above zero, in time order, at most 16; reported, never counted (decision 23). The error is the envelope's at that instant on the series as zeroed: the test's value there is zero, and the bounds are built from the zeroed reference |
| `masked_exceedance_total` | integer | always | the count before the cap |

`compared + masked + uncovered` is the number of test instants in the window. A tolerance name
that no tolerance object of the spec gives is written `0`. An envelope is built for a Real
output with at least one compared instant and no non-finite engine sample. A Boolean or Integer
mismatch is an exceedance of exactly `1` at that instant.

**When a masked instant has an exceedance.** OBC §12.7.2 step 8 zeroes both series where the
indicator is zero and then compares them as any two series; pyfunnel takes two series and no
mask. So the comparison still yields an error at a masked instant, and it can be above zero in
one place: a mask edge that falls between the two series' sample times. Without `sampling` the
reference and the test are zeroed at their own instants, and next to an edge the band still
runs from the reference's last unmasked value to zero while the test is already zero. With
`sampling` both series are zeroed at the same instants and the list is empty, except in the
rounding case of "Envelope". An uncovered instant's error is not recorded.

**A non-finite engine sample.** A Real output whose engine series holds a NaN or an infinity at
an instant that is neither masked nor uncovered is `fail`: no envelope is built for it,
`half_width` and `half_height` are absent, and `worst` is the earliest such instant with the
exceedance positive infinity. A sample above 2^1000 in magnitude is treated the same way, so
the envelope's arithmetic stays finite. This differs from a vector window, on purpose: there
the engine output is always the test, so the tick fails and the other ticks are still judged
([vectors note](vectors.md), decision 45). Here the engine series is the envelope's reference
by default, and no band can be built around a NaN; the output fails as a whole under either
role, so one rule covers both. The trend series holds finite samples only; the source reader
refuses any other cell (#660).

**Values.** A number the spec wrote (`window`, `sampling`, a tolerance, a parameter value) is
written as the spec's canonical form writes it. A computed Real (`t`, `from`, `to`, a half-size,
an exceedance, `exceedance_time_s`) is written in the canonical real spelling, so with a
fraction digit: `41235.0`. Negative zero, NaN and the infinities are the string `"0x"` and the
16 lowercase hex digits of the bits, as in a vector receipt. Counts are JSON integers.

A failing run with one Real output (ids and digests elided; keys in reading order where the
canonical bytes sort them):

```json
{
  "schema": "oce/receipt/v1",
  "kind": "commissioning",
  "content_id": "oce:canon1:fbbe…",
  "engine_version": "0.6.0",
  "spec_id": "spec:fnv1a128:9e4d…",
  "comparison": { "mode": "tolerance", "envelope": "funnel", "envelope_version": "funnel@a12fa96" },
  "passed": false,
  "receipt_id": "receipt:fnv1a128:0f1e…",
  "timings": { "load_ms": 0, "run_ms": 0, "compare_ms": 0 },
  "body": {
    "status": "fail", "reference": 0,
    "model": "Buildings.Utilities.Plotters.Examples.ControlsVerification_CoolingCoilValve",
    "sequence": "cooValSta", "data": "trended", "parameters": [],
    "time": { "base": "recorded", "origin": "2018-06-07T00:00:00-07:00",
              "window": { "start": 0, "stop": 86400 }, "sampling": 5 },
    "mapping": { "path": "b33-points.json", "digest": "fnv1a128:71c2…" },
    "units": { "table": "units/v1", "columns_total": 5, "columns": [
      { "device": "Clg Coil Valve", "cdl": "yCooVal", "from": "percent", "to": "1" },
      { "device": "OA Temp", "cdl": "TOut", "from": "degF", "to": "K" },
      { "device": "SA Clg Stpt", "cdl": "TSupSet", "from": "degF", "to": "K" },
      { "device": "Supply Air Temp", "cdl": "TSup", "from": "degF", "to": "K" },
      { "device": "VFD Fan Feedback", "cdl": "uFanFee", "from": "percent", "to": "1" } ] },
    "sources": [
      { "role": "inputs", "path": "data/Supply_Air_Temp.mos", "digest": "fnv1a128:5a0b…", "rows": 75563 },
      { "role": "inputs", "path": "data/SA_Clg_Stpt.mos", "digest": "fnv1a128:c81e…", "rows": 75563 },
      { "role": "inputs", "path": "data/OA_Temp.mos", "digest": "fnv1a128:27d4…", "rows": 75578 },
      { "role": "inputs", "path": "data/VFD_Fan_Feedback.mos", "digest": "fnv1a128:9f30…", "rows": 75563 },
      { "role": "inputs", "path": "data/VFD_Fan_Enable.mos", "digest": "fnv1a128:e6a2…", "rows": 7 },
      { "role": "expected", "path": "data/Clg_Coil_Valve.mos", "digest": "fnv1a128:40bb…", "rows": 75563 } ],
    "frames": { "executed": 17280, "substituted": 4, "refused": 0 },
    "load_warnings": [], "load_warnings_total": 0, "warnings": [], "warnings_total": 0,
    "outputs": [ {
      "output": "http://example.org#Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve.yCooVal",
      "type": "real", "status": "fail", "compared": 17281, "masked": 0, "uncovered": 0,
      "funnel_reference": "engine",
      "tolerances": { "atolx": 1, "atoly": 0, "ltolx": 0, "ltoly": 0, "rtolx": 0, "rtoly": 0.01 },
      "half_width": { "min": 1.0, "max": 1.0 }, "half_height": { "min": 0.01, "max": 0.01 },
      "worst": { "t": 41235.0, "exceedance": 0.0412 }, "first_exceedance_t": 30410.0,
      "exceedance_time_s": 315.0,
      "masked_intervals": [], "masked_intervals_total": 0,
      "masked_exceedance": [], "masked_exceedance_total": 0 } ],
    "unverified_outputs": [], "unverified_outputs_total": 0,
    "findings": [], "findings_total": 0
  }
}
```

The numbers show the shape; they are not a prediction. The example lists the worked spec's six
sources and the five points that carry a unit on both sides (the fan-enable point has none);
its CDL names, units, digests and row counts stand for what #664 writes into `b33-points.json`
and reads from the vendored files. The vector receipt fills the same header with
`kind: "vectors"`, `mode: "exact"` or `"tolerance"`, and its own body (scenarios, pass counts,
mutant score).

### As landed: the run, the comparison and the receipt

*(GitHub #744, issues #662 and #663.)* `run_commissioning(spec, reference, inputs, options)` in
`crates/leverage/oce-verify` runs one reference and returns a `CommissionRun`: the
`oce/receipt/v1` receipt with `kind: "commissioning"` and the body of the two tables above
(`CommissioningBody`, `OutputReport`), the trace with its inputs retained, and one `OutputCurves`
per compared output. The code is in `src/commissioning/`:

- `bind_ports.rs` and `bind_reference.rs`: binding the reference to its sequence;
- `run_bounds.rs`: the bounds checked before the replay;
- `drive.rs`: the replay;
- `prepare.rs`: the comparison instants;
- `indicator_mask.rs`: the indicators and their mask;
- `compare_output.rs`: one output's comparison;
- `report.rs` and `report_json.rs`: the receipt body and its writer;
- `run_options.rs` and `run_commissioning.rs`: the options and the entry point.

It builds on the shared receipt items the tolerance build (#684, GitHub #741) landed first:
`ComparisonMode::Tolerance`, `Comparison::tolerance(envelope)` with `mode()`, `envelope()` and
`envelope_version()`, and the codes `too-many-points` and `compare-refused`
(`COMPARE_REFUSAL_CAUSES`), whose documents now cover both kinds. It adds
`ReceiptKind::Commissioning` and `ReceiptBody::Commissioning`. No vector receipt byte moved. The
funnel port gained two accessors and no arithmetic: `FunnelResult::errors_at_test`, one error
per test sample in test order, and `half_width` and `half_height` (`HalfSize`, the min and max
over the reference points).

**Ruled by the owner on 2026-11-21**, in the review of GitHub #716, the questions of this build
and the review of GitHub #744:

1. **Quantity spelling.** A port's quantity matches by its local name, the text after its last
   `#`, `/` or `:`. So `TemperatureDifference`, `q:TemperatureDifference` and a full QUDT IRI
   are one quantity. A port with no quantity takes any row.
2. **A positive rule per quantity:**
   - a `ThermodynamicTemperature` port takes absolute rows or the identity;
   - a `TemperatureDifference` port takes difference rows or the identity;
   - any other quantity, or none, takes any row.

   A difference row on an absolute port is `UnitError::DifferenceForAbsolute`
   (`difference-for-absolute`), beside `absolute-for-difference`.
3. **`cdl.unit` must be the port's unit**, as recorded on #662 before this build:
   `UnitError::PortUnitMismatch` (`port-unit-mismatch`), the cause of `unit-refused`.
4. **A non-finite engine sample.**
   - The scan covers the engine series at the instants that are neither masked nor uncovered.
   - Every compared instant's exceedance is positive infinity, so `first_exceedance_t` is the
     first compared instant and `exceedance_time_s` spans the compared instants.
   - `worst` keeps this note's override: the earliest such instant.
5. **An indicator sample that is not Boolean** and not exactly `0` or `1` (a `0.5`, a NaN, an
   Integer `2`) is `indicator-not-boolean`, exit 2, with no receipt. The refusal names the
   indicator, the time and the value. Every sample is checked, whether or not the window
   reaches it.
6. **`sampling` is required for now.** A reference whose own `sampling` and the spec's are both
   absent is refused as `sampling-required`, exit 2, with no receipt. Without a grid, the test
   instants were only the test series' own times, and the envelope interpolated the reference
   linearly between its samples. A value the engine held between two frames was therefore
   judged as a ramp. The review reproduced two false passes: a trend step 3 s late passed under
   a 1 s `x` tolerance, and a Boolean toggling between two logged points passed. Issue #746 is
   the faithful change-of-value comparison that lifts the refusal: a step point at each change
   in the reference, and discrete outputs at the union of both series' times. Decision 14's
   "without `sampling`, both series are compared at their own times" waits for it.
7. **`stop` is the last instant when the grid does not reach it.** The instants are
   `start + k * s` for every `k >= 0` with `start + k * s <= stop`, and then `stop` itself when
   the last of those is before it ("each series gets a sample exactly at `start` and `stop`").
   Both series are held at that one set of instants; it is the `compared` row's set plus that
   last instant.

**The gap the first ruling leaves.** The vendored modelica-json translations write a quantity as
`qudt:hasQuantityKind`, which the importer does not read: 160 occurrences in the 44 G36
translations, 10 of them `TemperatureDifference`, none as `S231:quantity`. Every port of a
translation therefore has no quantity and takes any row, so the mapping's spelling decides
there. Their units are read (`qudt:hasUnit`, issue #631), so the unit check holds for them.
Issue #743 asks the importer to read `qudt:hasQuantityKind`, which closes the gap.

**Masking** keeps OBC §12.7.2's zero fill. Masked and uncovered instants are zeroed in both
series before the envelope is built. Next to a mask edge the zero widens the band, as the
owner's ruling on the tolerance windows (GitHub #741) records for vector sets; issue #745
follows it up. With both series on one grid and zeroed at the same instants, a masked instant's error is
above zero only by rounding, so `masked_exceedance` is empty in practice.

**Readings of this build**, each where the note was silent or left a choice. They are the
implementer's and the orchestrator's, listed for review:

- **Key refusals** (the note names no kind). #658's selection refusals reach the caller as
  `spec-refused`, with the spec reader's code as the cause: `unmatched-key`, `ambiguous-output`,
  `conflicting-override` or `work-limit-exceeded` ("a lower layer's word is a cause").
- **`duplicate-output`** is added, the twin of `duplicate-input`: two mapped names that resolve
  to one boundary output ("a duplicate name on either side … is a typed error before any
  frame"). A whole canonical path is an exact name, so `T` and `urn:x#top.T` are a duplicate.
- **An indicator name with the prefix that matches several signals** is `unknown-indicator`,
  with the candidates (decision 27: "an indicator name that is neither is `unknown-indicator`").
- **`cdl.unit` against the port** is exact equality of the two optional spellings on a Real
  port: none on both sides agrees, and a unit on one side only disagrees. The facade carries no
  unit for an Integer or Boolean port, and the trend reader refuses every conversion but the
  identity on one, so there is nothing to compare there.
- **A mapping's CDL type that is not the port's type** (`float` a Real, `int` an Integer, `bool`
  a Boolean) is `value-type-mismatch`, the vector bind's word.
- **No sample at or before `start`** in either series makes the output `no_data` with no
  instant: `compared`, `masked` and `uncovered` are all zero and its curve is empty. This is
  the orchestrator's call, on decision 16.
- **An uncovered stretch** is `[refused time, next executed time)`, empty when the two times are
  equal, and `[refused time, stop]` when no executed frame follows (decision 28). A trace whose
  engine accepted no frame has no engine sample at all, so every output is `no_data`.
- **The exceedance column** (`OutputCurves::errors`) has one point per instant:
  - the exceedance at a compared instant;
  - the envelope's error on the zeroed series at a masked one;
  - a NaN at an uncovered one, where the engine computed nothing.

  A NaN is also written at every instant of a Real output with nothing compared. The column is
  in memory only: the receipt never carries it, and nothing in the crate sums or compares it,
  since every figure is computed from the compared instants alone. `oce verify --out` (#664)
  decides how `errors.csv` spells it.
- **Parameter edits** are applied once, on the engine bind loads, before any frame. The header
  keeps that engine's `content_id` as loaded and the body takes `content_id` after the edits
  (decision 30); `parameters` echoes each value as the spec wrote it. A test holds an edit to
  moving the body's id and never the header's.
- **`CommissionOptions`** adds `trends` (`TrendLimits`, what `TrendSources::read` takes) and
  `max_total_aligned_grid_work` to the sketch's fields. It is `#[non_exhaustive]`, with
  `CommissionOptions::with_data`.

**The bounds of a run**, each checked before the replay and each pinned at its bound and one
past:

- **Instants per output**, at most the envelope's limit. That is `max_aligned_grid_points` for a
  Real output under `aligned-grid`, and `max_funnel_points` for every other: an exact comparison
  is linear too. The refusal is `too-many-points`, with the output's path. The count is the
  grid's plus the off-grid stop, exact, by a binary search over `k` on the arithmetic the
  instants are made with, so a huge window costs about 66 evaluations.
- **The run's work**, `instants × (1 + indicators)` summed over the compared outputs, at most
  16,777,216 (`MAX_RUN_POINTS`; `too-many-points`). The comparison is linear in the instants,
  and masking reads each of an output's indicators once per instant (the review of GitHub
  #744).
- **Distinct indicator names** per reference, at most 64 (`MAX_INDICATORS`;
  `too-many-indicators`). A rule's names are one bit each, so the name sets of the rules that
  select an output are united with one bitwise OR apiece; the indicators' values are then
  ANDed at each instant, as "Indicators" says. Names are found through a map and an `indicatorSources` point by a
  binary search, so binding is `n log n` in the names. The review measured the earlier linear
  scans at 4.18 s for 32,760 names.
- **The grid envelope's quadratic work**, `instants²` summed over the Real outputs under
  `aligned-grid`, at most 2^32 (`MAX_TOTAL_ALIGNED_GRID_WORK`, lowerable through
  `CommissionOptions`; `too-many-points`). This is the bound a vector set's tolerance windows
  are held to.
- **The trace**, `frames × bytes per row` at most the replay's `max_trace_bytes`. A row is 9
  bytes, plus each output and each retained input at its width, plus one substitution bit per
  input when one can be held: replay's formula. A run too long for its trace is refused as
  `replay-failed` with the cause `trace-too-large` before the trace is allocated.
- **The uncovered instants** are found by one walk over the instants and the stretches together,
  linear in the two (the review of GitHub #744 measured the earlier per-instant scan at 16.6 s
  for 160,000 alternating refused frames).

- **The instants strictly increase.** With a large `start` and a small `sampling`,
  `start + k * s` can round to one value for two `k`; such a reference is refused as
  `replay-failed` with the cause `grid-not-increasing`, the word a vector grid uses for two
  ticks that round to one time. An off-grid `stop` one ulp past the grid's last instant is not
  refused; issue #747 records it.

**What a run holds.** Beside the caller's own file bytes, a run holds:

- the merged trend inputs, up to 1 GiB (`TrendLimits`);
- the expected and indicator series, kept for the whole run: each source up to
  `max_source_bytes` of file, and each sample held as its time and its value (16 bytes for a
  Real), so a multiple of the file bytes: about four times for short `csv` cells;
- the trace, up to 1 GiB (`max_trace_bytes`, checked before the replay);
- the instants, 8 bytes each;
- for one output at a time, its held series, mask and curves, a few `f64` per instant under the
  per-output cap: about 50 MB at 2^20 instants;
- the indicator series, one per distinct name.

So the peak is a multiple of the expected and indicator files' bytes plus the merged inputs,
the trace and one output's series, not a fixed figure. Two measurements from the review of
GitHub #744:

- four expected `csv` sources of 4M rows × 16 Real columns (636 MB of files), with tiny inputs,
  peaked at 4.09 GB of resident memory, about 3.2 GiB beyond the file bytes;
- a run of 16M input frames peaked about 2.13 GiB beyond its `csv`.

The allocation test (`tests/commissioning/run_cost.rs`) pins the per-frame term only: 365 bytes
a frame at peak at 20,000 frames, with four outputs and five retained inputs (the review
measured about 250 at 2^20 frames). The receipt's complete lists are bounded at bind
(`parameters` 256, `sources` 256, `outputs` 4,096). Its other lists are capped by constants
(`MAX_UNIT_COLUMNS`, `MAX_RUN_WARNINGS`, `MAX_FINDINGS`, `MAX_UNVERIFIED_OUTPUTS`,
`MAX_MASKED_INTERVALS`, `MAX_MASKED_EXCEEDANCE`; decision 42).

**Codes** this build adds to `VERIFY_ERROR_CODES` (after GitHub #741's, in that order;
`too-many-points` and `compare-refused` are shared with it): `reference-out-of-range`, `reference-incomplete`, `spec-refused`, `unknown-indicator`,
`indicator-not-boolean`, `duplicate-output`, `frame-refused`, `too-many-sources`,
`too-many-outputs`, `source-refused`, `unit-refused`, `sampling-required` and
`too-many-indicators`. To `UNIT_ERROR_CODES` it adds `difference-for-absolute` and
`port-unit-mismatch`. A `frame-refused` carries its class as the cause, and
`VerifyError::frame_index`, `frame_time` and `engine_message` (cut to 256 bytes at a character
boundary).

**Evidence:**

- goldens of a `pass`, a `fail` and a `no_data` receipt and their exceedance columns
  (`tests/fixtures/commissioning/run/receipts/`);
- `derive_run.py`, run with `python3 -I`, which shares no code with the crate. It derives a
  four-output case under each envelope: the funnel's construction for a loop-free reference, and
  `aligned-grid`. The case has a 5 s grid, an off-grid stop, one frame refused for a missing
  sample and its uncovered stretch. The script also recomputes every golden's `receipt_id`;
- the funnel's figures for one output, derived by hand from "Envelope";
- the engine series, checked against a plain `prepare_frame` / `execute_frame` loop;
- one receipt for chunk sizes 1, 7 and the whole run, for every order of the files and whatever
  the timings;
- seeded byte edits and hostile numbers, none of which panics;
- the M3 gate row (decision 39): a synthetic trend from the engine, its setpoint raised by
  0.1 K, fails with zero tolerances and passes within `atoly = 0.2`. Both golden receipts are
  labelled `data: synthetic`.

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
pub const FUNNEL_VERSION: &str = "funnel@a12fa96";
pub const ALIGNED_GRID_VERSION: &str = "aligned-grid@1";
pub fn convert_unit(from: &str, to: &str, x: f64) -> Result<f64, UnitError>; // closed `units/v1`

/// The bytes a run reads, supplied by the caller: the library opens no file.
pub struct SpecInputs { pub cxf: Vec<u8>, pub files: Vec<(String, Vec<u8>)> }
pub enum TrendData { Trended, Synthetic }
pub struct CommissionOptions { pub data: TrendData,            // decision 41; default Trended
                               pub timings: Option<Timings>,   // the library measures nothing
                               pub bounds: bool,               // keep the four bound curves
                               pub chunk_frames: usize,        // frames per `Replay::advance`
                               pub replay: oce_replay::ReplayLimits,
                               pub compare: CompareLimits }    // lower-only
/// `Err` for every bind and run refusal; then no receipt exists.
pub fn run_commissioning(spec: &CommissioningSpec, reference: usize, inputs: &SpecInputs,
                         options: &CommissionOptions) -> Result<CommissionRun, VerifyError>;
pub struct CommissionRun { pub receipt: Receipt,
                           pub trace: oce_replay::Trace,       // inputs retained
                           pub curves: Vec<OutputCurves> }     // in `outputs[]` order
pub struct OutputCurves { pub output: String, pub errors: Curve,
                          pub bounds: Option<Bounds> }          // reference, test, lower, upper
pub fn funnel(reference: Series<'_>, test: Series<'_>, tol: &Tolerances,
              limits: &CompareLimits) -> Result<FunnelResult, CompareError>;
pub enum OutputStatus { Pass, Fail, NoData }
pub struct Receipt { pub header: ReceiptHeader, pub body: ReceiptBody }
#[non_exhaustive]
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

*(Amended 2026-11-20, issue #657:)* the sketch carries `CommissionOptions`, what `CommissionRun`
holds and the two envelope version constants. No record cap is an option (decision 42);
`CommissionRun` holds no file and no path, and the CLI writes its files from `curves`.

Every error is typed and names the spec field, file, row or output; nothing panics on hostile
input. Every code is kebab-case, listed in a constant a test pins, and reaches `oce vocab`. The
codes of a commissioning run come from three lists, by where the refusal is found:

| Where | Type | Codes this note adds or names |
| --- | --- | --- |
| loading the spec or the mapping | `SpecError`, from `CommissioningSpec::from_json_str`, before `run_commissioning` is called | the strict-JSON spellings the vector reader prints, `too-many-parameters` (decision 29) and, found when bind matches a reference's keys against the sequence's outputs, `unmatched-key`, `ambiguous-output` and `conflicting-override` *(the last two added 2026-11-21)* |
| bind and run | `VerifyError`, the type `run_vectors` returns, its `VerifyErrorKind` extended in place | `reference-out-of-range`, `reference-incomplete`, `unknown-indicator`, `frame-refused`, `too-many-sources`, `too-many-outputs`, `too-many-points`, `source-refused`, `unit-refused`, `compare-refused`; and the vector kinds it reuses (`subject-load-failed`, the name and `param-…` codes, `enum-port-unsupported`, `unsupported-port-type`, `run-refused`, `replay-failed`) |
| the verb | `oce-cli`'s own codes | `reference-required` (decision 35), and its existing `not-found`, `not-a-file`, `read-failed`, `input-too-large` and `write-failed` (decision 37) |

Three points about them:

- **A lower layer's word is a cause.** `source-refused` carries a `SourceError` code,
  `unit-refused` a `UnitError` code and `compare-refused` a `CompareError` code, as
  `replay-failed` carries its cause.
- **`frame-refused` is one word in two lists.** In a vector receipt it is the `code` of a
  failure record: the frame's refusal is a verdict (exit 1). Here it is an error kind: the
  refusal ends the run (exit 2). The two never meet in one document, a receipt or a report,
  and both name the same event, an engine frame-class refusal, as `window-uncovered` already
  names one refusal in two lists.
- **`too-many-points` is the verifier's, at bind.** `CompareError` has a code of the same
  spelling for the same limit. Bind applies that limit first (decision 42), so a comparison
  never returns it and `compare-refused` never carries it.

`ComparisonMode::Tolerance`, the envelope fields of `Comparison`, `compare-refused` and
`too-many-points` are shared with the tolerance windows of a vector set
([vectors note](vectors.md), decisions 42 and 48). Whichever of #663 and #684 merges first
adds each; the second finds it in place. `FUNNEL_VERSION` and `ALIGNED_GRID_VERSION` both come
from the funnel port (#655), beside the envelopes they name.

## CLI

```text
oce verify --commission <SPEC> [--reference <N>] [--out <DIR>] [--bounds]
           [--data trended|synthetic] [--deny-warnings] [--timings]
```

A form of the `oce verify` verb, in the `oce` binary of the
[authoring layer](authoring-layer.md#cli). One spec and one reference per run (decision 35);
there is no batch mode. `--commission` excludes the positional `<VECTORS>`, `--subject`,
`--subject-form` and `--require-mutants-caught`; giving one with it is a usage error. The human
rendering is the global `--render` (decision 36); the verb has no `--format`.

**Reads.** The spec; the mapping when the spec names a file; the CXF, bounded at
`oce_api::MAX_CXF_BYTES`; every trend source. The library opens no file (decision 25): the CLI
resolves each path against the spec's directory, which it may not leave, and checks each size
from metadata before a byte is read. The bound on the sources together is set with the verb
(#664).

**Output.** Stdout carries one document: the receipt when a verdict exists (exit 0 or 1), the
`oce-report/1` report otherwise (exit 2, the refusal in `errors[]`). With `--out DIR`, stdout is
the report, with `passed`, `status`, `receipt_id` and `output`, as `oce verify --out` prints it,
and these files are written (decision 37):

- `DIR/receipt.json`;
- `DIR/outputs/<k>/errors.csv` for every compared output, `<k>` its zero-based index in the
  receipt's `outputs[]`, written without padding: the exceedance at each test instant;
- under `--bounds`, for every Real output, `reference.csv`, `test.csv`, `lowerBound.csv` and
  `upperBound.csv` in the same directory.

The five names are pyfunnel's, and every number goes through the crate's `%.16g` writer. An
index needs no escaping rule, cannot collide, and holds no character a file system refuses; the
receipt is the index. `--bounds` without `--out` is a usage error. No trace file is written:
the trace's canonical bytes are not a published format. `DIR` must exist; `outputs/` and its
subdirectories are created, and a `DIR/outputs` that already exists is `write-failed`, so the
curves of two runs are never mixed. Each file is written atomically, the curves first and
`receipt.json` last, so a `receipt.json` written by a run means that run's curves are complete.
On a refusal before the first write nothing is written; a write that fails part-way is
`write-failed`, names the file, and leaves the files already written.

Exit codes follow the `oce` convention (decision 21):

| Exit | When |
| --- | --- |
| 0 | every compared output passed; under `--deny-warnings`, no load or run-time warning |
| 1 | an output failed, or the run is `no_data` (the receipt says which); under `--deny-warnings`, a warning |
| 2 | usage; a refused spec, mapping or unit; an unreadable or refused source; a CXF that does not load; an unresolved name; a refused parameter; a spec with several references and no `--reference`; an engine refusal; a comparison the limits refuse; an I/O failure |

`--deny-warnings` counts `load_warnings_total` and `warnings_total` and changes the exit code
only. `--timings` fills the header's `timings` as it does for a vector run: `load_ms` is reading
every file, `run_ms` the whole `run_commissioning` call, `compare_ms` zero. No receipt byte
outside `timings` depends on either flag. `--data` sets the body's `data` label (decision 41).
`oce vocab` lists every code the verb can print.

### As landed: the verb

*(GitHub #757, issue #664.)* `oce verify --commission` is in `crates/leverage/oce-cli`
(`src/commands/verify_commission.rs`, `commission_files.rs`, `commission_out.rs`, and
`contained.rs`, the path rule it now shares with the mutant reader). The receipt on stdout is
the library's, byte for byte: the tests hold the binary's stdout to `oce-verify`'s golden
`pass`, `fail` and `no_data` receipts. The Building 33 worked example and the Building 33 day of
the funnel differential need #661's vendored trends and moved to issue #755.

**The orchestrator's calls on 2026-11-21** (not owner rulings), on four places this note left
open:

1. **Scope.** The worked example moved to #755 (blocked on #661), and #664 closes with the verb.
2. **The bound on the files together** is the library's own: `TrendLimits::MAX.max_total_bytes`,
   1 GiB, over the mapping file and the trend sources, each path once, the CXF not counted. It is
   decided from each opened file's metadata before its bytes are read, and refused under the
   library's own word, `source-refused` with the cause `too-many-total-bytes`, at the spec field
   of the file that passes it. One file is bounded the same way: the CXF at `MAX_CXF_BYTES`, the
   mapping at `SpecLimits::MAX.max_bytes` (4 MiB), a trend source at `max_source_bytes` (1 GiB),
   each `input-too-large`. The Building 33 files, 11.6 MB, fit. Peak memory is a few times the
   bytes of the expected and indicator files, beside the merged inputs and the trace ("What a run
   holds", above).
3. **A path that resolves outside the spec's directory** is `file-outside-directory`. The issue
   asked for "a code named for a source"; the rule covers the CXF and the mapping file as well,
   so the code names what all three are.
4. **`--bounds` writes the four bound files only where an envelope was built.** "Output" above
   says "for every Real output"; a Real output with no compared instant, or one failed by a
   non-finite engine sample, has no envelope (`half_width` is absent from its receipt row), so no
   bound curve exists to write. `errors.csv` is written for every compared output. Because a
   missing file must never be read as anything, a pass least of all, the report on stdout lists
   under `curves` every compared output with its `index`, its canonical path and the `files`
   written for it.

**The orchestrator's calls in the review of GitHub #757** (not owner rulings):

5. **Nothing may already be at `DIR/receipt.json`.** "Output" above says a `DIR/outputs` that
   exists is `write-failed`, and says nothing of `receipt.json`. As first built, an existing
   `receipt.json` was replaced, and the atomic writer followed a symbolic link there: a link to
   a file outside `DIR` was overwritten, and a stale receipt could stand beside a partial
   `outputs/`. Any entry at `DIR/receipt.json` that is not one of the run's inputs (those stay
   `output-is-input`), a file, a directory or a link, dangling or not, is now `write-failed`,
   before the run; and every file is written without following a link at its name, so a link
   planted after the check is replaced and its target untouched. A `receipt.json` beside the
   curves is therefore always the run's own, as "Output" promises.
6. **A failed write lists what was written.** The report of a write that fails part-way carries
   `curves`, the files written before it, so a reader need not walk the directory.
7. **The last component is checked without following a link.** The file reader checks the
   canonical path's own entry (`symlink_metadata`); a canonical path holds no link, so one found
   there is a path that changed under the check (`read-failed`). The residual, an intermediate
   directory swapped for a link, needs a component-by-component no-follow open std does not
   offer; it is issue #758, beside the vector `--out`, which still writes through a link at its
   `receipt.json`.
8. **Two usage lines.** `oce verify --help` and every usage error print the vector form and the
   commissioning form as two lines, so clap never prints the combination it refuses.

**Readings of this build**, from the note:

- **`errors.csv`** is `Curve::to_csv`, pyfunnel's shape: the line `x,y`, then each point with
  both numbers through the `%.16g` writer. So an uncovered instant is `nan` and each compared
  instant of an output failed by a non-finite engine sample is `inf`, C's spellings; the library
  writes the constant `f64::NAN`, so never `-nan`. A test pins both spellings and holds every
  `errors.csv` of the run fixture to the library's golden column and, for the oracle case, to
  `derive_run.py`'s independent derivation.
- **Order of the refusals**: usage; the spec (4 MiB from metadata, then the spec reader); the
  reference (`reference-required`, decision 35); the reference's own completeness, decided by
  the library before any other file is read (`reference-out-of-range`, `reference-incomplete`,
  `sampling-required`); the files, in the receipt's order of sources after the CXF and the
  mapping; with `--out`, the output checks; the run; the writes. The output checks come after
  the reads and before the run: `DIR` must be an existing directory and `DIR/outputs` must not
  exist (`write-failed`), and a `DIR/receipt.json` that is the spec or a file the run read, by
  path, link or (on Unix) hard link, is `output-is-input`, so a refusal writes nothing and an
  existing `outputs/` is refused before a run is spent. `outputs/` is then made with a plain
  create, which also refuses one a racing writer made.
- **Where paths resolve**: the directory of `SPEC` as given, so a spec reached through a
  symbolic link reads the files beside the link, not beside its target. A file that grows after
  its metadata is read is read up to its remaining room and one byte more, then refused.
- **Which files** the CLI reads is the library's list (`TrendSources::read`): the mapping when a
  path, then the trend sources in the receipt's order, the `controllerOutput` only when the
  reference has no `expected` key at all, a path named twice read once. A list that drifted
  would be refused by the library as `file-missing` or `file-unlisted`, never run.
- **Flags.** `--reference`, `--bounds` and `--data` without `--commission` are usage errors
  (clap drops a `requires` whose argument conflicts with one present, so each also conflicts
  with the vector flags). `--data` absent is `trended`.
- **Names.** No name reaches a file name or a CSV cell: files are named by index and hold
  numbers under a fixed header. A name reaches the report only inside a detail (a refusal), as
  JSON, and the stderr rendering writes every format, control and separator character as an
  escape. The receipt's names are the library's JSON, which writes a format character as
  `\uXXXX`.
- **Vocabulary.** `oce vocab` gains the spec reader's codes (`spec_codes`), and as causes the
  source reader's (`source_codes`), the unit table's (`unit_codes`) and the engine refusal
  classes of `frame-refused` (`frame_refusal_codes`), each pinned to the library's list; the
  tool's own `reference-required` and `file-outside-directory`.
- **What a source refusal carries.** A `source-refused` error's cause is the source reader's
  code (`cell-refused`, `mapping-refused`, `unit-refused`); that reader's own cause (the cell
  rule's `not-boolean`, the mapping's spec code, the unit code) is not carried to the report,
  because `VerifyError` holds one cause, and the detail names the place and the reason instead.

**Open, and not resolved here:** the change-of-value comparison that lifts `sampling-required`
(#746), the library's follow-ups from the review of GitHub #744, among them the 1-ulp last
instant (#747), and the zero fill of masked ticks (#745).

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

### As landed: the Building 33 material

*(GitHub #761, issue #661.)* The eight files of the trend directory, the sequence's `.mo` and its
translation are under `third_party/modelica-buildings-cdl/`, verbatim at the pins the tree
already records; neither pin moved. Each upstream file's Git blob OID equals upstream's at
`a131864` (`.agents/revendor-upstream.sh`, whose fidelity leg now reads the two new upstream
cones), and `scripts/vendored_cxf/regenerate.sh` regenerates the translation, and the 44 G36
documents with it, byte for byte inside a Node container pinned by digest. What the material
showed:

- **The translation does not load.** modelica-json at `85721b82` writes the string `undefined`
  as the value of every parameter whose binding holds a parenthesised sub-expression:
  `TOutCooCut`, `TOutDelta`, `TSupHighLim`, `TSupHigLim` and `Ti`. "Building 33 data" above
  says `k = 0.018` and `Ti = 30` s "are derived inside the CXF"; `k` is written as
  `alc_prop_k*alc_k_unit_conv`, but `Ti` is not written at all. The engine refuses the document
  with ten `grounding-failed` errors, those five and the five bindings that read them, pinned
  by `crates/oce-cxf/tests/vendored_corpus_delta.rs`. The file is kept as generated (issue
  #760). modelica-json's `master` fixes the five values, but its output is refused for a unit
  IRI the closed table does not map, so a modelica-json pin advance would not by itself make the
  document load.
- **The trend files end their lines with CRLF** upstream. The repository normalises text to LF,
  so the root `.gitattributes` exempts the trend directory; the hash manifest and the blob OIDs
  pin the bytes with their carriage returns.
- **Row counts.** `Clg_Coil_Valve.mos`, `OA_Temp.mos` and `SA_Clg_Stpt.mos` hold 75,564 rows,
  `Supply_Air_Temp.mos` 75,563 and `VFD_Fan_Feedback.mos` 75,578; the receipt sketch's 75,563
  for the valve and the setpoint was not a prediction. The first day, `[0, 86400]`, holds 17,281
  rows of each 5 s trend and two of the fan enable (at 0 s and 0.02 s).
- **No temperature-difference port.** `CoolingCoilValve`'s ports are `TSup`, `TSupSet` and
  `TOut` (`ThermodynamicTemperature`, `K`), `uFanFee` (no unit, no quantity), `uFanSta`
  (Boolean) and `yCooVal` (`1`), so the quantity gap of #743 changes nothing here. Its parameter
  `TOutDelta` is a difference written as an absolute temperature upstream (2 °F converted with
  the offset, 256.48 K), which makes the hysteresis's `uLow` about 26.67 K.
- **`uFanFee` declares no unit.** The upstream example model scales the fan feedback trend by
  0.01 before it reaches the port. Under "`cdl.unit` must be the port's unit" a mapping cannot
  write `percent` → `1` for it, and a one-sided unit is refused, so the worked spec's sketch
  (`VFD Fan Feedback`, `percent` to `1`) cannot bind as written.
- **The worked spec's `cxf` path** names a `.json` file; the vendored translation, like the
  other 44, is `.jsonld`.

**Ruled by the owner on 2026-11-21**, on what the refusal means for the worked example:

1. **#661 lands alone**, the material verbatim and whole (decision 26 stands; about 11.68 MB
   added is accepted), the refusal pinned as evidence so a later pin advance shows when the
   document starts to load. The cause is issue #760 (M3 or later, P2): modelica-json's `master`
   fixes the five values but writes `http://qudt.org/vocab/unit/PER-K`, which the closed unit
   table does not map, and moves the QUDT namespaces, and advancing that pin regenerates the 44
   G36 translations, CXF-Core and `oce-vocab`'s generated vocabulary.
2. **The worked example (#755) runs in its own pull request from an in-repo `CoolingCoilValve`**
   authored from the vendored `.mo` (the text form, then `oce emit`), labelled "authored from
   the vendored model, not vendored". Its verdict is about that transcription, not upstream's
   translation.
3. **`uFanFee` is mapped with no unit on either side**, and the example says so: the raw percent
   reaches the port. On this data (60.9 to 72.5 % all trend long) the 0.05/0.15 hysteresis is
   true either way. No binder change.

The finding above that `CoolingCoilValve` has no `TemperatureDifference` port confirms "As
landed: the run, the comparison and the receipt": the quantity gap of #743 does not reach this
example.

### As landed: the worked example

*(GitHub #765, issue #755.)* The worked example runs, and its first verdict is **`fail`**. The
fixture is `crates/leverage/oce-verify/tests/fixtures/commissioning/building33/`, with a README
that maps every line of the `.mo` to what was authored:

- **`cooling_coil_valve.oce`, authored from the vendored model, not vendored**, and its CXF from
  `oce emit` (`cooling_coil_valve.cxf.json`, held to the emitter's output byte for byte). The
  verdict is about this transcription, not about upstream's translation. The text form takes no
  expression, so each binding the `.mo` writes as an expression is the f64 value it evaluates to
  in the `.mo`'s order (`k` 0.018000000000000002, `Ti` 30.0, `TOutThr.uLow`
  26.666666666666686, `uFanFeeThr.uLow` 0.04999999999999999); the parameter bounds and units the
  text form cannot carry, `protected`, and every annotation are dropped, each listed.
- **The spec** is "Worked spec" above with one change, the `cxf` path; `spec.json` is the run's
  and `building_33.json` beside the other specs stays the block above. The mapping
  (`b33-points.json`) gives `uFanFee` no unit on either side, by the owner's ruling.

**The verdict, recorded on the first run and not tuned:** `yCooVal` `fail`, 17,281 instants
compared, none masked or uncovered, worst exceedance 0.020864709745257892 at 65,965 s, first at
61,770 s, 4,085 s outside the band (817 instants, 61,770 to 67,755 s); the band 1 s by 0.01
throughout. The receipt is the golden `receipts/day.receipt.json`; `oce verify --commission
spec.json --data trended` prints it byte for byte and exits 1.

**Adjudication.** The failure is the recorded controller's, not the transcription's:

1. the transcription is checked from outside it four ways: its parameters, connectors, blocks
   and 21 connections against what `derive_declarations.py` reads from the `.mo`; its valve
   signal bit for bit against `derive_valve.py`'s derivation from the `.mo` and the CDL block
   sources over 64 frames that take every branch; ten mutants, every one caught by that
   derivation; and OpenModelica;
2. OpenModelica 1.25.1 (the digest-pinned image of the other OpenModelica evidence) simulates
   upstream's own `ControlsVerification_CoolingCoilValve` over the day, and the engine stays
   within 0.0034 of it at every instant (largest at 67,375 s);
3. that OpenModelica signal, put through the same funnel against the same trend, leaves the band
   over the same stretch, 61,770 to 67,755 s (834 instants, worst 0.0234 at 67,335 s);
4. OBC §12.6's own funnel figure shows its error non-zero only at about 62,000 to 68,000 s of the
   first day, at up to about 0.02, though its text says only "good agreement". §12.6 names the
   ALC controller's discrete time step and proprietary anti-windup as the residuals.

**Readings of this build:**

- **The frames are the trend's, not the window's.** The replay runs all 75,583 frames the trend
  files hold, to 493,800 s, and compares the window's instants: `frames` is `executed` 2,
  `substituted` 75,566, `refused` 15. The note's sketch showed one day's frames. The 15 refused
  frames are at 485,400 to 493,800 s, after the supply-air trend ends (484,800 s) and its 10 s
  hold has run out, while the fan feedback goes on; all outside the window. `substituted` is nearly every frame because the fan enable logs one sample
  a day and its 90,000 s hold fills every other frame.
- **`TOutDelta`** is transcribed as upstream writes it. On this day it changes nothing: the
  outdoor air starts at 50.39 degF and never falls below 48.89 degF, so the cut-off is on all day
  under upstream's 26.67 K threshold and under a corrected 282.04 K one alike; a test runs the day
  with the corrected threshold and finds every figure the same.
- **What the day cannot see.** The enable is on all day and the supply-air high limit never cuts
  the signal (the supply air is below 50 degF at 56 instants, where the limit is about 0.99 and
  the valve at most 0.91), so six of the ten mutants leave the day's receipt unchanged; the
  derivation's synthetic frames are the evidence for those parts.
- **The Building 33 day of the funnel differential** is `tests/fixtures/funnel/recorded/
  building_33_day/`, a set of its own beside `upstream/` and `hostile/`: its inputs are the run's
  two series, written to read back to the same bits, and the run's five curves print as the
  pinned pyfunnel's byte for byte (817 instants outside, as the receipt says). The capture
  re-ran for it and rewrote the other 66 cases' files unchanged.
- **Cost.** One run of the day takes about 0.8 s and 36 MB in a debug build and 0.1 s in a
  release build, so the tests run in the leverage lane with no slow-test override.

## Decisions

Decisions 1 to 4 are the owner's; 5 to 10 are recommendations accepted by the orchestrator. Both are
settled. Decisions 11 to 26 were ruled on by the architect pass; 17 and 21 are as it replaced them.
Decisions 27, 28, 38, 39, 40 and 41 were ruled by the owner on 2026-11-20. Decisions 29 to 37 and
42 follow from standing rulings or from `oce verify` as built: the owner was shown the list and
raised nothing, so they stand as written ([Run readings](#run-readings)).

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
12.1 would need editing, against decision 3). *(2026-11-20: decision 27 says what the whole
string is: the reference's `sequence` and a `.`, literal, then the last segment of the output's
canonical path. 2026-11-21, owner's ruling: an exact key beats a glob, two keys of one kind
that disagree about one output are `conflicting-override`, and no order of the file's keys
matters; see [Review rulings](#review-rulings).)*

**18. Scenario 1 fields parse and are ignored with a finding;** `controllerOutput` is the
expected source when `expected` is absent. Rejected: refusing them (LBL specs would not load);
acting on them (the verifier drives no controller). *(2026-11-21, owner's ruling: beside an
`expected`, `controllerOutput` is not read and is a finding of the same code.)*

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
frame, echoed in the receipt, never converted. *(2026-11-20: decision 29 gives the order of the
edits, and decision 30 the content id the receipt carries.)*

**25. the library opens no file;** it takes bytes and the CLI resolves paths relative to
the spec, as replay does.

**26. vendor Building 33 verbatim and whole** (about 11.6 MB beside the 3.46 MB tree).
The implementing PR follows the vendored README's pin-advance process although the pin does not
move, and brings the `CoolingCoilValve` `.mo` and its CXF at the recorded modelica-json pin.
Rejected: a one-day slice (not verbatim, outside the manifest's fidelity check).

**27. Names: one resolver, a prefix on the spec's keys, none on mapping names.**

- **A mapping's `cdl.name`** is written as Listing 12.2 writes it, with no prefix, and resolves
  by `oce_replay::resolve_path`: against the inputs for a point an `inputs` source reads,
  against the outputs for a point an `expected` source reads. Unknown and ambiguous names are
  `unknown-input`, `ambiguous-input`, `unknown-output` and `ambiguous-output`, the vector bind
  codes. A point an `indicatorSources` source reads is not resolved against the engine: its
  `cdl.name` is the indicator's name.
- **An `outputs` or `indicators` key** must start with the reference's `sequence` and a `.`,
  byte for byte. For an exact key the rest resolves by `resolve_path` against the outputs. For
  a glob the prefix is literal and the rest is matched, whole, against the last `.`-segment of
  each output's canonical path (`yCooVal` for `…CoolingCoilValve.yCooVal`). The output names an
  `UnmatchedKey` message lists are `<sequence>.<last segment>`.
- **An indicator name** that starts with `<sequence>.` is a signal of the sequence: the rest
  resolves by `resolve_path` against the inputs and outputs together. Any other indicator name
  must equal, byte for byte, the `cdl.name` of a point an `indicatorSources` source of the
  reference reads; Listing 12.1's `fanSta.y` is such a name. An indicator name that is neither
  is `unknown-indicator`.
- **A `parameters` key** is an instance path with no prefix and resolves by `resolve_path`
  against `Engine::params()`, with the vector codes `param-path-unknown`,
  `param-path-ambiguous` and `param-path-duplicate`.
- The root block's name is never compared with `sequence`: LBL's `sequence` names an instance
  in a model the engine does not load. Every refusal here is at bind, before any frame.

Rejected: the prefix on every name, mapping names included (it refuses Listing 12.2 as written
and cannot serve one mapping file to two instances of a sequence, which Listing 12.1 does);
every name as written (no engine path ends in `.cooValSta.yCooVal`). Why: one resolver wherever
one name selects one signal, and a stated set of names for a glob to match.

**28. A missing sample is recorded and the run continues; an engine refusal is an error.**

- **A missing sample** (no hold, or a hold older than `maxAge`) is a property of the trend log.
  The run uses `OnRefusal::RecordAndContinue`: the frame is recorded as refused, counted in
  `frames.refused`, and the run continues.
- **Uncovered instants.** From a refused frame's time up to the next executed frame the engine
  has no sample. A comparison instant in that stretch is **uncovered**: it is zeroed in both
  series as a masked instant is, counted in the output's `uncovered`, and excluded from
  `compared` and from the verdict. A frame the engine accepted with a held input (the trace's
  `Substituted` status) is an executed frame and ends the stretch; when no executed frame
  follows, the stretch runs to the window's `stop`, inclusive. A zero-order hold across the gap
  would compare the trend with an output the engine never computed for those times. An output
  whose every instant is masked or uncovered has `compared == 0` and is `no_data`.
- **An engine refusal of a submitted frame** is an error with no receipt (exit 2). A run-class
  refusal is `run-refused`, as in a vector run. A frame-class refusal (a value outside an
  input's declared domain, a non-finite value, a time the engine cannot take) is
  `frame-refused`. On a trend it most often means a wrong unit or mapping, and from that frame
  on the engine has not run what the controller ran, so no verdict on the frames after it would
  be evidence.
- **How a refusal is found, and what the error carries.** After each `Replay::advance` chunk
  the verifier reads the refusal rows added to `Replay::trace()`. A row whose cause is
  `RefusalCause::MissingInput` is a missing sample (the frame's status is `RefusedMissing`),
  and the run goes on. The first row whose cause is `RefusalCause::Engine` (status
  `RefusedEngine`) ends the run. Replay gives that cause as a class and the engine's message,
  with no slot and no path (`crates/leverage/oce-replay/src/trace.rs:228`), so the error
  carries exactly: its cause, the kebab-case `EngineRefusal` class (`oce-verify`'s
  `ENGINE_REFUSAL_CLASSES`); and in its detail the row's frame index, that frame's time in
  model seconds, and the engine's message, cut to 256 bytes at a character boundary. For the
  three input classes the engine's own message names the input
  (`crates/oce-api/src/frame_slots.rs:342`); the verifier does not parse it, and the error has
  no `path`.

Rejected: every refusal an error (a trend with a gap past its hold would give no receipt); every
refusal recorded; an engine frame-class refusal failing the verdict with exit 1, as it fails a
vector scenario (a setup error would read as a finding about the site).

**29. Parameter edits go through the vector passes.** The reference's edits, resolved by
decision 27 and sorted by canonical path, are applied by the code a vector run uses
(`oce-verify`'s `parameters::apply`): inside one halt, in passes, each pass trying the edits
still waiting in path order ([vectors note](vectors.md), decision 14). They are tried once at
bind, where `param-refused` and its class (`param-range`, `param-type`, `param-structural`) are
found. At most 256 parameters per reference, which bounds the passes; one more is
`too-many-parameters`. Rejected: the declaration order of the spec's `parameters` object (the
canonical spec sorts keys, so it would run differently from the file it was written from); one
pass in path order (it refuses lowering both hysteresis thresholds). *(2026-11-21: the owner's
ruling on override precedence makes the same true of `outputs` and `indicators`: the canonical
spec sorts every key, and no rule reads the order of a file's keys.)*

**30. The header carries the document's content id; the body carries the edited one.** The
header's `content_id` is the CXF's as loaded, before any edit. When `parameters` is not empty
the body carries `content_id`, the id after the edits: the id of what ran. A vector scenario
records its id the same way. Rejected: the edited id in the header (a registry keys receipts on
the document).

**31. The commissioning body is the field table.** [Report and receipt](#report-and-receipt) is
normative, as the vector body's table is (vectors decision 22): every field, its type, when it
is present, its order and its bound. Its goldens and the library pin that table. A pair is a
closed object (`"window": { "start", "stop" }`, `"half_width": { "min", "max" }`), never a
two-element array, for the reason the vectors note's decision 4 gives against `[[t, v]]` pairs.

**32. `comparison` names the envelope and its version for both envelopes, and the verdict is taken
at the test instants.** The header's `comparison` is `{ "mode": "tolerance", "envelope": …,
"envelope_version": … }`, all three keys present whenever the mode is `tolerance`; [Report and
receipt](#report-and-receipt) has the two versions. `aligned-grid@1` is a revision this project
defines, so it stays at 1 until 1.0.0 (standing rule 9): a change to that arithmetic is made in
place, and the engine version in the same header tells the two apart (standing rule 10).
`funnel@a12fa96` moves only when the port is proven against another `lbl-srg/funnel` commit. Both
strings are constants of the funnel port (#655), `FUNNEL_VERSION` and `ALIGNED_GRID_VERSION`;
neither #663 nor #684 defines one. A commissioning receipt always says `tolerance`, whatever the
types of its outputs. Under either envelope an instant's exceedance is "Envelope" step 5's error
at that test instant, from the envelope's two bound curves. `oce-conformance`'s `compare` also
checks the bounds' breakpoints between test samples, and the `aligned-grid` copy lists those
errors beside the test's; an `oce-verify` verdict reads the errors at the test's own instants and
no other, because an exceedance at a time that is no test instant would have no row in the
exceedance column, no place in `compared`, and no counterpart under `funnel`. Rejected: no
`envelope_version` under `aligned-grid` (a header whose shape depends on the envelope); the engine
version repeated there.

**33. Warnings are listed beside the findings, not inside them.** The body carries
`load_warnings` and `load_warnings_total`, the vector body's two fields with the same content,
order and cap, and `warnings` and `warnings_total` for the run-time warnings of the trace, each
`{ "t", "block", "message" }` in row then emission order, at most 16. `findings` holds what the
spec loader reports about the spec. With both lists present, `--deny-warnings` means for
`--commission` what it means for a vector run. Rejected: folding load warnings into `findings`
(a `DiagCode` is the engine's word about the CXF, and one reader of both receipt kinds should
find it under one key); dropping run-time warnings (a sequence's own `Assert` firing on a
site's data is evidence).

**34. An Enum boundary port is refused until #465.** A sequence with an Enum boundary input or
output is `enum-port-unsupported` at bind, and one with a String port is
`unsupported-port-type`, as `run_vectors` refuses them. Replay records no durable class for an
Enum column until #465, and the mapping's types (`float`, `int`, `bool`) have no Enum either.
Rejected: comparing ordinals (an ordinal with no recorded class is a number whose meaning the
receipt cannot state).

**35. One reference per invocation.** `--reference <N>` is the zero-based index into
`references[]`. A spec with one reference needs no flag. A spec with several and no flag is the
CLI's `reference-required` (exit 2), its detail listing each index with its `sequence` and
`model`; an index past the end is `oce-verify`'s `reference-out-of-range`. A caller that wants
every reference loops, as the library loops over its bundles. Rejected: every reference in one
run, as an array of receipts or one file each (stdout carries one document and one exit code;
the vectors note's decision 34 has no batch mode).

**36. `--render`, not `--format`.** The global `--render` flag of `oce` already answers "human
or JSON" for every verb. Stdout is the receipt (exit 0 or 1) or the report (exit 2), the same
bytes whatever `--render` says; the rendering on stderr is the report's.

**37. `--out` writes the receipt and the curves, and no trace.** As [CLI](#cli) states:
`receipt.json`, `outputs/<k>/errors.csv` for every compared output and, under `--bounds`, four
more pyfunnel-named files for every Real output, `<k>` the output's index in the receipt's
`outputs[]`. Rejected: a trace file (the trace's canonical bytes are not a published format);
the canonical path as a file name, escaped (it holds `#`, `:` and `/`, and an escaping rule is
one more contract); the path's last segment (two outputs can share one).

**38. The workspace version** *(ruled by the owner, 2026-11-20, on issues #656, #657, #663 and
#671).* The first PR that changes the bytes or meaning of `oce/vectors/v1` or `oce/receipt/v1`,
the tolerance-vector build under #656 or the commissioning receipt in #663, whichever merges
first, bumps the workspace on `development` from 0.5.0 to 0.6.0 and re-blesses every
version-bearing golden. For an additive change, the "previous layout is refused" test of
standing rule 10 is: an artefact carrying engine version 0.5.0 is refused by 0.6.0 with
`EngineVersionMismatch`. The second of the two PRs, and the closeout (#671), bump nothing.
`oce/receipt/v1` stays the one tag for both kinds (vectors decision 37): `kind: "commissioning"`
and `mode: "tolerance"` arrive in place, with no second receipt format, importer or dual-read
path. *(2026-11-21: the bump has landed, ahead of all of these, with
issue #683. An Integer bound to a Real parameter is held as a Real from 0.6.0 on, which moves
content ids and blob bytes; `crates/oce-api/tests/engine_0_5_0_artefacts.rs` is the test that a
0.5.0 blob and a 0.5.0 snapshot are refused. So none of the PRs named here bumps.)*

**39. The M3 gate row "Tolerance mode checked"** *(ruled by the owner, 2026-11-20, on issue
#663).* The four OpenModelica exact-bit cases are the existing `oce-conformance` differentials.
The perturbed-trace test runs through `oce-verify` in #663, on a synthetic trend labelled
`data: synthetic`: zero tolerances give `fail`, the stated band gives `pass`. No OpenModelica
plumbing enters a leverage crate.

**40. The compared outputs are those an `expected` source supplies; the rest are listed.**
`outputs[]` has one entry per boundary output that an `expected` source supplies through the
mapping. Every other boundary output is listed by canonical path in `unverified_outputs` and
changes no verdict: §12.7.2 compares the outputs the controller reports. Rejected: every
boundary output, one with no expected series being `no_data` (exit 1 would be the answer for
every site that trends fewer points than the sequence has); the rest unmentioned (a short trend
would shrink the test unseen, which decision 16 rejects for a window); an error at bind.

**41. The `data` label comes from the caller.** `CommissionOptions::data`, default `trended`,
and `--data trended|synthetic` on the verb. The label is a claim the caller makes about its
files, and it is inside the hashed part of the receipt. Rejected: a per-reference `data` key in
the spec (it reopens the spec's shape, #658); no label (decision 4 requires it).

**42. A list of evidence is complete; a list of observations is capped, and no cap is a
caller's option.**

- **Complete lists** carry what the receipt is evidence of, and are bounded at bind:
  `parameters` (256), `sources` (256; one more is `too-many-sources`) and `outputs` (4,096; one
  more is `too-many-outputs`). A source list cut short would leave digests out of `receipt_id`.
- **Capped lists** echo or observe, each with a `_total` beside it: `units.columns` (256),
  `load_warnings` (64), `warnings` (16), `findings` (64), `unverified_outputs` (64), and per
  output `masked_intervals` (64) and `masked_exceedance` (16).
- **Every cap is a constant of the receipt,** as `MAX_LOAD_WARNINGS` is, never a field a caller
  can lower: one run has one receipt. The vector receipt's two record caps are caller options,
  and #563 records the consequence: equal inputs, different caps, different `receipt_id`, with
  nothing in the receipt to say why.
- **Total work.** Decision 22 bounds one output's points. The run is also held to 16,777,216
  comparison points over all its outputs, refused at bind from the window and the sampling as
  `too-many-points`; without `sampling` the count is known only after the sources are read, and
  is checked then, before the replay runs.

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
- **Implementation issues.** The funnel port and its differential fixtures are #655. The
  verifier is seven parts: the spec and mapping readers (#658), the unit table (#659), the trend
  sources (#660), the Building 33 vendoring (#661), bind and run (#662), the comparison, report
  and receipt (#663), and the verb with the worked example (#664; the worked example moved to
#755). Goldens use `oce-bless` as a
  dev-dependency (authoring note, decision 19).

## Test plan

- **Funnel port.** The differential cases, from the committed capture script and pins (fixtures
  regenerated only by an issue that says so); bit-exact goldens of the port's own curves; each
  tolerance kind alone; zero half-sizes taking the fallbacks; vertical steps; leading duplicates;
  loop removal at a sharp peak; mismatched endpoints and non-finite input refused with variants.
- **Spec.** Listing 12.1 verbatim loads, and so does a copy with `//` and `/* */` comments added;
  load → write → load is byte-identical;
  duplicate keys, trailing commas, unknown fields, `NaN` and over-limit input refuse; exact keys,
  globs with `*` and `?` anchored at both ends, a key matching nothing refused with
  `UnmatchedKey` and its message, `u` not matching `uFan`, Listing 12.1's keys loading unchanged;
  override precedence: an exact key over a glob, two keys of one kind combining, a disagreement
  refused as `conflicting-override`, and the same answer for every order of the file's keys.
- **Units.** Every pair golden-tested bit for bit at `-0.0`, extremes and known points (`32 degF`
  is exactly `273.15 K`); unknown and one-sided pairs refuse; `0.5`, `2` and `NaN` in a Boolean
  column refuse.
- **Sources, time, indicators.** Holds at `maxAge` exactly and one ulp beyond; equal timestamps as
  two frames; a negative origin offset; window endpoints; zeroing, mask edges, ANDed indicators, an
  all-masked output `no_data`, masked exceedance excluded.
- **Names.** A Listing 12.2 mapping resolving with no prefix; one mapping file serving two
  references; an exact key, a glob over last segments, a key without the prefix, an ambiguous
  rest; an indicator from a file named as Listing 12.1 names `fanSta.y`; `unknown-indicator`.
- **Refused frames.** A hold one ulp past `maxAge` giving `frames.refused == 1` and the uncovered
  instants; a stretch ended by a substituted frame, and one that runs to `stop`; a `frame-refused`
  error carrying its class, frame index, time and the engine's message cut at 256 bytes; every
  frame of a window refused giving `no_data`; an input outside its declared domain giving
  `frame-refused` and no receipt.
- **Parameters.** Raising and lowering both hysteresis thresholds; an edit no order admits; the
  body's `content_id` present only with edits.
- **Report and receipt.** Golden receipts for pass, fail and `no_data`; each capped list at its
  cap and one past with its `_total`; each complete list at its bound and one past, refused; a
  NaN engine sample and one above 2^1000; a mask edge between the two series' sample times
  giving a `masked_exceedance` entry, and none under `sampling`; a breakpoint error under
  `aligned-grid` that no test instant shares changing no verdict; two identical series failing
  by rounding in the measured case; a reference with no `window` refused as
  `reference-incomplete`; one reference under each envelope, the header naming `funnel@a12fa96` and
  `aligned-grid@1`; `receipt_id` and `spec_id` recomputed outside the crate; `receipt_id`
  identical across runs and replay chunk sizes and unchanged by `timings`.
- **CLI.** A transcript per exit class: 0, 1 (`fail`, `no_data`) and 2; a spec with two
  references and no `--reference`; `--bounds` without `--out`; the files of `--out`, byte for
  byte, and nothing written on a refusal; stdout equal to the `oce-verify` golden receipt;
  `--render` leaving stdout unchanged.
- **The version (decision 38).** In whichever PR bumps: an artefact carrying engine version
  0.5.0 refused with `EngineVersionMismatch`, in the pattern of
  `crates/oce-api/tests/blob_old_layout.rs`.
- **Building 33.** One day end to end, its receipt a golden regenerated only through `oce-bless`.

## Out of scope

- Driving a real controller, BACnet I/O, modelica-json and OpenModelica (§12.7.2 steps 1–7).
- Plots, sequence diagrams (§12.5.4) and validation (§12.4 step 2).
- Converting parameters; unit pairs beyond `units/v1` (each needs a table PR with goldens).
- Any change to `oce-api`, `oce-conformance` or the replay crate's scope; gating on a receipt.
- The vector receipt's body and the meaning of id prefixes (M3).
- A trace file under `--out`, a run over several references, and an Enum boundary port before
  #465.

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

### Run readings

*(Added 2026-11-20, issue #657.)* The note was written before `oce-verify`, its receipt and the
`oce verify` verb existed. Decisions 27 to 42 close what it left open between "a spec and its
sources are loaded" and "a receipt is on stdout", so the run (#662), the receipt (#663) and the
verb (#664) each build from a complete contract. Decisions 27 to 39 answer the issue's thirteen
questions in its order (question *n* is decision 26 + *n*); decisions 40 to 42 were found while
writing the body table. Decisions 1 to 26 are not reopened.

**The owner's rulings** (2026-11-20). Four questions were put with their options, and each was
decided as recommended:

1. **A refused frame** (decision 28). A missing sample is recorded and the run continues, with
   its instants uncovered and out of the verdict; an engine refusal is an error, exit 2, with
   no receipt.
2. **Names outside the sequence** (decision 27). Mapping names carry no prefix; an indicator
   name without the sequence prefix is an `indicatorSources` column.
3. **Outputs with no expected series** (decision 40). Listed in `unverified_outputs`, with no
   effect on the verdict.
4. **The `data` label** (decision 41). The caller's option, and `--data` on the verb.

The same day the owner ruled on the workspace version (decision 38, on issues #656, #657, #663
and #671) and on the M3 gate row (decision 39, on #663). Those six are the owner's rulings. The
other ten, decisions 29 to 37 and 42, follow from a standing ruling or from `oce verify` as
built. The owner was given the list and raised nothing, so each stands as written; none is
recorded as a ruling.

**What the decisions rest on,** checked on `development` at commit `71256f99`:

- **The name rule.** `oce_replay::resolve_path` (`crates/leverage/oce-replay/src/map.rs:336`)
  finds a name in a sorted list of canonical paths: the exact path, else the one path that ends
  with the name after a `.`. A `#` is not a boundary: against `urn:x#top.ahu.uDam`, `uDam` and
  `ahu.uDam` resolve and `top.ahu.uDam` does not (its doc test, `map.rs:334`). An ambiguous name
  returns its candidates and an unknown one an empty list. `InputMap::resolve` (`map.rs:128`)
  and `oce-verify`'s binding both use it.
- **What a boundary path is.** The connector's `@id`. The in-repo rule's output is
  `urn:cxf-library:ahu-fc-050#ahu_fc_050.yFault`
  (`crates/leverage/oce-verify/tests/fixtures/receipts/both_open_900.receipt.json:60`). A
  modelica-json translation writes the class's dotted name where that document has a root name:
  `http://example.org#Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.FreezeProtection.and1`
  is an instance of the vendored `FreezeProtection.jsonld`. `FrameSlots::outputs()` lists the
  root boundary outputs only (`crates/oce-api/src/frame_slots.rs:73`). No path carries the LBL
  `sequence` instance name.
- **Refusals in replay.** `ReplayOptions::on_refusal` is one run-level policy
  (`crates/leverage/oce-replay/src/replay.rs:47`): `OnRefusal::Stop` or
  `OnRefusal::RecordAndContinue` (`src/policy.rs:93`). It covers a frame replay refuses for a
  missing sample and a frame the engine refuses alike. Refusal is atomic in the engine
  (`policy.rs:90`), and an `EngineRefusal::Run` refusal stops the run whatever the policy
  (`replay.rs:543` to `549`).
- **Parameter edits.** `parameters::apply` (`crates/leverage/oce-verify/src/parameters.rs:36`)
  applies a sorted edit list inside one halt, in passes, and returns `param-refused` with the
  class of the refusal; `set_param` checks class rules against the table as it stands
  (`crates/oce-api/src/params/class_rules.rs`).
- **The receipt.** `ReceiptKind` has the one variant `Vectors` and `ComparisonMode` the one
  variant `Exact` (`src/receipt.rs:28`, `:46`); `Comparison` holds the mode only (`:65`). A
  scenario records its own `content_id` only when it has edits (`src/run.rs:268`).
  `load_warnings` is capped by the constant `MAX_LOAD_WARNINGS`, 64 (`src/limits.rs:36`), which
  no caller can change; the two record caps are `VerifyLimits` fields a caller can lower
  (`src/limits.rs:65`, `:68`).
- **Enum ports.** Binding refuses a subject with an Enum boundary input or output as
  `enum-port-unsupported`, and a String port as `unsupported-port-type` (`src/bind.rs:279` to
  `297`).
- **The CLI.** `--render auto|always|never` is a global flag of `oce`
  (`crates/leverage/oce-cli/src/lib.rs:82`): it sends a human-readable rendering of the report
  to stderr and never changes stdout. `oce verify` puts one document on stdout
  (`src/commands/mod.rs:5`): the receipt for exit 0 or 1 through `Outcome::verdict`
  (`src/commands/mod.rs:76`), the `oce-report/1` report for exit 2. With `--out DIR` it writes
  `DIR/receipt.json` atomically and prints the report (`src/commands/verify.rs:112` to `139`).
  No `oce` command has a `--format` flag.
- **The trace.** `Trace::canonical_bytes` "is not a published interchange format"
  (`crates/leverage/oce-replay/src/canonical.rs:2`). A trace retains its inputs only when
  `TraceOptions::inputs` is set (`src/replay.rs:29`).
- **The second envelope.** `oce-conformance`'s `build_bounds` and `compare`
  (`crates/oce-conformance/src/funnel.rs:115`, `:153`) sum the tolerance terms (`:307`), and
  `compare` also checks the bounds' breakpoints between test samples
  (`crates/oce-conformance/tests/funnel.rs:256`). Issue #655 copies them into `oce-verify` as
  `aligned-grid`, beside the funnel port.
- **OBC §12.7, read on 2026-11-20.** Listing 12.2's CDL names carry no instance prefix
  (`TZonCooSetOcc`, `uOccSen`), and Listing 12.1 gives both of its references, `setPoiVAV` and
  `setPoiVAV1`, the one mapping file, `realControllerPointMapping.json`. Listing 12.1's keys are
  `setPoiVAV1.TSup*`, and its indicator is `fanSta.y`: a signal of another instance of the
  enclosing Modelica model, which the engine never loads. Neither listing holds a `//` or a
  `/* */` comment.

**Where the tree and the published text overruled the issue's draft answers.**

- **Mapping names.** The draft required the `<sequence>.` prefix on every name. Listing 12.2
  writes bare CDL names, and one mapping file serves two instances in Listing 12.1.
- **An indicator outside the sequence.** Under the draft, Listing 12.1's `fanSta.y` would be a
  typed error with no way to supply it.
- **Globs.** `resolve_path` takes one name to one path, so "strip the prefix and resolve" gives
  a glob nothing to match; decision 27 names the last path segment.
- **A gap under zero-order hold.** "The engine series has no sample there" would still compare
  the value held from before the gap; hence the uncovered instants of decision 28.
- **`sources`.** The draft put a cap and a `_total` beside it; a source list cut short would
  drop digests from `receipt_id`, so it is complete and bounded at bind (decision 42).

**One sentence corrected.** "Test-spec file" said that LBL's "examples carry comments".
Listings 12.1 and 12.2 as published carry none. The sentence states the loader's rule alone,
which is decision 3's and does not change, and the test plan asks for a commented copy beside
the verbatim listing.

**The owner's rulings of 2026-11-21** *(the review of GitHub #704, issue #658)*. Building the
spec reader showed two places where this note said two things, or nothing. Each was put to the
owner with its options:

1. **Override precedence is order-independent, and a conflict is refused.** "Test-spec file"
   said overrides apply in declaration order, later fields winning; decision 29 said the
   canonical spec sorts keys. Both cannot hold: sorted, a stored spec would run differently from
   its file. The ruling: an exact key beats a glob; two keys of one kind (two exact names, or
   two globs) that select one output and give one tolerance field different values are refused
   with a typed error, `conflicting-override`, naming both keys, the output and the field; two
   keys of one kind that give the same value, or different fields, combine. The canonical form
   sorts every key, `outputs` and `indicators` included, so a spec's meaning and its `spec_id`
   cannot change when a tool reorders it. The refusal is the key selection's, at bind (#662).
   Rejected: file order for `outputs` with a finding when one override hides another (a
   reordering tool would still change what runs); file order for both maps, kept by the
   canonical form, as first built in GitHub #704 (the canonical form would sort every object
   but two, and the spec's meaning would rest on an order JSON does not define).
2. **`controllerOutput` beside `expected` is a finding.** When a reference gives both,
   `expected` is the expected-output source and the loader records `lbl-field-ignored` at
   `references[i].controllerOutput`, in the stated order of the findings, like the four
   Scenario 1 fields. It is not a refusal.

Accepted as built in the same review: several `indicators` keys that select one output are all
applied, ANDed, each name once, as "Indicators" and OBC §12.7.2 read.

**Readings added in review** *(2026-11-21, the review of GitHub #676)*. None reopens a ruling;
each closes a place where a builder would have had to guess, and each is decided from the
rulings and the code as built.

- **What `frame-refused` carries** (decision 28). Replay reports an engine refusal as a class
  and a message, with no slot and no path (`crates/leverage/oce-replay/src/trace.rs:228`), and
  `Replay::trace()` gives the rows so far (`src/replay.rs:292`). The error carries the class as
  its cause and, in its detail, the frame index, its time and the engine's bounded message. It
  carries no input path.
- **Codes.** The three lists a commissioning run's codes come from, `frame-refused` as one word
  in two lists, and `too-many-points` caught at bind ("Rust API sketch").
- **`masked_exceedance`.** What is compared at a masked instant and when it can be above zero
  ("Report and receipt"). OBC §12.7.2 step 8 zeroes and then compares; pyfunnel has no mask.
- **The tables.** `window` is required to run (`reference-incomplete`); `sources[].rows`,
  `units.columns[].cdl`, `compared` without `sampling`, the half-sizes under `aligned-grid`, and
  the ends of an uncovered stretch are each stated in their rows.
- **No self-enclosure refusal in commissioning,** and what that means for two identical series
  ("Envelope"), after the funnel port's measurement on a tick grid (GitHub #687,
  `crates/leverage/oce-verify/tests/funnel_tick_grid.rs`).
- **A non-finite or out-of-range engine sample** fails its output, and why a vector window
  treats the same value differently ("Report and receipt").
- **Shared items.** `ALIGNED_GRID_VERSION` comes from the funnel port beside `FUNNEL_VERSION`;
  the tolerance mode, `compare-refused` and `too-many-points` arrive with whichever of #663 and
  #684 merges first.
