# Structural diff (`oce-diff`)

**Design note, written before any diff code (GitHub #437).** It fixes what `oce-diff` compares, how
a block keeps its identity across two revisions, the change categories, the data and text it
produces, and how a site retune is told apart from a graph change. Decisions 1 to 3 are the owner's
and decisions 4 to 8 were accepted with them from the prior-art review; both are stated here, not
reopened. Decisions 9 to 16 were made while drafting and accepted by the architect pass (see
[Review rulings](#review-rulings)). The model it diffs is the one in the
[authoring layer note](authoring-layer.md).

**As landed.** The authoring diff, the rename pass and the two renderers are built as
`crates/leverage/oce-diff` (issue #665, GitHub #692,
[crate README](../../crates/leverage/oce-diff/README.md)), the CXF adapter as its
`diff_topology` (issue #666, GitHub #715) and the blob comparison as its `compare_blobs` (issue
#667, GitHub #PR); the `oce diff` verb is #668. What building them showed is listed under
[As landed](#as-landed).

The owner's work plan names the M3 deliverable as "a structural diff of two authoring models or CXF
documents, used in receipts and bundle cards", with `oce diff` in the CLI. Its consumers are M3
receipts (plain data with machine-stable codes), library bundle cards (a "what changed from the
base" section), agent explanations (a code, a subject and a detail an agent can quote, as the
validator's findings give) and the registry's `derived_from` view.

## What `oce-diff` is, and is not

It is a **two-way comparison of two authoring models** (M2 `Sequence` values, from the text or the
wire form) that returns data, plus pure renderers that turn the data into text.

- **Primary at the authoring level.** Authored names are the only block identity that survives a
  re-emit; sequence-level parameters and comments exist only there.
- **A thin adapter at the CXF level.** Each document is loaded into a throwaway `Engine` and the two
  `oce_api::Topology` views are compared keyed by IRI, root prefix stripped. Order is not
  compared: `Topology::boundary_outputs` "is NOT the authored `hasOutput` array order … key by
  path, never by index", and `BlockId` is loader order.
- **Same-or-different at the blob level.** Two OCEB blobs compare by content id, by the class table
  (class path and class signature hash) and by the per-instance initial-state CRCs, nothing more.
  Blob ids are declaration-order numbers that renumber whenever order or topology moves.

It is **not** an evaluator: it never grounds a reference, runs a block, schedules, or loads the
authoring model into an engine. It never validates either, so an agent's broken draft can be diffed
against the reference it is repairing (decision 10). It is not three-way: a diff against the
`derived_from` base waits for M3. It contains no LLM and writes no prose beyond fixed templates;
narrative belongs to the agent or card layer above it. It lives in its own leverage crate,
`crates/leverage/oce-diff` (decision 9), and is surfaced as `oce diff A B`.

## Identity

**The authored block name is the identity** (decision 4): the local part of
`{namespace}{sequence}.{local}`. Blocks are matched in three steps, each block at most once:

1. **Same name, same class path**: the same block, whatever its parameters or edges.
2. **Same name, different class path**: `removed` plus `added`; a class change is never a
   reparameterization.
3. **Rename pass** over the blocks still unmatched on each side, reporting `renamed` with a
   closed-enum confidence (decision 2).

The rename pass (decision 11) is deterministic and bounded:

- **Hard gates.** A candidate pair has the same class path and the same assignments: same names,
  values equal bit for bit (Reals by `f64::to_bits`, so `+0.0` and `-0.0` differ and a NaN equals
  only its own payload), the same referenced parameter, the same enum member. Assignment order is
  ignored here.
- **Neighbourhood.** Each port is compared through the current match. An endpoint is an
  **anchor** when it is a boundary present on both sides or an already-matched block. A port
  **agrees** when both endpoints map to the same anchor, **disagrees** when at least one is an
  anchor and they differ, and is **unknown** when both are unmatched blocks. An output port
  compares its set of targets the same way. Three cases this leaves open are read as
  disagreement, because no later round can make them agree: a port linked on one side only, a
  different number of endpoints, and an endpoint that is neither an anchor nor an unmatched block
  (a boundary declared on one side only, a block no statement declares).
- **Rounds.** A round collects the pairs with no disagreement and at least one agreement, and
  accepts a pair only when each side has exactly one such candidate. Accepted pairs are anchors
  for the next round. The pass stops when a round accepts nothing, or after 8 rounds. Mutual
  uniqueness makes the result independent of iteration order.
- **Confidence.** `exact` when every port of the pair agrees at the end, `high` when some port is
  still unknown. Ambiguous or disagreeing pairs are never reported; they stay `removed` plus
  `added`.
- **Bounds.** At most 2,048 unmatched blocks per side and 65,536 gated pairs. Past either cap the
  pass does not run, the diff carries the note `rename-pass-skipped`, and every unmatched block is
  removed or added. A cap is only passed when both sides have unmatched blocks: with nothing left
  on one side there is nothing to pair and no note. `--no-renames` turns the pass off (note
  `rename-pass-off`).
- **Boundaries are never renamed by the heuristic.** A renamed input or output breaks every binding
  to it, so it reads as removed plus added in the interface section.

**`graph_equal` never depends on the heuristic.** It is computed from steps 1 and 2 alone: a
renamed block has a different name, so any rename makes `graph_equal` false. The rename pass only
changes how a difference is presented.

## Categories and sections

A diff has four sections. **Graph** entries use one closed `#[non_exhaustive]` enum (decision 5),
serialized as stable kebab-case `kind` strings:

| Kind | Subject | Meaning |
| --- | --- | --- |
| `added` | head block | Present only in head; carries its class, assignments and input drivers |
| `removed` | base block | Present only in base; carries the same |
| `renamed` | head block | Matched by the rename pass; carries `from`, class, the head block's assignments and `confidence` |
| `reparameterized` | `block.param` | A value differs, or an assignment exists on one side only (that side `null`) and is not the catalog default |
| `rewired` | `block.port` or boundary output | The driver of a target changed (`old`, `new`; `null` when undriven) |
| `reordered` | list name or block | Same elements, different authored order; no semantic claim. Carries `list` (`blocks`, `connections`, `assignments`), so a block named `blocks` is told from the block list |
| `parameter-defaulted` | `block.param` | An assignment exists on one side only and equals the catalog's literal default |

Every connection change is reported **once, at its target**, because an input has one driver
(decision 12). An added or removed block carries its own input drivers; any other target whose
driver moved is `rewired`. Drivers are compared after mapping renames, so a renamed source alone
does not rewire its targets.

`reordered` is its own kind because it is not semantic: the canonical content id
([content-id note](content-id.md)) ignores authored order, but under the [ordering
contract](../cxf-composite-subset.md#ordering-and-identity-contract) `@graph` and `containsBlock`
order still assign `BlockId`, and the emitter keeps authored order. (Before GitHub #470 order also
moved the export content id; that order-bearing id is retired.) It is computed over
name-matched elements only, at most one entry per list (`blocks`, `connections`, a block's
assignments).

**Parameters** (sequence-level `param`): `added`, `removed`, `changed`, `type-changed`, `reordered`.
**Interface** (boundary inputs and outputs): `added`, `removed`, `type-changed`, `unit-changed`,
`quantity-changed`, `display-unit-changed`, `min-changed`, `max-changed`, `reordered`. **Metadata**: `sequence-renamed`,
`derived-from-changed`, `semantic-changed` (subject such as `TOut.brick`), `comment-changed` and
`literal-respelled` (an Integer literal on a Real parameter, or a unit suffix, changed with the
value unchanged). `unit` is interface because the emitter writes it as `S231:unit`; the other
`SemanticPoint` fields are not emitted before M8 and are metadata (decision 14).

## Site parameters and the graph

A sequence-level `param` change is reported in the **parameters** section, listing as `affects`
the `block.param` assignments that reference it (head side; base side for a removed parameter). It
is **not** repeated as `reparameterized` graph entries, or every site retune would appear twice
(decision 6). The summary carries `graph_equal` and `parameters_equal`; a site retune is
`graph_equal && !parameters_equal`, which is the card's and the `derived_from` view's headline:
"same structure, different tuning".

The same semantic edit lands in different sections depending on authoring style: raising a
threshold through `param phiHigh` is a parameters change, editing a literal `uHigh = 0.6` is a graph
`reparameterized`. That is intended; the diff reports what the author changed.

**The CXF and blob adapters cannot separate the two.** The authoring note's decision 3 lowers a
sequence parameter onto the root block and `oce-expr` grounds every reference at load, so a loaded
`Topology` holds only grounded leaf values. The CXF adapter reports a retune as `reparameterized`
on each affected leaf and sets `parameters_equal` to `null` ("not separable at this level"); a blob
comparison says only "different" (decision 15).

## Output

**Data.** A serde `Diff`: a `schema` string (`oce-diff/1`), the `source` (`authoring` or `cxf`),
the catalog content id the defaults came from, both sequence names and `derived_from` ids (`null`
for none), a `summary`, closed-enum `notes`, and the four sections (decision 7). Each section is
sorted by subject bytewise, then kind in enum order, then detail; nothing is in hash order. Reals are carried
as the canonical printer's shortest round-trip spelling plus their bit pattern, so no float is
parsed back from JSON (decision 13). `equal` is true only when every section is empty.

**Text.** `render_text(&Diff)` is a pure function of the data: one line per entry, grouped by
section under `#` headers, in the text form's own syntax, with `+` added, `-` removed, `>` renamed,
`~` changed, and a trailing `# was …` for the old value. **Markdown.** `render_markdown(&Diff)`
writes a `Change | Subject | Was | Now` table for a bundle card. Both are golden-tested.

**CLI.** `oce diff <base> <head> [--from text|wire|cxf] [--no-renames]
[--format human|json|markdown]`. `.oce` and `.json` are authoring models, `.jsonld` is CXF; both
sides must be the same kind. The JSON report is the CLI's closed envelope (`tool`, `schema`,
`command`, `ok`) with the `Diff` as its result. Exit codes follow the **`oce` CLI rule for every
subcommand** (decision 1): **0** yes (here, `equal`), **1** no (any difference), **2** error (usage,
a parse refusal of either input, a CXF load refusal, I/O). Exit 1 never means a crash. The
[authoring note's CLI section](authoring-layer.md#cli) follows the same rule.

## Relation to content ids

The canonical content id ([content-id note](content-id.md), GitHub #470) is `oce:canon1:` and the
SHA-256 of a canonical encoding of the **loaded flat graph**: boundary I/O definitions, blocks by
authored name with class, class signature hash and bound parameters sorted by name, and connections
by port index. Authored order, JSON spelling and the instance namespace never enter it; authored
names do (owner, 2026-11-09), so a rename moves it and this crate's rename pass explains the
change. It replaces the order-bearing export tag (`cxf:fnv1a128:` over the emitted bytes), and
PC-021 keeps it apart from the authored model id.

**Equal canonical ids prove the two loaded graphs are equal** (decision 8, as narrowed by the
content-id review). So the CXF adapter and the blob comparison may return graph-equal without
matching, and an authoring diff whose two sides load to equal ids may skip its rename pass. They do
**not** prove an empty authoring diff: a sequence-parameter rename, a literal against a reference
with the same ground value, `derived_from`, comments, `SemanticPoint` fields and `reordered` entries
are all outside the id. Unequal ids still need the full diff.

## Rust API sketch

```rust
pub const DIFF_SCHEMA: &str = "oce-diff/1"; // also MAX_RENAME_{CANDIDATES,PAIRS,ROUNDS}

pub fn diff(base: &Sequence, head: &Sequence, options: &DiffOptions) -> Diff;
pub fn diff_topology(base: &Topology, head: &Topology, options: &DiffOptions)
    -> Result<Diff, DiffError>;
pub fn compare_blobs(base: &[u8], head: &[u8]) -> Result<BlobComparison, DiffError>;
pub fn render_text(diff: &Diff) -> String;
pub fn render_markdown(diff: &Diff) -> String;

#[non_exhaustive]
pub struct Diff {
    pub schema: String,
    pub source: Source,        // Authoring | Cxf
    pub catalog: String,       // catalog content id
    pub base: SequenceRef,     // { sequence, derived_from }
    pub head: SequenceRef,
    pub summary: Summary,      // equal, graph_equal, parameters_equal: Option<bool>, ...
    pub notes: Vec<Note>,      // NamesDisjoint | RenamePassSkipped | RenamePassOff | ...
    pub parameters: Vec<ParameterChange>,
    pub interface: Vec<InterfaceChange>,
    pub graph: Vec<GraphChange>,
    pub metadata: Vec<MetadataChange>,
}

#[non_exhaustive]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum GraphChange {
    Added { subject: String, class: String, params: Vec<Assignment>, inputs: Vec<Driver> },
    Removed { subject: String, class: String, params: Vec<Assignment>, inputs: Vec<Driver> },
    Renamed { subject: String, from: String, class: String, params: Vec<Assignment>,
              confidence: Confidence },
    Reparameterized { subject: String, old: Option<Literal>, new: Option<Literal> },
    Rewired { subject: String, old: Option<String>, new: Option<String> },
    Reordered { subject: String, list: GraphList },
    ParameterDefaulted { subject: String, value: Literal, side: Side },
}

#[non_exhaustive]
pub enum Confidence { Exact, High }
```

`Summary` holds `equal`, `graph_equal`, `interface_equal`, `parameters_equal` and `metadata_equal`
(the last two `Option<bool>`; from the CXF adapter `parameters_equal` is `None` and
`metadata_equal` is `false` beside a renamed root, `None` otherwise) and the block counts
(`by_name`, `renamed`, `base_only`, `head_only`). `Literal` mirrors the model's tagged literal (`{"real",
"bits"}`, `{"integer"}`, `{"boolean"}`, `{"text"}`, `{"ref"}`, `{"member"}`); `Driver` is
`{port, source}`; `DiffOptions` holds `renames` and `canonical_ids_equal` (the caller's statement
behind decision 8's fast path, a hint that never changes the result); the three rename caps are
public constants.
`BlobComparison` holds `same_bytes`, a `content_id` comparison (`same`, `different`, or `unknown`
when a blob records none), class-table changes by class path (`added`, `removed`,
`signature-changed`) and an `initial_state` comparison (`unknown` unless the block lists align by
class). `diff` is infallible because both inputs are parsed models; `DiffError` (typed,
`#[non_exhaustive]`) covers a topology whose blocks share no single root prefix and a blob the
facade refuses to decode.

## Worked examples

Recreated in the M2 text form from three revision pairs in the earlier prototype's history: a
sequence-sheet revision, a spec's version 2, and an agent's draft against its reference. Example
(a) shows the whole JSON; (b) shows the summary, notes and graph section, and (c) its graph
entries only (its notes are `["names-disjoint"]`); the other fields are as in (a).

### (a) A site retune: two thresholds change

The dehumidification override of a supply-air temperature reset, reduced to the part that changes:

```text
# Dehumidification override, sheet revision A.
sequence ahu_dehum_override
derived_from none

param phiHigh : Real = 0.6
param phiLow : Real = 0.55

in phiRet : Real @unit 1
out dehumOn : Boolean

block dehum = CDL.Reals.Hysteresis(uLow = phiLow, uHigh = phiHigh)

phiRet -> dehum.u
dehum.y -> dehumOn
```

Revision B differs in its comment (`sheet revision B`) and two lines: `param phiHigh : Real = 0.7`
and `param phiLow : Real = 0.65`.

```json
{
  "schema": "oce-diff/1",
  "source": "authoring",
  "catalog": "catalog:1:fnv1a128:<32 hex>",
  "base": { "sequence": "ahu_dehum_override", "derived_from": null },
  "head": { "sequence": "ahu_dehum_override", "derived_from": null },
  "summary": { "equal": false, "graph_equal": true, "parameters_equal": false,
               "interface_equal": true, "metadata_equal": false,
               "blocks": { "by_name": 1, "renamed": 0, "base_only": 0, "head_only": 0 } },
  "notes": [],
  "parameters": [
    { "kind": "changed", "subject": "phiHigh", "type": "Real", "affects": ["dehum.uHigh"],
      "old": { "real": "0.6", "bits": "0x3fe3333333333333" },
      "new": { "real": "0.7", "bits": "0x3fe6666666666666" } },
    { "kind": "changed", "subject": "phiLow", "type": "Real", "affects": ["dehum.uLow"],
      "old": { "real": "0.55", "bits": "0x3fe199999999999a" },
      "new": { "real": "0.65", "bits": "0x3fe4cccccccccccd" } }
  ],
  "interface": [],
  "graph": [],
  "metadata": [ { "kind": "comment-changed", "subject": "sequence",
                  "old": " Dehumidification override, sheet revision A.",
                  "new": " Dehumidification override, sheet revision B." } ]
}
```

```text
# oce diff ahu_dehum_override -> ahu_dehum_override
# graph equal; parameters differ; interface equal; metadata differs
# parameters
~ param phiHigh : Real = 0.7   # was 0.6; affects dehum.uHigh
~ param phiLow : Real = 0.65   # was 0.55; affects dehum.uLow
# metadata
~ # comment on sequence changed
```

Authored as literals (`uLow = 0.55, uHigh = 0.6`), the same retune is two graph `reparameterized`
entries, `dehum.uHigh` and `dehum.uLow`, and `graph_equal` is false.

### (b) Added, removed and rewired, with one rename

Version 1 asks for more static pressure when the damper has been nearly open for a minute and the
airflow is short of 70 % of setpoint. Version 2 drops the damper hold, lowers the damper threshold,
asks only when the shortfall lasts two minutes, and renames the 70 % gain block.

Version 2 as a source diff of version 1 (unchanged lines once):

```diff
-# Zone static-pressure reset request, version 1.
+# Zone static-pressure reset request, version 2.
 sequence zone_pressure_request
 derived_from none

 in uDam : Real @unit 1
 in VDis_flow : Real @unit m3/s
 in VSet_flow : Real @unit m3/s
 out yReq : Boolean

-block damOpen = CDL.Reals.GreaterThreshold(t = 0.95, h = 0.1)
-block openLong = CDL.Logical.TrueDelay(delayTime = 60, delayOnInit = true)
-block seventySet = CDL.Reals.MultiplyByParameter(k = 0.7)
+block damOpen = CDL.Reals.GreaterThreshold(t = 0.85, h = 0.1)
+block setFrac = CDL.Reals.MultiplyByParameter(k = 0.7)
 block short = CDL.Reals.Subtract
 block starved = CDL.Reals.GreaterThreshold(t = 0.0, h = 0.01)
+block starvedLong = CDL.Logical.TrueDelay(delayTime = 120, delayOnInit = true)
 block want = CDL.Logical.And

 uDam -> damOpen.u
-damOpen.y -> openLong.u
-VSet_flow -> seventySet.u
-seventySet.y -> short.u1
+VSet_flow -> setFrac.u
+setFrac.y -> short.u1
 VDis_flow -> short.u2
 short.y -> starved.u
-openLong.y -> want.u1
-starved.y -> want.u2
+starved.y -> starvedLong.u
+damOpen.y -> want.u1
+starvedLong.y -> want.u2
 want.y -> yReq
```

Four blocks match by name. `openLong` and `starvedLong` share a class but `delayTime` differs, so
they are no candidate pair. `seventySet` and `setFrac` pass both gates; `u` is driven by
`VSet_flow` on both sides and `y` drives `short.u1` on both, so round 1 accepts the pair as
`exact`. Name-matched elements keep their relative order, so nothing is `reordered`.

```json
{
  "summary": { "equal": false, "graph_equal": false, "parameters_equal": true,
               "interface_equal": true, "metadata_equal": false,
               "blocks": { "by_name": 4, "renamed": 1, "base_only": 1, "head_only": 1 } },
  "notes": [],
  "graph": [
    { "kind": "reparameterized", "subject": "damOpen.t",
      "old": { "real": "0.95", "bits": "0x3fee666666666666" },
      "new": { "real": "0.85", "bits": "0x3feb333333333333" } },
    { "kind": "removed", "subject": "openLong", "class": "CDL.Logical.TrueDelay",
      "params": [ { "name": "delayTime", "value": { "integer": 60 } },
                  { "name": "delayOnInit", "value": { "boolean": true } } ],
      "inputs": [ { "port": "u", "source": "damOpen.y" } ] },
    { "kind": "renamed", "subject": "setFrac", "from": "seventySet",
      "class": "CDL.Reals.MultiplyByParameter",
      "params": [ { "name": "k",
                    "value": { "real": "0.7", "bits": "0x3fe6666666666666" } } ],
      "confidence": "exact" },
    { "kind": "added", "subject": "starvedLong", "class": "CDL.Logical.TrueDelay",
      "params": [ { "name": "delayTime", "value": { "integer": 120 } },
                  { "name": "delayOnInit", "value": { "boolean": true } } ],
      "inputs": [ { "port": "u", "source": "starved.y" } ] },
    { "kind": "rewired", "subject": "want.u1", "old": "openLong.y", "new": "damOpen.y" },
    { "kind": "rewired", "subject": "want.u2", "old": "starved.y", "new": "starvedLong.y" }
  ]
}
```

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

The card rendering (`render_markdown`) puts the same entries in a table, abridged here:

```markdown
| Change | Subject | Was | Now |
| --- | --- | --- | --- |
| reparameterized | `damOpen.t` | 0.95 | 0.85 |
| removed | `openLong` | `CDL.Logical.TrueDelay(delayTime = 60, delayOnInit = true)` | |
| renamed (exact) | `setFrac` | `seventySet` | `setFrac` |
| rewired | `want.u1` | `openLong.y` | `damOpen.y` |
```

### (c) A wholesale rename the diff cannot fully match

An agent's draft of a reference sequence renames every block, swaps the operands of the `And` and
doubles the delay.

```text
# Chilled-water supply temperature reset, reference.
sequence chw_plant_reset
derived_from none

in uReq : Boolean
in uPlaEna : Boolean
out TChiWatSupSet : Real @unit K

block ena = CDL.Logical.And
block hold = CDL.Logical.TrueDelay(delayTime = 300, delayOnInit = true)
block TLow = CDL.Reals.Sources.Constant(k = 279.15)
block THigh = CDL.Reals.Sources.Constant(k = 285.15)
block sel = CDL.Reals.Switch

uPlaEna -> ena.u1
uReq -> ena.u2
ena.y -> hold.u
TLow.y -> sel.u1
hold.y -> sel.u2
THigh.y -> sel.u3
sel.y -> TChiWatSupSet
```

The draft has the same header and boundaries, the comment `agent draft`, and:

```text
block plaOn = CDL.Logical.And
block onDelay = CDL.Logical.TrueDelay(delayTime = 600, delayOnInit = true)
block TMin = CDL.Reals.Sources.Constant(k = 279.15)
block TMax = CDL.Reals.Sources.Constant(k = 285.15)
block sw = CDL.Reals.Switch

uReq -> plaOn.u1
uPlaEna -> plaOn.u2
plaOn.y -> onDelay.u
TMin.y -> sw.u1
onDelay.y -> sw.u2
TMax.y -> sw.u3
sw.y -> TChiWatSupSet
```

No block name is shared (note `names-disjoint`). Round 1 accepts `sel`→`sw` (`y` agrees on
`TChiWatSupSet`) and refuses `ena`→`plaOn`, whose inputs are driven by boundary anchors, crossed.
The constants have no agreeing port yet; round 2 accepts `TLow`→`TMin` and `THigh`→`TMax`
through `sw.u1` and `sw.u3`, and round 3 accepts nothing. `hold` and `onDelay` were never
candidates, because `delayTime` differs.

```json
{
  "graph": [
    { "kind": "renamed", "subject": "TMax", "from": "THigh",
      "class": "CDL.Reals.Sources.Constant",
      "params": [ { "name": "k",
                    "value": { "real": "285.15", "bits": "0x4071d26666666666" } } ],
      "confidence": "exact" },
    { "kind": "renamed", "subject": "TMin", "from": "TLow",
      "class": "CDL.Reals.Sources.Constant",
      "params": [ { "name": "k",
                    "value": { "real": "279.15", "bits": "0x4071726666666666" } } ],
      "confidence": "exact" },
    { "kind": "removed", "subject": "ena", "class": "CDL.Logical.And", "params": [],
      "inputs": [ { "port": "u1", "source": "uPlaEna" }, { "port": "u2", "source": "uReq" } ] },
    { "kind": "removed", "subject": "hold", "class": "CDL.Logical.TrueDelay",
      "params": [ { "name": "delayTime", "value": { "integer": 300 } },
                  { "name": "delayOnInit", "value": { "boolean": true } } ],
      "inputs": [ { "port": "u", "source": "ena.y" } ] },
    { "kind": "added", "subject": "onDelay", "class": "CDL.Logical.TrueDelay",
      "params": [ { "name": "delayTime", "value": { "integer": 600 } },
                  { "name": "delayOnInit", "value": { "boolean": true } } ],
      "inputs": [ { "port": "u", "source": "plaOn.y" } ] },
    { "kind": "added", "subject": "plaOn", "class": "CDL.Logical.And", "params": [],
      "inputs": [ { "port": "u1", "source": "uReq" }, { "port": "u2", "source": "uPlaEna" } ] },
    { "kind": "renamed", "subject": "sw", "from": "sel",
      "class": "CDL.Reals.Switch", "params": [], "confidence": "high" },
    { "kind": "rewired", "subject": "sw.u2", "old": "hold.y", "new": "onDelay.y" }
  ]
}
```

```text
# oce diff chw_plant_reset -> chw_plant_reset
# graph differs; parameters equal; interface equal; metadata differs
# names disjoint: no block kept its name and class; 3 of 5 base blocks matched by the rename pass, 2 removed, 2 added
# graph
> block TMax = CDL.Reals.Sources.Constant(k = 285.15)   # was THigh; renamed, exact
> block TMin = CDL.Reals.Sources.Constant(k = 279.15)   # was TLow; renamed, exact
- block ena = CDL.Logical.And
- uPlaEna -> ena.u1
- uReq -> ena.u2
- block hold = CDL.Logical.TrueDelay(delayTime = 300, delayOnInit = true)
- ena.y -> hold.u
+ block onDelay = CDL.Logical.TrueDelay(delayTime = 600, delayOnInit = true)
+ plaOn.y -> onDelay.u
+ block plaOn = CDL.Logical.And
+ uReq -> plaOn.u1
+ uPlaEna -> plaOn.u2
> block sw = CDL.Reals.Switch   # was sel; renamed, high
~ onDelay.y -> sw.u2   # was hold.y
# metadata
~ # comment on sequence changed
```

The diff admits what it cannot see. `plaOn` is `ena` with commuted operands and `onDelay` is `hold`
retuned, but claiming either would need knowledge of `And`'s commutativity or a guess across a
changed parameter, so a reviewer reads two removals and two additions and decides. `high` marks the
rename whose neighbourhood was only partly seen (`sw.u2`'s driver was never matched).

## Decisions

Decisions 1 to 3 are the owner's; 4 to 8 are the prior-art review's recommendations, accepted with
them. All eight are settled. Decisions 9 to 16 were drafted here and accepted in review.

**1. One exit-code rule for every `oce` subcommand.** 0 is yes (same, pass, clean), 1 is no
(different, fail, findings), 2 is an error (bad input, I/O); exit 1 never means a crash. Rejected:
`diff` always exiting 0; per-subcommand tables. Why: scripts and agents branch the same way
everywhere.

**2. Rename detection.** A bounded one-to-one rename pass (class as hard gate, bit-exact
parameters, neighbourhood agreement, a candidate cap as a hostile-input bound) reports `renamed`
with a closed-enum confidence; `graph_equal` never depends on it. Rejected: removal plus addition
only. Why: a wholesale rename must not read as a rewrite.

**3. Scope of the first version.** Two-way, authoring model to authoring model; CXF through a thin
adapter over `Topology` keyed by IRI; blobs same-or-different by content id, class table and
initial-state CRC; three-way against `derived_from` waits for M3. Rejected: a structural CXF or blob
diff now (noisy: `BlockId` is loader order, boundary outputs are not in authored order); three-way
from the start (needs `derived_from` resolution, a base content id and `oce fetch`). Why: authored
names are the identity that survives a re-emit.

**4. Identity is the authored block name**, then a class change as removal plus addition, then the
rename pass. Rejected: a content id as block identity (it identifies a document). Why: authors
control names; the heuristic only presents leftovers.

**5. A closed `#[non_exhaustive]` graph category enum**, interface and metadata in their own
sections, `reordered` separate because order is not semantic (and, since GitHub #470, does not move
the canonical content id either). Rejected: a
JSON-Patch-style path list. Why: cards and agents branch on a category.

**6. Site parameters in their own section** with derived `affects`, and the `graph_equal` and
`parameters_equal` booleans. Rejected: each affected assignment as `reparameterized`. Why: every
retune would appear twice, and "same structure, different tuning" is the card's headline.

**7. Data first, rendered text second**: a sorted, schema-versioned serde `Diff` with kebab-case
codes, a text renderer in the text form's syntax, a Markdown table, no LLM. Rejected: free-prose
change lists like the prototype's "explain the fix" records. Why: prose is rendered from facts,
never facts scraped from prose.

**8. Equal canonical ids are a graph-level fast path**, narrowed by the content-id review (ruling
11, 2026-11-09): equal ids prove equal loaded graphs, so the CXF and blob comparisons return
graph-equal and the authoring diff skips its rename pass; they do not prove an empty authoring diff
(sequence parameters, literal-against-reference, `derived_from`, comments, `SemanticPoint` fields
and `reordered` are outside the id). M3 decided names are inside the id
([content-id note](content-id.md), decision 2).

**9. a sibling leverage crate, `crates/leverage/oce-diff`**, on `oce-author` and
`oce-api`, `#![forbid(unsafe_code)]`, `serde` its only new dependency, linked by `oce-cli`.
Rejected: a module in `oce-author`. Why: the leverage README already names `oce-diff`; `oce-author`
carries M2's gate and should not change for M3 work; receipts and cards need the diff without the
CLI; the CXF and blob adapters need facade calls `oce-author` does not make.

**10. the diff never validates**; it needs parsed models and reads the catalog only for
literal defaults. Rejected: diffing validated models only. Why: an agent repairing a draft needs
the diff against the reference precisely while the draft is invalid.

**11. the rename pass as specified under [Identity](#identity)** (anchors, per-port
agreement, mutual-unique acceptance, 8 rounds, `exact` and `high`, 2,048 blocks per side and
65,536 pairs, boundaries never renamed). Rejected: a scored greedy assignment with a float
threshold (order-sensitive ties, a float in receipts); an unbounded fixed point. Why: mutual
uniqueness is order-independent and easy to pin with goldens.

**12. each connection change is reported once, at its target**, added and removed
blocks carrying their input drivers, drivers compared after renames. Rejected: reporting both
endpoints. Why: an input has exactly one driver, and nothing is reported twice.

**13. Reals as spelling plus bits.** Rejected: JSON numbers. Why: a consumer's float
parse can lose `-0.0` or a NaN payload; the receipt states exactly what the engine compares.

**14. `unit` is interface, other `SemanticPoint` fields are metadata;** a one-sided
assignment is `parameter-defaulted` only against a literal catalog default, never an expression
default. Why: `unit` reaches the CXF and the rest does not before M8; the diff never evaluates.

**15. the CXF adapter** strips the root prefix, ignores order, reports
`parameters_equal: null`, and matches an unnamed block (`b<id>`) only through the rename pass (note
`unnamed-blocks`). **Blob comparison needs a facade accessor** for a blob's content id, class table
and initial-state CRCs: the decoder is `oce_graph::blob`, a core crate a leverage crate may not
call, so the accessor is engine work, GitHub #448. It landed as `oce_api::inspect_blob`
([inspecting a blob](../compiled-schedule-blob.md#inspecting-a-blob)): it reports hashes and CRCs
as the blob records them, without correcting them to the running build's, so a comparison can
show a class whose signature changed. Since issue #680 the summary also carries the running
build's verdict on each blob (`decode_refusal`: `None`, or the decoder's own error), which a blob
comparison reads to say whether the build that compares would run either side.

**16. `oce diff` exits 0 only when `equal`**, so a `reordered`-only or metadata-only diff
exits 1 and scripts that want "same structure" read `summary.graph_equal`. For every `oce`
subcommand, input that cannot be parsed or loaded exits 2 (bad input) and a parsed input whose
answer is "no" exits 1: `oce validate` with findings is 1, with a parse refusal 2. Rejected: a
parse refusal as 1 and I/O as 3, as the authoring note first had it. Why: decision 1, applied
the same way by every subcommand; the authoring note is aligned in the same PR as this note.

## Relation to other work

- **M2 (`oce-author`, text form, `oce` CLI).** The diff consumes `Sequence` as the authoring note
  defines it; if M2 changes name rules or how `param` is lowered, identity and the parameters
  section change with it. Implementation starts after `oce-author` lands.
- **M3 (canonical id, registry, `oce fetch`, three-way).** The canonical id gives the fast path and
  must settle whether names are inside it. A three-way diff against the `derived_from` base needs
  `derived_from` resolution in the registry, the base's canonical id (so "based on X" is checkable),
  `oce fetch <id>@<content-id>`, and a rule for what a derived sequence may change. Until then the
  `derived_from` view is two two-way diffs: base to derived, and base to its newer revision.
- **M8 (semantics).** Filled `SemanticPoint` records are diffed field by field as metadata; their
  shape is M8's.
- **Library bundle cards and agent explanations.** A card's "changes from base" section is
  `render_markdown`; an agent quotes `kind`, `subject` and the old and new values.
- **Implementing issues.** The facade blob accessor is GitHub #448, engine work independent of M3
  (decision 15), landed as `oce_api::inspect_blob`. The `oce-diff` crate, the `oce diff` verb and the card renderer are cut from the
  M3 tracking issue when it exists.

## Test plan

- **Golden diffs.** The three worked examples as checked-in pairs with golden JSON, text and
  Markdown, compared byte for byte; the literal variant of (a); each side diffed against itself
  (empty diff, exit 0).
- **Per category.** One minimal pair per kind in every section, asserting code and subject: a class
  change under one name, `-0.0` against `0.0`, a NaN payload change, an Integer literal against an
  equal Real (`literal-respelled`), an assignment dropped back to its literal default
  (`parameter-defaulted`), an input left undriven (`null` driver).
- **Rename pass.** Example (c) pins refusal on disagreement and on a changed parameter; two
  identical candidates for one block stay removed plus added; a chain needing several rounds
  matches and one needing more than 8 stops at 8; each cap is pinned at its boundary (accepted at
  2,048 blocks and 65,536 pairs, `rename-pass-skipped` one past); `--no-renames`. Every golden
  asserts `graph_equal` is the same with the pass on and off.
- **Determinism.** The same inputs give byte-identical JSON, text and Markdown across runs and
  platforms; permuting either side's statements changes only `reordered` entries; the rename
  result is invariant under every permutation of the examples' blocks (enumerated, no RNG).
- **Hostile sizes.** Two models at the parser's statement limits diff within a pinned work bound;
  worst-case candidate buckets hit the pair cap, not the CPU.
- **Adapters.** The CXF adapter over the emitted examples agrees with the authoring diff on graph
  entries after grounding and reports `parameters_equal: null`; a blob compared with itself is
  same, and a pinned class-signature change shows in `classes`.
- **CLI.** Exit 0, 1 and 2 per outcome class, and golden JSON envelopes.

## Out of scope

- Three-way diff and merge against a `derived_from` base (M3 and later).
- A structural diff of blobs, and any diff across levels (authoring against CXF).
- Matching blocks by semantic equivalence (commutativity, algebraic rewrites); any evaluation.
- Prose explanations, and any LLM call, inside the crate.
- Nested composites, which the M2 model does not author.

## As landed

What building the authoring diff (issue #665, GitHub #692) showed, each also written into the
section it belongs to. None of these is a new decision: each follows from a decision above once
it meets the model as it stands, and each was accepted in the review of GitHub #692. The worked
examples above are the crate's oracle: their JSON, text and table rows are typed by hand into
`crates/leverage/oce-diff/tests/note_examples.rs`, so the two are kept equal by hand, and a
difference is resolved here or in the code.

- **`min-changed` and `max-changed`.** The interface kinds were written before a boundary had
  bounds (importer note, decision 4). A bound on an input is inside the canonical content id, so
  a changed bound is an interface entry of its own kind.
- **`renamed` carries the block's assignments, and a parameter `changed` its type.** The text
  rendering is a pure function of the data (decision 7) and writes whole statements
  (`> block setFrac = CDL.Reals.MultiplyByParameter(k = 0.7)`, `~ param phiHigh : Real = 0.7`),
  so the data holds what those lines need. The examples' JSON gained `params` and `type`.
- **`reordered` names its list** in the graph section, where the subject of a block's assignment
  order is the block and `blocks` is a legal block name.
- **`derived_from` is `null` for none**, not the word `none`: a wire document can carry the id
  `none` unchecked, and the diff never validates (decision 10).
- **A comment is carried as the model holds it**: the text after `#`, its leading space
  included, lines joined by a line feed. Example (a)'s `old` and `new` gained the space.
- **A section flag of the summary ignores `reordered`; `equal` does not.** Order is not semantic
  (decision 5), and decision 16 has a `reordered`-only diff exit 1 while scripts that want "same
  structure" read `summary.graph_equal`. So a section flag is "no entry other than `reordered`",
  `equal` is "every section is empty", and `equal` is not the four flags together. It is also
  what keeps the levels in step: the CXF adapter reports graph-equal from equal canonical ids
  (decision 8), which ignore authored order.
- **One value in two spellings needs a Real to be expected.** An Integer literal against the
  equal Real is `literal-respelled` on a `Real` sequence parameter, on a block parameter the
  catalog types Real and on a bound of a boundary both sides type `Real`, exactly up to 2^53, and
  a change of value anywhere else. So the diff reads a parameter's type from the catalog as well
  as its literal default. The engine is to normalise that spelling at load (owner ruling on
  #683), which makes the loaded graph agree with this level.
- **A value and its unit suffix that both change are both reported**: the value in its section,
  the two spellings as `literal-respelled`. A scaled suffix kept across a change (`1 h` to `2 h`)
  is not a respelling.
- **The rename pass's open cases**: a port linked on one side only, a different number of
  endpoints, and an endpoint that is neither an anchor nor an unmatched block all disagree; a cap
  is passed only when both sides have unmatched blocks. A pair accepted on its agreeing ports
  never comes to disagree later, so `high` means exactly "some port is still unknown".
- **The fast path needs no switch.** With equal canonical ids every block matches by name and
  class, so nothing is left for the rename pass, and `diff` takes that path whenever a side has
  nothing left, whoever says so. `DiffOptions::canonical_ids_equal` is kept as the caller's
  statement and is a hint only: it never changes the result, so a wrong one costs nothing.
- **The names-disjoint line** of the text reads "no block kept its name and class" and counts the
  removed and the added blocks: a name kept under another class is not a kept block.
- **An array element is never `parameter-defaulted`**: an element is part of the array's shape.
- **What an invalid model means** is fixed case by case in the
  [crate README](../../crates/leverage/oce-diff/README.md#what-an-invalid-model-means): a
  repeated name is matched occurrence by occurrence, a target with several drivers as a
  multiset, and a link end is compared as what it is (a boundary, or a block and a port), never
  as its spelling.
- **Reading a diff back is strict**: closed objects, every key required (`null` for a value that
  may be absent), `schema` exactly `oce-diff/1`, a Real's spelling the spelling of its bits, and
  the summary's flags what the sections give.
- **A round of the rename pass groups blocks instead of visiting pairs.** Whether a pair
  disagrees depends only on what each block sees, so the blocks with equal views are the
  candidates of one another and the work is the links at the unmatched blocks. The pair cap is
  still counted and enforced.

What starting the CXF adapter (issue #666) showed about the facade, ruled by the owner on
2026-11-21 (issue #701, GitHub #702):

- **`Topology` carries the boundary and the root.** The view held no type, `quantity`, `unit`,
  `displayUnit` or bound of a boundary, so two documents with different content ids could have
  equal topologies and the adapter could not have reported `interface_equal`; and it held no
  root id, which the block paths of a one-block document do not give. `oce_api::Topology` now
  has exactly the boundary facts the content id encodes: `boundary_inputs` (path, type, the
  three labels, the effective bounds), the type and labels on each `DeclaredOutput`, and
  `model_id`, the root. `diff_topology` keeps its two-topology signature.
- **Two consequences for #666.** No public load path produces an unnamed block, so that case
  of the adapter is tested below the public entry. And the claim across levels runs one way:
  equal content ids never coexist with a non-empty graph or interface section; an
  interface-only change moves the id while the graph section stays empty.

What building the CXF adapter (issue #666, GitHub #715) showed. Each is a consequence of
building decisions 3, 8 and 15 against the facade as it stands, not a new decision; the
[crate README](../../crates/leverage/oce-diff/README.md#the-cxf-adapter) states each in full.

- **The root is read, never guessed, and a path outside it is refused.** A name is a path with
  the root id, a `.` and a non-empty name holding no `#`, `/` or `:`; the 47 documents of the
  content-id pin all follow the convention, the twelve with one block included. A path that does
  not is the typed refusal `outside-root`, which is how "blocks under two root prefixes" is
  refused. The delimiters are refused because the content id names an IRI by what follows its
  last one: a name holding one would let two documents with equal ids differ here (`s.x/g`
  against `s.y/g`) or two with unequal ids diff empty. So is every block of a document the
  engine exported, whose root is the exporter's fixed `@id`: it has the original's content id
  and cannot be diffed at this level until GitHub #717.
- **Ports are named by index**, from `oce_api::class_ports` over the class and the ground
  parameters, never from a path; a block whose ports cannot be named so is the refusal
  `unresolved-ports`, which no loaded document reaches. Where a leaf's ports are unnamed (vector
  ports) they bind by position under the ordering contract, so reversing that block's `hasInput`
  or `hasOutput` rewires it and the adapter reports the rewiring; named ports bind in
  class-signature order whatever the list order. Reversing every port list of the 47 corpus
  documents keeps the id and an empty diff for 44 and moves both for 3, never one without the
  other.
- **The unnamed block.** A path of `b` and decimal digits is never matched by name, only by the
  rename pass, and adds the note `unnamed-blocks`; it is reached by editing a topology.
- **Ground values follow the authoring level's rules.** A value on one side only is
  `parameter-defaulted` against the catalog's literal default, a block's parameters are listed by
  name, and an Integer against the equal Real is a change of value until #683 makes the engine
  hold the Real.
- **An input's bounds are its effective domain** (an Integer input left open is the `i32` range);
  an output carries none, and `derived_from` is `null`: a topology holds neither. Nor does it
  hold a `unit`, `quantity` or `displayUnit` on a boundary that is not a Real, the spelling of an
  Integer-written bound on a Real boundary, or a constant-package member as a member.
- **How this level stands to decision 8.** The decision's text is about equal canonical ids and
  the loaded graphs they prove equal. This level reads only facts the id encodes, plus the
  root, and the id never encodes the root id while this level names every element under it. So the guarantee here
  is: equal canonical content ids **and equal sequence names** never coexist with a graph or
  interface entry; under the name rule the id names each element `{sequence}.{name}`, and equal
  sequence names make the two namings one. Equal ids alone are not enough: a document whose root
  `@id` alone moved to a shorter dot-prefix of its instance IRIs keeps its id and is another
  sequence here, every name changed. The claim of the bullet on #701 above holds in this form.
- **The fast path is a function that changes nothing.** `diff_topology_with_ids` records whether
  the two ids are equal as `canonical_ids_equal`, a hint that never changes a result; it returns
  exactly what `diff_topology` returns for any ids, and a test passes it a false one.
- **The claim across levels holds as an equality.** For two authoring models that emit and load,
  the CXF diff equals the authoring diff of the two models as loaded (references grounded,
  widened literals and constant-package members, unread inputs, undriven outputs, output bounds
  and non-Real labels dropped) on the graph and interface sections, their flags, the counts, the
  notes, the names, the `sequence-renamed` entry and `equal`.

Ruled by the owner on 2026-11-21, on the review of GitHub #715:

- **A renamed root is metadata at the CXF level.** When the two sequence names differ, the CXF
  diff carries a `sequence-renamed` metadata entry, so it is not `equal` and `oce diff` exits 1,
  as for an authoring rename. `metadata_equal` is `false` beside that entry and `null` (unknown)
  otherwise.
- **A path outside its root stays refused** (`outside-root`). Diffing by whole path is rejected:
  an export against its source would read as a total rewrite. Making an engine-exported document
  diffable is GitHub #717, with two options: fall back to the content id's own names on both
  sides when either side is outside its root, or have the exporter keep the source root `@id`.

What building the blob comparison (issue #667, GitHub #PR) showed, under decisions 3 and 15; the
[crate README](../../crates/leverage/oce-diff/README.md#the-blob-comparison) states each in full:

- **Each side carries this build's verdict.** A blob this build refuses to run can still have a
  summary (issue #680), so `BlobComparison` gained `base` and `head`: the recorded content id and
  the decoder's refusal (`decode_refusal`, the facade's typed error, kept in memory and written
  as `accepted`), and "same" on every axis is never read as "runs here". A blob with no summary
  at all is the typed `DiffError` `unreadable-blob`, carrying the side and the facade's refusal.
  The comparison is written, not read back.
- **`signature-changed` needs a second registry.** One engine version gives a class one hash, and
  `inspect_blob` reads only this version's blobs, so the case arises only from a build of the
  same version with another registry, whose blob this build refuses; the tests make one by
  flipping a recorded hash. A class change through a document is `added` plus `removed`.
- **`initial_state` aligns by position**, block id by block id: a reordering of one graph's
  blocks reads `unknown` or `different`, never `same`. A changed initial parameter also moves the
  content id, which covers bound parameters.

## Review rulings

The architect pass on the draft (orchestrator, 2026-11-08) ruled:

- 9 accepted: a sibling leverage crate, `crates/leverage/oce-diff`, on `oce-author` and `oce-api`.
- 10 accepted: the diff never validates.
- 11 accepted: the rename pass as specified, caps and rounds included.
- 12 accepted: each connection change once, at its target, drivers compared after renames.
- 13 accepted: Reals as spelling plus bits, never JSON numbers.
- 14 accepted: `unit` under interface, other `SemanticPoint` fields under metadata.
- 15 accepted; the blob accessor is filed as GitHub #448.
- 16 accepted, with the open question ruled: unparsable or unloadable input exits 2 for every
  subcommand, a parsed "no" exits 1; the authoring note's CLI section is aligned in this PR.
- Block identity by authored name, with the canonical id as the whole-document fast path, accepted
  as written.
- The note's length is accepted for three worked examples; example (c) shows its graph entries only.
