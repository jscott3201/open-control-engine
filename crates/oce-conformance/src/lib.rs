#![forbid(unsafe_code)]
//! `oce-conformance` — the funnel-style conformance harness for the Open Control Engine
//! (`07-conformance-and-verification.md`).
//!
//! This crate owns the M2 trace-comparison substrate: a pure deterministic L1 tolerance-band
//! comparison and byte-stable Modelica `CombiTimeTable` CSV I/O. Later M2 slices bind this logic
//! through the frozen `oce-api` facade; this slice intentionally has no `oce-api`, `oce-graph`, or
//! `oce-blocks` dependency.
//!
//! Status: **M2-PR-B1 as-built.** The core funnel DTOs/comparison and CombiTimeTable reader/writer
//! are implemented; masking, tier reports, and facade-bound trace driving land in later B-lane PRs.

pub mod csv;
pub mod funnel;

pub use csv::{CombiTimeTable, CsvError, ValueKind, format_f64};
pub use funnel::{FunnelResult, Series, Tolerances, build_bounds, compare};
