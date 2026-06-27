//! Class-level block-parameter validation.

use std::cmp::Ordering;

use oce_blocks::{MAX_RESOLVED_PORT_WIDTH, ParamRule, lookup};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::{BlockInstance, ModelGraph, ParamTable, Value};

use super::block_subject_of;

/// Check raw resolved block parameters against the registry-published class rules.
///
/// Unknown classes are skipped here for parity with `check_port_types`: `oce-api` owns the typed
/// load error for unresolved block implementations.
pub(crate) fn check_block_params(model: &ModelGraph, diags: &mut Vec<Diagnostic>) {
    for blk in &model.blocks {
        let Some(entry) = lookup(&blk.class_iri) else {
            continue;
        };
        for rule in entry.param_rules() {
            check_rule(blk, *rule, diags);
        }
    }
}

fn check_rule(blk: &BlockInstance, rule: ParamRule, diags: &mut Vec<Diagnostic>) {
    match rule {
        ParamRule::Required { name } if find_param(&blk.params, name).is_none() => {
            diags.push(
                Diagnostic::error(
                    DiagCode::MissingRequiredParameter,
                    format!(
                        "block `{}` is missing required parameter `{name}`",
                        blk.class_iri
                    ),
                )
                .with_subject(block_subject_of(blk)),
            );
        }
        ParamRule::Required { .. } => {}
        ParamRule::Structural { .. } => {}
        ParamRule::StructuralArrayElements { .. } => {}
        ParamRule::Real { name } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            if real_value(value).is_none() {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be numeric",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::RealFinite { name } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            let Some(v) = real_value(value) else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be numeric and finite",
                        blk.class_iri
                    ),
                );
                return;
            };
            if !v.is_finite() {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be finite; got {v}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::RealGreaterThan { name, min } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            let Some(v) = real_value(value) else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be numeric and > {min}",
                        blk.class_iri
                    ),
                );
                return;
            };
            if !real_greater_than(v, min) {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be > {min}; got {v}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::RealFiniteGreaterThan { name, min } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            let Some(v) = real_value(value) else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be numeric, finite, and > {min}",
                        blk.class_iri
                    ),
                );
                return;
            };
            if !v.is_finite() || !real_greater_than(v, min) {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be finite and > {min}; got {v}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::IntegerGreaterOrEqual { name, min } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            let Some(v) = integer_value(value) else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be an integer and >= {min}",
                        blk.class_iri
                    ),
                );
                return;
            };
            if v < min {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be >= {min}; got {v}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::IntegerLessOrEqualConstant { name, max } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            let Some(v) = integer_value(value) else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be an integer and <= {max}",
                        blk.class_iri
                    ),
                );
                return;
            };
            if v > max {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be <= {max}; got {v}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::IntegerArrayElements { base, len } => {
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
        ParamRule::IntegerArrayElementsInRange {
            base,
            len,
            len_default,
            min,
            max,
            max_default,
            default_to_index,
        } => {
            let Some(n) = integer_param_or_default(&blk.params, len, len_default) else {
                return;
            };
            let Some(upper) = integer_param_or_default(&blk.params, max, max_default) else {
                return;
            };
            let Ok(n) = usize::try_from(n) else {
                return;
            };
            for idx in 1..=n.min(MAX_RESOLVED_PORT_WIDTH) {
                let name = format!("{base}_{idx}");
                let member = match find_param(&blk.params, &name) {
                    Some(value) => {
                        let Some(value) = integer_value(value) else {
                            push_range_error(
                                blk,
                                diags,
                                format!(
                                    "parameter `{name}` on block `{}` must be an integer array \
                                     element",
                                    blk.class_iri
                                ),
                            );
                            continue;
                        };
                        value
                    }
                    None if default_to_index => idx as i64,
                    None => continue,
                };
                if member < min || member > upper {
                    push_range_error(
                        blk,
                        diags,
                        format!(
                            "parameter `{name}` on block `{}` must be in range {min}..={upper}; \
                             got {member}",
                            blk.class_iri
                        ),
                    );
                }
            }
        }
        ParamRule::RealArrayElements { base, len } => {
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
        ParamRule::BooleanArrayElements { base, len } => {
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
        ParamRule::BooleanArrayTrueCountEquals {
            base,
            len,
            count,
            default,
        } => {
            let (Some(len_raw), Some(count_raw)) =
                (find_param(&blk.params, len), find_param(&blk.params, count))
            else {
                return;
            };
            let (Some(n), Some(expected)) = (integer_value(len_raw), integer_value(count_raw))
            else {
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
                        "Boolean mask `{base}` on block `{}` has true count {true_count}, \
                         expected `{count}` = {expected}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::EnumMembers { name, members } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            if !enum_member_value(value, members) {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be one of {} source enum members",
                        blk.class_iri,
                        members.len()
                    ),
                );
            }
        }
        ParamRule::RealGreaterOrEqual { name, min } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            let Some(v) = real_value(value) else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be numeric and >= {min}",
                        blk.class_iri
                    ),
                );
                return;
            };
            if !real_greater_or_equal(v, min) {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be >= {min}; got {v}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::RealLessOrEqualConstant { name, max } => {
            let Some(value) = find_param(&blk.params, name) else {
                return;
            };
            let Some(v) = real_value(value) else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be numeric and <= {max}",
                        blk.class_iri
                    ),
                );
                return;
            };
            if !real_less_or_equal(v, max) {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{name}` on block `{}` must be <= {max}; got {v}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::RealTimesIntegerInclusiveRange {
            real,
            integer,
            min,
            max,
        } => {
            let (Some(real_raw), Some(integer_raw)) = (
                find_param(&blk.params, real),
                find_param(&blk.params, integer),
            ) else {
                return;
            };
            let (Some(real_param), Some(integer_param)) =
                (real_value(real_raw), integer_value(integer_raw))
            else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameters `{real}` and `{integer}` on block `{}` must be numeric \
                         and integer, respectively",
                        blk.class_iri
                    ),
                );
                return;
            };
            let scaled = real_param * integer_param as f64;
            if !real_greater_or_equal(scaled, min) || !real_less_or_equal(scaled, max) {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameter `{real}` on block `{}` must satisfy {min} <= \
                         {real}*{integer} <= {max}; got {scaled}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::IntegerProductLessOrEqualConstant { left, right, max } => {
            let (Some(left_raw), Some(right_raw)) = (
                find_param(&blk.params, left),
                find_param(&blk.params, right),
            ) else {
                return;
            };
            let (Some(left_value), Some(right_value)) =
                (integer_value(left_raw), integer_value(right_raw))
            else {
                return;
            };
            let Some(product) = left_value.checked_mul(right_value) else {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameters `{left}` and `{right}` on block `{}` overflow their product",
                        blk.class_iri
                    ),
                );
                return;
            };
            if product > max {
                push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameters `{left}` and `{right}` on block `{}` must satisfy \
                         {left}*{right} <= {max}; got {product}",
                        blk.class_iri
                    ),
                );
            }
        }
        ParamRule::RealLessOrEqual { lower, upper } => {
            let (Some(lo), Some(hi)) = (
                find_param(&blk.params, lower).map(real_value),
                find_param(&blk.params, upper).map(real_value),
            ) else {
                return;
            };
            match (lo, hi) {
                (Some(lo), Some(hi)) if real_less_or_equal(lo, hi) => {}
                (Some(lo), Some(hi)) => push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameters `{lower}` and `{upper}` on block `{}` must satisfy \
                         {lower} <= {upper}; got {lo} > {hi}",
                        blk.class_iri
                    ),
                ),
                _ => push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameters `{lower}` and `{upper}` on block `{}` must both be numeric",
                        blk.class_iri
                    ),
                ),
            }
        }
        ParamRule::RealLessOrEqualWarning { lower, upper } => {
            let (Some(lo), Some(hi)) = (
                find_param(&blk.params, lower).map(real_value),
                find_param(&blk.params, upper).map(real_value),
            ) else {
                return;
            };
            match (lo, hi) {
                (Some(lo), Some(hi)) if real_less_or_equal(lo, hi) => {}
                (Some(lo), Some(hi)) => diags.push(
                    Diagnostic::warning(
                        DiagCode::ParameterOutOfRange,
                        format!(
                            "parameters `{lower}` and `{upper}` on block `{}` should satisfy \
                             {lower} <= {upper}; got {lo} > {hi}",
                            blk.class_iri
                        ),
                    )
                    .with_subject(block_subject_of(blk)),
                ),
                _ => push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameters `{lower}` and `{upper}` on block `{}` must both be numeric",
                        blk.class_iri
                    ),
                ),
            }
        }
        ParamRule::RealGreaterOrEqualScaledWarning {
            left,
            right,
            factor,
        } => {
            let (Some(left_value), Some(right_value)) = (
                find_param(&blk.params, left).map(real_value),
                find_param(&blk.params, right).map(real_value),
            ) else {
                return;
            };
            match (left_value, right_value) {
                (Some(left_value), Some(right_value))
                    if real_greater_or_equal(left_value, right_value * factor) => {}
                (Some(left_value), Some(right_value)) => diags.push(
                    Diagnostic::warning(
                        DiagCode::ParameterOutOfRange,
                        format!(
                            "parameters `{left}` and `{right}` on block `{}` should satisfy \
                             {left} >= {factor}*{right}; got {left_value} < {}",
                            blk.class_iri,
                            right_value * factor
                        ),
                    )
                    .with_subject(block_subject_of(blk)),
                ),
                _ => push_range_error(
                    blk,
                    diags,
                    format!(
                        "parameters `{left}` and `{right}` on block `{}` must both be numeric",
                        blk.class_iri
                    ),
                ),
            }
        }
        ParamRule::RealEqualWarning { left, right } => {
            let (Some(left_value), Some(right_value)) = (
                find_param(&blk.params, left).and_then(real_value),
                find_param(&blk.params, right).and_then(real_value),
            ) else {
                return;
            };
            if real_equal(left_value, right_value) {
                diags.push(
                    Diagnostic::warning(
                        DiagCode::ParameterOutOfRange,
                        format!(
                            "parameters `{left}` and `{right}` on block `{}` are equal; Limiter \
                             will clamp to a constant",
                            blk.class_iri
                        ),
                    )
                    .with_subject(block_subject_of(blk)),
                );
            }
        }
        _ => {}
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

fn boolean_value(value: &Value) -> Option<bool> {
    match value {
        Value::Boolean(v) => Some(*v),
        _ => None,
    }
}

fn enum_member_value(value: &Value, members: &[&str]) -> bool {
    match value {
        Value::Enum { ordinal, .. } => {
            usize::try_from(*ordinal).is_ok_and(|ordinal| (1..=members.len()).contains(&ordinal))
        }
        Value::Integer(v) => {
            usize::try_from(*v).is_ok_and(|ordinal| (1..=members.len()).contains(&ordinal))
        }
        Value::String(s) => s
            .rsplit('.')
            .next()
            .is_some_and(|member| members.contains(&member)),
        _ => false,
    }
}

fn real_greater_than(value: f64, min: f64) -> bool {
    matches!(value.partial_cmp(&min), Some(Ordering::Greater))
}

fn real_greater_or_equal(value: f64, min: f64) -> bool {
    matches!(
        value.partial_cmp(&min),
        Some(Ordering::Greater | Ordering::Equal)
    )
}

fn real_less_or_equal(lower: f64, upper: f64) -> bool {
    matches!(
        lower.partial_cmp(&upper),
        Some(Ordering::Less | Ordering::Equal)
    )
}

fn real_equal(left: f64, right: f64) -> bool {
    matches!(left.partial_cmp(&right), Some(Ordering::Equal))
}

fn push_range_error(blk: &BlockInstance, diags: &mut Vec<Diagnostic>, message: String) {
    diags.push(
        Diagnostic::error(DiagCode::ParameterOutOfRange, message)
            .with_subject(block_subject_of(blk)),
    );
}
