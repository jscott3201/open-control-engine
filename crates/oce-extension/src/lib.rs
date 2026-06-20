#![forbid(unsafe_code)]
//! `oce-extension` — the FMI / extension-block boundary for the Open Control Engine.
//!
//! FMI/FMU runtime and native-handler execution are deferred to the post-v1 "later" band
//! (FRAME §7; roadmap `09` §2 "later"). In v1 an extension block is surfaced as an **unresolved
//! external**: it has no executable handler, only an optional FMU path. This crate is **Group A**
//! (no store, no database).
//!
//! The boundary DTO is reserved; FMI/native execution is out of v1 scope.

/// A v1 extension/FMI block surfaced as an unresolved external (FRAME §7; `04` §3.7).
#[derive(Clone, Debug, Default)]
pub struct ExtensionBlock {
    /// Optional FMU path; `None` for a bare unresolved external. No executable handler in v1.
    pub fmu_path: Option<String>,
}
