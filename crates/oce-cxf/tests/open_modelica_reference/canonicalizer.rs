//! Strict OpenModelica CSV parsing and contiguous keep-last projection.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::Path;

pub(crate) const MAX_FILE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_ROWS: usize = 4096;
pub(crate) const MAX_COLUMNS: usize = 256;
pub(crate) const MAX_CELLS: usize = 1_048_576;
pub(crate) const MAX_LINE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_HEADER_BYTES: usize = 256;
pub(crate) const MAX_CELL_BYTES: usize = 128;
pub(crate) const MAX_TABLE_NAME_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErrorCode {
    FileSize,
    CsvSyntax,
    HeaderIdentity,
    Shape,
    CellType,
    TimeOrder,
    Scenario,
    Io,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CanonicalizerError {
    pub(crate) code: ErrorCode,
    pub(crate) detail: String,
}

impl CanonicalizerError {
    fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for CanonicalizerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.detail)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BooleanRow {
    pub(crate) time: f64,
    pub(crate) u1: bool,
    pub(crate) u2: bool,
    pub(crate) y: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Canonicalization {
    pub(crate) raw_rows: Vec<BooleanRow>,
    pub(crate) rows: Vec<BooleanRow>,
    pub(crate) group_sizes: Vec<usize>,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Columns {
    time: usize,
    u1: usize,
    u2: usize,
    y: usize,
    width: usize,
}

#[derive(Clone, Copy)]
struct Field<'a> {
    value: &'a str,
    quoted: bool,
}

pub(crate) fn canonicalize_path(
    input: &Path,
    table_name: &str,
) -> Result<Canonicalization, CanonicalizerError> {
    let input = read_bounded_path(input)?;
    canonicalize_bytes(&input, table_name)
}

pub(crate) fn read_bounded_path(input: &Path) -> Result<Vec<u8>, CanonicalizerError> {
    let metadata = std::fs::metadata(input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if size > MAX_FILE_BYTES {
        return Err(CanonicalizerError::new(
            ErrorCode::FileSize,
            format!("input is {size} bytes; limit is {MAX_FILE_BYTES}"),
        ));
    }
    let file = std::fs::File::open(input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    let mut input = Vec::with_capacity(size);
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    if input.len() > MAX_FILE_BYTES {
        return Err(CanonicalizerError::new(
            ErrorCode::FileSize,
            format!("input is {} bytes; limit is {MAX_FILE_BYTES}", input.len()),
        ));
    }
    Ok(input)
}

pub(crate) fn canonicalize_bytes(
    input: &[u8],
    table_name: &str,
) -> Result<Canonicalization, CanonicalizerError> {
    if input.len() > MAX_FILE_BYTES {
        return Err(CanonicalizerError::new(
            ErrorCode::FileSize,
            format!("input is {} bytes; limit is {MAX_FILE_BYTES}", input.len()),
        ));
    }
    if input.is_empty() {
        return Err(CanonicalizerError::new(
            ErrorCode::CsvSyntax,
            "input is empty",
        ));
    }
    let text = std::str::from_utf8(input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::CsvSyntax, error.to_string()))?;
    let line_count = validate_csv_syntax(text)?;
    let header = text
        .split_terminator('\n')
        .next()
        .expect("non-empty input has a physical line");
    let columns = required_columns(header)?;
    if line_count == 1 {
        return Err(CanonicalizerError::new(
            ErrorCode::Shape,
            "header has no data rows",
        ));
    }
    let row_count = line_count - 1;
    validate_shape(row_count, columns.width)?;

    for (index, line) in text.split_terminator('\n').skip(1).enumerate() {
        let width = scan_fields(line).expect("syntax pass established valid CSV");
        if width != columns.width {
            return Err(CanonicalizerError::new(
                ErrorCode::Shape,
                format!(
                    "line {} has {width} columns; expected {}",
                    index + 2,
                    columns.width
                ),
            ));
        }
    }

    let mut rows = Vec::with_capacity(row_count);
    for (index, line) in text.split_terminator('\n').skip(1).enumerate() {
        let fields = collect_fields(line);
        for (column, field) in fields.iter().enumerate() {
            if field.value.len() > MAX_CELL_BYTES {
                return Err(cell_error(
                    index + 2,
                    &format!("index {column}"),
                    &format!("exceeds {MAX_CELL_BYTES} bytes"),
                ));
            }
        }
        let time = numeric_cell(fields[columns.time], index + 2, "time")?
            .parse::<f64>()
            .map_err(|_| cell_error(index + 2, "time", "is not an f64"))?;
        if !time.is_finite() {
            return Err(cell_error(index + 2, "time", "is not finite"));
        }
        rows.push(BooleanRow {
            time,
            u1: boolean_cell(fields[columns.u1], index + 2, "u1")?,
            u2: boolean_cell(fields[columns.u2], index + 2, "u2")?,
            y: boolean_cell(fields[columns.y], index + 2, "y")?,
        });
    }
    validate_time_order(&rows)?;
    project(rows, table_name)
}

fn validate_csv_syntax(text: &str) -> Result<usize, CanonicalizerError> {
    if text.as_bytes().contains(&b'\r') {
        return Err(CanonicalizerError::new(
            ErrorCode::CsvSyntax,
            "carriage returns are not accepted",
        ));
    }
    let mut line_count = 0;
    for (index, line) in text.split_terminator('\n').enumerate() {
        if line.len() > MAX_LINE_BYTES {
            return Err(CanonicalizerError::new(
                ErrorCode::CsvSyntax,
                format!("physical line exceeds {MAX_LINE_BYTES} bytes"),
            ));
        }
        if line.is_empty() {
            return Err(CanonicalizerError::new(
                ErrorCode::CsvSyntax,
                "empty physical lines are not accepted",
            ));
        }
        scan_fields(line).map_err(|detail| {
            CanonicalizerError::new(
                ErrorCode::CsvSyntax,
                format!("line {}: {detail}", index + 1),
            )
        })?;
        line_count += 1;
    }
    if line_count == 0 {
        return Err(CanonicalizerError::new(
            ErrorCode::CsvSyntax,
            "input has no physical lines",
        ));
    }
    Ok(line_count)
}

fn scan_fields(line: &str) -> Result<usize, &'static str> {
    let bytes = line.as_bytes();
    let mut cursor = 0;
    let mut fields = 0;
    loop {
        if cursor > bytes.len() {
            return Err("field cursor exceeded line");
        }
        if cursor < bytes.len() && bytes[cursor] == b'"' {
            cursor += 1;
            loop {
                match bytes.get(cursor) {
                    Some(b'"') if bytes.get(cursor + 1) == Some(&b'"') => cursor += 2,
                    Some(b'"') => {
                        cursor += 1;
                        break;
                    }
                    Some(_) => cursor += 1,
                    None => return Err("unterminated quoted field"),
                }
            }
            if cursor < bytes.len() && bytes[cursor] != b',' {
                return Err("characters follow a closing quote");
            }
        } else {
            while cursor < bytes.len() && bytes[cursor] != b',' {
                if bytes[cursor] == b'"' {
                    return Err("quote appears inside an unquoted field");
                }
                cursor += 1;
            }
        }
        fields += 1;
        if cursor == bytes.len() {
            return Ok(fields);
        }
        cursor += 1;
        if cursor == bytes.len() {
            fields += 1;
            return Ok(fields);
        }
    }
}

fn collect_fields(line: &str) -> Vec<Field<'_>> {
    let mut fields = Vec::new();
    for_each_field(line, |field| fields.push(field));
    fields
}

fn for_each_field<'a>(line: &'a str, mut visit: impl FnMut(Field<'a>)) {
    let bytes = line.as_bytes();
    let mut cursor = 0;
    loop {
        let start;
        let end;
        let quoted = cursor < bytes.len() && bytes[cursor] == b'"';
        if quoted {
            cursor += 1;
            start = cursor;
            loop {
                if bytes[cursor] == b'"' && bytes.get(cursor + 1) == Some(&b'"') {
                    cursor += 2;
                } else if bytes[cursor] == b'"' {
                    break;
                } else {
                    cursor += 1;
                }
            }
            end = cursor;
            cursor += 1;
        } else {
            start = cursor;
            while cursor < bytes.len() && bytes[cursor] != b',' {
                cursor += 1;
            }
            end = cursor;
        }
        visit(Field {
            value: &line[start..end],
            quoted,
        });
        if cursor == bytes.len() {
            return;
        }
        cursor += 1;
        if cursor == bytes.len() {
            visit(Field {
                value: "",
                quoted: false,
            });
            return;
        }
    }
}

fn required_columns(header: &str) -> Result<Columns, CanonicalizerError> {
    let mut required = [None; 4];
    let mut width = 0;
    let mut failure = None;
    for_each_field(header, |field| {
        if failure.is_some() {
            return;
        }
        let index = width;
        width += 1;
        if field.value.len() > MAX_HEADER_BYTES {
            failure = Some(CanonicalizerError::new(
                ErrorCode::HeaderIdentity,
                format!("header {index} exceeds {MAX_HEADER_BYTES} bytes"),
            ));
            return;
        }
        let slot = match field.value {
            "time" => Some(0),
            "u1" => Some(1),
            "u2" => Some(2),
            "y" => Some(3),
            _ => None,
        };
        if let Some(slot) = slot
            && required[slot].replace(index).is_some()
        {
            failure = Some(CanonicalizerError::new(
                ErrorCode::HeaderIdentity,
                format!("required header {:?} is ambiguous", field.value),
            ));
        }
    });
    if let Some(failure) = failure {
        return Err(failure);
    }
    let missing = ["time", "u1", "u2", "y"]
        .into_iter()
        .zip(required)
        .find_map(|(name, index)| index.is_none().then_some(name));
    if let Some(name) = missing {
        return Err(CanonicalizerError::new(
            ErrorCode::HeaderIdentity,
            format!("required header {name:?} is missing"),
        ));
    }
    Ok(Columns {
        time: required[0].unwrap(),
        u1: required[1].unwrap(),
        u2: required[2].unwrap(),
        y: required[3].unwrap(),
        width,
    })
}

fn numeric_cell<'a>(
    cell: Field<'a>,
    line: usize,
    name: &str,
) -> Result<&'a str, CanonicalizerError> {
    if cell.value.len() > MAX_CELL_BYTES {
        return Err(cell_error(
            line,
            name,
            &format!("exceeds {MAX_CELL_BYTES} bytes"),
        ));
    }
    if cell.value.is_empty() {
        return Err(cell_error(line, name, "is empty"));
    }
    if cell.quoted {
        return Err(cell_error(line, name, "must not be quoted"));
    }
    Ok(cell.value)
}

fn boolean_cell(cell: Field<'_>, line: usize, name: &str) -> Result<bool, CanonicalizerError> {
    match numeric_cell(cell, line, name)? {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(cell_error(line, name, "must be literal 0 or 1")),
    }
}

fn validate_shape(rows: usize, columns: usize) -> Result<(), CanonicalizerError> {
    if rows > MAX_ROWS {
        return Err(CanonicalizerError::new(
            ErrorCode::Shape,
            format!("row count is {rows}; limit is {MAX_ROWS}"),
        ));
    }
    if columns > MAX_COLUMNS {
        return Err(CanonicalizerError::new(
            ErrorCode::Shape,
            format!("column count is {columns}; limit is {MAX_COLUMNS}"),
        ));
    }
    if rows
        .checked_mul(columns)
        .is_none_or(|cells| cells > MAX_CELLS)
    {
        return Err(CanonicalizerError::new(
            ErrorCode::Shape,
            "rows multiplied by columns exceeds 1048576",
        ));
    }
    Ok(())
}

fn cell_error(line: usize, name: &str, detail: &str) -> CanonicalizerError {
    CanonicalizerError::new(
        ErrorCode::CellType,
        format!("line {line} column {name} {detail}"),
    )
}

fn validate_time_order(rows: &[BooleanRow]) -> Result<(), CanonicalizerError> {
    let mut prior = None;
    let mut prior_bits = None;
    let mut closed_groups = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        if prior.is_some_and(|time| row.time < time) {
            return Err(CanonicalizerError::new(
                ErrorCode::TimeOrder,
                format!("time decreases at data row {index}"),
            ));
        }
        let bits = row.time.to_bits();
        if prior_bits != Some(bits) {
            if let Some(previous) = prior_bits {
                closed_groups.insert(previous);
            }
            if closed_groups.contains(&bits) {
                return Err(CanonicalizerError::new(
                    ErrorCode::TimeOrder,
                    format!("equal-time group is noncontiguous at data row {index}"),
                ));
            }
        }
        prior = Some(row.time);
        prior_bits = Some(bits);
    }
    Ok(())
}

fn project(
    raw_rows: Vec<BooleanRow>,
    table_name: &str,
) -> Result<Canonicalization, CanonicalizerError> {
    if table_name.is_empty()
        || table_name.len() > MAX_TABLE_NAME_BYTES
        || !table_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(CanonicalizerError::new(
            ErrorCode::Scenario,
            format!(
                "table name must be a non-empty ASCII identifier of at most {MAX_TABLE_NAME_BYTES} bytes"
            ),
        ));
    }
    let mut rows = Vec::new();
    let mut group_sizes = Vec::new();
    for row in raw_rows.iter().copied() {
        if rows
            .last()
            .is_some_and(|prior: &BooleanRow| prior.time.to_bits() == row.time.to_bits())
        {
            *rows.last_mut().unwrap() = row;
            *group_sizes.last_mut().unwrap() += 1;
        } else {
            rows.push(row);
            group_sizes.push(1);
        }
    }
    let bytes = render(table_name, &rows).into_bytes();
    if bytes.len() > MAX_FILE_BYTES {
        return Err(CanonicalizerError::new(
            ErrorCode::FileSize,
            "canonical output exceeds 1 MiB",
        ));
    }
    Ok(Canonicalization {
        raw_rows,
        rows,
        group_sizes,
        bytes,
    })
}

fn render(table_name: &str, rows: &[BooleanRow]) -> String {
    let mut output = format!(
        "#1\n# columns: time u1 u2 y\ndouble {table_name}({},4)\n",
        rows.len()
    );
    for row in rows {
        let _ = writeln!(
            output,
            "{} {} {} {}",
            row.time,
            bool_text(row.u1),
            bool_text(row.u2),
            bool_text(row.y)
        );
    }
    output
}

fn bool_text(value: bool) -> &'static str {
    if value { "1.0" } else { "0.0" }
}

#[cfg(test)]
#[path = "canonicalizer_tests.rs"]
mod tests;
