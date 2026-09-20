# Executable CXF and HostTick product contract

Document revision: 12
Grounding SHA: dbce73fb20ada4a3a91653bb7ad9b48fae7ee87d

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
| PC-005 | CURRENT | Product claimant | Execution maintainers | MUST distinguish ordinary failed-load preservation of the complete in-memory executable/run image from external Store transactionality. | Recover, save_model and handle resolution may have non-rollback effects; old external handle validity is not promised. Panic, process death, allocation failure and concurrent host effects are excluded. Flatten and semantics currently have no returned refusal path. | [Build and commit](../crates/oce-api/src/engine.rs); [Compensation](host-responsibilities.md#load-replacement-and-the-store-compensation-boundary) | test [pre_store_refusals_preserve_fresh_advanced_and_dirty_run_images](../crates/oce-api/src/tests/reload_tests.rs#L125); [store_refusals_preserve_fresh_advanced_and_dirty_run_images](../crates/oce-api/src/tests/reload_tests.rs#L171); [private_build_tail_refusals_preserve_the_run_image](../crates/oce-api/src/tests/reload_tests.rs#L245); [failed_handle_resolution_leaves_saved_model_and_allocated_store_handles](../crates/oce-api/src/tests/reload_tests.rs#L214) |
| PC-006 | CURRENT | Engine | Execution maintainers | MUST use finite nondecreasing model seconds and perform one emit pass followed by one stateful update pass per successful HostTick call, including equal timestamps. | Loaded-block time representability checks also apply. Pre emits entry memory then latches current input; no Modelica fixed-point iteration or convergence test. | [Profile](execution-profile.md#hosttick-v1); [Preflight](../crates/oce-api/src/frame.rs); [Core](../crates/oce-api/src/engine.rs); [Evaluator](../crates/oce-graph/src/tick.rs#L128-L195) | test [parameter_seed_is_first_call_output_and_equal_time_calls_advance_memory](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L100-L128); [nonconvergent_boolean_feedback_is_accepted_and_advances_per_call](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L132) |
| PC-007 | CURRENT | Engine | Execution maintainers | MUST require every executable boundary input exactly once in a complete typed frame instead of sparse input staging. | Quality, freshness and plausibility remain host policy; the point inventory is not the executable input schema. | [Preparation](../crates/oce-api/src/frame.rs); [Migration](complete-frame-contract.md#legacy-paths-and-migration) | test [incomplete_observations_never_stage_prefixes_or_reuse_prior_values](../crates/oce-api/tests/frame_refusals.rs#L51) |
| PC-008 | CURRENT | Engine | Execution maintainers | MUST refuse missing determinants without implicit prior-value, type-seed or Store-sample substitution. | Explicit host substitutions are not qualified by the engine. An executable with no inputs admits an empty frame. | [Determinants](complete-frame-contract.md#determinants-and-executable-boundary) | test [store_samples_neither_supply_missing_determinants_nor_overwrite_complete_values](../crates/oce-api/tests/frame_purity.rs#L64) |
| PC-009 | CURRENT | Engine | Execution maintainers | MUST preserve the complete execution image and fresh durable-restore window on preparation refusal, including any valid candidate prefix. | Ordinary returned refusal only; panic, allocation failure and host effects are excluded. | [Preparation](../crates/oce-api/src/frame.rs) | test [every_refusal_preserves_fresh_and_advanced_stateful_images_and_store](../crates/oce-api/tests/prepare_frame_preservation.rs#L22) |
| PC-010 | CURRENT | Engine | Execution maintainers | MUST prevalidate the whole complete submission before staging or evaluating it. | Host trace collection and cadence are separate; there is no built-in simulation restart or input callback. | [Refusal matrix](complete-frame-contract.md#prevalidation-and-refusal-matrix) | test [incomplete_observations_never_stage_prefixes_or_reuse_prior_values](../crates/oce-api/tests/frame_refusals.rs#L51) |
| PC-011 | CURRENT | Engine | Execution maintainers | MUST continue current execution state across successive accepted frames without an implicit horizon restart. | A fresh load, dirty resume or explicit compatible checkpoint restore has its own lifecycle semantics. | [Execution](../crates/oce-api/src/frame.rs) | test [sampled_delay_matches_hand_recurrence_and_retained_frames_survive_lifecycle_changes](../crates/oce-api/tests/execute_frame.rs#L134) |
| PC-012 | CURRENT | Product claimant | Execution maintainers | MUST distinguish per-frame refusal atomicity from a whole host-loop transaction: earlier accepted frames remain committed when a later candidate refuses. | There is no whole-horizon rollback or engine-owned trace on error. | [Outcome](complete-frame-contract.md#accepted-transition-and-immutable-outcome) | test [context_readiness_and_time_refusals_preserve_public_images_and_store_calls](../crates/oce-api/tests/execute_frame_preservation.rs#L29) |
| PC-013 | CURRENT | Engine | Execution maintainers | MUST refuse invalid first submissions without clearing the prior clock, reseeding words or staging a prefix. | Execution consults no Store sample, so there is no first-frame Store failure after restart. | [Execution](../crates/oce-api/src/frame.rs) | test [every_refusal_preserves_fresh_and_advanced_stateful_images_and_store](../crates/oce-api/tests/prepare_frame_preservation.rs#L22) |
| PC-014 | CURRENT | Engine | Execution maintainers | MUST execute frames without Store reads, writes, write helpers or post-write failure behavior. | Realtime and Store orchestration are deferred; a completed frame proves computation, not persistence or actuation. Load-time Store validation remains. | [Execution](../crates/oce-api/src/frame.rs); [Host delivery](host-responsibilities.md#native-complete-frames-and-the-host-delivery-boundary) | test [complete_corpus_frames_never_read_or_write_the_store](../crates/oce-api/tests/frame_purity.rs#L15) |
| PC-015 | CURRENT | Engine | Execution maintainers | MUST interpret halt as parameter-edit permission and dirty resume as block/run re-seeding, not equipment stop or live tuning. | Halt alone does not prevent execution. Resume does not recompute schedule, store projection or authored model identity; no transactional resume guarantee. Pending edits refuse state capture/restore. | [Parameter lifecycle](../crates/oce-api/src/params.rs#L167-L278); [State preconditions](../crates/oce-api/src/state.rs#L422-L438) | test [param_lifecycle_halt_set_resume_refolds](../crates/oce-api/src/tests/frozen_surface.rs#L213); [pending_parameter_edits_take_precedence_over_restore_readiness](../crates/oce-api/src/tests/state_tests.rs#L391) |
| PC-016 | CURRENT | Engine | Facade maintainers | MUST preserve completed-stage diagnostics through contextual load errors: diagnostics exposes terminal diagnostics and all_diagnostics prepends available prior diagnostics. | Empty terminal diagnostics does not mean success; severity is not list position. Message text is not a newly frozen schema. | [Diagnostic accessors](../crates/oce-api/src/error.rs) | test [warning_context_survives_each_later_store_failure](../crates/oce-api/src/engine_tests.rs#L36) |
| PC-017 | CURRENT | Product claimant | Facade maintainers | MUST describe AssertLevel as Warning-only with Default equal to Warning and retained warnings on every completed frame. | No escalation, interlock or instance-identity guarantee. Sources remain producer-supplied and emission order is deterministic. | [Severity and collector](../crates/oce-api/src/observations.rs) | test [default_severity_is_warning_not_an_unemitted_failure](../crates/oce-api/tests/frame_assertions.rs#L18); [boolean_assertions_repeat_warning_records_and_continue_bit_exactly](../crates/oce-api/tests/frame_assertions.rs#L84) |
| PC-018 | CURRENT | Product claimant | CXF maintainers | MUST distinguish partial successful export from complete exported-document identity and use content_id_complete to refuse warning-bearing exports before treating the tag as complete. | Survivor-cone export is not source recovery or whole-model identity. FNV-1a-128 over emitted bytes is noncryptographic, not authentication; authored model identity can stay unchanged as export changes. | [Round trip](cxf-round-trip.md#the-deferral-trap); [Complete tag](../crates/oce-api/src/export.rs#L33-L89) | test [warning_bearing_content_id_identifies_the_partial_document](../crates/oce-api/tests/export_cxf.rs#L124-L147); [content_id_tracks_exported_synthetic_document_while_model_id_stays_authored](../crates/oce-api/tests/export_cxf.rs#L97-L122) |
| PC-019 | CURRENT | Engine | Execution maintainers | MUST validate compatible process-local checkpoint restore before committing values, words and clock, permitting rewind without evaluating or calling the store. | Pending edits and invalid state refuse. The bit-atomic test covers its corrupted-state scenario; the no-store test covers capture and durable restore, while checkpoint restore's no-store claim is source-grounded. | [Checkpoint](../crates/oce-api/src/state.rs#L367-L393); [Prepare and commit](../crates/oce-api/src/state.rs#L471-L562) | test [checkpoint_refusal_is_bit_atomic](../crates/oce-api/src/tests/state_tests.rs#L445); [capture_and_restore_call_no_store_method](../crates/oce-api/src/tests/state_tests.rs#L430) |
| PC-020 | CURRENT | Engine | Execution maintainers | MUST limit durable restore to a successfully loaded compatible target before a mutation boundary, validate before commit and restore continuation without evaluation or store calls. | Startup-only differs from checkpoint rewind. Pending edits refuse first; snapshot bytes omit host epoch, backend history and equipment authority. Equal restored time is another HostTick transition. | [Durable restore](../crates/oce-api/src/state.rs#L395-L438); [Prepare and commit](../crates/oce-api/src/state.rs#L471-L562) | test [mutation_boundaries_close_the_durable_restore_window](../crates/oce-api/src/tests/state_tests.rs#L352); [capture_and_restore_call_no_store_method](../crates/oce-api/src/tests/state_tests.rs#L430); [snapshot_restores_next_pre_output_at_same_timestamp](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L164) |
| PC-021 | CURRENT | Product claimant | Facade maintainers | MUST distinguish authored/source model, executable, exported-document, catalog, IO schema, build, execution profile, generation and state-revision identity roles. | The current manifest checks executable compatibility, not a new build/generation authentication contract. Diagnostic model id is not the compatibility key. Facade descriptors version bounded metadata and diagnostic shapes separately; host build and generation admission remain future work. | [Manifest](../crates/oce-api/src/state.rs#L340-L365); [Compatibility](../crates/oce-api/src/state.rs#L471-L493); [Identity glossary](public-surface-contract.md#identity-glossary) | test [diagnostic_model_identity_is_not_an_execution_compatibility_key](../crates/oce-api/src/tests/state_tests.rs#L504); [content_id_tracks_exported_synthetic_document_while_model_id_stays_authored](../crates/oce-api/tests/export_cxf.rs#L97-L122) |
| PC-022 | CURRENT | Product claimant | Block semantics maintainers | MUST NOT claim upstream equivalence for the local two-input TrueHoldWithReset behavior. | Its u/clr test proves local names and clear behavior only. Upstream-equivalent existence and provenance remain unresolved, not proven absent; no catalog identity or behavior changes here. | [Local implementation](../crates/oce-blocks/src/logical_timing.rs#L440-L489) | test [true_hold_with_reset_names_match_its_behaviour](../crates/oce-blocks/src/port_names_tests.rs#L245-L292) |
| PC-023 | CURRENT | Product claimant | Block semantics maintainers | MUST NOT extrapolate fixture/profile evidence into arbitrary G36 support, Modelica event equivalence, blanket numerical exactness or universal panic freedom. | Pre is excluded from expected-green Modelica same-time differential claims; no concurrency, cancellation, deadlock or adapter-panic guarantee is added. | [Conformance boundary](execution-profile.md#conformance-boundary); [Evidence context](#evidence-context) | test [nonconvergent_boolean_feedback_is_accepted_and_advances_per_call](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L130-L158) |
| PC-024 | HOST-OBLIGATION | Host | Host integrator | MUST qualify input quality, freshness, missing-data reaction and plausibility before or instead of execution. | Complete typed values do not establish sensor validity, simultaneous sampling or actual host compliance. | [Host policy](host-responsibilities.md#the-engine-implements-no-fail-safe-policy-of-its-own) | test [store_samples_neither_supply_missing_determinants_nor_overwrite_complete_values](../crates/oce-api/tests/frame_purity.rs#L64) |
| PC-025 | HOST-OBLIGATION | Host | Host integrator | MUST implement NO_EVAL by not executing the engine and separately own safe-state outputs, equipment interlocks and actuation. | NO_EVAL is neither halt nor a fabricated zero frame. Engine tests do not qualify equipment protection or prove command delivery; a halted engine can still execute. | [Lifecycle boundary](host-responsibilities.md#lifecycle-names-are-not-equipment-controls); [Halt](../crates/oce-api/src/params.rs#L167-L176) | test [param_lifecycle_halt_set_resume_refolds](../crates/oce-api/src/tests/frozen_surface.rs#L210-L256) |
| PC-026 | HOST-OBLIGATION | Host | Host integrator | MUST own scheduling, model-time cadence, wall-clock mapping and external delivery-failure handling. | No scheduler, deadline, cancellation or equipment-stop guarantee; equal-time resubmission advances again. Boundary tests do not prove host policy. | [Host time](host-responsibilities.md#time-is-host-supplied) | test [equal_time_feedback_advances_once_and_refusal_consumes_no_position](../crates/oce-api/tests/execute_frame.rs#L94-L110) |
| PC-027 | HOST-OBLIGATION | Host | Host integrator | MUST own persistence, authentication, authorization, freshness, generation fencing and restored actuator ownership outside engine snapshot capture/restore. | Integrity and executable compatibility are not authenticity or permission to command. Typed PointStore samples are not the engine-byte persistence channel. Boundary tests do not qualify an adapter or host. | [Host state duties](host-responsibilities.md#persist-engine-state-outside-the-store-port) | test [capture_and_restore_call_no_store_method](../crates/oce-api/src/tests/state_tests.rs#L430) |
| PC-028 | CURRENT | Facade delivery | Facade maintainers | MUST remove or quarantine deferred and panic-only supported surfaces with coordinated compatibility evidence. | Selected facade names are removed; private quarantines and package boundaries are unchanged. Compiler controls and bounded source inspection are not downstream acceptance or universal panic freedom; exact-candidate consumer qualification remains separate. | [Current ruling](public-surface-contract.md#surface-ruling); [Migration and inventory](facade-migration.md#package-and-panic-inventory-boundary) | test [retired_facade_symbols_are_absent](../crates/oce-api/tests/public_surface_contract.rs#L507-L512); [filtered_inventory_refuses_without_store_calls_or_engine_mutation](../crates/oce-api/tests/public_storage_adapter.rs#L175-L227) |
| PC-029 | CURRENT | Facade delivery | Facade maintainers | MUST expose versioned facade catalog, diagnostics, IO, values, parameters, assertions and execution-profile contracts with compatibility tests. | Additive typed metadata and immutable producer receipts; opaque subjects, Warning-only runtime, no generic value codec, build stamp, admission bounds, rollback or new snapshot/profile selector. | [Versioned contracts](facade-contracts.md); [Adoption](facade-migration.md#additive-contract-adoption) | test [canonical_catalog_matches_packaged_bytes_and_repeats_exactly](../crates/oce-api/tests/catalog_contract.rs#L16-L34); [unification_evidence_survives_structural_refusal_at_its_actual_producer](../crates/oce-api/src/tests/diagnostic_receipts.rs#L201-L219); [descriptors_cover_every_domain_with_explicit_shapes_and_semantic_limits](../crates/oce-api/src/tests/contract_schemas.rs#L10-L28) |
| PC-030 | CURRENT | Admission delivery | CXF maintainers | MUST reject serialized inputs above the effective per-engine cap before parsing, Store calls or engine mutation and replace model-bound state/caches only on successful load. | Default and maximum are exactly 8 MiB. Configuration above the maximum refuses through a typed error. Receipt refusal stays Import; errors contain counts only. External persistence is not atomic; replacement coverage has the documented stage limitations. | [Admission](../crates/oce-api/src/admission.rs#L1-L41); [Replacement](#bounded-admission-and-replacement) | test [oversized_valid_document_refuses_without_parser_allocation_or_store_calls](../crates/oce-api/tests/cxf_admission.rs#L16-L32); [oversize_receipts_allocate_only_the_error_box_and_are_deterministic](../crates/oce-api/tests/cxf_admission.rs#L104-L138); [invalid_configuration_is_typed_and_never_silently_widens](../crates/oce-api/tests/cxf_admission.rs#L83-L101); [successful_reload_replaces_model_bound_caches_but_not_host_policy](../crates/oce-api/src/tests/reload_tests.rs#L278) |
| PC-031 | CURRENT | Frame delivery | Execution maintainers | MUST retain the normative complete generation-atomic typed input/output frame contract, including completeness, prevalidation, reload fencing, one HostTick transition, refusal preservation and immutable correlated outputs/diagnostics. | Contract ratification and bounded runtime evidence do not qualify hosts, persistence or actuators. Historical gap evidence is superseded by frame refusal controls. | [Frame contract](complete-frame-contract.md#status-and-authority); [Refusal matrix](complete-frame-contract.md#prevalidation-and-refusal-matrix); [Outcome](complete-frame-contract.md#accepted-transition-and-immutable-outcome) | test [incomplete_observations_never_stage_prefixes_or_reuse_prior_values](../crates/oce-api/tests/frame_refusals.rs#L51); [test_report_is_an_independent_byte_golden](../scripts/product_contract/test_check.py#L94-L100) |
| PC-032 | CURRENT | Frame delivery | Execution maintainers | MUST resolve and prevalidate complete typed frames, refusing unknown, duplicate, missing, stale-generation and unloaded submissions before mutation. | Preparation only; no commit or output frame. Internal incarnation preflight is not host freshness or authorization. No public input aliases exist; injected alias coverage tests logical uniqueness only. | [Preparation](../crates/oce-api/src/frame.rs); [Contract](complete-frame-contract.md#current-preparation-api) | test [every_refusal_preserves_fresh_and_advanced_stateful_images_and_store](../crates/oce-api/tests/prepare_frame_preservation.rs#L22-L155); [reload_and_cross_engine_identity_refuse_even_identical_model_bytes](../crates/oce-api/src/frame_tests.rs#L103-L125); [dirty_resume_invalidates_but_clean_resume_and_compatible_restore_retain_context](../crates/oce-api/src/frame_tests.rs#L128-L170); [canonical_plan_owns_values_and_repeats_bit_exactly_under_entry_permutations](../crates/oce-api/src/frame_tests.rs#L52-L79); [repeated_large_fixture_preparation_has_a_linear_capacity_and_allocation_census](../crates/oce-api/src/frame_tests.rs#L329) |
| PC-033 | CURRENT | Frame delivery | Execution maintainers | MUST commit one HostTick transition and one immutable output/diagnostic frame per accepted frame, preserving time, state, connector values, output generation and replay identity on ordinary refusal. | Native in-place engine transition only; sequence is Engine-lifetime correlation, not serialized replay/deployment identity. No persistence or actuator-delivery atomicity, panic recovery or cancellation guarantee. | [Execution](../crates/oce-api/src/frame.rs); [Contract](complete-frame-contract.md#current-execution-api) | test [arithmetic_commits_complete_values_without_store_and_retains_independent_results](../crates/oce-api/tests/execute_frame.rs#L41); [sampled_delay_matches_hand_recurrence_and_retained_frames_survive_lifecycle_changes](../crates/oce-api/tests/execute_frame.rs#L134); [context_readiness_and_time_refusals_preserve_public_images_and_store_calls](../crates/oce-api/tests/execute_frame_preservation.rs#L29); [each_preflight_refusal_preserves_every_execution_bit_and_sequence](../crates/oce-api/src/frame_commit_tests.rs#L65); [complete_frames_match_the_independent_hosttick_reference_and_repeat_bit_exactly](../crates/oce-api/tests/g36_cooling_only_controller.rs#L417); [allocation_cost_is_only_prepared_targets_boundary_results_and_emitted_warnings](../crates/oce-api/tests/frame_observations.rs#L55) |
| PC-034 | CURRENT | Frame delivery | Execution maintainers | MUST retain one shared infallible evaluation core, entered exactly once after complete-frame preflight and staging. | Host loops are not a second evaluator or whole-horizon transaction. Parity is limited to equivalent complete-frame schedules. | [Shared core](../crates/oce-api/src/engine.rs); [Migration](complete-frame-contract.md#legacy-paths-and-migration); [Acceptance](#shared-convenience-core) | test [accepted_frames_enter_the_shared_core_once_and_refusals_never_enter](../crates/oce-api/src/shared_transition_tests.rs#L55) |
| PC-035 | CURRENT | Frame delivery | Execution maintainers | MUST expose only preparation followed by consuming complete-frame execution, removing legacy execution profiles, raw output access and their types without aliases or compatibility bridges. | get_output and watch are latest-state non-receipt inspections. No Store write helper, realtime orchestration, durable receipt or downstream qualification is supplied. | [Frame-only contraction](#frame-only-facade); [Compiler controls](../scripts/facade_contract/check.py); [Facade](../crates/oce-api/src/lib.rs) | test [retired_facade_symbols_are_absent](../crates/oce-api/tests/public_surface_contract.rs#L508); [incomplete_reference_inputs_refuse_in_both_cadences](../crates/oce-conformance/tests/driver.rs#L217); [store_samples_neither_supply_missing_determinants_nor_overwrite_complete_values](../crates/oce-api/tests/frame_purity.rs#L64) |
| PC-036 | CURRENT | Identity delivery | Facade maintainers | MUST expose distinct catalog and complete-export identity types plus a closed, versioned compact descriptor of public catalog, IO/value/parameter revisions, fixed HostTick profile and OCE package version, with optional complete export identity. | Public-fact equality is not executable identity, unique-build qualification, generation, state-wire compatibility or authentication. No private execution/state identity is exposed. | [Typed identities](#typed-identities); [Descriptor](../crates/oce-api/src/compatibility.rs); [Contract](facade-contracts.md#closed-host-compatibility-descriptor) | test [canonical_public_facts_match_the_hand_assembled_golden_and_repeat](../crates/oce-api/tests/compatibility.rs#L14); [partial_exports_refuse_instead_of_becoming_absent_or_complete_content](../crates/oce-api/tests/compatibility.rs#L27); [every_field_changes_canonical_bytes_and_has_an_exact_symmetric_refusal](../crates/oce-api/src/compatibility_tests.rs#L6); [descriptor_capture_preserves_state_and_survives_report_and_engine_lifetimes](../crates/oce-api/tests/compatibility.rs#L147) |
| PC-037 | CURRENT | Evidence delivery | Block semantics maintainers | MUST retain and enforce exact comparison for the pinned 21-signal corpus on Linux x86_64/aarch64 in debug/release, with two native runs per cell and zero mismatches. | Pinned rustc 1.97.1/libm 0.2.16 only; macOS and other targets remain unqualified at the existing 1e-12 aligned band. No mathematical correctness, arbitrary-input, whole-executable or Sim qualification follows. | [Accepted native receipt](strict-bit-evidence.md#accepted-native-receipt); [Testing standard](../TESTING.md#the-four-pillars) | test [retained_native_linux_evidence_is_complete_exact_and_source_bound](../crates/oce-conformance/tests/strict_bits/matrix.rs#L247-L254); [every_corpus_sample_uses_exact_facade_comparison_and_rejects_mutations](../crates/oce-conformance/tests/strict_bits/controls.rs#L59-L112); [qualified_linux_signals_are_exact_and_other_targets_keep_the_aligned_band](../crates/oce-conformance/tests/strict_bits/controls.rs#L196-L235) |
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

Revision 8 implemented M02-PR04 as private evaluation-core reuse, not complete-frame preparation
reuse or universal mode equality. Its weaker convenience profiles and post-write reconciliation
account were historical migration context. Revision 9 removes those routes rather than introducing
a generation-tagged write failure or a compatibility bridge. The same private infallible evaluator
remains behind consuming frame execution, with instrumented emit/update and refusal controls.

## Frame-only facade

Revision 9 implements M02-PR05 as a greenfield contraction. Preparation followed by consuming
execution is the only public state-advancing execution path. Legacy execution methods, epoch
configuration, raw output access, and associated source/spec/report/trace types are removed. All
first-party consumers submit complete frames. Uniform and event-aligned conformance modes retain
their cadence and comparison semantics, but missing determinants refuse before comparison.

PC-035 is CURRENT for this bounded outcome. Compiler controls run against all supported feature
selections; absence sentinels prevent re-blessing retired surfaces. Store noninterference, refusal
preservation, hand-derived frame goldens and existing independent sequence references provide
behavioral evidence. get_output and watch are latest-state non-receipt inspections. Realtime and
Store orchestration remain deferred; execute_frame performs no write-back. Assertion and execution
descriptor revisions advance to 2 to remove stale profile claims; HostTick v1, catalog identity,
state codecs, accepted-frame sequence and admission/load compensation semantics remain unchanged.
No latency improvement, hosted cross-architecture result, host qualification or publication follows.

## Typed identities

Revision 10 implements the owner-bounded M03-PR01 public host contract only. Catalog and complete
export tags have distinct facade-owned types; an immutable revision-1 descriptor captures public
catalog content/schema, IO/value/parameter revisions, fixed HostTick compatibility, the exact
OCE Cargo package version and optional complete export content. Canonical bytes and typed
first-mismatch outcomes have hand-assembled goldens, repeated captures, per-field mutation controls,
compile-time category refusals and unchanged FNV oracle evidence. Warning-bearing export refuses
through the existing completeness check; absence is explicit and never a wildcard.

PC-036 is CURRENT only for those facts. Package version is not a unique source/build/compiler/target
identity, and IO revision is not a model's executable input schema. Executable fingerprints,
generation and state-wire identities remain private pending the separately authorized state/replay
work. No manifest/codec bytes, source normalization, signing/PKI, host qualification or cross-release
compatibility follows. Existing APIs/bytes, package closure and state/restore behavior remain intact.
The [receipt mapping](facade-migration.md#compatibility-receipt-adoption) preserves current consumers;
actual downstream pins and qualification are unchanged.

## Strict-bit evidence

The [accepted native receipt](strict-bit-evidence.md#accepted-native-receipt) promotes PC-037 only
for the 21 inventoried Linux signal cases: four architecture/codegen cells, two native runs per
cell, 161 samples per run and zero mismatches. The checked-in raw captures reconstruct the accepted
matrix digest. Their recorded synthetic merge checkout is preserved; current source/oracle/CXF
digests establish applicability without requiring a later delivery HEAD to equal that capture SHA.
The original macOS observation is not platform qualification. macOS-arm64 stays conservative until
M06-PR02, and all other unqualified targets retain the existing aligned band. Whole-executable
exactness inherits the least-qualified contributing path, target and input domain; these finite
cases alone do not establish it. Mathematical correctness, arbitrary-input and downstream policy
claims remain outside this evidence. Sim adoption remains M05-PR07.

## Future outcomes

These named work items are planning assignments with clone-visible acceptance descriptions, not
links into ignored specifications or declarations that the work has shipped. Execution identity/state/replay
qualification remains later work. Execution of any later work still requires
its own accepted prerequisite and owner authorization.

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
- Revision 9, 2026-09-19: owner authorized frame-only greenfield delivery and coordinated first-party
  migration, not aliases or guarantee tags. Implementation and compiler absence, Store-free,
  refusal, cadence, retained-warning, hand-golden and independent-reference evidence promote PC-035.
  The sparse/restart/post-write requirements are superseded in place without deleting their IDs.
  Assertion/execution descriptor revisions are 2; runtime HostTick, frame domains, catalog, state
  bytes, dependencies and features remain unchanged. Public baseline and classification ledger
  record the intended contraction. Checker sentinels reject stale active guidance and substituted
  contraction evidence without claiming semantic proof. Sibling consumers still need migration;
  downstream qualification, durable identity/replay, publication and hosted gates are not claimed.
- Revision 10, 2026-09-20: owner limited identity delivery to the closed public host compatibility
  contract. Bounded descriptor/tag implementation and golden, mutation, completeness, lifecycle and
  compile-time evidence promote PC-036 only in that narrowed scope. Public baseline/classification
  and migration documentation change together; package version is explicitly not unique build
  identity. Private executable/generation/state-wire work stays with later state/replay outcomes.
  The traceability checker advances its accepted revision and enumerates only the new evidence
  paths; prior frame-contraction sentinels remain. Catalog/export algorithms, old descriptor bytes,
  snapshot/restore, frame/admission semantics, dependencies, features and downstream pins are
  unchanged. Independent delivery review, hosted architecture checks and host qualification remain
  separate; no stable release, signing authority, replay or state-wire support is claimed.
- Revision 11, 2026-09-20: owner authorized the bounded 21-signal Linux strict-bit matrix and raw
  provenance delivery. The checked-in observation is local macOS only; native Linux acceptance
  remains pending, so PC-037 is not promoted. Existing aligned bands remain the conservative
  platform policy; Linux candidates fail closed on disagreement. Runtime formulas, public APIs,
  state/restore, dependencies, toolchain and downstream policies are unchanged. The checker revision
  and pending documentation path advance mechanically without weakening earlier sentinels.
- Revision 12, 2026-09-20: accepted native run 35492290613 supplies the retained four-cell,
  two-run, 21-signal zero-mismatch Linux corpus result. PC-037 is CURRENT only in that bounded
  scope. Raw bits and synthetic merge provenance remain unchanged and digest-checked; no final-HEAD
  equality or source-digest exception is introduced. Existing Linux exact wiring and all Tier-A
  goldens remain unchanged. macOS/other targets retain the 1e-12 aligned band, with macOS-arm64
  unqualified until M06-PR02. No arbitrary-input, mathematical, whole-executable or Sim qualification,
  runtime/API/state change, dependency change or publication follows.
