# The Building 33 worked example

**`cooling_coil_valve.oce` is authored from the vendored model, not vendored.** It is a hand
transcription of `Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve`, read from
[`third_party/modelica-buildings-cdl/Buildings/Utilities/Plotters/Examples/BaseClasses/CoolingCoilValve.mo`](../../../../../../../third_party/modelica-buildings-cdl/Buildings/Utilities/Plotters/Examples/BaseClasses/CoolingCoilValve.mo)
(Modelica Buildings Library at `a131864`), written in the oce text form and emitted with
`oce emit`. Upstream's modelica-json translation of the same class is vendored beside the `.mo`
and does not load (issue #760); it is not copied, flattened or edited here. **Every verdict this
fixture gives is about this transcription, not about upstream's translation.** The owner ruled
so on 2026-11-21 (issue #755; commissioning note, "As landed: the Building 33 material").

The six trends are read in place from the vendored tree
(`third_party/.../Resources/Data/Utilities/Plotters/Examples/ControlsVerification_CoolingCoilValve/`)
and never copied into the repository a second time: the tests hand their bytes to the library
under the spec's `data/` paths, and the CLI test copies them into a scratch directory.

## Files

| Path | What it is | Written by |
| --- | --- | --- |
| `cooling_coil_valve.oce` | The transcription, in canonical `oce fmt` form; a comment above each statement names the `.mo` line it comes from. | by hand |
| `cooling_coil_valve.cxf.json` | Its CXF. `oce-cli`'s `building_33_commission.rs` holds it to `oce emit`'s output byte for byte. | `oce emit cooling_coil_valve.oce -o cooling_coil_valve.cxf.json` |
| `spec.json` | The commissioning note's worked spec, with `cxf` naming the file above. Every other value is the note's: one day (0 to 86,400 s) at 5 s, `atolx` 1 s, `rtoly` 0.01, `funnel` with the engine series as the reference, holds of 10 s and, for the daily fan-enable log, 90,000 s, the indicator `cooValSta.uFanSta`, `parameters: {}`. | by hand |
| `b33-points.json` | The point mapping: `degF` to `K` for the three temperatures, `percent` to `1` for the valve, a `float` read as a `bool` for the fan enable, and **no unit on either side for the fan feedback** (below). | by hand |
| `receipts/day.receipt.json` | The day's receipt. | `OCE_BLESS_VERIFY=1 cargo nextest run -p oce-verify` |
| `declarations.txt` | What `CoolingCoilValve.mo` declares: parameters with their bindings evaluated, connectors, blocks with their modifiers, connections; headed by the `.mo`'s digest. | `python3 -I derive_declarations.py <the .mo> > declarations.txt`, by hand; never blessed |
| `derive_declarations.py` | Reads the `.mo` with its own parser and a closed expression evaluator; never reads the `.oce`. | nobody: it is the reference |
| `valve_oracle.txt` | The valve signal over 64 synthetic frames that take every branch (below), with the frames' inputs. | `python3 -I derive_valve.py > valve_oracle.txt`, by hand; never blessed |
| `derive_valve.py` | The sequence written out from the `.mo` and the vendored CDL block sources, under the engine's documented discrete time. | nobody: it is the reference |
| `openmodelica/run_openmodelica.sh`, `simulate.mos`, `extract_day.py` | Simulate upstream's own `ControlsVerification_CoolingCoilValve`, unmodified, in the pinned OpenModelica 1.25.1 container, offline, and take its `yCooVal` at the day's 5 s instants. | by hand |
| `openmodelica/yCooVal_day.csv`, `provenance.json` | That signal (17,281 rows, as OpenModelica printed it) and the record of the run (image digest, `omc` version, architecture, the two library commits and trees). | `sh openmodelica/run_openmodelica.sh <buildings checkout> <msl checkout>`, by hand |

The Building 33 day of the funnel differential is in `../../funnel/recorded/building_33_day/`
(its inputs are this run's two series) and `../../funnel/expected/building_33_day/` (what the
pinned pyfunnel wrote); see the funnel fixture's README.

## How the transcription is checked

Four checks, each from outside the authored file:

1. **Declarations** (`tests/commissioning/building_33_transcription.rs`): the emitted CXF's 15
   root parameters, in the `.mo`'s order, each with the bits its binding evaluates to; the
   engine's six boundary connectors with the `.mo`'s direction, type, unit, quantity, display
   unit, `min` and `max`; the 15 blocks with the `.mo`'s classes, every modifier's value as the
   engine grounds it and as a reference where the `.mo` names a parameter; and the 21 `connect`
   statements as unordered pairs, no more and no fewer. `declarations.txt` carries the digest of
   the `.mo` it was read from, and a test checks it against the vendored bytes.
2. **Behaviour on every branch** (same file): the engine's valve signal equals
   `derive_valve.py`'s bit for bit at all 64 frames. The frames take, in turn: off before the
   outdoor air first passes the cut-off; the PI controller active, at 5 s, 2 s and 13 s steps and
   with a negative error; the outdoor-air hysteresis holding below the cut-off; the supply-air
   high limit cutting the PI's signal, and the line clamped at its lower end; the fan feedback
   hysteresis holding, then disabling below `uLow` and staying off below `uHigh`; the fan status
   disabling; the outdoor air below upstream's `uLow`; and each re-enable resetting the
   integrator.
3. **Upstream's model in OpenModelica** (`building_33_openmodelica.rs`): over the day, the
   engine's signal stays within **0.0034** of OpenModelica's at every 5 s instant (largest at
   67,375 s, the valve near 0.71; mean absolute difference under 1.5e-4). OpenModelica's run is
   the continuous model with its trends interpolated linearly (CombiTimeTable, linear segments)
   and upstream's own `CoolingCoilValve`, under cvode at tolerance 1e-6; the engine runs the
   transcription at the trends' frames with each sample held. The two differ by those choices,
   well inside the funnel's 0.01.
4. **Mutants** (`oce-cli`'s `building_33_mutants.rs`): ten plausible mistakes in the authored
   text, each emitted and held to checks 2 and the day's receipt. Check 2 catches all ten. The
   day catches four (the setpoint and measurement swapped, the gain without its 9/5, the
   integral time a tenth, reverse acting); it cannot see the other six, because on this day the
   high limit never cuts the signal (the supply air is below 50 degF at 56 instants, where the
   limit is about 0.99 and the valve at most 0.91) and the enable is on throughout. No mutant
   survives both. The structural check 1 catches every one by construction.

## The transcription, line by line

The text form takes no expression on the right of `=`. Where the `.mo` binds a parameter to an
expression, the transcription writes the f64 value Python gives that expression in the `.mo`'s
operation order (`derive_declarations.py` evaluates them again, independently, and check 1
compares the bits); the expression is in a comment above the statement. A consequence: an edit to
`alc_prop_k` would not move `k`, nor an edit to `TOutDelta` move `TOutThr.uLow`, as it would in
Modelica. The spec edits no parameter (`parameters: {}`).

| `.mo` line | `.mo` declaration | Authored element | Dropped, and why |
| --- | --- | --- | --- |
| 1 | `within Buildings.Utilities.Plotters.Examples.BaseClasses;` | — | The package: CXF has no package; the class name is the sequence name. |
| 2 | `block CoolingCoilValve "…"` | `sequence CoolingCoilValve`, `derived_from none` | The description string. |
| 4 | `parameter Real alc_prop_k(final unit="1/F") = 1/100` | `param alc_prop_k : Real = 0.01` | `unit="1/F"`: the text form puts no unit on a parameter, and `F` is no base unit for a suffix; `final`; `annotation(Evaluate=true, Dialog(…))` (tool hints). |
| 8 | `alc_int_k(final unit="1/F") = 0.5/100` | `param alc_int_k : Real = 0.005` | As line 4. |
| 12 | `alc_k_unit_conv = 9/5` | `param alc_k_unit_conv : Real = 1.8` | The annotation. |
| 16 | `Temperature TOutCooCut = 50*(5/9) - 32*(5/9) + 273.15` | `param TOutCooCut : Real = 283.15 K` | The type's unit and quantity (the `K` suffix is text only); the annotation. |
| 20 | `Temperature TOutDelta = 2*(5/9) - 32*(5/9) + 273.15` | `param TOutDelta : Real = 256.4833333333333 K` | **As upstream writes it**: a 2 degF difference converted as an absolute temperature (below). |
| 24 | `FanFeeCut = 15/100` | `param FanFeeCut : Real = 0.15` | The annotation. |
| 28 | `FanFeeDelta = 10/100` | `param FanFeeDelta : Real = 0.1` | The annotation. |
| 32 | `Temperature TSupHighLim = 50*(5/9) - 32*(5/9) + 273.15` | `param TSupHighLim : Real = 283.15 K` | As line 16. |
| 36 | `Temperature TSupHigLim = 42*(5/9) - 32*(5/9) + 273.15` | `param TSupHigLim : Real = 278.7055555555555 K` | As line 16. |
| 40 | `Time interval(min=1) = 15` | `param interval : Real = 15 s` | `min=1`: the text form has no parameter bounds; 15 satisfies it. |
| 44 | `Boolean reverseActing=false` | `param reverseActing : Boolean = false` | The annotation. The model's `false` stands over §12.6's prose `reverseAction = true` (commissioning note, "Conflicts accepted"). |
| 47 | `k(final unit="1/K") = alc_prop_k * alc_k_unit_conv` | `param k : Real = 0.018000000000000002 1/K` | The binding, evaluated (above); `unit`. |
| 51 | `Ti(final unit="s") = k*interval/(alc_int_k * alc_k_unit_conv)` | `param Ti : Real = 30.0 s` | As line 47. |
| 55 | `uMax(final min=0, final max=1, final unit="1") = 1` | `param uMax : Real = 1` | `min`, `max`, `unit`: no parameter bounds or units in the text form. |
| 62 | `uMin(final min=0, final max=1, final unit="1") = 0` | `param uMin : Real = 0` | As line 55. |
| 69 | `BooleanInput uFanSta` | `in uFanSta : Boolean` | The description; `Placement` (diagram). |
| 74 | `RealInput uFanFee` | `in uFanFee : Real` | No unit and no quantity, as declared. |
| 78 | `RealInput TSup(final unit="K", final quantity="ThermodynamicTemperature")` | `in TSup : Real @unit K @quantity ThermodynamicTemperature` | `Placement`. |
| 85 | `RealInput TSupSet(final unit="K", displayUnit="degC", final quantity=…)` | `in TSupSet : Real @unit K @quantity ThermodynamicTemperature @displayUnit degC` | `Placement`. |
| 92 | `RealInput TOut(…)`, as line 85 | `in TOut : … @displayUnit degC` | `Placement`. |
| 100 | `RealOutput yCooVal(final min=0, final max=1, final unit="1")` | `out yCooVal : Real @unit 1 @min 0 @max 1` | `Placement`. |
| 107 | `PIDWithReset limPI(final reverseActing=reverseActing, final k=k, final Ti=Ti, final yMax=1, final yMin=0, final y_reset=0)` | `block limPI = CDL.Reals.PIDWithReset(reverseActing = reverseActing, k = k, Ti = Ti, yMax = 1, yMin = 0, y_reset = 0)` | `final` (no effect on a run); the class defaults it does not set (`controllerType` PI, `r` 1, `Ni` 0.9, `xi_start` 0, …) are the catalog's, the vendored `PIDWithReset.mo`'s. |
| 117 | `Line higLim(final limitBelow=true, final limitAbove=true)` | `block higLim = CDL.Reals.Line(limitBelow = true, limitAbove = true)` | `Placement`. |
| 123 | `protected` | — | Visibility: a CXF block has none; it changes no result. |
| 124, 129 | `Constant yCooValMin(final k=uMin)`, `yCooValMax(final k=uMax)` | `block yCooValMin = CDL.Reals.Sources.Constant(k = uMin)`, likewise `yCooValMax` | — |
| 134 | `Hysteresis TOutThr(final uLow = TOutCooCut - TOutDelta, final uHigh = TOutCooCut)` | `block TOutThr = CDL.Reals.Hysteresis(uLow = 26.666666666666686, uHigh = TOutCooCut)` | The `uLow` expression, evaluated (above). |
| 140 | `Hysteresis uFanFeeThr(final uLow=FanFeeCut - FanFeeDelta, final uHigh=FanFeeCut)` | `block uFanFeeThr = CDL.Reals.Hysteresis(uLow = 0.04999999999999999, uHigh = FanFeeCut)` | As line 134; 0.15 - 0.1 in f64 is 0.04999999999999999. |
| 146, 151 | `Constant TSupMin(final k=TSupHigLim)`, `TSupMax(final k=TSupHighLim)` | `block TSupMin = …Constant(k = TSupHigLim)`, `block TSupMax = …Constant(k = TSupHighLim)` | — |
| 156, 160 | `And andOutAirFanFee`, `And andIntErr` | `block andOutAirFanFee = CDL.Logical.And`, `block andIntErr = CDL.Logical.And` | Upstream replaced a `Logical.And3` by these two in 2023 (its revision note). |
| 164–175 | `Min min`, `Change cha`, `BooleanToReal booToRea`, `Multiply pro` | `block min = CDL.Reals.Min`, `block cha = CDL.Logical.Change`, `block booToRea = CDL.Conversions.BooleanToReal`, `block pro = CDL.Reals.Multiply` | — |
| 179–222 | 21 `connect(a, b)` statements | 21 connections `source -> sink`, in the `.mo`'s order, each with its line in a comment | `annotation(Line(…))` (diagram). `connect` is undirected; each is written from the output to the input it feeds (`connect(yCooVal, min.y)` as `min.y -> yCooVal`). |
| 224–290 | `annotation(defaultComponentName, Icon, Diagram, Documentation)` | — | Graphics and documentation only. |

The `.mo` has no conditional component and no `if` equation; nothing was dropped for that reason.

## The rulings this example carries

- **`uFanFee` has no unit on either side.** The port declares none, and upstream's example model
  multiplies the fan feedback trend by 0.01 before it reaches the port. A mapping's `cdl.unit`
  must be the port's unit (`unit-refused`, cause `port-unit-mismatch`, pinned by a test), and a
  unit on one side alone is refused, so `percent` to `1` cannot bind: **the raw percent, 60.9 to
  72.5, reaches the port.** The fan feedback hysteresis switches at 0.05 and 0.15, so it is true
  either way on this data (the scaled values, 0.609 to 0.725, are above 0.15 too). On other data
  it would not be: a fan feedback between 0.15 % and 15 % reads as on here and as off in
  upstream's model.
- **`TOutDelta` is upstream's, as written.** 2 degF is converted with the offset, giving
  256.48 K, so `TOutThr.uLow` is 26.67 K: once the outdoor air passes 50 degF, the cut-off
  turns the enable off again only below 26.67 K, about -411.7 degF. On this day the outdoor air
  starts at 50.39 degF, so the hysteresis is on from the first frame and stays on. With the
  delta converted as a difference (1.11 K, `uLow` 282.04 K, 48 degF) it would stay on too: the
  day's outdoor air never falls below 48.89 degF. A test runs the day with `TOutThr.uLow` corrected and finds every
  figure the same.
- **Masking.** The indicator `cooValSta.uFanSta` keeps OBC §12.7.2's zero fill (#745). The fan
  was enabled all day, so it masked nothing.
