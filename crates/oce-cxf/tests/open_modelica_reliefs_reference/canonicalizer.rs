//! Strict Reliefs CSV parsing, exact-time projection, and input-tuple selection.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Component, Path};

pub(crate) const MAX_FILE_BYTES: usize = 1024 * 1024;
pub(crate) const MAX_ROWS: usize = 4096;
pub(crate) const MAX_COLUMNS: usize = 8;
pub(crate) const MAX_CELLS: usize = MAX_ROWS * MAX_COLUMNS;
pub(crate) const MAX_LINE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_CELL_BYTES: usize = 128;
pub(crate) const MAX_TABLE_NAME_BYTES: usize = 256;
pub(crate) const RAW_HEADER: &str = "\"time\",\"uOutDam_max\",\"uOutDam_min\",\"uRetDam_max\",\"uRetDam_min\",\"uTSup\",\"yOutDam\",\"yRetDam\"";

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
pub(crate) struct ReliefRow {
    pub(crate) source_index: usize,
    pub(crate) time: f64,
    pub(crate) u_t_sup: f64,
    pub(crate) u_out_dam_min: f64,
    pub(crate) u_out_dam_max: f64,
    pub(crate) u_ret_dam_min: f64,
    pub(crate) u_ret_dam_max: f64,
    pub(crate) y_out_dam: f64,
    pub(crate) y_ret_dam: f64,
}

impl ReliefRow {
    pub(crate) fn input_bits(self) -> [u64; 5] {
        [
            self.u_t_sup.to_bits(),
            self.u_out_dam_min.to_bits(),
            self.u_out_dam_max.to_bits(),
            self.u_ret_dam_min.to_bits(),
            self.u_ret_dam_max.to_bits(),
        ]
    }

    pub(crate) fn output_bits(self) -> [u64; 2] {
        [self.y_out_dam.to_bits(), self.y_ret_dam.to_bits()]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Canonicalization {
    pub(crate) raw_rows: Vec<ReliefRow>,
    pub(crate) grouped_rows: Vec<ReliefRow>,
    pub(crate) rows: Vec<ReliefRow>,
    pub(crate) group_sizes: Vec<usize>,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectionSelection {
    First,
    Last,
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

pub(crate) fn canonicalize_path_with_selection(
    input: &Path,
    table_name: &str,
    selection: ProjectionSelection,
) -> Result<Canonicalization, CanonicalizerError> {
    canonicalize_bytes_with_selection(&read_bounded_path(input)?, table_name, selection)
}

pub(crate) fn read_bounded_path(input: &Path) -> Result<Vec<u8>, CanonicalizerError> {
    #[cfg(unix)]
    let mut file = open_descriptor_relative(input)?;
    #[cfg(not(unix))]
    let mut file = open_checked_non_unix(input)?;
    let metadata = file
        .metadata()
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    if !metadata.is_file() {
        return Err(CanonicalizerError::new(
            ErrorCode::Io,
            "opened input is not a regular file",
        ));
    }
    let size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if size > MAX_FILE_BYTES {
        return Err(size_error(size));
    }
    let mut bytes = Vec::with_capacity(size);
    file.by_ref()
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(size_error(bytes.len()));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn open_descriptor_relative(input: &Path) -> Result<std::fs::File, CanonicalizerError> {
    use rustix::fs::{Mode, OFlags, open, openat};

    let open_component =
        |directory: &std::os::fd::OwnedFd, name: &std::ffi::OsStr, flags: OFlags| {
            openat(directory, name, flags, Mode::empty())
                .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))
        };
    let mut components = input.components().peekable();
    let mut directory = if input.is_absolute() {
        if !matches!(components.next(), Some(Component::RootDir)) {
            return Err(CanonicalizerError::new(
                ErrorCode::Io,
                "absolute input has no root component",
            ));
        }
        open(
            "/",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
    } else {
        open(
            ".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
    }
    .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
    while let Some(component) = components.next() {
        let Component::Normal(name) = component else {
            return Err(CanonicalizerError::new(
                ErrorCode::Io,
                "input path contains an unsupported component",
            ));
        };
        if components.peek().is_none() {
            let descriptor = open_component(
                &directory,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            )?;
            return Ok(std::fs::File::from(descriptor));
        }
        directory = open_component(
            &directory,
            name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        )?;
    }
    Err(CanonicalizerError::new(
        ErrorCode::Io,
        "input path has no final component",
    ))
}

#[cfg(not(unix))]
fn open_checked_non_unix(input: &Path) -> Result<std::fs::File, CanonicalizerError> {
    let mut current = std::path::PathBuf::new();
    for component in input.components() {
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(CanonicalizerError::new(
                ErrorCode::Io,
                "input path contains a symlink component",
            ));
        }
    }
    std::fs::File::open(input)
        .map_err(|error| CanonicalizerError::new(ErrorCode::Io, error.to_string()))
}

pub(crate) fn canonicalize_bytes(
    input: &[u8],
    table_name: &str,
) -> Result<Canonicalization, CanonicalizerError> {
    canonicalize_bytes_with_selection(input, table_name, ProjectionSelection::Last)
}

pub(crate) fn canonicalize_bytes_with_selection(
    input: &[u8],
    table_name: &str,
    selection: ProjectionSelection,
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
                    "line {} has {} columns; expected {MAX_COLUMNS}",
                    index + 2,
                    fields.len()
                ),
            ));
        }
        let value = |column: usize, name: &str| real_cell(fields[column], index + 2, name);
        rows.push(ReliefRow {
            source_index: index,
            time: value(0, "time")?,
            u_out_dam_max: value(1, "uOutDam_max")?,
            u_out_dam_min: value(2, "uOutDam_min")?,
            u_ret_dam_max: value(3, "uRetDam_max")?,
            u_ret_dam_min: value(4, "uRetDam_min")?,
            u_t_sup: value(5, "uTSup")?,
            y_out_dam: value(6, "yOutDam")?,
            y_ret_dam: value(7, "yRetDam")?,
        });
    }
    validate_time_order(&rows)?;
    project(rows, table_name, selection)
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
    if text.as_bytes().contains(&b'\r') || !text.ends_with('\n') {
        return Err(CanonicalizerError::new(
            ErrorCode::CsvSyntax,
            "only newline-terminated LF records are accepted",
        ));
    }
    let lines = text.lines().collect::<Vec<_>>();
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

fn real_cell(field: Field<'_>, line: usize, name: &str) -> Result<f64, CanonicalizerError> {
    if field.value.is_empty() || field.quoted || field.value.len() > MAX_CELL_BYTES {
        return Err(cell_error(
            line,
            name,
            "must be bounded, non-empty, and unquoted",
        ));
    }
    let value = field
        .value
        .parse::<f64>()
        .map_err(|_| cell_error(line, name, "is not an f64"))?;
    if !value.is_finite() {
        return Err(cell_error(line, name, "is not finite"));
    }
    Ok(value)
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

fn validate_time_order(rows: &[ReliefRow]) -> Result<(), CanonicalizerError> {
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
    raw_rows: Vec<ReliefRow>,
    table_name: &str,
    selection: ProjectionSelection,
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
    let mut grouped_rows = Vec::new();
    let mut group_sizes = Vec::new();
    for row in raw_rows.iter().copied() {
        if grouped_rows
            .last()
            .is_some_and(|prior: &ReliefRow| prior.time.to_bits() == row.time.to_bits())
        {
            if selection == ProjectionSelection::Last {
                *grouped_rows.last_mut().expect("equal-time group exists") = row;
            }
            *group_sizes.last_mut().expect("equal-time size exists") += 1;
        } else {
            grouped_rows.push(row);
            group_sizes.push(1);
        }
    }
    let mut rows = Vec::new();
    for row in grouped_rows.iter().copied() {
        if rows
            .last()
            .is_none_or(|prior: &ReliefRow| prior.input_bits() != row.input_bits())
        {
            rows.push(row);
        }
    }
    let bytes = render(table_name, &rows).into_bytes();
    if bytes.len() > MAX_FILE_BYTES {
        return Err(size_error(bytes.len()));
    }
    Ok(Canonicalization {
        raw_rows,
        grouped_rows,
        rows,
        group_sizes,
        bytes,
    })
}

fn render(table_name: &str, rows: &[ReliefRow]) -> String {
    let mut output = format!(
        "#1\n# columns: time uTSup uOutDam_min uOutDam_max uRetDam_min uRetDam_max yOutDam yRetDam\ndouble {table_name}({},8)\n",
        rows.len()
    );
    for row in rows {
        let _ = writeln!(
            output,
            "{} {} {} {} {} {} {} {}",
            row.time,
            row.u_t_sup,
            row.u_out_dam_min,
            row.u_out_dam_max,
            row.u_ret_dam_min,
            row.u_ret_dam_max,
            row.y_out_dam,
            row.y_ret_dam,
        );
    }
    output
}

#[cfg(test)]
#[path = "canonicalizer_tests.rs"]
mod tests;
