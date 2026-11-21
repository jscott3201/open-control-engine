//! Validation helpers for flattened CDL array and matrix parameters.

use std::collections::BTreeMap;

use oce_blocks::{MAX_RESOLVED_PORT_WIDTH, TimeTableValues};
use oce_diag::{DiagCode, Diagnostic, DiagnosticList};
use oce_model::{BlockInstance, ParamTable, Value};

use super::block_subject_of;
use super::params_absent::absent_in;
use super::params_element_names::is_canonical_index;

pub(super) struct RealMatrixRule {
    pub(super) base: &'static str,
    pub(super) rows: &'static str,
    pub(super) default_rows: i64,
    pub(super) cols: &'static str,
    pub(super) default_cols: i64,
}

pub(super) struct TimeTableRule {
    pub(super) base: &'static str,
    pub(super) values: TimeTableValues,
    pub(super) time_scale: &'static str,
    pub(super) period: Option<&'static str>,
    pub(super) extrapolation: Option<&'static str>,
}

pub(super) fn check_real_matrix_elements(
    blk: &BlockInstance,
    diags: &mut DiagnosticList,
    rule: RealMatrixRule,
) {
    let (Some(n_row), Some(n_col)) = (
        integer_param_or_default(&blk.params, rule.rows, rule.default_rows),
        integer_param_or_default(&blk.params, rule.cols, rule.default_cols),
    ) else {
        return;
    };
    let (Ok(n_row), Ok(n_col)) = (usize::try_from(n_row), usize::try_from(n_col)) else {
        return;
    };
    if n_row
        .checked_mul(n_col)
        .is_none_or(|width| width > MAX_RESOLVED_PORT_WIDTH)
    {
        return;
    }
    let prefix = format!("{}_", rule.base);
    for (name, value) in &blk.params.values {
        let name = name.as_ref();
        let Some(suffix) = name.strip_prefix(&prefix) else {
            continue;
        };
        let Some((row, col)) = parse_matrix_element_suffix(suffix) else {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must use one-based `{}_row_col` \
                     matrix-element naming",
                    blk.class_iri, rule.base
                ),
            );
            continue;
        };
        if row == 0 || col == 0 || row > n_row || col > n_col {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` is outside resolved matrix shape \
                     {n_row}x{n_col}",
                    blk.class_iri
                ),
            );
            continue;
        }
        if real_value(value).is_none() {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be a numeric real matrix element",
                    blk.class_iri
                ),
            );
        }
    }
}

pub(super) fn check_time_table_matrix(
    blk: &BlockInstance,
    diags: &mut DiagnosticList,
    rule: TimeTableRule,
) {
    let prefix = format!("{}_", rule.base);
    // The bound cells by (row, column): row-major order, the order every finding about them is
    // raised in. A repeated name keeps its last binding (a repeat is refused on its own).
    let mut cells = BTreeMap::<(usize, usize), &Value>::new();
    let mut n_row = 0_usize;
    let mut n_col = 0_usize;
    for (name, value) in &blk.params.values {
        let name = name.as_ref();
        let Some(suffix) = name.strip_prefix(&prefix) else {
            continue;
        };
        let Some((row, col)) = parse_matrix_element_suffix(suffix) else {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must use one-based `{}_row_col` \
                     table-element naming",
                    blk.class_iri, rule.base
                ),
            );
            continue;
        };
        if row == 0 || col == 0 {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must use positive one-based indexes",
                    blk.class_iri
                ),
            );
            continue;
        }
        n_row = n_row.max(row);
        n_col = n_col.max(col);
        cells.insert((row, col), value);
    }
    if cells.is_empty() {
        diags.push(
            Diagnostic::error(
                DiagCode::MissingRequiredParameter,
                format!(
                    "block `{}` is missing required flattened table parameter `{}`",
                    blk.class_iri, rule.base
                ),
            )
            .with_subject(block_subject_of(blk)),
        );
        return;
    }
    if n_col < 2 {
        push_range_error(
            blk,
            diags,
            format!(
                "table `{}` on block `{}` must have at least time plus one output column",
                rule.base, blk.class_iri
            ),
        );
        return;
    }
    if n_row
        .checked_mul(n_col)
        .is_none_or(|width| width > MAX_RESOLVED_PORT_WIDTH)
    {
        push_range_error(
            blk,
            diags,
            format!(
                "table `{}` on block `{}` exceeds maximum flattened cell count {}",
                rule.base, blk.class_iri, MAX_RESOLVED_PORT_WIDTH
            ),
        );
        return;
    }

    // Every bound cell is inside the shape, which its largest row and column set, so the absent
    // cells are found from the bound ones: one finding for all of them (issue #726).
    let linear = |(row, col): (usize, usize)| (row - 1) * n_col + col;
    let absent = absent_in(cells.keys().map(|&cell| linear(cell)), 1, n_row * n_col);
    let cell_name = |position: usize| {
        let (row, col) = ((position - 1) / n_col + 1, (position - 1) % n_col + 1);
        format!("{}_{row}_{col}", rule.base)
    };
    match absent.count {
        0 => {}
        1 => push_range_error(
            blk,
            diags,
            format!(
                "table `{}` on block `{}` is missing required cell {}",
                rule.base,
                blk.class_iri,
                absent.named(cell_name)
            ),
        ),
        count => push_range_error(
            blk,
            diags,
            format!(
                "table `{}` on block `{}` is missing {count} required cells of its \
                 {n_row}x{n_col} shape: {}",
                rule.base,
                blk.class_iri,
                absent.named(cell_name)
            ),
        ),
    }

    // The time column as bound, in row order. Its checks below run when every row has a numeric
    // time, whether or not an output cell is absent, as they did when the shape was walked. It
    // grows with the time cells bound, never with a shape no cell fills.
    let mut times = Vec::new();
    for (&(row, col), &value) in &cells {
        let Some(value) = real_value(value) else {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{}_{}_{}` on block `{}` must be numeric",
                    rule.base, row, col, blk.class_iri
                ),
            );
            continue;
        };
        if col == 1 {
            if !value.is_finite() {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "time cell `{}_{}_1` on block `{}` must be finite; got {value}",
                        rule.base, row, blk.class_iri
                    ),
                );
            }
            times.push(value);
        } else {
            check_table_output_value(blk, diags, rule.base, row, col, value, rule.values);
        }
    }
    if times.len() != n_row {
        return;
    }
    let time_scale = find_param(&blk.params, rule.time_scale)
        .map_or(Some(1.0), real_value)
        .filter(|value| value.is_finite() && *value > 0.0);
    let Some(time_scale) = time_scale else {
        return;
    };
    let scaled_times: Vec<f64> = times.iter().map(|time| time * time_scale).collect();
    for idx in 1..scaled_times.len() {
        if scaled_times[idx] < scaled_times[idx - 1] {
            push_range_error(
                blk,
                diags,
                format!(
                    "table `{}` on block `{}` must have nondecreasing time values; row {} is \
                     before row {}",
                    rule.base,
                    blk.class_iri,
                    idx + 1,
                    idx
                ),
            );
            break;
        }
    }
    if let Some(period_name) = rule.period {
        validate_periodic_step_table(blk, diags, rule.base, period_name, &times, &scaled_times);
    }
    if let Some(extrapolation_name) = rule.extrapolation {
        let periodic =
            find_param(&blk.params, extrapolation_name).is_none_or(extrapolation_is_periodic);
        if periodic
            && scaled_times.len() > 1
            && scaled_times[scaled_times.len() - 1] <= scaled_times[0]
        {
            push_range_error(
                blk,
                diags,
                format!(
                    "periodic table `{}` on block `{}` must span a positive time range",
                    rule.base, blk.class_iri
                ),
            );
        }
    }
}

pub(super) fn check_time_table_offset(
    blk: &BlockInstance,
    diags: &mut DiagnosticList,
    base: &str,
    table: &str,
) {
    let Some((_, n_col)) = infer_matrix_shape(&blk.params, table) else {
        return;
    };
    if n_col < 2 {
        return;
    }
    let nout = n_col - 1;
    let prefix = format!("{base}_");
    for (name, value) in &blk.params.values {
        let name = name.as_ref();
        let Some(suffix) = name.strip_prefix(&prefix) else {
            continue;
        };
        let Some(idx) = parse_array_element_suffix(suffix) else {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must use one-based `{base}_index` \
                     vector-element naming",
                    blk.class_iri
                ),
            );
            continue;
        };
        if idx == 0 || idx > nout {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` is outside resolved offset length {nout}",
                    blk.class_iri
                ),
            );
            continue;
        }
        if real_value(value).is_none() {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be a numeric Real offset element",
                    blk.class_iri
                ),
            );
        }
    }
}

pub(super) fn check_boolean_array_true_count_equals(
    blk: &BlockInstance,
    diags: &mut DiagnosticList,
    base: &str,
    len: &str,
    count: &str,
    default: bool,
) {
    let (Some(len_raw), Some(count_raw)) =
        (find_param(&blk.params, len), find_param(&blk.params, count))
    else {
        return;
    };
    let (Some(n), Some(expected)) = (integer_value(len_raw), integer_value(count_raw)) else {
        return;
    };
    let Ok(n) = usize::try_from(n) else {
        return;
    };
    if n > MAX_RESOLVED_PORT_WIDTH {
        return;
    }
    // The members `base_1` to `base_n` the block binds, each its first binding, read in one pass
    // over the parameters (GitHub #634). A bound member that is not Boolean ends the rule (the
    // element rule refuses it), and each member not bound is `default`, counted rather than
    // walked, so the work is the parameters' and never `n`'s (issue #726).
    let mut members = BTreeMap::<usize, &Value>::new();
    for (name, value) in &blk.params.values {
        let index = name
            .strip_prefix(base)
            .and_then(|rest| rest.strip_prefix('_'))
            .filter(|index| is_canonical_index(index))
            .and_then(|index| index.parse::<usize>().ok())
            .filter(|index| (1..=n).contains(index));
        if let Some(index) = index {
            members.entry(index).or_insert(value);
        }
    }
    let mut true_count = 0_i64;
    for value in members.values() {
        let Some(value) = boolean_value(value) else {
            return;
        };
        true_count += i64::from(value);
    }
    if default {
        // At most `n <= MAX_RESOLVED_PORT_WIDTH` members, so the count fits an `i64`.
        true_count += (n - members.len()) as i64;
    }
    if true_count != expected {
        push_range_error(
            blk,
            diags,
            format!(
                "Boolean mask `{base}` on block `{}` has true count {true_count}, expected \
                 `{count}` = {expected}",
                blk.class_iri
            ),
        );
    }
}

pub(super) fn find_param<'a>(params: &'a ParamTable, name: &str) -> Option<&'a Value> {
    params
        .values
        .iter()
        .find(|(n, _)| n.as_ref() == name)
        .map(|(_, v)| v)
}

fn infer_matrix_shape(params: &ParamTable, base: &str) -> Option<(usize, usize)> {
    let prefix = format!("{base}_");
    let mut n_row = 0_usize;
    let mut n_col = 0_usize;
    for (name, _) in &params.values {
        let Some(suffix) = name.as_ref().strip_prefix(&prefix) else {
            continue;
        };
        let Some((row, col)) = parse_matrix_element_suffix(suffix) else {
            continue;
        };
        n_row = n_row.max(row);
        n_col = n_col.max(col);
    }
    Some((n_row, n_col)).filter(|(row, col)| *row > 0 && *col > 0)
}

pub(super) fn real_value(value: &Value) -> Option<f64> {
    match value {
        Value::Real(v) => Some(*v),
        Value::Integer(v) => Some(*v as f64),
        _ => None,
    }
}

pub(super) fn integer_value(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(v) => Some(*v),
        _ => None,
    }
}

pub(super) fn integer_param_or_default(
    params: &ParamTable,
    name: &str,
    default: i64,
) -> Option<i64> {
    find_param(params, name).map_or(Some(default), integer_value)
}

fn parse_matrix_element_suffix(suffix: &str) -> Option<(usize, usize)> {
    let (row, col) = suffix.split_once('_')?;
    if col.contains('_') {
        return None;
    }
    Some((row.parse().ok()?, col.parse().ok()?))
}

fn parse_array_element_suffix(suffix: &str) -> Option<usize> {
    if suffix.contains('_') {
        return None;
    }
    suffix.parse().ok()
}

fn check_table_output_value(
    blk: &BlockInstance,
    diags: &mut DiagnosticList,
    base: &str,
    row: usize,
    col: usize,
    value: f64,
    kind: TimeTableValues,
) {
    match kind {
        TimeTableValues::Real => {}
        TimeTableValues::Integer if integer_encoded(value) => {}
        TimeTableValues::Integer => push_range_error(
            blk,
            diags,
            format!(
                "table value `{base}_{row}_{col}` on block `{}` must encode an Integer; got \
                 {value}",
                blk.class_iri
            ),
        ),
        TimeTableValues::Boolean
            if (value.abs() < TIMETABLE_SMALL) || ((value - 1.0).abs() < TIMETABLE_SMALL) => {}
        TimeTableValues::Boolean => push_range_error(
            blk,
            diags,
            format!(
                "table value `{base}_{row}_{col}` on block `{}` must encode Boolean 0 or 1; got \
                 {value}",
                blk.class_iri
            ),
        ),
    }
}

const TIMETABLE_SMALL: f64 = 1.0e-37;
const MIN_TIMETABLE_PERIOD: f64 = 1.0e-6;

fn integer_encoded(value: f64) -> bool {
    value.is_finite() && (value - value.floor()).abs() < TIMETABLE_SMALL
}

fn validate_periodic_step_table(
    blk: &BlockInstance,
    diags: &mut DiagnosticList,
    base: &str,
    period_name: &str,
    unscaled_times: &[f64],
    scaled_times: &[f64],
) {
    if unscaled_times
        .first()
        .is_some_and(|first| first.abs() >= TIMETABLE_SMALL)
    {
        push_range_error(
            blk,
            diags,
            format!(
                "table `{base}` on block `{}` must start at time 0 for periodic step lookup",
                blk.class_iri
            ),
        );
    }
    let Some(period) = find_param(&blk.params, period_name).and_then(real_value) else {
        return;
    };
    if !period.is_finite() || period < MIN_TIMETABLE_PERIOD {
        return;
    }
    if scaled_times
        .last()
        .is_some_and(|last| period - *last <= TIMETABLE_SMALL)
    {
        push_range_error(
            blk,
            diags,
            format!(
                "last time stamp in table `{base}` on block `{}` must be smaller than \
                 `{period_name}`",
                blk.class_iri
            ),
        );
    }
}

fn extrapolation_is_periodic(value: &Value) -> bool {
    match value {
        Value::Enum { ordinal: 3, .. } => true,
        Value::Integer(3) => true,
        Value::String(s) => s
            .rsplit('.')
            .next()
            .is_some_and(|member| member == "Periodic"),
        _ => false,
    }
}

pub(super) fn boolean_value(value: &Value) -> Option<bool> {
    match value {
        Value::Boolean(v) => Some(*v),
        _ => None,
    }
}

pub(super) fn push_range_error(blk: &BlockInstance, diags: &mut DiagnosticList, message: String) {
    diags.push(
        Diagnostic::error(DiagCode::ParameterOutOfRange, message)
            .with_subject(block_subject_of(blk)),
    );
}
