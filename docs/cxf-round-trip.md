# CXF round-trip: what export guarantees

For an integrator writing or consuming CXF documents against the Open Control Engine. It answers
one question: when `export` returns `Ok`, what is actually in those bytes — and what is quietly
not?

CXF is bidirectional here. `oce-cxf` imports through the §7.1 resolver
(`crates/oce-cxf/src/resolve/mod.rs:1`, reached via `oce_cxf::import_cxf` at
`crates/oce-cxf/src/lib.rs:106`) and exports through a separate, deliberately smaller path
(`oce_cxf::export` at `crates/oce-cxf/src/lib.rs:200`). Import and export do **not** cover the same
ground, and the gap between them is where the surprises live.

## The RT-2 contract

Export is specified as a fixpoint, not as source recovery
(`crates/oce-cxf/src/export.rs:5-9`). For a graph `G1` that `import_cxf` produced, re-importing the
emitted bytes yields a graph that renders **bit-identically** to `G1` — Reals compared by their
IEEE-754 bit patterns, never by an epsilon
(`crates/oce-cxf/src/lib.rs:122-135`; the fixpoint test is
`crates/oce-cxf/tests/export_roundtrip.rs`, which compares through a hand-written renderer using
`f64::to_bits`). Emission order derives from the `ModelGraph` vectors alone, so repeated exports of
the same graph are byte-identical (`crates/oce-cxf/src/export.rs:47-52`).

The carve-out belongs right here rather than in a footnote: **bit-identity holds over the survivor
cone, not necessarily over the whole input graph.** When nothing is deferred the survivor cone *is*
the whole graph. When deferral fires, it is not, and no re-import can restore what was omitted. The
next-but-one section is about exactly that.

What never round-trips at all: cosmetic source content. Labels, layout, and line numbers are not in
`ModelGraph`, so none of them come back. The original root `@id` is not recorded either — the root
composite is emitted under the fixed synthetic IRI `urn:open-control:cxf-export:root`
(`crates/oce-cxf/src/export.rs:11-14`).

## The export subset

Export accepts the **flat, ground, single-root, scalar-parameter** subset — the shape the resolver
produces (`crates/oce-cxf/src/lib.rs:114-120`). Everything outside it is a typed
`CxfError::Validation` carrying `DiagCode::ExportUnsupported` error diagnostics whose `subject` is
the offending block, connector owner, or declared boundary node. Never a panic
(`crates/oce-cxf/src/export.rs:61-64`).

Of the §7.4.1 connector attributes, five survive, each emitted as a bare JSON scalar on the minted
child port node and on each declared boundary-**output** node. `Engine::load_cxf` joins a declared
output and its source in the same §7.10 cluster: conflicting values refuse the load, while a value
declared on only one side propagates to the unset peer before export. Low-level callers that compose
`oce_cxf::import_cxf` and `export` directly must run the graph through `oce_validate` to apply that
load contract. Boundary-input nodes still carry none; that side is tracked as issue #243
(`crates/oce-cxf/src/export.rs:31-43`):

| Attribute | Emitted as | Applies to |
| --- | --- | --- |
| `unit`, `quantity`, `displayUnit` | bare string | Real connectors |
| `min`, `max` | bare number, **finite only** | Real (float) and Integer (int) connectors |

Attributes are emitted only when `Some`; an all-default connector emits zero attribute keys, which
is byte-identical to an attribute-free port node.

Two attributes are rejected rather than dropped — and the distinction between *rejected* and
*dropped* is the point. On a **surviving** block, a connector carrying `nominal` or `unbounded`
fails the export (`crates/oce-cxf/src/export.rs:131-139`), because the importer hardcodes both to
`None` and the value would vanish silently. A non-finite Real `min`/`max` bound is rejected for the
same reason: `serde_json` writes it as JSON `null`, which re-imports as `None`
(`crates/oce-cxf/src/export.rs:140-144`).

On a **deferred ordinary** block, none of that runs. The block is omitted from the document and
therefore contributes no error diagnostic of its own — not from its connector attributes, not from
its parameters, not from its boundary entries (`crates/oce-cxf/src/lib.rs:161-198`). A reserved
pass-through with hidden state is the exception: the resolver-produced lowering shape is the only
valid form in the reserved namespace, so it rejects even when an enum parameter also marks the
block deferred. Whole-graph guards behave differently:
an empty (zero-block) graph, non-dense ids, and a connection that is not output→input reject either
way, because they are attributable to no single block's presence in the document.

## The deferral trap

This is the most important thing on this page.

Ordinary enum-carrying blocks — any `ValueType::Enum` connector or `Value::Enum` parameter — are
**deferred, not rejected**. The block *and its entire transitive downstream cone* are omitted from
the emitted document so that the enum-free remainder can still export. Reserved pass-through
blocks remain strict: an enum parameter violates the resolver-produced lowering shape, so it rejects
despite being selected for deferral. Each omission is reported as a `DiagCode::ExportDeferred`
**warning**, which is non-aborting (`crates/oce-cxf/src/export_defer.rs:1-32`). The cone is a least
fixpoint: a single enum connector near the front of a chain dooms everything downstream of it.

How large does that get in practice? The G36 corpus pins two cases as tripwires
(`crates/oce-cxf/tests/export_g36_roundtrip.rs:678-698`):

| Fixture | Blocks in graph | Blocks deferred | Share |
| --- | --- | --- | --- |
| `cooling_only_controller` | 213 (`crates/oce-api/tests/g36_cooling_only_controller.rs:252`) | 83 | 39 % |
| `multizone_vav_relief_fan_group` | 226 (`crates/oce-api/tests/g36_relief_fan_group.rs:105`) | 63 | 28 % |

Rejection fires only on **total** deferral — a graph with no emitted runtime block left after
deferred and reserved lowering-only blocks are removed, which would be an unloadable root-only
shell (`crates/oce-cxf/src/export.rs:112-116`). In principle, then, all but one block can vanish
from an export that returns `Ok`.

And `export()` **discards the warnings** (`crates/oce-cxf/src/lib.rs:200-203` — it destructures them
into `_warnings`). A caller using `export()` alone cannot distinguish a complete export from one
that dropped 39 % of the graph. Both return `Ok(Vec<u8>)`.

**Use `export_with_report`** (`crates/oce-cxf/src/lib.rs:244`). It returns an `ExportReport` with
`bytes` and `warnings` (`crates/oce-cxf/src/lib.rs:205-231`); the bytes are identical to what
`export()` returns for the same graph. An **empty `warnings` list is what certifies that the round
trip covered the whole input.** Treat a non-empty list as "this document is a subset of the model I
asked you to write."

Through the facade, `Engine::export_cxf()` (`crates/oce-api/src/export.rs:98`) always goes through
`export_with_report` and keeps the warnings, so the facade route does not expose the trap. It is
`oce_cxf::export()` specifically that drops them.

## Pass-through elision is not deferral

CDL allows a boundary input wired straight to a boundary output. Import lowers each such connect to
a reserved internal identity block — `urn:oce:lowering#PassThrough.Real`, `.Integer`, or `.Boolean`
(`crates/oce-blocks/src/lowering.rs:66-78`) — and export elides those blocks back to the bare
boundary edge (`crates/oce-cxf/src/export.rs:720-778`, `:803-807`). Re-import re-synthesizes them,
so RT-2 holds by render identity.

The visible consequence: the emitted document lists **fewer `containsBlock` entries than the graph
holds blocks**, and a canonical imported pass-through produces **no warning at all**
(`crates/oce-cxf/src/lib.rs:136-141`). Reserved connectors have no emitted child-port node, so a
host-built boundary alias or connection involving a surviving reserved block is rejected rather
than silently omitted. If cascade deferral omits the reserved owner, well-directed relationships
follow the ordinary survivor-cone rule: they are omitted with `ExportDeferred` warnings. Structural
direction errors still reject before survivor filtering. An authored instance identity, parameter,
input attribute, or class/type mismatch on the reserved block rejects because elision has no wire
representation for that state. Output attributes remain representable on an emitted boundary
output. If cascade deferral omits the reserved block, any non-default connector attribute rejects
rather than disappearing with it. An empty warning list means nothing was deferred; it does not
mean the document explicitly lists every internal lowering block. If you are reconciling counts
between a `ModelGraph` and an emitted document, that is the difference to expect.

## Two ways an `Ok` export produces bytes that fail re-import

Both are documented, and both are reachable only from a hand-built `ModelGraph` — never from one the
resolver produced.

1. **Port arity contradicting the class.** Export takes no registry dependency, so it does not check
   a block's declared port count against the class its `class_path` names. A hand-built block naming
   a registered class while declaring fewer ports than that class requires exports `Ok`; the bytes
   then fail re-import with `MalformedDocument` (`crates/oce-cxf/src/lib.rs:143-149`).
2. **An unregistered class path.** Same root cause, different symptom: the bytes export fine and
   fail re-import loudly with `ClassNotFound` — never silently (`crates/oce-cxf/src/lib.rs:151-153`).

Every graph the resolver produces is correct by construction on both axes.

## `content_id_complete`: a checked integrity tag, not a digest

`ExportReport::content_id_complete()` returns `cxf:fnv1a128:<32 hex chars>` computed over
**exactly** the emitted bytes when the export is complete. If any content was deferred, it returns
the typed `ContentIdError::Incomplete { warning_count, .. }` instead of minting an identity. Its
rustdoc carries a runnable reproduction of the tag computation so a host can verify a returned tag
independently. Three properties worth internalizing:

- It is explicitly **non-cryptographic** and not a security boundary. A host needing a cryptographic
  digest must hash the same bytes itself.
- It is **not** `LoadReport::model_id`. `model_id` preserves the authored top-composite `@id`; export
  uses a synthetic root, and resumed parameter edits change exported bytes without recomputing
  `model_id`.
- When `warnings` is non-empty, the unchecked tag would name only the **partial** survivor document;
  `content_id_complete()` refuses that case and reports the exact warning count. The older
  `content_id()` method remains only as deprecated compatibility behavior and should not be used to
  mint version identities. The checked behavior is pinned by `crates/oce-api/tests/export_cxf.rs`.

## The import side

This page is about export. The normative contract for what nested-composite shapes **import**
accepts and rejects — how `S231:containsBlock` hierarchies flatten, which shapes reject, and the
machine-readable `composite/<rule-id>: ` message tags an emitter can match on — is
[`cxf-composite-subset.md`](cxf-composite-subset.md) in this directory. Read that one if you are
writing a CXF generator.

One hardening gap, stated rather than assumed: composite boundary resolution recurses per
`isConnectedTo` hop and is not yet depth-bounded. Composite *nesting* is bounded at
`MAX_COMPOSITE_NESTING_DEPTH` = 64 (`crates/oce-cxf/src/resolve/composite.rs:22`), but the boundary
walk is not. Treat untrusted CXF documents accordingly.

For which CDL classes and G36 sequences exist on the other end of that pipe, see
[`cdl-coverage.md`](cdl-coverage.md).
