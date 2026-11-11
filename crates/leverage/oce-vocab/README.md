# oce-vocab

The block vocabulary: reference text and unit facts about the 133 authorable block classes,
served beside the engine's catalog. For each class it has the upstream links; for each connector
and parameter a meaning and the unit and quantity the CDL source declares; and curated notes
written for agents. Design note: [`docs/design/vocabulary.md`](../../../docs/design/vocabulary.md)
(issue #499 is part 1 of 3: the data and this crate; issue #500, part 2, serves it through
[`oce catalog`](../oce-cli/README.md) and adds the rendered ranges and the scale-factor table).

It is a leverage crate: it depends on `oce-api` only, loads no engine and is not published. It is
**not engine behaviour**. Nothing on the load path, the emitter, the compiled-schedule blob, the
content id or a class signature hash reads it, and `oce-api`'s catalog is unchanged by it. It never
copies what the engine owns: port names and kinds, parameter types, defaults and ranges come from
`oce_api::catalog()`, `class_metadata()` and `class_ports()` when a view is built.

```rust
let delay = oce_vocab::class_vocab("CDL.Logical.TrueDelay").expect("an authorable class");
let delay_time = &delay.parameters[0];
assert_eq!((delay_time.name, delay_time.unit, delay_time.quantity), ("delayTime", "s", "Time"));
assert_eq!(delay_time.description, "Delay time");
for note in delay.notes_about("delayOnInit") {
    println!("{}: {}", note.kind.as_str(), note.text);
}
```

## The two files

Both live in [`data/`](data/), are embedded with `include_str!` and are parsed once
(`vocabulary()`). Both are strict JSON under the authoring wire profile: closed objects, every key
present, absent is `""` or `[]`, no `null`, keys sorted, two-space indentation, LF, one trailing
newline, ASCII only. A read is bounded like a wire document (4 MiB, 4,096-byte strings) and
refuses anything that is not the canonical spelling of what it says, with a typed `VocabError`.

Hostile input is bounded in work as well as in bytes (`tests/vocabulary/reader_bounds.rs`,
`src/vocabulary_tests.rs`):

- the **byte bound** is checked before anything is parsed, and it is the reader's only size bound.
  There is no separate list bound: an accepted document cannot hold the wire reader's 65,536 list
  items (the smallest item is 96 bytes in the canonical spelling), but a compact one fits about
  95,000 under the byte bound, and it is parsed and checked in full before it is refused as
  `NotCanonical`;
- the **closed shape** stops nesting: it is five levels deep, and a document nested a million
  deep is refused at the first level the shape does not have, never scanned to its end;
- the **join** of the two files is `n log n` in their entries: each curated class indexes its
  class's connectors and parameters once, and each curated description and note is one binary
  search. One class with 24,000 connectors and 32,000 notes naming the last one costs about 0.83
  million name comparisons; a scan per note is 498 million. The test asserts the count, not time.

### `generated.json`: never edited by hand

One entry per authorable catalog class, in catalog order:

```json
{
  "class": "CDL.Logical.TrueDelay",
  "connectors": [
    {"description": "Input signal to be delayed when it switches to true", "dims": 0, "name": "u", "quantity": "", "side": "input", "unit": ""}
  ],
  "parameters": [
    {"description": "Delay time", "name": "delayTime", "quantity": "Time", "unit": "s"}
  ],
  "upstream": true
}
```

(shown compact; the file writes one key per line.)

- **`connectors`** are declared CDL connectors, not resolved ports: a vector connector `u[nin]` is
  one entry with `dims` 1, and the engine's ports `u1` … `un` are its elements
  (`ClassVocab::connector_of_port`, `port_element`). `dims` 2 is a matrix connector, whose ports
  are `y1_1` … `yr_c`. Inputs come first, each side in declaration order.
- **`parameters`** are in the catalog's order. An array parameter the catalog spells as a template
  (`k_<i>`, `K_<row>_<col>`) is listed once under its CDL name (`k`, `K`; `parameter_base_name`).
- **`unit`** and **`quantity`** are what the CDL source declares, in Modelica spelling (`s`, `K`,
  `1`, `1/s`; `Time`, `ThermodynamicTemperature`). Most CDL connectors declare none. Defaults and
  ranges are deliberately absent.
- **`upstream`** is false only for `CDL.Logical.TrueHoldWithReset`, which the engine carries and
  current Buildings does not; it has no links and its descriptions are curated.
- **`source`** records the vendored document, its SHA-256 and the two upstream pins.

It is a projection of
[`third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld`](../../../third_party/modelica-buildings-cdl/README.md)
(modelica-json's `-o cxf --elementary --cxfCore` output at the recorded pins) through the
catalog's own names: the generator walks every class, connector and parameter `oce-api` names,
looks each up by `@id`, and reads the description and the QUDT unit and quantity identifiers back
into Modelica strings through a closed, pinned inverse of modelica-json's tables. The catalog
drives the walk; the document only supplies text.

### `curated.json`: written by people

```json
{
  "class": "CDL.Logical.TrueHoldWithReset",
  "descriptions": [
    {"evidence": "…", "subject": "clr", "text": "Clear input: while true the output is false and the hold is cancelled"}
  ],
  "notes": []
}
```

Curation is **additive** in v1:

- a **description** may exist only where the generated one is `""` (six today: the four of
  `TrueHoldWithReset`, and `MatrixGain`'s `nout` and `nin`, which upstream declares `protected`).
  A curated description for a subject that already has generated text is refused
  (`VocabError::CuratedReplacesGenerated`); an override with a required reason is v2;
- a **note** names its class (`subject` `""`), a connector or a parameter, and has a `kind`;
- every description and note carries **`evidence`**: where the claim comes from. It is for
  reviewers and is not shown to agents. The data types carry it (`Note::evidence`,
  `Vocabulary::curated()`), so a surface that serves agents prints a note's `kind`, `subject`
  and `text` only; `oce catalog` does, and its tests hold that over every class;
- classes are in catalog order, and a class with nothing curated is absent.

## Note kinds

| `kind` | What it says | Seeded example |
| --- | --- | --- |
| `convention` | A library convention that differs from the class default | `Logical.TrueDelay.delayOnInit = true` |
| `pitfall` | Something that looks right and is not | `Discrete.Sampler` is not a delay |
| `port-order` | Which input is which | `Reals.Subtract` gives `u1 - u2` |
| `loop` | Whether a feedback loop through the class alone is an algebraic loop | `Logical.Pre` and `Discrete.UnitDelay` break one; `Latch`, `TrueDelay`, `TrueFalseHold`, `Sampler` and `ZeroOrderHold` do not |
| `units` | A unit convention the declarations do not carry | none yet (v2) |

The kind lets a tool pick the notes that matter: a repair message for an `algebraic-loop` finding
can append the `loop` notes of the classes in the cycle.

A note is advice an agent acts on, so every seeded claim the engine can decide is checked against
a loaded engine in `tests/vocabulary/engine_agreement.rs`: each `loop` note against a self-loop
through the class, each `port-order` note against a frame with the inputs swapped, the
`delayOnInit` convention against both settings, `TrueHoldWithReset`'s descriptions and the
`Sampler` pitfall against a run of frames. Add a case there with a new note of those kinds.

## The view

`vocabulary()` is the merged pair, checked against the catalog; `class_vocab(class_path)` is one
class joined with it (`ClassVocab`):

| Field | From |
| --- | --- |
| `class_path`, `upstream` | `generated.json` |
| `description` (the class one-liner) | `oce_api::class_metadata()` |
| `links` | derived from the class path and the pinned Buildings commit |
| `connectors`: name, side, dims, description, unit, quantity | the two files; `origin` says which supplied the description |
| `parameters`: the same, plus `catalog` (type, default, settable) | the two files and `class_metadata()` |
| `parameters[].range` | rendered from `oce_api::catalog()`'s rules, never stored (below) |
| `notes` | `curated.json` |

`links.pinned_source` is the class's Modelica source on GitHub at the pinned commit.
`links.latest_help` is its page in the **latest** Buildings release documentation: it follows
upstream releases and **may drift from the pin**, so label it that way wherever it is shown.

`Vocabulary::from_parts` holds the pair to the rules above and to the catalog: the classes, each
class's parameters and each named class's connectors must be exactly the catalog's, in its order,
and every connector and parameter must have a description after the merge. A build whose embedded
files break a rule panics on the first `vocabulary()` call; the tests prove they do not.

## Ranges

A parameter's range is **rendered from the catalog's rules every time a view is built**
(`parameter_range(rules, name)`, `ParameterVocab::range`); neither data file holds a bound. It is
one line of plain text per rule that constrains the parameter's *value*, in the rules' order:

| Rule kind | Line | Listed for |
| --- | --- | --- |
| `RealGreaterOrEqual`, `RealLessOrEqualConstant` | `Ti >= 1e-13`, `width <= 1.0` | the parameter |
| `RealGreaterThan`, `RealFiniteGreaterThan`, `RealFinite` | `delta > 0.0`, `pAtm > 0.0 and finite`, `shift is finite` | the parameter |
| `IntegerGreaterOrEqual`, `IntegerLessOrEqualConstant` | `nin >= 0`, `nin <= 1048576` | the parameter |
| `RealLessOrEqual` | `uLow <= uHigh` | both |
| `RealTimesIntegerInclusiveRange`, `IntegerProductLessOrEqualConstant` | `0.001 <= h * n <= 0.5`, `nout * nin <= 1048576` | both |
| `RealEqualWarning`, `RealLessOrEqualWarning`, `RealGreaterOrEqualScaledWarning` | `uLow == uHigh warns`, `debounce > feedbackDelay warns`, `period < 2.0 * minTruFalHol warns` | both |
| `IntegerArrayElementsInRange` | `1 <= extract_<i> <= nin` | the array |
| `BooleanArrayTrueCountEquals` | `the number of true msk_<i> == nout` | the array and the count |

A `warns` line names the case the engine warns about and still loads; the authoring validator
reports every engine warning as an error. A Real bound is spelled as Rust prints the `f64`, which
reads back to the same bits. The twelve rule kinds about a value's type (`Required`, `Boolean`,
`Real`, `EnumMembers`), an array's shape (`IntegerArrayElements`, `RealArrayElements`,
`BooleanArrayElements`, `RealMatrixElements`, `TimeTableMatrix`, `TimeTableOffset`) and port
structure (`Structural`, `StructuralArrayElements`) render nothing: the type, the default, the
template name (`k_<i>`) and the resolved ports say that. Ports have no range: the catalog has no
rule about a port's value.

Every `CatalogRule` is named in one total `match`, so a new rule kind does not compile until it
is rendered or listed with the twelve. `tests/vocabulary/ranges.rs` pins one catalog example per
kind and checks every kind of line against the engine at its boundary: `class_ports` accepts the
bound and refuses the next `f64` past it, and a load warns exactly where a `warns` line says.
101 of the 189 parameters have a range.

## Unit scale factors

`SCALE_FACTORS` is the closed table of unit symbols that are an exact positive integer multiple
of a base unit, with no offset (design note decision 14); `scale_factor(symbol)` looks one up.
v1 is time only:

| Symbol | Base | Factor |
| --- | --- | --- |
| `min` | `s` | 60 |
| `h` | `s` | 3600 |
| `d` | `s` | 86400 |

`degC` and `degF` (affine), `%` and prefixed SI units (`kPa`) are not in it, by ruling. The crate
states the table; scaling a written literal and checking a suffix against a declared unit belong
to the text form that reads the suffix: `oce-author` does both (issue #501), reading this table
and each parameter's declared `unit`. `oce vocab` prints it.

## Regenerating `generated.json`

The generator is a test, so it runs in the leverage lane on every leverage and engine change:

```bash
cargo nextest run -p oce-vocab                # regenerates and compares byte for byte
OCE_BLESS=1 cargo nextest run -p oce-vocab    # rewrites data/generated.json
cargo nextest run -p oce-vocab                # run it again without the switch
```

It fails, naming the class and the subject, when a catalog port or parameter is renamed, added or
removed; when the vendored document loses a node, gains one that is pinned as absent, or carries a
unit or quantity identifier outside the pinned tables; and when `generated.json` is edited. A
quantity kind with more than one inverse (`Pressure`, which modelica-json also writes for
`PressureDifference`) refuses unless the subject has a pinned resolution
(`tests/vocabulary/generator/units.rs`), which the units test then checks against the
`quantity="…"` of the vendored `.mo` declaration.

After a bless, review the diff of `data/generated.json`, say in the PR which entries moved and
why, and re-run without the switch. A pin advance re-vendors `CXF-Core.jsonld` first
([pin-advance policy](../../../third_party/modelica-buildings-cdl/README.md#pin-advance-policy)).
A class whose ports the catalog does not name (width-driven or positional) also needs a row in
`tests/vocabulary/widths.rs`.

## Curating

Edit `data/curated.json` by hand, keep it in the canonical spelling (the reader names the first
line that is not), give every entry `evidence`, and run the suite. The tests also hold every
string in both files to plain agent-facing text: no absolute path, no UUID-shaped string, no
`modelica://` link, no HTML tag, no line break, inside a length bound.

## Coverage

| | Entries | Described | Units | Quantities |
| --- | --- | --- | --- | --- |
| Classes | 133 (132 upstream, with links) | | | |
| Connectors | 322 (265 named ports, 57 vector or positional connectors) | 319 generated, 3 curated | 21 | 11 |
| Parameters | 189 | 186 generated, 3 curated | 52 | 41 |

Units in use: `s`, `rad`, `1`, `K`, `1/s`, `Hz`, `J/kg`, `Pa`. Every generated description, unit
and quantity is found in its vendored `.mo` declaration, and every unit and quantity a public
declaration carries is generated (`tests/vocabulary/declared_units.rs`). Curated: 13 notes over
11 classes and 6 descriptions over 2. Unit conventions for the unit-free connectors are v2.
