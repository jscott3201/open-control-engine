//! Strict Toggle CSV parsing and contiguous keep-last projection.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Read as _;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "android"))]
const SAFE_OPEN_FLAGS: i32 = 0x0002_0000 | 0x0000_0800;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const SAFE_OPEN_FLAGS: i32 = 0x0000_0100 | 0x0000_0004;
// Unknown Unix targets retain the pre/post-open regular-file checks. Named deployment targets also
// use O_NOFOLLOW|O_NONBLOCK to close the replacement window without adding libc or unsafe code.
#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios"
    ))
))]
compile_error!("Toggle evidence file reads support Linux, Android, macOS, and iOS Unix targets");

pub(crate) const MAX_FILE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_ROWS: usize = 4096;
pub(crate) const MAX_COLUMNS: usize = 4;
pub(crate) const MAX_CELLS: usize = 16_384;
pub(crate) const MAX_LINE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_CELL_BYTES: usize = 128;
pub(crate) const MAX_TABLE_NAME_BYTES: usize = 256;
const RAW_HEADER: &str = "\"time\",\"clr\",\"u\",\"y\"";

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
    pub(crate) u: bool,
    pub(crate) clr: bool,
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
struct Field<'a> {
    value: &'a str,
    quoted: bool,
}

pub(crate) fn canonicalize_path(
    input: &Path,
    table_name: &str,
) -> Result<Canonicalization, CanonicalizerError> {
    canonicalize_bytes(&read_bounded_path(input)?, table_name)
}

pub(crate) fn read_bounded_path(input: &Path) -> Result<Vec<u8>, CanonicalizerError> {
    let metadata = std::fs::symlink_metadata(input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CanonicalizerError::new(
            ErrorCode::Io,
            "input must be a regular non-symlink file",
        ));
    }
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if size > MAX_FILE_BYTES {
        return Err(size_error(size));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(SAFE_OPEN_FLAGS);
    let file = options
        .open(input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    if !file
        .metadata()
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?
        .is_file()
    {
        return Err(CanonicalizerError::new(
            ErrorCode::Io,
            "opened input is not a regular file",
        ));
    }
    let mut input = Vec::with_capacity(size);
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    if input.len() > MAX_FILE_BYTES {
        return Err(size_error(input.len()));
    }
    Ok(input)
}

pub(crate) fn canonicalize_bytes(
    input: &[u8],
    table_name: &str,
) -> Result<Canonicalization, CanonicalizerError> {
    if input.len() > MAX_FILE_BYTES {
        return Err(size_error(input.len()));
    }
    let text = std::str::from_utf8(input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::CsvSyntax, error.to_string()))?;
    let lines = validate_csv_syntax(text)?;
    let header = lines.first().ok_or_else(|| {
        CanonicalizerError::new(ErrorCode::CsvSyntax, "input has no physical lines")
    })?;
    if *header != RAW_HEADER {
        return Err(CanonicalizerError::new(
            ErrorCode::HeaderIdentity,
            format!("raw header must be exactly {RAW_HEADER:?}"),
        ));
    }
    let row_count = lines.len() - 1;
    validate_shape(row_count)?;
    let mut rows = Vec::with_capacity(row_count);
    for (index, line) in lines.iter().skip(1).enumerate() {
        let fields = collect_fields(line).map_err(|detail| {
            CanonicalizerError::new(
                ErrorCode::CsvSyntax,
                format!("line {}: {detail}", index + 2),
            )
        })?;
        if fields.len() != MAX_COLUMNS {
            return Err(CanonicalizerError::new(
                ErrorCode::Shape,
                format!(
                    "line {} has {} columns; expected 4",
                    index + 2,
                    fields.len()
                ),
            ));
        }
        for (column, field) in fields.iter().enumerate() {
            if field.value.len() > MAX_CELL_BYTES {
                return Err(cell_error(
                    index + 2,
                    &format!("index {column}"),
                    &format!("exceeds {MAX_CELL_BYTES} bytes"),
                ));
            }
        }
        let time = numeric_cell(fields[0], index + 2, "time")?
            .parse::<f64>()
            .map_err(|_| cell_error(index + 2, "time", "is not an f64"))?;
        if !time.is_finite() {
            return Err(cell_error(index + 2, "time", "is not finite"));
        }
        rows.push(BooleanRow {
            time,
            clr: boolean_cell(fields[1], index + 2, "clr")?,
            u: boolean_cell(fields[2], index + 2, "u")?,
            y: boolean_cell(fields[3], index + 2, "y")?,
        });
    }
    validate_time_order(&rows)?;
    project(rows, table_name)
}

fn size_error(size: usize) -> CanonicalizerError {
    CanonicalizerError::new(
        ErrorCode::FileSize,
        format!("input is {size} bytes; limit is {MAX_FILE_BYTES}"),
    )
}

fn validate_csv_syntax(text: &str) -> Result<Vec<&str>, CanonicalizerError> {
    if text.is_empty() {
        return Err(CanonicalizerError::new(
            ErrorCode::CsvSyntax,
            "input is empty",
        ));
    }
    if text.as_bytes().contains(&b'\r') {
        return Err(CanonicalizerError::new(
            ErrorCode::CsvSyntax,
            "carriage returns are not accepted",
        ));
    }
    let lines = text.split_terminator('\n').collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        if line.is_empty() || line.len() > MAX_LINE_BYTES {
            return Err(CanonicalizerError::new(
                ErrorCode::CsvSyntax,
                format!("invalid physical line {}", index + 1),
            ));
        }
        scan_fields(line).map_err(|detail| {
            CanonicalizerError::new(
                ErrorCode::CsvSyntax,
                format!("line {}: {detail}", index + 1),
            )
        })?;
    }
    Ok(lines)
}

fn scan_fields(line: &str) -> Result<usize, &'static str> {
    let bytes = line.as_bytes();
    let mut cursor = 0;
    let mut fields = 0;
    loop {
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
            return Ok(fields + 1);
        }
    }
}

fn collect_fields(line: &str) -> Result<Vec<Field<'_>>, &'static str> {
    let bytes = line.as_bytes();
    let mut fields = Vec::new();
    let mut cursor = 0;
    loop {
        let quoted = cursor < bytes.len() && bytes[cursor] == b'"';
        let (start, end) = if quoted {
            cursor += 1;
            let start = cursor;
            let end = loop {
                match bytes.get(cursor) {
                    Some(b'"') if bytes.get(cursor + 1) == Some(&b'"') => cursor += 2,
                    Some(b'"') => break cursor,
                    Some(_) => cursor += 1,
                    None => return Err("unterminated quoted field"),
                }
            };
            cursor += 1;
            if cursor < bytes.len() && bytes[cursor] != b',' {
                return Err("characters follow a closing quote");
            }
            (start, end)
        } else {
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor] != b',' {
                if bytes[cursor] == b'"' {
                    return Err("quote appears inside an unquoted field");
                }
                cursor += 1;
            }
            (start, cursor)
        };
        fields.push(Field {
            value: &line[start..end],
            quoted,
        });
        if cursor == bytes.len() {
            return Ok(fields);
        }
        cursor += 1;
        if cursor == bytes.len() {
            fields.push(Field {
                value: "",
                quoted: false,
            });
            return Ok(fields);
        }
    }
}

fn numeric_cell<'a>(
    field: Field<'a>,
    line: usize,
    name: &str,
) -> Result<&'a str, CanonicalizerError> {
    if field.value.is_empty() || field.quoted {
        return Err(cell_error(line, name, "must be non-empty and unquoted"));
    }
    Ok(field.value)
}

fn boolean_cell(field: Field<'_>, line: usize, name: &str) -> Result<bool, CanonicalizerError> {
    match numeric_cell(field, line, name)? {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(cell_error(line, name, "must be literal 0 or 1")),
    }
}

fn validate_shape(rows: usize) -> Result<(), CanonicalizerError> {
    if rows == 0 || rows > MAX_ROWS || rows.saturating_mul(MAX_COLUMNS) > MAX_CELLS {
        return Err(CanonicalizerError::new(
            ErrorCode::Shape,
            format!("row count is {rows}; limit is {MAX_ROWS}"),
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
    let mut prior_time = None;
    let mut prior_bits = None;
    let mut closed_groups = BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        if prior_time.is_some_and(|time| row.time < time) {
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
        prior_time = Some(row.time);
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
            "table name must be a bounded ASCII identifier",
        ));
    }
    let mut rows = Vec::new();
    let mut group_sizes = Vec::new();
    for row in raw_rows.iter().copied() {
        if rows
            .last()
            .is_some_and(|prior: &BooleanRow| prior.time.to_bits() == row.time.to_bits())
        {
            *rows.last_mut().expect("equal-time group exists") = row;
            *group_sizes.last_mut().expect("equal-time size exists") += 1;
        } else {
            rows.push(row);
            group_sizes.push(1);
        }
    }
    let bytes = render(table_name, &rows).into_bytes();
    if bytes.len() > MAX_FILE_BYTES {
        return Err(size_error(bytes.len()));
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
        "#1\n# columns: time u clr y\ndouble {table_name}({},4)\n",
        rows.len()
    );
    for row in rows {
        let _ = writeln!(
            output,
            "{} {} {} {}",
            row.time,
            bool_text(row.u),
            bool_text(row.clr),
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
