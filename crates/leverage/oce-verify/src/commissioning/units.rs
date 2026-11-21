//! The closed unit table `units/v1`: how a device's reading is converted to the unit its CDL point
//! declares (commissioning note, "Point mapping", decisions 5, 19 and 24; issue #659).
//!
//! OBC §12.7 puts unit conversion in "the tool that reads the CSV files", never in the engine, so
//! a trend's cells are converted here, device unit to CDL unit, before any comparison: every
//! tolerance is then a quantity in the CDL unit. A parameter is never converted (decision 24).
//!
//! The table is **closed**. An unlisted pair is [`UnitError::UnknownPair`], never a pass-through;
//! a unit on one side only is [`UnitError::OneSided`]; no unit on either side means no conversion;
//! two identical spellings mean the identity. Spellings are compared byte for byte: no trimming,
//! no case folding, no alias beyond `percent` and `%`. Each row runs one way, device to CDL, so
//! the reverse of a listed pair (`K` to `degF`) is unknown unless it is a row of its own: `degC`
//! to `K` and `K` to `degC` are the one such couple.
//!
//! | Device → CDL | Operation, in this order, in `f64` | Constant's source |
//! | --- | --- | --- |
//! | `degF` → `K` | `(x - 32.0) * (5.0 / 9.0) + 273.15` | NIST SP 811 B.9; NIST SP 330 §2.3.1 |
//! | `degC` → `K` | `x + 273.15` | NIST SP 330 §2.3.1 (`T0` = 273.15 K exactly) |
//! | `degF` → `degC` | `(x - 32.0) * (5.0 / 9.0)` | NIST SP 811 B.9 |
//! | `K` → `degC` | `x - 273.15` | NIST SP 330 §2.3.1 |
//! | `kPa` → `Pa` | `x * 1000.0` | NIST SP 330, table 7 (kilo = 10³) |
//! | `inH2O` → `Pa` | `x * 249.08891` | NIST SP 811 B.8, conventional inch of water: 0.0254 m × 1000 kg/m³ × 9.80665 m/s², exactly |
//! | `inH2O_60F` → `Pa` | `x * 248.84` | NIST SP 811 B.8, inch of water at 60 °F, tabulated to five digits |
//! | `cfm` → `m3/s` | `x * 0.0004719474432` | NIST SP 811 B.8: 0.3048³ m³ / 60 s, exactly |
//! | `L/s` → `m3/s` | `x * 0.001` | NIST SP 811 B.8 (litre = 10⁻³ m³ exactly) |
//! | `percent` or `%` → `1` | `x / 100.0` | NIST SP 811 §7.10.2 (% = 0.01) |
//! | identical spellings | the identity, bits unchanged | |
//!
//! The two inches of water are two units, as decision 19 rules: the conventional one, which the
//! engine's unit reading also means by `inH2O` (its QUDT table drops modelica-json's
//! `IN_H2O_60DEG_F`, which names the 60 °F one), and the inch at 60 °F, spelled `inH2O_60F`. They
//! differ by about 0.1 %.
//!
//! The spellings on the CDL side (`K`, `degC`, `Pa`, `m3/s`, `1`) and `degF` are the engine's own,
//! the strings its unit reading gives the same units (`oce_api::QUDT_UNITS`): one unit, one
//! spelling across the project.
//!
//! **Determinism.** Each conversion is the operations above and nothing else: no library call, no
//! fused multiply-add (Rust never contracts `a * b + c` on its own), each constant a literal that
//! rounds once. The same input gives the same bits on every target and in every build. A NaN or an
//! infinity is returned with its bits unchanged, so replay still reads a missing Real cell (NaN)
//! as missing and the platform's NaN propagation never shows; a finite reading whose converted
//! value is not finite is refused ([`UnitError::Overflow`]).
//!
//! **Temperature differences.** Every pair converts a reading, a point on its scale, so the
//! affine pairs add their offset. The note gives tolerances in CDL units ("tolerances are
//! CDL-unit quantities"), so no difference, deadband or tolerance in a device unit ever passes
//! through this table, and it has no difference form.

use std::fmt::{self, Write as _};

/// The name and version of the unit table, as a commissioning receipt's `units.table` carries it.
pub const UNITS_TABLE: &str = "units/v1";

/// Every [`UnitError`] code, in declaration order; a test pins the list.
pub const UNIT_ERROR_CODES: &[&str] = &["unknown-pair", "one-sided", "empty-unit", "overflow"];

/// The most bytes of one unit spelling a [`UnitError`] quotes, its escapes and the `...` that
/// marks a cut included.
pub(crate) const MAX_UNIT_EXCERPT_BYTES: usize = 64;

/// Degrees Fahrenheit at 0 °C (NIST SP 811 B.9): `t/°C = (t/°F - 32) / 1.8`.
const FAHRENHEIT_AT_ZERO_CELSIUS: f64 = 32.0;
/// Kelvin, or degrees Celsius, in one degree Fahrenheit: 1 / 1.8, rounded once (NIST SP 811 B.9).
const KELVIN_PER_FAHRENHEIT_DEGREE: f64 = 5.0 / 9.0;
/// Kelvin at 0 °C, exactly by definition (NIST SP 330 §2.3.1), as the nearest `f64`.
const KELVIN_AT_ZERO_CELSIUS: f64 = 273.15;
/// Pascals in one kilopascal (NIST SP 330, table 7).
const PASCALS_PER_KILOPASCAL: f64 = 1000.0;
/// Pascals in one conventional inch of water: 0.0254 m × 1000 kg/m³ × 9.80665 m/s², an exact
/// decimal (NIST SP 811 B.8, which prints it as 2.490 889 E+02), as the nearest `f64`.
const PASCALS_PER_INCH_OF_WATER: f64 = 249.08891;
/// Pascals in one inch of water at 60 °F: NIST SP 811 B.8's tabulated 2.4884 E+02, a measured
/// property of water to five significant digits, not a definition.
const PASCALS_PER_INCH_OF_WATER_60F: f64 = 248.84;
/// Cubic metres a second in one cubic foot a minute: 0.3048³ / 60, the exact decimal
/// 0.000 471 947 443 2 (NIST SP 811 B.8 prints 4.719 474 E-04), as the nearest `f64`.
const CUBIC_METRES_PER_SECOND_PER_CFM: f64 = 0.000_471_947_443_2;
/// Cubic metres in one litre, exactly (NIST SP 811 B.8).
const CUBIC_METRES_PER_LITRE: f64 = 0.001;
/// The number a percentage is divided by (NIST SP 811 §7.10.2: % is 0.01).
const PERCENT_PER_ONE: f64 = 100.0;

/// The arithmetic of one listed pair.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Operation {
    /// `(x - 32.0) * (5.0 / 9.0) + 273.15`.
    FahrenheitToKelvin,
    /// `x + 273.15`.
    CelsiusToKelvin,
    /// `(x - 32.0) * (5.0 / 9.0)`.
    FahrenheitToCelsius,
    /// `x - 273.15`.
    KelvinToCelsius,
    /// `x * factor`.
    Multiply(f64),
    /// `x / divisor`.
    Divide(f64),
}

impl Operation {
    /// The operation on `x`, in the table's order, one rounding per operation.
    fn evaluate(self, x: f64) -> f64 {
        match self {
            Self::FahrenheitToKelvin => {
                (x - FAHRENHEIT_AT_ZERO_CELSIUS) * KELVIN_PER_FAHRENHEIT_DEGREE
                    + KELVIN_AT_ZERO_CELSIUS
            }
            Self::CelsiusToKelvin => x + KELVIN_AT_ZERO_CELSIUS,
            Self::FahrenheitToCelsius => {
                (x - FAHRENHEIT_AT_ZERO_CELSIUS) * KELVIN_PER_FAHRENHEIT_DEGREE
            }
            Self::KelvinToCelsius => x - KELVIN_AT_ZERO_CELSIUS,
            Self::Multiply(factor) => x * factor,
            Self::Divide(divisor) => x / divisor,
        }
    }
}

/// One listed pair: the device spelling, the CDL spelling and the arithmetic between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TableEntry {
    /// The device's unit, as a mapping's `device.unit` spells it.
    pub(crate) from: &'static str,
    /// The CDL point's unit, as a mapping's `cdl.unit` spells it.
    pub(crate) to: &'static str,
    /// The arithmetic.
    operation: Operation,
}

/// `units/v1`, every listed pair, in the design note's order. Closed: nothing is looked up
/// anywhere else.
pub(crate) const UNITS_V1: [TableEntry; 11] = [
    entry("degF", "K", Operation::FahrenheitToKelvin),
    entry("degC", "K", Operation::CelsiusToKelvin),
    entry("degF", "degC", Operation::FahrenheitToCelsius),
    entry("K", "degC", Operation::KelvinToCelsius),
    entry("kPa", "Pa", Operation::Multiply(PASCALS_PER_KILOPASCAL)),
    entry(
        "inH2O",
        "Pa",
        Operation::Multiply(PASCALS_PER_INCH_OF_WATER),
    ),
    entry(
        "inH2O_60F",
        "Pa",
        Operation::Multiply(PASCALS_PER_INCH_OF_WATER_60F),
    ),
    entry(
        "cfm",
        "m3/s",
        Operation::Multiply(CUBIC_METRES_PER_SECOND_PER_CFM),
    ),
    entry("L/s", "m3/s", Operation::Multiply(CUBIC_METRES_PER_LITRE)),
    entry("percent", "1", Operation::Divide(PERCENT_PER_ONE)),
    entry("%", "1", Operation::Divide(PERCENT_PER_ONE)),
];

const fn entry(from: &'static str, to: &'static str, operation: Operation) -> TableEntry {
    TableEntry {
        from,
        to,
        operation,
    }
}

/// Which side of a mapped point a unit belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitSide {
    /// The controller's point: the unit the trend's values are in.
    Device,
    /// The CDL sequence's point: the unit the engine's values are in.
    Cdl,
}

impl UnitSide {
    /// `device` or `cdl`, the key Listing 12.2 writes the side under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Device => "device",
            Self::Cdl => "cdl",
        }
    }
}

/// Why a unit pair, or one reading, has no conversion. Each variant has one stable code
/// ([`UnitError::code`]); `#[non_exhaustive]`, so consumers branch on the code.
///
/// A unit spelling in a refusal is an excerpt: printable ASCII, every other character (a space,
/// a backtick and a backslash included) written as `\u{…}` with its hexadecimal code point, and
/// cut with `...` at 64 bytes, so a refusal never carries a megabyte-long or invisible spelling.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum UnitError {
    /// Both sides have a unit, the two spellings differ and `units/v1` does not list the pair
    /// from the device's to the CDL's.
    UnknownPair {
        /// The device's unit, as an excerpt.
        from: String,
        /// The CDL point's unit, as an excerpt.
        to: String,
    },
    /// One side has a unit and the other none. A missing unit is never assumed to be the other
    /// side's.
    OneSided {
        /// The side that has the unit.
        side: UnitSide,
        /// Its unit, as an excerpt.
        unit: String,
    },
    /// A unit is the empty string: written, but naming no unit. The device side is reported when
    /// both are empty.
    EmptyUnit {
        /// The side whose unit is empty.
        side: UnitSide,
    },
    /// A finite reading whose converted value is not finite: the product overflows `f64`. Only
    /// a pair whose factor is above one can, and only beyond about 1e305.
    Overflow {
        /// The device's unit, as the table spells it.
        from: &'static str,
        /// The CDL point's unit, as the table spells it.
        to: &'static str,
        /// The reading, in the device's unit.
        value: f64,
    },
}

impl UnitError {
    /// The refusal's stable kebab-case code, one of [`UNIT_ERROR_CODES`].
    #[must_use]
    pub const fn code(&self) -> &'static str {
        UNIT_ERROR_CODES[self.declared_index()]
    }

    /// The position of the variant in the declaration. The match is exhaustive, so a new variant
    /// does not compile until it has a row here, and [`UNIT_ERROR_CODES`] a code at that position
    /// (a test holds the two the same length).
    const fn declared_index(&self) -> usize {
        match self {
            Self::UnknownPair { .. } => 0,
            Self::OneSided { .. } => 1,
            Self::EmptyUnit { .. } => 2,
            Self::Overflow { .. } => 3,
        }
    }
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPair { from, to } => {
                write!(f, "{UNITS_TABLE} has no conversion from `{from}` to `{to}`")
            }
            Self::OneSided { side, unit } => {
                let other = match side {
                    UnitSide::Device => UnitSide::Cdl,
                    UnitSide::Cdl => UnitSide::Device,
                };
                write!(
                    f,
                    "the {} side has the unit `{unit}` and the {} side has none",
                    side.as_str(),
                    other.as_str()
                )
            }
            Self::EmptyUnit { side } => write!(f, "the {} unit is empty", side.as_str()),
            Self::Overflow { from, to, value } => write!(
                f,
                "the reading {value:?} in `{from}` is beyond the largest `f64` in `{to}`"
            ),
        }
    }
}

impl std::error::Error for UnitError {}

/// What a point's two units call for, decided once per point and applied to each of its cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum UnitConversion {
    /// Neither side has a unit: "some points may not have a unit, but only have a type" (OBC
    /// §12.7). The reading is used as it is.
    Unitless,
    /// Both sides spell the same unit. The reading is used as it is.
    Identity,
    /// A listed pair.
    Listed(&'static TableEntry),
}

impl UnitConversion {
    /// The conversion for a point whose device side has the unit `device` and whose CDL side has
    /// the unit `cdl`, `None` where the mapping writes none.
    ///
    /// Decided in this order: an empty unit, the device's before the CDL's
    /// ([`UnitError::EmptyUnit`]); a unit on one side only ([`UnitError::OneSided`]); none on
    /// either side ([`Self::Unitless`]); the same spelling on both ([`Self::Identity`]); a listed
    /// pair ([`Self::Listed`]); anything else, [`UnitError::UnknownPair`]. Work is one comparison
    /// of the two spellings and at most eleven pairs of comparisons against the table; nothing is
    /// copied but the excerpts of a refusal.
    pub(crate) fn resolve(device: Option<&str>, cdl: Option<&str>) -> Result<Self, UnitError> {
        for (unit, side) in [(device, UnitSide::Device), (cdl, UnitSide::Cdl)] {
            if unit == Some("") {
                return Err(UnitError::EmptyUnit { side });
            }
        }
        match (device, cdl) {
            (None, None) => Ok(Self::Unitless),
            (Some(unit), None) => Err(UnitError::OneSided {
                side: UnitSide::Device,
                unit: excerpt(unit),
            }),
            (None, Some(unit)) => Err(UnitError::OneSided {
                side: UnitSide::Cdl,
                unit: excerpt(unit),
            }),
            (Some(from), Some(to)) if from == to => Ok(Self::Identity),
            (Some(from), Some(to)) => UNITS_V1
                .iter()
                .find(|listed| listed.from == from && listed.to == to)
                .map(Self::Listed)
                .ok_or_else(|| UnitError::UnknownPair {
                    from: excerpt(from),
                    to: excerpt(to),
                }),
        }
    }

    /// `x`, a reading in the device's unit, in the CDL point's unit.
    ///
    /// A NaN or an infinity is returned with its bits unchanged, as [`Self::Unitless`] and
    /// [`Self::Identity`] return every value. A listed pair applies its operation to a finite `x`,
    /// one rounding per operation in the table's order; `-0.0` keeps its sign where the operation
    /// keeps it (a factor or a divisor) and an offset makes it the offset.
    ///
    /// # Errors
    ///
    /// [`UnitError::Overflow`] when `x` is finite and its conversion is not.
    pub(crate) fn apply(self, x: f64) -> Result<f64, UnitError> {
        let Self::Listed(listed) = self else {
            return Ok(x);
        };
        if !x.is_finite() {
            return Ok(x);
        }
        let converted = listed.operation.evaluate(x);
        if converted.is_finite() {
            Ok(converted)
        } else {
            Err(UnitError::Overflow {
                from: listed.from,
                to: listed.to,
                value: x,
            })
        }
    }
}

/// `x`, a reading in the unit `from`, in the unit `to`, by the closed table `units/v1`.
///
/// `from` is the device's unit and `to` the CDL point's, both as a point mapping spells them,
/// compared byte for byte. The same spelling on both sides is the identity: every bit of `x` is
/// returned, a signalling NaN's included. A listed pair applies its operation, in the order the
/// module's table gives, to a finite `x`; a NaN or an infinity is returned with its bits
/// unchanged. The result is in the CDL unit, the same bits on every target, and the call
/// allocates only for a refusal. It never panics.
///
/// A point with no unit on one or both sides is decided before any reading by the source that
/// reads it: no unit on either side is no conversion, and one on one side only is
/// [`UnitError::OneSided`]. This function takes two units, so it is the case of a unit on both.
///
/// ```
/// use oce_verify::{UNITS_TABLE, convert_unit};
///
/// assert_eq!(UNITS_TABLE, "units/v1");
/// assert_eq!(convert_unit("degF", "K", 32.0)?, 273.15);
/// assert_eq!(convert_unit("degF", "degC", -40.0)?, -40.0);
/// assert_eq!(convert_unit("%", "1", 100.0)?, 1.0);
/// // The table runs one way, compares spellings exactly and never passes a value through.
/// assert_eq!(convert_unit("K", "degF", 273.15).unwrap_err().code(), "unknown-pair");
/// assert_eq!(convert_unit("degf", "K", 32.0).unwrap_err().code(), "unknown-pair");
/// # Ok::<(), oce_verify::UnitError>(())
/// ```
///
/// # Errors
///
/// [`UnitError::EmptyUnit`] when either spelling is empty, the device's first;
/// [`UnitError::UnknownPair`] when the spellings differ and the table does not list the pair;
/// [`UnitError::Overflow`] when `x` is finite and its conversion is not.
pub fn convert_unit(from: &str, to: &str, x: f64) -> Result<f64, UnitError> {
    UnitConversion::resolve(Some(from), Some(to))?.apply(x)
}

/// `text` as printable ASCII, at most [`MAX_UNIT_EXCERPT_BYTES`] bytes: a character from `!` to
/// `~` as itself but for the backtick and the backslash, every other one (those two, a space, a
/// control or format character, anything outside ASCII) as `\u{…}` with its hexadecimal code
/// point. A text that fits is written whole; a longer one is cut between two characters and ends
/// with `...`. It stops reading once the excerpt is full, so its work is bounded whatever the
/// length of `text`.
pub(crate) fn excerpt(text: &str) -> String {
    const MARK: &str = "...";
    let mut out = String::new();
    // The longest prefix written so far that leaves room for the mark.
    let mut room_for_mark = 0;
    for character in text.chars() {
        if character.is_ascii_graphic() && character != '`' && character != '\\' {
            out.push(character);
        } else {
            // Writing to a `String` cannot fail.
            let _ = write!(out, "\\u{{{:x}}}", u32::from(character));
        }
        if out.len() > MAX_UNIT_EXCERPT_BYTES {
            out.truncate(room_for_mark);
            out.push_str(MARK);
            return out;
        }
        if out.len() + MARK.len() <= MAX_UNIT_EXCERPT_BYTES {
            room_for_mark = out.len();
        }
    }
    out
}

#[cfg(test)]
#[path = "units_tests.rs"]
mod tests;
