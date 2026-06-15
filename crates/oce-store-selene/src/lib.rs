#![forbid(unsafe_code)]
//! `oce-store-selene` — the selene-db [`oce_store::Store`] adapter for the Open Control Engine.
//!
//! # M0: EMPTY, feature-gated stub — NO selene-db dependency yet
//!
//! This crate is the **only** crate in the workspace permitted to name a selene-db type
//! (R-SEAM-2). At M0 it is intentionally empty and pulls **no** selene-db crate, so that
//! `cargo build --features selene` still builds while the default DB-free posture (D-OWNER-1) is
//! preserved end to end.
//!
//! ## The M3 plan (roadmap `09` §2 M3; `06` Part 2; D8 / D-OWNER-2)
//!
//! selene-db is introduced for the first time at **M3**, behind the `selene` feature. This crate
//! will then implement `ModelStore + PointStore + SemanticStore` against selene-db:
//!
//! - **Dependency form (D8):** the five `selene-db-*` crates as git dependencies tracking
//!   `branch = "development"` with `package = "selene-db-*"` aliases and a pinned `rev`; all five
//!   share one git source + rev so Cargo unifies them to a single checkout. `Cargo.lock` is
//!   committed so rev bumps stay deliberate; switch to `version = "1.3"` once selene-db 1.3.0
//!   ships. A `[patch."https://github.com/jscott3201/selene-db"]` keyed by the published package
//!   names overrides to the local `/Users/justin/Development/selene-db` checkout for co-development.
//! - **Mapping (D5):** block instances → `CdlBlock` nodes, classes → `BlockClass` nodes,
//!   connectors → `Point` **nodes** (per-connector metadata must be indexable; selene indexes
//!   nodes only — D-OWNER-3), parameters → typed `CdlBlock` properties.
//! - **Persistence (D4 Shape A, D7):** runtime point state → WAL `Change` batches + periodic
//!   snapshot, tiered durability (`EveryN(1)` for safety-relevant writes; group-commit telemetry).
//! - **Hot path (FRAME §3.3):** at most one lock-free `graph.read()` per tick over pre-resolved
//!   node ids; never `begin_write`/GQL/algorithms on the tick.
//!
//! Status: **M0 scaffold** — the type/method surface lands at M3.

/// M0 placeholder so the `selene`-gated facade path has a symbol to link. Carries no selene-db
/// type. Removed when the real adapter surface (`SeleneStore`/`SeleneStoreConfig`) lands at M3.
pub const PLACEHOLDER: () = ();
