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
#[path = "support/dual_entry.rs"]
mod dual_entry;
#[path = "../../oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;
#[path = "replay_oracle/mod.rs"]
mod replay_oracle;

mod array_extent_cost;
mod blob_allocations;
mod blob_peak_allocation;
mod cxf_admission;
mod hostile_load_bounds;
mod prepare_frame;
mod replay;
mod replay_codec;
mod slot_frame_refusals;
mod slot_frames;
mod slot_output_types;
mod state_engine_version;
