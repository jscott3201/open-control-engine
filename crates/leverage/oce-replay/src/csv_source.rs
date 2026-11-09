//! The bounded CSV adapter (decisions 8 and 22): a byte reader, a header row, a time column and the
//! map's column names, tokenized by `csv-core` (the allocation-free core of the `csv` crate) inside
//! this crate's caps.
//!
//! The caps, not the parser, are the hostile-input boundary, for memory as well as for content.
//! Bytes are pulled from the reader in 8 KiB chunks and handed to `csv_core::Reader` one field at a
//! time, into a cell buffer of `max_cell_bytes + 1` bytes; each cap is checked as the bytes arrive:
//! bytes read (`max_source_bytes`), header fields (`max_columns`, checked per field), bytes per cell
//! (`max_cell_bytes`, header cells included, checked when the cell buffer overflows), fields per
//! data record (the header's count, checked per field) and data rows (`max_rows`). Peak memory is
//! therefore bounded by the chunk, one cell and one record of at most `max_columns` cells of at
//! most `max_cell_bytes`, whatever the input: a multi-megabyte line of commas or an unterminated
//! quote is refused after at most one cap's worth of bytes is held. Each refusal is a typed
//! [`SourceError`], never truncation. `csv-core` handles quoting, doubled quotes, CRLF, empty
//! lines and a leading UTF-8 byte-order mark. Reading a file by path is the caller's job.

use std::io::Read;

use csv_core::{ReadFieldResult, Reader as CoreReader, ReaderBuilder};
use oce_api::{Value, ValueType};

use crate::source::Row;
use crate::{FrameSource, InputMap, MapId, ReplayLimits, SourceError, TimeOrigin};

/// Bytes requested from the reader per refill.
const CHUNK: usize = 8 * 1024;

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
    /// The field delimiter: one ASCII byte other than `"`, `\r` and `\n`. Default `b','`.
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

/// The byte side: a reader, a fixed chunk buffer, the byte count against the cap, and the
/// tokenizer with its cell buffer and the current record's cells.
struct Tokenizer<R> {
    reader: R,
    core: CoreReader,
    chunk: Vec<u8>,
    start: usize,
    end: usize,
    eof: bool,
    read: u64,
    max_bytes: u64,
    max_cell: usize,
    cell: Vec<u8>,
    /// The current record's cell bytes, concatenated; `ends[i]` is the end of cell `i`.
    record: Vec<u8>,
    ends: Vec<usize>,
}

impl<R: Read> Tokenizer<R> {
    /// Refill the chunk if it is empty; reading one byte past the cap is `TooManyBytes`. The very
    /// first fill keeps reading until at least four bytes are buffered (or the input ends), so a
    /// UTF-8 byte-order mark is recognised, and skipped, even from a reader that returns one byte
    /// at a time.
    fn refill(&mut self) -> Result<(), SourceError> {
        if self.start < self.end || self.eof {
            return Ok(());
        }
        let first = self.read == 0;
        self.start = 0;
        self.end = 0;
        loop {
            let room = (self.max_bytes + 1 - self.read).min((CHUNK - self.end) as u64) as usize;
            let n = loop {
                match self.reader.read(&mut self.chunk[self.end..self.end + room]) {
                    Ok(n) => break n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        return Err(SourceError::Read {
                            message: e.to_string(),
                        });
                    }
                }
            };
            self.read += n as u64;
            if self.read > self.max_bytes {
                return Err(SourceError::TooManyBytes {
                    limit: self.max_bytes,
                });
            }
            self.end += n;
            if n == 0 {
                self.eof = true;
            }
            if self.eof || !(first && self.end < 4) {
                break;
            }
        }
        // A leading UTF-8 byte-order mark is skipped here rather than by `csv-core`, whose own
        // strip needs the mark and a following byte in its first input slice.
        if first && self.chunk[..self.end].starts_with(b"\xef\xbb\xbf") {
            self.start = 3;
        }
        Ok(())
    }

    /// Read one record's cells into `record`/`ends`, refusing a cell longer than the cap and a
    /// record wider than `max_fields` as the bytes arrive. `Ok(false)` at the end of the input.
    fn next_record(
        &mut self,
        row: u64,
        max_fields: usize,
        too_wide: impl Fn() -> SourceError,
    ) -> Result<bool, SourceError> {
        self.record.clear();
        self.ends.clear();
        let mut filled = 0;
        loop {
            self.refill()?;
            let input = &self.chunk[self.start..self.end];
            let (result, nin, nout) = self.core.read_field(input, &mut self.cell[filled..]);
            self.start += nin;
            filled += nout;
            match result {
                ReadFieldResult::InputEmpty => {}
                ReadFieldResult::OutputFull => {
                    return Err(SourceError::CellTooLong {
                        row,
                        field: self.ends.len(),
                        limit: self.max_cell as u32,
                    });
                }
                ReadFieldResult::Field { record_end } => {
                    if filled > self.max_cell {
                        return Err(SourceError::CellTooLong {
                            row,
                            field: self.ends.len(),
                            limit: self.max_cell as u32,
                        });
                    }
                    if self.ends.len() >= max_fields {
                        return Err(too_wide());
                    }
                    self.record.extend_from_slice(&self.cell[..filled]);
                    self.ends.push(self.record.len());
                    filled = 0;
                    if record_end {
                        return Ok(true);
                    }
                }
                ReadFieldResult::End => return Ok(false),
            }
        }
    }

    fn field(&self, index: usize) -> &[u8] {
        let start = if index == 0 { 0 } else { self.ends[index - 1] };
        &self.record[start..self.ends[index]]
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
    bytes: Tokenizer<R>,
    width: usize,
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
    /// The first error, returned again by every later `next_row`: after a refusal the byte
    /// position is undefined, so reading on could only truncate silently.
    failed: Option<SourceError>,
}

impl<R: Read> std::fmt::Debug for CsvSource<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CsvSource")
            .field("fields", &self.width)
            .field("row", &self.row)
            .finish_non_exhaustive()
    }
}

impl<R: Read> CsvSource<R> {
    /// Read the header from `reader` and bind its columns to `map`.
    ///
    /// # Errors
    /// [`SourceError::Limit`]; `BadDelimiter`; `TooManyBytes`; `TooManyColumns`; `CellTooLong`
    /// (row 0); `NotUtf8` (row 0); `DuplicateColumn`; `MissingColumn` for the time column or a
    /// mapped column (time first, then slot order); `Read` for a reader failure.
    pub fn new(
        map: &InputMap,
        reader: R,
        format: CsvFormat,
        limits: &ReplayLimits,
    ) -> Result<Self, SourceError> {
        limits.validate()?;
        let d = format.delimiter;
        if !d.is_ascii() || matches!(d, b'"' | b'\r' | b'\n') {
            return Err(SourceError::BadDelimiter { delimiter: d });
        }
        let max_cell = limits.max_cell_bytes as usize;
        let max_columns = limits.max_columns as usize;
        let mut bytes = Tokenizer {
            reader,
            core: ReaderBuilder::new().delimiter(d).build(),
            chunk: vec![0; CHUNK],
            start: 0,
            end: 0,
            eof: false,
            read: 0,
            max_bytes: limits.max_source_bytes,
            max_cell,
            cell: vec![0; max_cell + 1],
            record: Vec::new(),
            ends: Vec::new(),
        };
        let too_many = || SourceError::TooManyColumns {
            limit: limits.max_columns,
        };
        if !bytes.next_record(0, max_columns, too_many)? {
            return Err(SourceError::MissingColumn {
                column: format.time_column,
            });
        }
        let width = bytes.ends.len();
        let mut names: Vec<String> = Vec::with_capacity(width);
        for field in 0..width {
            let name = std::str::from_utf8(bytes.field(field))
                .map_err(|_| SourceError::NotUtf8 { row: 0, field })?;
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
            bytes,
            width,
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
            failed: None,
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

    /// The next row. Errors are sticky: after any error every later call returns the same error,
    /// never a truncated or resumed row.
    fn next_row(&mut self) -> Result<Option<Row<'_>>, SourceError> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        match self.read_row() {
            Ok(Some(t)) => Ok(Some((t, &self.cells))),
            Ok(None) => Ok(None),
            Err(error) => {
                self.failed = Some(error.clone());
                Err(error)
            }
        }
    }
}

impl<R: Read> CsvSource<R> {
    /// Tokenize and parse the next record into `cells`; its time, or `None` at the end.
    fn read_row(&mut self) -> Result<Option<f64>, SourceError> {
        let row = self.row + 1;
        if !self
            .bytes
            .next_record(row, self.width, || SourceError::Ragged { row })?
        {
            return Ok(None);
        }
        if self.bytes.ends.len() != self.width {
            return Err(SourceError::Ragged { row });
        }
        if row > self.max_rows {
            return Err(SourceError::TooManyRows {
                limit: self.max_rows,
            });
        }
        self.row = row;
        let t = self.parse_time(self.bytes.field(self.time_field), row)?;
        for slot in 0..self.cells.len() {
            let field = self.fields[slot];
            let cell = self.bytes.field(field);
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
        Ok(Some(t))
    }
}
