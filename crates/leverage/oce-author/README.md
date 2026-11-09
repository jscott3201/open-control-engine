# oce-author

The authoring layer's library (M2; design note [`docs/design/authoring-layer.md`](../../../docs/design/authoring-layer.md),
issues #463 and #464). A leverage crate: it depends on `oce-api` only, never evaluates an expression,
and loads only throwaway engines of its own (validation stage 2), never one a host runs.
`publish = false`.

| Surface | What it is |
| --- | --- |
| `Sequence` and its statements | The typed model: one flat sequence of parameters, boundary inputs and outputs (with `quantity`, `display_unit` and a closed `SemanticPoint`), leaf blocks with ordered assignments and typed connections, in authored order. Each statement carries text-form `Annotations` (comments, a blank line before it), the sequence its header comments (`annotations`) and `trailing_comments`, and assignments and parameters a text-only unit suffix; the wire form and the emitter ignore them. The model types are `#[non_exhaustive]`: build through the constructors or `SequenceBuilder`. |
| `SequenceBuilder` | A checked builder; refuses with a typed `BuildError` what it can see without the catalog. |
| `from_wire_str`, `from_wire_bytes`, `to_wire_string` | The strict-JSON wire form: closed objects, every key required, absent values `""`/`[]`/`false`, connections as `"a.y -> b.u"`. Reads are bounded (4 MiB, depth 8, 65,536 entries per list, 4,096-byte strings) and refuse hostile input with a typed `WireError`; writes are canonical and byte-stable. |
| `WIRE_SCHEMA`, `wire_schema()` | The JSON Schema (draft 2020-12) derived from the wire DTOs with `schemars`, published at [`schema/sequence.schema.json`](schema/sequence.schema.json) and pinned byte for byte; the `class` enum is the catalog's authorable class paths. Strict profile: closed objects, all required, `anyOf` only over the value shapes. |
| `validate`, `validate_author_side`, `Finding` | The two-stage validator. Stage 1 (`validate_author_side`): twenty-one finding codes (`STAGE_ONE_CODES`) from the model and the catalog, a repeated connection target (`single-assignment`) and an Integer above 2^53 on a Real parameter included. Stage 2, only when stage 1 is clean: emit, load in a throwaway `Engine::new()` and map every engine diagnostic back through the IRI scheme — every warning is an error finding with the engine's code (decision 14), `origin = engine`; a subject the scheme cannot name is attached to `sequence` with the raw subject in `detail`. Findings are sorted by authored position, code and detail. |
| `emit`, `EmitOptions`, `EmitError` | The emitter to flat CXF: authored order, never sorted; IRIs `{namespace}{sequence}.{local}` (default `urn:oce:author:`, an override must end in `#`, `/` or `:`); block `@type` under the fixed class base `http://example.org#Buildings.Controls.OBC.`; only the assignments the author wrote; arrays in the CXF array form (element by element under the flattened names when the author left a gap); boundary `unit`, `quantity` and `displayUnit` and no semantic record; a canonical writer (fixed key order, two-space indentation, ASCII, shortest round-trip reals); refuses stage-1 errors (`EmitError::Invalid`), a malformed namespace (`EmitError::InvalidNamespace`) and anything above `oce_api::MAX_CXF_BYTES` (`EmitError::TooLarge`). |
| `import_cxf`, `ImportError` | A minimal importer for the flat corpus dialect (gate item 1); see below. |
| `parse_text`, `print_text`, `fmt_text`, `ParseError` | The text form (issue #482): a hand-written recursive-descent parser, bounded at parse, refusing each malformed construct with one typed `ParseError { code, location, detail }` (`PARSE_CODES`, twenty-nine codes); a canonical printer; `fmt_text` = parse then print, idempotent. See below. |
| `validate_text`, `parse_text_with_spans`, `SourceMap` | Validation of a text document: both stages, with every finding's `span` set to its subject's line and column. |

## The minimal importer

`import_cxf` reads one flat, single-root CXF document into the model and refuses everything outside
the authoring subset with a typed `ImportError`, never a panic: oversize input (`TooLarge`),
non-strict JSON — a key written twice, or an integer-shaped token outside `i64` at any depth, checked lexically, with `-0` read as the Integer zero as on the wire (`Malformed`) — a construct outside
the subset (`Unsupported`) or a dangling reference (`Unresolved`). It deliberately ignores
`S231:label` everywhere, a parameter's descriptive `unit`, `quantity` and `displayUnit`, and the
root's `@type`. Gate item 1 is not yet met: of the 46 G36 fixtures, 6 are out of the M2 subset and 4
of the other 40 import and round-trip; the 36 others wait on model decisions (GitHub #475):

| First refusal | Fixtures |
| --- | --- |
| Sequence parameter of an enumeration type (CDL `SimpleController` or a G36 type) | 21 |
| `S231:min` / `S231:max` on a boundary or block-port connector | 10 |
| A G36 (library) enumeration as a block parameter value | 5 |
| Nested composite (out of subset) | 4 |
| Conditional component (out of subset) | 2 |

`ImportError::Unsupported { subject, construct }` carries `construct` as a `String` for now; its
shape is provisional until the full importer's design note (M2 item 5).

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
| enumeration member | `{"member": "SimpleController.PI"}` | `Literal::EnumMember` |

A sequence parameter's value takes the same shapes except the reference.

Goldens regenerate only with `OCE_BLESS_AUTHOR=1 cargo nextest run -p oce-author` (see
`.agents/project-facts.md`).

## The text form

A terse line grammar (authoring note, "Text form"; decisions 1, 3, 12, 13 and 15). The worked
example:

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

### Grammar

```text
file        = sequence-stmt extends-stmt { statement }
sequence-stmt = "sequence" name
extends-stmt  = "extends" ( "none" | provenance-id )
statement   = param-stmt | in-stmt | out-stmt | block-stmt | connection
param-stmt  = "param" name ":" type "=" scalar
in-stmt     = "in" name ":" type { tag }        (out-stmt alike, with "out")
block-stmt  = "block" name "=" path [ "(" [ assign { "," assign } ] ")" ]
assign      = name "=" ( scalar | list )
connection  = path "->" path                    (an endpoint: name or block.port)
scalar      = number [ suffix ] | "true" | "false" | string | path
list        = "[" item { "," item } "]"         (items scalars, or rows of scalars)
tag         = "@unit" atom | "@quantity" atom | "@displayUnit" atom | "@brick" atom
            | "@s223" atom | "@qudt" atom | "@haystack" atom | "@name" string
            | "@label" string | "@provisional"
type        = "Real" | "Integer" | "Boolean"
path        = name { "." name }
number      = [ "-" ] ( "0" | [1-9][0-9]* ) [ "." [0-9]+ ] [ ( "e" | "E" ) [ "+" | "-" ] [0-9]+ ]
suffix      = ( "1" "/" factors ) | factors [ "/" factors ]
factors     = factor { "." factor }, factor = symbol [ [ "-" ] digits ]
string      = '"' { character | '\"' | '\\' | '\n' } '"'
comment     = "#" to the end of the line, anywhere
```

- One statement per line, except a block's `(...)` list and the lists inside it, which may span
  lines. Spaces separate tokens; tabs, carriage returns and other control characters are refused.
- A `name` is any word `[A-Za-z_][A-Za-z0-9_]*`; whether it is a valid declared name (not a keyword
  or reserved word, at most 64 bytes, unique) is the validator's judgement, never the parser's.
  An `atom` and a provenance id run to the next space (an atom also stops at `#`).
- On the right of `=` in a block, a bare `name` is a reference to a sequence parameter and a
  dotted `Type.Member` an enumeration member. A sequence parameter takes a literal or a member,
  never a reference or a list (`misplaced-value`). `NaN`, `inf` and `Infinity` in any case are
  refused as values (`non-finite-number`), so no parameter so named can be referenced from text.
- An integer token is a `Literal::Integer` and never becomes a Real; a token with a fraction or an
  exponent is a `Literal::Real`. A leading `+` or a leading zero is **refused**
  (`malformed-number`), not normalized; `-0` reads as the Integer `0`, as on the wire.
- **Unit suffixes are base units only** (decision 13): every factor must be a coherent, unprefixed
  SI symbol (`BASE_UNIT_SYMBOLS`: `m kg s A K mol cd rad sr Hz N Pa J W C V F Ohm S Wb T H lm lx
  Bq Gy Sv kat`), so `86400 s`, `1 m3/s` and `4180 J.kg-1.K-1` are accepted and `1 h`, `20 degC`
  and `3 kPa` are refused (`unit-not-base`) until vocabulary v1. Parentheses are not accepted. The
  suffix is kept on the model as a text-only annotation; whatever follows a number on its line,
  up to `,`, `)`, `]` or `#`, is read as its suffix.
- A list lowers to one assignment per element under the catalog's element names
  (`oce_api::param_element_name`: `k = [1, 2]` is `k_1`, `k_2`; `K = [[1, 0], [0, 1]]` is
  `K_1_1` … `K_2_2`, row-major). For an unknown class or parameter the same shape is used, so the
  validator names the elements. A list nests at most two deep and is never empty or ragged.

### The canonical printer and `fmt`

`fmt_text` is `parse_text` then `print_text`, and `fmt_text(fmt_text(t)) == fmt_text(t)`.

- **Section order**: header comments, `sequence`, `extends`, then parameters, inputs, outputs,
  blocks and connections, each section in authored order (decision 12). Nothing is sorted.
- **Spelling**: one space around `=`, `->` and `:`; Reals in Rust's shortest round-trip spelling
  (`1.50` → `1.5`, `1E5` → `100000.0`, `0.000001` → `1e-6`); Integers bare; suffixes re-emitted
  after one space; tags in the grammar's order; two-space indentation; one final line feed.
- **Blocks** print on one line when they have at most two items, none annotated, and the line fits
  in 100 columns (`MAX_ONE_LINE_COLUMNS`); otherwise one item per indented line. An empty `()`
  is dropped.
- **Comments and blank lines** come from the annotations. A blank line before a statement (or its
  comments) is kept as one; a run collapses to one. Own-line comments stay above the statement
  that follows them. **An end-of-line comment moves to its own line above its statement** (inside
  a block's list, above its assignment; after `(` or `)`, above the block). Comments before
  `sequence` or between `sequence` and `extends` print above `sequence`; comments after the last
  statement print last, after one blank line. A comment after the last item of a `(...)` or
  `[...]` is refused (`comment-before-close`). Trailing spaces in a comment are dropped.
- **Arrays regroup through the catalog**: a complete, consecutive run of one array parameter's
  elements — starting at index 1, in row-major order, filling a vector or whole matrix rows, with
  no comment or blank line before any element but the first — prints as `name = [..]` (one element
  or row per line when it does not fit in 100 columns). Anything partial, out of order, commented
  or on an unknown class prints flattened (`k_1 = 1, k_3 = 3`). A lone first element is a run of
  one (`k = [1]`).

`parse_text(&print_text(m)) == m` for every model `parse_text` returns. A model the wire form or
the builder made prints to text that parses back to it when its names are words, its class paths
dotted words, its atoms free of spaces and `#`, and its Reals finite — every model stage 1
accepts (gate item 2: wire → text → wire is byte-identical for every bundle). Formatting can
lengthen a line by a few bytes and a block by a few lines, so a document within a few bytes of a
bound can format past it.

### Bounds

Each refused one past its limit with its own code, and accepted at it: 4 MiB (`input-too-large`),
100,000 lines (`too-many-lines`), 8,192 bytes per line (`line-too-long`; the note's 4,096 amended
so the longest wire string fits on one line), 256 bytes per name, number, suffix, atom or
provenance id (`token-too-long`), 4,096 bytes per string as written (`string-too-long`), list
nesting 2 (`nesting-too-deep`), and 65,536 statements of one kind or assignments in one block
(`too-many-statements`). Non-UTF-8 (`parse_text_bytes`), a byte-order mark and control characters
are refused before a token is read. Nothing in the parser panics or recurses past the list depth.

### Spans

`validate_text` (and `validate_text_author_side`) fills `Finding::span` from a `SourceMap` built
while parsing: `sequence` is the sequence name, `extends` its operand, a declared name its
declaration's name, `block.param` the assignment's name (an array element's value), and
`connection[N]` the connection's source. A `duplicate-name` or `duplicate-parameter` finding is
placed on the repeat, not the first declaration. Wire-form findings keep `None`.

The bundle texts (`tests/fixtures/text/*.oce`) and the `fmt` goldens
(`tests/fixtures/text/fmt/*.out.oce`) regenerate with `OCE_BLESS_AUTHOR=1`, like the other goldens.
