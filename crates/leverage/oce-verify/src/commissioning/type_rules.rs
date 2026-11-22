//! The discrete type rules of a point mapping: how one cell of a trend becomes a value of its
//! CDL point's type (commissioning note, "Point mapping", decision 19; issue #659).
//!
//! | Device type → CDL type | Rule | Here |
//! | --- | --- | --- |
//! | `float` → `float` | the number as read; its unit is converted after | [`CellRule::Real`] |
//! | `float` or `int` → `int` | integral and within `i32`, else refused | [`CellRule::Integer`], [`integer_from_number`] |
//! | `float`, `int` or `bool` → `bool` | a number exactly `0` is false and exactly `1` true; a text cell `true`/`false`, `active`/`inactive` (BACnet binary present value) or `0`/`1`; anything else refused | [`CellRule::Boolean`], [`boolean_from_number`], [`boolean_from_text`] |
//! | `bool` → `int` | the Boolean, then `false` is 0 and `true` 1 (Listing 12.2's `uOccSen`) | [`CellRule::BooleanAsInteger`], [`integer_from_boolean`] |
//! | `int` → `float` | a whole number within `i32`, widened exactly; else refused (owner ruling, 2026-11-21) | [`CellRule::IntegerAsReal`], [`real_from_whole_number`] |
//! | `bool` → `float` | not listed: refused | [`CellError::TypePairRefused`] |
//!
//! There is no threshold: a `0.5` in a Boolean column is refused, where the upstream model's
//! `GreaterThreshold(t = 0.5)` would read it as a state and a sensor fault with it (decision 19).
//! Text is compared byte for byte, with no trimming and no case folding. A text cell is read by
//! [`boolean_from_text`] only in a column whose device type is `bool`; a number in any column is
//! read by the number rules.
//!
//! Each function decides one cell and returns the typed cause of a refusal, [`CellError`]; the
//! trend source that reads the cell (#660) adds its file, row and column. Nothing here allocates
//! but a refusal, and nothing panics.

use std::fmt;

use super::choices::PointType;
use super::units::excerpt;

/// Every [`CellError`] code, in declaration order; a test pins the list.
pub(crate) const CELL_ERROR_CODES: &[&str] = &[
    "not-boolean",
    "not-boolean-text",
    "not-integral",
    "outside-int32",
    "type-pair-refused",
];

/// How the cells of one mapped point are read, decided once from its two types
/// ([`CellRule::for_types`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellRule {
    /// `float` → `float`: the number as read, NaN (a missing cell) and the infinities included;
    /// the unit table converts it after.
    Real,
    /// `float` or `int` → `int`: [`integer_from_number`].
    Integer,
    /// `float`, `int` or `bool` → `bool`: [`boolean_from_number`] for a number,
    /// [`boolean_from_text`] for a text cell of a `bool` device column.
    Boolean,
    /// `bool` → `int`: the cell read as for [`Self::Boolean`], then [`integer_from_boolean`].
    BooleanAsInteger,
    /// `int` → `float`: [`real_from_whole_number`]; the unit table converts it after.
    IntegerAsReal,
}

impl CellRule {
    /// The rule for a point whose device side has the type `device` and whose CDL side the type
    /// `cdl`, as the point mapping reads them.
    ///
    /// # Errors
    ///
    /// [`CellError::TypePairRefused`] for `bool` → `float`, the one pair the rules do not list.
    pub(crate) fn for_types(device: PointType, cdl: PointType) -> Result<Self, CellError> {
        use PointType::{Bool, Float, Int};
        match (device, cdl) {
            (Float, Float) => Ok(Self::Real),
            (Int, Float) => Ok(Self::IntegerAsReal),
            (Float | Int, Int) => Ok(Self::Integer),
            (Float | Int | Bool, Bool) => Ok(Self::Boolean),
            (Bool, Int) => Ok(Self::BooleanAsInteger),
            (Bool, Float) => Err(CellError::TypePairRefused { device, cdl }),
        }
    }
}

/// A number in a Boolean column: exactly `0` is false and exactly `1` is true. `-0.0` is zero,
/// so it is false.
///
/// # Errors
///
/// [`CellError::NotBoolean`] for every other value: a fraction (`0.5`), another integer (`2`,
/// `-1`), the neighbours of `1` (`1.0000000000000002`), a NaN or an infinity.
pub(crate) fn boolean_from_number(value: f64) -> Result<bool, CellError> {
    if value == 0.0 {
        Ok(false)
    } else if value == 1.0 {
        Ok(true)
    } else {
        Err(CellError::NotBoolean { value })
    }
}

/// A text cell in a Boolean column: `true`, `active` and `1` are true; `false`, `inactive` and
/// `0` are false. Byte for byte: `True`, `ACTIVE`, ` 1` and `1.0` are refused.
///
/// # Errors
///
/// [`CellError::NotBooleanText`] for every other text, the empty one included.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "replay's CsvSource hands a csv cell over typed, never as text (owner ruling, \
                  2026-11-21); the text rule waits for a replay reader that does"
    )
)]
pub(crate) fn boolean_from_text(text: &str) -> Result<bool, CellError> {
    match text {
        "true" | "active" | "1" => Ok(true),
        "false" | "inactive" | "0" => Ok(false),
        _ => Err(CellError::NotBooleanText {
            text: excerpt(text),
        }),
    }
}

/// A Boolean as a CDL Integer: `false` is 0 and `true` is 1.
pub(crate) fn integer_from_boolean(value: bool) -> i32 {
    i32::from(value)
}

/// A number in an Integer column: an integral value from `i32::MIN` to `i32::MAX`, both
/// included, as that integer. `-0.0` is 0.
///
/// # Errors
///
/// [`CellError::NotIntegral`] for a NaN, an infinity or a value with a fraction;
/// [`CellError::OutsideInt32`] for an integral value outside `i32`.
pub(crate) fn integer_from_number(value: f64) -> Result<i32, CellError> {
    if !value.is_finite() || value.trunc() != value {
        return Err(CellError::NotIntegral { value });
    }
    if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&value) {
        return Err(CellError::OutsideInt32 { value });
    }
    // Integral and inside `i32`, so the cast is exact.
    Ok(value as i32)
}

/// A number in an `int` device column read into a CDL Real: the whole number, checked as
/// [`integer_from_number`] checks it, then widened to `f64`, which holds every `i32` exactly.
/// `-0.0` reads as `0.0`, the integer zero.
///
/// # Errors
///
/// As [`integer_from_number`]: [`CellError::NotIntegral`] or [`CellError::OutsideInt32`].
pub(crate) fn real_from_whole_number(value: f64) -> Result<f64, CellError> {
    integer_from_number(value).map(f64::from)
}

/// Why one cell has no value of its CDL point's type. Each variant has one stable code
/// ([`CellError::code`]); the trend source that read the cell adds where it is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CellError {
    /// A number in a Boolean column that is not exactly 0 or 1.
    NotBoolean {
        /// The number.
        value: f64,
    },
    /// A text cell in a Boolean column that is none of the six spellings.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "replay's CsvSource hands a csv cell over typed, never as text (owner ruling, \
                      2026-11-21); the text rule waits for a replay reader that does"
        )
    )]
    NotBooleanText {
        /// The text, as an excerpt (printable ASCII, cut at 64 bytes).
        text: String,
    },
    /// A number in an Integer column that is a NaN, an infinity or not integral.
    NotIntegral {
        /// The number.
        value: f64,
    },
    /// An integral number in an Integer column outside `i32`.
    OutsideInt32 {
        /// The number.
        value: f64,
    },
    /// A device type and a CDL type no rule converts between: `bool` → `float`.
    TypePairRefused {
        /// The device point's type.
        device: PointType,
        /// The CDL point's type.
        cdl: PointType,
    },
}

impl CellError {
    /// The refusal's stable kebab-case code, one of [`CELL_ERROR_CODES`].
    pub(crate) const fn code(&self) -> &'static str {
        CELL_ERROR_CODES[self.declared_index()]
    }

    /// The position of the variant in the declaration. The match is exhaustive, so a new variant
    /// does not compile until it has a row here, and [`CELL_ERROR_CODES`] a code at that position.
    const fn declared_index(&self) -> usize {
        match self {
            Self::NotBoolean { .. } => 0,
            Self::NotBooleanText { .. } => 1,
            Self::NotIntegral { .. } => 2,
            Self::OutsideInt32 { .. } => 3,
            Self::TypePairRefused { .. } => 4,
        }
    }
}

impl fmt::Display for CellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotBoolean { value } => write!(
                f,
                "{value:?} is not a Boolean: only exactly 0 (false) and 1 (true) are"
            ),
            Self::NotBooleanText { text } => write!(
                f,
                "`{text}` is not a Boolean: only `true`, `false`, `active`, `inactive`, `1` and \
                 `0` are"
            ),
            Self::NotIntegral { value } => write!(f, "{value:?} is not an integral number"),
            Self::OutsideInt32 { value } => write!(
                f,
                "{value:?} is outside the CDL Integer range, {} to {}",
                i32::MIN,
                i32::MAX
            ),
            Self::TypePairRefused { device, cdl } => write!(
                f,
                "no rule reads a `{device}` device point as a `{cdl}` CDL point"
            ),
        }
    }
}

impl std::error::Error for CellError {}

#[cfg(test)]
#[path = "type_rules_tests.rs"]
mod tests;
