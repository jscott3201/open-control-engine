//! Validation helpers for flattened CDL array and matrix parameters.

use std::collections::HashMap;

use oce_blocks::{MAX_RESOLVED_PORT_WIDTH, TimeTableValues};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{BlockInstance, ParamTable, Value};

use super::block_subject_of;

pub(super) struct IntegerArrayRangeRule {
    pub(super) base: &'static str,
    pub(super) len: &'static str,
    pub(super) len_default: i64,
    pub(super) min: i64,
    pub(super) max: &'static str,
    pub(super) max_default: i64,
    pub(super) default_to_index: bool,
}

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

pub(super) fn check_integer_array_elements(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
    base: &str,
    len: &str,
) {
    let Some(value) = find_param(&blk.params, len) else {
        return;
    };
    let Some(n) = integer_value(value) else {
        return;
    };
    let Ok(n) = usize::try_from(n) else {
        return;
    };
    for idx in 1..=n.min(MAX_RESOLVED_PORT_WIDTH) {
        let name = format!("{base}_{idx}");
        let Some(value) = find_param(&blk.params, &name) else {
            continue;
        };
        if integer_value(value).is_none() {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be an integer array element",
                    blk.class_iri
                ),
            );
        }
    }
}

pub(super) fn check_integer_array_elements_in_range(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
    rule: IntegerArrayRangeRule,
) {
    let Some(n) = integer_param_or_default(&blk.params, rule.len, rule.len_default) else {
        return;
    };
    let Some(upper) = integer_param_or_default(&blk.params, rule.max, rule.max_default) else {
        return;
    };
    let Ok(n) = usize::try_from(n) else {
        return;
    };
    for idx in 1..=n.min(MAX_RESOLVED_PORT_WIDTH) {
        let name = format!("{}_{idx}", rule.base);
        let member = match find_param(&blk.params, &name) {
            Some(value) => {
                let Some(value) = integer_value(value) else {
                    push_range_error(
                        blk,
                        diags,
                        format!(
                            "parameter `{name}` on block `{}` must be an integer array element",
                            blk.class_iri
                        ),
                    );
                    continue;
                };
                value
            }
            None if rule.default_to_index => idx as i64,
            None => continue,
        };
        if member < rule.min || member > upper {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be in range {}..={upper}; got {member}",
                    blk.class_iri, rule.min
                ),
            );
        }
    }
}

pub(super) fn check_real_array_elements(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
    base: &str,
    len: &str,
) {
    let Some(value) = find_param(&blk.params, len) else {
        return;
    };
    let Some(n) = integer_value(value) else {
        return;
    };
    let Ok(n) = usize::try_from(n) else {
        return;
    };
    for idx in 1..=n.min(MAX_RESOLVED_PORT_WIDTH) {
        let name = format!("{base}_{idx}");
        let Some(value) = find_param(&blk.params, &name) else {
            continue;
        };
        if real_value(value).is_none() {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be a numeric real array element",
                    blk.class_iri
                ),
            );
        }
    }
}

pub(super) fn check_real_matrix_elements(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
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
    diags: &mut Vec<Diagnostic>,
    rule: TimeTableRule,
) {
    let prefix = format!("{}_", rule.base);
    let mut cells = HashMap::<(usize, usize), &Value>::new();
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

    let mut times = Vec::with_capacity(n_row);
    for row in 1..=n_row {
        for col in 1..=n_col {
            let Some(value) = cells.get(&(row, col)).copied() else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "table `{}` on block `{}` is missing required cell {}_{}_{}",
                        rule.base, blk.class_iri, rule.base, row, col
                    ),
                );
                continue;
            };
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
    diags: &mut Vec<Diagnostic>,
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

pub(super) fn check_boolean_array_elements(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
    base: &str,
    len: &str,
) {
    let Some(value) = find_param(&blk.params, len) else {
        return;
    };
    let Some(n) = integer_value(value) else {
        return;
    };
    let Ok(n) = usize::try_from(n) else {
        return;
    };
    for idx in 1..=n.min(MAX_RESOLVED_PORT_WIDTH) {
        let name = format!("{base}_{idx}");
        let Some(value) = find_param(&blk.params, &name) else {
            continue;
        };
        if boolean_value(value).is_none() {
            push_range_error(
                blk,
                diags,
                format!(
                    "parameter `{name}` on block `{}` must be a Boolean array element",
                    blk.class_iri
                ),
            );
        }
    }
}

pub(super) fn check_boolean_array_true_count_equals(
    blk: &BlockInstance,
    diags: &mut Vec<Diagnostic>,
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
    let mut true_count = 0_i64;
    for idx in 1..=n {
        let name = format!("{base}_{idx}");
        let value = match find_param(&blk.params, &name) {
            Some(value) => {
                let Some(value) = boolean_value(value) else {
                    return;
                };
                value
            }
            None => default,
        };
        if value {
            true_count += 1;
        }
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

fn find_param<'a>(params: &'a ParamTable, name: &str) -> Option<&'a Value> {
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

fn real_value(value: &Value) -> Option<f64> {
    match value {
        Value::Real(v) => Some(*v),
        Value::Integer(v) => Some(*v as f64),
        _ => None,
    }
}

fn integer_value(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(v) => Some(*v),
        _ => None,
    }
}

fn integer_param_or_default(params: &ParamTable, name: &str, default: i64) -> Option<i64> {
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
    diags: &mut Vec<Diagnostic>,
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
    diags: &mut Vec<Diagnostic>,
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

fn boolean_value(value: &Value) -> Option<bool> {
    match value {
        Value::Boolean(v) => Some(*v),
        _ => None,
    }
}

fn push_range_error(blk: &BlockInstance, diags: &mut Vec<Diagnostic>, message: String) {
    diags.push(
        Diagnostic::error(DiagCode::ParameterOutOfRange, message)
            .with_subject(block_subject_of(blk)),
    );
}
