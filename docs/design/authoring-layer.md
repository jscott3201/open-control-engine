# Authoring layer (M2)

**Design note, written before any M2 code.** It fixes what the authoring layer is, the typed model,
its text and wire forms, the validator, the CXF emitter and the `oce` CLI. Decisions 1 to 9 were
settled by the owner and are stated here, not reopened. Decisions 10 onwards were made while
drafting and ruled on by the architect pass (see [Review rulings](#review-rulings)). The engine-side
prerequisite is GitHub #440 (resolved port names, descriptions and a settable flag in the catalog).

The milestone goal, from the owner's work plan:

> A person or an agent writes a sequence without touching JSON-LD. One typed authoring model, two
> serializations, one deterministic emitter to CXF, and a vocabulary that gives every block, port
> and parameter a plain name.

The plan's gate has four items: (1) every library rule and the benchmark specs import, emit and
load through `oce-cxf` with the same canonical content id as their committed CXF; (2) every bundle
round-trips wire → text → wire byte-stably; (3) every engine refusal in the library corpus and the
benchmark failure set is reported by the validator, with the same code; (4) an agent benchmark on
the new wire form and vocabulary repairs at least as many rules as the previous prototype (130 to
132 of 137). Item 4 is discussed under [Dependencies and sequencing](#dependencies-and-sequencing).

## What the authoring layer is, and is not

It is a set of **leverage crates** (`crates/leverage/`, see its `README.md`): they sit above the
facade and depend on `oce-api` only, never on a core crate.

- **`oce-author`** (library): the typed model, a Rust builder over it, the text form (parser and
  canonical printer), the wire form (strict JSON and its schema), the validator and the emitter.
- **`oce-cli`** (binary crate, executable `oce`): `fmt`, `validate`, `emit`, `check` over
  `oce-author`. A terminal UI is a planned follow-on, not part of M2's gate.

It is **not** a second engine. It never evaluates an expression, never schedules, never decides
single assignment or an algebraic loop, and never opens a network listener. The only path from an
authored sequence to the engine is CXF bytes imported by `oce-cxf` through `Engine::load_cxf`. The
engine judges the result; the authoring layer reports that judgement in the author's terms. Nothing
here changes [the §7.17 seam](../architecture.md#the-organising-idea-the-cdl-717-non-computational-seam)
or anything the engine will never do; the binary carve-out is in decision 9.

## The authoring model

One sequence is one **flat, single-root** document: a root composite that contains leaf blocks
and nothing else. This is Rule 2 of the [composite subset](../cxf-composite-subset.md#rule-2--single-top-root-rejects-compositeroot-count)
in its simplest accepted form and the dialect the library already uses. Nested composites,
`hasInstance` members, conditional components and array connectors are not authorable in M2.

| Element | Content |
| --- | --- |
| Sequence | `name` (identifier), `extends` (opaque provenance id or none, decision 5) |
| Parameter | sequence-level `name`, `type` (Real, Integer, Boolean), literal `value`; lowered onto the root block (decision 3) |
| Input / output | boundary `name`, `type` (Real, Integer, Boolean), optional `quantity` and `display_unit` (decision 21), and a `SemanticPoint` slot (decision 2) |
| Block | instance `name`, catalog `class_path`, ordered parameter assignments |
| Assignment | parameter `name` and a value: a literal, an enum member, a string, or a reference to a sequence parameter |
| Connection | `source -> target`; an endpoint is a boundary name or `block.port` |

Every list is an ordered vector, kept in authored order. Names are identifiers
(`[A-Za-z_][A-Za-z0-9_]*`, at most 64 bytes, no dots, not a Modelica reserved word or a text-form
keyword) and are unique across parameters, inputs, outputs and blocks, because each becomes one
IRI under the root. Port names are never authored; they come from the catalog (#440).

`SemanticPoint` is a closed struct of optional fields: `name`, `label`, `unit`, `brick`, `s223`,
`qudt`, `haystack` (a list of tags) and `provisional`. Literals are tagged: `Integer(i64)` when the
token has no fraction or exponent, `Real(f64)` otherwise, `Boolean`, `Text`, `EnumMember` (the
member's type and name). A Real parameter accepts an Integer literal, as CDL parameter validation
widens it; an Integer parameter refuses a Real literal.

## Text form

A terse line grammar, hand-written recursive-descent parser and canonical printer, no third-party
parser (decision 1). One statement per line except a block's parameter list, which may span lines.

| Production | Grammar |
| --- | --- |
| file | `sequence-stmt extends-stmt { param-stmt } { in-stmt } { out-stmt } { block-stmt } { connection }` |
| sequence-stmt | `"sequence" name` |
| extends-stmt | `"extends" ( "none" \| provenance-id )` |
| param-stmt | `"param" name ":" type "=" literal` |
| in-stmt / out-stmt | `("in" \| "out") name ":" type { tag }` |
| block-stmt | `"block" name "=" class-path [ "(" [ assign { "," assign } ] ")" ]` |
| assign | `name "=" ( literal \| name \| enum-member \| list )` |
| connection | `endpoint "->" endpoint`, endpoint = `name [ "." name ]` |
| tag | `"@unit" atom \| "@quantity" atom \| "@displayUnit" atom \| "@brick" atom \| "@s223" atom \| "@qudt" atom \| "@haystack" atom \| "@name" string \| "@label" string \| "@provisional"` |
| literal | `number [ unit-suffix ] \| "true" \| "false" \| string` |
| list | `"[" [ item { "," item } ] "]"`, items literals or lists (vectors and matrices) |
| comment | `#` to end of line, anywhere |
| name / atom | an identifier as above; an atom is printable ASCII with no space or `#`. `@haystack` may repeat |

The `file` production is the canonical order; after `extends` the parser accepts the other
statements in any order and `fmt` regroups them (decision 12). `type` is `Real`, `Integer` or
`Boolean`. `class-path` is the catalog `class_path` verbatim
(`CDL.Reals.Hysteresis`). An `enum-member` is `Type.Member` with `Type` the last segment of the
enum class (`SimpleController.PI`); the catalog resolves it to the full member path. A bare `name`
on the right of `=` is a reference to a sequence parameter. A string is double-quoted with `\"`,
`\\` and `\n` escapes only.

**Unit-literal sugar.** `86400 s` is text-only: the suffix never reaches the wire form, the
emitter or the engine, and the value is the number as written (decision 3, refined by decision 13).
Comments are text-only in the same way: the model keeps them so `fmt` loses nothing, and the wire
form and the emitter drop them.

**Canonical printer.** `fmt` is parse then print. The printer emits statements in the section
order of the `file` production, keeps authored order inside every section, puts one space around
`=`, `->` and `:`, prints a block on one line when it has at most two assignments and fits in 100
columns and otherwise one assignment per indented line, prints integers bare and reals in the
shortest round-trip spelling (Rust's `{:?}` for `f64`, so `1.5`, `0.0`, `1e-7`), collapses blank
runs to one line, and keeps comments attached to the statement that follows them. Within a section
it never reorders anything (decision 6; decision 12 states what `fmt` changes beyond whitespace).

**Bounds.** The parser refuses, with a typed `ParseError { line, column, kind }` and never a panic:
input above 4 MiB, more than 100,000 lines, a line above 4,096 bytes, a token above 256 bytes, list
nesting deeper than 2, more than 65,536 statements of one kind, and non-UTF-8, control characters or
a byte-order mark. Each limit is pinned at its boundary: accepted at the limit, refused one past.

**Round-trip properties.** For every canonical text `t`, `print(parse(t)) == t` byte for byte; for
every model `m`, `parse(print(m)) == m`; and `fmt` is idempotent. Tests enumerate models
deterministically (no RNG, per [TESTING.md](../../TESTING.md#conventions)) and run over every
corpus bundle.

### Worked example

An economizer enable: enable when the outdoor-air temperature is at least a deadband below the
cut-off.

```text
# Economizer enable on outdoor-air temperature with a deadband.
sequence econ_enable
extends none

param dTHys : Real = 1 K

in TOut : Real @unit K @quantity ThermodynamicTemperature @displayUnit degC
in TOutCut : Real @unit K @provisional
out ecoEna : Boolean @brick Enable_Command

block sub = CDL.Reals.Subtract
block hys = CDL.Reals.Hysteresis(uLow = 0.0 K, uHigh = dTHys)

TOutCut -> sub.u1
TOut -> sub.u2
sub.y -> hys.u
hys.y -> ecoEna
```

With the default namespace the emitter writes these `@graph` nodes, in this order:

| # | `@id` (after `urn:oce:author:econ_enable`) | Node |
| --- | --- | --- |
| 1 | (root) | `S231:Block`: `hasParameter [dTHys]`, `hasInput [TOut, TOutCut]`, `hasOutput [ecoEna]`, `containsBlock [sub, hys]` |
| 2 | `.dTHys` | `S231:Parameter`, `S231:value` typed `xsd:double` `1.0` |
| 3 | `.sub` | `http://example.org#Buildings.Controls.OBC.CDL.Reals.Subtract`, `hasInput [u1, u2]`, `hasOutput [y]` |
| 4–6 | `.sub.u1`, `.sub.u2`, `.sub.y` | port nodes in catalog order; `.sub.y` `isConnectedTo [hys.u]` |
| 7 | `.hys` | `…CDL.Reals.Hysteresis`, `hasParameter [uLow, uHigh]`, `hasInput [u]`, `hasOutput [y]` |
| 8–9 | `.hys.uLow`, `.hys.uHigh` | `uLow` typed `xsd:double` `0.0`; `uHigh` the reference string `"dTHys"` |
| 10–11 | `.hys.u`, `.hys.y` | `.hys.y` `isConnectedTo [ecoEna]` |
| 12–13 | `.TOut`, `.TOutCut` | `S231:RealInput` with `unit`, `quantity`, `displayUnit` where authored; `isConnectedTo [sub.u2]` / `[sub.u1]` |
| 14 | `.ecoEna` | `S231:BooleanOutput` |

The `@brick` and `@provisional` tags stay in the model and both forms and are not emitted
(decision 2). `hys.uHigh` reads `dTHys` from the root's scope: in the flat dialect the root is a
composite and a leaf parameter value resolves enclosing-first under
[Rule 5](../cxf-composite-subset.md#rule-5--parameter-scope-inheritance-rejects-compositearray-parameter-compositedeclaration-cycle-compositeduplicate-declaration),
as corpus fixture `accepted/leaf_identity_parameter_modification.jsonld` already shows.
`oce-expr` grounds it at load; `oce-author` never evaluates it.

## Wire form

Strict JSON for agents and structured-output decoders (decision 8). Every object is closed
(`additionalProperties: false`), every property is required, an absent value is `""`, `[]` or
`false`, and the profile uses no `oneOf`, `allOf`, `pattern` or `minItems`. Parameters are lists of
`{name, value}`; below the root, `value` is one of five shapes: a number, a boolean, a string, a
reference `{"ref": "<sequence-param>"}` or an enum member `{"member": "<Type.Member>"}`. The two
tagged objects are closed, and the schema lists the five shapes under `anyOf`. Connections are
`"a.y -> b.u"` strings, parsed into typed endpoints at the model boundary.

```json
{
  "sequence": "econ_enable",
  "extends": "",
  "params": [{ "name": "dTHys", "type": "Real", "value": 1 }],
  "inputs": [{ "name": "TOut", "type": "Real", "quantity": "ThermodynamicTemperature",
               "display_unit": "degC",
               "semantic": { "name": "", "label": "", "unit": "K", "brick": "", "s223": "",
                             "qudt": "", "haystack": [], "provisional": false } }],
  "outputs": [],
  "blocks": [{ "name": "hys", "class": "CDL.Reals.Hysteresis",
               "params": [{ "name": "uLow", "value": 0.0 },
                          { "name": "uHigh", "value": { "ref": "dTHys" } }] }],
  "connections": ["sub.y -> hys.u"]
}
```

(Abridged; a real document lists every input, output, block and connection.) A plain string `value`
is always text and never depends on the catalog: a reference or an enum member is written as its
tagged object, so any parameter kind, String included, may take a reference (decision 11). An enum
member uses the text form's `Type.Member` spelling, e.g. `{"member": "SimpleController.PI"}`.

The JSON Schema is derived from the wire DTOs with `schemars`, a dependency of `oce-author` only,
never of an engine crate. The `class` enum is generated from the non-reserved class paths of
`oce_api::catalog()` (133 today, well under the 1,000-value cap of common decoders). A test
regenerates the schema and compares it byte for byte with the published file, and a second test
walks the schema and refuses any keyword outside the profile.

## Validator

`validate(&Sequence) -> Vec<Finding>`, where a `Finding` is `{ code, severity, subject, span,
detail, origin }`. `subject` is a model path (`hys`, `hys.uHigh`, `TOut`, `connection[2]`), `span`
the text location when the input was text, and `origin` is `author` or `engine`. The list is sorted
by the subject's authored position, then code, then detail, and deduplicated.

It runs in two stages (decision 4):

1. **Author-side**, from the model and the catalog alone: names, the sequence shape, edge syntax and
   endpoints, class lookup, parameter lookup, kinds and catalog rules, references, arity and port
   names from the ports #440 synthesizes, duplicates, `SemanticPoint` and `extends` syntax.
2. **Engine-side**, only when stage 1 found no error: emit, load the bytes in a throwaway
   `Engine::new()`, and map every diagnostic back to the model. Subjects are IRIs of the form
   `{namespace}{sequence}.{local}`, so stripping the root prefix yields the model path; a
   `connector#N` subject is mapped through the loaded topology; anything unmappable stays attached
   to the sequence with the raw subject in `detail`.

Every engine **warning** becomes an error finding with the engine's code. The emitter's contract is
a warning-free import ([acceptance precondition 4](../cxf-composite-subset.md#acceptance-preconditions)),
so an unused boundary input, which the engine reports as warning `unused-boundary-input`, is an
error here (decision 14).

**Finding codes.** The vocabulary starts from the prototype's eighteen codes, renamed to this
repo's conventions. Where the engine has a `DiagCode` for the defect, the finding uses that string
and the table says **engine**; author-only codes are listed as such, with the engine code they
pre-empt where one exists. The table is a test fixture: gate item 3 is a table-driven test.

| Finding code | Owner | Prototype code | Stage | Engine code pre-empted |
| --- | --- | --- | --- | --- |
| `empty-sequence` | author-only | `no-blocks` | 1 | `malformed-document` (zero roots) |
| `invalid-name` | author-only | `invalid-name` | 1 | — |
| `duplicate-name` | author-only | `duplicate-name` | 1 | `duplicate-id` |
| `class-not-found` | engine | `unknown-class` | 1 | — |
| `class-not-authorable` | author-only | `class-not-authorable` | 1 | — (reserved classes) |
| `unknown-parameter` | author-only | `unknown-param` | 1 | none: the engine ignores an unknown name (#467; amended 2026-11-09) |
| `parameter-not-settable` | author-only | `unsupported-param` | 1 | — (reserved classes only; #440's settable flag) |
| `duplicate-parameter` | author-only (amended 2026-11-09) | `duplicate-param` | 1 | `duplicate-id` (the repeated parameter node) |
| `missing-required-parameter` | engine | `missing-required-param` | 1 | — |
| `parameter-kind-mismatch` | engine | `param-type-mismatch` | 1 | — |
| `parameter-out-of-range` | engine | `invalid-width` | 1 | — (catalog rules, widths included) |
| `non-finite-parameter` | engine | — | 1 | — |
| `unknown-enum-literal` | author-only (amended 2026-11-09) | — | 1 | `grounding-failed` |
| `unknown-parameter-reference` | author-only | — | 1 | `grounding-failed` |
| `malformed-connection` | author-only | `malformed-edge` | 1 | — |
| `unknown-source` | author-only | `unknown-source` | 1 | `unresolved-reference` |
| `unknown-target` | author-only | `unknown-destination` | 1 | `unresolved-reference` |
| `unknown-port` | author-only | — | 1 | `unresolved-reference` (no such port node; amended 2026-11-09) |
| `invalid-semantic-point` | author-only | — | 1 | — |
| `invalid-provenance-id` | author-only | — | 1 | — |
| `type-mismatch` | engine | `type-mismatch` | 2 | — |
| `single-assignment` | author and engine (amended 2026-11-09: author for a target the connection list repeats, engine for the rest) | `multiply-driven`, `undriven` | 1 and 2 | `single-assignment` (stage 1 pre-empts the engine's own code for a repeated target) |
| `algebraic-loop` | engine | — | 2 | — (one finding per member) |
| `unused-boundary-input` | engine (warning → error) | `unused-input` | 2 | — |
| `undriven-boundary-output` | engine (warning → error) | — | 2 | — |
| `document-too-large` | author-only (added 2026-11-09) | — | 2 | — (the emitted document exceeds `MAX_CXF_BYTES`) |
| `load-failed` | engine (added 2026-11-09) | — | 2 | — (a refusal that carries no coded diagnostic) |
| any other `DiagCode` | engine | — | 2 | passed through verbatim |

`parameter-out-of-range` also covers, author-side only, a String value the wire form cannot carry
(a control character other than line feed, or more than 4,096 bytes as written), so every
validate-clean model reads back from its own wire bytes (GitHub #466).

`parameter-not-settable` fires only for a parameter of a reserved lowering identity: #440's
`settable` flag means "an author may assign this parameter", so every parameter of an authorable
class is settable, derived defaults included ([#440 amendment](https://github.com/jscott3201/open-control-engine/issues/440#issuecomment-6730952664)).
It stays a code distinct from `class-not-authorable`, which the same block also earns: that finding's
subject is the block (`hys`), this one's is the parameter (`hys.uHigh`), so an agent is told both
that the class cannot be authored and which assignment to remove.

A stage-1 finding that carries an engine code must agree with the engine. The parity test emits
each defect with stage 1 bypassed (a test-only path) and asserts that the engine reports the same
code at the same subject. An author-only code that pre-empts an engine code is checked the same way
against the code in the last column.

**Amended 2026-11-09 by the parity test (issue #464, GitHub #473).** Four rows changed when the
parity test first ran against the emitter: `duplicate-parameter` and `unknown-enum-literal` were marked engine codes, but
the flat emitter writes a repeated assignment as a second node with the same `@id` (`duplicate-id`)
and the engine refuses an unknown member while grounding it (`grounding-failed`, recorded on #455),
so both are author-only codes that pre-empt those engine codes; `unknown-parameter` pre-empts
nothing, because the load gate ignores a parameter name it does not read (#467); and `unknown-port`
pre-empts `unresolved-reference`, because the emitter writes every port the catalog resolves, so a
wrong port name is a dangling target rather than a port-name mismatch. Stage 2 also has two codes of
its own: `document-too-large` (the emitter's size refusal after a clean stage 1) and `load-failed`.
In every row but one the engine reports at the same model subject: stage 2 maps a block-level
message that names an assignment in backticks to that assignment, as stage 1 does. The exception
is `single-assignment`, which is same-code only: stage 1 reports the repeating connection
(`connection[N]`), the engine the input it names, which stage 2 maps to the first connection
driving it. The same review added two stage-1
rules: a target the connection list repeats is `single-assignment` at stage 1 (when this was
written the engine loaded a block output beside a boundary input without a refusal, GitHub #472;
it refuses it since GitHub #477, and stage 1 still reports the repeat first), and an Integer literal of magnitude above 2^53 on a Real
parameter is `parameter-out-of-range` (it has no exact Real, so the engine would load a value the
author never wrote).

## Emitter

`emit(&Sequence, &EmitOptions) -> Result<Vec<u8>, EmitError>` runs stage 1 first and refuses with
`EmitError::Invalid(findings)`; it never writes a document it knows to be wrong (decision 6).

**Layout.** Fixed: the root; the sequence parameters in authored order; each block in authored
order, followed by its parameter nodes in authored order and then its port nodes, inputs then
outputs, in catalog order; then the boundary inputs, then the boundary outputs, each in authored
order. Every port the catalog resolves for an instance is emitted, connected or not. `isConnectedTo`
hangs on the source node, with targets in authored connection order. Nothing is sorted: the
[ordering contract](../cxf-composite-subset.md#ordering-and-identity-contract) makes `@graph`,
`containsBlock` and target order bear on identity, so the author's order is the canonical order.

**Identity.** Instance IRIs are `{namespace}{sequence}.{local}`. The namespace defaults to
`urn:oce:author:` and `EmitOptions` overrides it for library round trips; it is never derived from
a host path or a UUID-shaped value, and must end in `#`, `/` or `:` so the canonical content id's
last-delimiter rule (GitHub #471) strips exactly the namespace. Block `@type` is the exporter's class base
`http://example.org#Buildings.Controls.OBC.` followed by the catalog class path (decision 10). The
context is `{"S231": "http://data.ashrae.org/S231P#"}`.

**Values.** A Real is a typed literal (`{"@value": "<shortest round-trip>", "@type": xsd:double}`),
an Integer a bare JSON integer, a Boolean a JSON boolean, an enum member its full member path with
`S231:isOfDataType` naming the enum class, a String the CDL string expression (`"\"text\""`), and a
reference the bare parameter name. Array parameters use the `isArray` / `numberDimensions` /
`sizeOfDimensions` form the library uses (decision 15). The emitter writes only parameters the
author assigned and never fills a default (decision 16).

**Semantics.** Boundary nodes carry `S231:unit` (from `SemanticPoint.unit`), `S231:quantity` and
`S231:displayUnit` where authored, and nothing else, until M8 fixes the CDL §7.17.2 record shape.
No Brick, 223P, QUDT or Haystack record is written.

**Writer.** The emitter owns a canonical writer: objects are ordered vectors with a fixed key order
per node kind, two-space indentation, one trailing newline, ASCII-only output with `\uXXXX`
escapes, the shortest round-trip number spelling, and relation lists always written as arrays
(decision 17). It does not imitate any other generator's spelling. The result is checked against
`oce_api::MAX_CXF_BYTES` before it is returned.

**Determinism tests.** Emitting one model twice yields identical bytes. Each permutation class of
the model (block order, port-target order, boundary order, sequence parameter order) is applied in
turn and checked against what the ordering contract says for that surface: no permutation moves
the canonical content id (`Engine::content_id`, GitHub #470), which names the loaded graph by name;
block and boundary order still move export bytes and dense ids; direct target permutations move
export bytes but leave executable compatibility unchanged; sequence-parameter order is normalized
by Rule 5, so the loaded model is identical. The [emitter test drivers](../cxf-composite-subset.md#testing-your-emitter)
(resolver-layer and full-pipeline composite-contract drivers) run over emitter output, every emitted
golden must load warning-free, and the emitted goldens are checked in and compared bit for bit.

## CLI

`oce` lives in its own leverage crate, links `oce-author` and `oce-api`, and parses its arguments
with `clap` (derive) for consistent `--help`, suggestions and shell completions (decision 20).

| Command | Does |
| --- | --- |
| `oce fmt <file> [--check] [--to text\|wire]` | Parse, then print canonically. `--check` prints nothing and fails if the file is not canonical. `--to` converts between the forms. |
| `oce validate <file> [--author-only]` | Both validator stages; `--author-only` skips the engine load. |
| `oce emit <file> [-o <out>] [--namespace <iri>]` | Full validation, then the CXF bytes; refuses to write anything on an error finding. |
| `oce check <file>` | `validate`, then the throwaway load's report: input definitions, outputs, block count and the canonical content id (`Engine::content_id`). |

`.oce` files are text, `.json` files are wire; `--from text|wire` overrides and is required on
stdin (`-`). Every command takes `--format human|json`. The JSON report is one closed object
(`tool`, `schema`, `command`, `ok`, `findings`, plus the command's result), deterministic and
documented, so an agent can branch on `code` and quote `detail`. Exit codes follow the one `oce`
rule: **0** yes (clean); **1** no (error findings, or `--check` found a non-canonical file); **2**
error (a usage error, input that cannot be parsed or loaded, an I/O failure).

**Binaries above the facade.** The architecture page says the engine is library-only, with no binary
target in `crates/`. The PR that adds this note narrows that sentence to the engine crates: leverage
crates may ship binaries above the facade, and they still run no daemon, server or network listener
(decision 9).

**TUI follow-on (direction, not M2 scope).** The owner is open to a `ratatui` terminal UI for the
authoring and validation loop, modelled on the sibling `rusty-bacnet` TUI: a status bar, a body and
a footer of the keys that work now; numbered screens; a `:` command bar and `?` help; graceful
degradation in narrow terminals; and plain `oce` opening the TUI only on an interactive terminal.
Where that TUI is "read-only until you arm a write session", this one would be: **validate and emit
never touch an engine the host runs** — every check loads a throwaway `Engine`. It needs its own
note and its own issue.

## Decisions

Decisions 1 to 9 are the owner's. Decisions 10 to 21 were proposed while drafting and settled by
the architect pass, with the owner deciding 11, 12 and 20; each ruling is recorded under
[Review rulings](#review-rulings).

**1. Text form: a terse line grammar.** A hand-written recursive-descent parser and canonical
printer, no third-party parser; `fmt` is parse then print; bounded at parse with typed errors.
Rejected: YAML, which needs quoting for `->`, `@` and units, has several spellings per datum (so a
byte-stable printer must be hand-written anyway) and brings a parser with its own hostile-input
bounds; YAML as an import-only form (more surface). Why: byte-stable round trips and hostile-input
discipline are easier to own than to inherit.

**2. `SemanticPoint` before M8: a closed slot, not emitted.** A closed struct of optional fields
(`name`, `label`, `unit`, `brick`, `s223`, `qudt`, `haystack`, `provisional`) in the model and both
forms; the validator checks syntax only; the emitter writes today's `unit`, `quantity` and
`displayUnit` attributes and no semantic records; `provisional` marks an unfilled slot. Rejected:
inventing a `semantic` JSON-LD shape now; leaving the slot out. Why: the §7.17.2 shape belongs to
M8, and the engine must round-trip it. The library CXF has no records, so "required" means "the
slot exists, and is validated if present".

**3. Parameter values: a literal or a named reference, never evaluated.** A reference names a
sequence-level parameter, lowered onto the root block so the engine's scope rules (Rule 5)
evaluate it; unit-literal sugar is text-only; enum parameters take tagged member values. Rejected:
full CDL expressions as text (late errors, no author-side typing); literals only (loses
site-parameter reuse). Why: no second semantics; `oce-expr` is the only evaluator.

**4. Validator: author-side for what only the model can see, the engine for the rest.**
Single assignment, algebraic loops and unused inputs come from emitting and loading in a throwaway
`Engine`, mapped back through the IRI scheme; a parity table pins which codes are the engine's.
Rejected: a full duplicate validator (drift); a validate-only facade API first (clean, but needs a
facade issue before M2; it may still be proposed later). Why: the same rule as decision 3.

**5. `extends` before M3: an opaque provenance id.** Stored, round-tripped and syntax-checked;
never merged and never emitted into CXF; `extends none` is explicit in text. The id grammar is
permissive: a lowercase format prefix, a colon, then printable non-space ASCII, at most 256 bytes
(`cxf:fnv1a128:<hex>` fits); M3 owns the meaning of the prefixes it defines. Rejected: a local
bundle-directory resolver (pre-empts M3); leaving it out (both forms change later). Why: the content
id hashes emitted bytes, so an unemitted id cannot move it; resolution and diff are M3's
(`oce-diff`).

**6. Emission order and IRIs: authored order, never sorted.** `fmt` normalises whitespace, section
grouping and canonical Real spelling; authored order is kept inside each section, and integers and
unit suffixes keep their spelling (decision 12); the emitter never sorts; connection targets follow
authored order; IRIs are `{namespace}{sequence}.{local}` with default `urn:oce:author:` and an
override; the emitter owns a canonical writer and does not mimic another generator's indentation.
Rejected: a canonical sort in `fmt` or the emitter (loses intent, and every reorder becomes a
`BlockId` and schedule tie-break change); a namespace derived from kind and name. Why: authored
order still assigns `BlockId` and schedule tie-breaks, and a namespace must never derive from a host
path or a UUID. *Amended 2026-11-09 (GitHub #470):* the original rationale also said "the content
id is order-bearing"; the canonical content id ([content-id note](content-id.md)) ignores order and
namespace, so that reason is superseded. The decision stands on the `BlockId` reason.

**7. Catalog gaps: a separate pre-M2 engine issue (#440).** Port-name synthesis as a pure function
over a constructed block, descriptions and a settable-versus-derived flag, exposed through the
catalog; the existing catalog golden unchanged and a new golden for synthesized names; units and
further descriptions wait for vocabulary v1. Rejected: folding it into the first M2 PR (mixes the
core-four and leverage lanes); `oce-author` deriving names itself (an author guess that drifts).
Why: width-class port naming is an engine contract.

**8. Wire-form schema: derived from the Rust types.** `schemars`, a leverage dependency only,
checked in CI against the published file; the class enum from the catalog; the strict profile above,
with parameter `value` an `anyOf` over a number, a boolean, a string, a closed `{"ref": …}` object
and a closed `{"member": …}` object (decision 11), and no `oneOf`. Rejected: a hand-written schema
(the prototype's drifted). Why: one source of truth.

**9. CLI now, TUI later; binaries allowed above the facade.** An `oce` CLI in `crates/leverage`
(`fmt`, `validate`, `emit`, later `bind`, `points`, `doc`, `vocab`), and the architecture page
amended so the no-binaries rule covers the engine crates only. Rejected: a library-only M2. Why:
user and agent experience is the point of M2; binaries stay above the facade and out of the
engine's dependency graph.

**10. Block `@type` uses a fixed class base.** The `oce-cxf` class bridge
(`oce_cxf::bridge::class_path_of`) takes the fragment after the last `#` of `@type`, then strips
`Buildings.Controls.OBC.`. The default namespace `urn:oce:author:` has no `#`, so a class IRI
under it would resolve to nothing. The emitter writes
`http://example.org#Buildings.Controls.OBC.<class>` (the exporter's own class base) whatever the
instance namespace is. The instance namespace and the class base are independent by
design: `class_path_of` reads only the class IRI's fragment, so no instance namespace can change
which class a block resolves to. Rejected: changing the default namespace to end in `#` (that
re-opens decision 6); a configurable class base (one more way to emit an unloadable document).

**11. Wire values: three scalars and two tagged objects.** `value` is a number, a boolean, a
string, `{"ref": "<sequence-param>"}` or `{"member": "<Type.Member>"}`, expressed in the schema as
`anyOf` over five closed shapes. A plain string is always text; any parameter kind, String
included, may take a reference. The text form keeps its three distinct spellings and the model is
tagged, so the two forms map one to one. Rejected: typing a string by the parameter's catalog kind
(the drafted proposal; an agent writing JSON must not have a string's meaning depend on the
catalog, and String parameters could not take references); a `$` sigil for references (a magic
convention agents get wrong).

**12. What `fmt` changes beyond whitespace.** The owner's first wording of decision 6 said `fmt`
normalises whitespace only. Two more edits are needed for a canonical text to exist: statements are
grouped into the fixed section order (authored order kept inside each section, so emission is
unaffected), and Real literals take their shortest round-trip spelling (`1.50` becomes `1.5`).
Integer literals keep their spelling. Rejected: refusing out-of-section statements (hostile to hand
editing); storing lexical spellings in the model (two equal models with different bytes).

**13. Unit suffixes are base units only in M2.** The catalog carries no units until
vocabulary v1, so there is no conversion table. A suffix is kept on the model literal as a
text-only annotation, so `fmt` is lossless, and the number is the base-unit value as written.
`1 h` is refused with a typed error until the vocabulary supplies scale factors. Text → wire → text
drops suffixes; wire → text → wire, the gate's direction, is unaffected.

**14. Every engine warning is an error finding.** Accepted means warning-free, so a
warning is a defect in the authored sequence, not advice. This covers `unused-boundary-input`,
`undriven-boundary-output` and parameter-rule warnings such as `uLow == uHigh` on
`CDL.Reals.Hysteresis`.

**15. Array parameters.** Text writes `[1, 2]` and `[[1, 0], [0, 1]]`; the wire form
lists one `{name, value}` per element under the catalog's `<i>`, `<row>`, `<col>` template names,
keeping decision 8's scalar profile; the emitter writes the CXF array form. Leaf array parameters
are accepted (Rule 5 refuses arrays on composites only). #440 must expose the template expansion,
and the amended issue does. *Amended 2026-11-09 (issue #464):* when the author assigns only some
elements, the array form would need the rest, which the emitter never fills (decision 16), so each
assigned element is written as its own parameter node under its flattened name (`k_2`), the name
the engine's class rules read; the class default supplies the others.

**16. The emitter never fills a default.** It emits exactly the assignments the author
wrote, and the engine supplies defaults. A library convention (the library sets
`Logical.TrueDelay.delayOnInit = true` throughout, against the CDL default) belongs in vocabulary
notes an agent reads, not in a silent emitter default.

**17. Relation lists are always arrays.** `hasInput`, `hasOutput`, `hasParameter`,
`containsBlock` and `isConnectedTo` are written as arrays even with one entry. The singleton
spelling is normalized on import, and one shape keeps the writer small. Byte equality with
committed library CXF is not a goal; the gate compares canonical content ids after load.

**18. The #440 surface reaches `oce-author` through `oce-api`.** A leverage crate cannot
call `oce_blocks`, so the facade exposes resolved ports for a class and an `oce_api::ParamTable`,
and the new per-class and per-parameter metadata. That metadata stays out of the revision-1
canonical catalog JSON, so `CATALOG_JSON` and `catalog_content_id` stay byte-identical, and it does
not enter `class_signature_hash` (descriptions are not behaviour, and settability follows from the
already-hashed default source). #440 has been amended to match: a facade accessor, the
revision-1 catalog JSON and the class signature hash unchanged, and the template expansion exposed.

**19. Leverage crates may dev-depend on `oce-bless`.** Goldens in `oce-author` need the
repo's single bless switch. The leverage rule allows only `oce-api` and other leverage crates;
copying the truthiness policy would recreate the drift `oce-bless` exists to prevent.
The rule changes to allow exactly this dev-dependency; `crates/leverage/README.md` and
`scripts/ci/leverage-members.sh` are amended in the first M2 crate PR, not in this note's PR.

**20. Crate shape, and `clap` for the CLI.** `crates/leverage/oce-author` (library) and
`crates/leverage/oce-cli` (package `oce-cli`, binary `oce`), both `publish = false` for M2 and
`#![forbid(unsafe_code)]`. The CLI parses its arguments with `clap` (derive). `schemars` and `clap`
are M2's two new third-party dependencies, both confined to `crates/leverage/`. Rejected:
hand-written argument parsing (the drafted proposal). Why: the binary lives above the facade and is
never embedded, so its dependency tree does not touch the engine, and consistent `--help`,
suggestions and completions are the point of a CLI.

**21. `quantity` and `displayUnit` live on the boundary declaration, outside `SemanticPoint`.** The
emitter must write `S231:quantity` and `S231:displayUnit` (the library CXF carries them), but
decision 2's closed `SemanticPoint` has no such fields. They are optional fields of the input or
output declaration (`@quantity`, `@displayUnit` in text; `quantity`, `display_unit` on the wire);
`unit` stays in `SemanticPoint`. Rejected: widening `SemanticPoint` (reopens decision 2 and
pre-empts M8's record shape); deriving them from `unit` (no table until vocabulary v1). Why: they
are CXF connector attributes the engine already reads, and M8 can then design semantic records
without moving them.

## Dependencies and sequencing

- **#440 first.** No M2 crate PR starts before the catalog publishes resolved port names and the
  settable flag through `oce-api` (decision 18).
- **Suggested PR order:** (1) model, builder, wire form and schema, stage-1 validator; (2) emitter
  and engine-side validator with the library round trip; (3) text form; (4) the `oce` CLI and the
  architecture amendment taking effect; (5) the importer (library CXF and the prototype's v0 JSON
  into the model) and vocabulary v1, each with its own note. Adding a leverage crate changes
  `Cargo.lock`, so those PRs run the determinism lane as well.
- **M2 runs in parallel with M3.** M3 owns the content id, the registry and `oce-diff`. M3 replaced
  the order-bearing `content_id_complete()` with `Engine::content_id` (GitHub #470), so gate item
  1 compares canonical ids: the canonical id of the emitter's output equals the committed CXF's.
  The namespace is free (the default `urn:oce:author:` matches the library's `http://example.org#`);
  the sequence name must match, because it is inside every name.
  M3 also gives `extends` its resolution and diff. Four library rules are known to differ in node
  order from the emitter's layout; library bundle v2 regenerates them with the emitter, or the gate
  compares loaded models for them.
- **M8 (semantics)** fixes the §7.17.2 record shape. The emitter then writes `SemanticPoint`
  records with no change to the model, either form or the schema.
- **Vocabulary v1** (from `modelica-json -o doc` over `third_party/`) supplies units, descriptions
  and agent notes; the plan's type and unit checks wait for it.
- **The gate does not depend on the frozen prototype harness.** The benchmark specs and failure
  set move into this repository's conformance corpus or the library first; a follow-up issue filed
  when the M2 tracking issue is cut covers it. Until then gate item 4 cannot run.

## Test plan

- **Text form.** Round-trip properties over enumerated models and every corpus bundle; `fmt`
  idempotence; each parse bound at its boundary; one typed refusal per malformed construct
  (unterminated string, bad escape, bad suffix, stray token, missing `extends`, out-of-range
  integer, `NaN` or `inf` spelled as a number); comment and blank-line preservation goldens.
- **Wire form.** wire → text → wire byte equality for every bundle (gate item 2); closed-object
  refusals (unknown key, missing key, wrong type) as typed errors; the schema regeneration and
  profile-keyword tests.
- **Validator.** One test per finding code with the exact code and subject; the parity test
  (stage 1 bypassed, engine code equal); the gate item 3 table test over the library refusal
  corpus; finding-order determinism over permuted inputs; numeric boundaries for parameters (`-0.0`,
  `i32` and `i64` limits, values past 2^53, subnormals).
- **Emitter.** Byte goldens for the worked example and every corpus bundle; emit-twice equality;
  the permutation-class tests; every emitted golden loads warning-free; the composite-contract
  drivers over emitter output; the default namespace and an override both load; a document at
  `MAX_CXF_BYTES` is accepted and one byte over is refused before load.
- **Library round trip (gate item 1).** Every library rule imports into the model, emits, and the
  canonical content id of the emitter's output equals that of its committed CXF.
  *Coverage, 2026-11-09 (GitHub #473):* not yet met. The minimal importer reads 4 of the 40
  in-subset G36 fixtures (4 nested and 2 conditional fixtures are out of the M2 subset), and all 4
  round-trip; the other 36 wait on model decisions — enumeration-typed sequence parameters,
  connector `min`/`max`, library enumerations — tracked in GitHub #475.
- **CLI.** Golden JSON reports for each command and exit-code tests for each outcome class.
- **§7.17 seam.** Filling and mutating every `SemanticPoint` field leaves emitted bytes unchanged
  except `unit`, and leaves the loaded executable identical.

## Out of scope for M2

- Nested composites, `hasInstance` members, conditional components and array connectors.
- Expression parameter values beyond a bare reference, and any evaluation in `oce-author`.
- Emitting semantic records (M8); resolving or diffing `extends` (M3).
- Unit conversion beyond base units, and unit checks (vocabulary v1).
- A validate-only facade API (may be proposed later; decision 4).
- The TUI, `bind`, `points` and `doc` commands.
- Byte equality with committed library CXF or with any other generator's formatting.

## Review rulings

The architect pass on the draft (owner consulted on 11, 12 and 20) ruled:

- 10 accepted; a sentence names `class_path_of` and states that namespace and class base are
  independent.
- 11 rejected and replaced: wire `value` is a number, boolean, string, `{"ref"}` or `{"member"}`
  (`anyOf`, no `oneOf`); String parameters may take references; decision 8 reworded.
- 12 accepted; decision 6 now names whitespace, section grouping and canonical Real spelling.
- 13 accepted: base units only in M2, `1 h` refused.
- 14 accepted: every engine warning is an error finding.
- 15 accepted; #440 exposes the template expansion.
- 16 accepted: the emitter never fills a default.
- 17 accepted: relation lists are always arrays.
- 18 accepted; #440 amended (facade accessor, revision-1 catalog JSON and class hash unchanged).
- 19 accepted; the leverage README and `leverage-members.sh` change in the first M2 crate PR.
- 20 shape accepted, hand-written parsing rejected: `clap` with derive; `schemars` and `clap` are
  M2's two new dependencies, confined to `crates/leverage/`.
- 21 added and accepted: `quantity` and `displayUnit` on the boundary declaration.
- Gate item 3 reading accepted as written; the parity test is the proof.
- Gate item 4: accepted; the benchmark-data follow-up issue is filed when the M2 tracking issue is
  cut.
- 2026-11-09 (issue #464, GitHub #473): the parity test amended four rows of the finding-code
  table (`duplicate-parameter` and `unknown-enum-literal` became author-only codes pre-empting
  `duplicate-id` and `grounding-failed`; `unknown-parameter` pre-empts nothing, #467;
  `unknown-port` pre-empts `unresolved-reference`), added two stage-2 rows (`document-too-large`,
  `load-failed`), made `single-assignment` a stage-1 code for a repeated target as well as an
  engine code, refused Integer literals above 2^53 on Real parameters, and added decision 15's
  partial-array sentence.
- 2026-11-08: the CLI exit-code rule was aligned with the diff and commissioning passes: 0 yes, 1
  no, 2 error, with a parse refusal and an I/O failure both 2 and no exit 3.
