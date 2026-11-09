# Executable CXF and HostTick product contract

Document revision: 24
Grounding SHA: e81480b02271456719d55cbe1e5090b0dea6d63c

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
| Facade maintainers | [Frame-only facade](#frame-only-facade), the facade rustdoc, the [compile-time signature guards](../crates/oce-api/src/guards.rs) and the [identity glossary](facade-contracts.md#identity-glossary). No enumerated surface baseline exists before 1.0. |
| Release maintainers | [Publication rules](ci-and-the-gate.md#publishing), enforced from each crate manifest by the [package validator](ci-and-the-gate.md#the-package-validator), including that oce-api declares no features. |
| CXF maintainers | [Composite subset](cxf-composite-subset.md#active-nodes) and [round-trip contract](cxf-round-trip.md#the-rt-2-contract). |
| Execution maintainers | [Execution profile](execution-profile.md#hosttick-v1), [complete-frame contract](complete-frame-contract.md#status-and-authority), load/execute/state implementation and focused tests below. |
| Block semantics maintainers | Local block behavior and bounded [conformance boundary](execution-profile.md#conformance-boundary); upstream provenance is a separate evidence question. |
| Host integrator | [Host responsibilities](host-responsibilities.md); qualification of the actual consuming application, adapter and equipment. |

These are accountable roles, not claims of named-person assignments. Conflicts go to the affected
domain owner with source/test evidence before a revision is accepted. Existing goldens, guards
and the [authority index](authority-claims.md) retain their separate checks. This is not a
signature baseline or generated authority projection.

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
| PC-002 | CURRENT | Product claimant | Release maintainers | MUST keep oce-api the host facade, refuse retired facade names at compile time, and keep every workspace crate either private or registry-ready with no private crate in a publishable closure. | Nothing is published; pre-1.0 makes no SemVer promise. No support role beyond the facade is enforced: which other crates a host may depend on is a review decision, not a checked classification. | [Packages](ci-and-the-gate.md#the-package-validator); [Facade](#frame-only-facade); [CXF ingest](../crates/oce-api/src/engine.rs#L254-L294) | test [test_publish_and_private_sets_follow_the_manifests](../scripts/package_policy/test_validate.py#L189); [test_manifest_publish_flag_flip_moves_the_member](../scripts/package_policy/test_validate.py#L195); [valid_minimal_loop_loads_clean](../crates/oce-api/tests/conformance.rs#L78-L88); [test_reintroduced_symbol_cannot_pass_by_compiling](../scripts/facade_contract/test_check.py#L25) |
| PC-003 | CURRENT | Product claimant | CXF maintainers | MUST constrain executable CXF claims to the closed, bounded, already-specialized graph profile, including the documented bounded composite lowering. | Not general Modelica flattening, source recovery, every CXF construct, or promotion of every accepted syntax to stable support. Semantic and traversal bounds do not bound serialized bytes. | [Load pipeline](../crates/oce-api/src/engine.rs#L254-L294); [Subset](cxf-composite-subset.md#active-nodes); [Removed loaders](facade-migration.md#removed-names-and-working-alternatives) | test [composite_nesting_accepts_the_limit_and_rejects_one_past](../crates/oce-cxf/tests/ingest_totality.rs#L414-L429); [boundary_hops_accept_the_limit_and_reject_the_attempted_next_hop](../crates/oce-cxf/tests/ingest_totality.rs#L465-L486); [test_reintroduced_symbol_cannot_pass_by_compiling](../scripts/facade_contract/test_check.py#L25) |
| PC-004 | HOST-OBLIGATION | Host | Host integrator | MUST cap serialized CXF transport/buffering and apply isolation appropriate to the trust boundary, outside any process actively commanding equipment for untrusted programs. | Facade admission defaults to 8 MiB and can only be configured at or below that maximum; a byte cap does not bound memory, CPU, expansion or qualify a host. The low-level parser is not the bounded facade. | [Admission](../crates/oce-api/src/admission.rs#L1-L47); [Untrusted input](host-responsibilities.md#treat-untrusted-cxf-as-untrusted-input) | test [inclusive_default_and_stricter_boundaries_preserve_legacy_acceptance](../crates/oce-api/tests/cxf_admission.rs#L38-L74) |
| PC-005 | CURRENT | Product claimant | Execution maintainers | MUST preserve the complete in-memory executable/run image on ordinary failed load, and leave no external effect: load calls no storage port. | Persisting the returned LoadedModel is a separate host operation whose ordering, transaction and compensation the host owns. Panic, process death, allocation failure and concurrent host effects are excluded. Semantics currently has no returned refusal path. | [Build and commit](../crates/oce-api/src/engine.rs); [No external effect](host-responsibilities.md#load-replacement-leaves-no-external-effect) | test [load_refusals_preserve_fresh_advanced_and_dirty_run_images](../crates/oce-api/src/tests/reload_tests.rs#L127); [private_build_tail_refusals_preserve_the_run_image](../crates/oce-api/src/tests/reload_tests.rs#L172) |
| PC-006 | CURRENT | Engine | Execution maintainers | MUST use finite nondecreasing model seconds and perform one emit pass followed by one stateful update pass per successful HostTick call, including equal timestamps. | Loaded-block time representability checks also apply. Pre emits entry memory then latches current input; no Modelica fixed-point iteration or convergence test. | [Profile](execution-profile.md#hosttick-v1); [Preflight](../crates/oce-api/src/frame.rs); [Frame driver](../crates/oce-graph/src/frame.rs); [Evaluator](../crates/oce-graph/src/tick.rs#L128-L195) | test [parameter_seed_is_first_call_output_and_equal_time_calls_advance_memory](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L100-L128); [nonconvergent_boolean_feedback_is_accepted_and_advances_per_call](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L132) |
| PC-007 | CURRENT | Engine | Execution maintainers | MUST require every executable boundary input exactly once in a complete typed frame instead of sparse input staging. | Quality, freshness and plausibility remain host policy; the point inventory is not the executable input schema. | [Preparation](../crates/oce-api/src/frame.rs); [Migration](complete-frame-contract.md#legacy-paths-and-migration) | test [incomplete_observations_never_stage_prefixes_or_reuse_prior_values](../crates/oce-api/tests/frame_refusals.rs#L51) |
| PC-008 | CURRENT | Engine | Execution maintainers | MUST refuse missing determinants without implicit prior-value, type-seed or stored-sample substitution. | Explicit host substitutions are not qualified by the engine. An executable with no inputs admits an empty frame. | [Determinants](complete-frame-contract.md#determinants-and-executable-boundary) | test [a_missing_determinant_refuses_and_never_reuses_a_prior_value](../crates/oce-api/tests/frame_purity.rs#L108) |
| PC-009 | CURRENT | Engine | Execution maintainers | MUST preserve the complete execution image and fresh durable-restore window on preparation refusal, including any valid candidate prefix. | Ordinary returned refusal only; panic, allocation failure and host effects are excluded. | [Preparation](../crates/oce-api/src/frame.rs) | test [every_refusal_preserves_fresh_and_advanced_stateful_images](../crates/oce-api/tests/prepare_frame_preservation.rs#L17) |
| PC-010 | CURRENT | Engine | Execution maintainers | MUST prevalidate the whole complete submission before staging or evaluating it. | Host trace collection and cadence are separate; there is no built-in simulation restart or input callback. | [Refusal matrix](complete-frame-contract.md#prevalidation-and-refusal-matrix) | test [incomplete_observations_never_stage_prefixes_or_reuse_prior_values](../crates/oce-api/tests/frame_refusals.rs#L51) |
| PC-011 | CURRENT | Engine | Execution maintainers | MUST continue current execution state across successive accepted frames without an implicit horizon restart. | A fresh load, dirty resume or explicit compatible checkpoint restore has its own lifecycle semantics. | [Execution](../crates/oce-api/src/frame.rs) | test [sampled_delay_matches_hand_recurrence_and_retained_frames_survive_lifecycle_changes](../crates/oce-api/tests/execute_frame.rs#L128) |
| PC-012 | CURRENT | Product claimant | Execution maintainers | MUST distinguish per-frame refusal atomicity from a whole host-loop transaction: earlier accepted frames remain committed when a later candidate refuses. | There is no whole-horizon rollback or engine-owned trace on error. | [Outcome](complete-frame-contract.md#accepted-transition-and-immutable-outcome) | test [context_readiness_and_time_refusals_preserve_public_images](../crates/oce-api/tests/execute_frame_preservation.rs#L25) |
| PC-013 | CURRENT | Engine | Execution maintainers | MUST refuse invalid first submissions without clearing the prior clock, reseeding words or staging a prefix. | Execution consults no storage port, so there is no first-frame storage failure after restart. | [Execution](../crates/oce-api/src/frame.rs) | test [every_refusal_preserves_fresh_and_advanced_stateful_images](../crates/oce-api/tests/prepare_frame_preservation.rs#L17) |
| PC-014 | CURRENT | Engine | Execution maintainers | MUST execute frames without storage reads, writes, write helpers or post-write failure behavior. | Realtime and storage orchestration are deferred; a completed frame proves computation, not persistence or actuation. The facade has no storage dependency at all, so load performs no storage validation either. | [Execution](../crates/oce-api/src/frame.rs); [Host delivery](host-responsibilities.md#native-complete-frames-and-the-host-delivery-boundary); [Facade storage isolation, no-DB pass 4](../.github/scripts/check-default-no-db.sh#L18-L23); [Pass 4 edge check](../.github/scripts/check-default-no-db.sh#L200-L225) | test [complete_corpus_frames_are_a_function_of_the_bytes_and_the_frames_alone](../crates/oce-api/tests/frame_purity.rs#L75); [the_facade_manifest_declares_no_storage_dependency](../crates/oce-api/tests/frame_purity.rs#L164); [no_storage_crate_is_reachable_from_the_facade_in_the_locked_graph](../crates/oce-api/tests/frame_purity.rs#L240); [no_facade_source_names_a_storage_crate_symbol](../crates/oce-api/tests/frame_purity.rs#L278) |
| PC-015 | CURRENT | Engine | Execution maintainers | MUST interpret halt as parameter-edit permission and dirty resume as block/run re-seeding, not equipment stop or live tuning. | Halt alone does not prevent execution. Resume does not recompute schedule, resolved-model projection or authored model identity; no transactional resume guarantee. Pending edits refuse state capture/restore. | [Parameter lifecycle](../crates/oce-api/src/params.rs#L166-L289); [State preconditions](../crates/oce-api/src/state.rs#L421-L437) | test [param_lifecycle_halt_set_resume_refolds](../crates/oce-api/src/tests/frozen_surface.rs#L213); [pending_parameter_edits_take_precedence_over_restore_readiness](../crates/oce-api/src/tests/state_tests.rs#L387) |
| PC-016 | CURRENT | Engine | Facade maintainers | MUST preserve completed-stage diagnostics through contextual load errors: diagnostics exposes terminal diagnostics and all_diagnostics prepends available prior diagnostics. | Empty terminal diagnostics does not mean success; severity is not list position. Message text is not a newly frozen schema. | [Diagnostic accessors](../crates/oce-api/src/error.rs) | test [resolver_warning_survives_a_later_build_failure](../crates/oce-api/src/tests/diagnostics_access.rs#L198) |
| PC-017 | CURRENT | Product claimant | Facade maintainers | MUST describe AssertLevel as Warning-only with Default equal to Warning and retained warnings on every completed frame. | No escalation, interlock or instance-identity guarantee. Sources remain producer-supplied and emission order is deterministic. | [Severity and collector](../crates/oce-api/src/observations.rs) | test [default_severity_is_warning_not_an_unemitted_failure](../crates/oce-api/tests/frame_assertions.rs#L18); [boolean_assertions_repeat_warning_records_and_continue_bit_exactly](../crates/oce-api/tests/frame_assertions.rs#L84) |
| PC-018 | CURRENT | Product claimant | CXF maintainers | MUST mint one canonical content id over the loaded graph (SHA-256, `oce:canon1:`) only through `Engine::content_id`, invariant under re-serialization, reordering and instance namespace and moved by any parameter bit, name, wiring, class or boundary I/O change, while `ExportReport::complete` still distinguishes a partial export. | The id is defined for every loaded model, partial export or not, and refuses while parameter edits are pending. Equal ids promise equal outputs, per-block state and refusals for one engine build, not byte-identical blobs, point inventories or interchangeable snapshots; internal connector IRIs and attributes, `SemanticPoint` records and authored order are outside it. Not a signature or build identity; a class signature change in a new build moves it. Survivor-cone export is not source recovery. | [Design](design/content-id.md); [Round trip](cxf-round-trip.md#export-completeness-and-the-canonical-content-id); [Canonical id](../crates/oce-api/src/content_id/mod.rs#L51-L93); [Completeness](../crates/oce-api/src/export.rs#L20-L49) | test [a_warning_bearing_export_is_incomplete_while_the_content_id_stays_defined](../crates/oce-api/tests/export_cxf.rs#L152-L179); [content_id_tracks_applied_parameter_edits_while_model_id_stays_authored](../crates/oce-api/tests/export_cxf.rs#L110-L149); [content_id_is_independent_of_authored_graph_order](../crates/oce-api/tests/export_cxf.rs#L226-L252); [every_order_spelling_and_namespace_rewrite_keeps_the_id_of_every_document](../crates/oce-api/tests/content_id_permutations.rs#L55-L71); [every_name_wiring_class_and_boundary_change_moves_the_id](../crates/oce-api/tests/content_id_changes.rs#L265-L408); [equal_ids_run_the_frame_corpus_to_identical_outputs_state_and_refusals](../crates/oce-api/tests/content_id_behaviour.rs#L137-L165) |
| PC-019 | CURRENT | Engine | Execution maintainers | MUST validate compatible process-local checkpoint restore before committing values, words and clock, permitting rewind without evaluating or calling a storage port. | Pending edits and invalid state refuse. Process-local rewind has no durable format or host authority. | [Checkpoint and prepare/commit](../crates/oce-api/src/state.rs); [State contract](state-compatibility.md#capture-parse-and-restore) | test [checkpoint_refusal_is_bit_atomic](../crates/oce-api/src/tests/state_tests.rs#L426); [every_manifest_field_refuses_deterministically_without_mutating_the_engine](../crates/oce-api/src/tests/state_manifest_refusal_tests.rs#L276) |
| PC-020 | CURRENT | Engine | Execution maintainers | MUST limit durable restore to a successfully loaded compatible target before a mutation boundary, validate before commit and restore continuation without evaluation or storage calls. | Startup-only differs from checkpoint rewind. Pending edits refuse first; snapshot bytes omit host epoch, backend history and equipment authority. Equal restored time is another HostTick transition. | [Durable restore and prepare/commit](../crates/oce-api/src/state.rs); [State contract](state-compatibility.md#capture-parse-and-restore) | test [mutation_boundaries_close_the_durable_restore_window](../crates/oce-api/src/tests/state_tests.rs#L348); [snapshot_restores_next_pre_output_at_same_timestamp](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L164) |
| PC-021 | CURRENT | Product claimant | Facade maintainers | MUST distinguish authored/source model, canonical content, executable, exported-document, catalog, IO schema, build, execution profile, generation and state-revision identity roles. | The manifest checks executable compatibility, not build/generation authentication. Diagnostic model id is not the compatibility key, and the canonical content id (the loaded graph by name) is neither the authored model id, executable compatibility, the export bytes nor a build identity. Facade descriptors version bounded metadata separately; the authenticated host envelope is a precondition, not an OCE build token. | [Manifest and compatibility](../crates/oce-api/src/state.rs); [Identity glossary](facade-contracts.md#identity-glossary); [State authority](state-compatibility.md#authority-and-scope) | test [diagnostic_model_identity_is_not_an_execution_compatibility_key](../crates/oce-api/src/tests/state_tests.rs#L485); [content_id_tracks_applied_parameter_edits_while_model_id_stays_authored](../crates/oce-api/tests/export_cxf.rs#L110-L149) |
| PC-022 | CURRENT | Product claimant | Block semantics maintainers | MUST NOT claim upstream equivalence for the local two-input TrueHoldWithReset behavior. | Its u/clr test proves local names and clear behavior only. Upstream-equivalent existence and provenance remain unresolved, not proven absent; no catalog identity or behavior changes here. | [Local implementation](../crates/oce-blocks/src/logical_timing.rs#L440-L489) | test [true_hold_with_reset_names_match_its_behaviour](../crates/oce-blocks/src/port_names_tests.rs#L245-L292) |
| PC-023 | CURRENT | Product claimant | Block semantics maintainers | MUST NOT extrapolate fixture/profile evidence into arbitrary G36 support, Modelica event equivalence, blanket numerical exactness or universal panic freedom. | Pre is excluded from expected-green Modelica same-time differential claims; no concurrency, cancellation, deadlock or adapter-panic guarantee is added. | [Conformance boundary](execution-profile.md#conformance-boundary); [Evidence context](#evidence-context) | test [nonconvergent_boolean_feedback_is_accepted_and_advances_per_call](../crates/oce-api/src/tests/pre_execution_profile_tests.rs#L130-L158) |
| PC-024 | HOST-OBLIGATION | Host | Host integrator | MUST qualify input quality, freshness, missing-data reaction and plausibility before or instead of execution. | Complete typed values do not establish sensor validity, simultaneous sampling or actual host compliance. | [Host policy](host-responsibilities.md#the-engine-implements-no-fail-safe-policy-of-its-own) | test [a_missing_determinant_refuses_and_never_reuses_a_prior_value](../crates/oce-api/tests/frame_purity.rs#L108) |
| PC-025 | HOST-OBLIGATION | Host | Host integrator | MUST implement NO_EVAL by not executing the engine and separately own safe-state outputs, equipment interlocks and actuation. | NO_EVAL is neither halt nor a fabricated zero frame. Engine tests do not qualify equipment protection or prove command delivery; a halted engine can still execute. | [Lifecycle boundary](host-responsibilities.md#lifecycle-names-are-not-equipment-controls); [Halt](../crates/oce-api/src/params.rs#L166-L175) | test [param_lifecycle_halt_set_resume_refolds](../crates/oce-api/src/tests/frozen_surface.rs#L210-L256) |
| PC-026 | HOST-OBLIGATION | Host | Host integrator | MUST own scheduling, model-time cadence, wall-clock mapping and external delivery-failure handling. | No scheduler, deadline, cancellation or equipment-stop guarantee; equal-time resubmission advances again. Boundary tests do not prove host policy. | [Host time](host-responsibilities.md#time-is-host-supplied) | test [equal_time_feedback_advances_once_and_refusal_consumes_no_position](../crates/oce-api/tests/execute_frame.rs#L88-L104) |
| PC-027 | HOST-OBLIGATION | Host | Host integrator | MUST authenticate an envelope binding exact snapshot bytes to approved build/deployment qualification, freshness and generation before from_bytes, refusing incompatible host combinations, and own persistence, authorization, wall-clock mapping, external point/history state and restored actuator fencing. | OCE carries no build token. Integrity and executable compatibility are not authenticity or permission to command. The storage port is not the engine-byte channel; boundary tests do not qualify a host. | [Host state duties](host-responsibilities.md#persist-engine-state-outside-the-engine); [State authority](state-compatibility.md#authority-and-scope) | test [absent_or_unauthenticated_envelopes_never_reach_the_decoder](../crates/oce-api/tests/release_fallback.rs#L188) |
| PC-028 | CURRENT | Facade delivery | Facade maintainers | MUST remove or quarantine deferred and panic-only supported surfaces with coordinated compatibility evidence. | Selected facade names are removed; private quarantines and package boundaries are unchanged. Compiler controls and bounded source inspection are not downstream acceptance or universal panic freedom; exact-candidate consumer qualification remains separate. | [Fulfilled contraction](#fulfilled-facade-contraction); [Migration and inventory](facade-migration.md#package-and-panic-inventory-boundary) | test [test_reintroduced_symbol_cannot_pass_by_compiling](../scripts/facade_contract/test_check.py#L25); [device_filtered_inventory_preserves_the_load_typed_refusal](../crates/oce-api/src/tests/frozen_surface.rs#L201-L208); [device_filtered_inventory_refuses_without_engine_mutation](../crates/oce-api/tests/device_filtered_inventory.rs#L8) |
| PC-029 | CURRENT | Facade delivery | Facade maintainers | MUST expose versioned facade catalog, diagnostics, IO, values, parameters, assertions and execution-profile contracts with compatibility tests. | Additive typed metadata and immutable producer receipts; opaque subjects, Warning-only runtime, no generic value codec, build stamp, admission bounds, rollback or new snapshot/profile selector. | [Versioned contracts](facade-contracts.md); [Adoption](facade-migration.md#additive-contract-adoption) | test [canonical_catalog_matches_packaged_bytes_and_repeats_exactly](../crates/oce-api/tests/catalog_contract.rs#L16-L34); [unification_evidence_survives_structural_refusal_at_its_actual_producer](../crates/oce-api/src/tests/diagnostic_receipts.rs#L204-L222); [descriptors_cover_every_domain_with_explicit_shapes_and_semantic_limits](../crates/oce-api/src/tests/contract_schemas.rs#L10-L28) |
| PC-030 | CURRENT | Admission delivery | CXF maintainers | MUST reject serialized inputs above the effective per-engine cap before parsing or engine mutation and replace model-bound state/caches only on successful load. | Default and maximum are exactly 8 MiB. Configuration above the maximum refuses through a typed error. Receipt refusal stays Import; errors contain counts only. Host persistence is outside the engine; replacement coverage has the documented stage limitations. | [Admission](../crates/oce-api/src/admission.rs#L1-L47); [Replacement](#bounded-admission-and-replacement) | test [oversized_valid_document_refuses_without_parser_allocation](../crates/oce-api/tests/cxf_admission.rs#L13-L24); [oversize_receipts_allocate_only_the_error_box_and_are_deterministic](../crates/oce-api/tests/cxf_admission.rs#L96-L127); [invalid_configuration_is_typed_and_never_silently_widens](../crates/oce-api/tests/cxf_admission.rs#L77-L93); [successful_reload_replaces_model_bound_caches_but_not_host_policy](../crates/oce-api/src/tests/reload_tests.rs#L206) |
| PC-031 | CURRENT | Frame delivery | Execution maintainers | MUST retain the normative complete generation-atomic typed input/output frame contract, including completeness, prevalidation, reload fencing, one HostTick transition, refusal preservation and immutable correlated outputs/diagnostics. | Contract ratification and bounded runtime evidence do not qualify hosts, persistence or actuators. Historical gap evidence is superseded by frame refusal controls. | [Frame contract](complete-frame-contract.md#status-and-authority); [Refusal matrix](complete-frame-contract.md#prevalidation-and-refusal-matrix); [Outcome](complete-frame-contract.md#accepted-transition-and-immutable-outcome) | test [incomplete_observations_never_stage_prefixes_or_reuse_prior_values](../crates/oce-api/tests/frame_refusals.rs#L51); [test_report_is_an_independent_byte_golden](../scripts/product_contract/test_check.py#L94-L100) |
| PC-032 | CURRENT | Frame delivery | Execution maintainers | MUST resolve and prevalidate complete typed frames, refusing unknown, duplicate, missing, stale-generation and unloaded submissions before mutation. | Preparation only; no commit or output frame. Internal incarnation preflight is not host freshness or authorization. No public input aliases exist; injected alias coverage tests logical uniqueness only. | [Preparation](../crates/oce-api/src/frame.rs); [Contract](complete-frame-contract.md#current-preparation-api) | test [every_refusal_preserves_fresh_and_advanced_stateful_images](../crates/oce-api/tests/prepare_frame_preservation.rs#L17-L151); [reload_and_cross_engine_identity_refuse_even_identical_model_bytes](../crates/oce-api/src/frame_tests.rs#L103-L125); [dirty_resume_invalidates_but_clean_resume_and_compatible_restore_retain_context](../crates/oce-api/src/frame_tests.rs#L128-L170); [canonical_plan_owns_values_and_repeats_bit_exactly_under_entry_permutations](../crates/oce-api/src/frame_tests.rs#L52-L79); [repeated_large_fixture_preparation_has_a_linear_capacity_and_allocation_census](../crates/oce-api/src/frame_tests.rs#L329) |
| PC-033 | CURRENT | Frame delivery | Execution maintainers | MUST commit one HostTick transition and one immutable output/diagnostic frame per accepted frame, preserving time, state, connector values, output generation and replay identity on ordinary refusal. | Native in-place engine transition only; sequence is Engine-lifetime correlation, not serialized replay/deployment identity. No persistence or actuator-delivery atomicity, panic recovery or cancellation guarantee. | [Execution](../crates/oce-api/src/frame.rs); [Contract](complete-frame-contract.md#current-execution-api) | test [arithmetic_commits_complete_values_and_retains_independent_results](../crates/oce-api/tests/execute_frame.rs#L38); [sampled_delay_matches_hand_recurrence_and_retained_frames_survive_lifecycle_changes](../crates/oce-api/tests/execute_frame.rs#L128); [context_readiness_and_time_refusals_preserve_public_images](../crates/oce-api/tests/execute_frame_preservation.rs#L25); [each_preflight_refusal_preserves_every_execution_bit_and_sequence](../crates/oce-api/src/frame_commit_tests.rs#L61); [complete_frames_match_the_independent_hosttick_reference_and_repeat_bit_exactly](../crates/oce-api/tests/g36_cooling_only_controller.rs#L417); [allocation_cost_is_only_prepared_targets_boundary_results_and_emitted_warnings](../crates/oce-api/tests/frame_observations.rs#L55) |
| PC-034 | CURRENT | Frame delivery | Execution maintainers | MUST retain one shared infallible evaluation core, entered exactly once after complete-frame preflight and staging. | Host loops are not a second evaluator or whole-horizon transaction. Parity is limited to equivalent complete-frame schedules. | [Shared core](../crates/oce-graph/src/frame.rs); [Migration](complete-frame-contract.md#legacy-paths-and-migration); [Acceptance](#shared-convenience-core) | test [accepted_frames_enter_the_shared_core_once_and_refusals_never_enter](../crates/oce-api/src/shared_transition_tests.rs#L55) |
| PC-035 | CURRENT | Frame delivery | Execution maintainers | MUST expose only preparation followed by consuming complete-frame execution, and its slot-indexed twin over the same core driver (issue #444), removing legacy execution profiles, raw output access and their types without aliases or compatibility bridges. | get_output and watch are latest-state non-receipt inspections. No storage write helper, realtime orchestration, durable receipt or downstream qualification is supplied. | [Frame-only contraction](#frame-only-facade); [Compiler controls](../scripts/facade_contract/check.py); [Facade](../crates/oce-api/src/lib.rs) | test [test_reintroduced_symbol_cannot_pass_by_compiling](../scripts/facade_contract/test_check.py#L25); [incomplete_reference_inputs_refuse_in_both_cadences](../crates/oce-conformance/tests/driver.rs#L217); [a_missing_determinant_refuses_and_never_reuses_a_prior_value](../crates/oce-api/tests/frame_purity.rs#L108) |
| PC-036 | CURRENT | Identity delivery | Facade maintainers | MUST expose distinct catalog and canonical content identity types plus a closed, versioned compact descriptor of public catalog, IO/value/parameter revisions, fixed HostTick profile and OCE package version, with an optional canonical content id. | Public-fact equality is not executable identity, unique-build qualification, generation, state-wire compatibility or authentication. No private execution/state identity is exposed. | [Typed identities](#typed-identities); [Descriptor](../crates/oce-api/src/compatibility.rs); [Contract](facade-contracts.md#closed-host-compatibility-descriptor) | test [canonical_public_facts_match_the_hand_assembled_golden_and_repeat](../crates/oce-api/tests/compatibility.rs#L14); [a_partial_export_does_not_withhold_the_content_line](../crates/oce-api/tests/compatibility.rs#L24); [every_field_changes_canonical_bytes_and_has_an_exact_symmetric_refusal](../crates/oce-api/src/compatibility_tests.rs#L6); [descriptor_capture_preserves_state_and_survives_id_and_engine_lifetimes](../crates/oce-api/tests/compatibility.rs#L140) |
| PC-037 | CURRENT | Evidence delivery | Block semantics maintainers | MUST enforce, on every pull request, exact comparison for the pinned 21-signal corpus on native Linux x86_64/aarch64 runners in debug/release, with two native runs per cell, byte-identical repeats and zero mismatches against the Tier-A references and across cells. | Pinned rustc 1.97.1/libm 0.2.16 only; macOS and other targets remain unqualified at the existing 1e-12 aligned band. The evidence is recomputed per PR, not admitted from stored digests; an unavailable runner blocks the merge rather than passing it. No mathematical correctness, arbitrary-input, whole-executable or Sim qualification follows. | [Per-PR native matrix](strict-bit-evidence.md#per-pr-native-matrix); [Testing standard](../TESTING.md#the-four-pillars) | test [native_linux_matrix_is_complete_and_exact](../crates/oce-conformance/tests/strict_bits/matrix.rs#L253-L295); [every_signal_and_cell_reports_the_exact_finite_mutation](../crates/oce-conformance/tests/strict_bits/matrix.rs#L358-L393); [one_flipped_bit_in_a_cell_or_only_its_repeat_fails_the_cross_cell_comparison](../crates/oce-conformance/tests/strict_bits/controls.rs#L281-L328); [every_corpus_sample_uses_exact_facade_comparison_and_rejects_mutations](../crates/oce-conformance/tests/strict_bits/controls.rs#L61-L114); [qualified_linux_signals_are_exact_and_other_targets_keep_the_aligned_band](../crates/oce-conformance/tests/strict_bits/controls.rs#L198-L233) |
| PC-038 | CURRENT | State delivery | Execution maintainers | MUST support same-loaded-executable durable continuation and explicit portability domains, refusing incompatible execution ABI, referenced catalog, executable, IO, state and target facts before mutation with typed state errors. | Host-envelope build/deployment approval precedes decoding; OCE refusal does not authenticate builds or freshness or authorize actuation. Format/ABI 2 refuses format 1 without migration. Portable policy and the finite Linux corpus do not imply arbitrary-input, full-closure or macOS qualification. | [State contract](state-compatibility.md); [Prepare and commit](../crates/oce-api/src/state.rs); [Host envelope](host-responsibilities.md#persist-engine-state-outside-the-engine) | test [every_manifest_field_refuses_deterministically_without_mutating_the_engine](../crates/oce-api/src/tests/state_manifest_refusal_tests.rs#L276); [changed_input_acceptance_domain_refuses_before_mutation](../crates/oce-api/tests/state_contract.rs#L54); [parsed_snapshot_continuation_preserves_signed_zero_and_retained_results](../crates/oce-api/tests/state_contract.rs#L103); [every_target_bound_class_round_trips_policy_and_refuses_each_foreign_target_component](../crates/oce-api/src/tests/state_portability_tests.rs#L146); [every_truncation_boundary_is_a_typed_refusal](../crates/oce-api/src/tests/state_codec_tests.rs#L94) |
| PC-039 | CURRENT | Replay delivery | Execution maintainers | MUST provide one bounded canonical accepted-frame replay record with exact public facts, placement, time, complete inputs/outputs and ordered Warning diagnostics, independently decodable with typed deterministic refusal and exact comparison. | Hosts authenticate order, build/executable/prior-state qualification and optional snapshot sidecars; no sequence container, build token, tolerance, second evaluator or rollback on comparison mismatch. Hosted cross-architecture replay-byte comparison remains pending. | [Replay contract](replay-record.md); [Facade](../crates/oce-api/src/replay.rs) | test [canonical_receipt_matches_independent_bytes_and_repeats_without_reexecution](../crates/oce-api/tests/replay.rs#L46); [host_stream_continues_from_separate_snapshot_with_equal_times_and_exact_end_state](../crates/oce-api/tests/replay.rs#L86); [host_eligibility_refusals_preserve_fresh_and_advanced_state_and_restore_window](../crates/oce-api/tests/replay.rs#L274); [header_version_lengths_integrity_and_every_truncation_refuse_repeatedly](../crates/oce-api/tests/replay_codec.rs#L61); [inclusive_record_cap_accepts_a_full_string_and_one_past_allocates_nothing](../crates/oce-api/tests/replay_codec.rs#L380); [small_wire_entries_cannot_amplify_decode_workspace_past_the_charged_budget](../crates/oce-api/tests/replay_codec.rs#L412); [independently_authored_full_domain_bytes_are_the_capture_encoding_too](../crates/oce-api/src/replay_capture_tests.rs#L23) |
| PC-040 | CURRENT | Release delivery | Release maintainers | MUST support only current/current and refuse cross-candidate state and replay envelopes under host policy before decoding or execution. | No N-1 support, migration, release, publication or downstream qualification follows. Producer absence is not decoder refusal; host cold requalification or prior-qualified-binary/own-state rollback stays external. | [Candidate pairing policy](#candidate-pairing-policy); [Host fallback checklist](host-responsibilities.md) | test [cross_candidate_envelopes_refuse_before_decode_or_engine_mutation](../crates/oce-api/tests/release_fallback.rs#L60); [absent_or_unauthenticated_envelopes_never_reach_the_decoder](../crates/oce-api/tests/release_fallback.rs#L188); [rollback_handoff_retains_prior_qualified_identity_and_opaque_state_outside_oce](../crates/oce-api/tests/release_fallback.rs#L205) |
| PC-041 | CURRENT | Blob delivery | Execution maintainers | MUST encode a loaded executable as one canonical OCEB format-2 compiled-schedule blob whose bytes are identical on x86_64 and aarch64 and in debug and release codegen, and decode it in `oce-graph` (`no_std + alloc`) only under the exact same engine version, refusing malformed, truncated, mis-versioned, class-mismatched or inconsistent bytes with typed errors and never a panic. | Same-build rule only: no N-1 support or migration. Semantics are trusted (a well-formed blob with wrong parameters executes as written; the CRC-32C trailer detects corruption and is not authentication; the host envelope authenticates and authorizes blobs); memory safety is hostile-input with a documented peak-allocation ratio. Run state does not travel. Target-bound classes still make outputs depend on the target math library. Byte identity across x86_64 and aarch64 is compared per PR in debug codegen; the release-codegen encoding on both architectures and the comparison of debug against release run in the release gate on every merge into `development`, not on the pull request, where the author's local gate run compares the release-codegen encoding with the goldens on one host (issue #502). Bare-metal execution is evidenced for one golden only: per PR CI builds the firmware image that runs it on `thumbv7em-none-eabihf` and `riscv32imac-unknown-none-elf` within a size budget and runs the image's code on the host for every golden, and it executes the image only nightly under QEMU, outside CI OK, never on silicon (GitHub #449). The HostTick frame driver is not part of the blob's bytes; a decoded blob executes through the one core driver via `oce_graph::blob::Executable` (GitHub #441). The decoded blob's run equals the facade's bit for bit — every output, warning and raw state word after every frame — for the 46 G36 fixtures over one 160-frame schedule, per PR on x86_64, aarch64 and `wasm32-wasip1` under one pinned wasmtime release, replaying one recorded x86_64 artifact (issue #403); no browser engine, other runtime or `wasm32-unknown-unknown` build is executed. | [Compiled-schedule blob](compiled-schedule-blob.md); [Trust model](compiled-schedule-blob.md#trust-model); [Encoder](../crates/oce-api/src/blob.rs); [Decoder](../crates/oce-graph/src/blob/decode.rs); [Bare-metal cell](ci-and-the-gate.md#the-bare-metal-cell); [Blob round trip](compiled-schedule-blob.md#the-blob-round-trip); [Round-trip legs](ci-and-the-gate.md#the-blob-round-trip) | test [every_fixture_blob_matches_its_checked_in_golden_byte_for_byte](../crates/oce-api/tests/blob_goldens.rs#L45); [decoding_and_re_encoding_reproduces_every_golden](../crates/oce-api/tests/blob_goldens.rs#L96); [the_worked_example_has_the_design_note_layout](../crates/oce-api/tests/blob_goldens.rs#L142); [the_per_class_signature_hash_table_matches_its_golden](../crates/oce-api/tests/blob_goldens.rs#L173); [the_facade_and_the_decoded_blob_agree_bit_for_bit_after_every_tick](../crates/oce-api/src/tests/blob_dual_path_tests.rs#L594); [a_perturbed_input_on_the_decoded_side_fails_the_comparison](../crates/oce-api/src/tests/blob_dual_path_tests.rs#L641); [every_prefix_of_a_blob_is_a_typed_refusal](../crates/oce-api/tests/blob_hostile_bytes.rs#L73); [header_defects_refuse_in_the_documented_order](../crates/oce-api/tests/blob_hostile_bytes.rs#L110); [random_byte_mutations_never_panic_the_decoder](../crates/oce-api/tests/blob_fuzz.rs#L42); [a_hostile_port_list_is_refused_within_eight_times_the_blob_length](../crates/oce-api/tests/blob_peak_allocation.rs#L67); [a_target_bound_class_makes_the_decoded_executable_target_bound](../crates/oce-api/tests/blob_hostile_io_schedule.rs#L306); [every_constructor_survives_hostile_parameter_tables](../crates/oce-blocks/tests/hostile_parameters.rs#L121); [a_derived_default_formula_is_documentation_and_does_not_change_the_hash](../crates/oce-blocks/src/class_signature_tests.rs#L335); [every_golden_reaches_its_table_digest_through_the_firmware_run](../firmware/link/tests/run_tests.rs#L77); [every_input_of_every_golden_equals_the_host_schedule_by_bits](../firmware/link/tests/schedule_tests.rs#L66); [every_fixture_round_trips_through_its_blob_bit_for_bit](../crates/oce-conformance/tests/blob_round_trip.rs#L225); [a_one_bit_flip_in_a_recorded_output_names_the_fixture_frame_slot_and_both_bit_patterns](../crates/oce-conformance/tests/blob_round_trip/controls.rs#L31) |

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
returned refusal path. Since revision 18 load calls no storage port, so there is no residual external
effect to compensate. Successful replacement refreshes model-bound
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
execution is the only public state-advancing execution path, beside its slot-indexed twin
(`frame_slots` once, then `execute_slots`; issue #444), which submits the same complete frame by
slot through the same core driver with the same refusals and is not another profile. Legacy execution methods, epoch
configuration, raw output access, and associated source/spec/report/trace types are removed. All
first-party consumers submit complete frames. Uniform and event-aligned conformance modes retain
their cadence and comparison semantics, but missing determinants refuse before comparison.

PC-035 is CURRENT for this bounded outcome. Compiler controls run against all supported feature
selections; absence sentinels prevent re-blessing retired surfaces. Frame purity, refusal
preservation, hand-derived frame goldens and existing independent sequence references provide
behavioral evidence. get_output and watch are latest-state non-receipt inspections. Realtime and
storage orchestration remain deferred; execute_frame performs no write-back. Assertion and execution
descriptor revisions advance to 2 to remove stale profile claims; HostTick v1, catalog identity,
state codecs, accepted-frame sequence and admission/load compensation semantics remain unchanged.
No latency improvement, hosted cross-architecture result, host qualification or publication follows.

## Typed identities

Revision 10 implemented the owner-bounded M03-PR01 public host contract only. Catalog and complete
export tags had distinct facade-owned types; an immutable revision-1 descriptor captured public
catalog content/schema, IO/value/parameter revisions, fixed HostTick compatibility, the exact
OCE Cargo package version and optional complete export content. Revision 23 (GitHub #470) replaces
the export tag with the canonical content id (`ContentId`, `oce:canon1:`, minted by
`Engine::content_id`) and the descriptor with revision 2, whose `content` line carries it.
Canonical bytes and typed first-mismatch outcomes have hand-assembled goldens, repeated captures,
per-field mutation controls and compile-time category refusals. A partial export no longer
withholds identity: the id names the loaded model. Absence is explicit and never a wildcard.

PC-036 is CURRENT only for those facts. Package version is not a unique source/build/compiler/target
identity, and IO revision is not a model's executable input schema. Executable fingerprints,
generation and state-wire identities remain private pending the separately authorized state/replay
work. No manifest/codec bytes, source normalization, signing/PKI, host qualification or cross-release
compatibility follows. Existing APIs/bytes, package closure and state/restore behavior remain intact.
The [receipt mapping](facade-migration.md#compatibility-receipt-adoption) preserves current consumers;
actual downstream pins and qualification are unchanged.

## Strict-bit evidence

The [per-PR native matrix](strict-bit-evidence.md#per-pr-native-matrix) supports PC-037's
observed-output claim, only for the 21 inventoried Linux signal cases. Every pull request captures
them in four native cells (x86_64 on `ubuntu-latest`, aarch64 on `ubuntu-24.04-arm`, debug and
release), two independent runs per cell, 161 samples per run, and a fan-in job requires byte-identical
repeats, exact agreement with the unchanged Tier-A references and exact agreement across cells.
Each run recomputes the result from the pull request's own checkout through the actual
`oce_api::Engine` path, so the claim tracks the current sources without a stored source-digest
binding; the 35-file selected-source receipt that carried it until GitHub #392 is retired, and its
captures stay as reconstructed history. `receipts.json` logs the last green native run and gates
nothing. A runner type that is unavailable leaves `CI OK` unreported, so the PR cannot merge.
The original macOS observation is not platform qualification. macOS-arm64 stays conservative until
M06-PR02, and all other unqualified targets retain the existing aligned band. Whole-executable
exactness inherits the least-qualified contributing path, target and input domain; these finite
cases alone do not establish it. Mathematical correctness, arbitrary-input and downstream policy
claims remain outside this evidence.

## Same-loaded-executable state

Revision 13 implements M03-PR03 under the owner's explicit host-enforced-envelope decision.
The [state contract](state-compatibility.md) defines capture, parse, refusal precedence, startup
window, bit-exact continuation and encoded portability separately from build qualification.
Read-only public portability inspection adds no build token or public executable fingerprint.
Format/ABI revision 2 adds effective frame input domains and connector computation units/quantities;
revision-1 bytes refuse rather than silently receiving stronger compatibility claims. Capture
validates its own encoded bytes through the bounded decoder. Field mutations, canonical goldens,
partial-byte refusal, Store noninterference, all 15 target-bound classes and hand-derived equal-time
continuation supply bounded engine evidence, not host-envelope or cross-release qualification.
Hosted matrices remain separate exact-candidate evidence; no local run fabricates them.

## Canonical replay

Revision 14 implements the owner-bounded M03-PR04 v1 as one canonical accepted-frame record.
The facade receipt retains accepted inputs and captured placement; record creation never executes
again. Independent hand-authored payload bytes, checksum construction, mutation/refusal controls,
full native-value domain coverage, snapshot-sidecar continuation and an allocation-counted streamed
host prototype supply bounded PC-039 evidence. Exact bits only; no conformance tolerance is reused.
Sequence ordering and same-build/deployment/executable qualification stay in the authenticated host
envelope. Snapshots are separate sidecars; the private execution fingerprint is not public authority.
Hosted replay artifact comparison is pending: changing the existing workflow would cross the accepted
strict-bit source guard. Local vectors are retained without reblessing that separate evidence.

## Candidate pairing policy

Revision 15 implemented the owner-selected fail-closed policy: current/current is the only supported
candidate pairing. Revision 17 keeps that policy and retires the retained directed matrix, its
checker and the historical v0.1.0 source pin evidence: the engine is pre-1.0 with no
backwards-compatibility promise, and a matrix proving refusal of an unsupported pairing adds nothing
the host refusal tests do not. Equal package strings are still not build authority.

PC-040 is CURRENT only for that policy, not general release-to-release support. The host fixture
refuses cross-candidate envelopes before decoding, preserves engine and Store state, and contrasts
same-current continuation with fresh cold requalification. External rollback dispatch is bounded
fixture evidence, not an executed prior qualified binary or field qualification. The
[host fallback checklist](host-responsibilities.md) retains those limits. Broader release/platform
qualification and any new support pairing still require separate owner authorization and acceptance.

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

Library, Studio and Edge retain their downstream roles. Runtime is an additive future
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
requirement list. The [hostile tests](../scripts/product_contract/test_check.py) retain manually
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
  the additive public baseline and its surface classification are updated together. Preparation
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
  baseline/classification/guards and migration guidance change together. Pending-path enumeration admits
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
  bytes, dependencies and features remain unchanged. Public baseline and surface classification
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
  provenance delivery. The checked-in observation was local macOS only; native Linux acceptance
  was still pending, so PC-037 was not promoted in that revision. Existing aligned bands remained
  the conservative platform policy; Linux candidates failed closed on disagreement. Runtime formulas, public APIs,
  state/restore, dependencies, toolchain and downstream policies are unchanged. The checker revision
  and pending documentation path advance mechanically without weakening earlier sentinels.
- Revision 12, 2026-09-20: accepted native run 35492290613 supplies the retained four-cell,
  two-run, 21-signal zero-mismatch Linux corpus result. PC-037 is CURRENT only in that bounded
  scope. Raw bits and synthetic merge provenance remain unchanged and digest-checked; no final-HEAD
  equality or source-digest exception is introduced. Existing Linux exact wiring and all Tier-A
  goldens remain unchanged. macOS/other targets retain the 1e-12 aligned band, with macOS-arm64
  unqualified until M06-PR02. No arbitrary-input, mathematical, whole-executable or Sim qualification,
  runtime/API/state change, dependency change or publication follows.
  Admission with the reviewed selected source boundary uses run 37382761120, preserving its synthetic
  merge identity and eight capture files verbatim, with exactly all 35 enumerated paths/digests
  verified and no bound-source changes. This boundary is not the full compiled dependency closure;
  PC-037 remains an observed-output claim over the pinned corpus, targets and toolchain.
  The earlier receipt remains historical; the ordinary current-qualification test validates the
  newly admitted receipt without a final-HEAD identity requirement or source-map bypass.
- Revision 13, 2026-09-20: owner explicitly chose host-envelope enforcement of compiled-build and
  deployment qualification before snapshot decoding; no OCE build token is introduced. Bounded
  implementation/evidence promotes PC-038 for same-loaded-executable continuation only. Format and
  execution ABI advance to 2 for missing IO compatibility determinants; HostTick v1 and the 15-class
  placement policy remain unchanged. Public portability inspection, baseline/classification and
  authority projection change together. The traceability checker advances its accepted revision and pending
  evidence paths without relaxing earlier sentinels. Revision-1 snapshots intentionally refuse;
  rollback/cold-start policy is host-owned. No replay design, cross-release migration, dependencies,
  CI policy, downstream qualification, host security service or publication is added.
- Revision 14, 2026-09-20: owner selected a single canonical exact per-accepted-frame replay record,
  host-owned ordered envelopes and optional snapshot sidecars. Bounded decoding, independent byte
  construction, hostile mutations, exact stateful continuation and streamed allocation evidence
  promote PC-039 only in that scope. New facade items, public baseline/classification and the
  traceability revision/pending evidence paths advance together; existing contraction sentinels
  remain. Receipt capture now retains accepted inputs; record-capture refusal does not undo execution.
  No Engine replay method, second evaluator, sequence container, tolerance, embedded snapshot, build
  token, dependencies or state/profile revision changes. Cross-architecture replay-byte artifacts
  and real downstream qualification remain pending; the existing strict-bit evidence is not rewritten.
- Revision 15, 2026-09-20: owner froze fail-closed, no-N-1 candidate compatibility. Retained directed
  evidence, source/history identities, hostile checker controls and isolated host fallback fixtures
  promote PC-040 only to that policy. The historical source build is reproducible but unqualified;
  absent producers are not labeled decoder refusals. Current runtime/API, package version, formats,
  dependencies, workflows, strict-bit guard and downstream pins remain unchanged. No publication,
  migration, cross-build restore, M06 ratification or downstream equipment qualification follows.
- Revision 16, 2026-10-26: editorial only (issue #359). Open Control Sim was discontinued, so the
  Sim downstream role and the M05-PR07 Sim adoption note are removed; Library, Studio and Edge keep
  their roles. Historical Sim inventory statements and the PC-037 limitation stay. No requirement,
  status, owner, grounding, evidence link, runtime, API or downstream pin changes. The traceability
  checker advances its accepted revision only.
- Revision 17, 2026-11-08: owner retired the compatibility records pre-1.0 (GitHub #388). The
  public surface baselines, their classification, the package publication record, the retained
  release matrix and the dated stability snapshot are deleted; a surface baseline returns at 1.0.
  PC-002 is narrowed to what is enforced (the facade, compile-time refusal of retired names, the
  private-or-registry-ready closure); the per-crate support roles lived only in the deleted
  records and are no longer claimed. PC-003, PC-028 and PC-035 now cite the facade compiler
  controls; PC-040 keeps the current/current refusal policy on the host refusal tests alone. The
  release-gate nightly toolchain and its surface steps are removed. No runtime, API, state bytes,
  catalog, HostTick or golden changes. The traceability checker advances its accepted revision.
- Revision 18, 2026-11-08: the engine is pure (GitHub #390). `Engine` loses its storage type
  parameter, `with_store`, `store()` and the three load-time storage calls; `oce-api` no longer
  depends on `oce-store` or `oce-store-mem`, and load returns a plain-data `LoadedModel` beside its
  report. PC-005 now claims no external load effect instead of distinguishing Store
  transactionality; PC-014 drops the load-time Store validation note and cites the facade
  storage-isolation tests and `check-default-no-db.sh` pass 4 beside corpus purity; PC-008, PC-013,
  PC-019, PC-020, PC-027, PC-030 and PC-035 replace Store wording with storage-port wording.
  Evidence links follow renamed tests; the Store-refusal, residual-handle and Store-call-count
  tests are removed with the behaviour they covered, PC-016 and PC-027 cite remaining tests of the
  same claims, and PC-028 cites the adapter-free port of the filtered-inventory refusal test. The
  diagnostics schema advances to revision 3; state bytes, catalog, HostTick and goldens are
  unchanged. The traceability checker advances its accepted revision.
- Revision 19, 2026-11-08: new requirement PC-041 for the OCEB compiled-schedule blob (GitHub #438,
  issue #400), approved by the owner's delegate for the review; affected owner Execution
  maintainers. It claims canonical, cross-architecture and cross-codegen identical bytes, decoding
  in `oce-graph` under the exact same engine version only, and typed refusal without panics, with a
  trusted-semantics, hostile-memory trust model and a documented peak-allocation ratio. No existing
  requirement, status, owner or evidence link changes; state bytes, catalog, HostTick and goldens are
  unchanged. The traceability checker advances its accepted revision.
- Revision 20, 2026-11-08: the HostTick v1 frame driver moves from `oce-api` into `oce-graph`
  (`oce_graph::frame`, GitHub #441, issue #401), and the `no_std` blob execution entry calls it.
  Grounding and evidence: PC-006's core link and PC-034's shared-core link now point at the core driver,
  and PC-041's two dual-path evidence links follow their tests' new lines in the extended
  dual-path file (same test names, now also covering the core driver, the blob entry and refusal
  parity). PC-041's limitation adds that a decoded blob executes through the one core driver via
  `oce_graph::blob::Executable`; the frame driver is still not part of the blob's bytes. No
  requirement, status or owner changes; behaviour, refusal order, state bytes, replay records,
  catalog and goldens are unchanged. The traceability checker advances its
  accepted revision.
- Revision 21, 2026-11-08: the bare-metal gate cell and firmware image (GitHub #449, issue #402).
  PC-041's limitation replaces "CI does not yet execute the blob on a bare-metal target" with the
  bounded evidence that now exists: a firmware image runs one golden through the `no_std` entry,
  built per PR for `thumbv7em-none-eabihf` and `riscv32imac-unknown-none-elf` within a size budget,
  with its run code tested on the host against every golden, and executed only by the nightly QEMU
  workflow, outside CI OK, never on silicon. PC-041 gains a grounding link to the bare-metal cell
  and two evidence links to the firmware crate's host tests. The firmware image is the
  workspace's third private member, so PC-002's two package-validator evidence links follow their
  tests two lines down (the private set and the release workflow's byte golden each grew a line).
  No requirement, status or owner
  changes; behaviour, state bytes, catalog and goldens are unchanged. The traceability checker
  advances its accepted revision.
- Revision 22, 2026-11-08: the blob round trip (issue #403). PC-041's limitation adds the bounded
  execution evidence: the decoded blob's outputs, warnings and raw state words equal the facade's
  after every frame of the 46 G36 fixtures, per PR on x86_64, aarch64 and `wasm32-wasip1` under one
  pinned wasmtime, replaying one recorded x86_64 artifact, with no browser or
  `wasm32-unknown-unknown` execution. PC-041 gains grounding links to the round trip and its CI
  legs and evidence links to the suite and its one-bit control; its two dual-path evidence links
  follow their tests six lines down (the dual-path comparison now also ties the new public
  `Engine::state_words` view to the run state). No requirement, status or owner changes; behaviour,
  state bytes, catalog and goldens are unchanged. The traceability checker advances its accepted
  revision.
- Revision 23, 2026-11-09: the canonical content id (issue #470, design note GitHub #471). PC-018
  is reworded: one canonical content id over the loaded graph (SHA-256, `oce:canon1:`) minted only
  by `Engine::content_id`, invariant under re-serialization, reordering and namespace and moved by
  any parameter bit, name, wiring, class or boundary I/O change, with `ExportReport::complete`
  distinguishing a partial export. Its three evidence tests change (the partial-export test checks
  `complete()`, the parameter-edit test is rewritten against `Engine::content_id`, and the
  authored-order test inverts: order no longer moves the id), and the permutation, one-change and
  equal-behaviour tests join. PC-021 names canonical content identity as its own role. PC-036's
  descriptor is revision 2 with a `content` line and two of its tests are renamed with the surface.
  The retired `content_id_complete()`, `content_id()` and `CompleteExportContentId` are removed.
  PC-041 now names a format-2 blob. Blob format 2 and descriptor revision 2 re-bless their goldens; export bytes and every other
  golden are unchanged. The traceability checker advances its accepted revision.
- Revision 24, 2026-11-09: evidence delivery only (issue #502, owner decision of 2026-11-09);
  affected owner Execution maintainers. The determinism matrix's release-codegen cells and its
  debug-against-release comparisons move from the per-PR gate to the release gate, which now runs
  on every merge into `development`; the per-PR gate keeps both architectures in debug codegen.
  PC-041's limitation records where its cross-codegen byte identity is now compared, and that the
  pre-merge release-codegen evidence is the author's local gate run on one host. PC-037 is
  unchanged: all four strict-bit cells and the eight-capture comparison stay per pull request. No
  requirement, runtime behaviour, signature, catalog identity, state byte or golden changed. The
  traceability checker advances its accepted revision.
