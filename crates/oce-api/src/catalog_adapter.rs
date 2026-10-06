//! Exhaustive registry projection into facade-owned catalog rules.

use crate::{CatalogPortKind, CatalogRule, CatalogValueKind};
use oce_blocks::{ParamRuleProjection, TimeTableValues};
use oce_model::ValueType;

pub(super) struct RuleAdapter;
impl ParamRuleProjection for RuleAdapter {
    type Output = CatalogRule;
    fn required(name: &'static str, kind: ValueType) -> CatalogRule {
        CatalogRule::Required {
            name,
            kind: value_kind(kind),
        }
    }
    fn structural(name: &'static str) -> CatalogRule {
        CatalogRule::Structural { name }
    }
    fn structural_array_elements(base: &'static str) -> CatalogRule {
        CatalogRule::StructuralArrayElements { base }
    }
    fn boolean(name: &'static str) -> CatalogRule {
        CatalogRule::Boolean { name }
    }
    fn real(name: &'static str) -> CatalogRule {
        CatalogRule::Real { name }
    }
    fn real_finite(name: &'static str) -> CatalogRule {
        CatalogRule::RealFinite { name }
    }
    fn real_greater_than(name: &'static str, min: f64) -> CatalogRule {
        CatalogRule::RealGreaterThan { name, min }
    }
    fn real_finite_greater_than(name: &'static str, min: f64) -> CatalogRule {
        CatalogRule::RealFiniteGreaterThan { name, min }
    }
    fn integer_greater_or_equal(name: &'static str, min: i64) -> CatalogRule {
        CatalogRule::IntegerGreaterOrEqual { name, min }
    }
    fn integer_less_or_equal_constant(name: &'static str, max: i64) -> CatalogRule {
        CatalogRule::IntegerLessOrEqualConstant { name, max }
    }
    fn integer_array_elements(base: &'static str, len: &'static str) -> CatalogRule {
        CatalogRule::IntegerArrayElements { base, len }
    }
    fn integer_array_elements_in_range(
        base: &'static str,
        len: &'static str,
        len_default: i64,
        min: i64,
        max: &'static str,
        max_default: i64,
        default_to_index: bool,
    ) -> CatalogRule {
        CatalogRule::IntegerArrayElementsInRange {
            base,
            len,
            len_default,
            min,
            max,
            max_default,
            default_to_index,
        }
    }
    fn real_array_elements(base: &'static str, len: &'static str) -> CatalogRule {
        CatalogRule::RealArrayElements { base, len }
    }
    fn real_matrix_elements(
        base: &'static str,
        rows: &'static str,
        default_rows: i64,
        cols: &'static str,
        default_cols: i64,
    ) -> CatalogRule {
        CatalogRule::RealMatrixElements {
            base,
            rows,
            default_rows,
            cols,
            default_cols,
        }
    }
    fn time_table_matrix(
        base: &'static str,
        values: TimeTableValues,
        time_scale: &'static str,
        period: Option<&'static str>,
        extrapolation: Option<&'static str>,
    ) -> CatalogRule {
        CatalogRule::TimeTableMatrix {
            base,
            values: table_kind(values),
            time_scale,
            period,
            extrapolation,
        }
    }
    fn time_table_offset(base: &'static str, table: &'static str) -> CatalogRule {
        CatalogRule::TimeTableOffset { base, table }
    }
    fn boolean_array_elements(base: &'static str, len: &'static str) -> CatalogRule {
        CatalogRule::BooleanArrayElements { base, len }
    }
    fn boolean_array_true_count_equals(
        base: &'static str,
        len: &'static str,
        count: &'static str,
        default: bool,
    ) -> CatalogRule {
        CatalogRule::BooleanArrayTrueCountEquals {
            base,
            len,
            count,
            default,
        }
    }
    fn enum_members(name: &'static str, members: &'static [&'static str]) -> CatalogRule {
        CatalogRule::EnumMembers { name, members }
    }
    fn real_greater_or_equal(name: &'static str, min: f64) -> CatalogRule {
        CatalogRule::RealGreaterOrEqual { name, min }
    }
    fn real_less_or_equal_constant(name: &'static str, max: f64) -> CatalogRule {
        CatalogRule::RealLessOrEqualConstant { name, max }
    }
    fn real_times_integer_inclusive_range(
        real: &'static str,
        integer: &'static str,
        min: f64,
        max: f64,
    ) -> CatalogRule {
        CatalogRule::RealTimesIntegerInclusiveRange {
            real,
            integer,
            min,
            max,
        }
    }
    fn integer_product_less_or_equal_constant(
        left: &'static str,
        right: &'static str,
        max: i64,
    ) -> CatalogRule {
        CatalogRule::IntegerProductLessOrEqualConstant { left, right, max }
    }
    fn real_less_or_equal(lower: &'static str, upper: &'static str) -> CatalogRule {
        CatalogRule::RealLessOrEqual { lower, upper }
    }
    fn real_less_or_equal_warning(lower: &'static str, upper: &'static str) -> CatalogRule {
        CatalogRule::RealLessOrEqualWarning { lower, upper }
    }
    fn real_greater_or_equal_scaled_warning(
        left: &'static str,
        right: &'static str,
        factor: f64,
    ) -> CatalogRule {
        CatalogRule::RealGreaterOrEqualScaledWarning {
            left,
            right,
            factor,
        }
    }
    fn real_equal_warning(left: &'static str, right: &'static str) -> CatalogRule {
        CatalogRule::RealEqualWarning { left, right }
    }
}

fn table_kind(kind: TimeTableValues) -> CatalogPortKind {
    match kind {
        TimeTableValues::Real => CatalogPortKind::Real,
        TimeTableValues::Integer => CatalogPortKind::Integer,
        TimeTableValues::Boolean => CatalogPortKind::Boolean,
    }
}
fn value_kind(kind: ValueType) -> CatalogValueKind {
    match kind {
        ValueType::Real => CatalogValueKind::Real,
        ValueType::Integer => CatalogValueKind::Integer,
        ValueType::Boolean => CatalogValueKind::Boolean,
        ValueType::String => CatalogValueKind::String,
        ValueType::Enum(id) => {
            let descriptor = oce_model::enum_descriptor(id)
                .expect("registered parameter enum has a canonical descriptor");
            CatalogValueKind::Enum {
                class_path: descriptor.class_path,
                members: descriptor.members,
            }
        }
    }
}
