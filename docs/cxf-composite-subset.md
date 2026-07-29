# The CXF composite-subset import contract

This is the canonical statement of which nested-composite CXF shapes the Open Control Engine
import accepts and rejects. It is written for the author of an external CXF-emitting tool who has
never read the engine source. The behavior described here is what `oce_cxf::import_cxf` — and
therefore `Engine::load_cxf` in the published `open-control-engine` facade — enforces; every rule
is pinned by tests against the checked-in conformance corpus (see
[Testing your emitter](#testing-your-emitter)).

Scope: this contract covers the composite subset of CXF lowering — how `S231:containsBlock` hierarchies
flatten, which hierarchy shapes reject, and the document-wide rejection of active array-valued
connector and block-instance nodes. Other leaf-block semantics, connector typing, and
post-lowering validation (unit checks, single-assignment) have their own diagnostics and are out
of scope here.

## How rejections are reported

Every rejection is a diagnostic with three parts:

- a **DiagCode** (a stable kebab-case code string, e.g. `malformed-document`),
- an optional **subject** (the `@id` of the offending node, where one exists),
- a **message**.

The *rejecting* rules below — except Rule 6, whose rejections are generic diagnostics with no
tag — are contract rules: their messages begin with a
stable machine-readable tag of the form `composite/<rule-id>: ` (note the single trailing space
after the colon). Match rejections with `message.starts_with("composite/<rule-id>: ")`; the rest
of the message is human prose and may change. The tag-to-code mapping is published twice — in the
[rule catalog](#rule-catalog) table below and as the machine-readable artifact
`tools/reference-catalog/oce-cxf.composite-rules.json` — and a drift-guard test holds this
document, the artifact, and the emitting code to the same catalog identities.

Rules 1 and 3 are *non-rejecting* classification and ordering rules. They carry
**no DiagCode, no message tag, and no catalog entry** — there is nothing to match, because they
never fail. They are stated here because an emitter that misunderstands them produces a model
that imports cleanly but means something else.

JSON-LD fragments below are illustrative: they elide `@context`, connector nodes, and unrelated
keys. Complete importable documents live in the conformance corpus.

## Active nodes

Source profiles may mark components and connectors conditional
(`S231:isConditionalComponent: true` plus an `S231:conditionalExpression` guard). At load time
the guard is evaluated against the owning composite's own grounded parameters and constants; a
false guard makes the node — and, recursively, its inputs, outputs, parameters, constants, and
contained blocks — **inactive**. Everything else is active.

Rules 3, 4, 5, and 7 operate on active nodes only: inactive children are not traversed (their
whole subtree drops out of the leaf order), inactive parameters are not grounded — an inactive
array-valued parameter does **not** reject — and banned Modelica keys or `S231:isReplaceable` on
an inactive node are tolerated. Root classification (rule 2) does not consult activity.
Connections are not exempt: an active connection into or out of an inactive node rejects with
the generic `inactive-conditional-node` diagnostic — prune conditional structure so inactive
nodes take their connections with them.

## Rule 1 — Composite discriminator (non-rejecting)

> A node is a runtime composite if and only if its `S231:containsBlock` list is non-empty AND its
> `@type` does not resolve to a registered leaf block class. A node whose `@type` resolves to a
> registered class is a leaf even when it carries `S231:containsBlock` — the carve-out that keeps
> protected implementation children out of composite classification. Classification never
> rejects; rule 1 has no DiagCode.

`@type` resolution: take the fragment after the last `#` (the whole string when there is no
`#`), strip a leading `Buildings.Controls.OBC.`, and look the remainder up in the native block
registry (published as `tools/reference-catalog/oce-blocks.registry-manifest.json`). So
`http://example.org#Buildings.Controls.OBC.CDL.Reals.Add` resolves to the registered class
`CDL.Reals.Add` and is a leaf; `S231:Block` or a vendor class path resolves to nothing and — with
children — is a composite.

```json
{ "@id": "…#M.sub", "@type": "http://…#Vendor.Sequences.ScaleAndForward",
  "S231:containsBlock": { "@id": "…#M.sub.gain" } }
```
is a runtime composite, while
```json
{ "@id": "…#M.con", "@type": "http://…#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
  "S231:containsBlock": { "@id": "…#M.con.protected" } }
```
stays a leaf: it imports as a normal `CDL.Reals.Sources.Constant` block and the protected child
is elided (corpus fixture `accepted/registered_leaf_carveout.jsonld`).

## Rule 2 — Single top root (rejects: `composite/root-count`)

> After classification, exactly one runtime composite must be unreferenced by any other runtime
> composite's `S231:containsBlock`. Zero candidates, or two or more, reject with
> `composite/root-count` (DiagCode `malformed-document`). With two or more candidates the message
> enumerates every candidate in `@graph` order and the first candidate is the subject. With zero
> candidates the diagnostic carries **no subject** — there is no candidate to name.

Normative consequence: a **pure** composite `containsBlock` cycle (every composite referenced,
no root at all) classifies as **zero** roots and is reported as `composite/root-count`, never as
`composite/contains-cycle` (corpus fixture `rejected/pure_cycle.jsonld`). The cycle detector of
rule 4 only runs below a valid single root.

```json
{ "@id": "…#M",  "@type": "S231:Block", "S231:containsBlock": [ … ] },
{ "@id": "…#M2", "@type": "S231:Block", "S231:containsBlock": [ … ] }
```
rejects with subject `…#M` and a message ending
`found 2 candidate roots: …#M, …#M2` (corpus fixture `rejected/multi_root.jsonld`).

## Rule 3 — Nesting traversal order (non-rejecting)

> Active composite children lower depth-first in `S231:containsBlock` **array order**; inactive
> children are skipped along with their entire subtrees. The flat leaf order — and with it every
> dense block id, connector id, and declaration order in the imported model — derives from that
> traversal. Non-rejecting; no DiagCode.

Array order is significant: reordering a `containsBlock` array reorders the imported model's
block and connector ids, which changes goldens, point ids, and any consumer keyed on dense ids.
An emitter must produce `containsBlock` arrays in a deterministic order of its own choosing and
keep that order stable across exports of the same source.

```json
"S231:containsBlock": [ { "@id": "…#M.sub" }, { "@id": "…#M.post" } ]
```
lowers `…#M.sub`'s leaves (depth-first) before `…#M.post`.

## Rule 4 — Acyclicity (rejects: `composite/contains-cycle`)

> The `containsBlock` graph reachable from the root through active children must be acyclic. A
> cycle rejects with `composite/contains-cycle` (DiagCode `malformed-document`); the message
> names **all** participants in traversal path order, ending at the re-entered id, and the
> re-entered id is the subject.

Normative consequence: **one diagnostic per re-entry**. A cycle reachable via *k* distinct paths
yields *k* truthful path-ordered diagnostics; a consumer must not assume one diagnostic per
structural cycle (corpus fixture `rejected/diamond_cycle.jsonld`: one cycle, two paths, two
diagnostics). The degenerate self-loop (`A` contains `A`) reports the two-entry list
`…#A -> …#A` (corpus fixture `rejected/self_loop.jsonld`).

```json
{ "@id": "…#R", "@type": "S231:Block", "S231:containsBlock": { "@id": "…#A" } },
{ "@id": "…#A", "@type": "S231:Block", "S231:containsBlock": { "@id": "…#B" } },
{ "@id": "…#B", "@type": "S231:Block", "S231:containsBlock": { "@id": "…#C" } },
{ "@id": "…#C", "@type": "S231:Block", "S231:containsBlock": { "@id": "…#A" } }
```
rejects with subject `…#A` and message tail `…#A -> …#B -> …#C -> …#A` (corpus fixture
`rejected/reachable_cycle.jsonld`).

## Rule 5 — Parameter-scope inheritance (rejects: `composite/array-parameter`)

> A composite's active `S231:hasParameter` bindings ground in declaration order, then its
> active `S231:hasConstant` bindings in declaration order, into a scope inherited by every child
> composite and leaf. A binding's value may reference any binding grounded **earlier** in the
> scope chain — parent-composite bindings and earlier siblings; forward references fail
> grounding (a generic `grounding-failed`, see
> [Generic diagnostics](#generic-diagnostics)). References resolve **nearest-wins**: the scope
> is searched from the most recently grounded binding backwards, so a same-named binding
> grounded later — a child composite's, or a later sibling's — shadows an earlier one, which
> becomes unreachable for every binding grounded after it. An array-valued
> (`S231:isArray: true`) active parameter or constant on a composite rejects with
> `composite/array-parameter` (DiagCode `non-subset-construct`); the subject is the parameter
> node.

References use the local name — the segment after the last `.` of the binding's `@id` — so two
bindings anywhere in a nesting chain with the same local name collide silently under
nearest-wins. Give bindings distinct local names unless shadowing is intended; the corpus does.

```json
{ "@id": "…#M", "@type": "S231:Block",
  "S231:hasParameter": [ { "@id": "…#M.kBase" }, { "@id": "…#M.kTop" } ], … },
{ "@id": "…#M.kBase",      "S231:value": { "@value": "0.25", "@type": "…#double" } },
{ "@id": "…#M.kTop",       "S231:value": "kBase + 0.25" },
{ "@id": "…#M.sub.kInner", "S231:value": "kTop" }
```
grounds the sibling reference `kTop` to `0.5`; the child composite's constant `kInner`
(declared under `…#M.sub` via `S231:hasConstant`) inherits it through the parent scope, and the
leaf parameter `"S231:value": "kInner"` grounds the chain's end — the
`kBase → kTop → kInner → gain.k` chain of corpus fixture `accepted/minimal_nested.jsonld`.

## Rule 6 — Boundary elision (rejects: generic diagnostics)

> Composite boundary connectors are lowered away. A boundary **input** rewires to the child
> connectors it drives; the **top** composite's boundary inputs surface as the imported model's
> external inputs. A boundary **output** of a non-top composite is followed through to its final
> targets. A boundary output of the **top** composite is elided outright: its `@id` appears on
> no connector in the flat model, and a leaf output whose only target is a top boundary output
> ends with no connection at all — the driving leaf connector remains, carrying no source
> `@id`. The composite node itself never becomes a runtime block. Boundary elision rejects
> invalid direction (`DirectionMismatch`), mismatched value types (`TypeMismatch`), unresolved
> endpoints or missing boundary nodes (`UnresolvedReference`), and boundary datatype declarations
> that cannot be derived (`MalformedDocument`).

CXF §8.2 permits either endpoint of a connection to carry `S231:isConnectedTo`; subject position
does not encode signal direction. Before boundary elision, the importer therefore derives each
endpoint's source/sink role from its owning block, port direction, and position inside or outside
the nested composite, then re-anchors reverse-spelled edges on their canonical driver. This is an
orientation rule over the existing connector and containment data, not a new runtime model.
Authoring the same relation from both endpoints collapses when either spelling required
re-anchoring. In particular, both directions between one composite's input and output denote one
pass-through relation, not a boundary cycle.

What an emitter must NOT expect to survive import: composite nodes as blocks, boundary connector
hops, nesting depth, or the authored bytes. The import-parity boundary is flat by contract:
re-importing an exported document reproduces the flat `ModelGraph` — never the original
nested/authored bytes. Round-tripping a nested document through the engine and comparing bytes
will always "fail"; compare imported models instead.

```json
{ "@id": "…#M.u",     "@type": "S231:RealInput",
  "S231:isConnectedTo": { "@id": "…#M.sub.u" } },
{ "@id": "…#M.sub.u", "@type": "S231:RealInput",
  "S231:isConnectedTo": { "@id": "…#M.sub.gain.u" } }
```
imports as one external input feeding `…#M.sub.gain.u` directly; `…#M.sub.u` is gone.

## Rule 7 — Rejected constructs (rejects: `composite/banned-modelica-key`, `composite/replaceable`, `composite/array-connector`, `composite/array-instance`)

> Six Modelica construct keys are banned on any active node: `redeclare`, `constrainedby`,
> `extends`, `extendsFrom`, `moSource`, `modelicaSource`. Matching is on the term after the last
> `:`, `#`, or `/` in the key, so the bare (`extends`), prefixed (`S231:extends`), and
> absolute-IRI (`http://data.ashrae.org/S231P#extends`) spellings all reject. A banned key
> rejects with `composite/banned-modelica-key` (DiagCode `non-subset-construct`); the subject is
> the owning node and the message names the key exactly as authored.
>
> `S231:isReplaceable: true` on any active node rejects with `composite/replaceable` (DiagCode
> `unresolved-polymorphism`). The subject is the replaceable node. Replaceable components must be
> resolved to concrete classes before export.
>
> An active connector — any node referenced by an active node's `S231:hasInput` or
> `S231:hasOutput` list, anywhere in the document, whether or not the referencing node is
> reachable from the top-level root — rejects when it carries an array marker:
> `S231:isArray: true` or any `S231:sizeOfDimensions`. Marker keys match on the term after the
> last `:`, `#`, or `/`, like the banned-key matching above, so absolute-IRI spellings reject
> too. The rejection is `composite/array-connector` (DiagCode `non-subset-construct`) with the
> connector node as subject. Flatten connector arrays to one connector per element.
>
> An active block instance — any node referenced by an active node's `S231:containsBlock` —
> rejects under the same array markers as `composite/array-instance` (DiagCode
> `non-subset-construct`) with the instance node as subject. Flatten block arrays to one
> instance per element. A node referenced as both connector and instance receives both
> rejections, and a `S231:hasParameter` listing does not exempt it. Array-valued parameters
> **on a composite** are governed by Rule 5; an array-valued parameter on a leaf block is
> preserved and expanded, not rejected. Inactive conditional subtrees are invisible to these
> checks.

```json
{ "@id": "…#M.c2", "@type": "…MultiplyByParameter",
  "S231:isReplaceable": true,
  "redeclare": "…#SomeBase", … }
```
rejects twice: once under `composite/banned-modelica-key` naming `` `redeclare` ``, once under
`composite/replaceable` (corpus fixtures `rejected/banned_key_*.jsonld`,
`rejected/replaceable.jsonld`).

## Rule catalog

The contract identities, mirroring
`tools/reference-catalog/oce-cxf.composite-rules.json` (catalog order). Rules 1 and 3 do not
appear here because they are non-rejecting. Rule 6 has no `composite/` rule identity; its
rejections are generic diagnostics.

| Rule | Rule id | DiagCode | Message prefix |
| --- | --- | --- | --- |
| 2 | `root-count` | `malformed-document` | `composite/root-count: ` |
| 4 | `contains-cycle` | `malformed-document` | `composite/contains-cycle: ` |
| 7 | `replaceable` | `unresolved-polymorphism` | `composite/replaceable: ` |
| 7 | `banned-modelica-key` | `non-subset-construct` | `composite/banned-modelica-key: ` |
| 5 | `array-parameter` | `non-subset-construct` | `composite/array-parameter: ` |
| 7 | `array-connector` | `non-subset-construct` | `composite/array-connector: ` |
| 7 | `array-instance` | `non-subset-construct` | `composite/array-instance: ` |

Every message prefix is `composite/<rule-id>: ` — colon, then **one trailing space** (U+0020),
which markdown table cells cannot render unambiguously. Match with
`starts_with("composite/<rule-id>: ")`, trailing space included. The drift-guard test
(`crates/oce-cxf/tests/composite_contract_doc.rs`) checks the *Rule id* and *DiagCode* columns of
this table against the catalog artifact and derives the prefix from the rule id; the prefix
column above is display-only.

## Generic diagnostics

Two diagnostics that can accompany or replace a contract rejection are shared import machinery,
**deliberately untagged** (no `composite/` prefix, no catalog entry):

- `unresolved-reference` — a `containsBlock` child, parameter node, or composite `@id` referenced
  but not present in `@graph`.
- `grounding-failed` — a parameter value that cannot ground: a missing `S231:value` (values are
  required — Ground mode), a forward or unknown identifier reference, or an expression error.

They are not contract rules because they do not describe a composite *shape*; they fire anywhere
in the import pipeline. Match them by DiagCode, not by message. The conditional-pruning
rejection `inactive-conditional-node` (see [Active nodes](#active-nodes)) is generic machinery
in the same sense — untagged, no catalog entry.

## Acceptance preconditions

A document that satisfies rules 1–7 must also meet the general import preconditions before it
loads warning-free:

1. Exactly one top composite, and its `@type` must be **unregistered** (`S231:Block` works). A
   registered leaf standing alone — even one with `containsBlock` — classifies as zero composites
   and rejects under rule 2 with zero candidates.
2. Every parameter and constant carries a `S231:value` (missing values are `grounding-failed`).
3. Every leaf `@type` resolves to a registered block class (else `class-not-found`).
4. Accepted means **warning-free**: the corpus drivers assert an empty diagnostic report, not
   merely a non-error one.

## Testing your emitter

The engine tests itself against a checked-in conformance corpus; point your emitter's output at
the same files and drivers.

- Corpus: `crates/oce-cxf/tests/fixtures/composite_contract/{accepted,rejected}/*.jsonld`, one
  fixture per contract behavior, indexed in the corpus
  [`README.md`](../crates/oce-cxf/tests/fixtures/composite_contract/README.md).
- Accepted-fixture goldens (byte-exact `ModelGraph` renders):
  `crates/oce-cxf/tests/fixtures/golden/composite_contract_*.modelgraph.txt`.
- Resolver-layer drivers:
  `cargo nextest run -p oce-cxf --test composite_contract_corpus`
- Full-pipeline (`Engine::load_cxf`) drivers:
  `cargo nextest run -p oce-api --test conformance composite_contract`
- Doc/catalog drift guard:
  `cargo nextest run -p oce-cxf --test composite_contract_doc`

To check a document your tool produced:

- **Rejected**: drop it under `rejected/`, add a README index row, and add its expected
  `(DiagCode, subject, message)` triples to the pin tables in both drivers
  (`expected_rejections()` in `composite_contract_corpus.rs`, `COMPOSITE_REJECTIONS` in
  `crates/oce-api/tests/conformance.rs`).
- **Accepted**: drop it under `accepted/`, add a README index row, add the pair to the
  `ACCEPTED` table in `composite_contract_corpus.rs` — the golden filename convention is
  `tests/fixtures/golden/composite_contract_<fixture-stem>.modelgraph.txt` — then bless the
  golden with
  `OCE_BLESS=1 cargo test -p oce-cxf --test composite_contract_corpus accepted_fixtures_match_their_blessed_modelgraph_goldens_byte_exactly`
  and review the blessed bytes before committing. The oce-api driver picks the new file up
  automatically and requires the warning-free load.

The corpus completeness tests fail on any unindexed or unpinned fixture, so a fixture cannot
land half-wired.
