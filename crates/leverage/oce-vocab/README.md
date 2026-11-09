# oce-vocab

The block vocabulary: reference text and unit facts about the 133 authorable block classes,
served beside the engine's catalog. For each class it has the upstream links; for each connector
and parameter a meaning and the unit and quantity the CDL source declares; and curated notes
written for agents. Design note: [`docs/design/vocabulary.md`](../../../docs/design/vocabulary.md)
(issue #499 is part 1 of 3: the data and this crate).

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
  reviewers and is not shown to agents;
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
| `notes` | `curated.json` |

`links.pinned_source` is the class's Modelica source on GitHub at the pinned commit.
`links.latest_help` is its page in the **latest** Buildings release documentation: it follows
upstream releases and **may drift from the pin**, so label it that way wherever it is shown.

`Vocabulary::from_parts` holds the pair to the rules above and to the catalog: the classes, each
class's parameters and each named class's connectors must be exactly the catalog's, in its order,
and every connector and parameter must have a description after the merge. A build whose embedded
files break a rule panics on the first `vocabulary()` call; the tests prove they do not.

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
