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
| 0 | Import: parse/resolution, including the CXF resolver's internal passes |
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
| [Assertions](../crates/oce-api/contracts/assertions.schema.json) | `StepReport.asserts` collects all block warnings, including Assert and other classes. Sources are currently class-level, not guaranteed instances. Repeated false Assert inputs warn each evaluation; true is silent. |
| [Execution profile](../crates/oce-api/contracts/execution-profile.schema.json) | Fixed HostTick v1: one advance per successful call, including equal timestamps; no Modelica same-time event iteration. Descriptive, not a runtime selector or snapshot revision. |

Warning collection remains confined to `step_realtime`; ordinary tick and simulation keep no-op
sinks. Realtime writes follow the tick and may fail before a collected report is delivered.
Load/store side effects and commit ordering are unchanged. This adds no rollback, admission bounds,
warn-once, escalation, scheduler, equipment policy or safety guarantee.

## Consumer migration boundary

Existing consumers can continue using their current APIs. New adapters can depend on `oce-api`
alone for the typed catalog and receipts. Studio retains its full source/build/features identity,
catalog policy, diagnostic truncation and authored-target mapping. These are separate from the
facade catalog content tag. No future OCE commit is embedded in the artifact, and this change
advances no downstream source or pin. Coordinated companion removal remains later work.

The [migration record](facade-migration.md#additive-contract-adoption) distinguishes this additive
contract from the earlier facade contraction. Full local tests, exact public baselines and external
compiler fixtures establish bounded implementation evidence; hosted cross-architecture checks and
actual downstream qualification remain separate evidence.
