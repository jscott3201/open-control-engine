//! What a loaded commissioning spec holds below its top level: references, their overrides,
//! their trend sources and their windows (commissioning note, "Test-spec file").
//!
//! Every type here is built only by a load ([`crate::CommissioningSpec::from_json_bytes`]), so
//! the rules of the format hold by construction: a name is not empty, a path stays inside the
//! spec's directory, a tolerance is not negative, a sampling is above zero and a window's start
//! is not after its stop. Names are kept as written: resolving them against a sequence's inputs,
//! outputs and parameters is the work of the run that binds a reference to a CXF document.
//!
//! Three kinds of list are held, each in the order that carries its meaning. `outputs` and
//! `indicators` keep the order the file declares their keys in, because overrides apply in that
//! order. `parameters` and a source's `columns` are sorted by their keys' bytes, the order the
//! canonical form writes. The source lists (`inputs`, `expected`, `indicatorSources`) and the
//! references keep file order.

use super::choices::{Envelope, FunnelReference, TrendFormat};
use super::mapping::PointMapping;
use super::tolerances::PartialTolerances;
use crate::canonical::Json;
use crate::model::Parameter;
use crate::number::Number;

/// One `outputs` entry: a key and the tolerances it gives the outputs it selects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputOverride {
    pub(crate) key: String,
    pub(crate) tolerances: PartialTolerances,
}

impl OutputOverride {
    /// The key as written: `<sequence>.<name>`, an exact name or, with `*` or `?` after the
    /// prefix, a glob ([`crate::OverrideKey`]). Never empty, and unique among its reference's
    /// `outputs` keys.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The tolerances the key gives, as written: the names it leaves out are left to the keys
    /// before it and to the spec's top-level `tolerances`.
    #[must_use]
    pub fn tolerances(&self) -> &PartialTolerances {
        &self.tolerances
    }
}

/// One `indicators` entry: a key and the indicators that gate the outputs it selects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndicatorRule {
    pub(crate) key: String,
    pub(crate) names: Vec<String>,
}

impl IndicatorRule {
    /// The key as written, read as an `outputs` key is. Never empty, and unique among its
    /// reference's `indicators` keys.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The indicator names, in file order, each as written and never empty. A name that starts
    /// with the reference's `sequence` and a `.` is a signal of the sequence; any other is a
    /// column of an `indicatorSources` file (decision 27). An output is compared only where
    /// every one of its indicators is true.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// Which column of a trend source holds its time.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceTimeColumn {
    /// A column by position, counted from 1, as a `combiTimeTable` matrix has no names.
    Index(u32),
    /// A column by its header name, as a `csv` file has one. Never empty.
    Name(String),
}

/// One entry of a source's `columns` map: a column of the file and the device point it trends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceColumn {
    pub(crate) column: String,
    pub(crate) point: String,
}

impl SourceColumn {
    /// The column as the map's key writes it: a header name, or a position counted from 1 in
    /// decimal (`"3"`). Never empty; what it must be for each format is the source reader's rule.
    #[must_use]
    pub fn column(&self) -> &str {
        &self.column
    }

    /// The device point name the column holds: the `device.name` of a mapped point. Never empty.
    #[must_use]
    pub fn point(&self) -> &str {
        &self.point
    }
}

/// One trend source: a file, how it is written, and which of its columns hold which points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrendSource {
    pub(crate) path: String,
    pub(crate) format: TrendFormat,
    pub(crate) table: Option<String>,
    pub(crate) time_column: SourceTimeColumn,
    pub(crate) columns: Vec<SourceColumn>,
    pub(crate) hold_max_age: Option<Number>,
}

impl TrendSource {
    /// The file's path as written: `/`-separated and inside the spec's directory, with no empty,
    /// `.` or `..` segment and no backslash. A run is handed the file's bytes under this path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// How the file is written.
    #[must_use]
    pub fn format(&self) -> TrendFormat {
        self.format
    }

    /// The matrix to read, when the spec names one: a `combiTimeTable` file can hold more than
    /// one. Never empty.
    #[must_use]
    pub fn table(&self) -> Option<&str> {
        self.table.as_deref()
    }

    /// The column that holds the time.
    #[must_use]
    pub fn time_column(&self) -> &SourceTimeColumn {
        &self.time_column
    }

    /// The columns read, sorted by the bytes of [`SourceColumn::column`]; no column repeats. May
    /// be empty.
    #[must_use]
    pub fn columns(&self) -> &[SourceColumn] {
        &self.columns
    }

    /// `hold.maxAge` in seconds, when the source declares a hold: the longest a sample stands in
    /// for a missing one. Finite and at least zero. `None` means a missing sample refuses its
    /// frame.
    #[must_use]
    pub fn hold_max_age_s(&self) -> Option<f64> {
        self.hold_max_age.map(Number::seconds)
    }
}

/// A comparison window: both series are clipped to `start_s()..=stop_s()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComparisonWindow {
    pub(crate) start: Number,
    pub(crate) stop: Number,
}

impl ComparisonWindow {
    /// The first instant, in model seconds: finite, at most [`Self::stop_s`].
    #[must_use]
    pub fn start_s(&self) -> f64 {
        self.start.seconds()
    }

    /// The last instant, included, in model seconds: finite.
    #[must_use]
    pub fn stop_s(&self) -> f64 {
        self.stop.seconds()
    }
}

/// Where a reference's point mapping is.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PointNameMapping {
    /// A mapping file, by a path inside the spec's directory. A run is handed its bytes and
    /// reads them with [`PointMapping::from_json_bytes`].
    Path(String),
    /// The mapping written inline in the spec, and so inside its `spec_id`.
    Inline(PointMapping),
}

/// One entry of `references`: one sequence to verify against one set of trends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecReference {
    pub(crate) model: String,
    pub(crate) sequence: String,
    pub(crate) point_name_mapping: Option<PointNameMapping>,
    pub(crate) outputs: Option<Vec<OutputOverride>>,
    pub(crate) indicators: Option<Vec<IndicatorRule>>,
    pub(crate) sampling: Option<Number>,
    pub(crate) controller_output: Option<String>,
    pub(crate) cxf: Option<String>,
    pub(crate) parameters: Option<Vec<Parameter>>,
    pub(crate) inputs: Option<Vec<TrendSource>>,
    pub(crate) expected: Option<Vec<TrendSource>>,
    pub(crate) indicator_sources: Option<Vec<TrendSource>>,
    pub(crate) window: Option<ComparisonWindow>,
    pub(crate) envelope: Option<Envelope>,
    pub(crate) funnel_reference: Option<FunnelReference>,
    /// `generateJson` as written, when present: kept for the canonical form, never interpreted.
    pub(crate) generate_json: Option<Json>,
    /// `runController` as written, when present: kept for the canonical form, never interpreted.
    pub(crate) run_controller: Option<Json>,
}

impl SpecReference {
    /// `model`: the Modelica class the reference names. A label only.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// `sequence`: the instance name. Every `outputs` and `indicators` key starts with it and a
    /// `.`. Never empty. It is a label of the spec's own: the engine's root block is never
    /// compared with it (decision 27).
    #[must_use]
    pub fn sequence(&self) -> &str {
        &self.sequence
    }

    /// `pointNameMapping`: a mapping file's path, or the mapping inline; `None` when the
    /// reference gives neither.
    #[must_use]
    pub fn point_name_mapping(&self) -> Option<&PointNameMapping> {
        self.point_name_mapping.as_ref()
    }

    /// `outputs`: the tolerance overrides, in the order the file declares them, which is the
    /// order they apply in. Empty when the reference has no `outputs` key or an empty one; the
    /// canonical form keeps the difference.
    #[must_use]
    pub fn outputs(&self) -> &[OutputOverride] {
        self.outputs.as_deref().unwrap_or_default()
    }

    /// `indicators`: the indicator rules, in the order the file declares them. Empty when the
    /// reference has no `indicators` key or an empty one.
    #[must_use]
    pub fn indicators(&self) -> &[IndicatorRule] {
        self.indicators.as_deref().unwrap_or_default()
    }

    /// The reference's own `sampling` in seconds, when it gives one: finite and above zero.
    /// [`crate::CommissioningSpec::sampling_for`] applies the spec's default.
    #[must_use]
    pub fn sampling_s(&self) -> Option<f64> {
        self.sampling.map(Number::seconds)
    }

    /// `controllerOutput`: the file of the controller's trended outputs, by a path inside the
    /// spec's directory. It is the expected-output source when the reference gives no `expected`
    /// (decision 18).
    #[must_use]
    pub fn controller_output(&self) -> Option<&str> {
        self.controller_output.as_deref()
    }

    /// `cxf`: the sequence's CXF document, by a path inside the spec's directory. A reference
    /// without one loads, and cannot run.
    #[must_use]
    pub fn cxf(&self) -> Option<&str> {
        self.cxf.as_deref()
    }

    /// `parameters`: the parameter edits in CDL units, sorted by path bytes, as written and
    /// unresolved; `None` when the reference has no `parameters` key (an empty map is `Some` and
    /// empty). A path carries no sequence prefix.
    #[must_use]
    pub fn parameters(&self) -> Option<&[Parameter]> {
        self.parameters.as_deref()
    }

    /// `inputs`: the sources of the trended inputs, in file order; `None` when the reference
    /// has no `inputs` key.
    #[must_use]
    pub fn inputs(&self) -> Option<&[TrendSource]> {
        self.inputs.as_deref()
    }

    /// `expected`: the sources of the trended outputs, in file order; `None` when the reference
    /// has no `expected` key, in which case [`Self::controller_output`] is the source.
    #[must_use]
    pub fn expected(&self) -> Option<&[TrendSource]> {
        self.expected.as_deref()
    }

    /// `indicatorSources`: the sources of the indicator columns that are no signal of the
    /// sequence (OBC §12.7.3's `indicator.csv`), in file order; `None` when the reference has no
    /// such key.
    #[must_use]
    pub fn indicator_sources(&self) -> Option<&[TrendSource]> {
        self.indicator_sources.as_deref()
    }

    /// `window`: the comparison window. A reference without one loads, because an LBL spec has
    /// none, and cannot run.
    #[must_use]
    pub fn window(&self) -> Option<&ComparisonWindow> {
        self.window.as_ref()
    }

    /// `envelope`, when the reference writes one; `None` asks for [`Envelope::Funnel`].
    #[must_use]
    pub fn envelope(&self) -> Option<Envelope> {
        self.envelope
    }

    /// `funnelReference`, when the reference writes one; `None` asks for
    /// [`FunnelReference::Engine`].
    #[must_use]
    pub fn funnel_reference(&self) -> Option<FunnelReference> {
        self.funnel_reference
    }
}
