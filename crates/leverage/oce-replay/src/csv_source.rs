//! The bounded CSV adapter (decisions 8 and 22): a byte reader, a header row, a time column and the
//! map's column names, parsed by the `csv` crate inside this crate's caps.
//!
//! The caps, not the parser, are the hostile-input boundary: bytes read (`max_source_bytes`),
//! header fields (`max_columns`), bytes per cell (`max_cell_bytes`, header cells included) and
//! data rows (`max_rows`), each a typed [`SourceError`] and never truncation. Records must all have
//! the header's field count; cells must be UTF-8. The `csv` crate handles quoting, CRLF and a
//! leading UTF-8 byte-order mark. Reading a file by path is the caller's job.

use std::io::{self, Read};

use csv::{ByteRecord, ErrorKind, ReaderBuilder};
use oce_api::{Value, ValueType};

use crate::source::Row;
use crate::{FrameSource, InputMap, MapId, ReplayLimits, SourceError, TimeOrigin};

/// How the time column is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CsvTime {
    /// Model seconds as a decimal `f64`, used as given.
    Seconds,
    /// RFC 3339 instants with an explicit `Z` or `±hh:mm` offset (see [`TimeOrigin::parse`]),
    /// converted to model seconds against `origin` by the fixed split of [`crate::time`]. The
    /// origin is reported to the trace header.
    Instant {
        /// The wall-clock origin.
        origin: TimeOrigin,
    },
}

/// The shape of one CSV source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CsvFormat {
    /// The field delimiter. Default `b','`.
    pub delimiter: u8,
    /// The header name of the time column.
    pub time_column: String,
    /// How the time column is written.
    pub time: CsvTime,
    /// Cell texts that mean "missing" besides the empty cell. Default `NA` and `NaN`. A Real cell
    /// that parses to NaN is missing whatever this list says.
    pub missing_tokens: Vec<String>,
}

impl CsvFormat {
    /// A comma-separated format with `time_column` in model seconds and the default tokens.
    #[must_use]
    pub fn seconds(time_column: impl Into<String>) -> Self {
        Self {
            delimiter: b',',
            time_column: time_column.into(),
            time: CsvTime::Seconds,
            missing_tokens: vec!["NA".to_owned(), "NaN".to_owned()],
        }
    }
}

/// Counts bytes and fails the read one byte past the cap.
struct Bounded<R> {
    inner: R,
    read: u64,
    max: u64,
    over: bool,
}

impl<R: Read> Read for Bounded<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Ask for at most one byte past the cap, so an oversized source is detected without
        // reading it all.
        let room = (self.max + 1 - self.read).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..room])?;
        self.read += n as u64;
        if self.read > self.max {
            self.over = true;
            return Err(io::Error::other("source byte cap"));
        }
        Ok(n)
    }
}

/// A CSV byte reader bound to an [`InputMap`].
///
/// Every mapped column and the time column must appear in the header; other header columns are
/// ignored and reported by [`FrameSource::ignored_columns`]. Cell parsing: Real by Rust's `f64`
/// grammar (`inf`, `-inf` are values, NaN is missing); Integer as `i64`; Boolean as `true`,
/// `false`, `1` or `0`; Enum as a `u32` ordinal. No cell is trimmed. Once the record buffer has
/// grown to the longest row, reading a row allocates nothing.
pub struct CsvSource<R: Read> {
    map: MapId,
    reader: csv::Reader<Bounded<R>>,
    record: ByteRecord,
    time_field: usize,
    time_name: String,
    fields: Vec<usize>,
    names: Vec<String>,
    types: Vec<ValueType>,
    time: CsvTime,
    missing: Vec<Vec<u8>>,
    ignored: Vec<String>,
    cells: Vec<Option<Value>>,
    row: u64,
    max_rows: u64,
    max_cell: u32,
}

impl<R: Read> std::fmt::Debug for CsvSource<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CsvSource")
            .field("fields", &self.fields.len())
            .field("row", &self.row)
            .finish_non_exhaustive()
    }
}

impl<R: Read> CsvSource<R> {
    /// Read the header from `reader` and bind its columns to `map`.
    ///
    /// # Errors
    /// [`SourceError::Limit`]; `TooManyBytes`; `TooManyColumns`; `CellTooLong` (row 0);
    /// `NotUtf8` (row 0); `DuplicateColumn`; `MissingColumn` for the time column or a mapped
    /// column (time first, then slot order); `Read` for a reader failure.
    pub fn new(
        map: &InputMap,
        reader: R,
        format: CsvFormat,
        limits: &ReplayLimits,
    ) -> Result<Self, SourceError> {
        limits.validate()?;
        let bounded = Bounded {
            inner: reader,
            read: 0,
            max: limits.max_source_bytes,
            over: false,
        };
        let mut reader = ReaderBuilder::new()
            .delimiter(format.delimiter)
            .has_headers(false)
            .flexible(false)
            .from_reader(bounded);
        let mut header = ByteRecord::new();
        let read = reader.read_byte_record(&mut header);
        let present = read.map_err(|e| map_csv_error(&e, reader.get_ref(), 0))?;
        if !present {
            return Err(SourceError::MissingColumn {
                column: format.time_column,
            });
        }
        if header.len() as u64 > u64::from(limits.max_columns) {
            return Err(SourceError::TooManyColumns {
                limit: limits.max_columns,
            });
        }
        let mut names: Vec<String> = Vec::with_capacity(header.len());
        for (field, cell) in header.iter().enumerate() {
            if cell.len() as u64 > u64::from(limits.max_cell_bytes) {
                return Err(SourceError::CellTooLong {
                    row: 0,
                    field,
                    limit: limits.max_cell_bytes,
                });
            }
            let name =
                std::str::from_utf8(cell).map_err(|_| SourceError::NotUtf8 { row: 0, field })?;
            if names.iter().any(|n| n == name) {
                return Err(SourceError::DuplicateColumn {
                    column: name.to_owned(),
                });
            }
            names.push(name.to_owned());
        }
        let position = |column: &str| names.iter().position(|n| n == column);
        let time_field =
            position(&format.time_column).ok_or_else(|| SourceError::MissingColumn {
                column: format.time_column.clone(),
            })?;
        let mut fields = Vec::with_capacity(map.len());
        for slot in 0..map.len() {
            let column = map.column(slot);
            fields.push(position(column).ok_or_else(|| SourceError::MissingColumn {
                column: column.to_owned(),
            })?);
        }
        let ignored = names
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != time_field && !fields.contains(i))
            .map(|(_, n)| n.clone())
            .collect();
        Ok(Self {
            map: map.id().clone(),
            reader,
            record: ByteRecord::new(),
            time_field,
            time_name: format.time_column,
            names: (0..map.len()).map(|s| map.column(s).to_owned()).collect(),
            fields,
            types: map.inputs().iter().map(|d| d.value_type).collect(),
            time: format.time,
            missing: format
                .missing_tokens
                .into_iter()
                .map(String::into_bytes)
                .collect(),
            ignored,
            cells: vec![None; map.len()],
            row: 0,
            max_rows: limits.max_rows,
            max_cell: limits.max_cell_bytes,
        })
    }

    fn parse_time(&self, cell: &[u8], row: u64) -> Result<f64, SourceError> {
        if cell.is_empty() || self.missing.iter().any(|m| m.as_slice() == cell) {
            return Err(SourceError::MissingTime { row });
        }
        let text = std::str::from_utf8(cell).map_err(|_| SourceError::NotUtf8 {
            row,
            field: self.time_field,
        })?;
        match &self.time {
            CsvTime::Seconds => match text.parse::<f64>() {
                Ok(t) if t.is_nan() => Err(SourceError::MissingTime { row }),
                Ok(t) => Ok(t),
                Err(_) => Err(SourceError::BadCell {
                    row,
                    column: self.time_name.clone(),
                }),
            },
            CsvTime::Instant { origin } => origin
                .seconds_of(text)
                .ok_or(SourceError::BadInstant { row }),
        }
    }
}

fn map_csv_error<R>(error: &csv::Error, bounded: &Bounded<R>, row: u64) -> SourceError {
    if bounded.over {
        return SourceError::TooManyBytes { limit: bounded.max };
    }
    match error.kind() {
        ErrorKind::UnequalLengths { .. } => SourceError::Ragged { row },
        _ => SourceError::Read {
            message: error.to_string(),
        },
    }
}

fn parse_cell(cell: &str, value_type: ValueType) -> Option<Option<Value>> {
    Some(match value_type {
        ValueType::Real => {
            let x: f64 = cell.parse().ok()?;
            (!x.is_nan()).then_some(Value::Real(x))
        }
        ValueType::Integer => Some(Value::Integer(cell.parse().ok()?)),
        ValueType::Boolean => Some(Value::Boolean(match cell {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return None,
        })),
        ValueType::Enum(class) => Some(Value::Enum {
            class,
            ordinal: cell.parse().ok()?,
        }),
        ValueType::String => return None,
    })
}

impl<R: Read> FrameSource for CsvSource<R> {
    fn map_id(&self) -> &MapId {
        &self.map
    }

    fn origin(&self) -> Option<&TimeOrigin> {
        match &self.time {
            CsvTime::Seconds => None,
            CsvTime::Instant { origin } => Some(origin),
        }
    }

    fn ignored_columns(&self) -> &[String] {
        &self.ignored
    }

    fn next_row(&mut self) -> Result<Option<Row<'_>>, SourceError> {
        let row = self.row + 1;
        let present = match self.reader.read_byte_record(&mut self.record) {
            Ok(present) => present,
            Err(error) => return Err(map_csv_error(&error, self.reader.get_ref(), row)),
        };
        if !present {
            return Ok(None);
        }
        if row > self.max_rows {
            return Err(SourceError::TooManyRows {
                limit: self.max_rows,
            });
        }
        self.row = row;
        for (field, cell) in self.record.iter().enumerate() {
            if cell.len() as u64 > u64::from(self.max_cell) {
                return Err(SourceError::CellTooLong {
                    row,
                    field,
                    limit: self.max_cell,
                });
            }
        }
        let t = self.parse_time(&self.record[self.time_field], row)?;
        for slot in 0..self.cells.len() {
            let field = self.fields[slot];
            let cell = &self.record[field];
            self.cells[slot] =
                if cell.is_empty() || self.missing.iter().any(|m| m.as_slice() == cell) {
                    None
                } else {
                    let text = std::str::from_utf8(cell)
                        .map_err(|_| SourceError::NotUtf8 { row, field })?;
                    parse_cell(text, self.types[slot]).ok_or_else(|| SourceError::BadCell {
                        row,
                        column: self.names[slot].clone(),
                    })?
                };
        }
        Ok(Some((t, &self.cells)))
    }
}
