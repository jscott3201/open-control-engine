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
//! A temperature is either a **reading**, a point on its scale (`degF`, `degC`), or a
//! **difference** between two readings (`degF_delta`, `degC_delta`): a setpoint adjustment, a
//! deadband, an approach. The two convert differently, so they are two spellings: 2 `degF` is
//! 256.48 K, and 2 `degF_delta` is 1.11 K. OBC §12.7 Listing 12.2 maps the setpoint adjustments
//! `setAdj` and `heaSetAdj` from `degF` to `K`; those points are differences, and their device
//! side needs `degF_delta` (owner ruling, 2026-11-21).
//!
//! | Device → CDL | Operation, in this order, in `f64` | Constant's source |
//! | --- | --- | --- |
//! | `degF` → `K` | `(x - 32.0) * (5.0 / 9.0) + 273.15` | NIST SP 811 B.9; NIST SP 330 §2.3.1 |
//! | `degC` → `K` | `x + 273.15` | NIST SP 330 §2.3.1 (`T0` = 273.15 K exactly) |
//! | `degF` → `degC` | `(x - 32.0) * (5.0 / 9.0)` | NIST SP 811 B.9 |
//! | `K` → `degC` | `x - 273.15` | NIST SP 330 §2.3.1 |
//! | `degF_delta` → `K` | `x * (5.0 / 9.0)` | NIST SP 811 B.9: a degree Fahrenheit is 1/1.8 K |
//! | `degC_delta` → `K` | the reading as it is | NIST SP 330 §2.3.1: a degree Celsius is one kelvin |
//! | `kPa` → `Pa` | `x * 1000.0` | NIST SP 330, table 7 (kilo = 10³) |
//! | `inH2O` → `Pa` | `x * 249.08891` | the conventional inch of water, 0.0254 m × 1000 kg/m³ × 9.80665 m/s², exactly: the conventional value of ISO 80000-3, as NIST SP 811 footnote 12 gives it; SP 811 B.8 prints 2.490 889 E+02 |
//! | `inH2O_60F` → `Pa` | `x * 248.84` | NIST SP 811 B.8, inch of water at 60 °F, tabulated to five digits |
//! | `cfm` → `m3/s` | `x * 0.0004719474432` | NIST SP 811 B.8: 0.3048³ m³ / 60 s, exactly |
//! | `L/s` → `m3/s` | `x / 1000.0` | NIST SP 811 B.8 (litre = 10⁻³ m³ exactly) |
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
//! **Divide, never multiply by a reciprocal.** `L/s` and `%` divide by an exact `1000.0` and
//! `100.0`, which rounds the exact quotient once; multiplying by `0.001` or `0.01`, neither exact
//! in binary, would round twice and be one ulp off for about one reading in eight.
//!
//! **What bind adds** (#662). Two checks need the sequence's ports, and bind makes both through
//! this module. A port's declared quantity admits the temperature rows of its kind only, a
//! positive rule per quantity matched by its local name (`QuantityRule`; owner rulings of
//! 2026-11-21): an absolute pair on a `TemperatureDifference` port is
//! [`UnitError::AbsoluteForDifference`], a difference pair on a `ThermodynamicTemperature` port
//! [`UnitError::DifferenceForAbsolute`]. And a mapping's `cdl.unit` must equal the unit the port
//! declares ([`UnitError::PortUnitMismatch`]): the identity rule and every row trust `cdl.unit`
//! to be the engine's unit, so they are only safe together with that check.

use std::fmt;

use super::error::printable;

/// The name and version of the unit table, as a commissioning receipt's `units.table` carries it.
pub const UNITS_TABLE: &str = "units/v1";

/// Every [`UnitError`] code, in declaration order; a test pins the list.
pub const UNIT_ERROR_CODES: &[&str] = &[
    "unknown-pair",
    "one-sided",
    "empty-unit",
    "overflow",
    "absolute-for-difference",
    "difference-for-absolute",
    "port-unit-mismatch",
];

/// The quantity a CDL port declares when its value is a temperature difference, as the
/// Buildings library's CDL sequences write it (`quantity="TemperatureDifference"`).
pub(crate) const TEMPERATURE_DIFFERENCE: &str = "TemperatureDifference";

/// The quantity a CDL port declares when its value is a temperature reading, a point on a scale
/// (`quantity="ThermodynamicTemperature"`).
pub(crate) const THERMODYNAMIC_TEMPERATURE: &str = "ThermodynamicTemperature";

/// What a port's declared quantity says about the temperature rows of `units/v1` (owner ruling
/// of 2026-11-21 on #662): a positive rule per quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuantityRule {
    /// `ThermodynamicTemperature`: absolute rows or the identity, never a difference row.
    Absolute,
    /// `TemperatureDifference`: difference rows or the identity, never an absolute row.
    Difference,
    /// Any other quantity, or none: every row.
    Any,
}

impl QuantityRule {
    /// The rule for a port whose declared quantity is `quantity`, as the facade carries it.
    ///
    /// A quantity is matched by its **local name**, the text after its last `#`, `/` or `:`
    /// (the content id's rule for a relative name), byte for byte: the bare
    /// `TemperatureDifference`, the compact IRI `q:TemperatureDifference` and a full QUDT IRI
    /// `http://qudt.org/vocab/quantitykind/TemperatureDifference` are one quantity (owner ruling
    /// of 2026-11-21). A port with no quantity takes every row: the modelica-json translations
    /// write theirs as `qudt:hasQuantityKind`, which the importer does not read yet, so for
    /// them the mapping's spelling decides.
    pub(crate) fn of(quantity: Option<&str>) -> Self {
        let Some(quantity) = quantity else {
            return Self::Any;
        };
        let local = quantity
            .rfind(['#', '/', ':'])
            .map_or(quantity, |at| &quantity[at + 1..]);
        match local {
            TEMPERATURE_DIFFERENCE => Self::Difference,
            THERMODYNAMIC_TEMPERATURE => Self::Absolute,
            _ => Self::Any,
        }
    }
}

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
/// decimal, as the nearest `f64`. Water at 1000 kg/m³ under standard gravity is the conventional
/// value of ISO 80000-3, which NIST SP 811 gives in its footnote 12; SP 811 B.8 prints the
/// factor as 2.490 889 E+02.
const PASCALS_PER_INCH_OF_WATER: f64 = 249.08891;
/// Pascals in one inch of water at 60 °F: NIST SP 811 B.8's tabulated 2.4884 E+02, a measured
/// property of water to five significant digits, not a definition.
const PASCALS_PER_INCH_OF_WATER_60F: f64 = 248.84;
/// Cubic metres a second in one cubic foot a minute: 0.3048³ / 60, the exact decimal
/// 0.000 471 947 443 2 (NIST SP 811 B.8 prints 4.719 474 E-04), as the nearest `f64`.
const CUBIC_METRES_PER_SECOND_PER_CFM: f64 = 0.000_471_947_443_2;
/// Litres in one cubic metre, exactly (NIST SP 811 B.8: the litre is 10⁻³ m³).
const LITRES_PER_CUBIC_METRE: f64 = 1000.0;
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
    /// `x * (5.0 / 9.0)`: a difference in degrees Fahrenheit, in kelvin.
    FahrenheitDifferenceToKelvin,
    /// `x`: a difference in degrees Celsius is the same number of kelvin.
    CelsiusDifferenceToKelvin,
    /// `x * factor`.
    Multiply(f64),
    /// `x / divisor`.
    Divide(f64),
}

impl Operation {
    /// Whether the operation converts a temperature reading, a point on a scale with an offset
    /// from the kelvin's zero: the four pairs among `degF`, `degC` and `K`.
    const fn is_absolute_temperature(self) -> bool {
        matches!(
            self,
            Self::FahrenheitToKelvin
                | Self::CelsiusToKelvin
                | Self::FahrenheitToCelsius
                | Self::KelvinToCelsius
        )
    }

    /// Whether the operation converts a temperature difference: the two `*_delta` rows.
    const fn is_temperature_difference(self) -> bool {
        matches!(
            self,
            Self::FahrenheitDifferenceToKelvin | Self::CelsiusDifferenceToKelvin
        )
    }

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
            Self::FahrenheitDifferenceToKelvin => x * KELVIN_PER_FAHRENHEIT_DEGREE,
            Self::CelsiusDifferenceToKelvin => x,
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
pub(crate) const UNITS_V1: [TableEntry; 13] = [
    entry("degF", "K", Operation::FahrenheitToKelvin),
    entry("degC", "K", Operation::CelsiusToKelvin),
    entry("degF", "degC", Operation::FahrenheitToCelsius),
    entry("K", "degC", Operation::KelvinToCelsius),
    entry("degF_delta", "K", Operation::FahrenheitDifferenceToKelvin),
    entry("degC_delta", "K", Operation::CelsiusDifferenceToKelvin),
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
    entry("L/s", "m3/s", Operation::Divide(LITRES_PER_CUBIC_METRE)),
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
/// A unit spelling in a refusal is an excerpt, written as the spec reader writes what it quotes:
/// printable ASCII from the space to `~` as itself, a line feed as `\n`, every other character
/// as `\u{…}` with its hexadecimal code point, and cut with `...` at 64 bytes, so a refusal
/// never carries a megabyte-long or invisible spelling. A backtick or a backslash in a unit is
/// not escaped, so a quoted spelling that holds one can read ambiguously; the code, not the
/// sentence, is the contract.
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
    /// A finite reading whose converted value is not finite: its magnitude is beyond `f64::MAX`,
    /// positive or negative. Only a pair whose factor is above one can overflow, and only for a
    /// reading beyond about ±1e305.
    Overflow {
        /// The device's unit, as the table spells it.
        from: &'static str,
        /// The CDL point's unit, as the table spells it.
        to: &'static str,
        /// The reading, in the device's unit.
        value: f64,
    },
    /// An absolute temperature pair (`degF` → `K`, `degC` → `K`, `degF` → `degC`, `K` →
    /// `degC`) on a port whose declared quantity is `TemperatureDifference`: the offset would
    /// turn a 2 °F adjustment into 256.48 K. The device side needs a difference unit,
    /// `degF_delta` or `degC_delta` (owner ruling, 2026-11-21). Decided at bind (#662), where
    /// the port's quantity is known.
    AbsoluteForDifference {
        /// The device's unit, as the table spells it.
        from: &'static str,
        /// The CDL point's unit, as the table spells it.
        to: &'static str,
    },
    /// A temperature difference pair (`degF_delta` → `K`, `degC_delta` → `K`) on a port whose
    /// declared quantity is `ThermodynamicTemperature`: a reading converted without its offset
    /// would be 255 K too cold. The device side needs `degF` or `degC` (owner ruling,
    /// 2026-11-21). Decided at bind, where the port's quantity is known.
    DifferenceForAbsolute {
        /// The device's unit, as the table spells it.
        from: &'static str,
        /// The CDL point's unit, as the table spells it.
        to: &'static str,
    },
    /// A mapping's `cdl.unit` that is not the unit the sequence port declares: the identity
    /// rule and every row trust `cdl.unit` to be the engine's unit (owner ruling, 2026-11-21).
    /// A unit on one side only disagrees; no unit on either side agrees. Decided at bind.
    PortUnitMismatch {
        /// The mapping's `cdl.unit`, as an excerpt; `None` when the mapping writes none.
        mapping: Option<String>,
        /// The port's declared unit, as an excerpt; `None` when it declares none.
        port: Option<String>,
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
            Self::AbsoluteForDifference { .. } => 4,
            Self::DifferenceForAbsolute { .. } => 5,
            Self::PortUnitMismatch { .. } => 6,
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
                "the reading {value:?} in `{from}` converts to a value in `{to}` whose magnitude \
                 is beyond the largest `f64`"
            ),
            Self::AbsoluteForDifference { from, to } => write!(
                f,
                "`{from}` to `{to}` converts a temperature reading, but the port is a \
                 temperature difference: write the device unit as `degF_delta` or `degC_delta`"
            ),
            Self::DifferenceForAbsolute { from, to } => write!(
                f,
                "`{from}` to `{to}` converts a temperature difference, but the port is a \
                 temperature reading: write the device unit as `degF` or `degC`"
            ),
            Self::PortUnitMismatch { mapping, port } => {
                let shown = |unit: &Option<String>| {
                    unit.as_ref()
                        .map_or_else(|| "no unit".to_owned(), |unit| format!("`{unit}`"))
                };
                write!(
                    f,
                    "the mapping gives the CDL point {} and the sequence port declares {}",
                    shown(mapping),
                    shown(port)
                )
            }
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
    /// of the two spellings and at most thirteen pairs of comparisons against the table; nothing is
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

    /// The conversion again, for a CDL port whose declared quantity is `quantity` (the
    /// facade's `PointInfo::quantity`): bind calls it once per mapped point, before any frame.
    ///
    /// A positive rule per quantity ([`QuantityRule::of`], owner ruling of 2026-11-21): a
    /// `ThermodynamicTemperature` port takes an absolute row or the identity, a
    /// `TemperatureDifference` port a difference row or the identity, and any other quantity,
    /// or none, every row. A quantity is matched by its local name, so an IRI spelling of
    /// either counts.
    ///
    /// # Errors
    ///
    /// [`UnitError::AbsoluteForDifference`] for `degF` → `K`, `degC` → `K`, `degF` → `degC` or
    /// `K` → `degC` on a `TemperatureDifference` port; [`UnitError::DifferenceForAbsolute`] for
    /// `degF_delta` → `K` or `degC_delta` → `K` on a `ThermodynamicTemperature` port.
    pub(crate) fn for_port_quantity(self, quantity: Option<&str>) -> Result<Self, UnitError> {
        let Self::Listed(listed) = self else {
            return Ok(self);
        };
        match QuantityRule::of(quantity) {
            QuantityRule::Difference if listed.operation.is_absolute_temperature() => {
                Err(UnitError::AbsoluteForDifference {
                    from: listed.from,
                    to: listed.to,
                })
            }
            QuantityRule::Absolute if listed.operation.is_temperature_difference() => {
                Err(UnitError::DifferenceForAbsolute {
                    from: listed.from,
                    to: listed.to,
                })
            }
            _ => Ok(self),
        }
    }

    /// Whether a mapping's `cdl.unit` is the unit the port declares: the same spelling, byte for
    /// byte, or none on both sides (owner ruling of 2026-11-21).
    ///
    /// # Errors
    ///
    /// [`UnitError::PortUnitMismatch`] for any other pair.
    pub(crate) fn check_port_unit(
        mapping: Option<&str>,
        port: Option<&str>,
    ) -> Result<(), UnitError> {
        if mapping == port {
            Ok(())
        } else {
            Err(UnitError::PortUnitMismatch {
                mapping: mapping.map(excerpt),
                port: port.map(excerpt),
            })
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

/// A unit spelling as a refusal quotes it: the spec reader's one quoting rule
/// ([`printable`]), cut at [`MAX_UNIT_EXCERPT_BYTES`].
pub(crate) fn excerpt(text: &str) -> String {
    printable(text, MAX_UNIT_EXCERPT_BYTES)
}

#[cfg(test)]
#[path = "units_tests.rs"]
mod tests;
