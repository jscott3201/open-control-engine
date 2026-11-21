# oce-diff

A structural diff of two revisions of a control sequence. It answers "what changed" as data a
receipt, a bundle card or an agent can read, with authored block names as the identity of a block.
Design note: [`docs/design/diff.md`](../../../docs/design/diff.md), which is the specification of
everything here.

Two of four parts are in. **The authoring diff** (issue #665): `diff` compares two authoring
models and returns a sorted, schema-tagged `Diff`; the bounded rename pass presents renamed blocks;
two pure renderers turn the data into text and a Markdown table. **The CXF adapter** (issue #666):
`diff_topology` gives the same data for two loaded CXF documents, compared through the facade's
`Topology` views ([below](#the-cxf-adapter)). The blob comparison (#667) and the `oce diff` verb
(#668) build on them.

It is a leverage crate: among `oce-*` packages it depends on `oce-api` and `oce-author` only. It
never validates and never evaluates: it takes parsed models or loaded topologies as they are, reads
the catalog only for a parameter's literal default and type and the facade only for a block's port
names, loads no engine, opens no file and is not published.

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

```rust
use oce_api::Engine;
use oce_diff::{DiffOptions, diff_topology};

// The caller loads each document into a throwaway engine; the crate loads nothing.
let (mut base, mut head) = (Engine::new(), Engine::new());
base.load_cxf(&std::fs::read("reset.v1.jsonld")?)?;
head.load_cxf(&std::fs::read("reset.v2.jsonld")?)?;
let changes = diff_topology(&base.topology(), &head.topology(), &DiffOptions::default())?;
assert_eq!(changes.summary.parameters_equal, None); // a retune is on the leaves it reaches
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

`DiffOptions` has one switch and one hint. `renames: false` turns the pass off and always carries
the note `rename-pass-off`. `canonical_ids_equal: true` is a caller's statement that both sides
load to equal canonical content ids ([content-id note](../../../docs/design/content-id.md)), and
it **never changes the result**. Names and classes are inside the id, so with equal ids no block
is left unmatched and the pass has nothing to do: that is the whole fast path, and `diff` takes it
whenever a side has nothing left, hinted or not. Equal ids prove nothing about parameters,
comments or authored order, so everything else is compared in full. If the statement is wrong and
blocks are unmatched on both sides, the pass runs as it would without the hint.

## The CXF adapter

`diff_topology(base, head, options)` compares two loaded documents through their
`oce_api::Topology` views (design note decisions 3, 8 and 15). The caller loads the engines; the
crate takes `&Topology` and never reads CXF bytes. It lowers each view into the same level-neutral
side the authoring diff lowers into (`src/wiring`), so a block is matched, renamed and rewired
exactly as at the authoring level, and it builds a `Diff` with `source: cxf`.

- **Names.** A block or boundary is named by its path with the root prefix stripped: the root id
  (`Topology::model_id`), a `.`, and a non-empty name with no `#`, `/` or `:`, which may hold
  dots of its own (a flattened nested block is `sub.block`). That is the modelica-json convention
  the emitter and the corpus follow; the loader does not check it, and the adapter never guesses a
  root from the paths (the blocks of a one-block document share that block's whole path). The
  delimiters are excluded because the content id names an IRI by what follows its last `#`, `/`
  or `:`: without one in the name, the id names the element `{sequence}.{name}` and this level
  names it `name`, so the two agree. A path that breaks the rule is refused with
  `DiffError::OutsideRoot` (`outside-root`): a block or boundary named in another namespace or
  with a delimiter in its name, every block of a document the engine exported (whose root is the
  exporter's fixed `@id` while its instance IRIs keep their own, so it has the original's content
  id and is still refused; [#717](https://github.com/jscott3201/open-control-engine/issues/717)
  is to make it diffable), and any block or boundary of a model with a synthetic root key. Diffing
  by whole path instead is rejected: an export against its source would read as a total rewrite
  (owner ruling of 2026-11-21). Base is checked first.
- **Unnamed blocks.** A path that is `b` and decimal digits is the facade's spelling of a block
  with no instance IRI. It is loader order, no identity across two documents, so such a block is
  never matched by name, only by the rename pass, its entries carry that spelling, and the diff
  carries the note `unnamed-blocks`. No public load path produces one.
- **Ports by index.** A block's connector paths are positional (an input a boundary feeds carries
  the boundary's path), so port `i` is the `i`-th name `oce_api::class_ports` gives for the
  block's class and ground parameters. A block whose ports cannot be named that way is refused
  with `DiffError::UnresolvedPorts` (`unresolved-ports`); no loaded document has one.
- **Ground values.** A block's parameters are the values its document bound, grounded: a
  sequence-parameter retune is `reparameterized` on each leaf it reaches, so `parameters_equal` is
  `null`. A value on one side only is `parameter-defaulted` against the catalog's literal default,
  as at the authoring level. An Integer literal against the equal Real is a change of value here:
  the loaded graph holds two values and the content id moves (issue #683 will make the engine
  hold the Real). An entry lists a block's ground parameters by name.
- **Interface.** A boundary input is in a topology only when something reads it and an output
  only when something drives it. An input's `min` and `max` are its effective bounds (the
  declaration's intersected with every port it feeds; an Integer input left open is the `i32`
  range). An output carries no bounds: the engine enforces none and the id leaves them out.
- **Not seen.** Comments, semantic fields other than `unit`, `derived_from` (always `null`) and
  authored order are not in a loaded graph, so no entry is `reordered`. Neither are a `unit`,
  `quantity` or `displayUnit` on a boundary that is not a Real, the spelling of an
  Integer-written bound on a Real boundary (held as the Real), or a member of an Integer constant
  package (held as its Integer).
- **The sequence** of each side is its root id after the namespace (the IRI after its last `#`,
  `/` or `:`, as the content id names an IRI). Every name is stripped of the root, so a renamed
  root is no graph entry. It is reported as at the authoring level, a `sequence-renamed` metadata
  entry, so the diff is not `equal` and `oce diff` will exit 1 for it (owner ruling of
  2026-11-21). That entry is the only metadata this level sees: `metadata_equal` is `false` beside
  it and `null` (unknown) without it, and the strict reader accepts a CXF diff in exactly those two
  shapes, with `parameters_equal` `null`.

**Content ids.** Equal canonical content ids and equal sequence names never coexist with a graph
or interface entry: a topology holds every fact the id encodes, and under the name rule above
the id names every element `{sequence}.{name}`. Equal ids alone are not enough: the id never
encodes the root id, so a document whose root `@id` alone moved to a shorter dot-prefix of its
instance IRIs keeps its id, has another sequence name, and names every element differently here.
The converse does not hold either: a renamed root moves the id and is a `sequence-renamed` entry
alone (two corpus documents differ only so), and an interface-only change moves the id with the
graph section empty. `diff_topology_with_ids` takes the two ids beside the two topologies and
records whether they are equal as `DiffOptions::canonical_ids_equal`, a hint that never changes a
result: it returns exactly what `diff_topology` returns, whatever ids it is given.

**Across the levels.** For two authoring models that emit and load, the CXF diff of the two
loaded documents equals the authoring diff of the two models *as loaded*: references replaced by
the values they ground to and sequence parameters dropped, an Integer on a Real parameter widened
as the emitter writes it, constant-package members as their Integers, an input nothing reads and
an output nothing drives dropped, output bounds dropped, Integer inputs at their effective domain.
They agree on the graph and interface sections (`reordered` aside), their two flags, the block
counts, the notes, the sequence names, the `sequence-renamed` entry and `equal` once those are
the only entries counted, with the rename pass on and off.
`tests/cxf_cross_level.rs` holds it over every ordered pair of the 42 tracked authoring fixtures
that load and over eleven edits of each.

## The data: `oce-diff/1`

A `Diff` holds `schema` (`DIFF_SCHEMA`), `source` (`authoring` or `cxf`), `catalog` (the content id of the
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

- **`summary`** holds `equal`, the four section flags `graph_equal`, `interface_equal`,
  `parameters_equal` and `metadata_equal` (the last two `null` from a level that cannot tell),
  and `blocks` (`by_name`, `renamed`, `base_only`, `head_only`). A section flag is true when its
  section holds no entry other than `reordered`: authored order is not semantic, so a reordered
  list is still the same structure. `equal` is true only when every section is empty. So `equal`
  is **not** the four flags together: a diff of reorderings alone has all four true and `equal`
  false. "Same structure?" reads `graph_equal`; "nothing at all changed?" reads `equal`. A site
  retune is `graph_equal && !parameters_equal`.
- **`notes`**: `names-disjoint` (both sides have blocks and none kept its name and class),
  `rename-pass-skipped`, `rename-pass-off`, `unnamed-blocks` (CXF only).
- **A value** is one object per kind: `{"real", "bits"}`, `{"integer"}`, `{"boolean"}`,
  `{"text"}`, `{"ref"}`, `{"member"}`. A Real is its shortest round-trip spelling and its bits
  (`{ "real": "0.6", "bits": "0x3fe3333333333333" }`), never a JSON number.
- **Connections** are reported once, at their target. An added or removed block carries its
  driven inputs; any other target whose driver moved is `rewired`, compared after renames are
  mapped. A driver on a block that is not matched is compared by the block and port names as
  written.
- **A sequence parameter** change lists as `affects` the `block.param` assignments that reference
  it (head side; base side for a removed parameter) and is not repeated in the graph section.
- **One value, two spellings** is metadata (`literal-respelled`), not a change: an Integer
  literal against the equal Real where a Real is expected (a `Real` sequence parameter, a block
  parameter the catalog types Real, a bound of a boundary both sides type `Real`; exact up to
  2^53), or a changed unit suffix or written number. When a value and its unit suffix both
  change, both are reported: the value in its section and the two spellings here.
- **`parameter-defaulted`** is an assignment on one side only whose literal equals the catalog's
  literal default. Against a derived default, a required parameter, an array element, a
  reference or an unknown class it is `reparameterized` with the other side `null`.
- **`reordered`** is computed over elements matched by name, at most one entry per list. The
  graph kind carries `list` (`blocks`, `connections`, `assignments`), so a block named `blocks`
  is not mistaken for the block list.
- **Comments** are compared on statements matched by name, as the model holds them (the text
  after `#`, the leading space included), lines joined by a line feed.

One thing in a model is not compared: whether a blank line precedes a statement. Every other
field is, and as a field, never as its spelling: a boundary named `a.b` is not the port `b` of
`a`, and a written number no spelling shows is still a difference. So an empty diff means the two
models are equal up to blank lines, for any model the types can hold;
`tests/model_coverage.rs` holds that against the model's own equality.

Reading a `Diff` back with `serde` is strict: every object is closed and every key required (a
value that may be absent is written `null`), `schema` must be `oce-diff/1`, a Real's spelling
must be the spelling of its bits, and the five summary flags must be what the sections give. The
block counts and the order of the entries are taken as written.

## What an invalid model means

The diff never validates, so a model may hold what the validator refuses. Each case has one fixed
reading, and the diff of any model against itself is empty.

- A name used more than once in a list is matched occurrence by occurrence. A connection that
  names a repeated block means the first.
- A connection end that names no declared block is kept as written and never matched. Ends are
  compared as what they are (a boundary, or a block and a port), never as text.
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
pair by pair would break. A CXF diff stays within 16 units per topology element (block, port,
ground parameter, connection, boundary) and bit of their count, the rename pass's rounds included
(`src/topology/topology_tests.rs`, chains at the rename cap, one past it and a sixteenth of it);
the facade's naming of each block's ports is outside the count. When the diff succeeds that
naming costs as much as the counted ports, since each named port is one; before a refusal it is
one block build of at most `MAX_RESOLVED_ARITY` (2^21) ports, the bound every block constructor
holds.

## Tests and goldens

`tests/fixtures/examples/` holds the note's three worked examples and the literal variant of the
first as checked-in pairs in the text form (`*.base.oce`, `*.head.oce`), and `every_kind`, a pair
whose diff holds every kind of every section. Each has golden JSON, text and Markdown
(`*.diff.json`, `*.diff.txt`, `*.diff.md`), compared byte for byte, and each worked example has
the same three of its CXF diff, both sides emitted and loaded (`*.cxf.diff.json`, `.txt`, `.md`).

```bash
cargo nextest run -p oce-diff --locked --profile ci --no-tests=fail
OCE_BLESS_DIFF=1 cargo nextest run -p oce-diff   # rewrite the goldens, then run again without
```

Every diff records the catalog's content id, so a catalog change moves the `catalog` line of each
JSON golden. Two oracles are never blessed. `tests/note_examples.rs` holds the examples' expected
JSON, text and table rows, typed by hand from the note. `tests/rename_oracle.rs` holds a second
implementation of the note's rename rules, written pair by pair as the note states them and
sharing nothing with the crate's, and requires the two to agree on 135,168 enumerated pairs of
small models, sides a validator refuses among them. `tests/model_coverage.rs` edits every field of
the model one at a time and requires each edit to be reported in the section it belongs to.

The CXF adapter's tests: `tests/cxf_examples.rs` (the note's adapter test and the goldens),
`tests/cxf_cross_level.rs` (the agreement above), `tests/cxf_corpus.rs` (every document of the
content-id pin, the 46 G36 catalog fixtures among them, against itself, a second load, four
re-spelled copies and a re-rooted copy; equal ids and names against every entry over the
corpus's pairs; a delimiter in a name refused in every document; every port list reversed, the
id and the diff agreeing on each of the 47), `tests/cxf_identity.rs` (roots, refusals, the
unnamed block, topologies no load writes), `tests/cxf_values.rs` (each value kind and entry
kind) and `tests/cxf_metadata.rs` (the renamed root and what the reader accepts).

## Parts 3 and 4

- **The blob comparison** (#667) is a function of its own over `oce_api::inspect_blob`; it
  shares the crate and nothing of the matching.
- **`oce diff`** (#668) puts a `Diff` in the CLI's report. `Diff::to_json` writes the report's
  spelling (sorted keys, two-space indentation), and `Diff` reads back with `serde`. For two
  `.jsonld` files it loads each into a throwaway engine and calls `diff_topology_with_ids` with
  the two topologies and content ids; a `DiffError` is an error (exit 2), like a load refusal.
