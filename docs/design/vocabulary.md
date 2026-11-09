# Vocabulary v1

**Design note, written before any vocabulary code (M2 item 6, tracking issue #455).** It fixes
where the block vocabulary comes from, the shape of the data and how generated text is kept apart
from curated text, where the data lives and who serves it, what v1 does with units, the coverage
targets, and how the M2 "agent-usable" gate row is measured against it. Every decision below was
made while drafting and ruled on in review (see [Review rulings](#review-rulings)); the note is
amended to match, and nothing here is built yet.
The authoring note's decisions and review rulings are not reopened; where this note amends one
(decision 13 on unit suffixes) it says so, and the implementing PR amends the authoring note with
a dated ruling.

The work plan's M2 item 6:

> Vocabulary v1: generate from `modelica-json -o doc` over `third_party/` (block, port and
> parameter descriptions, units, Buildings links), curate agent notes, serve through `catalog()`.

The ideation record lists its readers: agents (tool descriptions and repair messages), editor hover
text and the library's card generator. The authoring note defers three things here: unit suffix
scale factors (decision 13), library conventions such as `Logical.TrueDelay.delayOnInit = true`
(decision 16), and descriptions beyond the one-liners of #440 (decision 7).

## What the vocabulary is, and is not

It is **reference text and unit facts about the 133 authorable block classes**, served to readers
beside the engine's catalog: for each class its Buildings links, for each port and parameter a
meaning, a declared unit and quantity where the CDL source declares one, and curated agent notes.
It is a leverage crate, **`oce-vocab`**, holding two checked-in JSON data files and a joined view
over `oce-api`'s catalog.

It is **not engine behaviour**. Nothing on the load path, the emitter, the compiled-schedule blob,
the canonical content id or `class_signature_hash` reads it ([content-id note](content-id.md):
descriptions are not behaviour). It never restates what the engine owns: port names and kinds,
parameter types, defaults and ranges are read from `oce_api::catalog()`, `class_metadata()` and
`class_ports()` when a view is built, never copied into the data. It is not a semantic model
either: QUDT and Brick mappings are M8's.

## What the sources actually contain

The plan names `modelica-json -o doc`. It was run in a scratch directory at the two pins the
vendored tree records (Buildings `a131864e`, modelica-json `85721b82`, Node 26), together with the
two other candidate sources. What each carries for `Reals.PID`, `Logical.TrueDelay` and
`Reals.Hysteresis`:

| Source | Class one-liner | Port meaning | Parameter meaning | Unit, quantity | Notes |
| --- | --- | --- | --- | --- | --- |
| `modelica-json -o doc` | no | no | no | no | Writes `Sequence of Operation.html`, the G36 sequence-document generator. For TrueDelay and Hysteresis it is 75 bytes, a title only; for PID it fails (`TypeError` in `lib/expressionEvaluation.js`). It also needs the `package.mo` files the vendored cone omits. |
| `modelica-json -o cxf --elementary --cxfCore -p` (`CXF-Core.jsonld`) | no (`S231:documentation` holds the class's `info` HTML) | `S231:description` on every connector | `S231:description` on every public parameter | `qudt:hasUnit` and `qudt:hasQuantityKind` only, mapped by a fixed table | 682,615 bytes for all of `CDL`, byte-identical across two runs, no host paths. Defaults and `min`/`max` are included, the bounds as unevaluated text (`100*Constants.eps`). |
| `.mo` `Documentation(info=…)` | the class description string, already in the engine | no | no | no | HTML prose with `<img>`, `modelica://` links and the revision history; 72 of the 132 class documents contain an `<a href>`. |
| `.mo` declarations | yes | description strings | description strings | `unit=`/`quantity=` modifications, verbatim | The engine forbids a `.mo` parser in crates, and the vendored README records five defects of the last declaration extractor. |
| `third_party/…/cxf/` (44 G36 documents) | no | no | no | no | 1,670 `S231:description` values, but on component *instances* inside G36 composites ("Absolute change of the setpoint temperature"), not on CDL class ports. Useful for composites later, not here. |

A sample of what CXF-Core gives (trimmed to the fields the generator reads):

```text
CDL.Reals.PID         u_s  RealInput   "Setpoint input signal"
                      u_m  RealInput   "Measurement input signal"
                      y    RealOutput  "Actuator output signal"
                      Ti   Parameter   "Time constant of integrator block"  unit:SEC  q:Time  value 0.5  min "100*Constants.eps"
                      r    Parameter   "Typical range of control error, used for scaling the control error"
                      controllerType   "Type of controller"  (no @type, absent from hasParameter)
CDL.Logical.TrueDelay u    BooleanInput  "Input signal to be delayed when it switches to true"
                      delayTime  "Delay time"  unit:SEC  q:Time
                      delayOnInit  "Set to true to delay initial true input"  value false
CDL.Reals.Hysteresis  u    RealInput   "Input to be compared against hysteresis values"
                      uLow   "if y=true and u<uLow, switch to y=false"
                      uHigh  "if y=false and u>uHigh, switch to y=true"
```

Its known defects, each handled below: class nodes carry no one-line description; `discrete`
connectors (`CalendarTime.year`, `.month`) lose their `@type` and drop out of `hasOutput`;
enumeration parameters (`PID.controllerType`, `CalendarTime.zerTim`) lose theirs and drop out of
`hasParameter`; protected parameters (`MatrixGain.nin`, `.nout`, which the catalog lists as derived)
are absent; units become QUDT identifiers through a table that is one-to-one for the units this
cone declares but lossy for quantities (`PressureDifference` becomes `Pressure`), and an unmapped
unit is written as `S231:1/s`. The file modelica-json ships at its own root is from a different
Buildings commit and differs from the regeneration.

## Coverage

The catalog has 136 classes; 3 are reserved lowering identities, so 133 are authorable. Walking the
catalog's names (not CXF-Core's relation lists) over the regenerated CXF-Core:

| Item | In the catalog | Described in the source | Missing |
| --- | --- | --- | --- |
| Classes | 133 | 132 | `Logical.TrueHoldWithReset` (absent from current Buildings; its one-liner is engine-written) |
| Named ports (105 named classes) | 265 | 262 | `TrueHoldWithReset.u`, `.clr`, `.y` |
| Vector or positional connectors (28 classes) | 57 connectors | 57 | none; 42 are arrays |
| Parameters (catalog names, templates by base name) | 189 | 186 | `TrueHoldWithReset.duration`, `MatrixGain.nin`, `.nout` |
| Declared units | — | 21 ports and 52 parameters in 39 classes | `s`, `rad`, `1`, `K`, `1/s`, `Hz`, `J/kg`, `Pa` |
| Declared quantities | — | 11 ports, 41 parameters | `Time`, `Angle`, `ThermodynamicTemperature`, `Frequency`, `SpecificEnergy`, `Pressure` |
| Buildings link | — | 132 | `TrueHoldWithReset` |

*(Amended 2026-11-09, issue #500: the parameter row read 186 and 183. The draft's count left out
the three `TimeTable` sources' rule-only `table` parameter, which the catalog lists and CXF-Core
describes; `oce-vocab` carries 189. The missing column was right.)*

So six descriptions need curating for v1 to describe every catalog port and parameter. Most CDL
ports are deliberately unit-free (`Reals.Add` adds whatever it is given); their unit convention
("the unit of `u`") is curation work for v2.

## The data

Two files in `crates/leverage/oce-vocab/data/`, both strict JSON under the authoring wire profile
(closed objects, every key present, absent is `""` or `[]`, no `null`), written by one canonical
writer: keys sorted, two-space indent, LF, a trailing newline, classes in catalog order,
connectors and parameters in declaration order. *(Ruled 2026-11-09: both files are under the
wire profile with sorted keys, so the same reader bounds apply to them as to an authored bundle.)*

`generated.json`, regenerated from the vendored CXF-Core and never edited by hand:

```json
{
  "classes": [
    {
      "class": "CDL.Logical.TrueDelay",
      "connectors": [
        {"description": "Input signal to be delayed when it switches to true", "dims": 0, "name": "u", "quantity": "", "side": "input", "unit": ""},
        {"description": "Output with delayed input signal after it switched to true", "dims": 0, "name": "y", "quantity": "", "side": "output", "unit": ""}
      ],
      "parameters": [
        {"description": "Delay time", "name": "delayTime", "quantity": "Time", "unit": "s"},
        {"description": "Set to true to delay initial true input", "name": "delayOnInit", "quantity": "", "unit": ""}
      ],
      "upstream": true
    }
  ],
  "format": 1,
  "source": {"buildings_commit": "a131864e4c4df22ebcd52bb8da439de0087ac365", "document": "third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld", "modelica_json_commit": "85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb", "sha256": "bb2bf4a7f868af884df4b2f05258a754625e10920716739527b46631a4383314"}
}
```

`curated.json`, written by people, never touched by regeneration:

```json
{
  "classes": [
    {
      "class": "CDL.Logical.TrueDelay",
      "descriptions": [],
      "notes": [
        {"evidence": "open-control-library: 177 of 177 TrueDelay instances", "kind": "convention", "subject": "delayOnInit", "text": "The open-control-library sets delayOnInit = true on every TrueDelay, against the CDL default false, so a condition already true at start-up must still persist for the full delay. The emitter never fills it; write it."},
        {"evidence": "prototype authoring rule 9", "kind": "loop", "subject": "", "text": "Passes the current value through: a feedback loop through TrueDelay alone is an algebraic loop. Break it with Logical.Pre."}
      ]
    },
    {
      "class": "CDL.Reals.Hysteresis",
      "descriptions": [],
      "notes": [
        {"evidence": "authoring decision 14", "kind": "pitfall", "subject": "uLow", "text": "uLow equal to uHigh is refused: the engine warns and the validator reports every warning as an error."}
      ]
    },
    {
      "class": "CDL.Logical.TrueHoldWithReset",
      "descriptions": [
        {"subject": "clr", "text": "Clears the hold: while true, the output follows the input"}
      ],
      "notes": []
    }
  ],
  "format": 1
}
```

The served view joins, per class: the class path and the #440 one-liner from
`class_metadata()`; links derived from the class path and the recorded commit (a pinned GitHub
source link, and the Buildings release help page, which may drift from the pin and is labelled so);
each resolved port (`class_ports`) with its kind and its connector's meaning, unit and quantity
(`u3` maps to connector `u` element 3 by the engine's naming contract); each parameter from
`class_metadata()` with type, default and settable, its range rendered from the catalog's rules
(`Ti >= 1e-13`, `yMin <= yMax`, `uLow == uHigh warns`), and its meaning, unit and quantity; then the
notes. The text of `TrueHoldWithReset.clr` above is illustrative; the curation PR writes it from
the engine's behaviour and its tests.

## Units

v1 carries each port's and parameter's **declared** unit and quantity as the CDL source spells
them (`s`, `K`, `1`), the spelling the corpus and the engine's §7.10 check use. It also supplies the
**scale-factor table** decision 13 waited for, and nothing more: a closed table of non-base symbols,
each an exact positive integer multiple of a base unit, with no offsets. v1's table is time only:

| Symbol | Base | Factor |
| --- | --- | --- |
| `min` | `s` | 60 |
| `h` | `s` | 3600 |
| `d` | `s` | 86400 |

`delayTime = 1 h` becomes legal in the text form. The model literal keeps the suffix and the
written number (so `fmt` still prints `1 h`) and carries the base value, computed by scaling the
written decimal exactly and rounding once to the nearest `f64` (`1.1 h` is `3960`; multiplying
the parsed `f64` would give `3960.0000000000005`). The wire form carries the base value, as today. One check comes with it, at
stage 1: a suffix on a block parameter whose declared unit is non-empty must reduce to that unit
(`delayTime = 5 K` and `delayTime = 1 Hz` are refused, author-only code `unit-suffix-mismatch`), and
a *scaled* suffix on a parameter with no declared unit is refused with the same code, because
nothing says what it scales to. A base-unit suffix on an undeclared parameter keeps its M2
behaviour (annotation only). Everything else about units is informational in v1.

## Decisions

**1. The generation source is `CXF-Core.jsonld`, not `-o doc`.** Generated by
`modelica-json -o cxf --elementary --cxfCore -p` over the Buildings `CDL` package at the two
recorded pins *(amended 2026-11-09, issue #500: `-p`, pretty print, is part of the command; without
it the output is 585,029 bytes of the same graph and does not match the vendored 682,615; the
vendored README has the whole command)*. Rejected: `-o doc` (a sequence-document generator; empty or failing on elementary classes);
reading `.mo` declarations in a generator (a `.mo` parser by another name, and the README's five
extractor defects are the warning); `Documentation(info=…)` HTML (prose, not per port; turning it
into plain text is a lossy derivation); the vendored G36 `cxf/` (instance descriptions, not class
ones). Why: it is the upstream translator's own per-connector and per-parameter output, it is
byte-stable, and reading JSON-LD as data is what the engine already does. *(Ruled 2026-11-09:
accepted; the plan's `-o doc` wording is corrected in the owner's planning record, and this note
is where the correction is stated.)*

**2. CXF-Core is vendored, at the existing pins, as a derived file.**
`third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld` (683 KB), with its regeneration
command, the two pins and the date in the vendored README, and in the hash manifest. Node and
modelica-json never run in CI. Rejected: running modelica-json in CI (a Node toolchain and a network
clone of the whole Buildings `CDL` package, because the vendored cone omits `package.mo`,
`Interfaces` and `Types`); vendoring modelica-json's root copy (a different Buildings commit). Why:
the same contract as the 44 checked-in `cxf/` translations; a pin advance regenerates both.
*(Ruled 2026-11-09: accepted, with three additions. The vendored README records the regeneration
command, both pins, the date and the file's sha256. Byte-identity of a regeneration across
operating systems and Node versions is unverified (one macOS host, two runs), so a pin advance
re-vendors the file and re-blesses the manifest and `generated.json` rather than assuming
reproducibility. The derived file is covered by the same Buildings licence line as the 44 `cxf/`
translations.)*

**3. The generator is a regenerate-and-diff test in `oce-vocab`.** It reads the vendored CXF-Core
at test time, walks the catalog's class, port and parameter names through `oce-api`, looks each up
by `@id` (`S231:Buildings.Controls.OBC.<class>.<name>`), and renders `generated.json`; `OCE_BLESS=1`
rewrites it. Rejected: a `tools/vocab-gen` cargo workspace (`tools/golden-gen` stands apart to stay
free of engine crates for anti-tautology; a projection of engine names wants the opposite, and a
second workspace needs its own CI job); a Python script (no access to the catalog's names, and a
second implementation of the naming contract). Why: the leverage lane runs it on every leverage and
engine PR, so a renamed port or a pin advance that changes a description fails the PR that causes
it, as `oce-blocks`' registry-manifest test does for the registry.

**4. The catalog drives the walk.** Names come from `oce-api`; CXF-Core only supplies text by id.
Rejected: walking CXF-Core's `hasInput`/`hasOutput`/`hasParameter` (it drops `discrete` connectors
and enumeration parameters, and lists PID's internals). Why: the engine's names are the contract;
the walk also proves every catalog name has a source entry or a curated one.

**5. Units are inverted from modelica-json's own unit table.** A closed, pinned inverse of the
`getQudtUnit` table (`unit:SEC` to `s`, `unit:UNITLESS` to `1`, …) plus the `S231:` fallback, which
carries the original string; any other identifier fails the generator. Quantities are inverted the
same way, and a quantity whose inverse is not unique refuses. A test checks each generated unit
against its declaration in the vendored `.mo` source as a substring (`unit="s"`), the same kind of
data read `class_descriptions_match_vendored_sources` already does. Rejected: carrying QUDT
identifiers (M8 owns the QUDT mapping, and the corpus and the engine use Modelica strings).

**6. Two files: `generated.json` and `curated.json`.** Regeneration writes only the first; people
write only the second. Curated text may add notes, and may add a description only where the
generated one is `""`; replacing a generated description is refused by a test in v1. Rejected: one
file with `generated`/`curated` fields per entry (every regeneration rewrites the file people edit,
and a merge conflict there is where curation gets lost); provenance markers inside strings. Why: a
reviewer sees a regeneration and a curation as separate diffs, and neither can clobber the other.
*(Ruled 2026-11-09: accepted, additive only in v1 with the refusing test; an override carrying a
required reason is v2.)*

**7. Notes are typed.** `kind` is one of `convention` (a library convention, such as
`delayOnInit`), `pitfall`, `port-order` (`Subtract` gives `u1 - u2`), `loop` (whether the class
breaks an algebraic loop), `units`; `subject` names a port or parameter or is `""` for the class;
`evidence` says where the claim comes from and is shown to reviewers, not agents. Rejected: free
text (an MCP tool or a repair message cannot pick the relevant notes). Why: the prototype's
system prompt had to hand-write exactly these four kinds (its rules 5, 7 and 9, and the
`Discrete.Sampler` failures); here they live once, per class.

**8. Defaults and ranges are never copied into the data.** They are rendered from the catalog when
a view is built. Rejected: carrying CXF-Core's `value` and `min` (unevaluated expressions, and a
second copy that can drift from the rules the engine enforces). Why: the engine's rules are the
truth an author is judged against.

**9. The Documentation HTML is not ingested in v1.** Readers get the links. Rejected: an
HTML-to-text pass (lossy, images and `modelica://` links, and agent text must be plain). Why: a
curated summary, where one helps, is v2 work and reviewable as text.

**10. The data lives in a new leverage crate, `oce-vocab`.** It depends on `oce-api` (and
`serde`/`serde_json`), embeds both files with `include_str!`, parses them once, and exposes
`vocabulary()` and a joined `class_vocab(class_path)`. `oce-author` and `oce-cli` depend on it.
Rejected: tables in `oce-blocks` like `class_descriptions.rs` (grows the `no_std` core and
firmware images with prose the engine never reads); data in `oce-api` behind a feature (a change to
the closed package and feature contract, for data no host loop needs); data only inside `oce-cli`
(the library's card generator and a future MCP server are libraries). Why: leverage-lane only, the
facade and its catalog JSON stay byte-identical, and the 700-line cap does not apply to JSON. The
accessor parses embedded bytes a test proves well formed, the same posture as `catalog()`.

**11. `oce-api` gains nothing in v1.** `catalog()`, `class_metadata()` and `CATALOG_JSON` are
unchanged, and so is the facade's sentence "units and quantities are not available in this
catalog". Rejected: adding `units` to `ClassParameter` (engine surface for non-behaviour). Why: the
plan's "serve through `catalog()`" is met by `oce catalog` and `class_vocab()`, which join the
catalog with the vocabulary. *(Ruled 2026-11-09, the orchestrator's reading, which the owner may overrule: no facade change in v1. The
plan's "serve through `catalog()`" is read as the catalog surface: `oce_api::catalog()` for
behaviour, `oce_vocab::class_vocab()` joined over it for text, `oce catalog` for people and agents.
A facade accessor is a separate decision when an MCP server or `oce-bacnet` needs one; the owner
may overrule this reading.)*

**12. `oce catalog` serves the block vocabulary; `oce vocab` serves the language.** On the CLI
branch, `oce vocab` lists the text form's keywords, tags, base units, codes and the enumeration
table. v1 adds the scale-factor table there. `oce catalog CLASS` gains per-port and per-parameter
`description`, `unit`, `quantity` and `range`, the notes and the links; the class list keeps one
line per class. Both keep `--json` in the strict profile. Rejected: block text under `oce vocab`
(two commands answering "what does PID take"). Why: the CLI already split them that way.
*(Landed in GitHub #PRNUM, issue #500, with three readings the implementation had to make. `range` is
per parameter only: the catalog has no rule about a port's value, so a port carries `description`,
`unit` and `quantity`. A range is one line per catalog rule that constrains a value (bounds,
relations, relations that only warn, element bounds); rules about type, array shape and port
structure render nothing, and the rendering lives in `oce-vocab` (`parameter_range`), where the
joined view and any other reader share it. `oce catalog --json` without a class gives every class
the single-class shape, notes included, because the benchmark harness reads that listing; the
report schema stays `oce-report/1`, since keys were added and none changed. A note's `evidence` is
never printed, which a test holds over every class.)*

**13. The text form does not show the vocabulary inline in v1.** `fmt` stays a canonicaliser and
inserts nothing; hover text needs an editor and inline annotation needs `doc`, both outside M2.
Rejected: `fmt --annotate` writing comments (comments are the author's, and a regenerated
annotation would churn every file on a vocabulary change).

**14. Scale factors: time only, exact, at stage 1 (amends authoring decision 13).** The table above,
exact decimal scaling with one rounding, and `unit-suffix-mismatch` against declared units.
Rejected: `degC`/`degF` (affine; `20 degC` in a setpoint is the classic silent error, and an
offset is not a scale); `%` and prefixed SI units (`kPa`) in v1 (no corpus need yet; they extend
the same table later); a unit check on connections (ports are mostly unit-free, block-port unit
attributes are refused, and the engine's §7.10 check covers declared units; a second semantics is
what authoring decision 4 rules out). Why: the one thing decision 13 deferred, delays and periods
written in hours, with no new silent conversion. *(Ruled 2026-11-09: accepted; `degC`/`degF`, `%`
and prefixed SI units stay refused. `unit-suffix-mismatch` is a stage-1, author-only parse-meaning
check, and the dated amendment of authoring decision 13 in `authoring-layer.md` lands with the
implementation PR, not with this note.)*

**15. Coverage targets.** v1: an entry for every one of the 133 classes; a description for every
catalog port (by connector) and parameter, generated for all but the six curated ones; every
declared unit and quantity; links for 132; a seed of notes covering the prototype's hand-written
rules and the library's `delayOnInit` convention. v2 (work plan): curated one-liners where the
upstream one reads poorly, unit conventions for unit-free ports, notes complete, served to MCP and
hover text; M8's gate then requires "no generated-only entries left uncurated", which v1 makes
measurable (an entry is curated when `curated.json` names it).

**16. The benchmark harness is not part of this work.** The vocabulary is what a harness consumes;
measuring gate item 4 needs the benchmark data (#457), a harness and model access, none of which is
CI. Rejected: building a harness in this PR series (out of the item's scope, and the prototype is
frozen). Why: see [The benchmark gate](#the-benchmark-gate). *(Ruled 2026-11-09: the harness is
an `open-control-library` issue consuming `oce catalog --json` (standing rule 7), and gate item 4
is deferred to the milestone that lands it; the owner confirmed this on 2026-11-09.)*

## Serving to agents

The fields an agent uses, from the frozen prototype's tools and prompt:

| Need | Prototype evidence | v1 field |
| --- | --- | --- |
| Which classes exist, one line each | `palette` tool: "Authorable CDL block classes … one line each" | class path, one-liner |
| Ports in declaration order with kinds | `palette_listing`: `in(u1: Real, …) -> out(y: Boolean)` | resolved ports, kind, meaning |
| Required parameters and their types | `palette_listing`: `name* (Real)` | type, default (`required`), settable |
| What a parameter means, its unit | not offered: task hints carried `unit` and `meaning` per point instead | parameter meaning, unit, quantity, range |
| Port-order semantics | prompt rule 5 ("Reals.Subtract gives u1 - u2") | `port-order` notes |
| Library conventions | prompt rule 7 (`delayOnInit = true`); the cards' params map hid 177 of 203 values | `convention` notes |
| Loop breaking | prompt rule 9 and `feedback.rs` (`LOOP_BREAKERS`) | `loop` notes |
| Repair text | `feedback_message`: code, subject, detail; "check signs, thresholds, timing and port order" | finding codes unchanged; a repair message may append the notes whose `subject` the finding names |

The prototype reached 130 to 132 of 137 after repair with a palette that had **no** descriptions
at all, so the vocabulary adds information to that baseline rather than replacing any. It must at
least carry the three hand-written rules, so a harness can drop its hand text without losing them.
Rules using `Discrete.Sampler` were the weakest group (48% first try, 71% after repair); a
`pitfall` note there is the first curation candidate, written from the engine's tests, not guessed.

## The benchmark gate

The M2 row reads: the benchmark harness runs on the new wire form and vocabulary with a repair rate
at or above the v0 result (130 to 132 of 137 after repair). The v0 record is three runs of all 137
rules, tier A, validator-only repair feedback, up to three attempts, medium effort: 132, 130 and 131
after repair on engine `fa5cb19`. A comparable measurement consumes `oce catalog --json` (or
`class_vocab()`) for the palette and notes, the wire form and its schema for the reply, and
`oce validate` findings for repair; it fixes the same settings and reports each run.

What it cannot run on today: the 137 rules and their vectors live in the library and the frozen
prototype, and #457 (benchmark specs out of the prototype) is open. A harness needs model access and
is not a CI job.

**Gate item 4 is deferred to the milestone that lands the harness, and M2 closes on gate items 1
to 3 with the vocabulary shipped** *(ruled 2026-11-09 and confirmed by the owner the same day; it
changes the M2 gate, and tracking issue #455 carries the deferral)*. The harness is an
`open-control-library` issue consuming `oce catalog --json`. Its pass criterion *(ruled
2026-11-09)*: three runs at the v0 settings, at least 130 of 137 after repair in each run, and a
mean of at least 131.

## Test plan

- **Freshness.** The regenerate-and-diff test leaves `generated.json` byte-identical; running it
  twice gives the same bytes; a deliberately perturbed CXF-Core (a changed description, a dropped
  connector, an unknown unit identifier) fails with a typed error naming the subject.
- **Coverage, both directions.** Every authorable catalog class has exactly one entry and no entry
  names an unknown or reserved class; every catalog port and parameter name maps to exactly one
  connector or parameter entry and every entry maps back; for each width-driven class at widths 1,
  2 and the maximum, every resolved port name maps to exactly one connector by the naming contract;
  every catalog port and parameter has a non-empty description after the merge.
- **Curation discipline.** `curated.json` names only existing classes and subjects; a curated
  description exists only where the generated one is `""`; note kinds are from the closed set;
  `curated.json` is in canonical form (a canonical rewrite leaves it unchanged).
- **Units.** Every generated unit appears as `unit="…"` in its class's vendored `.mo`; the inverse
  tables are total over CXF-Core and injective; scale-factor goldens for `1 h`, `1.1 h`, `1.5 min`,
  `2 d` and a literal past 2^53; `unit-suffix-mismatch` for a wrong base, a scaled suffix on an
  undeclared parameter, and an unknown symbol, each with its exact code and subject; `fmt` keeps
  `1 h`; wire → text → wire stays byte-stable.
- **Agent-facing text** *(ruled 2026-11-09: run over every string in both JSON files, since the
  doc-mode side output leaked a host path)*. No agent-facing string contains an absolute path, a
  UUID-shaped string, a `modelica://` link, `<html`, or an HTML tag from a closed list (`<p>`,
  `<code>`, `<a `, `<br`, `<img`, `<i>`, closing tags); comparison signs in descriptions (`u<uLow`)
  stay legal. No string is empty
  after the merge, none has leading or trailing whitespace or a newline, and each fits a bound.
- **Serving.** Goldens for `oce catalog CLASS` text and `--json` for PID, TrueDelay, Hysteresis,
  `MultiSum` at `nin = 3` and `TrueHoldWithReset`; `oce vocab` golden with the unit table; the
  joined view's range rendering for every rule kind the catalog uses.
- **Isolation.** The catalog golden, `CATALOG_JSON`, `catalog_content_id`, every class signature
  hash, the 46 fixture goldens and 422 provenance goldens stay byte-identical; emitting any corpus
  bundle gives the same bytes with and without the vocabulary linked; the content id of every
  emitted bundle is unchanged.

## Dependencies and sequencing

1. **After the importer (#496) and the CLI (issue #489, PR #491)**, which touch `oce-author`'s text form and both
   CLI commands this work extends.
2. **Vendoring and `oce-vocab` in one PR**: CXF-Core under `third_party/` with the README rows and
   the hash-manifest re-bless, the crate with both data files, the generator test, the coverage
   tests and the seed curation. A `third_party` change and a new crate both run the determinism
   lane. *(Landed in GitHub #511.)*
3. **CLI PR**: `oce catalog` and `oce vocab` as decision 12. *(Landed in GitHub #PRNUM. The
   scale-factor table itself is `oce_vocab::SCALE_FACTORS`, so step 4 reads it and does not
   restate it; until step 4 lands the text form still refuses `1 h`.)*
4. **Text-form PR**: scale factors and `unit-suffix-mismatch`, amending authoring decision 13 with a
   dated ruling.
5. **Closeout** records coverage against decision 15; gate item 4 follows #457.

## Out of scope

- Inline vocabulary in the text form, editor hover text, `oce doc` and the MCP server itself.
- Plain-text summaries of the Buildings `Documentation` HTML.
- Unit conventions for unit-free ports, affine and prefixed units, `%`, and any unit check on
  connections.
- QUDT, Brick and other semantic mappings (M8).
- G36 composite classes; their instance descriptions in `cxf/` are a later source.
- The benchmark harness and the move of its data (#457).
- Any change to `oce-api`'s catalog, the content id or the class signature hash.

## Questions for review (ruled; see below)

1. **"Serve through `catalog()`".** The note serves the vocabulary from a leverage crate joined with
   the catalog, and leaves `oce_api::catalog()` unchanged (decisions 10 and 11). Did the owner mean
   the facade function itself? Recommendation: no facade change in v1; revisit when an MCP server
   or `oce-bacnet` (M9) needs it, and then as a separate accessor, not the revision-1 JSON. *(Ruled 2026-11-09: no facade change, the orchestrator's reading, which the owner may overrule.)*
2. **The generation source.** CXF-Core from `-o cxf --elementary --cxfCore -p` instead of the plan's
   `-o doc`, vendored at the existing pins (decisions 1 and 2). Recommendation: accept, and amend
   the plan's wording. *(Ruled 2026-11-09: accepted, with the decision-2 additions.)*
3. **Generator location.** A regenerate-and-diff test in `oce-vocab` rather than `tools/vocab-gen`
   (decision 3). Recommendation: accept. *(Ruled 2026-11-09: accepted.)*
4. **Two files.** `generated.json` and `curated.json`, curated text additive only in v1 (decision
   6). Should v1 allow a curated description to replace a poor upstream one (for example
   Hysteresis's "if y=false and u>uHigh, switch to y=true")? Recommendation: not in v1; v2 adds an
   override with a required reason. *(Ruled 2026-11-09: accepted.)*
5. **Scale factors.** Time only (`min`, `h`, `d`), exact, plus `unit-suffix-mismatch` (decision
   14). Add `%` or `kPa` now? Recommendation: time only; `degC` stays refused until a design pass
   on affine units. *(Ruled 2026-11-09: accepted.)*
6. **The suffix check is stage 1 and author-only.** It pre-empts no engine code and the engine never
   sees declared block-port or block-parameter units. Accept it as a parse-meaning check rather than
   a second semantics? Recommendation: accept; refusing is reversible, a wrong conversion is not. *(Ruled
   2026-11-09: accepted; the authoring-note amendment lands with the implementation PR.)*
7. **Gate item 4's pass criterion.** Recommendation: three runs, same settings as the v0 record, at
   least 130 after repair in every run and a mean of at least 131. *(Ruled 2026-11-09: accepted.)*
8. **Where the benchmark harness lives.** The prototype is frozen; #457 moves the data but not a
   harness. Recommendation: a harness issue in `open-control-library` (rule 7: rules and their
   vectors live there) consuming `oce catalog --json`; if it cannot land in M2, gate item 4 moves to
   the milestone that lands it, and M2 closes on items 1 to 3 with the vocabulary shipped. *(Ruled 2026-11-09 and confirmed by
   the owner: gate item 4 deferred.)*
9. **Curation seed size.** v1 seeds notes for the prototype's three rules (eleven classes) and
   the six missing descriptions. Recommendation: that seed, plus a `Discrete.Sampler` pitfall once
   written from the engine's tests; everything else is v2. *(Ruled 2026-11-09: accepted; each note carries `evidence`.)*
10. **Sequencing.** Vendoring and `oce-vocab` together, then CLI, then the text form. Recommendation:
    as listed; the vendored file has no reader without the crate. *(Ruled 2026-11-09:
    accepted, after #496 and #491; implementation issues are cut from this note after it merges.)*

## Review rulings

The [review comment on GitHub #497](https://github.com/jscott3201/open-control-engine/pull/497#issuecomment-6744139683)
(2026-11-09) ruled on the ten questions; the note is amended to match. Two rulings are
owner-visible: the owner confirmed 8 on 2026-11-09, and 1 stands as the orchestrator's reading,
which the owner may overrule.

1. **"Serve through `catalog()`":** no facade change in v1; the phrase means the catalog surface
   (`oce_api::catalog()`, `oce_vocab::class_vocab()` joined over it, `oce catalog`). A facade
   accessor is a later, separate decision. *The orchestrator's reading; the owner may overrule it.*
2. **Source:** CXF-Core, vendored at the existing pins, accepted. The vendored README records the
   command, both pins, the date and the sha256; cross-OS and cross-Node byte-identity is unverified,
   so a pin advance re-vendors and re-blesses; the Buildings licence line covers the file.
3. **Generator:** a regenerate-and-diff test in `oce-vocab`, accepted.
4. **Curation:** additive only in v1 with the refusing test; an override with a required reason is
   v2.
5. **Scale factors:** time only (`min`, `h`, `d`), exact; `degC`/`degF`, `%` and prefixed SI stay
   refused.
6. **`unit-suffix-mismatch`:** a stage-1, author-only parse-meaning check, accepted; the dated
   amendment of authoring decision 13 lands with the implementation PR.
7. **Gate item 4 criterion:** three runs at the v0 settings, at least 130 of 137 after repair in
   each, mean at least 131.
8. **Harness and gate item 4:** the harness is an `open-control-library` issue consuming
   `oce catalog --json`; gate item 4 is deferred to the milestone that lands it, and M2 closes on
   items 1 to 3 with vocabulary v1 shipped. *Confirmed by the owner 2026-11-09; #455 carries the deferral.*
9. **Curation seed:** the prototype's three rules over eleven classes, the six missing
   descriptions and the `Discrete.Sampler` pitfall written from the engine's tests, each note with
   `evidence`.
10. **Sequencing:** vendoring and `oce-vocab` in one PR, then the CLI PR, then the text-form PR,
    after #496 and #491; implementation issues are cut from the amended note after it merges.

The same review added two requirements, now in the note: the agent-facing-text test runs over every
string in both JSON files (no absolute path, no HTML tag, no `modelica://` link), and both data files
are strict JSON under the authoring wire profile with sorted keys.
