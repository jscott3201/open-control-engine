//! Class-level block-parameter validation.

use std::cmp::Ordering;

use oce_blocks::{ParamRule, lookup};
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
        ParamRule::Required { name } => {
            if find_param(&blk.params, name).is_none() {
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

fn real_greater_than(value: f64, min: f64) -> bool {
    matches!(value.partial_cmp(&min), Some(Ordering::Greater))
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
