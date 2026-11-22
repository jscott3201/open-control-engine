//! The two files a commissioning run reads: the test spec, `oce/commissioning-spec/v1`, and the
//! point mapping (design note `docs/design/commissioning.md`, "Test-spec file" and "Point
//! mapping"; issue #658).
//!
//! Bytes in, typed values or typed refusals out. Nothing here opens a file, loads an engine or
//! compares a series: those are the later parts of the commissioning verifier, and each reads
//! the types this module builds.
//!
//! - **The spec** ([`CommissioningSpec`]): a strict-JSON superset of OBC §12.7 Listing 12.1.
//!   `//` and `/* */` comments outside strings are removed, then the text is read by the crate's
//!   one strict cursor, under the vector reader's profile and byte bound ([`SpecLimits`]). The
//!   LBL fields load unchanged; the fields of Scenario 1 the verifier does not act on are
//!   reported as findings ([`Finding`], [`LBL_FIELD_IGNORED`]); the added fields are the CXF
//!   path, parameters, trend sources ([`TrendSource`]), the window, the envelope
//!   ([`crate::Envelope`], the comparison's own type) and the time origin.
//! - **The point mapping** ([`PointMapping`]): Listing 12.2's shape, unchanged, from a file of
//!   its own or inline in a spec.
//! - **Keys** (`glob`, `selection`; crate-private until the bind that calls them lands, issue
//!   #662): which outputs an `outputs` or `indicators` key selects, as a glob or as an exact
//!   name, and the tolerances and indicators that follow, by a precedence that does not depend
//!   on the order of the file.
//! - **The canonical form** ([`CommissioningSpec::to_canonical_json`]) and the spec's identity
//!   over it ([`CommissioningSpec::spec_id`]).
//! - **Typed refusals** ([`SpecError`], [`SpecErrorKind`], [`SPEC_ERROR_CODES`]): one closed
//!   kind per refusal, each with a stable kebab-case code and, for everything found in the
//!   text, the line and column in the text as written.
//!
//! And what a trend's cells become (issue #659): the closed unit table `units/v1`
//! ([`convert_unit`], [`UnitError`]), which converts a reading from its device unit to its CDL
//! point's, and the discrete type rules of a point mapping (`type_rules`, crate-private). Both
//! are pure functions of a cell and the mapping: they read no file, load no engine and never
//! touch a parameter (decision 24).
//!
//! And the trend sources (issue #660): [`TrendSources::read`] takes the bytes of a reference's
//! files ([`SpecInputs`]), plans every column against the mapping before any row is read
//! (`trend_plan`), reads each file (`trend_read`: `csv` through replay's `CsvSource`,
//! `combiTimeTable` through `combi_time_table`), converts every cell once, and merges the input
//! sources on the union of their times (`trend_merge`), under [`TrendLimits`], every refusal a
//! [`SourceError`].
//!
//! And the run (issues #662 and #663): [`run_commissioning()`] binds a reference to its sequence
//! (`bind_ports`, `bind_reference`), runs one replay over its merged inputs (`drive`), prepares
//! each compared output's two series on their comparison instants (`prepare`), masks them by
//! their indicators and the refused stretches (`indicator_mask`), compares them under the
//! reference's envelope or exactly (`compare_output`), and issues the receipt (`report`,
//! `report_json`), every refusal a [`crate::VerifyError`].

mod bind_ports;
mod bind_reference;
mod canonical;
mod change_of_value;
mod choices;
mod combi_time_table;
mod comments;
mod compare_change;
mod compare_output;
mod decode;
mod decode_sources;
mod drive;
mod error;
mod fields;
mod glob;
mod indicator_mask;
mod limits;
mod mapping;
mod model;
mod prepare;
mod report;
mod report_json;
mod run_bounds;
mod run_commissioning;
mod run_options;
mod selection;
mod source_error;
mod spec;
mod tolerances;
mod trend_limits;
mod trend_merge;
mod trend_plan;
mod trend_read;
mod trend_sources;
mod trend_types;
mod type_rules;
mod units;

pub use choices::{FunnelReference, PointType, TrendFormat};
pub use error::{MAX_SPEC_DETAIL_BYTES, SPEC_ERROR_CODES, SpecError, SpecErrorKind};
pub use limits::SpecLimits;
pub use mapping::{PointMapping, PointPair, PointSide};
pub use model::{
    ComparisonWindow, IndicatorRule, OutputOverride, PointNameMapping, SourceColumn,
    SourceTimeColumn, SpecReference, TrendSource,
};
pub use source_error::{SOURCE_ERROR_CODES, SourceError, SourceErrorKind};
pub use spec::{CommissioningSpec, Finding, LBL_FIELD_IGNORED, SPEC_FINDING_CODES};
pub use tolerances::{PartialTolerances, TOLERANCE_NAMES};
pub use trend_limits::TrendLimits;
pub use trend_types::{
    InputColumn, MappingRecord, SeriesValues, SourceRecord, SourceRole, SpecInputs,
    TrendFrameSource, TrendInputs, TrendSeries, TrendSources, UnitColumn,
};
pub use units::{UNIT_ERROR_CODES, UNITS_TABLE, UnitError, UnitSide, convert_unit};

pub use bind_reference::MAX_INDICATORS;
pub use report::{
    CommissioningBody, CommissioningParam, Exceedance, FindingRecord, MAX_COMPARED_OUTPUTS,
    MAX_FINDINGS, MAX_MASKED_EXCEEDANCE, MAX_MASKED_INTERVALS, MAX_RUN_POINTS, MAX_RUN_WARNINGS,
    MAX_UNIT_COLUMNS, MAX_UNVERIFIED_OUTPUTS, MaskedInterval, OutputReport, OutputStatus,
    OutputType, SpecNumber, TimeRecord, TrendData,
};
pub(crate) use report_json::receipt_body_json;
pub use run_commissioning::run_commissioning;
pub use run_options::{
    CommissionOptions, CommissionRun, MAX_TOTAL_ALIGNED_GRID_WORK, OutputBounds, OutputCurves,
};

/// The format tag a commissioning spec carries in its `schema` key. A spec without the key is an
/// LBL spec and is read as this format; any other tag is refused as `unknown-schema`.
pub const COMMISSIONING_SPEC_SCHEMA: &str = "oce/commissioning-spec/v1";
