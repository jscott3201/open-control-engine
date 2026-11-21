//! Why a trend source, or the files a commissioning run was handed, were refused (commissioning
//! note, "Point mapping" and "Sources, sampling and time"; issue #660).
//!
//! A [`SourceError`] is one closed [`SourceErrorKind`] with a stable kebab-case code, the place it
//! was found (the file as the spec writes its path, the data row counted from 1, the column as the
//! spec addresses it, and for a `combiTimeTable` the line of the file), the code of a lower
//! layer's refusal when one caused it ([`SourceError::cause`]: a [`crate::UnitError`] code, a
//! point-mapping [`crate::SpecError`] code or a cell rule's code), and a sentence for people. The
//! code is the contract; [`SOURCE_ERROR_CODES`] lists every code in declaration order and a test
//! pins it. Every text the error carries is printable ASCII and bounded
//! ([`MAX_SPEC_DETAIL_BYTES`]), whatever the file held.

use std::fmt;

use super::error::{MAX_SPEC_DETAIL_BYTES, printable};

/// Every [`SourceErrorKind`] code, in declaration order; a test pins the list.
pub const SOURCE_ERROR_CODES: &[&str] = &[
    "reference-out-of-range",
    "too-many-sources",
    "duplicate-file",
    "file-unlisted",
    "file-missing",
    "mapping-required",
    "mapping-refused",
    "column-reference-invalid",
    "unmapped-point",
    "point-read-twice",
    "type-pair-refused",
    "unit-refused",
    "unit-on-discrete-point",
    "format-tag-missing",
    "table-syntax",
    "table-required",
    "table-not-found",
    "duplicate-table",
    "size-mismatch",
    "bad-number",
    "non-finite-number",
    "bad-cell",
    "cell-refused",
    "column-missing",
    "duplicate-column",
    "no-header",
    "no-point-columns",
    "ragged-row",
    "not-utf8",
    "cell-too-long",
    "too-many-rows",
    "too-many-columns",
    "too-many-bytes",
    "time-missing",
    "bad-time",
    "time-origin-required",
    "time-regression",
    "unreadable",
];

/// What was refused. Each kind has one stable code ([`SourceErrorKind::code`]);
/// `#[non_exhaustive]`, so consumers branch on the code.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceErrorKind {
    /// The reference index is past the end of the spec's `references`.
    ReferenceOutOfRange,
    /// The reference names more trend sources than [`crate::TrendLimits::max_sources`].
    TooManySources,
    /// The run was handed the bytes of one path twice.
    DuplicateFile,
    /// The run was handed the bytes of a path the reference does not name.
    FileUnlisted,
    /// The reference names a file (its mapping or a trend source) and no bytes were handed in.
    FileMissing,
    /// The reference reads trend sources and has no `pointNameMapping`.
    MappingRequired,
    /// The point-mapping file does not load; the cause is its [`crate::SpecError`] code.
    MappingRefused,
    /// A source addresses its file in a way its format does not take: a `timeColumn` or a
    /// `columns` key of the wrong kind, a position that is not a canonical decimal from 1, a
    /// `table` on a `csv` source, or the time column also mapped to a point.
    ColumnReferenceInvalid,
    /// A column names a device point the mapping does not have.
    UnmappedPoint,
    /// Two columns of one role (two columns of one source, or of two sources of the role) name one
    /// device point.
    PointReadTwice,
    /// The mapping gives the point a device type and a CDL type no rule converts between.
    TypePairRefused,
    /// The point's two units have no conversion, or a reading's conversion overflows; the cause
    /// is the [`crate::UnitError`] code.
    UnitRefused,
    /// A unit conversion other than the identity on a point whose CDL type is `int` or `bool`.
    UnitOnDiscretePoint,
    /// A `combiTimeTable` file does not start with `#1`.
    FormatTagMissing,
    /// A `combiTimeTable` file is not the format: a matrix header that is not
    /// `double <name>(<rows>,<cols>)`, an empty cell, a separator after the last value, or
    /// anything else outside a number, a separator or a comment.
    TableSyntax,
    /// A `combiTimeTable` file holds more than one matrix and the source names no `table`.
    TableRequired,
    /// No matrix of the file has the name the source's `table` gives.
    TableNotFound,
    /// Two matrices of the file have the name the source's `table` gives.
    DuplicateTable,
    /// A matrix holds fewer or more values than its declared rows times columns.
    SizeMismatch,
    /// A `combiTimeTable` value that is not a decimal number in C syntax.
    BadNumber,
    /// A number that is not finite: an infinity in a `csv` cell or time, or a `combiTimeTable`
    /// value whose magnitude is beyond the largest `f64`.
    NonFiniteNumber,
    /// A `csv` cell its column's device type does not read: not a number in a `float` or `int`
    /// column, not `true`, `false`, `1` or `0` in a `bool` column.
    BadCell,
    /// A number or a Boolean the point's type rule refuses; the cause is the rule's code
    /// (`not-boolean`, `not-integral`, `outside-int32`, …).
    CellRefused,
    /// A column the source's map names, or its time column, is not in the file.
    ColumnMissing,
    /// A `csv` header names one column twice.
    DuplicateColumn,
    /// A `csv` file read without a `timeColumn` (a `controllerOutput`) has no header row.
    NoHeader,
    /// A `controllerOutput` file whose header names no column but the time.
    NoPointColumns,
    /// A `csv` row with a different number of fields from the header.
    RaggedRow,
    /// A `csv` cell or header that is not UTF-8.
    NotUtf8,
    /// A `csv` cell, or a `combiTimeTable` number or matrix name, longer than
    /// [`oce_replay::ReplayLimits::max_cell_bytes`].
    CellTooLong,
    /// More rows than [`oce_replay::ReplayLimits::max_rows`]: in one file, or in the frames the
    /// input sources merge into.
    TooManyRows,
    /// More columns than [`oce_replay::ReplayLimits::max_columns`], the time column included: in
    /// one file, or in the merged inputs.
    TooManyColumns,
    /// More bytes than [`oce_replay::ReplayLimits::max_source_bytes`]: in one file, or held by
    /// the columns a source or the merged inputs keep.
    TooManyBytes,
    /// A `csv` row whose time cell is empty, `NA` or `NaN`.
    TimeMissing,
    /// A time cell that is neither a number of seconds nor an instant, or not the kind of the
    /// first row's.
    BadTime,
    /// A `csv` time column of RFC 3339 instants in a spec that gives no `time.origin`.
    TimeOriginRequired,
    /// A time earlier than the row before it, in one file.
    TimeRegression,
    /// A refusal of the replay reader this crate has no word of its own for.
    Unreadable,
}

impl SourceErrorKind {
    /// The stable kebab-case code of this kind, one of [`SOURCE_ERROR_CODES`].
    #[must_use]
    pub const fn code(self) -> &'static str {
        SOURCE_ERROR_CODES[self.declared_index()]
    }

    /// The position of the variant in the declaration. The match is exhaustive, so a new variant
    /// does not compile until it has a row here, and [`SOURCE_ERROR_CODES`] a code at that
    /// position (a test holds the two the same length).
    const fn declared_index(self) -> usize {
        match self {
            Self::ReferenceOutOfRange => 0,
            Self::TooManySources => 1,
            Self::DuplicateFile => 2,
            Self::FileUnlisted => 3,
            Self::FileMissing => 4,
            Self::MappingRequired => 5,
            Self::MappingRefused => 6,
            Self::ColumnReferenceInvalid => 7,
            Self::UnmappedPoint => 8,
            Self::PointReadTwice => 9,
            Self::TypePairRefused => 10,
            Self::UnitRefused => 11,
            Self::UnitOnDiscretePoint => 12,
            Self::FormatTagMissing => 13,
            Self::TableSyntax => 14,
            Self::TableRequired => 15,
            Self::TableNotFound => 16,
            Self::DuplicateTable => 17,
            Self::SizeMismatch => 18,
            Self::BadNumber => 19,
            Self::NonFiniteNumber => 20,
            Self::BadCell => 21,
            Self::CellRefused => 22,
            Self::ColumnMissing => 23,
            Self::DuplicateColumn => 24,
            Self::NoHeader => 25,
            Self::NoPointColumns => 26,
            Self::RaggedRow => 27,
            Self::NotUtf8 => 28,
            Self::CellTooLong => 29,
            Self::TooManyRows => 30,
            Self::TooManyColumns => 31,
            Self::TooManyBytes => 32,
            Self::TimeMissing => 33,
            Self::BadTime => 34,
            Self::TimeOriginRequired => 35,
            Self::TimeRegression => 36,
            Self::Unreadable => 37,
        }
    }
}

/// A refused trend source, or a refused set of files: the kind, where it was found, the lower
/// layer's code that caused it, and a sentence. Never a panic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceError {
    kind: SourceErrorKind,
    /// Boxed, so a `Result` that carries one stays two words wide.
    place: Box<Place>,
}

/// Where a refusal was found, its cause and its sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Place {
    file: Option<String>,
    line: Option<u64>,
    row: Option<u64>,
    column: Option<String>,
    cause: Option<&'static str>,
    detail: String,
}

impl SourceError {
    /// A refusal with a sentence and no place yet.
    pub(crate) fn new(kind: SourceErrorKind, detail: impl AsRef<str>) -> Self {
        Self {
            kind,
            place: Box::new(Place {
                file: None,
                line: None,
                row: None,
                column: None,
                cause: None,
                detail: printable(detail.as_ref(), MAX_SPEC_DETAIL_BYTES),
            }),
        }
    }

    /// This refusal in the file the spec names `path`.
    #[must_use]
    pub(crate) fn in_file(mut self, path: &str) -> Self {
        self.place.file = Some(printable(path, MAX_SPEC_DETAIL_BYTES));
        self
    }

    /// This refusal at line `line` of the file, counted from 1.
    #[must_use]
    pub(crate) fn at_line(mut self, line: u64) -> Self {
        self.place.line = Some(line);
        self
    }

    /// This refusal at data row `row`, counted from 1.
    #[must_use]
    pub(crate) fn at_row(mut self, row: u64) -> Self {
        self.place.row = Some(row);
        self
    }

    /// This refusal in the column the spec addresses as `column`.
    #[must_use]
    pub(crate) fn at_column(mut self, column: &str) -> Self {
        self.place.column = Some(printable(column, MAX_SPEC_DETAIL_BYTES));
        self
    }

    /// This refusal, caused by a lower layer's refusal of code `cause`.
    #[must_use]
    pub(crate) fn caused_by(mut self, cause: &'static str) -> Self {
        self.place.cause = Some(cause);
        self
    }

    /// The closed kind.
    #[must_use]
    pub fn kind(&self) -> SourceErrorKind {
        self.kind
    }

    /// The kind's stable code ([`SourceErrorKind::code`]).
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// The file, as the spec writes its path, when the refusal is about one.
    #[must_use]
    pub fn file(&self) -> Option<&str> {
        self.place.file.as_deref()
    }

    /// The line of a `combiTimeTable` file the refusal was found on, counted from 1.
    #[must_use]
    pub fn line(&self) -> Option<u64> {
        self.place.line
    }

    /// The data row, counted from 1: for `csv` the rows after the header row, for a
    /// `combiTimeTable` the rows of the matrix. `None` for a refusal of no one row.
    #[must_use]
    pub fn row(&self) -> Option<u64> {
        self.place.row
    }

    /// The column, as the spec addresses it: a header name for `csv`, a position counted from 1
    /// for a `combiTimeTable`.
    #[must_use]
    pub fn column(&self) -> Option<&str> {
        self.place.column.as_deref()
    }

    /// The code of the lower layer's refusal that caused this one: a [`crate::UnitError`] code
    /// under `unit-refused`, a [`crate::SpecError`] code under `mapping-refused`, a cell rule's
    /// code under `cell-refused` and `type-pair-refused`.
    #[must_use]
    pub fn cause(&self) -> Option<&'static str> {
        self.place.cause
    }

    /// The sentence for people: at most [`MAX_SPEC_DETAIL_BYTES`] bytes of printable ASCII. Not a
    /// contract: branch on [`Self::code`].
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.place.detail
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.place.file {
            write!(f, "`{file}`")?;
            if let Some(line) = self.place.line {
                write!(f, " line {line}")?;
            }
            if let Some(row) = self.place.row {
                write!(f, " row {row}")?;
            }
            if let Some(column) = &self.place.column {
                write!(f, " column `{column}`")?;
            }
            f.write_str(": ")?;
        }
        f.write_str(&self.place.detail)
    }
}

impl std::error::Error for SourceError {}

#[cfg(test)]
#[path = "source_error_tests.rs"]
mod tests;
