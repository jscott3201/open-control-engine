# The CXF composite-subset import contract

This is the canonical statement of which nested-composite CXF shapes the Open Control Engine
import accepts and rejects. It is written for the author of an external CXF-emitting tool who has
never read the engine source. The behavior described here is what `oce_cxf::import_cxf` — and
therefore `Engine::load_cxf` in the published `open-control-engine` facade — enforces; every rule
is pinned by tests against the checked-in conformance corpus (see
[Testing your emitter](#testing-your-emitter)).

Scope: this contract covers the nested-composite lowering only — how `S231:containsBlock`
hierarchies flatten and which hierarchy shapes reject. Leaf-block semantics, connector typing,
and post-lowering validation (unit checks, single-assignment) have their own diagnostics and are
out of scope here.

## How rejections are reported

Every rejection is a diagnostic with three parts:

- a **DiagCode** (a stable kebab-case code string, e.g. `malformed-document`),
- an optional **subject** (the `@id` of the offending node, where one exists),
- a **message**.

The four *rejecting* rules below (2, 4, 5, 7) are contract rules: their messages begin with a
stable machine-readable tag of the form `composite/<rule-id>: ` (note the single trailing space
after the colon). Match rejections with `message.starts_with("composite/<rule-id>: ")`; the rest
of the message is human prose and may change. The tag-to-code mapping is published twice — in the
[rule catalog](#rule-catalog) table below and as the machine-readable artifact
`tools/reference-catalog/oce-cxf.composite-rules.json` — and a drift-guard test holds this
document, the artifact, and the emitting code to the same five identities.

Rules 1, 3, and 6 are *non-rejecting* classification, ordering, and transform rules. They carry
**no DiagCode, no message tag, and no catalog entry** — there is nothing to match, because they
never fail. They are stated here because an emitter that misunderstands them produces a model
that imports cleanly but means something else.

JSON-LD fragments below are illustrative: they elide `@context`, connector nodes, and unrelated
keys. Complete importable documents live in the conformance corpus.

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
`composite/contains-cycle`. The cycle detector of rule 4 only runs below a valid single root.

```json
{ "@id": "…#M",  "@type": "S231:Block", "S231:containsBlock": [ … ] },
{ "@id": "…#M2", "@type": "S231:Block", "S231:containsBlock": [ … ] }
```
rejects with subject `…#M` and a message ending
`found 2 candidate roots: …#M, …#M2` (corpus fixtures `rejected/multi_root.jsonld`,
`rejected/pure_cycle.jsonld`).

## Rule 3 — Nesting traversal order (non-rejecting)

> Composite children lower depth-first in `S231:containsBlock` **array order**. The flat leaf
> order — and with it every dense block id, connector id, and declaration order in the imported
> model — derives from that traversal. Non-rejecting; no DiagCode.

Array order is significant: reordering a `containsBlock` array reorders the imported model's
block and connector ids, which changes goldens, point ids, and any consumer keyed on dense ids.
An emitter must produce `containsBlock` arrays in a deterministic order of its own choosing and
keep that order stable across exports of the same source.

```json
"S231:containsBlock": [ { "@id": "…#M.sub" }, { "@id": "…#M.post" } ]
```
lowers `…#M.sub`'s leaves (depth-first) before `…#M.post`.

## Rule 4 — Acyclicity (rejects: `composite/contains-cycle`)

> The `containsBlock` graph reachable from the root must be acyclic. A cycle rejects with
> `composite/contains-cycle` (DiagCode `malformed-document`); the message names **all**
> participants in traversal path order, ending at the re-entered id, and the re-entered id is the
> subject.

Normative consequence: **one diagnostic per re-entry**. A cycle reachable via *k* distinct paths
yields *k* truthful path-ordered diagnostics; a consumer must not assume one diagnostic per
structural cycle. The degenerate self-loop (`A` contains `A`) reports the two-entry list
`…#A -> …#A`.

```json
{ "@id": "…#R", "@type": "S231:Block", "S231:containsBlock": { "@id": "…#A" } },
{ "@id": "…#A", "@type": "S231:Block", "S231:containsBlock": { "@id": "…#B" } },
{ "@id": "…#B", "@type": "S231:Block", "S231:containsBlock": { "@id": "…#A" } }
```
rejects with subject `…#A` and message tail `…#A -> …#B -> …#A` (corpus fixtures
`rejected/reachable_cycle.jsonld`, `rejected/self_loop.jsonld`).

## Rule 5 — Parameter-scope inheritance (rejects: `composite/array-parameter`)

> A composite's `S231:hasParameter` bindings ground in declaration order, then its
> `S231:hasConstant` bindings in declaration order, into a scope inherited by every child
> composite and leaf. A binding's value may reference any binding grounded **earlier** in the
> scope chain — parent-composite bindings and earlier siblings; forward references fail
> grounding (a generic `grounding-failed`, see
> [Generic diagnostics](#generic-diagnostics)). An array-valued (`S231:isArray: true`) parameter
> or constant on a composite rejects with `composite/array-parameter` (DiagCode
> `non-subset-construct`); the subject is the parameter node.

```json
{ "@id": "…#M", "@type": "S231:Block",
  "S231:hasParameter": [ { "@id": "…#M.kBase" }, { "@id": "…#M.kTop" } ], … },
{ "@id": "…#M.kBase", "S231:value": { "@value": "0.25", "@type": "…#double" } },
{ "@id": "…#M.kTop",  "S231:value": "kBase + 0.25" }
```
grounds `kTop` to `0.5`; a child leaf parameter with `"S231:value": "kTop"` inherits that value
(corpus fixture `accepted/minimal_nested.jsonld`). References use the local name — the segment
after the last `.` of the binding's `@id`.

## Rule 6 — Boundary elision (non-rejecting)

> Composite boundary connectors are lowered away. A boundary **input** rewires to the child
> connectors it drives; the **top** composite's boundary inputs surface as the imported model's
> external inputs. A boundary **output** of a non-top composite is followed through to its final
> targets. The composite node itself never becomes a runtime block. Non-rejecting; no DiagCode.

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

## Rule 7 — Rejected constructs (rejects: `composite/banned-modelica-key`, `composite/replaceable`)

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

```json
{ "@id": "…#M.c2", "@type": "…MultiplyByParameter",
  "S231:isReplaceable": true,
  "redeclare": "…#SomeBase", … }
```
rejects twice: once under `composite/banned-modelica-key` naming `` `redeclare` ``, once under
`composite/replaceable` (corpus fixtures `rejected/banned_key_*.jsonld`,
`rejected/replaceable.jsonld`).

## Rule catalog

The five contract identities, mirroring
`tools/reference-catalog/oce-cxf.composite-rules.json` (catalog order). Rules 1, 3, and 6 do not
appear here — non-rejecting rules have no identity to publish.

| Rule | Rule id | DiagCode | Message prefix |
| --- | --- | --- | --- |
| 2 | `root-count` | `malformed-document` | `composite/root-count: ` |
| 4 | `contains-cycle` | `malformed-document` | `composite/contains-cycle: ` |
| 7 | `replaceable` | `unresolved-polymorphism` | `composite/replaceable: ` |
| 7 | `banned-modelica-key` | `non-subset-construct` | `composite/banned-modelica-key: ` |
| 5 | `array-parameter` | `non-subset-construct` | `composite/array-parameter: ` |

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
in the import pipeline. Match them by DiagCode, not by message.

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

To check a document your tool produced, drop it under `accepted/` or `rejected/`, add a README
index row, extend the pin tables in the two drivers, and run the commands above — the corpus
completeness tests fail on any unindexed or unpinned fixture.
