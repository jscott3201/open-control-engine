# Versioned facade contracts

The additive `oce-api` contracts describe metadata and evidence independently of a loaded engine.
Existing load/export reports, diagnostic aliases, value/IO types and conditional storage aliases
retain their signatures and behavior. The [public surface contract](public-surface-contract.md)
and row ledger classify the exact API; pre-1.0 change control still applies.

## Catalog ownership and identity

`oce_api::catalog()` returns facade-owned `CatalogEntry` values with static metadata strings and
owned vectors. A caller can clone them without retaining an engine or depending directly on
`oce-blocks`. The [sole-facade consumer fixture](../crates/oce-api/tests/fixtures/catalog_consumer.rs)
adapts every rule payload and all Studio-shaped catalog fields under default, explicit mem and
no-default feature selections. The existing companion catalog remains supported during migration.

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
the original `OcError`/standard error source chain. JSON, build and store errors can carry no
diagnostics; terminal stage and error context still explain failure. No engine code is invented.
An unloaded export retains its existing `export-unsupported` diagnostic. A partial export still
succeeds with warnings; use the legacy report's completeness check for emitted-document identity.

Producer stages are explicit boundary labels, not inferred from codes. Revision-1 ranks are:

| Rank | Stage |
| --- | --- |
| 0 | Import: serialized admission, parse/resolution, including the CXF resolver's internal passes |
| 1 | Flatten |
| 2 | AttributeUnification |
| 3 | Validation |
| 4 | Instantiation |
| 5 | Schedule |
| 6 | Semantics |
| 7 | Projection |
| 8 | StoreRecovery |
| 9 | StoreSave |
| 10 | StoreInputs |
| 11 | Export |

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
