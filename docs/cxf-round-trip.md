# CXF round-trip: what export guarantees

For an integrator writing or consuming CXF documents against the Open Control Engine. It answers
one question: when `export` returns `Ok`, what is actually in those bytes — and what is quietly
not?

CXF is bidirectional here. `oce-cxf` imports through the §7.1 resolver
(`crates/oce-cxf/src/resolve/mod.rs`, reached via `oce_cxf::import_cxf` in
`crates/oce-cxf/src/lib.rs`) and exports through a separate, deliberately smaller path
(`oce_cxf::export` in `crates/oce-cxf/src/lib.rs`). Import and export do **not** cover the same
ground, and the gap between them is where the surprises live.

## The RT-2 contract

Export is specified as a fixpoint, not as source recovery
(`crates/oce-cxf/src/export.rs`). For a graph `G1` that `import_cxf` produced, re-importing the
emitted bytes yields a graph that renders **bit-identically** to `G1` — Reals compared by their
IEEE-754 bit patterns, never by an epsilon
(`crates/oce-cxf/src/lib.rs`; the fixpoint test is
`crates/oce-cxf/tests/export_roundtrip.rs`, which compares through a hand-written renderer using
`f64::to_bits`). Emission order derives from the `ModelGraph` vectors alone, so repeated exports of
the same graph are byte-identical (`crates/oce-cxf/src/export.rs`).

The carve-out belongs right here rather than in a footnote: **bit-identity holds over the survivor
cone, not necessarily over the whole input graph.** When nothing is deferred the survivor cone *is*
the whole graph. When deferral fires, it is not, and no re-import can restore what was omitted. The
next-but-one section is about exactly that.

That promise is for graphs produced by `import_cxf`. A hand-built legacy graph may carry
`external_inputs` with an empty `boundary_inputs` sidecar. Export keeps accepting that shape and
emits attribute-free root input declarations; re-import then materializes empty sidecars, so the
re-imported graph is not structurally identical to the hand-built input even when no warning was
reported (`crates/oce-cxf/src/lib.rs`).

What never round-trips at all: cosmetic source content. Labels, layout, and line numbers are not in
`ModelGraph`, so none of them come back. The original root `@id` is not recorded either — the root
composite is emitted under the fixed synthetic IRI `urn:open-control:cxf-export:root`
(`crates/oce-cxf/src/export.rs`).

## The export subset

Export accepts the **flat, ground, single-root, scalar-parameter** subset — the shape the resolver
produces (`crates/oce-cxf/src/lib.rs`). Everything outside it is a typed
`CxfError::Validation` carrying `DiagCode::ExportUnsupported` error diagnostics whose `subject` is
the offending block, connector owner, or declared boundary node. Never a panic
(`crates/oce-cxf/src/export.rs`).

Of the §7.4.1 connector attributes, five survive, each emitted as a bare JSON scalar on minted child
ports and represented root boundary-input and boundary-output nodes. A root boundary input that no
connection reads is not represented, so export omits it; its load-time `unused-boundary-input`
warning is its only record. A boundary input keeps its declaration attrs separate from every child
target, including fan-out; export never infers one from the other. `Engine::load_cxf` joins a
declared output and its source in the same §7.10 cluster:
conflicting values refuse the load, while a value declared on only one side propagates to the unset
peer before export. Boundary-input declaration unification is a separate acceptance change and is
not implemented. Low-level callers that compose `oce_cxf::import_cxf` and `export` directly must run
the graph through `oce_validate` to apply the current output-side load contract
(`crates/oce-cxf/src/export.rs`):

| Attribute | Emitted as | Applies to |
| --- | --- | --- |
| `unit`, `quantity`, `displayUnit` | bare string | Real connectors |
| `min`, `max` | bare number, **finite only** | Real (float) and Integer (int) connectors |
| `nominal` | bare number, **finite only** | Real connectors |

Attributes are emitted only when `Some`; an all-default connector emits zero attribute keys, which
is byte-identical to an attribute-free port node.

Some attributes are rejected rather than dropped — and the distinction between *rejected* and
*dropped* is the point. On a **surviving** block, a connector carrying `unbounded` fails the export
(`crates/oce-cxf/src/export_attrs.rs`), because the importer hardcodes it to `None` and the value
would vanish silently. A non-finite Real `min`/`max` bound or `nominal` is rejected for the same
reason: `serde_json` writes it as JSON `null`, which re-imports as `None`
(`crates/oce-cxf/src/export_attrs.rs`).

`S231:nominal` is imported (issue #379). Before, the importer left it in the opaque pass-through
map: a document carrying it loaded with no diagnostic, the value never reached the model, and
export silently dropped it. It now grounds like a bound onto Real connectors and boundary
declarations, so the deep gate refuses a non-finite one (`non-finite-bound`) and a finite one
round-trips as a bare number. It is still excluded from §7.10 unification (R13.4) and is never read
while executing a frame; it does not bound input values. A `nominal` on an Integer, Boolean, String
or Enumeration connector is a `malformed-document` error, since only Real declares one.

On a **deferred ordinary** block, none of that runs. The block is omitted from the document and
therefore contributes no error diagnostic of its own — not from its connector attributes, not from
its parameters, not from its boundary entries (`crates/oce-cxf/src/lib.rs`). A reserved
pass-through with hidden state is the exception: the resolver-produced lowering shape is the only
valid form in the reserved namespace, so it rejects even when an enum parameter also marks the
block deferred. A boundary-input sidecar follows its target owner into that omission; invalid attrs
on a declaration whose entire target set is deferred do not abort the partial export. Whole-graph
guards behave differently:
an empty (zero-block) graph, non-dense ids, and a connection that is not output→input reject either
way, because they are attributable to no single block's presence in the document.

## The deferral trap

This is the most important thing on this page.

Ordinary enum-carrying blocks — any `ValueType::Enum` connector or `Value::Enum` parameter — are
**deferred, not rejected**. The block *and its entire transitive downstream cone* are omitted from
the emitted document so that the enum-free remainder can still export. Reserved pass-through
blocks remain strict: an enum parameter violates the resolver-produced lowering shape, so it rejects
despite being selected for deferral. Each omission is reported as a `DiagCode::ExportDeferred`
**warning**, which is non-aborting (`crates/oce-cxf/src/export_defer.rs`). The cone is a least
fixpoint: a single enum connector near the front of a chain dooms everything downstream of it.

How large does that get in practice? The G36 corpus pins two cases as tripwires
(`crates/oce-cxf/tests/export_g36_roundtrip.rs`):

| Fixture | Blocks in graph | Blocks deferred | Share |
| --- | --- | --- | --- |
| `cooling_only_controller` | 213 (`crates/oce-api/tests/g36_cooling_only_controller.rs`) | 83 | 39 % |
| `multizone_vav_relief_fan_group` | 226 (`crates/oce-api/tests/g36_relief_fan_group.rs`) | 63 | 28 % |

Rejection fires only on **total** deferral — a graph with no emitted runtime block left after
deferred and reserved lowering-only blocks are removed, which would be an unloadable root-only
shell (`crates/oce-cxf/src/export.rs`). In principle, then, all but one block can vanish
from an export that returns `Ok`.

And `export()` **discards the warnings** (`crates/oce-cxf/src/lib.rs` — it destructures them
into `_warnings`). A caller using `export()` alone cannot distinguish a complete export from one
that dropped 39 % of the graph. Both return `Ok(Vec<u8>)`.

**Use `export_with_report`** (`oce_cxf::export_with_report` in `crates/oce-cxf/src/lib.rs`). It returns an `ExportReport` with
`bytes` and `warnings` (`ExportReport` in `crates/oce-cxf/src/lib.rs`); the bytes are identical to what
`export()` returns for the same graph. An **empty `warnings` list is what certifies that the round
trip covered the whole resolver-produced input.** The legacy empty-sidecar exception above still
applies to hand-built graphs. Treat a non-empty list as "this document is a subset of the model I
asked you to write."

Through the facade, `Engine::export_cxf()` (`crates/oce-api/src/export.rs`) always goes through
`export_with_report` and keeps the warnings, so the facade route does not expose the trap. It is
`oce_cxf::export()` specifically that drops them.

## Pass-through elision is not deferral

CDL allows a boundary input wired straight to a boundary output. Import lowers each such connect to
a reserved internal identity block — `urn:oce:lowering#PassThrough.Real`, `.Integer`, or `.Boolean`
(`crates/oce-blocks/src/lowering.rs`) — and export elides those blocks back to the bare
boundary edge (in `crates/oce-cxf/src/export.rs`). Re-import re-synthesizes them,
so RT-2 holds by render identity.

The visible consequence: the emitted document lists **fewer `containsBlock` entries than the graph
holds blocks**, and a canonical imported pass-through produces **no warning at all**
(`crates/oce-cxf/src/lib.rs`). Reserved connectors have no emitted child-port node, so a
host-built boundary alias or connection involving a surviving reserved block is rejected rather
than silently omitted. If cascade deferral omits the reserved owner, well-directed relationships
follow the ordinary survivor-cone rule: they are omitted with `ExportDeferred` warnings. Structural
direction errors still reject before survivor filtering. An authored instance identity, parameter,
connector attribute, or class/type mismatch on the reserved block rejects because elision has no
wire representation for that internal state. Declaration-side attrs remain representable on the
emitted boundary input and output. If cascade deferral omits the reserved block, those declarations
leave with it rather than appearing without a target. An empty warning list means nothing was
deferred; it does not mean the document explicitly lists every internal lowering block. If you are
reconciling counts between a `ModelGraph` and an emitted document, that is the difference to expect.

## Two ways an `Ok` export produces bytes that fail re-import

Both are documented, and both are reachable only from a hand-built `ModelGraph` — never from one the
resolver produced.

1. **Port arity contradicting the class.** Export takes no registry dependency, so it does not check
   a block's declared port count against the class its `class_path` names. A hand-built block naming
   a registered class while declaring fewer ports than that class requires exports `Ok`; the bytes
   then fail re-import with `MalformedDocument` (`crates/oce-cxf/src/lib.rs`).
2. **An unregistered class path.** Same root cause, different symptom: the bytes export fine and
   fail re-import loudly with `ClassNotFound` — never silently (`crates/oce-cxf/src/lib.rs`).

Every graph the resolver produces is correct by construction on both axes.

## `content_id_complete`: a checked integrity tag, not a digest

`ExportReport::content_id_complete()` returns `cxf:fnv1a128:<32 hex chars>` computed over
**exactly** the emitted bytes when the export is complete. If any content was deferred, it returns
the typed `ContentIdError::Incomplete { warning_count, .. }` instead of minting an identity. Its
rustdoc carries a runnable reproduction of the tag computation so a host can verify a returned tag
independently. Four properties worth internalizing:

- It is explicitly **non-cryptographic** and not a security boundary. A host needing a cryptographic
  digest must hash the same bytes itself.
- It is **not** `LoadReport::model_id`. `model_id` preserves the authored top-composite `@id`; export
  uses a synthetic root, and resumed parameter edits change exported bytes without recomputing
  `model_id`.
- When `warnings` is non-empty, the unchecked tag would name only the **partial** survivor document;
  `content_id_complete()` refuses that case and reports the exact warning count. The older
  `content_id()` method remains only as deprecated compatibility behavior and should not be used to
  mint version identities. The checked behavior is pinned by `crates/oce-api/tests/export_cxf.rs`.
- It identifies the **emitted document, not an order-independent model**. `@graph` node order and
  `containsBlock` order are [order-bearing](cxf-composite-subset.md#ordering-and-identity-contract):
  they assign `ConnectorId`/`BlockId` and `decl_order`, and export follows those vectors. Two
  sources with the same nodes and edges that differ only in that order can both load cleanly and
  receive different tags; reloading identical bytes always reproduces the same tag. The engine
  provides no "same model regardless of serialization order" identity today; a host that needs one
  must define it itself and must not substitute this tag for it. Pinned by
  `content_id_identifies_authored_graph_order_not_an_order_independent_model`.

## The import side

This page is about export. The normative contract for what nested-composite shapes **import**
accepts and rejects — how `S231:containsBlock` hierarchies flatten, which shapes reject, and the
machine-readable `composite/<rule-id>: ` message tags an emitter can match on — is
[`cxf-composite-subset.md`](cxf-composite-subset.md) in this directory. Read that one if you are
writing a CXF generator.

Composite *nesting* is bounded at 64. Boundary lowering is iterative and separately rejects paths
beyond 64 non-top `isConnectedTo` hops or documents beyond 65,536 target examinations or 8 MiB of
aggregate target-IRI bytes within boundary walks. These are engine acceptance bounds, not CDL
semantics. Hosts must still bound input bytes before JSON deserialization when accepting untrusted
documents.

For which CDL classes and G36 sequences exist on the other end of that pipe, see
[`cdl-coverage.md`](cdl-coverage.md).
