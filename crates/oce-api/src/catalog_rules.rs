//! Facade-owned complete parameter-rule vocabulary.

use crate::{CatalogPortKind, CatalogValueKind};

/// One class-level parameter rule, in source declaration order.
///
/// These are metadata, not an instance validator. Real bounds retain binary64 bits; compare
/// serialized catalog bytes when distinguishing signed zero or NaN payloads. New schema rules
/// require an explicit contract revision and a total projection in the registry owner.
#[derive(Clone, Debug, PartialEq)]
pub enum CatalogRule {
    /// The named parameter must appear in the resolved parameter table with the declared semantic
    /// kind. `Real` accepts both `Value::Real` and `Value::Integer` because CDL widens Integer
    /// literals to Real parameters. Enumeration parameters retain their resolver-supported enum,
    /// integer-ordinal, and qualified-string representations.
    Required {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Semantic kind consumed by the block constructor.
        kind: CatalogValueKind,
    },
    /// The named parameter changes the block's resolved structure and cannot be edited at rest.
    ///
    /// Examples include vector widths such as `nin`, where changing the value changes the flattened
    /// port arity and therefore requires rebuilding the model rather than resuming an existing
    /// schedule.
    Structural {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// Flattened members of the named array parameter change the resolved block structure.
    ///
    /// Examples include selector arrays such as `extract[nout]` and masks such as `msk[nin]`,
    /// where editing one element changes the source-to-output feedthrough map. Flattened CXF names
    /// use `base_1`, `base_2`, ... and require a fresh model load when changed.
    StructuralArrayElements {
        /// Flattened array base name, for example `"extract"` matching `extract_1`, ...
        base: &'static str,
    },
    /// The named parameter, when present, must be a Boolean value.
    Boolean {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// The named parameter, when present, must be numeric and usable as a `Real`.
    Real {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// The named parameter, when present, must be numeric and finite as a `Real`.
    RealFinite {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// The named `Real` parameter must be strictly greater than `min`.
    RealGreaterThan {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Exclusive lower bound.
        min: f64,
    },
    /// The named `Real` parameter must be finite and strictly greater than `min`.
    RealFiniteGreaterThan {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Exclusive lower bound.
        min: f64,
    },
    /// The named `Integer` parameter must be greater than or equal to `min`.
    IntegerGreaterOrEqual {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive lower bound.
        min: i64,
    },
    /// The named `Integer` parameter must be less than or equal to `max`.
    IntegerLessOrEqualConstant {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive upper bound.
        max: i64,
    },
    /// Present members of a flattened integer array parameter must be integer values.
    ///
    /// The array may be sparse because some CDL array parameters, such as `k[nin]=fill(1,nin)`, have
    /// source defaults for every omitted element. When a member is supplied, this rule rejects a
    /// non-integer value instead of letting the constructor silently fall back to that default.
    IntegerArrayElements {
        /// Flattened array base name, for example `"k"` matching `k_1`, `k_2`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
    },
    /// Present members of a flattened integer array must be in an inclusive parameterized range.
    ///
    /// Sparse members may be allowed when a CDL source default supplies omitted values. When
    /// `default_to_index` is true, omitted member `base_i` validates as value `i`; this models
    /// defaults such as `extract[nout]=1:nout` and catches invalid defaulted selectors.
    IntegerArrayElementsInRange {
        /// Flattened array base name, for example `"extract"` matching `extract_1`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
        /// Default array length when `len` is omitted by source defaulting.
        len_default: i64,
        /// Inclusive lower bound.
        min: i64,
        /// Integer parameter carrying the inclusive upper bound.
        max: &'static str,
        /// Default upper bound when `max` is omitted by source defaulting.
        max_default: i64,
        /// Whether omitted `base_i` defaults to integer value `i`.
        default_to_index: bool,
    },
    /// Present members of a flattened real array parameter must be numeric values usable as `Real`.
    ///
    /// Like [`CatalogRule::IntegerArrayElements`], sparse arrays are allowed when the source default
    /// supplies omitted elements. Supplied members accept CDL integer-to-real promotion.
    RealArrayElements {
        /// Flattened array base name, for example `"k"` matching `k_1`, `k_2`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
    },
    /// Present members of a flattened two-dimensional Real matrix parameter must be numeric values.
    ///
    /// Flattened CXF names use one-based row-major keys such as `K_1_1`, `K_1_2`, `K_2_1`, ...
    /// Sparse matrices are allowed only where the source block defines defaults for omitted
    /// members. Supplied members accept CDL integer-to-real promotion.
    RealMatrixElements {
        /// Flattened matrix base name, for example `"K"` matching `K_1_1`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved row count.
        rows: &'static str,
        /// Source default row count when `rows` is omitted.
        default_rows: i64,
        /// Integer parameter carrying the resolved column count.
        cols: &'static str,
        /// Source default column count when `cols` is omitted.
        default_cols: i64,
    },
    /// Flattened source `TimeTable` matrix elements must form a complete rectangular table.
    ///
    /// Flattened names use one-based row-major keys such as `table_1_1`, `table_1_2`, ...
    /// The first column is model time; remaining columns carry typed outputs. `period` is present
    /// only for periodic Integer/Logical sources; `extrapolation` is present only for the Real
    /// source so validation can reject a degenerate periodic range.
    TimeTableMatrix {
        /// Flattened matrix base name, normally `"table"`.
        base: &'static str,
        /// Required value kind for output columns.
        values: CatalogPortKind,
        /// Time-scale parameter name.
        time_scale: &'static str,
        /// Optional period parameter name for periodic step tables.
        period: Option<&'static str>,
        /// Optional extrapolation parameter name for Real tables.
        extrapolation: Option<&'static str>,
    },
    /// Present members of a flattened TimeTable offset vector must be numeric and in shape.
    ///
    /// The valid vector length is inferred from the resolved table matrix column count as
    /// `size(table, 2)-1`. Missing members use the CDL source default `0`.
    TimeTableOffset {
        /// Flattened offset vector base name, normally `"offset"`.
        base: &'static str,
        /// Flattened table matrix base name that defines the output count.
        table: &'static str,
    },
    /// Present members of a flattened Boolean array parameter must be Boolean values.
    BooleanArrayElements {
        /// Flattened array base name, for example `"msk"` matching `msk_1`, `msk_2`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
    },
    /// The true count of a flattened Boolean array must equal another integer parameter.
    ///
    /// Missing members use the source default supplied in `default`, so sparse masks validate the
    /// same way source-expanded arrays do.
    BooleanArrayTrueCountEquals {
        /// Flattened array base name, for example `"msk"` matching `msk_1`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
        /// Integer parameter carrying the expected true count.
        count: &'static str,
        /// Source default for omitted Boolean array members.
        default: bool,
    },
    /// The named enum parameter must be one of the source-verified members.
    EnumMembers {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Allowed member names in source ordinal order.
        members: &'static [&'static str],
    },
    /// The named `Real` parameter must be greater than or equal to `min`.
    RealGreaterOrEqual {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive lower bound.
        min: f64,
    },
    /// The named `Real` parameter must be less than or equal to `max`.
    RealLessOrEqualConstant {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive upper bound.
        max: f64,
    },
    /// The named `Real` parameter, multiplied by an `Integer` parameter, must be in range.
    RealTimesIntegerInclusiveRange {
        /// Real-valued parameter name.
        real: &'static str,
        /// Integer-valued scale parameter name.
        integer: &'static str,
        /// Inclusive lower bound on `real * integer`.
        min: f64,
        /// Inclusive upper bound on `real * integer`.
        max: f64,
    },
    /// The product of two integer parameters must not exceed a constant upper bound.
    IntegerProductLessOrEqualConstant {
        /// Left-hand integer parameter name.
        left: &'static str,
        /// Right-hand integer parameter name.
        right: &'static str,
        /// Inclusive upper bound on `left * right`.
        max: i64,
    },
    /// The two named `Real` parameters must satisfy `lower <= upper`.
    RealLessOrEqual {
        /// Lower-bound parameter name.
        lower: &'static str,
        /// Upper-bound parameter name.
        upper: &'static str,
    },
    /// The two named `Real` parameters should satisfy `lower <= upper`; violations warn only.
    RealLessOrEqualWarning {
        /// Lower-bound parameter name.
        lower: &'static str,
        /// Upper-bound parameter name.
        upper: &'static str,
    },
    /// The left `Real` parameter should be at least `factor * right`; violations warn only.
    RealGreaterOrEqualScaledWarning {
        /// Left-hand parameter name.
        left: &'static str,
        /// Right-hand parameter name multiplied by `factor`.
        right: &'static str,
        /// Positive scale factor applied to `right`.
        factor: f64,
    },
    /// Equal `Real` parameter values are permitted but should produce a warning.
    RealEqualWarning {
        /// Left-hand parameter name.
        left: &'static str,
        /// Right-hand parameter name.
        right: &'static str,
    },
}
