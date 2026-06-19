//! Modelica `CombiTimeTable` CSV I/O (`07` §9).

use std::fmt;
use std::fs;
use std::path::Path;

use oce_model::{Value, ValueType};

use crate::Series;

/// A scalar signal kind that can be encoded in a numeric `CombiTimeTable` column.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueKind {
    /// Real-valued signal, encoded as its `f64` payload.
    Real,
    /// Integer-valued signal, encoded exactly as `f64` within the safe integer range.
    Integer,
    /// Boolean-valued signal, encoded as `0.0` / `1.0`.
    Boolean,
}

impl ValueKind {
    /// Derive the conformance signal kind from an `oce-model` value type.
    ///
    /// # Errors
    /// Returns [`CsvError::UnsupportedValueType`] for non-signal metadata types.
    pub fn from_value_type(value_type: ValueType) -> Result<Self, CsvError> {
        match value_type {
            ValueType::Real => Ok(Self::Real),
            ValueType::Integer => Ok(Self::Integer),
            ValueType::Boolean => Ok(Self::Boolean),
            other => Err(CsvError::UnsupportedValueType(other)),
        }
    }

    /// Encode a typed `oce-model` value into a numeric CombiTimeTable cell.
    ///
    /// # Errors
    /// Returns a typed [`CsvError`] if the value kind mismatches, a real value is non-finite, or
    /// an integer is not exactly representable in `f64`.
    pub fn encode_value(self, value: &Value) -> Result<f64, CsvError> {
        match (self, value) {
            (Self::Real, Value::Real(x)) if x.is_finite() => Ok(*x),
            (Self::Real, Value::Real(x)) => Err(CsvError::NonFinite {
                line: 0,
                column: 0,
                text: format_f64(*x),
            }),
            (Self::Integer, Value::Integer(i)) => encode_i64(*i),
            (Self::Boolean, Value::Boolean(b)) => Ok(if *b { 1.0 } else { 0.0 }),
            (_, other) => Err(CsvError::ValueKindMismatch {
                expected: self,
                found: other.value_type(),
            }),
        }
    }
}

/// A Modelica `CombiTimeTable` table. `data` is row-major and includes the time column.
#[derive(Clone, Debug, PartialEq)]
pub struct CombiTimeTable {
    /// Table name from `double <name>(rows,cols)`.
    pub name: String,
    /// Declared row count.
    pub n_rows: usize,
    /// Declared column count, including time column 0.
    pub n_cols: usize,
    /// Row-major numeric data, length `n_rows * n_cols`.
    pub data: Vec<f64>,
    /// Optional column names parsed from or written as a `# columns:` comment.
    pub col_names: Option<Vec<String>>,
}

impl CombiTimeTable {
    /// Read a `CombiTimeTable` CSV file.
    ///
    /// # Errors
    /// Returns [`CsvError`] for I/O, malformed headers, shape mismatches, invalid or non-finite
    /// numbers, or decreasing time.
    pub fn read(path: &Path) -> Result<Self, CsvError> {
        let text = fs::read_to_string(path).map_err(|e| CsvError::Io(e.to_string()))?;
        Self::parse(&text)
    }

    /// Parse a `CombiTimeTable` from a string.
    ///
    /// Full-line comments are accepted before and within the data section. Mid-line `#` comments
    /// on data rows are rejected rather than stripped so malformed cells cannot be hidden.
    ///
    /// # Errors
    /// Returns [`CsvError`] for malformed content.
    pub fn parse(text: &str) -> Result<Self, CsvError> {
        let mut lines = text.lines().enumerate().peekable();
        let Some((_, first)) = lines.next() else {
            return Err(CsvError::BadHeader("empty file".to_string()));
        };
        if first.trim() != "#1" {
            return Err(CsvError::BadHeader("missing #1 version header".to_string()));
        }

        let mut col_names = None;
        let (name, n_rows, n_cols) = loop {
            let Some((line_no, raw)) = lines.next() else {
                return Err(CsvError::BadHeader("missing double shape line".to_string()));
            };
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(comment) = line.strip_prefix('#') {
                if let Some(rest) = comment.trim().strip_prefix("columns:") {
                    col_names = Some(
                        split_fields(rest)
                            .iter()
                            .map(|s| (*s).to_string())
                            .collect(),
                    );
                }
                continue;
            }
            break parse_shape(line)
                .map_err(|e| CsvError::BadHeader(format!("line {}: {e}", line_no + 1)))?;
        };

        let mut rows = Vec::new();
        for (line_no, raw) in lines {
            let data_part = raw.trim();
            if data_part.is_empty() {
                continue;
            }
            if data_part.starts_with('#') {
                continue;
            }
            if data_part.contains('#') {
                return Err(CsvError::InvalidCell {
                    line: line_no + 1,
                    text: data_part.to_string(),
                });
            }
            let fields = split_fields(data_part);
            if fields.is_empty() {
                continue;
            }
            if fields.len() != n_cols {
                let row_index = rows.len() / n_cols + 1;
                return Err(CsvError::ShapeMismatch {
                    declared: (n_rows, n_cols),
                    actual: (row_index, fields.len()),
                });
            }
            for (col, f) in fields.into_iter().enumerate() {
                let value = f.parse::<f64>().map_err(|_| CsvError::InvalidCell {
                    line: line_no + 1,
                    text: f.to_string(),
                })?;
                if !value.is_finite() {
                    return Err(CsvError::NonFinite {
                        line: line_no + 1,
                        column: col,
                        text: f.to_string(),
                    });
                }
                rows.push(value);
            }
        }

        let actual_rows = rows.len().checked_div(n_cols).unwrap_or(0);
        if rows.len() != n_rows.saturating_mul(n_cols) {
            return Err(CsvError::ShapeMismatch {
                declared: (n_rows, n_cols),
                actual: (actual_rows, n_cols),
            });
        }
        validate_time(&rows, n_rows, n_cols)?;
        Ok(Self {
            name,
            n_rows,
            n_cols,
            data: rows,
            col_names,
        })
    }

    /// Write this table to `path` using deterministic shortest-round-trip float formatting.
    ///
    /// The writer emits canonical single-space separators and preserves only the structured
    /// `# columns:` comment; arbitrary input comments are not byte-preserved.
    ///
    /// # Errors
    /// Returns [`CsvError`] for I/O, shape mismatch, non-finite values, or decreasing time.
    pub fn write(&self, path: &Path) -> Result<(), CsvError> {
        fs::write(path, self.to_csv_string()?).map_err(|e| CsvError::Io(e.to_string()))
    }

    /// Serialize this table as deterministic `CombiTimeTable` CSV text.
    ///
    /// The serialized form uses single-space separators and includes only the optional
    /// `# columns:` comment. Other parsed comments are intentionally dropped.
    ///
    /// # Errors
    /// Returns [`CsvError`] for shape mismatch, non-finite values, or decreasing time.
    pub fn to_csv_string(&self) -> Result<String, CsvError> {
        validate_shape(self)?;
        validate_time(&self.data, self.n_rows, self.n_cols)?;
        let mut out = String::new();
        out.push_str("#1\n");
        if let Some(names) = &self.col_names {
            out.push_str("# columns:");
            for name in names {
                out.push(' ');
                out.push_str(name);
            }
            out.push('\n');
        }
        out.push_str(&format!(
            "double {}({},{})\n",
            self.name, self.n_rows, self.n_cols
        ));
        for row in 0..self.n_rows {
            for col in 0..self.n_cols {
                if col > 0 {
                    out.push(' ');
                }
                out.push_str(&format_f64(self.data[row * self.n_cols + col]));
            }
            out.push('\n');
        }
        Ok(out)
    }

    /// Copy one value column into owned `(time, value)` vectors usable as a funnel [`Series`].
    ///
    /// # Errors
    /// Returns [`CsvError::ShapeMismatch`] if `col` is not a valid value column.
    pub fn column_owned(&self, col: usize) -> Result<OwnedSeries, CsvError> {
        validate_shape(self)?;
        if col == 0 || col >= self.n_cols {
            return Err(CsvError::ShapeMismatch {
                declared: (self.n_rows, self.n_cols),
                actual: (self.n_rows, col + 1),
            });
        }
        let mut x = Vec::with_capacity(self.n_rows);
        let mut y = Vec::with_capacity(self.n_rows);
        for row in 0..self.n_rows {
            x.push(self.data[row * self.n_cols]);
            y.push(self.data[row * self.n_cols + col]);
        }
        Ok(OwnedSeries { x, y })
    }
}

/// Owned time/value vectors for one table column.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnedSeries {
    /// Time samples.
    pub x: Vec<f64>,
    /// Value samples.
    pub y: Vec<f64>,
}

impl OwnedSeries {
    /// Borrow this owned series as a funnel [`Series`].
    #[must_use]
    pub fn as_series(&self) -> Series<'_> {
        Series {
            x: &self.x,
            y: &self.y,
        }
    }
}

/// Format `value` with the deterministic shortest-round-trip representation used for CSV goldens.
#[must_use]
pub fn format_f64(value: f64) -> String {
    let mut buffer = ryu::Buffer::new();
    buffer.format(value).to_string()
}

/// Errors emitted by CombiTimeTable CSV I/O and typed signal encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CsvError {
    /// Filesystem I/O failure.
    Io(String),
    /// Missing or malformed header/shape content.
    BadHeader(String),
    /// Malformed data cell on a table row.
    InvalidCell {
        /// One-based input line number.
        line: usize,
        /// Offending cell or row text.
        text: String,
    },
    /// Non-finite numeric data cell.
    NonFinite {
        /// One-based input line number, or zero when the value came from an in-memory encoder.
        line: usize,
        /// Zero-based column index.
        column: usize,
        /// Offending numeric token.
        text: String,
    },
    /// Declared shape does not match actual row/column data.
    ShapeMismatch {
        /// Declared `(rows, cols)`.
        declared: (usize, usize),
        /// Actual `(rows, cols)` observed.
        actual: (usize, usize),
    },
    /// The time column decreased at `row`.
    NonMonotonicTime {
        /// Zero-based row index where the decrease was observed.
        row: usize,
    },
    /// Non-signal `oce-model` type cannot be encoded as a numeric trace column.
    UnsupportedValueType(ValueType),
    /// The requested signal kind did not match the value payload.
    ValueKindMismatch {
        /// Expected signal kind.
        expected: ValueKind,
        /// Found model value type.
        found: ValueType,
    },
    /// Integer value exceeds the exact `f64` integer range.
    IntegerNotExactlyRepresentable(i64),
}

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CsvError::Io(e) => write!(f, "CSV I/O failed: {e}"),
            CsvError::BadHeader(e) => write!(f, "bad CombiTimeTable header: {e}"),
            CsvError::InvalidCell { line, text } => {
                write!(f, "invalid CombiTimeTable data cell at line {line}: {text}")
            }
            CsvError::NonFinite { line, column, text } => {
                write!(
                    f,
                    "non-finite CombiTimeTable value at line {line}, column {column}: {text}"
                )
            }
            CsvError::ShapeMismatch { declared, actual } => {
                write!(
                    f,
                    "CombiTimeTable shape mismatch: declared {declared:?}, actual {actual:?}"
                )
            }
            CsvError::NonMonotonicTime { row } => {
                write!(f, "CombiTimeTable time column decreases at row {row}")
            }
            CsvError::UnsupportedValueType(t) => write!(f, "unsupported trace value type {t:?}"),
            CsvError::ValueKindMismatch { expected, found } => {
                write!(
                    f,
                    "trace kind mismatch: expected {expected:?}, found {found:?}"
                )
            }
            CsvError::IntegerNotExactlyRepresentable(i) => {
                write!(f, "integer {i} is not exactly representable as f64")
            }
        }
    }
}

impl std::error::Error for CsvError {}

fn parse_shape(line: &str) -> Result<(String, usize, usize), String> {
    let rest = line
        .strip_prefix("double ")
        .ok_or_else(|| "shape line must start with `double `".to_string())?;
    let open = rest
        .find('(')
        .ok_or_else(|| "shape line missing `(`".to_string())?;
    let close = rest
        .rfind(')')
        .ok_or_else(|| "shape line missing `)`".to_string())?;
    let name = rest[..open].trim();
    if name.is_empty() {
        return Err("shape line has empty table name".to_string());
    }
    let dims: Vec<_> = rest[open + 1..close].split(',').map(str::trim).collect();
    if dims.len() != 2 {
        return Err("shape must contain rows,cols".to_string());
    }
    let rows = dims[0]
        .parse()
        .map_err(|_| "invalid row count".to_string())?;
    let cols = dims[1]
        .parse()
        .map_err(|_| "invalid column count".to_string())?;
    if cols == 0 {
        return Err("column count must include time column".to_string());
    }
    Ok((name.to_string(), rows, cols))
}

fn split_fields(line: &str) -> Vec<&str> {
    line.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|s| !s.is_empty())
        .collect()
}

fn validate_shape(table: &CombiTimeTable) -> Result<(), CsvError> {
    if table.data.len() != table.n_rows.saturating_mul(table.n_cols) {
        let actual_rows = table.data.len().checked_div(table.n_cols).unwrap_or(0);
        return Err(CsvError::ShapeMismatch {
            declared: (table.n_rows, table.n_cols),
            actual: (actual_rows, table.n_cols),
        });
    }
    Ok(())
}

fn validate_time(data: &[f64], n_rows: usize, n_cols: usize) -> Result<(), CsvError> {
    if n_cols == 0 {
        return Err(CsvError::ShapeMismatch {
            declared: (n_rows, n_cols),
            actual: (0, 0),
        });
    }
    for row in 0..n_rows {
        for col in 0..n_cols {
            let value = data[row * n_cols + col];
            if !value.is_finite() {
                return Err(CsvError::NonFinite {
                    line: row,
                    column: col,
                    text: format_f64(value),
                });
            }
        }
    }
    for row in 1..n_rows {
        let prev = data[(row - 1) * n_cols];
        let next = data[row * n_cols];
        if next < prev {
            return Err(CsvError::NonMonotonicTime { row });
        }
    }
    Ok(())
}

fn encode_i64(i: i64) -> Result<f64, CsvError> {
    const MAX_EXACT: i64 = 9_007_199_254_740_992;
    if (-MAX_EXACT..=MAX_EXACT).contains(&i) {
        Ok(i as f64)
    } else {
        Err(CsvError::IntegerNotExactlyRepresentable(i))
    }
}
