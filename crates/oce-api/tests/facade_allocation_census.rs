//! The facade's integration tests that measure heap allocations, compiled as one test binary.
//!
//! Every module here calls `allocation_counter::measure`. Naming that crate links its counting
//! global allocator into the whole binary, so these files share a binary apart from `facade.rs`
//! (GitHub #504) and each test still runs under the allocator it ran under as its own binary. The
//! counters are thread-local, and nextest runs every test in its own process, so a measured region
//! never counts another test's allocations. Each module keeps its file name, and nextest reports
//! its tests as `oce-api::facade_allocation_census <file>::<test>`. The check that every file under
//! `tests/` is compiled into exactly one binary lives in `facade.rs`.

// Helpers that several modules share, declared once for the whole binary.
#[path = "support/blob_corpus.rs"]
mod blob_corpus;
#[path = "support/blob_image.rs"]
mod blob_image;
#[path = "support/blob_models.rs"]
mod blob_models;
#[path = "support/dual_entry.rs"]
mod dual_entry;
#[path = "../../oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;
// The parameter fuzz's hostile tables, shared with `oce-blocks` and `oce-validate`.
#[allow(
    dead_code,
    reason = "this binary uses part of the support the parameter fuzz shares"
)]
#[path = "../../oce-blocks/tests/support/hostile_tables.rs"]
mod hostile_tables;
#[path = "replay_oracle/mod.rs"]
mod replay_oracle;

mod array_extent_cost;
mod blob_allocations;
mod blob_hostile_parameter_models;
mod blob_peak_allocation;
mod blob_summary_allocations;
mod cxf_admission;
mod cxf_array_element_names_cost;
mod cxf_constructed_budget;
mod cxf_context_expansion_cost;
mod cxf_diagnostic_list_cost;
mod cxf_pass_through_key_cost;
mod hostile_load_bounds;
mod prepare_frame;
mod replay;
mod replay_codec;
mod slot_frame_refusals;
mod slot_frames;
mod slot_output_types;
mod state_engine_version;
mod validate_sparse_table_cost;
