# Executable CXF and HostTick product contract

Document revision: 8
Grounding SHA: 8ea3e8f38d580868179bfa985b443b71ca3b86c1

This is the aggregate product boundary and requirement-to-evidence map for the work toward a
stable embeddable kernel. It records current observations, host obligations, and future acceptance
outcomes separately. **It does not declare a published stable product or change runtime behavior.**
The document revision is not a runtime profile, catalog, snapshot, execution ABI, wire, or release
identity. HostTick v1 remains the fixed existing profile, not a new selectable API.

## Authority and owners

Domain authorities retain their scope; this aggregate does not supersede them:

| Owner role | Delegated authority |
| --- | --- |
| Contract maintainers | This document's revision, traceability and acceptance boundaries; affected domain owners decide semantics. |
| Facade maintainers | [Public surface contract](public-surface-contract.md#surface-ruling), exact [facade baseline](../crates/oce-api/tests/public-api.txt) and [storage baseline](../crates/oce-store/tests/public-api.txt). |
| Release maintainers | [Package and publication policy](package-publication-policy.md#closed-package-matrix), including its separate [feature matrix](package-publication-policy.md#closed-oce-api-feature-matrix). |
| CXF maintainers | [Composite subset](cxf-composite-subset.md#active-nodes) and [round-trip contract](cxf-round-trip.md#the-rt-2-contract). |
| Execution maintainers | [Execution profile](execution-profile.md#hosttick-v1), [complete-frame contract](complete-frame-contract.md#status-and-authority), load/execute/state implementation and focused tests below. |
| Block semantics maintainers | Local block behavior and bounded [conformance boundary](execution-profile.md#conformance-boundary); upstream provenance is a separate evidence question. |
| Host integrator | [Host responsibilities](host-responsibilities.md); qualification of the actual consuming application, adapter and equipment. |

These are accountable roles, not claims of named-person assignments. Conflicts go to the affected
domain owner with source/test evidence before a revision is accepted. Existing baselines, ledgers,
and the [authority index](authority-claims.md) retain their separate checks. This is not another
signature ledger or generated authority projection.

## Reading the requirements

The single table below is the normative grammar. Each physical row has the eight displayed cells,
one immutable `PC-` number, one uppercase obligation keyword in the Requirement cell, a named actor
and owner, a nonempty limitation, grounding links, and either named test links or one future-outcome
assignment. IDs are allocated in ascending order without reuse; withdrawn obligations remain
visible pending an explicit revision rather than disappearing silently.

- **CURRENT**: observed behavior or present claim boundary, not a published stability promise.
  PC-031 is accepted normative contract/evidence delivery, not a runtime frame implementation.
- **HOST-OBLIGATION**: required host policy now. Engine boundary tests show why responsibility is
  outside the engine; they do **not** prove that any host complies.
- **FUTURE**: acceptance outcome only, explicitly **not implemented by this contract**. Existing
  partial mechanisms are not evidence that the complete future contract is available.

Grounding links identify source or delegated policy. `test` links name an existing test declaration;
the line fragment includes that declaration. `future` links name a later work item and point to its
clone-visible outcome here. Multiple test links use semicolons. Limitations are part of each row,
not optional caveats. All product obligations are indexed in this table; linked domain contracts
define their detail without changing the current/future acceptance boundary.

## Requirements

| ID | Status | Actor | Owner | Requirement | Limitation | Grounding | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PC-001 | CURRENT | Contract maintainer | Contract maintainers | MUST retain immutable requirement IDs and record a document revision, affected-owner approval, evidence assessment and migration review for normative changes; promotion from FUTURE requires implementation acceptance. | Traceability tests check structure only; human approval and semantic relevance are not mechanically proven. Editorial-only changes still receive a change record. | [Change record](#change-record) | test [test_report_is_an_independent_byte_golden](../scripts/product_contract/test_check.py#L94-L100) |
| PC-002 | CURRENT | Product claimant | Release maintainers | MUST preserve oce-api as host facade, oce-store as conditional adapter port and oce-blocks catalog as transitional companion under the delegated package and surface classifications. | Nothing is published; stable-candidate is not stable SemVer. Removed placeholder loaders are unavailable; implementation dependencies are not independently supported APIs. | [Packages](package-publication-policy.md#closed-package-matrix); [Surface](public-surface-contract.md#surface-ruling); [CXF ingest](../crates/oce-api/src/engine.rs#L221-L257) | test [test_ratified_publish_and_private_sets_are_exact](../scripts/package_policy/test_validate.py#L181-L191); [test_owner_approved_categories_cannot_drift_with_same_publish_bit](../scripts/package_policy/test_validate.py#L193-L209); [valid_minimal_loop_loads_clean](../crates/oce-api/tests/conformance.rs#L78-L88); [retired_facade_symbols_are_absent](../crates/oce-api/tests/public_surface_contract.rs#L507-L512) |
| PC-003 | CURRENT | Product claimant | CXF maintainers | MUST constrain executable CXF claims to the closed, bounded, already-specialized graph profile, including the documented bounded composite lowering. | Not general Modelica flattening, source recovery, every CXF construct, or promotion of every accepted syntax to stable support. Semantic and traversal bounds do not bound serialized bytes. | [Load pipeline](../crates/oce-api/src/engine.rs#L221-L257); [Subset](cxf-composite-subset.md#active-nodes); [Removed loaders](facade-migration.md#removed-names-and-working-alternatives) | test [composite_nesting_accepts_the_limit_and_rejects_one_past](../crates/oce-cxf/tests/ingest_totality.rs#L414-L429); [boundary_hops_accept_the_limit_and_reject_the_attempted_next_hop](../crates/oce-cxf/tests/ingest_totality.rs#L465-L486); [retired_facade_symbols_are_absent](../crates/oce-api/tests/public_surface_contract.rs#L507-L512) |
| PC-004 | HOST-OBLIGATION | Host | Host integrator | MUST cap serialized CXF transport/buffering and apply isolation appropriate to the trust boundary, outside any process actively commanding equipment for untrusted programs. | Facade admission defaults to 8 MiB and can only be configured at or below that maximum; a byte cap does not bound memory, CPU, expansion or qualify a host. The low-level parser is not the bounded facade. | [Admission](../crates/oce-api/src/admission.rs#L1-L41); [Untrusted input](host-responsibilities.md#treat-untrusted-cxf-as-untrusted-input) | test [inclusive_default_and_stricter_boundaries_preserve_legacy_acceptance](../crates/oce-api/tests/cxf_admission.rs#L44-L80) |
| PC-005 | CURRENT | Product claimant | Execution maintainers | MUST distinguish ordinary failed-load preservation of the complete in-memory executable/run image from external Store transactionality. | Recover, save_model and handle resolution may have non-rollback effects; old external handle validity is not promised. Panic, process death, allocation failure and concurrent host effects are excluded. Flatten and semantics currently have no returned refusal path. | [Build and commit](../crates/oce-api/src/engine.rs#L162-L225); [Compensation](host-responsibilities.md#load-replacement-and-the-store-compensation-boundary) | test [pre_store_refusals_preserve_fresh_advanced_and_dirty_run_images](../crates/oce-api/src/tests/reload_tests.rs#L142-L185); [store_refusals_preserve_fresh_advanced_and_dirty_run_images](../crates/oce-api/src/tests/reload_tests.rs#L188-L228); [private_build_tail_refusals_preserve_the_run_image](../crates/oce-api/src/tests/reload_tests.rs#L266-L296); [failed_handle_resolution_leaves_saved_model_and_allocated_store_handles](../crates/oce-api/src/tests/reload_tests.rs#L231-L263) |
| PC-006 | CURRENT | Engine | Execution maintainers | MUST use finite nondecreasing model seconds and perform one emit pass followed by one stateful update pass per successful HostTick call, including equal timestamps. | Loaded-block time representability checks also apply. Pre emits entry memory then latches current input; no Modelica fixed-point iteration or convergence test. | [Profile](execution-profile.md#hosttick-v1); [Tick guard and core](../crates/oce-api/src/engine.rs#L363-L409); [Evaluator](../crates/oce-graph/src/tick.rs#L128-L195) | test [parameter_seed_is_first_call_output_and_equal_time_calls_advance_memory](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L100-L128); [nonconvergent_boolean_feedback_is_accepted_and_advances_per_call](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L130-L158) |
| PC-007 | CURRENT | Engine | Execution maintainers | MUST retain sparse input staging: set_input stages named typed slots, and an available store sample overwrites bound slots before evaluation regardless of sample status or timestamp. | Not a complete typed frame. Driver wiring can override a connector slot's effect. Quality, freshness and plausibility are host policy. | [Explicit staging](../crates/oce-api/src/sim.rs#L615-L622); [Store conversion](../crates/oce-api/src/engine.rs#L519-L574) | test [store_backed_input_staging_is_status_agnostic](../crates/oce-api/src/tests/store_backed_inputs.rs#L87-L109) |
| PC-008 | CURRENT | Engine | Execution maintainers | MUST hold a bound input's current connector value when its store sample is missing. | Missing is not a refusal or unknown-value marker; before staging, type seeds may be zero or false. No freshness diagnostic follows. | [Missing samples](../crates/oce-api/src/engine.rs#L519-L540) | test [missing_store_sample_holds_prior_input_value](../crates/oce-api/src/tests/store_backed_inputs.rs#L111-L156) |
| PC-009 | CURRENT | Product claimant | Execution maintainers | MUST expose that a valid store-input prefix can remain staged before a later type refusal, closing the fresh durable-restore window. | No block evaluates and time/output snapshot do not advance on that refusal; connector staging is nevertheless non-atomic. Snapshot acquisition failure stages no store sample. | [Staging order](../crates/oce-api/src/engine.rs#L363-L423); [Prefix writes](../crates/oce-api/src/engine.rs#L526-L536) | test [partial_store_staging_closes_the_fresh_durable_restore_window](../crates/oce-api/src/tests/store_backed_inputs.rs#L158-L209) |
| PC-010 | CURRENT | Engine | Execution maintainers | MUST preflight simulation collection, constant inputs and the first closure list before resetting the prior run clock or state words. | The first closure result is reused once; preflight preservation does not extend to all later failures or host closure side effects. | [Simulation preflight](../crates/oce-api/src/sim.rs#L480-L529) | test [a_wrong_type_in_the_first_closure_list_preserves_the_prior_run](../crates/oce-api/src/tests/sim_tests.rs#L398-L404) |
| PC-011 | CURRENT | Engine | Execution maintainers | MUST treat simulation after successful preflight as a restart of state words and prev_t, not of the connector image. | Undriven inputs inherit entry values; store samples add determinants. Splitting a horizon does not imply continuation or repeatability from spec and parameters alone. | [Restart](../crates/oce-api/src/sim.rs#L527-L568) | test [an_undriven_input_inherits_whatever_the_entry_image_holds](../crates/oce-api/src/tests/sim_tests.rs#L211-L253) |
| PC-012 | CURRENT | Product claimant | Execution maintainers | MUST retain the limitation that a later simulation closure refusal leaves earlier ticks and any valid prefix staging applied. | No whole-horizon transaction or automatic rollback; metrics and partial trace need not be returned on error. | [Dynamic staging](../crates/oce-api/src/sim.rs#L698-L717) | test [a_closure_naming_an_unknown_point_still_refuses_mid_run](../crates/oce-api/src/tests/input_staging_tests.rs#L417-L448) |
| PC-013 | CURRENT | Product claimant | Execution maintainers | MUST distinguish simulation preflight refusal from a first-tick store refusal after restart. | On first-tick snapshot/type failure, words are re-seeded and prev_t cleared while model time and output snapshot can remain prior; a valid store prefix may remain staged. | [Restart before tick](../crates/oce-api/src/sim.rs#L527-L556) | test [a_first_tick_snapshot_error_leaves_the_restart_reset_in_effect](../crates/oce-api/src/tests/store_backed_inputs.rs#L336-L416); [a_first_tick_store_type_error_leaves_the_restart_reset_in_effect](../crates/oce-api/src/tests/store_backed_inputs.rs#L211-L334) |
| PC-014 | CURRENT | Engine | Execution maintainers | MUST validate the realtime epoch mapping before ticking and attempt store write-back only after the tick has applied. | Write failure leaves state committed and returns Err(Store) without StepReport, collected warnings, receipt or generation. Hosts reconcile outputs/watch and state. Written count on success is an adapter receipt, not generic durability, rollback or actuator delivery. | [Realtime order](../crates/oce-api/src/sim.rs#L572-L605) | test [host_epoch_is_required_and_exact_mapping_handles_signed_model_time](../crates/oce-api/src/tests/realtime_write_back_tests.rs#L92-L156); [failed_store_write_returns_typed_error_after_tick_remains_applied](../crates/oce-api/src/tests/realtime_write_back_tests.rs#L313-L335); [write_error_drops_the_collected_warning_report_without_a_generation_receipt](../crates/oce-api/src/tests/realtime_write_back_tests.rs#L382) |
| PC-015 | CURRENT | Engine | Execution maintainers | MUST interpret halt as parameter-edit permission and dirty resume as block/run re-seeding, not equipment stop or live tuning. | Halt does not prevent ticks. Resume does not recompute schedule, store projection or authored model identity; no transactional resume guarantee. Pending edits refuse state capture/restore. | [Parameter lifecycle](../crates/oce-api/src/params.rs#L167-L278); [State preconditions](../crates/oce-api/src/state.rs#L422-L438) | test [param_lifecycle_halt_set_resume_refolds](../crates/oce-api/src/tests/frozen_surface.rs#L210-L256); [pending_parameter_edits_take_precedence_over_restore_readiness](../crates/oce-api/src/tests/state_tests.rs#L394-L411) |
| PC-016 | CURRENT | Engine | Facade maintainers | MUST preserve completed-stage diagnostics through contextual load errors: diagnostics exposes terminal diagnostics and all_diagnostics prepends available prior diagnostics. | Empty terminal diagnostics does not mean success; severity is not list position. Message text is not a newly frozen schema. | [Diagnostic accessors](../crates/oce-api/src/error.rs#L160-L215) | test [warning_context_survives_each_later_store_failure](../crates/oce-api/src/engine_tests.rs#L156-L186) |
| PC-017 | CURRENT | Product claimant | Facade maintainers | MUST describe AssertLevel as Warning-only with Default equal to Warning and realtime warning collection, with tick and simulation using a no-op diagnostic sink. | The removed Error variant and changed default are intentional source/value breaks, not a safety interlock or new escalation. A failed write can prevent report delivery. | [Severity and collector](../crates/oce-api/src/sim.rs#L398-L423); [No-op tick](../crates/oce-api/src/engine.rs#L358-L361) | test [default_severity_is_warning_not_an_unemitted_failure](../crates/oce-api/tests/sim_assertions.rs#L17-L24); [boolean_assertions_repeat_warning_records_and_continue_bit_exactly](../crates/oce-api/tests/sim_assertions.rs#L79-L87); [step_realtime_delivers_assert_diagnostics_and_simulate_drops_them](../crates/oce-api/src/assert_tests.rs#L78-L101) |
| PC-018 | CURRENT | Product claimant | CXF maintainers | MUST distinguish partial successful export from complete exported-document identity and use content_id_complete to refuse warning-bearing exports before treating the tag as complete. | Survivor-cone export is not source recovery or whole-model identity. FNV-1a-128 over emitted bytes is noncryptographic, not authentication; authored model identity can stay unchanged as export changes. | [Round trip](cxf-round-trip.md#the-deferral-trap); [Complete tag](../crates/oce-api/src/export.rs#L33-L89) | test [warning_bearing_content_id_identifies_the_partial_document](../crates/oce-api/tests/export_cxf.rs#L124-L147); [content_id_tracks_exported_synthetic_document_while_model_id_stays_authored](../crates/oce-api/tests/export_cxf.rs#L97-L122) |
| PC-019 | CURRENT | Engine | Execution maintainers | MUST validate compatible process-local checkpoint restore before committing values, words and clock, permitting rewind without evaluating or calling the store. | Pending edits and invalid state refuse. The bit-atomic test covers its corrupted-state scenario; the no-store test covers capture and durable restore, while checkpoint restore's no-store claim is source-grounded. | [Checkpoint](../crates/oce-api/src/state.rs#L367-L393); [Prepare and commit](../crates/oce-api/src/state.rs#L471-L562) | test [checkpoint_refusal_is_bit_atomic](../crates/oce-api/src/tests/state_tests.rs#L448-L492); [capture_and_restore_call_no_store_method](../crates/oce-api/src/tests/state_tests.rs#L433-L446) |
| PC-020 | CURRENT | Engine | Execution maintainers | MUST limit durable restore to a successfully loaded compatible target before a mutation boundary, validate before commit and restore continuation without evaluation or store calls. | Startup-only differs from checkpoint rewind. Pending edits refuse first; snapshot bytes omit host epoch, backend history and equipment authority. Equal restored time is another HostTick transition. | [Durable restore](../crates/oce-api/src/state.rs#L395-L438); [Prepare and commit](../crates/oce-api/src/state.rs#L471-L562) | test [mutation_boundaries_close_the_durable_restore_window](../crates/oce-api/src/tests/state_tests.rs#L342-L392); [capture_and_restore_call_no_store_method](../crates/oce-api/src/tests/state_tests.rs#L433-L446); [snapshot_restores_next_pre_output_at_same_timestamp](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L160-L193) |
| PC-021 | CURRENT | Product claimant | Facade maintainers | MUST distinguish authored/source model, executable, exported-document, catalog, IO schema, build, execution profile, generation and state-revision identity roles. | The current manifest checks executable compatibility, not a new build/generation authentication contract. Diagnostic model id is not the compatibility key. Facade descriptors version bounded metadata and diagnostic shapes separately; host build and generation admission remain future work. | [Manifest](../crates/oce-api/src/state.rs#L340-L365); [Compatibility](../crates/oce-api/src/state.rs#L471-L493); [Identity glossary](public-surface-contract.md#identity-glossary) | test [diagnostic_model_identity_is_not_an_execution_compatibility_key](../crates/oce-api/src/tests/state_tests.rs#L509-L516); [content_id_tracks_exported_synthetic_document_while_model_id_stays_authored](../crates/oce-api/tests/export_cxf.rs#L97-L122) |
| PC-022 | CURRENT | Product claimant | Block semantics maintainers | MUST NOT claim upstream equivalence for the local two-input TrueHoldWithReset behavior. | Its u/clr test proves local names and clear behavior only. Upstream-equivalent existence and provenance remain unresolved, not proven absent; no catalog identity or behavior changes here. | [Local implementation](../crates/oce-blocks/src/logical_timing.rs#L440-L489) | test [true_hold_with_reset_names_match_its_behaviour](../crates/oce-blocks/src/port_names_tests.rs#L245-L292) |
| PC-023 | CURRENT | Product claimant | Block semantics maintainers | MUST NOT extrapolate fixture/profile evidence into arbitrary G36 support, Modelica event equivalence, blanket numerical exactness or universal panic freedom. | Pre is excluded from expected-green Modelica same-time differential claims; no concurrency, cancellation, deadlock or adapter-panic guarantee is added. | [Conformance boundary](execution-profile.md#conformance-boundary); [Evidence context](#evidence-context) | test [nonconvergent_boolean_feedback_is_accepted_and_advances_per_call](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L130-L158) |
| PC-024 | HOST-OBLIGATION | Host | Host integrator | MUST qualify input quality, freshness, missing-data reaction and plausibility before or instead of execution. | Status/timestamp-agnostic staging and hold-last tests prove an engine boundary, not actual host compliance, sensor validity or safety qualification. | [Host policy](host-responsibilities.md#the-engine-implements-no-fail-safe-policy-of-its-own) | test [store_backed_input_staging_is_status_agnostic](../crates/oce-api/src/tests/store_backed_inputs.rs#L87-L109); [missing_store_sample_holds_prior_input_value](../crates/oce-api/src/tests/store_backed_inputs.rs#L111-L156) |
| PC-025 | HOST-OBLIGATION | Host | Host integrator | MUST implement NO_EVAL by not executing the engine and separately own safe-state outputs, equipment interlocks and actuation. | NO_EVAL is neither halt nor a fabricated zero frame. Engine tests do not qualify equipment protection or prove command delivery; a halted engine can still execute. | [Lifecycle boundary](host-responsibilities.md#lifecycle-names-are-not-equipment-controls); [Halt](../crates/oce-api/src/params.rs#L167-L176) | test [param_lifecycle_halt_set_resume_refolds](../crates/oce-api/src/tests/frozen_surface.rs#L210-L256) |
| PC-026 | HOST-OBLIGATION | Host | Host integrator | MUST own scheduling, model-time cadence, wall-clock mapping and recovery from convenience-path write failures. | No scheduler, deadline, cancellation or equipment-stop guarantee; repeating equal time advances again, and write failure does not roll back the engine. Boundary tests do not prove host policy. | [Host time](host-responsibilities.md#time-is-host-supplied); [Realtime](../crates/oce-api/src/sim.rs#L570-L601) | test [host_epoch_is_required_and_exact_mapping_handles_signed_model_time](../crates/oce-api/src/tests/realtime_write_back_tests.rs#L92-L156); [failed_store_write_returns_typed_error_after_tick_remains_applied](../crates/oce-api/src/tests/realtime_write_back_tests.rs#L313-L335) |
| PC-027 | HOST-OBLIGATION | Host | Host integrator | MUST own persistence, authentication, authorization, freshness, generation fencing and restored actuator ownership outside engine snapshot capture/restore. | Integrity and executable compatibility are not authenticity or permission to command. Typed PointStore samples are not the engine-byte persistence channel. Boundary tests do not qualify an adapter or host. | [Host state duties](host-responsibilities.md#persist-engine-state-outside-the-store-port) | test [capture_and_restore_call_no_store_method](../crates/oce-api/src/tests/state_tests.rs#L433-L446) |
| PC-028 | CURRENT | Facade delivery | Facade maintainers | MUST remove or quarantine deferred and panic-only supported surfaces with coordinated compatibility evidence. | Selected facade names are removed; private quarantines and package boundaries are unchanged. Compiler controls and bounded source inspection are not downstream acceptance or universal panic freedom; exact-candidate consumer qualification remains separate. | [Current ruling](public-surface-contract.md#surface-ruling); [Migration and inventory](facade-migration.md#package-and-panic-inventory-boundary) | test [retired_facade_symbols_are_absent](../crates/oce-api/tests/public_surface_contract.rs#L507-L512); [filtered_inventory_refuses_without_store_calls_or_engine_mutation](../crates/oce-api/tests/public_storage_adapter.rs#L175-L227) |
| PC-029 | CURRENT | Facade delivery | Facade maintainers | MUST expose versioned facade catalog, diagnostics, IO, values, parameters, assertions and execution-profile contracts with compatibility tests. | Additive typed metadata and immutable producer receipts; opaque subjects, Warning-only runtime, no generic value codec, build stamp, admission bounds, rollback or new snapshot/profile selector. | [Versioned contracts](facade-contracts.md); [Adoption](facade-migration.md#additive-contract-adoption) | test [canonical_catalog_matches_packaged_bytes_and_repeats_exactly](../crates/oce-api/tests/catalog_contract.rs#L16-L34); [unification_evidence_survives_structural_refusal_at_its_actual_producer](../crates/oce-api/src/tests/diagnostic_receipts.rs#L201-L219); [descriptors_cover_every_domain_with_explicit_shapes_and_semantic_limits](../crates/oce-api/src/tests/contract_schemas.rs#L10-L28) |
| PC-030 | CURRENT | Admission delivery | CXF maintainers | MUST reject serialized inputs above the effective per-engine cap before parsing, Store calls or engine mutation and replace model-bound state/caches only on successful load. | Default and maximum are exactly 8 MiB. Configuration above the maximum refuses through a typed error. Receipt refusal stays Import; errors contain counts only. External persistence is not atomic; replacement coverage has the documented stage limitations. | [Admission](../crates/oce-api/src/admission.rs#L1-L41); [Replacement](#bounded-admission-and-replacement) | test [oversized_valid_document_refuses_without_parser_allocation_or_store_calls](../crates/oce-api/tests/cxf_admission.rs#L16-L32); [oversize_receipts_allocate_only_the_error_box_and_are_deterministic](../crates/oce-api/tests/cxf_admission.rs#L104-L138); [invalid_configuration_is_typed_and_never_silently_widens](../crates/oce-api/tests/cxf_admission.rs#L83-L101); [successful_reload_replaces_model_bound_caches_but_not_host_policy](../crates/oce-api/src/tests/reload_tests.rs#L299-L380) |
| PC-031 | CURRENT | Frame delivery | Execution maintainers | MUST retain the normative complete generation-atomic typed input/output frame contract, including completeness, prevalidation, reload fencing, one HostTick transition, refusal preservation and immutable correlated outputs/diagnostics. | This row ratifies contract/evidence; preparation, native transition and shared evaluation-core reuse have separate implementation evidence. Legacy gap tests alone do not implement frames or qualify hosts, persistence or actuators. | [Frame contract](complete-frame-contract.md#status-and-authority); [Refusal matrix](complete-frame-contract.md#prevalidation-and-refusal-matrix); [Outcome](complete-frame-contract.md#accepted-transition-and-immutable-outcome) | test [setter_refusal_keeps_the_already_staged_prefix](../crates/oce-api/tests/legacy_frame_boundary.rs#L54-L86); [store_type_refusal_keeps_a_prefix_without_advancing_execution](../crates/oce-api/tests/legacy_frame_boundary.rs#L88-L123); [test_report_is_an_independent_byte_golden](../scripts/product_contract/test_check.py#L94-L100) |
| PC-032 | CURRENT | Frame delivery | Execution maintainers | MUST resolve and prevalidate complete typed frames, refusing unknown, duplicate, missing, stale-generation and unloaded submissions before mutation. | Preparation only; no commit or output frame. Internal incarnation preflight is not host freshness or authorization. No public input aliases exist; injected alias coverage tests logical uniqueness only. | [Preparation](../crates/oce-api/src/frame.rs); [Contract](complete-frame-contract.md#current-preparation-api) | test [every_refusal_preserves_fresh_and_advanced_stateful_images_and_store](../crates/oce-api/tests/prepare_frame_preservation.rs#L22-L155); [reload_and_cross_engine_identity_refuse_even_identical_model_bytes](../crates/oce-api/src/frame_tests.rs#L103-L125); [dirty_resume_invalidates_but_clean_resume_and_compatible_restore_retain_context](../crates/oce-api/src/frame_tests.rs#L128-L170); [canonical_plan_owns_values_and_repeats_bit_exactly_under_entry_permutations](../crates/oce-api/src/frame_tests.rs#L52-L79); [repeated_large_fixture_preparation_has_a_linear_capacity_and_allocation_census](../crates/oce-api/src/frame_tests.rs#L340) |
| PC-033 | CURRENT | Frame delivery | Execution maintainers | MUST commit one HostTick transition and one immutable output/diagnostic frame per accepted frame, preserving time, state, connector values, output generation and replay identity on ordinary refusal. | Native in-place engine transition only; sequence is Engine-lifetime correlation, not serialized replay/deployment identity. No persistence or actuator-delivery atomicity, panic recovery or cancellation guarantee. | [Execution](../crates/oce-api/src/frame.rs); [Contract](complete-frame-contract.md#current-execution-api) | test [arithmetic_commits_complete_values_without_store_and_retains_independent_results](../crates/oce-api/tests/execute_frame.rs#L41); [sampled_delay_matches_hand_recurrence_and_retained_frames_survive_lifecycle_changes](../crates/oce-api/tests/execute_frame.rs#L128); [context_readiness_and_time_refusals_preserve_public_images_and_store_calls](../crates/oce-api/tests/execute_frame_preservation.rs#L29); [each_preflight_refusal_preserves_every_execution_bit_and_sequence](../crates/oce-api/src/frame_commit_tests.rs#L66); [complete_frames_match_the_independent_hosttick_reference_and_repeat_bit_exactly](../crates/oce-api/tests/g36_cooling_only_controller.rs#L417); [allocation_cost_is_only_prepared_targets_boundary_results_and_emitted_warnings](../crates/oce-api/tests/frame_observations.rs#L55) |
| PC-034 | CURRENT | Convenience delivery | Execution maintainers | MUST route simulation and realtime execution through shared transition semantics or explicitly document the weaker convenience profile. | Only the infallible evaluation/refresh core is shared, not frame preparation. Legacy staging, restart, sinks, projections and post-transition Err(Store) remain; no receipt, generation report, whole-horizon or Store-write rollback. Parity covers lifecycle-equivalent fully driven subsets only. | [Shared core](../crates/oce-api/src/engine.rs#L386-L409); [Weaker profiles](complete-frame-contract.md#legacy-paths-and-migration); [Acceptance](#shared-convenience-core) | test [native_route_enters_core_and_evaluates_once_per_call](../crates/oce-api/src/shared_transition_tests.rs#L122); [tick_route_enters_core_and_evaluates_once_per_call](../crates/oce-api/src/shared_transition_tests.rs#L127); [simulation_route_enters_core_and_evaluates_once_per_call](../crates/oce-api/src/shared_transition_tests.rs#L132); [realtime_route_enters_core_and_evaluates_once_per_call](../crates/oce-api/src/shared_transition_tests.rs#L137); [fully_driven_add_and_sampled_delay_match_at_equivalent_lifecycle_boundaries](../crates/oce-api/src/shared_transition_tests.rs#L172); [rounded_equal_time_pre_grid_keeps_projection_sets_distinct_and_post_transition](../crates/oce-api/src/shared_transition_tests.rs#L250); [warnings_are_dropped_without_collection_cost_or_retained_by_the_selected_caller](../crates/oce-api/src/shared_transition_tests.rs#L343); [failed_writes_preserve_committed_pre_memory_and_reconcilable_output_views](../crates/oce-api/src/tests/realtime_write_back_tests.rs#L338); [write_error_drops_the_collected_warning_report_without_a_generation_receipt](../crates/oce-api/src/tests/realtime_write_back_tests.rs#L382) |
| PC-035 | FUTURE | Convenience delivery | Execution maintainers | MUST classify and guard legacy sparse and store-backed profiles separately from complete atomic frames. | Existing hold-last, status-agnostic and partial-staging behavior remains visible until an accepted migration changes it. | [Current store staging](../crates/oce-api/src/engine.rs#L519-L540) | future [M02-PR05](#legacy-profile-boundaries) |
| PC-036 | FUTURE | Identity delivery | Facade maintainers | MUST introduce distinct typed identity layers and a compact compatibility descriptor. | Identity is not authentication; current manifest fields do not fulfill the whole future descriptor. | [Current manifest](../crates/oce-api/src/state.rs#L340-L365) | future [M03-PR01](#typed-identities) |
| PC-037 | FUTURE | Evidence delivery | Block semantics maintainers | MUST retain a strict-bit cross-platform exactness matrix or explicit reasons for paths remaining tolerance-qualified. | No automatic widening of tolerances or promotion of self-output into an independent oracle. | [Testing standard](../TESTING.md#the-four-pillars) | future [M03-PR02](#strict-bit-evidence) |
| PC-038 | FUTURE | State delivery | Execution maintainers | MUST stabilize same-build durable continuation and explicit portability domains with refusal evidence. | No general cross-build restore, host authentication or actuator ownership inferred. | [Current restore](../crates/oce-api/src/state.rs#L411-L438) | future [M03-PR03](#same-build-state) |
| PC-039 | FUTURE | Replay delivery | Execution maintainers | MUST define canonical execution-frame and replay records that reproduce or refuse deterministically. | Current snapshots and simulation traces are not the complete future replay contract. | [Current state image](../crates/oce-api/src/state.rs#L355-L365) | future [M03-PR04](#canonical-replay) |
| PC-040 | FUTURE | Release delivery | Release maintainers | MUST establish release-to-release compatibility and refusal tests before making those support claims. | No release compatibility or actual publication is authorized by this document. | [Publication authority](package-publication-policy.md#reversal-before-release-freeze) | future [M03-PR05](#release-compatibility) |

## Fulfilled facade contraction

Revision 2 implements the selected M01-PR02 removals and Warning default, with compiler absence
controls, baseline reintroduction controls, typed inventory refusal and CXF assertion goldens.
This is implementation evidence for the bounded surface outcome, not a merged-release or downstream
acceptance claim. The [migration record](facade-migration.md) retains compatibility limits.

## Facade schemas

Revision 3 implements M01-PR03 as additive facade catalog DTOs, canonical metadata identity,
packaged shape descriptors and immutable producer-stage load/export receipts. PC-029 acceptance
is bounded by the linked implementation tests and compatibility controls. Existing legacy ordering,
Warning-only runtime behavior and snapshot bytes remain; complete frame
contracts and build/generation qualification remain future work. Hosted and downstream evidence
remain separately qualified in the delivery record.

## Bounded admission and replacement

Revision 4 implements the bounded M01-PR04 outcome with an inclusive 8 MiB default/maximum,
per-engine tightening and typed pre-parser refusal shared by both load entry points. The failure
matrix compares the complete in-memory run image for fresh, advanced and halted/dirty runs.
Instantiation/projection use the existing private build seam; flatten and semantics have no current
returned refusal path. Store residual effects are tested separately, not hidden by the image comparison.
The host owns compensation and external-handle validity. Successful replacement refreshes model-bound
caches, while host epoch/admission policy persist. Bounded parser allocation observation is supporting
evidence, not a general hostile-input safety or peak-memory bound.

## Complete frame contract

Revision 5 fulfills M02-PR01 as the [normative frame/output contract](complete-frame-contract.md)
and passing contract-to-current-code gap evidence. The owner approved PC-031 promotion on that
contract-only implementation acceptance; PC-032 through PC-035 remain FUTURE. The loaded-executable
and IO fence is engine-local, distinct from host deployment fencing. No public frame representation,
runtime path, diagnostic severity, catalog, state/snapshot bytes, dependency or package changes here.

## Complete frame prevalidation

Revision 6 implements M02-PR02: owned input definitions and opaque, nonserializable prepared
frames, with canonical first-cause typed refusals, exact domain checks, Store noninterference and
engine-local load/rebuild fencing. PC-032 evidence includes stateful before/after images, fan-out,
same-byte reload and clean/dirty-resume controls, exact goldens and a repeated allocation census.
The internal compatibility seam is exercised directly for future commit reuse; no otherwise-unused
public validator or reusable schema/cache API is added. Current input aliases do not exist, and
enum/String detached-probe coverage does not broaden the executable CXF profile.
PC-033 through PC-035 remain FUTURE. Legacy sparse, hold-last and last-wins behavior, snapshots,
profile, diagnostics, dependencies and downstream pins are unchanged. See the
[migration guidance](facade-contracts.md#complete-frame-preparation-adoption).

## Atomic transition and frame

Revision 7 implements M02-PR03 as consuming `execute_frame` and owned immutable `CompletedFrame`.
All ordinary refusal conditions precede mutation; one successful call stages every target, evaluates
once, refreshes latest outputs and retains lexical boundary outputs plus Warning diagnostics.
The lifetime-local accepted sequence cannot rewind on restore/reload/resume or wrap on exhaustion;
legacy paths never consume it. The private retained context fence exposes no durable identity.
No RunState shadow/rollback or Store path is reused. PC-033 is CURRENT on this bounded evidence;
PC-034/035 remained FUTURE at revision 7. The [adoption guide](facade-contracts.md#complete-frame-execution-adoption)
and [cost observations](benchmarks.md#complete-frame-observations) retain the compatibility and
measurement limits. Snapshot/profile/catalog bytes and downstream pins are unchanged.

## Shared convenience core

Revision 8 implements M02-PR04: native frames and legacy tick share one private infallible HostTick
evaluation/refresh implementation. Simulation and realtime retain their legacy orchestration into
that seam. Preflight, input staging, words-only restart, diagnostic sinks, native-only sequence and
output/trace/durable projections remain caller-owned and unchanged. This is not complete-frame
preparation reuse or universal mode equality. PC-034 is CURRENT for this bounded reuse and explicit
weaker-profile classification; PC-035 remains FUTURE for full legacy classification/guards.

The planning phrase “write failure reports committed generation” is replaced by compatible
reconciliation semantics: realtime write failure still returns Err(Store) after committing state.
The error carries no generation, receipt, StepReport or collected warnings. Hosts reconcile through
time guards, outputs/watch, captured state and their own external delivery records; equal-time
retry is another transition. No new public, snapshot, wire, catalog or profile contract is introduced.

Hand-derived Add, sampled Delay and equal-time Pre expectations anchor only lifecycle-equivalent
fully driven subsets. Private seam-entry and block emit/update counters detect bypass and double
evaluation. Existing restart, preflight, partial-staging and frame allocation controls remain required.
No extra checked-in engine-output golden is treated as an oracle; no latency improvement is claimed.

## Future outcomes

These named work items are planning assignments with clone-visible acceptance descriptions, not
links into ignored specifications or declarations that the work has shipped. Order is legacy
classification, then identity/state/replay qualification. Execution of any later work still requires
its own accepted prerequisite and owner authorization.

### Legacy profile boundaries

M02-PR05: Classify and guard sparse, hold-last and store-backed conveniences so hosts cannot mistake
them for prevalidated complete frames; record compatibility consequences of any changed acceptance.

### Typed identities

M03-PR01: Separate source/authored model, executable, export, catalog, IO schema, build, profile,
generation and state-revision identities in a typed compatibility descriptor with evidence.

### Strict-bit evidence

M03-PR02: Resolve issue #250 with retained strict-bit evidence for the currently tolerance-qualified
paths or explicit limitations retaining tolerance qualification, across the claimed target matrix.

### Same-build state

M03-PR03: Qualify same-build state capture/restore and explicit portable versus target-bound domains,
including refusal evidence. Host authenticity, freshness and generation fencing remain outside OCE.

### Canonical replay

M03-PR04: Specify canonical frame/replay records and evidence for deterministic reproduction or
refusal across the supported codegen/target domains, rather than assuming traces are replay logs.

### Release compatibility

M03-PR05: Retain release-pair compatibility and refusal evidence, including migration decisions for
identity, profile and state revisions, before claiming release-to-release support.

## Evidence context

Primary-source observations, accessed 2026-09-05: the official
[OBC specification](https://obc.lbl.gov/specification/index.html) calls itself a working document
whose design may change. The [CXF introduction](https://obc.lbl.gov/specification/cxf.html#introduction)
describes configured logic and elementary, composite and extension blocks translated from Modelica;
it does not say all CXF is OCE's executable subset.
[Modelica 3.7 pre](https://specification.modelica.org/maint/3.7/operators-and-expressions.html#modelica:pre)
defines same-time iteration to a fixed point. The selected
[Buildings Pre source](https://github.com/lbl-srg/modelica-buildings/blob/a131864e4c4df22ebcd52bb8da439de0087ac365/Buildings/Controls/OBC/CDL/Logical/Pre.mo)
also describes event iteration until u equals pre(u). That Buildings SHA selects source bytes,
not a pin for the mutable OBC website. HostTick explicitly differs as delegated above.
TrueHoldWithReset provenance remains unresolved; there is no assertion of upstream absence.

Downstream inspection is supporting inventory, not qualification against the grounding SHA. Studio
was observed compiling/loading/exporting at an older OCE pin, without ticking or a store; its
serialized cap and Real/Integer/Boolean adapter boundary are not OCE promises. Library's older-pin
verifier used IO/tick/export with its own tolerance; sibling-path verification is not qualification
of this engine revision. Sim had no active OCE dependency and only a bridge scaffold; cxf-json had
no OCE dependency. No downstream pin, compatibility claim or adapter policy changes here.

Library, Studio, Edge and Sim retain their downstream roles. Runtime is an additive future
M05-PR03 consumer/host qualification candidate only; BOPTEST is Runtime host evidence, not OCE
equivalence. This adds no second evaluator, snapshot or replay stack, and no runtime, database,
network, driver, quality/staleness, lease, command or fallback policy to OCE.

Broad conformance qualification remains later work (M05), and platform/release qualification remains
later work (M06). No general flattening, direct Modelica loader, Python binding, FMI runtime,
database, scheduler, driver, universal panic freedom or equipment safety certification is added.

## Traceability check boundary

The standard-library [checker](../scripts/product_contract/check.py) checks this prescribed table,
numbering, closed statuses, actor/owner presence, nonempty limitations, test declarations or future
assignments, local regular clone-visible targets and anchors, and required integration pointers.
It rejects obligation keywords outside Requirement cells. It is not a universal Markdown parser,
Rust compiler, standards semantic parser, proof of test relevance, human approval or host compliance.
Named-test lexical existence is only a locator; running the existing tests and auditing relevance
remain separate requirements of the contribution process.

Only the explicitly enumerated pending contract/checker and facade-transition evidence paths can
be non-ignored untracked targets in a pre-commit checkout; arbitrary untracked targets still refuse.
Existing evidence targets remain tracked. A clean clone after commit supplies
the publication-facing link check; pre-commit GitHub source links still name the baseline revision
and cannot demonstrate remote availability of new files. There is no auto-bless mode or duplicate
requirement ledger. The [hostile tests](../scripts/product_contract/test_check.py) retain manually
written expected output and deterministic repetitions; the runnable gate remains
[the gate script](../.agents/gate.sh).

## Change record

- Revision 1, 2026-09-05: initial bounded executable-CXF/HostTick product contract and traceability
  map, grounded at f8918501586a1b99ed59a2c66ce79abcc58d0135. No runtime, signatures, catalog identities, state bytes or package
  selection changed. Future acceptance is not current implementation; normative revision approval
  and implementation acceptance remain owner/review decisions.
- Revision 2, 2026-09-05: owner approved the selected deferred-symbol removals and intentional
  Warning default, then explicitly authorized continuation with the corrected baseline fact that
  filtered inventory retains OcError::Load. Facade-owned implementation evidence promotes PC-028;
  PC-002, PC-003 and PC-017 now cite working/absent behavior rather than dead loaders. All IDs,
  package classifications, execution failure limits, state bytes and catalog identities remain.
  The migration account distinguishes source breaks from downstream qualification. Pending-path
  enumeration adds only the new migration and assertion-test evidence needed before commit; no
  status, assignment, clone-visibility, test-locator or obligation rule is relaxed. Independent
  delivery review and exact-candidate downstream qualification remain separate acceptance steps.
- Revision 3, 2026-09-05: owner approved additive catalog/schema contracts and immutable
  producer-stage receipts, preserving legacy signatures, order, runtime and state compatibility.
  PC-029 now maps to bounded facade implementation and compatibility tests; PC-021 distinguishes
  those descriptors from still-future build/generation qualification. Seven domain descriptions
  include parameter metadata limits and Warning-only collection from all block classes. Real
  downstream pins, host mapping/truncation policy and future admission/rollback work are unchanged.
- Revision 4, 2026-09-19: owner fixed the 8 MiB maximum/default, stricter-only per-engine
  configuration, typed count-only refusal at Import, and the in-memory versus Store compensation
  boundary. PC-004, PC-005 and PC-030 now map to bounded admission and complete owned-image
  failure/success tests. The additive facade baseline gains only admission items; the Store
  baseline, catalog/descriptor identity, state bytes, HostTick and dependencies remain unchanged.
  Existing raw-load callers intentionally lose above-cap acceptance; widening is unsupported.
  Pending-path enumeration adds the new admission source/tests only. No parser or transaction
  oracle is claimed; boundary expectations are hand-derived and existing determinism goldens
  remain. Exact-candidate review, hosted target checks and host qualification are separate steps.
- Revision 5, 2026-09-19: owner approved the normative complete-frame contract and PC-031 CURRENT
  promotion as contract-only implementation acceptance. Execution-maintainer requirements now have
  clone-visible determinant, refusal, identity, immutable-output and migration detail, plus passing
  legacy gap evidence. PC-032 through PC-035 remain future implementation/migration; no accepted
  M01 semantics change. Hand-derived bit goldens and snapshot comparisons expose retained staging,
  not future atomicity. Pending-path enumeration adds only the new contract and gap-test evidence;
  checker status/visibility rules stay intact. Runtime is additive future M05-PR03 host evidence,
  not a qualified consumer or OCE/BOPTEST equivalence claim. API, profile, state, snapshot, catalog,
  dependencies, publication and downstream pins are unchanged; review and hosted checks remain separate.
- Revision 6, 2026-09-19: owner approved the opaque prepared-frame plus minimal owned-definition
  shape, engine-local successful-load/dirty-rebuild invalidation with clean-resume retention, and
  deterministic first typed cause. Bounded implementation and behavioral evidence promote PC-032;
  the additive public baseline and its classification ledger are updated together. Preparation
  changes no run state, Store samples, wire formats or legacy acceptance. Boundary and target
  bounds are intersected without Integer-to-Real coercion. Input alias and enum/String limitations
  follow live source rather than a new identity vocabulary or executable profile. Pending-path
  enumeration adds only preparation source/test evidence; checker rules are unchanged. No commit,
  output-frame, release, publication, downstream adoption, host qualification or equipment-safety
  claim follows. PC-033 through PC-035 and hosted architecture evidence remain separate work.
- Revision 7, 2026-09-19: owner approved single-use prepared submissions, owned lexical executable
  boundary results, a never-reset Engine-lifetime accepted sequence, private incarnation binding
  and structural allocation budget without shadow state. Implementation and failure-preservation,
  hand-golden, independent G36 HostTick, warning, retention and allocation evidence promote PC-033
  only. PC-034/035 remain future; their stale source locators are corrected. The additive facade
  baseline/ledger/guards and migration guidance change together. Pending-path enumeration admits
  only the new bounded execution evidence; checker rules are not relaxed. Legacy execution,
  Store baseline, state/checkpoint/wire formats, catalog/profile identities, dependencies, features,
  MSRV and downstream pins are unchanged. Local timing observations are not a speed guarantee;
  hosted target checks and downstream qualification remain separate. No stable release,
  publication, Store transaction or equipment-safety claim is made.
- Revision 8, 2026-09-19: owner approved evaluation-core reuse only, unchanged legacy failure and
  lifecycle semantics, caller-selected existing warning sinks, and compatible post-write Err(Store)
  reconciliation without a generation receipt. Bounded parity, exactly-once controls and retained
  failure/projection evidence promote PC-034; PC-035 remains future. The pending-path list adds only
  the private shared-transition suite; checker rules remain unchanged. Public signatures, error
  variants, native sequence, state/checkpoint/wire formats, catalog/profile identities, dependencies,
  features, MSRV and downstream pins remain unchanged. Hosted architecture evidence, actual host
  qualification, stable release, publication, Store atomicity and equipment safety remain unclaimed.
