//! The `combiTimeTable` reader: OBC §12.5.1's `#1` text format, the Modelica Buildings library's
//! trend format (commissioning note, "A trend source"; issue #660).
//!
//! This is a reader of a table of numbers, not of Modelica: standing rule 1 forbids a `.mo`
//! parser, and nothing here reads one. The grammar, as §12.5.1 states it and no wider:
//!
//! - **The tag.** "The first two characters in the file need to be `#1`"; the rest of that line
//!   is a comment. No byte-order mark, no leading space.
//! - **Comments.** "Line comments start with the hash symbol (#) and can appear everywhere" a
//!   separator can: from `#` to the end of the line. A comment may hold any bytes.
//! - **A matrix** is "declared with type `double`, name and dimensions": `double`, one or more
//!   spaces or tabs, a name (`[A-Za-z_][A-Za-z0-9_]*`, at most `max_cell_bytes` bytes), then
//!   `(rows, cols)` with optional spaces or tabs around each part, the two decimal integers at
//!   least 1, then the end of the line or a comment. `float` and every other type are refused.
//! - **The values** follow "as a sequence of numbers in row-wise order": rows times cols numbers,
//!   wherever the line breaks fall. Between two numbers stands a separator: any run of spaces,
//!   tabs, line ends and comments, with at most one `,` or `;` in it ("number separators are
//!   spaces, tab, comma, or semicolon"). Two commas with no number between them are an empty
//!   cell and refused; so is a separator after a matrix's last value.
//! - **A number** "according to C syntax (such as 2.3, -2, +2.e4)": the decimal form
//!   `[+-]? (D+ ('.' D*)? | '.' D+) ([eE] [+-]? D+)?`, at most `max_cell_bytes` bytes. No
//!   hexadecimal, no `inf`, no `nan`: a value is a finite number. It is converted by Rust's
//!   `f64` parser, which rounds correctly (to nearest, ties to even), so the same text gives the
//!   same bits on every target; a magnitude beyond `f64::MAX` is `non-finite-number`, and one
//!   below the smallest subnormal reads as zero of its sign.
//! - **Several matrices** may follow one another. With no `table`, a file must hold exactly one;
//!   with one, exactly one matrix must have that name. Every matrix of the file is read to the
//!   same rules, but only the chosen one's values are kept, and of those only the columns asked
//!   for.
//!
//! **Bounds.** The file's length is checked against `max_source_bytes` before a byte is read; a
//! matrix's rows against `max_rows` and columns against `max_columns` as its header is read; the
//! values kept (8 bytes each, the kept columns times the rows) against `max_source_bytes` before
//! any is stored. The reader makes one pass over the bytes, keeps no line and no token beyond the
//! number being converted, so its work is linear in the file and its memory the values kept. A
//! line has no length of its own: nothing is held per line.

use oce_replay::ReplayLimits;

use super::source_error::{SourceError, SourceErrorKind as Kind};

/// The slot of a column that is not kept.
const NOT_KEPT: u16 = u16::MAX;

/// The values kept of one matrix.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Matrix {
    /// The matrix's rows, as declared and read.
    pub(crate) rows: u64,
    /// One list per position asked for, in the order asked, each `rows` long.
    pub(crate) columns: Vec<Vec<f64>>,
}

/// Read the matrix `table` (or the file's one matrix) from `bytes`, keeping the columns at the
/// 1-based positions `keep`.
///
/// # Errors
/// A [`SourceError`] with no file (the caller adds it), the line, and for a value its row and
/// column: `too-many-bytes`, `format-tag-missing`, `table-syntax`, `too-many-rows`,
/// `too-many-columns`, `cell-too-long`, `column-missing` (a position past the chosen matrix's
/// columns), `bad-number`, `non-finite-number`, `size-mismatch`, `table-required`,
/// `table-not-found` or `duplicate-table`, the first met in the file.
pub(crate) fn read_matrix(
    bytes: &[u8],
    table: Option<&str>,
    keep: &[u32],
    limits: &ReplayLimits,
) -> Result<Matrix, SourceError> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limits.max_source_bytes {
        return Err(SourceError::new(
            Kind::TooManyBytes,
            format!(
                "the file is {} bytes; at most {} are read",
                bytes.len(),
                limits.max_source_bytes
            ),
        ));
    }
    if !bytes.starts_with(b"#1") {
        return Err(SourceError::new(
            Kind::FormatTagMissing,
            "a combiTimeTable file starts with the two characters `#1`",
        )
        .at_line(1));
    }
    let mut cursor = Cursor {
        bytes,
        at: 2,
        line: 1,
        max_token: limits.max_cell_bytes as usize,
    };
    cursor.skip_comment();
    let mut chosen: Option<Matrix> = None;
    let mut seen = 0_u64;
    loop {
        cursor.skip_blank();
        if cursor.at >= bytes.len() {
            break;
        }
        let header_line = cursor.line;
        let header = cursor.header(limits)?;
        seen += 1;
        let wanted = match table {
            None if seen > 1 => {
                return Err(SourceError::new(
                    Kind::TableRequired,
                    format!(
                        "the file holds a second matrix, `{}`; name the one to read with `table`",
                        header.name
                    ),
                )
                .at_line(header_line));
            }
            None => true,
            Some(name) if name == header.name && chosen.is_some() => {
                return Err(SourceError::new(
                    Kind::DuplicateTable,
                    format!("the file holds two matrices named `{name}`"),
                )
                .at_line(header_line));
            }
            Some(name) => name == header.name,
        };
        if wanted {
            chosen = Some(cursor.values(&header, Some(keep), limits)?);
        } else {
            cursor.values(&header, None, limits)?;
        }
    }
    match (chosen, table) {
        (Some(matrix), _) => Ok(matrix),
        (None, Some(name)) => Err(SourceError::new(
            Kind::TableNotFound,
            format!("no matrix of the file is named `{name}`"),
        )),
        (None, None) => Err(
            SourceError::new(Kind::TableSyntax, "the file declares no matrix").at_line(cursor.line),
        ),
    }
}

/// A matrix header as read.
struct Header {
    name: String,
    rows: u64,
    cols: u64,
    line: u64,
}

/// One pass over the file: the byte position and the line it is on.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
    line: u64,
    max_token: usize,
}

/// Whether `byte` ends a number: a separator or the start of a comment.
fn ends_token(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b',' | b';' | b'#')
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Step over one byte, counting a line feed, and a carriage return not followed by one, as
    /// a line end.
    fn bump(&mut self) {
        match self.peek() {
            Some(b'\n') => self.line += 1,
            Some(b'\r') if self.bytes.get(self.at + 1) != Some(&b'\n') => self.line += 1,
            _ => {}
        }
        self.at += 1;
    }

    /// Skip to the end of the line, the line end not included.
    fn skip_comment(&mut self) {
        while let Some(byte) = self.peek() {
            if byte == b'\n' || byte == b'\r' {
                return;
            }
            self.at += 1;
        }
    }

    /// Skip spaces, tabs, line ends and comments.
    fn skip_blank(&mut self) {
        while let Some(byte) = self.peek() {
            match byte {
                b' ' | b'\t' | b'\r' | b'\n' => self.bump(),
                b'#' => self.skip_comment(),
                _ => return,
            }
        }
    }

    /// Skip spaces and tabs on the line.
    fn skip_inline(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.at += 1;
        }
    }

    fn syntax(&self, detail: impl AsRef<str>) -> SourceError {
        SourceError::new(Kind::TableSyntax, detail).at_line(self.line)
    }

    fn expect(&mut self, byte: u8, what: &str) -> Result<(), SourceError> {
        self.skip_inline();
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.syntax(format!("the matrix header needs {what}")))
        }
    }

    /// A decimal integer of the header: digits only, at least 1, at most `u64::MAX`.
    fn dimension(&mut self, what: &str) -> Result<u64, SourceError> {
        self.skip_inline();
        let start = self.at;
        let mut value: u64 = 0;
        while let Some(digit @ b'0'..=b'9') = self.peek() {
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add(u64::from(digit - b'0')))
                .ok_or_else(|| self.syntax(format!("the matrix's {what} overflow 64 bits")))?;
            self.at += 1;
        }
        if self.at == start || value == 0 {
            return Err(self.syntax(format!(
                "the matrix's {what} is a decimal integer of at least 1"
            )));
        }
        Ok(value)
    }

    /// `double <name>(<rows>,<cols>)` and the rest of its line.
    fn header(&mut self, limits: &ReplayLimits) -> Result<Header, SourceError> {
        let line = self.line;
        if !self.bytes[self.at..].starts_with(b"double")
            || !matches!(self.bytes.get(self.at + 6), Some(b' ' | b'\t'))
        {
            return Err(self.syntax("a matrix is declared `double <name>(<rows>, <cols>)`"));
        }
        self.at += 6;
        self.skip_inline();
        let start = self.at;
        while let Some(byte) = self.peek() {
            let first = self.at == start;
            if byte == b'_' || byte.is_ascii_alphabetic() || (!first && byte.is_ascii_digit()) {
                self.at += 1;
            } else {
                break;
            }
            if self.at - start > self.max_token {
                return Err(SourceError::new(
                    Kind::CellTooLong,
                    format!("a matrix name is longer than {} bytes", self.max_token),
                )
                .at_line(line));
            }
        }
        if self.at == start {
            return Err(self.syntax("a matrix name is `[A-Za-z_][A-Za-z0-9_]*`"));
        }
        // The name is ASCII by its grammar.
        let name = String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned();
        self.expect(b'(', "`(` after the name")?;
        let rows = self.dimension("row count")?;
        self.expect(b',', "`,` between the row and column counts")?;
        let cols = self.dimension("column count")?;
        self.expect(b')', "`)` after the column count")?;
        self.skip_inline();
        match self.peek() {
            None | Some(b'\r' | b'\n' | b'#') => {}
            Some(_) => {
                return Err(self.syntax("the matrix header ends its line"));
            }
        }
        if rows > limits.max_rows {
            return Err(SourceError::new(
                Kind::TooManyRows,
                format!(
                    "matrix `{name}` declares {rows} rows; at most {} are read",
                    limits.max_rows
                ),
            )
            .at_line(line));
        }
        if cols > u64::from(limits.max_columns) {
            return Err(SourceError::new(
                Kind::TooManyColumns,
                format!(
                    "matrix `{name}` declares {cols} columns; at most {} are read",
                    limits.max_columns
                ),
            )
            .at_line(line));
        }
        Ok(Header {
            name,
            rows,
            cols,
            line,
        })
    }

    /// The separator before a value that is not a matrix's first: blanks with at most one `,`
    /// or `;`. A number ends only at a separator, a comment or the end of the file, so there is
    /// always one unless the file has ended.
    fn separator(&mut self) -> Result<(), SourceError> {
        self.skip_blank();
        if matches!(self.peek(), Some(b',' | b';')) {
            self.at += 1;
            self.skip_blank();
            if matches!(self.peek(), Some(b',' | b';')) {
                return Err(self.syntax("two separators with no number between them"));
            }
        }
        Ok(())
    }

    /// The values of `header`, keeping the 1-based positions in `keep` when given.
    fn values(
        &mut self,
        header: &Header,
        keep: Option<&[u32]>,
        limits: &ReplayLimits,
    ) -> Result<Matrix, SourceError> {
        let mut columns: Vec<Vec<f64>> = Vec::new();
        // The slot each column is kept in, `NOT_KEPT` for one not kept: two bytes a column, as a
        // matrix has at most `max_columns` (4,096) columns.
        let mut slot_of: Vec<u16> = Vec::new();
        if let Some(keep) = keep {
            for &position in keep {
                if u64::from(position) > header.cols || position == 0 {
                    return Err(SourceError::new(
                        Kind::ColumnMissing,
                        format!(
                            "matrix `{}` has {} columns, so no column {position}",
                            header.name, header.cols
                        ),
                    )
                    .at_line(header.line)
                    .at_column(&position.to_string()));
                }
            }
            let kept_bytes = u128::from(header.rows) * keep.len() as u128 * 8;
            if kept_bytes > u128::from(limits.max_source_bytes) {
                return Err(SourceError::new(
                    Kind::TooManyBytes,
                    format!(
                        "the {} columns read of matrix `{}` hold {kept_bytes} bytes; at most {} \
                         are kept",
                        keep.len(),
                        header.name,
                        limits.max_source_bytes
                    ),
                )
                .at_line(header.line));
            }
            // Each value takes at least two bytes of the file, so the file bounds the rows that
            // can follow whatever the header declares.
            let room = (self.bytes.len() - self.at) as u64 / 2 / header.cols + 1;
            let capacity = usize::try_from(header.rows.min(room)).unwrap_or(usize::MAX);
            // `header.cols` is at most `max_columns`, so this list is small.
            slot_of = vec![NOT_KEPT; usize::try_from(header.cols).unwrap_or(usize::MAX)];
            for (slot, &position) in keep.iter().enumerate() {
                slot_of[position as usize - 1] = u16::try_from(slot).unwrap_or(NOT_KEPT);
                columns.push(Vec::with_capacity(capacity));
            }
        }
        let total = u128::from(header.rows) * u128::from(header.cols);
        let mut index: u128 = 0;
        while index < total {
            let (row, column) = (
                index / u128::from(header.cols),
                index % u128::from(header.cols),
            );
            if index == 0 {
                self.skip_blank();
            } else {
                self.separator()?;
            }
            let (row, column) = (row as u64 + 1, column as u64 + 1);
            let place = |error: SourceError| error.at_row(row).at_column(&column.to_string());
            let start = self.at;
            while self.peek().is_some_and(|byte| !ends_token(byte)) {
                self.at += 1;
                if self.at - start > self.max_token {
                    return Err(place(
                        SourceError::new(
                            Kind::CellTooLong,
                            format!("a value is longer than {} bytes", self.max_token),
                        )
                        .at_line(self.line),
                    ));
                }
            }
            let token = &self.bytes[start..self.at];
            if token.is_empty() && self.peek().is_some() {
                return Err(place(
                    self.syntax("a matrix's values start with a number, not a separator"),
                ));
            }
            if token.is_empty() || token == b"double" {
                let missing = total - index;
                return Err(place(
                    SourceError::new(
                        Kind::SizeMismatch,
                        format!(
                            "matrix `{}` declares {} rows of {} columns and ends {missing} \
                             values short",
                            header.name, header.rows, header.cols
                        ),
                    )
                    .at_line(self.line),
                ));
            }
            let value = number(token).map_err(|kind| {
                let detail = match kind {
                    Kind::NonFiniteNumber => "the value's magnitude is beyond the largest f64",
                    _ => "a value is a decimal number in C syntax, such as 2.3, -2 or +2.e4",
                };
                place(SourceError::new(kind, detail).at_line(self.line))
            })?;
            match slot_of.get(column as usize - 1) {
                Some(&slot) if slot != NOT_KEPT => columns[usize::from(slot)].push(value),
                _ => {}
            }
            index += 1;
        }
        // After the last value: the end of the file, or the next matrix's header.
        self.skip_blank();
        match self.peek() {
            None => {}
            Some(b',' | b';') => {
                return Err(self.syntax(format!(
                    "a separator follows the last value of matrix `{}`",
                    header.name
                )));
            }
            Some(_) if self.bytes[self.at..].starts_with(b"double") => {}
            Some(_) => {
                return Err(SourceError::new(
                    Kind::SizeMismatch,
                    format!(
                        "matrix `{}` declares {} rows of {} columns and holds more values",
                        header.name, header.rows, header.cols
                    ),
                )
                .at_line(self.line));
            }
        }
        Ok(Matrix {
            rows: header.rows,
            columns,
        })
    }
}

/// A value: the C decimal grammar of the module documentation, then Rust's correctly rounded
/// conversion.
///
/// # Errors
/// `bad-number` for text outside the grammar; `non-finite-number` for a magnitude beyond
/// `f64::MAX`.
pub(crate) fn number(token: &[u8]) -> Result<f64, Kind> {
    let mut at = 0;
    if matches!(token.first(), Some(b'+' | b'-')) {
        at += 1;
    }
    let digits = |from: usize| {
        token[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let whole = digits(at);
    at += whole;
    let mut fraction = 0;
    if token.get(at) == Some(&b'.') {
        at += 1;
        fraction = digits(at);
        at += fraction;
    }
    if whole == 0 && fraction == 0 {
        return Err(Kind::BadNumber);
    }
    if matches!(token.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(token.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let exponent = digits(at);
        if exponent == 0 {
            return Err(Kind::BadNumber);
        }
        at += exponent;
    }
    if at != token.len() {
        return Err(Kind::BadNumber);
    }
    // ASCII by the grammar just checked; Rust's grammar is a superset of it.
    let text = std::str::from_utf8(token).map_err(|_| Kind::BadNumber)?;
    let value: f64 = text.parse().map_err(|_| Kind::BadNumber)?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Kind::NonFiniteNumber)
    }
}

#[cfg(test)]
#[path = "combi_time_table_tests.rs"]
mod tests;
