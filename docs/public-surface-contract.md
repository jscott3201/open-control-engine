# Public surface contract

The [generated authority summary](authority-claims.md) indexes this owner without replacing it.

This document and [`public-surface-ledger.json`](public-surface-ledger.json) are the normative
classification contract for the public items captured by the `oce-api` and `oce-store` blessed
baselines. It classifies the surface that exists; it does not change a Rust signature, runtime
behavior, or the pre-1.0 compatibility policy. Package support, `oce-api` feature selections, and
future registry eligibility are separately governed by the
[package, feature, and publication policy](package-publication-policy.md).

## Authority

When public-surface descriptions disagree, use this order:

1. this accepted tracked contract and its machine-checked ledger for design classification;
2. live Rust source together with `crates/oce-api/tests/public-api.txt` and
   `crates/oce-store/tests/public-api.txt` for exact names and signatures;
3. `crates/oce-api/src/guards.rs` for only the selected shape invariants it compiles;
4. other tracked supporting documentation.

Issue prose and local ignored `_spec/` material are historical evidence, not clone-visible
authority. Prose never substitutes for either blessed baseline's exact signature inventory.

The ledger binds one-based baseline rows to reviewed classification groups and to each baseline's
SHA-256. `public_surface_contract` expands every range and rejects an uncovered row, an out-of-range
row, an overlap, an unknown status, or baseline-byte drift.

## Classification meanings

| Status | Meaning |
| --- | --- |
| `stable-candidate` | Intended long-lived embeddable contract. Pre-1.0 change control still applies; this is not a SemVer 1.0 guarantee. |
| `conditional` | Supported only inside the stated storage-port or adapter contract and its validity/lifecycle conditions. |
| `deprecated` | Retained compatibility surface whose replacement is documented; new consumers should not adopt it. |
| `unstable/deferred` | Name or shape is reserved, but working behavior or promotion evidence is incomplete. |
| `implementation-leakage-to-remove` | Exposes an internal mechanism and is targeted for a future coordinated removal, not removal in this change. |

The ledger groups mechanically related baseline rows—such as auto-trait and blanket-impl rows—under
one rationale while assigning every baseline row exactly once.

## Surface ruling

`oce-api` is the primary host facade. Its working host operations, owned DTOs, typed errors, value
types, and state capture/restore shapes are stable candidates except where the ledger says
otherwise. `oce_api::catalog()` supplies independent typed facade metadata and versioned contracts; see
[facade contracts](facade-contracts.md). `oce-blocks::catalog()` remains a supported, actively
consumed companion surface for block metadata. It remains a separate dependency and is outside these two baselines; that separation is
not evidence that the catalog is implementation leakage.

Serialized admission (`MAX_CXF_BYTES`, `Engine::cxf_byte_limit`, `set_cxf_byte_limit`, and the
bounded `OcError` variants) is stable-candidate facade policy. The maximum is 8 MiB; hosts may
only select a limit at or below it. Existing load signatures remain, while oversized acceptance
intentionally narrows. The [migration note](facade-migration.md#bounded-serialized-load-adoption)
and [host compensation boundary](host-responsibilities.md#load-replacement-and-the-store-compensation-boundary)
separate in-memory replacement from non-rollback Store effects and external-handle validity.

The database-free `oce-store` traits and DTOs are conditional storage-port surface. Hosts may
provide adapters, but adapter lifecycle and identity conditions remain part of the contract.
`Engine::with_store` and `Engine::store` share that classification. The default in-memory facade
remains supported through `Engine::in_memory`.

Complete-frame **preparation** (`Engine::input_definitions`, `Engine::prepare_frame`, owned
`InputDefinition`, opaque `PreparedInputFrame`, and the explicit `OcError` causes) is additive
stable-candidate facade surface, not a stable release. The artifact
has no serialization or exposed resolved/generation internals. Its compatibility preflight stays
crate-private and is reused by execution. The [frame contract](complete-frame-contract.md#current-preparation-api)
owns ordering, domains, bounded errors and load/dirty-resume invalidation; the
[adoption guide](facade-contracts.md#complete-frame-preparation-adoption) describes frame-only migration.

`Engine::execute_frame`, immutable owned `CompletedFrame` and `OcError::FrameSequenceExhausted`
are likewise additive stable-candidate surface. The plan is consumed; the result exposes only model
time, engine-lifetime accepted sequence, lexical boundary `(String, Value)` pairs and Warning
diagnostics through read-only accessors. No mutable Outputs, internal connector indices or public
context token escapes. Neither frame type is serializable. Reload/resume/restore never reset the
sequence, which is correlation, not replay or deployment authority. See the
[execution contract](complete-frame-contract.md#current-execution-api). Legacy execution surfaces
and raw output access are removed, without aliases. Latest-state `get_output`/`watch` remain non-receipt inspection.

`Engine::schedule` is implementation leakage. It stays source- and binary-shape unchanged for now;
removal requires a later coordinated change with consumers and tests.

The placeholder loaders, `TemplateRef`, flat `SemanticQuery` alias, and `InputSource::Csv` have
been removed, not promoted or placed behind an unstable feature. The conditional
`oce_api::oce_store::SemanticQuery` namespace remains. `AssertLevel` now contains only `Warning`,
also its intentional default. See the [migration account](facade-migration.md) for the source break.
The now-empty deferred-surface group is removed from the ledger; its status vocabulary stays closed
and unchanged. Compiler controls across all three supported feature selections and baseline
reintroduction controls guard the removals independently of ledger hashes.

`Engine::point_list(None)` remains a supported own-inventory operation with its existing signature.
Device filtering (`Some`) is explicitly **outside** the supported profile: it always returns the
existing typed `OcError::Load` directly, even with a capable custom store. It is not a delegated
semantic query or an experimental supported feature. The refusal and the None path call no store
method and preserve the engine; `public_storage_adapter` exercises this boundary.

`ExportReport::content_id` is deprecated; use
`ExportReport::content_id_complete`. Exact members are recorded by the ledger rather than by a prose
method list.

`CompatibilityDescriptor`, `CatalogContentId`, `CompleteExportContentId` and `CompatibilityMismatch`
are additive stable-candidate facade surface. The [closed revision-1 host contract](facade-contracts.md#closed-host-compatibility-descriptor)
owns canonical text, per-field first-cause comparison, completeness refusal and evidence limits.
Only already-public catalog/shape/profile facts, the OCE Cargo package version and optional complete
export identity are covered. Private executable/generation/state-wire identities remain private.
No parser, build fingerprint, signing authority or restore/replay eligibility is added. The existing
seven domain artifacts and all package/feature/publication classifications remain unchanged.

Future Python bindings wrap a selected subset of the Rust facade. The compile guards constrain that
subset and selected owned/thread-safe shapes; they do not assert that every Rust facade signature is
Python-facing.

## Identity glossary

- **`DomainKey`** is a database-free semantic key DTO. In `LoadReport.model_id` it is a
  stable-candidate facade DTO carrying the loaded model's authored top-composite identity when
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
- **Compatibility descriptor** versions the closed public-fact receipt itself. Equality does not
  establish executable identity, model-specific IO equality, generation, state compatibility or trust.

Frame preparation and latest-state inspection resolve model-local connector identities in the IO
inventory, not Store handles. Load still validates adapter handle cardinality, but retains no runtime
Store-input handles. Preparation admits only complete executable boundary inputs with exact types
and declared domains. The private incarnation fence
is distinct from authored model identity and is neither portable nor serialized into state bytes.

## Repeatability and durable state

A frame transition depends on the loaded executable and parameters, compatible prior state, model
time, complete typed observations and fixed HostTick profile. Missing inputs refuse rather than
inheriting connector seeds, prior values or Store samples. Host simulation loops use that same
contract; no implicit horizon restart or whole-horizon transaction is supplied.

`Engine::state_snapshot` produces engine-owned continuation bytes. The host persists and protects
those bytes; they do not travel through the typed `PointStore` port. That port carries typed point
samples keyed by `DomainKey`. `state_tests::capture_and_restore_call_no_store_method` verifies that
capture and restore do not call any store method.

Existing controls remain the behavioral authority for adjacent cases rather than being duplicated
here: `frame_purity` exercises Store noninterference across the corpus,
`projection_tests::source_model_iri_becomes_projection_model_id` pins model-id projection, and
`engine_tests::load_validation_rejects_mismatched_handle_count` pins adapter cardinality.

## Downstream compatibility

Current inspected consumers establish a compatibility floor, not an exhaustive usage proof. Open
Control Studio and Logic Studio use `oce-api` together with `oce-blocks::catalog`; Verdant Watch uses
the facade. Consumers read `LoadReport.model_id.as_str()`. No inspected consumer directly imports
`PointHandle` or calls `Engine::store` or `Engine::schedule`, but absence in that sample does not
authorize removal. The dated Studio pin evidence remains available in
[`stability-baseline-2026-08-26.json`](stability-baseline-2026-08-26.json). The earlier reconciliation
changed no signatures; the subsequent [facade contraction](facade-migration.md) deliberately removes
selected names without editing downstream sources or pins. Inspected usage is not candidate
compilation, downstream acceptance or general compatibility certification.

## Issue #254 reconciliation

| Contradiction | Ruling and executable/source evidence |
| --- | --- |
| `PointHandle` described as private or unreadable | Public construction/readback is required for external adapters. See `PointHandle` and `public_storage_adapter::external_adapter_uses_only_supported_public_paths`. “Opaque” means no backend type is embedded. |
| A trait-boundary prohibition was asserted for handles | Handles cross the port deliberately: `PointStore::resolve_points` returns one and `PointSnapshot::read_resolved` accepts one. The external adapter fixture exercises both sides. |
| String IO described as handle-based | Frame bindings and `IoInventory::resolve_output` produce model connector identities; Store handles remain adapter tokens and are not frame determinants. |
| A prose list claimed to exhaust the public facade | The two blessed baselines exhaust exact signatures, and the ledger classifies every row. `public_surface_contract` supplies missing/extra/overlap and baseline-drift negative controls. Guards pin selected shapes only. |
| Python constraints described every Rust signature | `guards.rs` now states its selected-subset scope. Store and schedule signatures remain Rust-visible without becoming Python-wrapped promises. |
| Repeatability omitted determinants | Complete-frame preparation refuses omissions; the frame contract explicitly retains compatible prior state as a determinant. Historical sparse simulation is no longer a public profile. |
| Durable engine bytes were routed through `PointStore` | `Engine::state_snapshot`/`restore_state` own the byte channel; `PointStore` owns typed point samples. `capture_and_restore_call_no_store_method` is the negative behavioral control. |

The contract validator also injects each of these historical claims into an in-memory supported-doc
corpus and requires the corresponding rule to fail, so a validator that does nothing cannot pass.
