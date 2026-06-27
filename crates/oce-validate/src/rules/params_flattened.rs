//! Validation helpers for flattened CDL array and matrix parameters.

use oce_blocks::MAX_RESOLVED_PORT_WIDTH;
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
