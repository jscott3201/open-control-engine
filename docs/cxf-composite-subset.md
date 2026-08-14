# The CXF composite-subset import contract

This is the canonical statement of which nested-composite CXF shapes the Open Control Engine
import accepts and rejects. It is written for the author of an external CXF-emitting tool who has
never read the engine source. The behavior described here is what `oce_cxf::import_cxf` — and
therefore `Engine::load_cxf` in the `oce-api` facade — enforces; every rule
is pinned by tests against the checked-in conformance corpus (see
[Testing your emitter](#testing-your-emitter)).

Scope: this contract covers the composite subset of CXF lowering — how `S231:containsBlock` hierarchies
flatten, which hierarchy shapes reject, and the document-wide rejection of active array-valued
connector and block-instance nodes. Other leaf-block semantics, connector typing, and
post-lowering validation (unit checks, single-assignment) have their own diagnostics and are out
of scope here.

Resource bounds are part of this engine's accepted subset, not CDL semantics. `containsBlock`
nesting is limited to 64. Boundary lowering is iterative and permits at most 64 non-top boundary
hops per path, 65,536 target examinations, and 8 MiB of aggregate target-IRI bytes across boundary
walks in the document. Exceeding a boundary limit returns `MalformedDocument` without constructing
a partial flat graph. Ordinary direct leaf wiring does not consume the boundary-work budgets.

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

`@type` resolution operates on the `@context`-expanded form: the token is first expanded
against the document `@context` (a CURIE with a declared prefix becomes its absolute IRI;
anything else stays as written — a typing token is never refused), then take the fragment after
the last `#` (the whole string when there is no `#`), strip a leading `Buildings.Controls.OBC.`,
and look the remainder up in the native block registry (published as
`tools/reference-catalog/oce-blocks.registry-manifest.json`). So
`http://example.org#Buildings.Controls.OBC.CDL.Reals.Add` — or the compact
`ex:Buildings.Controls.OBC.CDL.Reals.Add` under `"ex": "http://example.org#"` — resolves to the
registered class `CDL.Reals.Add` and is a leaf; `S231:Block` (expanded,
`http://data.ashrae.org/S231P#Block`) or a vendor class path resolves to nothing and — with
children — is a composite. Note the contrast with rule 7: **identities and typing tokens
expand; property KEYS match by suffix** — the banned-key and array-marker matching below stays
on the term after the last `:`, `#`, or `/`, whatever the spelling.

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

The full order contract: array order is load-bearing wherever the resolver reads an array —
`@graph` node position, `containsBlock` order, each instance's port and parameter lists,
`isConnectedTo` order. Two carve-outs: the boundary-input elision vector (`external_inputs`)
and the pass-through pair list are re-keyed on the boundary port's own `@graph` node position
instead of inheriting the order of that port's `isConnectedTo` array
(`crates/oce-cxf/src/resolve/mod.rs`, Step 9); and a `S231:hasInstance` member array's order is
load-bearing for **nothing** — derived ports bind by name against the class signature,
synthesized connectors order by `(owner @graph position, class-signature position)`, and
classified parameter members append in class-signature order, so permuting the array moves no
`ConnectorId`, no `decl_order`, and no `param` row. Neither array order nor node position is a
stable identity: key by authored name, never by position.

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

## Rule 5 — Parameter-scope inheritance (rejects: `composite/array-parameter`, `composite/declaration-cycle`, `composite/duplicate-declaration`)

> A composite's active `S231:hasParameter` and `S231:hasConstant` bindings form **one mutual
> scope**: every binding's value may reference any sibling of either kind, declared earlier or
> later — declaration array order carries no meaning for the composite's own scope. A document
> that loads does so with a byte-identical imported model and an identical diagnostic vector
> under any permutation of the two arrays; a document these rules refuse refuses under every
> permutation with the same rule ids and the same participant sets — only a diagnostic's
> *subject* may relocate, because subjects follow the chained declaration order that
> permutation changes. Inside an own
> binding's value, an own local name always denotes the own sibling, shadowing a same-named
> binding of an enclosing composite; only names with **no** own binding fall through to the
> enclosing scope chain (innermost composite first). The grounded scope is inherited by every
> child composite and leaf. Identifier-shaped text inside an expression String literal is data,
> not a sibling reference: `S231:value: "\"b\""` creates no dependency on an own declaration
> named `b`. Three shapes reject:
>
> - A reference **cycle** among a composite's own bindings — including the length-1
>   self-reference `x = "x * 2"`, which is never an enclosing read — rejects with
>   `composite/declaration-cycle` (DiagCode `malformed-document`): **one diagnostic per
>   distinct cycle per chain evaluation**. Like `contains-cycle`, a composite reachable via
>   multiple `containsBlock` paths is evaluated once per path (enclosing scopes can differ per
>   path), so the same cycle can surface once per visit — consumers must not assume one
>   diagnostic per structural cycle document-wide. Subject = the participant earliest in the
>   params-then-constants chained declaration order; the message's arrow list is the
>   participant **ring** in chained declaration order closing on the first
>   (`…#M.a -> …#M.b -> …#M.a`) — not the discovered edge path. Bindings outside the cycle
>   still ground (maximal progress); cycle members keep their own binding's name — masking a
>   same-named enclosing binding — but are absent from the scope, so a reference to one fails
>   with a generic `grounding-failed`.
> - One local name declared **twice** in one composite's own chain rejects with
>   `composite/duplicate-declaration` (DiagCode `malformed-document`): one diagnostic per
>   occurrence beyond the first in chained order (three declarations of one name emit two),
>   subject = that later occurrence, message naming it and the first occurrence's `@id`. The
>   first occurrence stays a normal binding.
> - An array-valued (`S231:isArray: true`) active parameter or constant on a composite rejects
>   with `composite/array-parameter` (DiagCode `non-subset-construct`); the subject is the
>   parameter node.
>
> **Leaf members are a different level** and keep their order-sensitive contract: a leaf
> member's **value** reference resolves **enclosing-first** — when the name is bound both in
> the enclosing scope chain and by an *earlier* sibling member, the enclosing binding wins, and
> within each region the most recently grounded binding shadows earlier ones (issue #239) — so
> a leaf member's forward reference to a sibling member still fails grounding. A leaf
> **dimension** reference (`S231:sizeOfDimensions`) still resolves **nearest-wins** over the
> undivided scope, so there a sibling binding shadows a same-named enclosing one — when the
> sibling is grounded earlier; member array order still decides the dimension reading (values
> are order-invariant under member order, dimensions are not). When the two readings of one
> name disagree on an array's shape, the element-count divergence refuses with
> `grounding-failed` (both counts in the message); a value divergence with a matching count is
> silent, exactly like the scalar path.

Conditional-guard specialization evaluates guards against the same own-scope semantics through
the same mechanism, so guard decisions are equally order-independent. The specialization pass
also grounds *leaf* declaration chains that carry conditional members; on that pass, generic
grounding machinery is non-emitting, and the two tagged rules above apply to **composite**
chains only — a leaf chain's bindings are member modifications (the leaf level described
above), so a cycle or duplicate among them produces no tagged finding there: the participants
simply fail to ground in the guard scope. A composite-chain defect visible to both passes is
reported once, from the lowering view; a composite chain only the specialization pass grounds
(for example one pruned by a false guard) still surfaces through the two tagged rules; and a
guard that genuinely cannot evaluate refuses through the guard's own diagnostics — never as a
bare `grounding-failed`. A leaf with a legal array parameter plus a conditional member
therefore loads (corpus fixture `accepted/leaf_array_parameter_conditional_member.jsonld`),
and so does the leaf identity-modification idiom — a leaf parameter
`samplePeriod = "samplePeriod"` reading the same-named enclosing composite parameter, beside a
conditional member (corpus fixture `accepted/leaf_identity_parameter_modification.jsonld`, the
member value grounding enclosing-first per the leaf rules above); the specialization model
itself — what a guard means and how pruning propagates — is unchanged.

References use the local name — the segment after the last `.` of the binding's `@id` — so two
same-named bindings at *different* nesting levels shadow (own-scope-wins for the composite's
own bindings, enclosing-first for leaf member values, nearest-wins for dimensions), while two
same-named bindings in **one** composite's own chain reject under
`composite/duplicate-declaration`. Give bindings distinct local names unless shadowing is
intended; the corpus does. Element names minted by leaf array expansion (`k[2]` → `k_1`, `k_2`)
shadow like any sibling binding: a later member's value reference to `k_1` reads a same-named
enclosing binding when one exists, not the minted element, while a same-named sibling
*parameter* collides and refuses (`ArrayFlattenCollision`). Because grounded values feed block
construction, own-scope resolution can change what a document means, so a constructed document
that imported under the older order-sensitive reading can refuse under this rule (a cycle or a
duplicate) or ground differently (a forward or shadowed sibling reference). Measured against
the pre-change base (`43d8a13`, which held 147 checked-in CXF documents: 103 crate fixtures
plus 44 vendored modelica-json translations), all 103 crate documents are byte-identical in
import outcome under the rule; 12 vendored documents — every one still refusing on unrelated
grounds — shed 48 `grounding-failed` diagnostics in exactly the two ruled classes (forward
sibling references now grounding, and specialization-pass generic machinery going
non-emitting), with zero new diagnostics anywhere. The tree now holds 197 documents (153 crate
plus 44 vendored; the growth is the conformance fixtures the declaration-scope and
`hasInstance`-interface rules added). The wider reach
exists off-corpus.

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
endpoint's source/sink role from its owning block, its port direction, and the peer's location
inside or outside that owning composite, then re-anchors reverse-spelled edges on their canonical
driver. This is an orientation rule over the existing connector and containment data, not a new
runtime model. Edges whose roles cannot be derived — a dangling or non-connector peer, a port
claimed by two owners, non-tree containment — as well as same-polarity (contradictory) pairs and
reverse spellings whose canonical driver has neither a node nor a synthesized connector identity,
are left exactly as authored and reject under the existing Rule 6 diagnostics when the relation
survives lowering. If boundary elision would erase an active relation that cannot be kept or
swapped, the importer defers a direction diagnostic until the bounded boundary walk succeeds. This
applies whether the boundary is the authored source or target. An active elided boundary source
targeting an inactive node similarly retains the ordinary inactive-node refusal. A node-less output
derived from `hasInstance` can be a canonical driver; its lowered edges follow authored sources in
derived connector order. Re-anchoring never invents or silently removes a relation: an input driven
twice still rejects. Authoring the same relation from both endpoints collapses when either spelling
required re-anchoring. In particular, both directions between one composite's input and output
denote one pass-through relation, not a boundary cycle.

The boundary walk preserves canonical target order and duplicate multiplicity below its resource
limits. It checks an active-path cycle or missing boundary node before the hop limit, so those
shapes retain their `UnresolvedReference` outcome at the boundary. The target-examination budget
counts inactive, terminal, dangling, and cycle-revisit targets before classification; it prevents a
shallow branching graph from expanding without bound even when every path is short. The aggregate
byte budget bounds repeated long target IRIs before the completed target lists are cloned.
Resource-limit diagnostics omit the attempted target subject to avoid an additional untrusted IRI
copy at refusal. Deferred diagnostics, including the inactive-target variant, also omit their
subject.

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

## Rule 7 — Rejected constructs (rejects: `composite/banned-modelica-key`, `composite/replaceable`, `composite/array-connector`, `composite/array-instance`, `composite/vector-port-instance`, `composite/unsupported-instance-member`, `composite/colliding-member-identity`)

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
> reachable from the top-level root, or any member of an active **derivation-shaped** node's
> `S231:hasInstance` list (a `containsBlock` referent that is not a runtime composite,
> declares neither port list, and carries a member list) — rejects when it carries an array
> marker. The member source matches the existing sources on reachability and is narrower only
> in shape: an orphan node's list and a runtime composite's list contribute nothing, where the
> existing sources take any active node's list at all; the scan stays reference-based and
> class-independent on both. The markers:
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
>
> Three rules govern the `hasInstance` interface derivation (an instance declaring neither
> `hasInput` nor `hasOutput` and carrying a `S231:hasInstance` list derives its interface from
> the list; a node declaring either port list keeps its own interface). Each refusal skips the
> instance's derivation whole — the tagged rejection **replaces** the generic arity mismatch
> rather than doubling it:
>
> - `composite/vector-port-instance` (DiagCode `non-subset-construct`, subject the instance
>   node, one per instance): the resolved class publishes no declared port names — its port
>   count is a function of a parameter, so one member stands for N scalar connectors and this
>   subset derives scalar interfaces only. A document declaring the same class's ports
>   explicitly through `hasInput`/`hasOutput` is untouched.
> - `composite/unsupported-instance-member` (DiagCode `non-subset-construct`, subject the
>   member IRI, one per offending member): a member outside its owner's namespace (not
>   `<owner>.<oneSegment>`), a member that is itself a block instance, or a member whose local
>   name is neither a declared port nor a declared parameter of the class.
> - `composite/colliding-member-identity` (DiagCode `non-subset-construct`, subject the
>   colliding IRI, one per collision): a synthesized connector identity that is already an
>   `@graph` node or is minted twice for one owner, or a parameter name declared twice for one
>   instance — across its classified members or against its own `hasParameter`/`hasConstant`
>   list.

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
| 5 | `declaration-cycle` | `malformed-document` | `composite/declaration-cycle: ` |
| 5 | `duplicate-declaration` | `malformed-document` | `composite/duplicate-declaration: ` |
| 7 | `vector-port-instance` | `non-subset-construct` | `composite/vector-port-instance: ` |
| 7 | `unsupported-instance-member` | `non-subset-construct` | `composite/unsupported-instance-member: ` |
| 7 | `colliding-member-identity` | `non-subset-construct` | `composite/colliding-member-identity: ` |

Every message prefix is `composite/<rule-id>: ` — colon, then **one trailing space** (U+0020),
which markdown table cells cannot render unambiguously. Match with
`starts_with("composite/<rule-id>: ")`, trailing space included. The drift-guard test
(`crates/oce-cxf/tests/composite_contract_doc.rs`) checks the *Rule id* and *DiagCode* columns of
this table against the catalog artifact and derives the prefix from the rule id; the prefix
column above is display-only.

## Generic diagnostics

Three diagnostics that can accompany or replace a contract rejection are shared import
machinery, **deliberately untagged** (no `composite/` prefix, no catalog entry):

- `unresolved-reference` — a `containsBlock` child, parameter node, composite `@id`, or a
  connection/boundary reference naming a `hasInstance` member of an instance whose interface
  was not derived (an unregistered class, or a `composite/vector-port-instance` refusal),
  referenced but not resolvable. A classifiable member itself is never reported under this
  code: a node-less port member becomes a synthesized connector and a node-less parameter
  member refuses as `grounding-failed` at derivation.
- `grounding-failed` — a parameter value that cannot ground: a missing `S231:value` (values are
  required — Ground mode, on the `hasParameter`/`hasConstant` route and the `hasInstance`
  member route alike), an unknown identifier (including a reference to a cycle-refused
  sibling, or a leaf member's forward reference to a later sibling member), or an expression
  error.
- `conflicting-interface-declaration` — an instance declaring `hasInput`/`hasOutput` beside a
  `hasInstance` list whose class-declared names its own routes do not carry (a **warning**;
  compared one-directional, list minus own), or one parameter name valued differently on the
  two routes (an **error** — two values for one name state a contradiction).

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
   A parameter declared through a `S231:hasInstance` member is covered too: a valueless or
   node-less member named after a declared parameter refuses the same way.
3. Every leaf `@type` resolves to a registered block class (else `class-not-found`).
4. Accepted means **warning-free**: the corpus drivers assert an empty diagnostic report, not
   merely a non-error one.

## Testing your emitter

The engine tests itself against a checked-in conformance corpus; point your emitter's output at
the same files and drivers.

- Corpus: `crates/oce-cxf/tests/fixtures/composite_contract/{accepted,warned,rejected}/*.jsonld`,
  one fixture per contract behavior, indexed in the corpus
  [`README.md`](../crates/oce-cxf/tests/fixtures/composite_contract/README.md). The `warned/`
  category holds documents that load successfully with a pinned advisory vector — untagged
  import machinery such as `undriven-boundary-output`, not a composite-shape rule.
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
- **Warned**: drop it under `warned/`, add a README index row, add its exact complete warning
  vector to `expected_warnings()` in `composite_contract_corpus.rs`, and add its ordered
  warning triples to the `composite_warnings()` table in
  `crates/oce-api/tests/conformance.rs` — the end-to-end warned driver is table-driven and its
  on-disk listing pin is taken over that table, so a warned fixture cannot land half-wired.
- **Accepted**: drop it under `accepted/`, add a README index row, add the pair to the
  `ACCEPTED` table in `composite_contract_corpus.rs` — the golden filename convention is
  `tests/fixtures/golden/composite_contract_<fixture-stem>.modelgraph.txt` — then bless the
  golden with
  `OCE_BLESS=1 cargo test -p oce-cxf --test composite_contract_corpus accepted_fixtures_match_their_blessed_modelgraph_goldens_byte_exactly`
  and review the blessed bytes before committing. The oce-api driver picks the new file up
  automatically and requires the warning-free load.

The corpus completeness tests fail on any unindexed or unpinned fixture, so a fixture cannot
land half-wired.

The 44 vendored modelica-json translations under `third_party/modelica-buildings-cdl/cxf/` are
additionally held to a per-document characterization capture
(`crates/oce-cxf/tests/vendored_corpus_delta.rs`): per-`DiagCode` counts, severities, and
duplicate diagnostic triples, re-blessed only deliberately. The `hasInstance` interface
derivation moved that capture in both directions, every increase declared: 904 arity
mismatches and 2,579 unresolved references removed, 30 arity mismatches replaced by
`composite/vector-port-instance`, `single-assignment` arriving at 57 (31 undriven and 8
multiply-driven derived inputs, plus 18 multiply-driven declared boundary outputs assessed for
this dialect for the first time), `grounding-failed` rising 62 → 204 as member values and
member connector bounds ground for the first time on class-translation documents, and
`inactive-conditional-node` rising 11 → 44 because pruning now reaches a conditional instance's
listed members as well as its `@graph` nodes (32 members still carrying active connections, 12
connection targets).
