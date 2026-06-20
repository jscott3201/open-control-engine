#![forbid(unsafe_code)]
//! `oce-conformance` — the funnel-style conformance harness for the Open Control Engine
//! (`07-conformance-and-verification.md`).
//!
//! This crate owns the trace-comparison substrate: a pure deterministic L1 tolerance-band comparison
//! and byte-stable Modelica `CombiTimeTable` CSV I/O. The core funnel DTOs/comparison and
//! CombiTimeTable reader/writer are implemented; masking, tier reports, and facade-bound trace
//! driving are layered on top later. This crate intentionally has no `oce-api`, `oce-graph`, or
//! `oce-blocks` dependency.

pub mod csv;
pub mod funnel;

pub use csv::{CombiTimeTable, CsvError, ValueKind, format_f64};
pub use funnel::{FunnelResult, Series, Tolerances, build_bounds, compare};
