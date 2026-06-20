//! Self-contained, byte-stable Modelica `CombiTimeTable` CSV writer (`_spec/07` §9.1).
//!
//! This is a deliberate re-implementation of the I/O FORMAT only (the `#1` version marker,
//! `double <name>(rows,cols)` shape line, time in column 0, row-major data, single-space
//! separator, `ryu` shortest-round-trip f64 formatting). It carries NO block math.
//!
//! Unlike the `oce-conformance` writer it does NOT reject non-finite cells: the IEEE Divide
//! oracle legitimately emits `+inf` / `-inf` / `NaN` (compared by IEEE class, not by value), so
//! those must round-trip into the golden. For finite values the `ryu` output is byte-identical to
//! the conformance writer, keeping goldens interoperable.

/// Format `value` with the deterministic shortest-round-trip representation used for CSV goldens.
///
/// Mirrors `oce_conformance::csv::format_f64`: delegates to `ryu`, which emits `inf` / `-inf` /
/// `NaN` for the non-finite cases. Byte-identical to the conformance writer for finite inputs.
#[must_use]
pub fn format_f64(value: f64) -> String {
    let mut buffer = ryu::Buffer::new();
    buffer.format(value).to_string()
}

/// A single named, time-stamped scalar signal column.
///
/// `time` and `values` are parallel and equal length. `values` are already encoded to the numeric
/// CombiTimeTable convention (Boolean -> 0.0/1.0, Integer -> exact f64, Real -> f64 payload).
pub struct SignalColumn {
    /// Output connector name (becomes the value column header).
    pub name: String,
    /// Time samples in seconds, non-decreasing, parallel to `values`.
    pub time: Vec<f64>,
    /// Encoded numeric samples parallel to `time`.
    pub values: Vec<f64>,
}

/// A single named numeric data column parallel to a shared time vector.
pub struct DataColumn {
    /// Column name.
    pub name: String,
    /// Encoded numeric samples parallel to the table time vector.
    pub values: Vec<f64>,
}

/// Serialize one signal as a deterministic two-column `CombiTimeTable` CSV (`time`, `<name>`).
///
/// Emits the `#1` header, a structured `# columns:` comment, the `double <name>(rows,2)` shape
/// line, then row-major `time value` rows with a single-space separator. Byte-stable.
///
/// # Panics
/// Panics if `time` and `values` differ in length (a generator bug, not a runtime condition).
#[must_use]
pub fn to_csv_string(table_name: &str, col: &SignalColumn) -> String {
    assert_eq!(
        col.time.len(),
        col.values.len(),
        "time/value length mismatch for signal {}",
        col.name
    );
    to_table_csv_string(
        table_name,
        &col.time,
        &[DataColumn {
            name: col.name.clone(),
            values: col.values.clone(),
        }],
    )
}

/// Serialize multiple columns as one deterministic `CombiTimeTable` CSV.
///
/// Emits the `#1` header, a structured `# columns:` comment, the
/// `double <name>(rows,cols)` shape line, then row-major data with a single-space separator.
///
/// # Panics
/// Panics if any data column length differs from `time.len()`.
#[must_use]
pub fn to_table_csv_string(table_name: &str, time: &[f64], cols: &[DataColumn]) -> String {
    for col in cols {
        assert_eq!(
            time.len(),
            col.values.len(),
            "time/value length mismatch for signal {}",
            col.name
        );
    }
    let n_rows = time.len();
    let n_cols = cols.len() + 1;
    let mut out = String::new();
    out.push_str("#1\n");
    out.push_str("# columns: time");
    for col in cols {
        out.push(' ');
        out.push_str(&col.name);
    }
    out.push('\n');
    out.push_str(&format!("double {table_name}({n_rows},{n_cols})\n"));
    for (row, t) in time.iter().enumerate() {
        out.push_str(&format_f64(*t));
        for col in cols {
            out.push(' ');
            out.push_str(&format_f64(col.values[row]));
        }
        out.push('\n');
    }
    out
}
