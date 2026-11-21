//! What reading a reference's trend sources gives (commissioning note, "Sources, sampling and
//! time" and "Report and receipt"; issue #660): the merged inputs a replay is driven by, the
//! expected and indicator series a comparison reads, and what the receipt records of the files.

use oce_replay::{
    Column, ColumnKind, ColumnMap, ColumnSpec, ColumnarSource, DuplicateTime, FrameSource,
    InputMap, MapId, MissingPolicy, ReplayLimits, Row, SourceError as ReplaySourceError,
    SourceKind, TimeBase, TimeColumn, TimeOrigin,
};

/// The bytes a commissioning run reads, supplied by the caller: the library opens no file
/// (decision 25).
///
/// `files` pairs each path the reference names, as the spec writes it, with that file's bytes:
/// the point mapping when the reference names a file, each trend source, and the
/// `controllerOutput` when the reference reads it (no `expected`). Every such path is given
/// once, and no other path: a named path with no bytes is `file-missing`, bytes under a path the
/// reference does not name `file-unlisted`, one path given twice `duplicate-file`. The CXF is
/// `cxf`, never an entry of `files`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpecInputs {
    /// The sequence's CXF bytes.
    pub cxf: Vec<u8>,
    /// `(path as the spec writes it, bytes)` for every other file the reference names.
    pub files: Vec<(String, Vec<u8>)>,
}

/// Which list of a reference a trend source is declared in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceRole {
    /// `inputs`: the trended inputs, merged into the frames of the replay.
    Inputs,
    /// `expected`, or the `controllerOutput` read in its place: the trended outputs.
    Expected,
    /// `indicatorSources`: indicator columns that are no signal of the sequence.
    Indicators,
}

impl SourceRole {
    /// The role's spelling in a receipt's `sources[].role`: `inputs`, `expected` or
    /// `indicators`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inputs => "inputs",
            Self::Expected => "expected",
            Self::Indicators => "indicators",
        }
    }
}

/// What the receipt records of one trend source (its `sources[]` row).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRecord {
    /// The list the source is declared in.
    pub role: SourceRole,
    /// The file's path, as the spec writes it.
    pub path: String,
    /// The matrix read, when the source names one.
    pub table: Option<String>,
    /// `fnv1a128:<32 hex>` of the file's bytes.
    pub digest: String,
    /// The data rows of the table read: for `csv` the rows after the header row, for a
    /// `combiTimeTable` the rows of the matrix read. Header, comment and blank lines are not
    /// rows.
    pub rows: u64,
}

/// What the receipt records of a point-mapping file (its `mapping` object).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingRecord {
    /// The file's path, as the spec writes it.
    pub path: String,
    /// `fnv1a128:<32 hex>` of the file's bytes.
    pub digest: String,
}

/// One converted point (a `units.columns` row of the receipt): a point a source reads whose
/// mapping gives a unit on both sides, identity pairs included.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnitColumn {
    /// The device point's name.
    pub device: String,
    /// The mapping's `cdl.name`, as written.
    pub cdl: String,
    /// The device's unit.
    pub from: String,
    /// The CDL point's unit.
    pub to: String,
}

/// The values of one series, of the point's CDL type.
#[derive(Clone, Debug, PartialEq)]
pub enum SeriesValues {
    /// A CDL Real, in the CDL unit: finite.
    Real(Vec<f64>),
    /// A CDL Integer.
    Integer(Vec<i32>),
    /// A CDL Boolean.
    Boolean(Vec<bool>),
}

impl SeriesValues {
    /// The number of samples.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Real(values) => values.len(),
            Self::Integer(values) => values.len(),
            Self::Boolean(values) => values.len(),
        }
    }

    /// Whether there is no sample.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One expected or indicator point as a time series: its samples in model seconds, converted
/// to the CDL point's unit and type. Only samples are held: a missing cell is no sample, so
/// every value is finite. Times are in file order, never decreasing; equal times are kept, each
/// its own sample.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct TrendSeries {
    /// The mapping's `cdl.name`, as written: unresolved (decision 27). For an indicator source
    /// it is the indicator's name.
    pub name: String,
    /// The device point's name.
    pub device: String,
    /// The sample times, in model seconds.
    pub times: Vec<f64>,
    /// The values, one per time.
    pub values: SeriesValues,
}

/// One merged input column: one CDL point an `inputs` source reads, at every frame.
#[derive(Clone, Debug)]
pub struct InputColumn {
    pub(crate) name: String,
    pub(crate) device: String,
    pub(crate) values: Column,
    pub(crate) hold_max_age_s: Option<f64>,
}

impl InputColumn {
    /// The column's name: the mapping's `cdl.name`, as written. It is also the target replay
    /// resolves against the engine's inputs ([`TrendInputs::column_map`]).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The device point the column was read from.
    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    /// The kind: Real for a CDL `float`, Integer for `int`, Boolean for `bool`.
    #[must_use]
    pub fn kind(&self) -> ColumnKind {
        match self.values {
            Column::Integer { .. } => ColumnKind::Integer,
            Column::Boolean { .. } => ColumnKind::Boolean,
            // The reader builds no Enum column.
            Column::Real(_) | Column::Enum { .. } => ColumnKind::Real,
        }
    }

    /// The values, one per frame: a Real column holds NaN where its file has no sample at the
    /// frame, an Integer or Boolean column a cleared validity flag (always present).
    #[must_use]
    pub fn values(&self) -> &Column {
        &self.values
    }

    /// The `hold.maxAge` of the source the column was read from, in seconds, when it declares
    /// one.
    #[must_use]
    pub fn hold_max_age_s(&self) -> Option<f64> {
        self.hold_max_age_s
    }
}

/// The input sources of a reference merged on the union of their times (decision 15): one frame
/// per instant of the union, and as many at one instant as the source with the most rows there
/// has; a source with no row at a frame is missing there.
#[derive(Clone, Debug)]
pub struct TrendInputs {
    pub(crate) times: Vec<f64>,
    pub(crate) columns: Vec<InputColumn>,
    pub(crate) origin: Option<TimeOrigin>,
}

impl TrendInputs {
    /// The frame times, in model seconds: never decreasing, equal times kept, `-0.0` read as
    /// `0.0`.
    #[must_use]
    pub fn times(&self) -> &[f64] {
        &self.times
    }

    /// The columns, sorted by name bytes.
    #[must_use]
    pub fn columns(&self) -> &[InputColumn] {
        &self.columns
    }

    /// One spec per column, for [`InputMap::resolve`].
    #[must_use]
    pub fn column_specs(&self) -> Vec<ColumnSpec> {
        self.columns
            .iter()
            .map(|column| ColumnSpec::new(column.name.clone(), column.kind()))
            .collect()
    }

    /// Each column to the input of its name, by replay's name rule: the mapping's `cdl.name`
    /// carries no sequence prefix (decision 27).
    #[must_use]
    pub fn column_map(&self) -> ColumnMap {
        ColumnMap {
            entries: self
                .columns
                .iter()
                .map(|column| (column.name.clone(), column.name.clone()))
                .collect(),
        }
    }

    /// The run-level missing-sample policy: `Refuse`, so a frame with a missing sample of a
    /// source that declares no hold is refused (decision 15).
    #[must_use]
    pub fn missing_policy(&self) -> MissingPolicy {
        MissingPolicy::Refuse
    }

    /// One `HoldLast { max_age }` override per column of a source that declares `hold.maxAge`,
    /// by column name, sorted by name.
    #[must_use]
    pub fn overrides(&self) -> Vec<(String, MissingPolicy)> {
        self.columns
            .iter()
            .filter_map(|column| {
                column
                    .hold_max_age_s
                    .map(|max_age| (column.name.clone(), MissingPolicy::HoldLast { max_age }))
            })
            .collect()
    }

    /// The time base a commissioning replay runs on: the recorded times, unchanged.
    #[must_use]
    pub fn time_base(&self) -> TimeBase {
        TimeBase::Recorded
    }

    /// Equal times are allowed, each row its own frame: OBC §12.2 allows "two equal time stamps
    /// to indicate when a value switches".
    #[must_use]
    pub fn duplicate_time(&self) -> DuplicateTime {
        DuplicateTime::Allow
    }

    /// The spec's `time.origin`, carried once as metadata.
    #[must_use]
    pub fn origin(&self) -> Option<&TimeOrigin> {
        self.origin.as_ref()
    }

    /// The merged inputs as a frame source bound to `map`: a [`ColumnarSource`] that also
    /// reports the origin.
    ///
    /// # Errors
    /// The [`ColumnarSource::new`] refusals: a column `map` does not use or a mapped column not
    /// given, a kind the map does not expect, and the caps of `limits`.
    pub fn into_source(
        self,
        map: &InputMap,
        limits: &ReplayLimits,
    ) -> Result<TrendFrameSource, ReplaySourceError> {
        let columns = self
            .columns
            .into_iter()
            .map(|column| (column.name, column.values))
            .collect();
        Ok(TrendFrameSource {
            inner: ColumnarSource::new(map, TimeColumn::Seconds(self.times), columns, limits)?,
            origin: self.origin,
        })
    }
}

/// The merged inputs bound to an input map: a [`ColumnarSource`] with the spec's origin.
#[derive(Debug)]
pub struct TrendFrameSource {
    inner: ColumnarSource,
    origin: Option<TimeOrigin>,
}

impl TrendFrameSource {
    /// The number of frames.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.inner.rows()
    }
}

impl FrameSource for TrendFrameSource {
    fn map_id(&self) -> &MapId {
        self.inner.map_id()
    }

    fn kind(&self) -> SourceKind {
        self.inner.kind()
    }

    fn origin(&self) -> Option<&TimeOrigin> {
        self.origin.as_ref()
    }

    fn next_row(&mut self) -> Result<Option<Row<'_>>, ReplaySourceError> {
        self.inner.next_row()
    }
}

/// Everything read from one reference's trend sources.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct TrendSources {
    /// The `inputs` sources, merged into frames.
    pub inputs: TrendInputs,
    /// The `expected` series (or the `controllerOutput`'s), one per point, sorted by name.
    pub expected: Vec<TrendSeries>,
    /// The `indicatorSources` series, one per point, sorted by name.
    pub indicators: Vec<TrendSeries>,
    /// One record per trend source: `inputs`, `expected` (or the `controllerOutput`) and
    /// `indicatorSources`, in that order and in declaration order within each. Complete.
    pub sources: Vec<SourceRecord>,
    /// The mapping file, when the reference names one.
    pub mapping: Option<MappingRecord>,
    /// One row per point read whose mapping gives a unit on both sides, sorted by `device`
    /// bytes. Complete: the receipt caps it.
    pub unit_columns: Vec<UnitColumn>,
}
