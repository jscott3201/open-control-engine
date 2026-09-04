# Public surface contract

This document and [`public-surface-ledger.json`](public-surface-ledger.json) are the normative
classification contract for the public items captured by the `oce-api` and `oce-store` blessed
baselines. It classifies the surface that exists; it does not change a Rust signature, runtime
behavior, or the pre-1.0 compatibility policy.

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
otherwise. `oce-blocks::catalog()` is a supported, actively consumed companion surface for block
metadata. It remains a separate dependency and is outside these two baselines; that separation is
not evidence that the catalog is implementation leakage.

The database-free `oce-store` traits and DTOs are conditional storage-port surface. Hosts may
provide adapters, but adapter lifecycle and identity conditions remain part of the contract.
`Engine::with_store` and `Engine::store` share that classification. The default in-memory facade
remains supported through `Engine::in_memory`.

`Engine::schedule` is implementation leakage. It stays source- and binary-shape unchanged for now;
removal requires a later coordinated change with consumers and tests.

The two placeholder loaders, their template/query facade exposure, and `InputSource::Csv` are
unstable/deferred. `ExportReport::content_id` is deprecated; use
`ExportReport::content_id_complete`. Exact members are recorded by the ledger rather than by a prose
method list.

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

String host IO (`set_input`, `get_output`, and `watch`) looks up model-local `ConnectorId` values in
the IO inventory. `PointHandle` is confined to the store-backed point-read route.

## Repeatability and durable state

Simulation output depends on `SimSpec`, parameters, and the entry connector-value image. Store-bound
inputs additionally depend on samples staged by the adapter at each tick. The shorter two-input
formulation is valid only when the supplied input source covers every relevant external input, so no
unwritten entry value can influence the horizon. The executable controls are
`sim_tests::an_undriven_input_inherits_whatever_the_entry_image_holds` and
`sim_tests::a_fully_driven_spec_reproduces_a_fresh_engine_exactly`.

`Engine::state_snapshot` produces engine-owned continuation bytes. The host persists and protects
those bytes; they do not travel through the typed `PointStore` port. That port carries typed point
samples keyed by `DomainKey`. `state_tests::capture_and_restore_call_no_store_method` verifies that
capture and restore do not call any store method.

Existing controls remain the behavioral authority for adjacent cases rather than being duplicated
here: `g36_tick_path_stays_store_pure_and_alloc_free` exercises invalid handles,
`projection_tests::source_model_iri_becomes_projection_model_id` pins model-id projection, and
`engine_tests::resolve_store_inputs_rejects_mismatched_handle_count` pins adapter cardinality.

## Downstream compatibility

Current inspected consumers establish a compatibility floor, not an exhaustive usage proof. Open
Control Studio and Logic Studio use `oce-api` together with `oce-blocks::catalog`; Verdant Watch uses
the facade. Consumers read `LoadReport.model_id.as_str()`. No inspected consumer directly imports
`PointHandle` or calls `Engine::store` or `Engine::schedule`, but absence in that sample does not
authorize removal. The dated Studio pin evidence remains available in
[`stability-baseline-2026-08-26.json`](stability-baseline-2026-08-26.json). This reconciliation makes
no downstream edit and changes no signature.

## Issue #254 reconciliation

| Contradiction | Ruling and executable/source evidence |
| --- | --- |
| `PointHandle` described as private or unreadable | Public construction/readback is required for external adapters. See `PointHandle` and `public_storage_adapter::external_adapter_uses_only_supported_public_paths`. “Opaque” means no backend type is embedded. |
| A trait-boundary prohibition was asserted for handles | Handles cross the port deliberately: `PointStore::resolve_points` returns one and `PointSnapshot::read_resolved` accepts one. The external adapter fixture exercises both sides. |
| String IO described as handle-based | `IoInventory::resolve_inputs` and `IoInventory::resolve_output` produce model connector identities; `engine::resolve_store_inputs` is the distinct point-handle route. |
| A prose list claimed to exhaust the public facade | The two blessed baselines exhaust exact signatures, and the ledger classifies every row. `public_surface_contract` supplies missing/extra/overlap and baseline-drift negative controls. Guards pin selected shapes only. |
| Python constraints described every Rust signature | `guards.rs` now states its selected-subset scope. Store and schedule signatures remain Rust-visible without becoming Python-wrapped promises. |
| Repeatability omitted the entry connector image | `Engine::simulate` rustdoc and the two named `sim_tests` controls establish the complete non-store determinant rule and the fully supplied-input special case. |
| Durable engine bytes were routed through `PointStore` | `Engine::state_snapshot`/`restore_state` own the byte channel; `PointStore` owns typed point samples. `capture_and_restore_call_no_store_method` is the negative behavioral control. |

The contract validator also injects each of these historical claims into an in-memory supported-doc
corpus and requires the corresponding rule to fail, so a validator that does nothing cannot pass.
