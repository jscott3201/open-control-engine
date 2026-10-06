//! Exhaustive owner-side projection of parameter rules for independent metadata contracts.

use crate::{ParamRule, TimeTableValues};
use oce_model::ValueType;

/// A total projection of every parameter rule and all of its payload fields.
///
/// Implementors supply every method. Adding a rule requires updating both this owner-side
/// exhaustive dispatch and each adapter; there is no wildcard or silent omission path.
/// Methods receive immutable static metadata; they do not execute validation.
#[allow(clippy::too_many_arguments)]
pub trait ParamRuleProjection {
    /// The independent representation produced for one rule.
    type Output;
    /// Project [`ParamRule::Required`], preserving every declared field.
    fn required(name: &'static str, kind: ValueType) -> Self::Output;
    /// Project [`ParamRule::Structural`], preserving every declared field.
    fn structural(name: &'static str) -> Self::Output;
    /// Project [`ParamRule::StructuralArrayElements`], preserving every declared field.
    fn structural_array_elements(base: &'static str) -> Self::Output;
    /// Project [`ParamRule::Boolean`], preserving every declared field.
    fn boolean(name: &'static str) -> Self::Output;
    /// Project [`ParamRule::Real`], preserving every declared field.
    fn real(name: &'static str) -> Self::Output;
    /// Project [`ParamRule::RealFinite`], preserving every declared field.
    fn real_finite(name: &'static str) -> Self::Output;
    /// Project [`ParamRule::RealGreaterThan`], preserving every declared field.
    fn real_greater_than(name: &'static str, min: f64) -> Self::Output;
    /// Project [`ParamRule::RealFiniteGreaterThan`], preserving every declared field.
    fn real_finite_greater_than(name: &'static str, min: f64) -> Self::Output;
    /// Project [`ParamRule::IntegerGreaterOrEqual`], preserving every declared field.
    fn integer_greater_or_equal(name: &'static str, min: i64) -> Self::Output;
    /// Project [`ParamRule::IntegerLessOrEqualConstant`], preserving every declared field.
    fn integer_less_or_equal_constant(name: &'static str, max: i64) -> Self::Output;
    /// Project [`ParamRule::IntegerArrayElements`], preserving every declared field.
    fn integer_array_elements(base: &'static str, len: &'static str) -> Self::Output;
    /// Project [`ParamRule::IntegerArrayElementsInRange`], preserving every declared field.
    fn integer_array_elements_in_range(
        base: &'static str,
        len: &'static str,
        len_default: i64,
        min: i64,
        max: &'static str,
        max_default: i64,
        default_to_index: bool,
    ) -> Self::Output;
    /// Project [`ParamRule::RealArrayElements`], preserving every declared field.
    fn real_array_elements(base: &'static str, len: &'static str) -> Self::Output;
    /// Project [`ParamRule::RealMatrixElements`], preserving every declared field.
    fn real_matrix_elements(
        base: &'static str,
        rows: &'static str,
        default_rows: i64,
        cols: &'static str,
        default_cols: i64,
    ) -> Self::Output;
    /// Project [`ParamRule::TimeTableMatrix`], preserving every declared field.
    fn time_table_matrix(
        base: &'static str,
        values: TimeTableValues,
        time_scale: &'static str,
        period: Option<&'static str>,
        extrapolation: Option<&'static str>,
    ) -> Self::Output;
    /// Project [`ParamRule::TimeTableOffset`], preserving every declared field.
    fn time_table_offset(base: &'static str, table: &'static str) -> Self::Output;
    /// Project [`ParamRule::BooleanArrayElements`], preserving every declared field.
    fn boolean_array_elements(base: &'static str, len: &'static str) -> Self::Output;
    /// Project [`ParamRule::BooleanArrayTrueCountEquals`], preserving every declared field.
    fn boolean_array_true_count_equals(
        base: &'static str,
        len: &'static str,
        count: &'static str,
        default: bool,
    ) -> Self::Output;
    /// Project [`ParamRule::EnumMembers`], preserving every declared field.
    fn enum_members(name: &'static str, members: &'static [&'static str]) -> Self::Output;
    /// Project [`ParamRule::RealGreaterOrEqual`], preserving every declared field.
    fn real_greater_or_equal(name: &'static str, min: f64) -> Self::Output;
    /// Project [`ParamRule::RealLessOrEqualConstant`], preserving every declared field.
    fn real_less_or_equal_constant(name: &'static str, max: f64) -> Self::Output;
    /// Project [`ParamRule::RealTimesIntegerInclusiveRange`], preserving every declared field.
    fn real_times_integer_inclusive_range(
        real: &'static str,
        integer: &'static str,
        min: f64,
        max: f64,
    ) -> Self::Output;
    /// Project [`ParamRule::IntegerProductLessOrEqualConstant`], preserving every declared field.
    fn integer_product_less_or_equal_constant(
        left: &'static str,
        right: &'static str,
        max: i64,
    ) -> Self::Output;
    /// Project [`ParamRule::RealLessOrEqual`], preserving every declared field.
    fn real_less_or_equal(lower: &'static str, upper: &'static str) -> Self::Output;
    /// Project [`ParamRule::RealLessOrEqualWarning`], preserving every declared field.
    fn real_less_or_equal_warning(lower: &'static str, upper: &'static str) -> Self::Output;
    /// Project [`ParamRule::RealGreaterOrEqualScaledWarning`], preserving every declared field.
    fn real_greater_or_equal_scaled_warning(
        left: &'static str,
        right: &'static str,
        factor: f64,
    ) -> Self::Output;
    /// Project [`ParamRule::RealEqualWarning`], preserving every declared field.
    fn real_equal_warning(left: &'static str, right: &'static str) -> Self::Output;
}

impl ParamRule {
    /// Project this rule through a total owner-side adapter without exposing it in the result.
    ///
    /// Calls exactly one required adapter method with every field. No allocation or panic is
    /// introduced here; the adapter owns its result and behavior.
    pub fn project<P: ParamRuleProjection>(&self) -> P::Output {
        match *self {
            Self::Required { name, kind } => P::required(name, kind),
            Self::Structural { name } => P::structural(name),
            Self::StructuralArrayElements { base } => P::structural_array_elements(base),
            Self::Boolean { name } => P::boolean(name),
            Self::Real { name } => P::real(name),
            Self::RealFinite { name } => P::real_finite(name),
            Self::RealGreaterThan { name, min } => P::real_greater_than(name, min),
            Self::RealFiniteGreaterThan { name, min } => P::real_finite_greater_than(name, min),
            Self::IntegerGreaterOrEqual { name, min } => P::integer_greater_or_equal(name, min),
            Self::IntegerLessOrEqualConstant { name, max } => {
                P::integer_less_or_equal_constant(name, max)
            }
            Self::IntegerArrayElements { base, len } => P::integer_array_elements(base, len),
            Self::IntegerArrayElementsInRange {
                base,
                len,
                len_default,
                min,
                max,
                max_default,
                default_to_index,
            } => P::integer_array_elements_in_range(
                base,
                len,
                len_default,
                min,
                max,
                max_default,
                default_to_index,
            ),
            Self::RealArrayElements { base, len } => P::real_array_elements(base, len),
            Self::RealMatrixElements {
                base,
                rows,
                default_rows,
                cols,
                default_cols,
            } => P::real_matrix_elements(base, rows, default_rows, cols, default_cols),
            Self::TimeTableMatrix {
                base,
                values,
                time_scale,
                period,
                extrapolation,
            } => P::time_table_matrix(base, values, time_scale, period, extrapolation),
            Self::TimeTableOffset { base, table } => P::time_table_offset(base, table),
            Self::BooleanArrayElements { base, len } => P::boolean_array_elements(base, len),
            Self::BooleanArrayTrueCountEquals {
                base,
                len,
                count,
                default,
            } => P::boolean_array_true_count_equals(base, len, count, default),
            Self::EnumMembers { name, members } => P::enum_members(name, members),
            Self::RealGreaterOrEqual { name, min } => P::real_greater_or_equal(name, min),
            Self::RealLessOrEqualConstant { name, max } => {
                P::real_less_or_equal_constant(name, max)
            }
            Self::RealTimesIntegerInclusiveRange {
                real,
                integer,
                min,
                max,
            } => P::real_times_integer_inclusive_range(real, integer, min, max),
            Self::IntegerProductLessOrEqualConstant { left, right, max } => {
                P::integer_product_less_or_equal_constant(left, right, max)
            }
            Self::RealLessOrEqual { lower, upper } => P::real_less_or_equal(lower, upper),
            Self::RealLessOrEqualWarning { lower, upper } => {
                P::real_less_or_equal_warning(lower, upper)
            }
            Self::RealGreaterOrEqualScaledWarning {
                left,
                right,
                factor,
            } => P::real_greater_or_equal_scaled_warning(left, right, factor),
            Self::RealEqualWarning { left, right } => P::real_equal_warning(left, right),
        }
    }
}
