#![forbid(unsafe_code)]
//! `oce-conformance` — the funnel-style conformance harness for the Open Control Engine
//! (`07-conformance-and-verification.md`).
//!
//! This crate owns the trace-comparison substrate: a pure deterministic L1 tolerance-band comparison
//! and byte-stable Modelica `CombiTimeTable` CSV I/O. It also owns the verification config,
//! indicator masking, and Tier 0-4 report DTOs used by the later facade-bound trace driver. This
//! crate intentionally has no `oce-api`, `oce-graph`, or `oce-blocks` dependency.

pub mod config;
pub mod csv;
pub mod funnel;
pub mod mask;
pub mod tiers;

pub use config::{
    ConfigError, PartialTolerances, PointEnd, PointMapEntry, ReferenceSpec, VerifyConfig,
};
pub use csv::{CombiTimeTable, CsvError, ValueKind, format_f64};
pub use funnel::{FunnelResult, Series, Tolerances, build_bounds, compare};
pub use mask::{Indicator, Mask, MaskError, compare_masked};
pub use tiers::{ConformanceReport, ConformanceTier, TierReport, TierStatus};
