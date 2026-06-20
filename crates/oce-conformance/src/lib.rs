#![forbid(unsafe_code)]
//! `oce-conformance` — the trace conformance harness for the Open Control Engine
//! (`07-conformance-and-verification.md`).
//!
//! This crate owns the trace-comparison substrate: a pure deterministic L1 tolerance-band comparison
//! and byte-stable Modelica `CombiTimeTable` CSV I/O. It also owns the verification config,
//! indicator masking, Tier 0-4 report DTOs, and the golden-trace driver bound through the frozen
//! `oce-api` facade. It intentionally has no direct `oce-graph` or `oce-blocks` dependency.

pub mod config;
pub mod csv;
pub mod driver;
pub mod exact;
pub mod funnel;
pub mod mask;
pub mod tiers;

pub use config::{
    ConfigError, IndicatorPattern, OutputPattern, PartialTolerances, PointEnd, PointMapEntry,
    ReferenceSpec, VerifyConfig,
};
pub use csv::{CombiTimeTable, CsvError, ValueKind, format_f64};
pub use driver::{
    CapturedColumn, CapturedTrace, ComparisonMode, ComparisonResult, DriveCadence, DriveMode,
    DriverError, DriverInputReplay, DriverOptions, DriverRun, SignalComparison, drive_trace,
    drive_trace_with_options,
};
pub use exact::{ExactMismatch, ExactResult, compare_exact};
pub use funnel::{FunnelResult, Series, Tolerances, build_bounds, compare};
pub use mask::{Indicator, Mask, MaskError, compare_masked};
pub use tiers::{ConformanceReport, ConformanceTier, TierReport, TierStatus};
