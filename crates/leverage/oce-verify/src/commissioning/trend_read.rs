//! Reading one trend source's file into converted columns (issue #660).
//!
//! A `csv` file is read by replay's own [`CsvSource`], under replay's caps, never by a second CSV
//! reader: a synthetic input map with one Real input per `float` or `int` device column hands
//! each of those to it by header name, and each `bool` device column is one of its text columns
//! (issue #728), handed over unparsed. Replay trims the spaces and tabs around every unquoted
//! cell and header name first (issue #733), so OBC §12.7.3's padded files read. So a number is
//! read by Rust's `f64` grammar (correctly rounded; `inf` and `infinity` are values, `nan` is
//! missing), a Boolean by [`boolean_from_text`] (`true`, `false`, `active`, `inactive`, `1` or
//! `0`), and an empty cell, `NA` or `NaN` is missing. A `combiTimeTable` is read by
//! [`read_matrix`].
//!
//! Then every cell is converted exactly once: an infinity is refused (`non-finite-number`; the
//! owner's ruling of 2026-11-21), the point's [`CellRule`] gives a value of its CDL type, and
//! for a Real the point's [`UnitConversion`](super::units::UnitConversion) puts it in the CDL unit. A missing cell stays
//! missing. Each refusal names the file, the data row and the column.
//!
//! **Time.** A `combiTimeTable`'s time column is seconds. A `csv` time column is seconds or RFC
//! 3339 instants with an explicit offset, fixed for the whole column by its first data row's
//! cell (the owner's ruling of 2026-11-21): the two grammars share no text, and a later cell of
//! the other kind is `bad-time`. Instants become model seconds from the spec's `time.origin` by
//! replay's fixed split; with no origin they are `time-origin-required`. A `controllerOutput`'s
//! time column, its first header, is seconds only (OBC §12.7.3). Times never decrease within a
//! file (`time-regression`); equal times are kept; `-0.0` is read as `0.0`.

use oce_api::{InputDefinition, Value, ValueType};
use oce_replay::{
    ColumnKind, ColumnMap, ColumnSpec, CsvFormat, CsvSource, CsvTime, FrameSource, InputMap,
    MissingPolicy, ReplayLimits, SourceError as ReplaySourceError, TimeOrigin,
};

use super::choices::{PointType, TrendFormat};
use super::combi_time_table::read_matrix;
use super::source_error::{SourceError, SourceErrorKind as Kind};
use super::trend_plan::{Address, PlannedColumn, PlannedSource, TimeAddress};
use super::type_rules::{
    CellRule, boolean_from_number, boolean_from_text, integer_from_boolean, integer_from_number,
    real_from_whole_number,
};

/// The converted values of one column, one per data row of its file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Values {
    /// A CDL Real; NaN where the cell is missing (a converted cell is never NaN).
    Real(Vec<f64>),
    /// A CDL Integer; `None` where the cell is missing.
    Integer(Vec<Option<i32>>),
    /// A CDL Boolean; `None` where the cell is missing.
    Boolean(Vec<Option<bool>>),
}

impl Values {
    fn for_type(cdl: PointType, capacity: usize) -> Self {
        match cdl {
            PointType::Float => Self::Real(Vec::with_capacity(capacity)),
            PointType::Int => Self::Integer(Vec::with_capacity(capacity)),
            PointType::Bool => Self::Boolean(Vec::with_capacity(capacity)),
        }
    }

    fn shrink_to_fit(&mut self) {
        match self {
            Self::Real(values) => values.shrink_to_fit(),
            Self::Integer(values) => values.shrink_to_fit(),
            Self::Boolean(values) => values.shrink_to_fit(),
        }
    }

    /// The bytes a merged column of this type holds per frame, as a [`oce_replay::Column`]
    /// counts them: 8 per Real or Integer value, 1 per Boolean, 1 per validity flag.
    pub(crate) fn frame_bytes(cdl: PointType) -> u64 {
        match cdl {
            PointType::Float => 8,
            PointType::Int => 9,
            PointType::Bool => 2,
        }
    }
}

/// One file, read and converted.
#[derive(Clone, Debug)]
pub(crate) struct ReadSource {
    pub(crate) times: Vec<f64>,
    /// One per planned column, in plan order.
    pub(crate) columns: Vec<Values>,
    pub(crate) rows: u64,
}

/// A cell as the file gives it.
enum Raw<'a> {
    Missing,
    Number(f64),
    /// A `csv` cell of a `bool` device column, trimmed of its padding.
    Text(&'a str),
}

/// The bytes one row of a source keeps: its time and each column.
fn row_bytes(columns: &[PlannedColumn]) -> u64 {
    8 + columns
        .iter()
        .map(|column| Values::frame_bytes(column.cdl_type))
        .sum::<u64>()
}

fn too_many_bytes(rows: u64, limits: &ReplayLimits) -> SourceError {
    SourceError::new(
        Kind::TooManyBytes,
        format!(
            "the values read from {rows} rows take more than {} bytes",
            limits.max_source_bytes
        ),
    )
}

/// Convert one cell of `column` and append it.
fn push(column: &PlannedColumn, raw: Raw<'_>, into: &mut Values) -> Result<(), SourceError> {
    let refused = |kind: Kind, detail: String, cause: Option<&'static str>| {
        let error = SourceError::new(kind, detail);
        match cause {
            Some(cause) => error.caused_by(cause),
            None => error,
        }
    };
    let cell = |error: super::type_rules::CellError| {
        refused(Kind::CellRefused, error.to_string(), Some(error.code()))
    };
    if let Raw::Number(x) = raw
        && x.is_infinite()
    {
        return Err(refused(
            Kind::NonFiniteNumber,
            format!("{x:?} is not a finite reading"),
            None,
        ));
    }
    match (into, raw) {
        (Values::Real(values), Raw::Missing) => values.push(f64::NAN),
        (Values::Integer(values), Raw::Missing) => values.push(None),
        (Values::Boolean(values), Raw::Missing) => values.push(None),
        (Values::Real(values), Raw::Number(x)) => {
            let read = match column.rule {
                CellRule::IntegerAsReal => real_from_whole_number(x).map_err(cell)?,
                _ => x,
            };
            let converted = column.unit.apply(read).map_err(|error| {
                refused(Kind::UnitRefused, error.to_string(), Some(error.code()))
            })?;
            values.push(converted);
        }
        (Values::Integer(values), Raw::Number(x)) => values.push(Some(match column.rule {
            CellRule::BooleanAsInteger => {
                integer_from_boolean(boolean_from_number(x).map_err(cell)?)
            }
            _ => integer_from_number(x).map_err(cell)?,
        })),
        (Values::Integer(values), Raw::Text(text)) => {
            values.push(Some(integer_from_boolean(
                boolean_from_text(text).map_err(cell)?,
            )));
        }
        (Values::Boolean(values), Raw::Number(x)) => {
            values.push(Some(boolean_from_number(x).map_err(cell)?));
        }
        (Values::Boolean(values), Raw::Text(text)) => {
            values.push(Some(boolean_from_text(text).map_err(cell)?));
        }
        // A text cell comes only from a `bool` device column, whose CDL side is never a Real
        // (`type-pair-refused` at planning).
        (Values::Real(_), Raw::Text(_)) => {
            return Err(refused(
                Kind::BadCell,
                "a Boolean cell in a Real column".to_owned(),
                None,
            ));
        }
    }
    Ok(())
}

/// Read `source` from `bytes`.
///
/// # Errors
/// A [`SourceError`] naming the file and, for a cell, its row and column.
pub(crate) fn read_source(
    source: &PlannedSource,
    bytes: &[u8],
    origin: Option<&TimeOrigin>,
    limits: &ReplayLimits,
) -> Result<ReadSource, SourceError> {
    let read = match (source.format, &source.time) {
        (TrendFormat::CombiTimeTable, TimeAddress::Position(time)) => {
            read_combi(source, *time, bytes, limits)
        }
        (_, TimeAddress::Name(time)) => read_csv(source, time, bytes, origin, limits),
        (TrendFormat::Csv, TimeAddress::Position(_)) => Err(SourceError::new(
            Kind::ColumnReferenceInvalid,
            "a csv source gives its `timeColumn` as a header name",
        )),
    };
    read.map_err(|error| error.in_file(&source.path))
}

fn read_combi(
    source: &PlannedSource,
    time: u32,
    bytes: &[u8],
    limits: &ReplayLimits,
) -> Result<ReadSource, SourceError> {
    let mut keep = vec![time];
    for column in &source.columns {
        match column.address {
            Address::Position(position) => keep.push(position),
            Address::Name(_) => {
                return Err(SourceError::new(
                    Kind::ColumnReferenceInvalid,
                    "a combiTimeTable column is a position",
                ));
            }
        }
    }
    let matrix = read_matrix(bytes, source.table.as_deref(), &keep, limits)?;
    if u128::from(matrix.rows) * u128::from(row_bytes(&source.columns))
        > u128::from(limits.max_source_bytes)
    {
        return Err(too_many_bytes(matrix.rows, limits));
    }
    let mut columns = matrix.columns.into_iter();
    let times = columns.next().unwrap_or_default();
    let rows = times.len();
    let mut checked = Vec::with_capacity(rows);
    let time_label = time.to_string();
    for (index, t) in times.into_iter().enumerate() {
        checked.push(next_time(&checked, t, index as u64 + 1, &time_label)?);
    }
    let mut converted = Vec::with_capacity(source.columns.len());
    for (column, raw) in source.columns.iter().zip(columns) {
        let mut values = Values::for_type(column.cdl_type, rows);
        for (index, x) in raw.into_iter().enumerate() {
            push(column, Raw::Number(x), &mut values).map_err(|error| {
                error
                    .at_row(index as u64 + 1)
                    .at_column(&column.address.label())
            })?;
        }
        converted.push(values);
    }
    Ok(ReadSource {
        times: checked,
        columns: converted,
        rows: matrix.rows,
    })
}

/// `t` as the next time after `times`: finite, `-0.0` as `0.0`, never below the previous.
fn next_time(times: &[f64], t: f64, row: u64, column: &str) -> Result<f64, SourceError> {
    let place = |error: SourceError| error.at_row(row).at_column(column);
    if !t.is_finite() {
        return Err(place(SourceError::new(
            Kind::NonFiniteNumber,
            format!("the time {t:?} is not finite"),
        )));
    }
    // `-0.0 + 0.0` is `0.0`; every other value is unchanged.
    let t = t + 0.0;
    if times.last().is_some_and(|&previous| t < previous) {
        return Err(place(SourceError::new(
            Kind::TimeRegression,
            format!("the time {t:?} is earlier than the row before it"),
        )));
    }
    Ok(t)
}

/// Where [`CsvSource`] hands one planned column of a `csv` file over.
#[derive(Clone, Copy)]
enum CsvCell {
    /// A Real input of the synthetic map, at this slot.
    Slot(usize),
    /// The next of its text cells, in plan order.
    Text,
}

/// How a `csv` file is handed to [`CsvSource`]: the synthetic input map with one Real input per
/// planned `float` or `int` device column, the header names of the `bool` device columns, read
/// as text, and where each planned column is read, in plan order.
struct CsvPlan {
    map: InputMap,
    texts: Vec<String>,
    cells: Vec<CsvCell>,
}

/// The [`CsvPlan`] of `columns`.
fn csv_plan(columns: &[PlannedColumn]) -> Result<CsvPlan, SourceError> {
    let mut inputs = Vec::with_capacity(columns.len());
    let mut specs = Vec::with_capacity(columns.len());
    let mut map = ColumnMap::default();
    let mut texts = Vec::new();
    let mut cells = Vec::with_capacity(columns.len());
    for (index, column) in columns.iter().enumerate() {
        let Address::Name(name) = &column.address else {
            return Err(SourceError::new(
                Kind::ColumnReferenceInvalid,
                "a csv column is a header name",
            ));
        };
        if column.device_type == PointType::Bool {
            texts.push(name.clone());
            cells.push(CsvCell::Text);
            continue;
        }
        // Zero-padded, so the paths are in canonical order, and slot order is plan order; a
        // file has at most 4,096 columns.
        let path = format!("c{index:05}");
        cells.push(CsvCell::Slot(inputs.len()));
        inputs.push(InputDefinition {
            path: path.clone(),
            value_type: ValueType::Real,
            min: None,
            max: None,
        });
        specs.push(ColumnSpec::new(name.clone(), ColumnKind::Real));
        map.entries.push((name.clone(), path));
    }
    let map = InputMap::resolve(&inputs, &specs, &map, &MissingPolicy::Refuse, &[])
        .map_err(|error| SourceError::new(Kind::Unreadable, error.to_string()))?;
    Ok(CsvPlan { map, texts, cells })
}

/// A replay refusal of a `csv` file, in this crate's words.
fn from_replay(error: &ReplaySourceError, time: &str) -> SourceError {
    use ReplaySourceError as R;
    let detail = error.to_string();
    match error {
        R::TooManyRows { .. } => SourceError::new(Kind::TooManyRows, detail),
        R::TooManyColumns { .. } => SourceError::new(Kind::TooManyColumns, detail),
        R::TooManyBytes { .. } => SourceError::new(Kind::TooManyBytes, detail),
        R::CellTooLong { row, field, .. } => {
            let error = SourceError::new(Kind::CellTooLong, detail).at_column(&field_label(*field));
            if *row == 0 { error } else { error.at_row(*row) }
        }
        R::MissingColumn { column } => {
            SourceError::new(Kind::ColumnMissing, detail).at_column(column)
        }
        R::DuplicateColumn { column } => {
            SourceError::new(Kind::DuplicateColumn, detail).at_column(column)
        }
        R::Ragged { row } => SourceError::new(Kind::RaggedRow, detail).at_row(*row),
        R::NotUtf8 { row, field } => {
            let error = SourceError::new(Kind::NotUtf8, detail).at_column(&field_label(*field));
            if *row == 0 { error } else { error.at_row(*row) }
        }
        R::BadCell { row, column } if column == time => SourceError::new(Kind::BadTime, detail)
            .at_row(*row)
            .at_column(column),
        R::BadCell { row, column } => SourceError::new(Kind::BadCell, detail)
            .at_row(*row)
            .at_column(column),
        R::MissingTime { row } => SourceError::new(Kind::TimeMissing, detail)
            .at_row(*row)
            .at_column(time),
        R::NonFiniteTime { row } => SourceError::new(Kind::NonFiniteNumber, detail)
            .at_row(*row)
            .at_column(time),
        R::BadInstant { row } => SourceError::new(Kind::BadTime, detail)
            .at_row(*row)
            .at_column(time),
        _ => SourceError::new(Kind::Unreadable, detail),
    }
}

/// A field the reader cannot name, by its position from 1 in the record.
fn field_label(field: usize) -> String {
    format!("#{}", field + 1)
}

/// The origin a time column of instants is first read against when the spec gives none: a row
/// that converts against it is an instant, and is refused as `time-origin-required`.
const PROBE_ORIGIN: &str = "1970-01-01T00:00:00Z";

fn csv_source<'b>(
    map: &InputMap,
    texts: &[String],
    bytes: &'b [u8],
    time: &str,
    kind: CsvTime,
    limits: &ReplayLimits,
) -> Result<CsvSource<&'b [u8]>, SourceError> {
    let format = CsvFormat {
        time: kind,
        text_columns: texts.to_vec(),
        ..CsvFormat::seconds(time)
    };
    CsvSource::new(map, bytes, format, limits).map_err(|error| from_replay(&error, time))
}

/// How the time column of a `csv` file is written, from its first data row.
///
/// # Errors
/// `time-origin-required` for a column of instants in a spec with no origin; the header's
/// refusals.
fn time_kind(
    source: &PlannedSource,
    plan: &CsvPlan,
    bytes: &[u8],
    time: &str,
    origin: Option<&TimeOrigin>,
    limits: &ReplayLimits,
) -> Result<CsvTime, SourceError> {
    let source_of = |kind| csv_source(&plan.map, &plan.texts, bytes, time, kind, limits);
    let mut seconds = source_of(CsvTime::Seconds)?;
    let not_seconds = match seconds.next_row() {
        Err(ReplaySourceError::BadCell { row: 1, column }) => column == time,
        _ => false,
    };
    if !not_seconds || source.seconds_only {
        return Ok(CsvTime::Seconds);
    }
    if let Some(origin) = origin {
        return Ok(CsvTime::Instant {
            origin: origin.clone(),
        });
    }
    let probe = TimeOrigin::parse(PROBE_ORIGIN)
        .map_err(|error| SourceError::new(Kind::Unreadable, error.to_string()))?;
    let mut instants = source_of(CsvTime::Instant { origin: probe })?;
    // Replay reads a row's time before its cells, so a cell it refuses after the time means
    // the time was read as an instant.
    let instant = match instants.next_row() {
        Ok(Some(_)) => true,
        Err(ReplaySourceError::BadCell { row: 1, column }) => column != time,
        _ => false,
    };
    if instant {
        return Err(SourceError::new(
            Kind::TimeOriginRequired,
            "the time column holds instants and the spec gives no `time.origin`",
        )
        .at_row(1)
        .at_column(time));
    }
    // Neither kind: read as seconds, the first row is refused as `bad-time`.
    Ok(CsvTime::Seconds)
}

fn read_csv(
    source: &PlannedSource,
    time: &str,
    bytes: &[u8],
    origin: Option<&TimeOrigin>,
    limits: &ReplayLimits,
) -> Result<ReadSource, SourceError> {
    let plan = csv_plan(&source.columns)?;
    let kind = time_kind(source, &plan, bytes, time, origin, limits)?;
    let mut reader = csv_source(&plan.map, &plan.texts, bytes, time, kind, limits)?;
    let per_row = row_bytes(&source.columns);
    let mut times: Vec<f64> = Vec::new();
    let mut columns: Vec<Values> = source
        .columns
        .iter()
        .map(|column| Values::for_type(column.cdl_type, 0))
        .collect();
    let mut rows: u64 = 0;
    // The row's slot cells, copied out so its text cells can be read beside them.
    let mut numbers: Vec<Option<f64>> = Vec::with_capacity(plan.map.len());
    let unexpected = || SourceError::new(Kind::Unreadable, "an unexpected cell type");
    loop {
        let t = match reader.next_row() {
            Ok(Some((t, cells))) => {
                numbers.clear();
                for cell in cells {
                    numbers.push(match cell {
                        None => None,
                        Some(Value::Real(x)) => Some(*x),
                        Some(_) => return Err(unexpected()),
                    });
                }
                t
            }
            Ok(None) => break,
            Err(error) => return Err(from_replay(&error, time)),
        };
        rows += 1;
        if u128::from(rows) * u128::from(per_row) > u128::from(limits.max_source_bytes) {
            return Err(too_many_bytes(rows, limits));
        }
        times.push(next_time(&times, t, rows, time)?);
        let mut texts = reader.text_cells();
        for ((column, via), values) in source.columns.iter().zip(&plan.cells).zip(&mut columns) {
            let raw = match via {
                CsvCell::Slot(slot) => match numbers.get(*slot) {
                    Some(None) => Raw::Missing,
                    Some(Some(x)) => Raw::Number(*x),
                    None => return Err(unexpected()),
                },
                CsvCell::Text => match texts.next() {
                    Some(None) => Raw::Missing,
                    Some(Some(text)) => Raw::Text(text),
                    None => return Err(unexpected()),
                },
            };
            push(column, raw, values)
                .map_err(|error| error.at_row(rows).at_column(&column.address.label()))?;
        }
    }
    // The lists grew by doubling, as rows arrived; give back what no row uses, so a source
    // holds its rows and no more until the merge.
    times.shrink_to_fit();
    columns.iter_mut().for_each(Values::shrink_to_fit);
    Ok(ReadSource {
        times,
        columns,
        rows,
    })
}

/// The header of a `controllerOutput` file (OBC §12.7.3: "the first column is time in seconds",
/// every other a point): the first column's name and the others', in header order.
///
/// The first name is taken from the file's first record as `csv-core` writes it, which replay's
/// reader then confirms, so the header is still read by [`CsvSource`]: blank lines before it
/// are skipped, a leading byte-order mark too, a quoted name is unquoted with `""` as `"`, and
/// an unquoted one loses the spaces and tabs at either end, as replay trims every header name.
///
/// # Errors
/// `no-header` for a file with no record; `cell-too-long` for a first name past the cap;
/// `no-point-columns` for a header of the time alone; replay's header refusals.
pub(crate) fn controller_output_header(
    bytes: &[u8],
    limits: &ReplayLimits,
) -> Result<(String, Vec<String>), SourceError> {
    let mut rest = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    while let [b'\r' | b'\n', tail @ ..] = rest {
        rest = tail;
    }
    if rest.is_empty() {
        return Err(SourceError::new(
            Kind::NoHeader,
            "the file has no header row: its first column is the time, its others points",
        ));
    }
    let too_long = || {
        SourceError::new(
            Kind::CellTooLong,
            format!(
                "the first header is longer than {} bytes",
                limits.max_cell_bytes
            ),
        )
        .at_column(&field_label(0))
    };
    let cap = limits.max_cell_bytes as usize;
    let mut first = Vec::new();
    if let [b'"', quoted @ ..] = rest {
        let mut at = 0;
        loop {
            match quoted.get(at) {
                Some(b'"') if quoted.get(at + 1) == Some(&b'"') => {
                    first.push(b'"');
                    at += 2;
                }
                Some(b'"') | None => break,
                Some(byte) => {
                    first.push(*byte);
                    at += 1;
                }
            }
            if first.len() > cap {
                return Err(too_long());
            }
        }
    } else {
        for byte in rest {
            if matches!(byte, b',' | b'\r' | b'\n') {
                break;
            }
            first.push(*byte);
            if first.len() > cap {
                return Err(too_long());
            }
        }
        // Replay trims the spaces and tabs around an unquoted header name, after its cap.
        let end = first
            .iter()
            .rposition(|byte| !matches!(byte, b' ' | b'\t'))
            .map_or(0, |last| last + 1);
        first.truncate(end);
        let start = first
            .iter()
            .position(|byte| !matches!(byte, b' ' | b'\t'))
            .unwrap_or(first.len());
        first.drain(..start);
    }
    let time = String::from_utf8(first).map_err(|_| {
        SourceError::new(Kind::NotUtf8, "the first header is not UTF-8").at_column(&field_label(0))
    })?;
    let empty = InputMap::resolve(&[], &[], &ColumnMap::default(), &MissingPolicy::Refuse, &[])
        .map_err(|error| SourceError::new(Kind::Unreadable, error.to_string()))?;
    let reader = csv_source(&empty, &[], bytes, &time, CsvTime::Seconds, limits)?;
    let points = reader.ignored_columns().to_vec();
    if points.is_empty() {
        return Err(SourceError::new(
            Kind::NoPointColumns,
            "the header names the time and no point",
        ));
    }
    Ok((time, points))
}
