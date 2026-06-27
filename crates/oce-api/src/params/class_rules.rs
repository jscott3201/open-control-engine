//! Registry-published parameter rule checks for tune-at-rest edits.

use std::cmp::Ordering;

use oce_blocks::{ParamRule, TimeTableValues, lookup};
use oce_model::{BlockId, Value};
use oce_store::Store;

use crate::engine::Engine;

use super::ParamEntry;

impl<S: Store> Engine<S> {
    /// Whether parameter `idx` changes the resolved block structure and therefore requires reload.
    pub(super) fn is_structural_param(&self, idx: usize) -> bool {
        let edited = &self.params.entries[idx];
        let Some(block) = self.model.blocks.get(edited.block.0 as usize) else {
            return false;
        };
        let Some(entry) = lookup(&block.class_iri) else {
            return false;
        };
        entry.param_rules().iter().any(|rule| {
            matches!(
                rule,
                ParamRule::Structural { name } if edited.name.as_ref() == *name
            ) || matches!(
                rule,
                ParamRule::StructuralArrayElements { base }
                    if flattened_array_element_name(edited.name.as_ref(), base)
            )
        })
    }

    pub(super) fn class_param_rules_reject(&self, idx: usize, value: &Value) -> bool {
        let edited = &self.params.entries[idx];
        let Some(block) = self.model.blocks.get(edited.block.0 as usize) else {
            return false;
        };
        let Some(entry) = lookup(&block.class_iri) else {
            return false;
        };
        for rule in entry.param_rules() {
            match *rule {
                ParamRule::Required { .. }
                | ParamRule::Structural { .. }
                | ParamRule::StructuralArrayElements { .. }
                | ParamRule::Boolean { .. }
                | ParamRule::RealEqualWarning { .. }
                | ParamRule::RealLessOrEqualWarning { .. }
                | ParamRule::RealGreaterOrEqualScaledWarning { .. } => {}
                ParamRule::Real { name }
                    if edited.name.as_ref() == name && real_param_value(value).is_none() =>
                {
                    return true;
                }
                ParamRule::Real { .. } => {}
                ParamRule::RealFinite { name } if edited.name.as_ref() == name => {
                    let Some(v) = real_param_value(value) else {
                        return true;
                    };
                    if !v.is_finite() {
                        return true;
                    }
                }
                ParamRule::RealFinite { .. } => {}
                ParamRule::RealGreaterThan { name, min } if edited.name.as_ref() == name => {
                    let Some(v) = real_param_value(value) else {
                        return true;
                    };
                    if !real_greater_than(v, min) {
                        return true;
                    }
                }
                ParamRule::RealGreaterThan { .. } => {}
                ParamRule::RealFiniteGreaterThan { name, min } if edited.name.as_ref() == name => {
                    let Some(v) = real_param_value(value) else {
                        return true;
                    };
                    if !v.is_finite() || !real_greater_than(v, min) {
                        return true;
                    }
                }
                ParamRule::RealFiniteGreaterThan { .. } => {}
                ParamRule::IntegerGreaterOrEqual { name, min } if edited.name.as_ref() == name => {
                    let Some(v) = int_param_value(value) else {
                        return true;
                    };
                    if v < min {
                        return true;
                    }
                }
                ParamRule::IntegerGreaterOrEqual { .. } => {}
                ParamRule::IntegerLessOrEqualConstant { name, max }
                    if edited.name.as_ref() == name =>
                {
                    let Some(v) = int_param_value(value) else {
                        return true;
                    };
                    if v > max {
                        return true;
                    }
                }
                ParamRule::IntegerLessOrEqualConstant { .. } => {}
                ParamRule::IntegerArrayElements { base, .. }
                    if flattened_array_element_name(edited.name.as_ref(), base)
                        && int_param_value(value).is_none() =>
                {
                    return true;
                }
                ParamRule::IntegerArrayElements { .. } => {}
                ParamRule::IntegerArrayElementsInRange {
                    base,
                    min,
                    max,
                    max_default,
                    ..
                } if flattened_array_element_name(edited.name.as_ref(), base) => {
                    let Some(v) = int_param_value(value) else {
                        return true;
                    };
                    let upper =
                        block_param_int_value(&self.params.entries, edited.block, max, idx, value)
                            .unwrap_or(max_default);
                    if v < min || v > upper {
                        return true;
                    }
                }
                ParamRule::IntegerArrayElementsInRange { .. } => {}
                ParamRule::RealArrayElements { base, .. }
                    if flattened_array_element_name(edited.name.as_ref(), base)
                        && real_param_value(value).is_none() =>
                {
                    return true;
                }
                ParamRule::RealArrayElements { .. } => {}
                ParamRule::RealMatrixElements { base, .. }
                    if flattened_array_element_name(edited.name.as_ref(), base)
                        && real_param_value(value).is_none() =>
                {
                    return true;
                }
                ParamRule::RealMatrixElements { .. } => {}
                ParamRule::TimeTableMatrix { base, values, .. }
                    if flattened_array_element_name(edited.name.as_ref(), base) =>
                {
                    let Some(v) = real_param_value(value) else {
                        return true;
                    };
                    if !time_table_value_allowed(v, values) {
                        return true;
                    }
                }
                ParamRule::TimeTableMatrix { .. } => {}
                ParamRule::TimeTableOffset { base, .. }
                    if flattened_array_element_name(edited.name.as_ref(), base)
                        && real_param_value(value).is_none() =>
                {
                    return true;
                }
                ParamRule::TimeTableOffset { .. } => {}
                ParamRule::BooleanArrayElements { base, .. }
                    if flattened_array_element_name(edited.name.as_ref(), base)
                        && !matches!(value, Value::Boolean(_)) =>
                {
                    return true;
                }
                ParamRule::BooleanArrayElements { .. } => {}
                ParamRule::BooleanArrayTrueCountEquals { .. } => {}
                ParamRule::EnumMembers { name, members }
                    if edited.name.as_ref() == name && !enum_param_value(value, members) =>
                {
                    return true;
                }
                ParamRule::EnumMembers { .. } => {}
                ParamRule::RealGreaterOrEqual { name, min } if edited.name.as_ref() == name => {
                    let Some(v) = real_param_value(value) else {
                        return true;
                    };
                    if !real_greater_or_equal(v, min) {
                        return true;
                    }
                }
                ParamRule::RealGreaterOrEqual { .. } => {}
                ParamRule::RealLessOrEqualConstant { name, max }
                    if edited.name.as_ref() == name =>
                {
                    let Some(v) = real_param_value(value) else {
                        return true;
                    };
                    if !real_less_or_equal(v, max) {
                        return true;
                    }
                }
                ParamRule::RealLessOrEqualConstant { .. } => {}
                ParamRule::RealTimesIntegerInclusiveRange {
                    real,
                    integer,
                    min,
                    max,
                } if edited.name.as_ref() == real || edited.name.as_ref() == integer => {
                    let real_value = block_param_real_value(
                        &self.params.entries,
                        edited.block,
                        real,
                        idx,
                        value,
                    );
                    let integer_value = block_param_int_value(
                        &self.params.entries,
                        edited.block,
                        integer,
                        idx,
                        value,
                    );
                    if let (Some(real_value), Some(integer_value)) = (real_value, integer_value) {
                        let scaled = real_value * integer_value as f64;
                        if !real_greater_or_equal(scaled, min) || !real_less_or_equal(scaled, max) {
                            return true;
                        }
                    }
                }
                ParamRule::RealTimesIntegerInclusiveRange { .. } => {}
                ParamRule::IntegerProductLessOrEqualConstant { left, right, max }
                    if edited.name.as_ref() == left || edited.name.as_ref() == right =>
                {
                    let left_value =
                        block_param_int_value(&self.params.entries, edited.block, left, idx, value);
                    let right_value = block_param_int_value(
                        &self.params.entries,
                        edited.block,
                        right,
                        idx,
                        value,
                    );
                    if let (Some(left_value), Some(right_value)) = (left_value, right_value) {
                        let Some(product) = left_value.checked_mul(right_value) else {
                            return true;
                        };
                        if product > max {
                            return true;
                        }
                    }
                }
                ParamRule::IntegerProductLessOrEqualConstant { .. } => {}
                ParamRule::RealLessOrEqual { lower, upper }
                    if edited.name.as_ref() == lower || edited.name.as_ref() == upper =>
                {
                    let lower_value = block_param_real_value(
                        &self.params.entries,
                        edited.block,
                        lower,
                        idx,
                        value,
                    );
                    let upper_value = block_param_real_value(
                        &self.params.entries,
                        edited.block,
                        upper,
                        idx,
                        value,
                    );
                    if let (Some(lower_value), Some(upper_value)) = (lower_value, upper_value)
                        && !real_less_or_equal(lower_value, upper_value)
                    {
                        return true;
                    }
                }
                ParamRule::RealLessOrEqual { .. } => {}
                _ => {}
            }
        }
        false
    }
}

fn flattened_array_element_name(name: &str, base: &str) -> bool {
    let Some(suffix) = name.strip_prefix(base).and_then(|s| s.strip_prefix('_')) else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .split('_')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn block_param_real_value(
    entries: &[ParamEntry],
    block: BlockId,
    name: &str,
    edited_idx: usize,
    edited_value: &Value,
) -> Option<f64> {
    entries
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.block == block && entry.name.as_ref() == name)
        .and_then(|(idx, entry)| {
            if idx == edited_idx {
                real_param_value(edited_value)
            } else {
                real_param_value(&entry.value)
            }
        })
}

fn block_param_int_value(
    entries: &[ParamEntry],
    block: BlockId,
    name: &str,
    edited_idx: usize,
    edited_value: &Value,
) -> Option<i64> {
    entries
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.block == block && entry.name.as_ref() == name)
        .and_then(|(idx, entry)| {
            if idx == edited_idx {
                int_param_value(edited_value)
            } else {
                int_param_value(&entry.value)
            }
        })
}

fn real_param_value(value: &Value) -> Option<f64> {
    match value {
        Value::Real(v) => Some(*v),
        Value::Integer(v) => Some(*v as f64),
        _ => None,
    }
}

fn time_table_value_allowed(value: f64, kind: TimeTableValues) -> bool {
    const SMALL: f64 = 1.0e-37;
    match kind {
        TimeTableValues::Real => true,
        TimeTableValues::Integer => value.is_finite() && (value - value.floor()).abs() < SMALL,
        TimeTableValues::Boolean => value.abs() < SMALL || (value - 1.0).abs() < SMALL,
    }
}

fn int_param_value(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(v) => Some(*v),
        _ => None,
    }
}

fn enum_param_value(value: &Value, members: &[&str]) -> bool {
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
