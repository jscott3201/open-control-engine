# oce-author

The authoring layer's library (M2; design note [`docs/design/authoring-layer.md`](../../../docs/design/authoring-layer.md),
issues #463, #464, #482, #492, #496 and #501). A leverage crate: among the `oce-*` packages it depends on `oce-api` and the leverage crate `oce-vocab` (for unit suffixes) only, never evaluates an expression,
and loads only throwaway engines of its own (validation stage 2), never one a host runs.
`publish = false`.

| Surface | What it is |
| --- | --- |
| `Sequence` and its statements | The typed model: one flat sequence of parameters (typed `ParameterType`: `Real`, `Integer`, `Boolean` or an enumeration type name), boundary inputs and outputs (with `quantity`, `display_unit`, optional `min` and `max` bounds (`Bound`) and a closed `SemanticPoint`), leaf blocks with ordered assignments and typed connections, in authored order. Each statement carries text-form `Annotations` (comments, a blank line before it), the sequence its header comments (`annotations`) and `trailing_comments`, and assignments and parameters a text-only unit suffix; the wire form and the emitter ignore them. The model types are `#[non_exhaustive]`: build through the constructors or `SequenceBuilder`. |
| `SequenceBuilder` | A checked builder; refuses with a typed `BuildError` what it can see without the catalog. |
| `from_wire_str`, `from_wire_bytes`, `to_wire_string` | The strict-JSON wire form: closed objects, every key required, absent values `""`/`[]`/`false` (never `null`), an object read from a JSON object only (never from the list of its values; any other type is `WireError::WrongType` with the path of the value, GitHub #544), connections as `"a.y -> b.u"`, boundary bounds as a list of at most one number (`"min": []` or `[0]`; a second item is `WireError::BoundListTooLong`). Reads are bounded (4 MiB, depth 8, 65,536 entries per list, 4,096-byte strings) and refuse hostile input with a typed `WireError`; writes are canonical and byte-stable. |
| `WIRE_SCHEMA`, `wire_schema()` | The JSON Schema (draft 2020-12) derived from the wire DTOs with `schemars`, published at [`schema/sequence.schema.json`](schema/sequence.schema.json) and pinned byte for byte; the `class` enum is the catalog's authorable class paths and a sequence parameter's `type` enum is `parameter_type_names()` (`Real`, `Integer`, `Boolean` and every enumeration of `oce_api::enumeration_types()`, never a constant package). Strict profile: closed objects, all required, `anyOf` only over the value shapes. |
| `validate`, `validate_author_side`, `Finding` | The two-stage validator. Stage 1 (`validate_author_side`): twenty-six finding codes (`STAGE_ONE_CODES`) from the model and the catalog, a repeated connection target (`single-assignment`), a unit suffix against the parameter's declared unit (`unit-suffix-mismatch`, through `oce-vocab`), an Integer above 2^53 on a Real parameter, enumeration types and members (`unknown-enum-type`, `unknown-enum-literal`) and boundary bounds (`invalid-bound`, `non-finite-bound`, `crossed-bounds`) included. Stage 2, only when stage 1 is clean: emit, load in a throwaway `Engine::new()` and map every engine diagnostic back through the IRI scheme — every warning is an error finding with the engine's code (decision 14), `origin = engine`; a subject the scheme cannot name is attached to `sequence` with the raw subject in `detail`. Findings are sorted by authored position, code and detail. |
| `emit`, `EmitOptions`, `EmitError` | The emitter to flat CXF: authored order, never sorted; IRIs `{namespace}{sequence}.{local}` (default `urn:oce:author:`, an override must end in `#`, `/` or `:`); block `@type` under the fixed class base `http://example.org#Buildings.Controls.OBC.`; only the assignments the author wrote; arrays in the CXF array form (element by element under the flattened names when the author left a gap); an enumeration-typed sequence parameter with its class IRI as `isOfDataType` and the full member path as value, and a library constant with its package IRI as `isOfDataType`, as the corpus writes them; boundary `unit`, `quantity`, `displayUnit`, `min` and `max` and no semantic record; a canonical writer (fixed key order, two-space indentation, ASCII, shortest round-trip reals); the `S231` context bound to `http://data.ashrae.org/S231#`, and on the root node the translation-software label `S231:translationSoftware` (`open-control-engine`) and `S231:translationSoftwareVersion` (the workspace version; OBC §8.4), which no importer reads back and which changes every emitted document when the workspace version does; refuses stage-1 errors (`EmitError::Invalid`), a malformed namespace (`EmitError::InvalidNamespace`) and anything above `oce_api::MAX_CXF_BYTES` (`EmitError::TooLarge`). |
| `import_cxf`, `import_graph_v0`, `ImportOptions`, `Imported`, `ImportReport`, `ImportError` | The importer (importer note [`docs/design/importer.md`](../../../docs/design/importer.md), issue #496): the in-subset library CXF and the frozen prototype's v0 graph JSON into the model, a report of what was read and not carried, and a closed `ImportErrorKind` with a stable `code()` for every refusal; see below. |
| `parse_text`, `parse_text_bytes`, `print_text`, `fmt_text`, `ParseError`, `PrintError` | The text form (issue #482): a hand-written recursive-descent parser over `&str` or raw bytes, bounded at parse, refusing each malformed construct with one typed `ParseError { code, location, detail }` (`PARSE_CODES`, thirty-one codes, pinned in order); a canonical printer that is total or refusing (`PrintError::Unspellable`); `fmt_text` = parse then print, idempotent. See below. |
| `validate_text`, `parse_text_with_spans`, `SourceMap` | Validation of a text document: both stages, with every finding's `span` set to its subject's line and column. |

## The importer

Two pure functions, one per source form, each returning `Imported { sequence, report }`: the
sequence in the source's own order and an `ImportReport` of what was read and not carried. Neither
validates nor evaluates; names are carried, never repaired, for the validator to judge.

- **`import_cxf(bytes, &ImportOptions)`** reads one flat, single-root CXF document (the library
  dialect): leaf blocks; boundaries with `unit`, `quantity`, `displayUnit`, `min` and `max` (a bound
  converted exactly as the loader converts it: a typed literal's text with `str::parse`, untrimmed,
  an Integer on a Real boundary with `as f64`); sequence parameters of a scalar or enumeration type;
  block parameters, arrays row-major under the catalog's template names; enumeration members and
  library constants as full member paths resolved through `oce_api::enumerations()`; connections.
- **`import_graph_v0(bytes, &ImportOptions)`** reads the prototype's compact block graph under the
  wire form's bounds: `bool`/`real`/`int` boundaries, blocks under `CDL.`, parameters by token kind.
  `unit` and `description` become comment lines on the boundary (split at line feeds, every other
  control character a space, each line truncated at a character boundary so `# ` plus the line
  stays within `MAX_V0_COMMENT_LINE_BYTES` = 4,096, the truncation reported), never
  `SemanticPoint.unit`; the sequence gets `# imported from v0 graph JSON`
  (`V0_PROVENANCE_COMMENT`). Nothing about provenance reaches the wire form or the emitter.
- **`ImportOptions`** sets `derived_from` (none by default; the importer never derives one).
- **`ImportReport`** lists, in reading order, every `Dropped { subject, property, value }` (closed
  `DroppedProperty`: the CXF root's `@type` when it is not `S231:Block`, an `S231:label` that
  differs from the local name, a parameter's descriptive `unit`, `quantity`, `displayUnit`, `min`
  and `max` — the engine reads none of them — and a v0 `unit` or `description`), every
  `Normalised` connector (one typed by a constant package, `S231:IntegerInput` with
  `isOfDataType …G36.Types.OperationModes`, read as the Integer the engine reads; importer note,
  decision 21) and every `Truncated` v0 comment line. An emitted document re-imports with an empty
  report.

Every refusal is an `ImportError { source, subject, kind }`, never a panic. `ImportErrorKind` is
`#[non_exhaustive]`; branch on `code()`, one of the 30 stable kebab-case codes in
`IMPORT_ERROR_CODES` (pinned by an in-crate test, and each produced by `tests/import_codes.rs`):
`too-large`, `not-strict-json` (a key written twice, an integer outside `i64` at any depth, broken
syntax, non-UTF-8), `limit-exceeded` (v0 bounds), `unknown-key`, `namespace-mismatch` (a
`@context` that binds `S231` to any IRI but `http://data.ashrae.org/S231#`, or any term to the
namespace written through release 0.4.0), `missing-key`, `wrong-shape`,
`no-root`, `duplicate-id`, `unresolved-reference`, `stray-node`, `invalid-name`, the out-of-subset
kinds `nested-composite`, `instance-member`, `constant-member`, `conditional-component`,
`array-connector`, `array-sequence-parameter`, `expression-value`, `port-attribute` and
`connector-attribute` (`nominal`, `unbounded`), then `driving-sink`, `class-outside-obc`,
`datatype-mismatch`, `unknown-datatype`, `unknown-member-type`, `malformed-literal`,
`array-shape`, `string-value` and `malformed-connection`. A document with several out-of-subset
constructs is refused for the first of nested composite, conditional component, expression value,
port attribute and name, whatever its node order.

**Gate item 1 is met** (`tests/library_round_trip.rs`): 31 of the 34 G36 fixtures inside the M2
subset import, emit under `urn:oce:library:`, load warning-free and have the canonical content id
of the committed fixture re-stamped and loaded directly; their exports match byte for byte wherever
block order agrees. The other 15 are refused with their kind:

| Kind | Fixtures |
| --- | --- |
| `nested-composite` (out of subset) | 4 |
| `conditional-component` (out of subset; post-M2 design pass #487) | 7 |
| `expression-value` (out of subset) | 1 |
| `port-attribute` (block-port bounds, not authorable in M2; #486) | 3 |

The three v0 golden pairs vendored from the prototype (`tests/fixtures/v0/`, provenance in its
`PROVENANCE.md`) each import and load to the content id of the CXF the prototype wrote.

The stage-1 / engine parity table, the library round-trip table and the permutation classes are
in the tests (`src/validate/engine_tests.rs`, `tests/library_round_trip.rs`,
`tests/emit_permutations.rs`); emitted byte goldens are under `tests/fixtures/emit/`.

A block parameter `value` takes one of five shapes on the wire:

| Shape | Example | Model |
| --- | --- | --- |
| number | `1`, `0.5`, `1e-7` | `Literal::Integer` with no fraction or exponent, else `Literal::Real` |
| boolean | `true` | `Literal::Boolean` |
| string | `"text"` | `Literal::Text`; always text, never a reference |
| reference | `{"ref": "dTHys"}` | `ParamValue::Ref` to a sequence parameter |
| member | `{"member": "SimpleController.PI"}`, `{"member": "OperationModes.occupied"}` | `Literal::Member` (`MemberRef`): an enumeration member or a library constant |

A sequence parameter's value takes the same shapes except the reference.

## Enumeration types, members and bounds (issue #492)

The three model changes of the importer note (`docs/design/importer.md`, decisions 1 to 6 and 10),
which the importer reads:

- **Enumeration-typed sequence parameters.** `ParameterType::Enum(type_name)`, spelled by the
  enumeration's last segment in both forms: wire `"type": "SimpleController"`, text
  `param conTyp : SimpleController = SimpleController.PI`. The value is a member of that type. The
  reader and the parser accept any type string (the schema's `type` enum lists the known ones);
  the validator resolves it through `oce_api::enumeration_types()` and refuses an unknown name or a
  constant package (`unknown-enum-type`). A block parameter of `ValueType::Enum(c)` may reference
  it only when the type resolves to class `c`; any other pairing is `parameter-kind-mismatch`. The
  emitter writes `isOfDataType` the class IRI and the full member path, as the corpus does.
- **Members.** `Literal::Member(MemberRef)` covers an enumeration member (`SimpleController.PI`)
  and an Integer constant of a library constant package (`OperationModes.occupied`), one wire tag
  and one text spelling; the type name decides which, through `oce_api::enumerations()`. A constant
  fits an Integer parameter (sequence or block) and grounds to its Integer; on any other parameter
  it is `parameter-kind-mismatch` (`unknown-enum-literal` on an enumeration parameter). The emitter
  writes the package IRI as the node's `isOfDataType` (importer note, ruling 9).
- **Boundary bounds.** `Boundary::min` / `max`, each an optional `Bound` (an Integer or Real number
  literal, never a reference): wire `"min": []` or `[<number>]`, text `@min <number>` / `@max
  <number>` beside `@displayUnit` (no unit suffix). A Real boundary takes either kind (an Integer
  widened, exactly up to 2^53), an Integer boundary an Integer inside the engine's `i32` Integer
  domain (a bound outside `i32::MIN..=i32::MAX` is refused by stage 1, because the engine would
  narrow it silently), a Boolean boundary none (`invalid-bound`); a non-finite bound is `non-finite-bound` (the builder refuses it too,
  `BuildError::NonFiniteBound`) and `min > max` is `crossed-bounds`. The emitter writes `S231:min` /
  `S231:max` after `S231:displayUnit`: a typed `xsd:double` on a Real boundary, a bare JSON integer
  on an Integer one. On an input they are the effective acceptance domain and inside the canonical
  content id; on an output they are load-checked and outside it. `nominal` and `unbounded` are not
  carried.

Goldens regenerate only with `OCE_BLESS_AUTHOR=1 cargo nextest run -p oce-author` (see
`.agents/project-facts.md`).

## The text form

A terse line grammar (authoring note, "Text form"; decisions 1, 3, 12, 13 and 15). The worked
example:

```text
# Economizer enable on outdoor-air temperature with a deadband.
sequence econ_enable
derived_from none

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

### Grammar

```text
file        = sequence-stmt derived-from-stmt { statement }
sequence-stmt = "sequence" name
derived-from-stmt = "derived_from" ( "none" | provenance-id )
statement   = param-stmt | in-stmt | out-stmt | block-stmt | connection
param-stmt  = "param" name ":" ( type | name ) "=" scalar   (a name: an enumeration type)
in-stmt     = "in" name ":" type { tag }        (out-stmt alike, with "out")
block-stmt  = "block" name "=" path [ "(" [ assign { "," assign } ] ")" ]
assign      = name "=" ( scalar | list )
connection  = path "->" path                    (an endpoint: name or block.port)
scalar      = number [ suffix ] | "true" | "false" | string | path
list        = "[" item { "," item } "]"         (items scalars, or rows of scalars)
tag         = "@unit" atom | "@quantity" atom | "@displayUnit" atom | "@min" number
            | "@max" number | "@brick" atom | "@s223" atom | "@qudt" atom
            | "@haystack" atom | "@name" string | "@label" string | "@provisional"
type        = "Real" | "Integer" | "Boolean"
path        = name { "." name }
number      = [ "-" ] ( "0" | [1-9][0-9]* ) [ "." [0-9]+ ] [ ( "e" | "E" ) [ "+" | "-" ] [0-9]+ ]
suffix      = ( "1" "/" factors ) | factors [ "/" factors ]      (at most one "/")
factors     = factor { "." factor }, factor = symbol [ [ "-" ] digits ]   (exponent not zero)
atom        = 1 to 256 bytes of printable ASCII ("!" to "~") without "#"
provenance-id = 1 to 256 bytes of printable ASCII (may contain "#", decision 5)
string      = '"' { character | '\"' | '\\' | '\n' } '"'
comment     = "#" to the end of the line, anywhere
```

- One statement per line, except a block's `(...)` list and the lists inside it, which may span
  lines. Spaces separate tokens; tabs, carriage returns and other control characters are refused,
  and so are bidirectional overrides and isolates (U+202A to U+202E, U+2066 to U+2069) anywhere,
  comments and strings included (`control-character`).
- A `name` is any word `[A-Za-z_][A-Za-z0-9_]*`; whether it is a valid declared name (not a keyword
  or reserved word, at most 64 bytes, unique) is the validator's judgement, never the parser's.
  A dotted name (`CDL.Reals.Abs`, `hys.y`, `SimpleController.PI`) has no space on either side of a
  dot (`stray-token`).
- An `atom` runs to the next space or `#` and must be printable ASCII (`malformed-atom`). A
  provenance id runs to the next space and may contain `#`, as decision 5's grammar allows; it too
  must be printable ASCII.
- The provenance statement is `derived_from` (issue #529). `extends` is kept for its CDL meaning,
  composite inheritance (OBC specification §7.15): it starts no statement, so a file in the
  earlier spelling is refused (`missing-derived-from` in the header, `unknown-keyword` after it),
  and a wire document with an `extends` key is refused (`WireError::UnknownKey`, `unknown-key` in
  an `oce` report). `derived_from` is a text-form keyword (`TEXT_FORM_KEYWORDS`) and `extends` a
  Modelica reserved word, so neither is a declared name.
- On the right of `=` in a block, a bare `name` is a reference to a sequence parameter and a
  dotted `Type.Member` a member of an enumeration or a library constant package. A sequence parameter takes a literal or a member,
  never a reference or a list (`misplaced-value`). `NaN`, `inf` and `Infinity` in any case are
  refused as values (`non-finite-number`), and stage 1 refuses them as declared names
  (`invalid-name`, `NON_FINITE_WORDS`), so every reference a clean model holds is spellable.
- A sequence parameter's type is `Real`, `Integer`, `Boolean` or any other word, read as an
  enumeration type name the validator resolves; a boundary's type is one of the three
  (`unknown-type`). `@min` and `@max` take one number token and nothing after it but another tag,
  a comment or the end of the line (a unit suffix is `stray-token`).
- An integer token is a `Literal::Integer` and never becomes a Real; a token with a fraction or an
  exponent is a `Literal::Real`. A leading `+` or a leading zero is **refused**
  (`malformed-number`), not normalized; `-0` reads as the Integer `0`, as on the wire.
- **Unit suffixes are base units, or one scaled time unit alone** (decision 13 as amended by the
  vocabulary note's decision 14; issue #501). A base-unit suffix is built from coherent,
  unprefixed SI symbols (`BASE_UNIT_SYMBOLS`: `m kg s A K mol cd rad sr Hz N Pa J W C V F Ohm S Wb
  T H lm lx Bq Gy Sv kat`), so `86400 s`, `1 m3/s` and `4180 J.kg-1.K-1` are accepted, and the
  number is the value as written. A suffix that is exactly `min`, `h` or `d`
  (`oce_vocab::SCALE_FACTORS`) **scales**: the value is the base-unit Real, exactly and rounded
  once (`1.1 h` is `3960.0`, an Integer by its exact product, a Real through its shortest
  round-trip decimal), and `Assignment::scaled_from` keeps the written number, so **`fmt` prints
  `1 h` again** and never moves a value; the wire form and the emitter see `3600.0`. A scaled
  value past binary64 is `non-finite-number`. `20 degC`, `3 kPa`, `1 m3/h` (a scaled symbol in a
  compound) and `1 hr` are refused (`unit-not-base`). Whether the suffix fits its parameter is
  stage 1's `unit-suffix-mismatch`: it must reduce to the block parameter's declared unit
  (`oce-vocab`), spelled alike (`delayTime = 5 K` is refused, `3600 s` and `1 h` are not), and a
  scaled suffix on a parameter that declares no unit, or on a sequence parameter, is refused; a
  base-unit suffix where no unit is declared stays an annotation. The grammar is a subset of
  Modelica's unit syntax: a symbol with an optional non-zero integer exponent (`m2`, `s-1`; `K0` and
  `m-0` are refused), `.` for products, at most one `/` whose right-hand side is the whole
  denominator, and no parentheses. So `kg/m.s2` reads as kg/(m·s²), a deliberate deviation from
  Modelica's left-to-right reading. The suffix is kept on the model as a text-only annotation;
  whatever follows a number on its line, up to `,`, `)`, `]` or `#`, is read as its suffix.
- A list lowers to one assignment per element under the catalog's element names
  (`oce_api::param_element_name`: `k = [1, 2]` is `k_1`, `k_2`; `K = [[1, 0], [0, 1]]` is
  `K_1_1` … `K_2_2`, row-major). For an unknown class or parameter the same shape is used, so the
  validator names the elements. A list nests at most two deep and is never empty or ragged
  (`malformed-list`; the note's `list` production allowed an empty one). Inside a list, an
  end-of-line comment belongs to the element before it, as a statement's does.

### The canonical printer and `fmt`

`fmt_text` is `parse_text` then `print_text`, and `fmt_text(fmt_text(t)) == fmt_text(t)`.

- **Section order**: header comments, `sequence`, `derived_from`, then parameters, inputs,
  outputs, blocks and connections, each section in authored order (decision 12). Nothing is sorted.
- **Spelling**: one space around `=`, `->` and `:`; Reals (bounds included) in Rust's shortest round-trip spelling
  (`1.50` → `1.5`, `1E5` → `100000.0`, `0.000001` → `1e-6`); Integers bare; suffixes re-emitted
  after one space; tags in the grammar's order; two-space indentation; one final line feed.
- **Blocks** print on one line when they have at most two items, none annotated, and the line fits
  in 100 columns (`MAX_ONE_LINE_COLUMNS`); otherwise one item per indented line. An empty `()`
  is dropped.
- **Comments and blank lines** come from the annotations. A blank line before a statement (or its
  comments) is kept as one; a run collapses to one. Own-line comments stay above the statement
  that follows them. **An end-of-line comment moves to its own line above its statement** (inside
  a block's list, above its assignment; after `(` or `)`, above the block). Comments before
  `sequence` or between `sequence` and `derived_from` print above `sequence`; comments after the
  last statement print last, after one blank line. A comment after the last item of a `(...)`
  or `[...]` is refused (`comment-before-close`). Trailing spaces in a comment are dropped. A
  comment is at most `MAX_COMMENT_BYTES` (8,189) bytes after its `#`, the line bound less the `#`
  and the two-space indent the printer may give it (`comment-too-long`).
- **Arrays regroup through the catalog** only when the run is exactly what the block assigns of
  that array: every element of the template the block holds, consecutive, starting at index 1, in
  row-major order, at least two elements, a matrix a whole rectangle, and no comment or blank line
  before any element but the first. It prints as `name = [..]`, one element or row per line when it
  does not fit in 100 columns, and one cell per line for a row that does not. Anything else prints
  flattened: a gap (`k_1 = 1, k_3 = 3`), another order (`k_2 = 2, k_1 = 1`), an element elsewhere
  in the block (`k_1 = 1, nin = 2, k_2 = 2`), a partial matrix, a lone first element, a commented
  element or an unknown class.

**Total or refusing.** `print_text` returns `Result<String, PrintError>` and never emits text that
parses to a different model. A name, class path, atom, suffix, string, value or `derived_from` id
the lexer would not read back as the same value — a space or line feed in a name, `#` in an atom,
a non-path class, a suffix that is not a base unit, `none` as a `derived_from` id, a reference
named `true` or `inf`, a member with no `Type.`, an enumeration type spelled as a scalar type or
not as one word, a non-finite Real or bound, a control character or bidirectional
override in a string — is refused with `PrintError::Unspellable { subject, detail }`, and so is a
statement whose printed line would pass 8,192 bytes (a dotted name or a tag list too long for one
line) and a document that would pass 4 MiB, 100,000 lines or 65,536 statements of one kind. The only
normalizations are the parser's own: a comment with a line feed prints as several comment lines,
trailing spaces in a comment are dropped, and the header's `blank_line_before` is not printed.

**The round-trip property.** For every model `m`, `print_text(m)` either refuses or returns text
within every parse bound that `parse_text` reads back as `m` (up to the three normalizations).
It succeeds for every model `parse_text` returns and every model stage 1 accepts whose comments and
suffixes are spellable, unless the printed document would pass 4 MiB or 100,000 lines (comments
alone, or re-spacing a document already at a bound), which it refuses rather than emits. For every
text `t` that parses, `fmt_text(t)` is either that refusal or a canonical text with
`fmt_text(&fmt_text(t)?) == fmt_text(t)`. Stage 1 keeps every clean statement within the line
bound: at most 16 `@haystack` tags (`MAX_HAYSTACK_TAGS`), 64-byte names, catalog class paths,
256-byte atoms and labels, 4,096-byte strings, and no non-finite word or bidirectional override
where text would refuse it. Gate item 2: wire → text → wire is byte-identical for every bundle.

### Bounds

Each refused one past its limit with its own code, and accepted at it: 4 MiB (`input-too-large`),
100,000 lines (`too-many-lines`), 8,192 bytes per line (`line-too-long`; the note's 4,096 amended
so the longest wire string fits on one line), 256 bytes per name, number, suffix, atom or
provenance id (`token-too-long`), 4,096 bytes per string as written (`string-too-long`), list
nesting 2 (`nesting-too-deep`), and 65,536 statements of one kind or assignments in one block
(`too-many-statements`), and 8,189 bytes per comment (`comment-too-long`). Non-UTF-8
(`parse_text_bytes`), a byte-order mark, control characters and bidirectional overrides are
refused before a token is read. Nothing in the parser panics or recurses past the list depth.

### Spans

`validate_text` (and `validate_text_author_side`) fills `Finding::span` from a `SourceMap` built
while parsing: `sequence` is the sequence name, `derived_from` its operand, a declared name its
declaration's name, `block.param` the assignment's name (an array element's value), and
`connection[N]` the connection's source. A `duplicate-name` or `duplicate-parameter` finding is
placed on the later declaration in model section order (parameters, inputs, outputs, blocks), as
the validator reports it. A subject no statement names (an engine `connector#N` stage 2 could not
map) keeps `None`, as do wire-form findings. The subject index is built once per document, so
placing the findings of a document at the statement cap costs about what validating it does.

The bundle texts (`tests/fixtures/text/*.oce`) and the `fmt` goldens
(`tests/fixtures/text/fmt/*.out.oce`) regenerate with `OCE_BLESS_AUTHOR=1`, like the other goldens.
