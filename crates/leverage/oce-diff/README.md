# oce-diff

A structural diff of two revisions of a control sequence. It answers "what changed" as data a
receipt, a bundle card or an agent can read, with authored block names as the identity of a block.
Design note: [`docs/design/diff.md`](../../../docs/design/diff.md), which is the specification of
everything here.

One of four parts is in. **The authoring diff** (issue #665): `diff` compares two authoring models
and returns a sorted, schema-tagged `Diff`; the bounded rename pass presents renamed blocks; two
pure renderers turn the data into text and a Markdown table. The CXF adapter (#666), the blob
comparison (#667) and the `oce diff` verb (#668) build on it.

It is a leverage crate: among `oce-*` packages it depends on `oce-api` and `oce-author` only. It
never validates and never evaluates: it takes parsed models as they are, reads the catalog only for
a parameter's literal default and type, loads no engine, opens no file and is not published.

```rust
use oce_author::parse_text;
use oce_diff::{DiffOptions, diff, render_markdown, render_text};

let base = parse_text(&std::fs::read_to_string("reset.v1.oce")?)?;
let head = parse_text(&std::fs::read_to_string("reset.v2.oce")?)?;
let changes = diff(&base, &head, &DiffOptions::default());
if changes.summary.graph_equal && changes.summary.parameters_equal == Some(false) {
    println!("same structure, different tuning");
}
print!("{}", render_text(&changes));      // one line per entry
print!("{}", render_markdown(&changes));  // a Change | Subject | Was | Now table
print!("{}", changes.to_json());          // the data: sorted keys, two-space indentation
```

## Identity

**The authored block name is the identity.** Blocks are matched in three steps, each block at most
once:

1. the same name and the same class path is the same block, whatever its parameters or edges;
2. the same name with another class path is `removed` plus `added`, never a reparameterization;
3. the blocks still unmatched on both sides go through the **rename pass**.

The rename pass is deterministic and bounded.

- **Hard gates.** A candidate pair has the same class path and the same assignments, order
  ignored: a Real by its bits (`-0.0` and `0.0` differ, a NaN equals only its own payload), a
  reference by the parameter it names, a member by its spelling. An Integer literal and the equal
  Real are two literals here.
- **Neighbourhood.** A link end is an *anchor* when it is a boundary declared on both sides or a
  port of a matched block. A port *agrees* when the ends on both sides are the same anchors,
  *disagrees* when an anchor on one side has no equal on the other, and is *unknown* when what is
  left on both sides are ports of unmatched blocks. Three further cases are disagreements too,
  because no later round can make them agree: a port linked on one side only, a
  different number of ends, and an end that is neither an anchor nor an unmatched block (a
  boundary declared on one side only, a block no statement declares).
- **Rounds.** A pair is accepted only when it has no disagreement and at least one agreement and
  each of its two blocks has exactly one such candidate. Accepted pairs are anchors for the next
  round. At most `MAX_RENAME_ROUNDS` (8).
- **Confidence.** `exact` when every port of the pair agrees at the end, `high` otherwise.
- **Bounds.** With more than `MAX_RENAME_CANDIDATES` (2,048) unmatched blocks on a side, or more
  than `MAX_RENAME_PAIRS` (65,536) gated pairs, the pass does not run and the diff carries the
  note `rename-pass-skipped`. A cap is only passed when both sides have unmatched blocks: with
  nothing left on one side there is nothing to pair.
- **Boundaries are never renamed.** A renamed input or output is `removed` plus `added` in the
  interface section.

A round is computed without visiting pairs. Whether two blocks disagree depends only on each
block's own *profile* (its gate and, per port, the anchors and the number of unmatched ends), so
a round groups the unmatched blocks by profile and accepts the groups that hold one base block and
one head block. Its work is the links at the unmatched blocks: a bucket of thousands of identical
blocks costs what its links cost. The pair cap is still counted and enforced.

**`summary.graph_equal` never depends on the pass.** A block left unmatched by name and class is
in the graph section as `removed`, `added` or `renamed` whatever the pass did.

`DiffOptions` has two switches. `renames: false` turns the pass off and always carries the note
`rename-pass-off`. `canonical_ids_equal: true` is a caller's statement that both sides load to
equal canonical content ids ([content-id note](../../../docs/design/content-id.md)): names are
inside the id, so no block can be left unmatched and the pass is skipped. Equal ids prove nothing
about parameters, comments or authored order, so everything else is compared in full. If blocks
are unmatched on both sides all the same, the statement was wrong for these inputs and the diff
says `rename-pass-off`.

## The data: `oce-diff/1`

A `Diff` holds `schema` (`DIFF_SCHEMA`), `source` (`authoring`), `catalog` (the content id of the
catalog the defaults were read from), `base` and `head` (`sequence`, `derived_from`, `null` for
none), `summary`, `notes` and four sections. Each section is sorted by subject bytewise, then by
kind in the order below, then by the rest of the entry. Every enum is `#[non_exhaustive]` and is
written as a kebab-case string; every object is closed.

| Section | Kinds, in order | Subject |
| --- | --- | --- |
| `parameters` | `added`, `removed`, `changed`, `type-changed`, `reordered` | a sequence parameter, or `params` |
| `interface` | `added`, `removed`, `type-changed`, `unit-changed`, `quantity-changed`, `display-unit-changed`, `min-changed`, `max-changed`, `reordered` | a boundary, or `inputs` / `outputs` |
| `graph` | `added`, `removed`, `renamed`, `reparameterized`, `rewired`, `reordered`, `parameter-defaulted` | a block, `block.param`, `block.port`, a boundary output, `blocks` or `connections` |
| `metadata` | `sequence-renamed`, `derived-from-changed`, `semantic-changed`, `comment-changed`, `literal-respelled` | `sequence`, `derived_from`, `boundary.field`, a statement, `sequence.trailing` |

- **`summary`** holds `equal` (every section empty), `graph_equal`, `interface_equal`,
  `parameters_equal` and `metadata_equal` (each: its section is empty; the last two are `null`
  from a level that cannot tell) and `blocks` (`by_name`, `renamed`, `base_only`, `head_only`).
  A site retune is `graph_equal && !parameters_equal`.
- **`notes`**: `names-disjoint` (both sides have blocks and none kept its name and class),
  `rename-pass-skipped`, `rename-pass-off`.
- **A value** is one object per kind: `{"real", "bits"}`, `{"integer"}`, `{"boolean"}`,
  `{"text"}`, `{"ref"}`, `{"member"}`. A Real is its shortest round-trip spelling and its bits
  (`{ "real": "0.6", "bits": "0x3fe3333333333333" }`), never a JSON number.
- **Connections** are reported once, at their target. An added or removed block carries its
  driven inputs; any other target whose driver moved is `rewired`, compared after renames are
  mapped. A driver on a block that is not matched is compared by its spelling.
- **A sequence parameter** change lists as `affects` the `block.param` assignments that reference
  it (head side; base side for a removed parameter) and is not repeated in the graph section.
- **One value, two spellings** is metadata (`literal-respelled`), not a change: an Integer
  literal against the equal Real where a Real is expected (a `Real` sequence parameter, a block
  parameter the catalog types Real, a bound of a `Real` boundary), or a changed unit suffix.
- **`parameter-defaulted`** is an assignment on one side only whose literal equals the catalog's
  literal default. Against a derived default, a required parameter, an array element, a
  reference or an unknown class it is `reparameterized` with the other side `null`.
- **`reordered`** is computed over elements matched by name, at most one entry per list. The
  graph kind carries `list` (`blocks`, `connections`, `assignments`), so a block named `blocks`
  is not mistaken for the block list.
- **Comments** are compared on statements matched by name, as the model holds them (the text
  after `#`, the leading space included), lines joined by a line feed.

One thing in a model is not compared: whether a blank line precedes a statement. Every other
field is, so an empty diff means the two models are equal up to blank lines.

## What an invalid model means

The diff never validates, so a model may hold what the validator refuses. Each case has one fixed
reading, and the diff of any model against itself is empty.

- A name used more than once in a list is matched occurrence by occurrence. A connection that
  names a repeated block means the first.
- A connection end that names no declared block is kept as spelled and never matched.
- A target with several drivers is compared as a multiset: the shared drivers are set aside and
  what is left is paired in sorted order, a side that runs out reading as undriven.

## The renderers

Both are pure functions of the data, golden-tested, and write fixed templates only.

`render_text` writes one line per entry under `#` section headers, in the text form's own syntax:
`+` added, `-` removed, `>` renamed, `~` changed, and a trailing `# was …`. An added or removed
block is followed by one line per driven input.

```text
# oce diff zone_pressure_request -> zone_pressure_request
# graph differs; parameters equal; interface equal; metadata differs
# blocks: 4 by name, 1 renamed, 1 removed, 1 added
# graph
~ damOpen.t = 0.85   # was 0.95
- block openLong = CDL.Logical.TrueDelay(delayTime = 60, delayOnInit = true)
- damOpen.y -> openLong.u
> block setFrac = CDL.Reals.MultiplyByParameter(k = 0.7)   # was seventySet; renamed, exact
+ block starvedLong = CDL.Logical.TrueDelay(delayTime = 120, delayOnInit = true)
+ starved.y -> starvedLong.u
~ damOpen.y -> want.u1   # was openLong.y
~ starvedLong.y -> want.u2   # was starved.y
# metadata
~ # comment on sequence changed
```

`render_markdown` writes a `Change | Subject | Was | Now` table, one row per entry.

No string in the data can add a line or a cell: a control character is written `\u{…}`, and a
value that could not sit in a Markdown code span is written as escaped prose.

## Cost

The diff is near-linear in the two models: every lookup goes through an ordered index built once.
The rename pass adds at most eight passes over the links at no more than 2,048 unmatched blocks a
side. Both are pinned in counted work (key comparisons and elements visited, compiled into the
crate's own test build), never in elapsed time: `src/work_tests.rs` holds two models at the text
form's statement limits and a bucket at the pair cap to bounds a scan per lookup or an evaluation
pair by pair would break.

## Tests and goldens

`tests/fixtures/examples/` holds the note's three worked examples and the literal variant of the
first as checked-in pairs in the text form (`*.base.oce`, `*.head.oce`), and `every_kind`, a pair
whose diff holds every kind of every section. Each has golden JSON, text and Markdown
(`*.diff.json`, `*.diff.txt`, `*.diff.md`), compared byte for byte.

```bash
cargo nextest run -p oce-diff --locked --profile ci --no-tests=fail
OCE_BLESS_DIFF=1 cargo nextest run -p oce-diff   # rewrite the goldens, then run again without
```

Every diff records the catalog's content id, so a catalog change moves the `catalog` line of each
JSON golden. `tests/note_examples.rs` is the oracle: the examples' expected JSON, text and table
rows typed by hand from the note, never blessed.

## Parts 2 to 4

- **The CXF adapter** (#666) lowers two `oce_api::Topology` views into the same level-neutral
  side the authoring diff lowers into (`src/wiring`), so it gets the same matching, rename pass
  and `added` / `removed` / `renamed` / `rewired` entries, and builds a `Diff` with
  `source: cxf` and `parameters_equal` and `metadata_equal` `null`. Both renderers already write
  such a diff.
- **The blob comparison** (#667) is a function of its own over `oce_api::inspect_blob`; it
  shares the crate and nothing of the matching.
- **`oce diff`** (#668) puts a `Diff` in the CLI's report. `Diff::to_json` writes the report's
  spelling (sorted keys, two-space indentation), and `Diff` reads back with `serde`.
