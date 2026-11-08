# Versioned facade contracts

The additive `oce-api` contracts describe metadata and evidence independently of a loaded engine.
Existing load/export reports, diagnostic aliases, value/IO types and conditional storage aliases
retain their signatures and behavior. There is no enumerated surface baseline before 1.0;
the [product contract](product-contract.md) and pre-1.0 change control still apply.

## Identity glossary

- **`DomainKey`** is a database-free semantic key DTO. In `LoadReport.model_id` it is a
  facade DTO carrying the loaded model's authored top-composite identity when
  available, otherwise the documented deterministic projection key. **Model id** is the prose role;
  there is no separate Rust `ModelId` type.
- **`ConnectorId`** is a dense, zero-based connector/index identity within one loaded flattened
  model. It is not durable and has no cross-reload, cross-model, or host-control identity guarantee.
- **`PointHandle`** is an adapter-defined scalar token containing no backend type. Its public tuple
  field is intentional: external adapters mint it in `PointStore::resolve_points` and consume it in
  `PointSnapshot::read_resolved`. A handle is valid only for the same adapter mapping from resolution
  through compatible snapshot reads. It is not durable, global, cross-adapter, cross-reload, or a
  host/equipment control identity.
- **Catalog content tag** identifies all canonical facade metadata bytes within catalog schema
  revision 1. It is distinct from the existing registry fingerprint and state compatibility key.
- **Contract descriptor revision** versions one facade domain's shapes/semantics. HostTick's
  descriptor remains descriptive; it is not a new state-wire or execution-profile selector.
- **Host build identity** remains consumer-owned and includes the host's source/build/features
  qualifications; catalog metadata alone cannot establish it. The compatibility descriptor's OCE
  package version is only a public build fact, not a unique build identity.
- **Complete export content identity** is captured as `CompleteExportContentId`, distinct from
  `CatalogContentId` and authored `DomainKey`. Existing string APIs retain their bytes and algorithms.
  It identifies the emitted document, which follows authored `@graph` and `containsBlock` order, so
  sources differing only in [that order](cxf-composite-subset.md#ordering-and-identity-contract)
  can receive different ids. No order-independent model identity is provided.
- **Compatibility descriptor** versions the closed public-fact receipt itself. Equality does not
  establish executable identity, model-specific IO equality, generation, state compatibility or trust.

Frame preparation and latest-state inspection resolve model-local connector identities in the IO
inventory, not Store handles. Load still validates adapter handle cardinality, but retains no runtime
Store-input handles. Preparation admits only complete executable boundary inputs with exact types
and declared domains. The private incarnation fence
is distinct from authored model identity and is neither portable nor serialized into state bytes.

## Catalog ownership and identity

`oce_api::catalog()` returns facade-owned `CatalogEntry` values with static metadata strings and
owned vectors. A caller can clone them without retaining an engine or depending directly on
`oce-blocks`. The [sole-facade consumer fixture](../crates/oce-api/tests/fixtures/catalog_consumer.rs)
adapts every rule payload and all Studio-shaped catalog fields under the default build; `oce-api`
declares no cargo features. The existing companion catalog remains supported during migration.

Entries retain registry order; ports, rules, defaults and enum members retain declaration order.
The projection includes canonical class paths, ordered port kinds/names, named/positional/width-driven
regimes, every rule and default payload, structural-width flags, conservative statefulness and
reserved lowering identities. An exhaustive dispatch inside the registry owner requires a complete
adapter for each rule variant. The prior owner manifest and its exhaustive serializer remain intact.

Default-parameter ports are a metadata view; resolved instance arity can differ. Conservative
statefulness is a class hint: for example, hysteresis-free comparators resolve algebraically.
Reserved entries describe engine lowering identities and cannot be authored in CXF. Units,
quantities, palette display policy and host ontology are not supplied by the catalog.

`catalog_to_json` serializes schema revision 1 as compact UTF-8 JSON with lexical object keys and
one trailing LF. Arrays preserve their input order. All fields are present, with absent port names
and optional rule names encoded as null. Integer payloads are JSON integers. Real bounds and
literals are sixteen lowercase hexadecimal digits of the exact binary64 bits; this preserves
signed zero, infinity and NaN payloads without introducing a general value codec. The function
serializes caller-supplied DTOs without validating them.

`CATALOG_JSON` contains the [packaged artifact](../crates/oce-api/contracts/catalog.json).
`CATALOG_SCHEMA_REVISION` and the [catalog schema](../crates/oce-api/contracts/catalog.schema.json)
identify its shape. `catalog_content_id` hashes ASCII `oce:catalog:1`, one NUL byte, and every
canonical JSON byte using FNV-1a-128, returning `catalog:1:fnv1a128:` plus 32 lowercase hex digits.
All fields, rule/default payloads, flags and array order affect the tag. It is a non-security
content identifier, with no authentication or engine-build compatibility guarantee. The existing
registry fingerprint, state format and execution ABI do not change.

The [catalog example](../crates/oce-api/examples/catalog_contract.rs) writes the live canonical
catalog, or writes every packaged descriptor when passed `schemas`. Schema artifacts are authored
contract descriptions; repeated export checks their exact bytes, not derivation from Rust source.
The catalog golden is a metadata regression artifact. A separate byte-arithmetic hash oracle and
[provenance note](../crates/oce-api/tests/fixtures/catalog.provenance.json) record its evidence limits.

## Immutable producer evidence

`Engine::load_cxf_with_receipt` and `Engine::export_cxf_with_receipt` share the legacy operation
pipelines. Their success receipts contain the legacy report and independently captured
`DiagnosticReceipt`. `into_parts` separates them, so caller edits to report warnings cannot change
the receipt's provenance. Legacy entrypoints avoid this additional evidence allocation.

Failures return `OperationFailure`: terminal stage, completed-stage and terminal diagnostics, and
the original `OcError`/standard error source chain. JSON and store errors can carry no
diagnostics; terminal stage and error context still explain failure. No engine code is invented.
An algebraic-loop refusal records one `Error` per member of one concrete cycle at stage `Schedule`:
`algebraic-loop` for a connector-level feedthrough cycle (subjects are member connector identities)
and `block-algebraic-loop` for a block-granularity cycle that atomic firing cannot order (subjects
are member block identities). A `block-algebraic-loop` refusal does not mean the model breaks CDL
§7.16. No connector-level cycle exists, so the model may be CDL-legal, and the refusal is a
fail-closed engine coverage limit. The message says so and suggests splitting the block. The block
shapes that can trigger it and the remedies are listed under
[Atomic block firing refuses some CDL-legal feedback](cdl-coverage.md#atomic-block-firing-refuses-some-cdl-legal-feedback),
and scheduling such models is tracked in GitHub issue #347. CXF loads use authored IRIs. Exactly
one cycle is reported even when several exist: a depth-first search over the connectors (or blocks) left unscheduled, visiting roots
and successors in declaration order, reports the first cycle it closes. Like every receipt, the
records are in **subject order, not loop order**. Each message is bounded independently of the loop
length and has the form `<kind> member <k> of <n>; next member: <identity>. <cause and remedy>`,
where `<k>` is the member's 1-based loop position; ordering records by `<k>`, or following each
subject to its next member, reconstructs the loop. The message is still display text, not a
machine field: the `DiagnosticKey` has no position field, so this is the only carrier. The
`OperationFailure::error` chain is unchanged (`OcError::Build`, or `OcError::LoadContext` after a
prior warning) and its display still lists every member in loop order; the legacy `load_cxf`
error carries no such records.
An unloaded export retains its existing `export-unsupported` diagnostic. A partial export still
succeeds with warnings; use the legacy report's completeness check for emitted-document identity.

Producer stages are explicit boundary labels, not inferred from codes. Revision-2 ranks are:

| Rank | Stage |
| --- | --- |
| 0 | Import: serialized admission, parse/resolution, including the CXF resolver's internal passes |
| 1 | AttributeUnification |
| 2 | Validation |
| 3 | Instantiation |
| 4 | Schedule |
| 5 | Semantics |
| 6 | Projection |
| 7 | StoreRecovery |
| 8 | StoreSave |
| 9 | StoreInputs |
| 10 | Export |

Revision 2 removed the revision-1 `Flatten` stage (rank 1), an identity pass that never produced
evidence; the relative order of every remaining stage is unchanged.

The ordering is stage rank, subject category/presence and exact UTF-8 subject text, code string,
then severity rank (`Error=0`, `Warning=1`, `Info=2`). `Absent` sorts before `Opaque`; an empty
present string differs from absence. Subjects can identify authored or synthetic nodes, classes
or positional content. Current producers do not supply reliable subject provenance, so the facade
preserves opaque text without URI/Unicode normalization or guessing. Hosts own authored-target mapping.

`DiagnosticKey` equality/order excludes display messages. Equal machine records retain multiplicity
and producer-relative tie order, without a prose tie-breaker or uniqueness claim. Code strings are
extensible. There is no truncation, deduplication or stable human-message promise. The existing
`all_diagnostics` and legacy warning order remain unchanged. The
[receipt schema](../crates/oce-api/contracts/diagnostics.schema.json) describes this separate revision.

## Other contract descriptors

`contract_descriptors()` returns all seven domain/revision/artifact descriptions. Catalog uses
JSON Schema 2020-12. Other artifacts describe actual Rust fields and semantic limits in JSON;
they do not promise new JSON wire codecs or schema-driven runtime validation.

| Domain | Actual contract and limits |
| --- | --- |
| [Values](../crates/oce-api/contracts/values.schema.json) | Existing `Value`, `ValueType` and `ConnectorId` aliases remain. Real values use bit-preserving binary64; enums retain class and ordinal. Constructor representability does not establish operation-specific validity. Connector IDs are model-local indices. |
| [IO](../crates/oce-api/contracts/io.schema.json) | Existing point fields and declared static attributes; enums project to `Int`, strings are omitted. Current inventory classification/defaults are explicit. |
| [Parameters](../crates/oce-api/contracts/parameters.schema.json) | Existing tune-at-rest rows and available static bounds. Absent bounds do not establish freedom from cross-parameter rules. Unit/quantity provenance is currently absent. |
| [Assertions](../crates/oce-api/contracts/assertions.schema.json) | Revision 2: `CompletedFrame::diagnostics` retains all block warnings, including Assert and other classes. Sources are currently class-level, not guaranteed instances. Repeated false Assert inputs warn each evaluation; true is silent. |
| [Execution profile](../crates/oce-api/contracts/execution-profile.schema.json) | Descriptor revision 2, still fixed HostTick v1: one advance per accepted complete frame, including equal timestamps; no Modelica same-time event iteration. Descriptive, not a runtime selector or snapshot revision. |

Every completed frame retains warnings. There is no public no-op-sink execution profile or engine
write-back route. Load/Store side effects and commit ordering are unchanged. These descriptors add no rollback,
warn-once, escalation, scheduler, equipment policy or safety guarantee. The separately documented
[serialized admission and replacement policy](facade-migration.md#bounded-serialized-load-adoption)
uses `Import` for byte refusals without adding/reordering stages or changing descriptor bytes.

## Consumer migration boundary

Consumers of removed execution profiles need source migration. New adapters can depend on `oce-api`
alone for the typed catalog and receipts. Studio retains its full source/build/features identity,
catalog policy, diagnostic truncation and authored-target mapping. These are separate from the
facade catalog content tag. No future OCE commit is embedded in the artifact, and this change
advances no downstream source or pin. Coordinated companion removal remains later work.

The [migration record](facade-migration.md#additive-contract-adoption) distinguishes this additive
contract from the earlier facade contraction. Full local tests, exact public baselines and external
compiler fixtures establish bounded implementation evidence; hosted cross-architecture checks and
actual downstream qualification remain separate evidence.

## Closed host compatibility descriptor

`CompatibilityDescriptor::current(None)` captures public facade facts without an engine. Passing
`Some(&export_report)` additionally captures the existing complete exported-document tag, calling
`content_id_complete()` first. Any warnings return its unchanged `ContentIdError::Incomplete`;
partial export never silently becomes absent or complete content. No export is performed implicitly.
This additive artifact does not add an eighth `ContractDomain` or change the seven existing artifacts.

`CatalogContentId` and `CompleteExportContentId` are distinct read-only types. Hosts can borrow their
unchanged tag strings with `as_str()` or print them. Neither accepts an authored `DomainKey`, an
arbitrary string, or the other category. No speculative executable, generation or state-wire type
is exposed. The owned descriptor survives report mutation and engine reload/drop; capture again
after re-export to describe changed exported content. Capture neither observes nor changes run state.

### Canonical bytes and comparison

`Display` / `to_string()` is the complete revision-1 artifact, not Debug output. UTF-8 lines occur
in exactly this order, with no spaces, BOM or CR, decimal revisions without leading zeros, and one
LF after every line, including the last:

| Label | Covered value |
| --- | --- |
| `oce-compatibility` | Descriptor revision, exactly `1`. |
| `catalog-schema` | Current public catalog schema revision. |
| `catalog` | Verbatim current facade catalog content tag. |
| `io-schema` | Public IO inventory contract revision, not a loaded input-definition digest. |
| `value-schema` | Public value contract revision, not a new value codec. |
| `parameter-schema` | Public parameter metadata contract revision, not instance parameter values. |
| `execution-profile` | Exactly `HostTick-v1`, the canonical label for fixed HostTick v1. |
| `execution-profile-schema` | Public execution-profile descriptor revision (currently 2). |
| `oce-api-version` | Exact Cargo package version, including any pre-release/build suffix. |
| `export` | Verbatim complete CXF content tag, or exactly `none` if no report was supplied. |

The [absent-export golden](../crates/oce-api/tests/fixtures/compatibility.txt) and
[complete-export golden](../crates/oce-api/tests/fixtures/compatibility_export.txt) are hand-assembled
from those fields, public revisions/version and the existing independently checked tag goldens.
No source normalization is claimed. Catalog FNV-1a-128 covers ASCII `oce:catalog:1`, NUL, then all
`catalog_to_json(catalog())` bytes including LF. Export FNV-1a-128 covers exactly `ExportReport.bytes`,
without prefix or length. Both start at `0x6c62272e07bb014262b821756295c58d`; each byte is XORed,
then multiplied by `0x0000000001000000000000000000013b` modulo 2^128. Existing tag prefixes and
32-lowercase-hex formatting stay unchanged. The new artifact adds no hash of its own.

`check_compatible` requires exact field equality and returns the first `CompatibilityMismatch` in
canonical order: DescriptorRevision, CatalogSchema, CatalogContent, IoSchema, ValueSchema,
ParameterSchema, ExecutionProfile (label or descriptor revision), Build (package version),
ExportPresence, ExportContent. Either unsupported descriptor revision refuses, even if equal.
There is no SemVer range, wildcard, fallback or coercion. Two absent exports agree only on absence;
hosts needing content identity separately require presence. Persisted receipts are canonical bytes,
not parsed OCE objects: compare exactly against a newly captured descriptor, refusing unknown
revisions, changed/missing/extra fields and all byte differences. OCE supplies no receipt parser.

### Limits and security

The only build fact is the public `oce-api` package version. Default, explicit mem and no-default
selections intentionally share the same descriptor because their supported package behavior is
equivalent. Compiler, source commit, dependency lock, target, codegen and binary identity are not
captured. In particular, unpublished same-version builds can disagree while these facts agree.
Hosts retain full source/build qualification. Matching descriptors are necessary public-fact checks,
not sufficient execution equivalence, cross-platform numerical evidence, restore eligibility or
release-to-release compatibility. `io()` inventory is not `input_definitions()`; these revisions
do not identify either model's actual IO. LoadReport.model_id remains diagnostic authored/synthetic
identity, never a compatibility key.

FNV is non-cryptographic; collision resistance, authenticity and authorization are not supplied.
ExportReport is mutable: an empty warning list at capture is not authenticated producer provenance
or CXF revalidation. Hosts may hash/sign canonical bytes themselves and own freshness, trust,
generation fencing and equipment policy. OCE is not a signing/PKI authority. State/manifest bytes,
executable fingerprints, generation tokens and native frame sequence are absent and remain private
where currently private. Snapshots, restore, frames and admission limits retain their existing rules.

## Complete-frame preparation adoption

`Engine::input_definitions` and `Engine::prepare_frame` add the preparation part of the
[complete-frame contract](complete-frame-contract.md#current-preparation-api). Discover inputs from
the owned exact-type definitions, not `io().iter().filter(In)`: that point projection includes
internal driven points, omits strings and projects enums to Int. Supply every canonical definition
once with a real host observation. The definition's exact inclusive bounds are schema domains, not
input values or defaults. Missing values are refused rather than seeded or read from the Store.

Replace old per-value staging with one complete borrowed-key list passed
to `prepare_frame(time, entries)`. Preparation returns an owned opaque `PreparedInputFrame` without
staging the loop's valid prefix. The plan carries all fan-out targets and exact native values, but
no Store handles or public connector indices. There is no serializable plan or reusable schema/cache
object. Refresh discovery after successful load/reconfiguration; old plans are invalid after reload
or dirty resume, including same-byte reload and same-value edits. Clean resume and compatible restore
alone preserve the context, while an advanced clock can make the submitted time ineligible.

**Do not replace execution with preparation.** Pass the owned plan to `engine.execute_frame(plan)`
for one complete transition and an owned `CompletedFrame`. The old execution methods and supporting
types have been removed, without aliases. Hosts own quality, freshness, scheduling, persistence
and actuation; neither missing observations nor duplicate names are silently accepted.

The [public tests](../crates/oce-api/tests/prepare_frame.rs),
[stateful preservation matrix](../crates/oce-api/tests/prepare_frame_preservation.rs), and
[private plan/lifecycle census](../crates/oce-api/src/frame_tests.rs) establish the bounded current
preparation evidence. Execution, shared-core and frame-only contraction evidence make PC-033 through
PC-035 current. The assertion and execution descriptors are now revision 2; other descriptors,
catalog identity, state formats, HostTick v1, stable-release status and downstream pins are unchanged.

## Complete-frame execution adoption

Collect host observations, call `prepare_frame`, then move the plan into `execute_frame`. The latter
rechecks readiness, incarnation, model time and sequence capacity before staging anything. Any
ordinary returned error preserves the full engine image and consumes no accepted position, although
the Rust plan is moved. On success, retain or clone the returned result rather than relying on
latest-state inspection. Read `time()`, `sequence()`, `outputs()` and `diagnostics()`; there is no serialization API.

The result contains lexical root boundary identities and native typed values, not every internal
output connector or durable column. Distinct declared outputs may share a driver; pass-throughs
occur once. Warning records retain emission order and producer source semantics. No pointer, Store
handle, schedule, deployment token or durable replay identity is exposed. Sequence starts at one,
counts successful native frames only and never resets during this Engine's lifetime, even on reload
or checkpoint rewind. Equal-time success is another transition, never an idempotent retry.

Hosts decide whether to execute and what to do with completed values; Store/actuation follow outside
this API. Sibling consumers need to migrate; no sibling source or pin is changed or qualified here. See the
[execution contract and evidence](complete-frame-contract.md#current-execution-api).

The single frame path retains the private HostTick evaluation/refresh implementation. Host loops
submit complete frames and own cadence and trace capture. `get_output` and `watch` are latest-state,
non-receipt views, including internal points. There is no raw output view or Store write helper.
Parity evidence is limited to equivalent complete schedules, not arbitrary legacy workflows.
