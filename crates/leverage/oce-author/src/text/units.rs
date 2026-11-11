//! Unit suffixes: base units, and the scaled time units of the vocabulary's table (decision 13,
//! amended by the vocabulary note's decision 14; issue #501).
//!
//! A suffix is text-only: it never reaches the wire form, the emitter or the engine. A suffix
//! built from coherent, unprefixed SI units ([`BASE_UNIT_SYMBOLS`]) is an annotation, and the
//! number is the value in that base unit as written. A suffix that is exactly one symbol of
//! `oce_vocab::SCALE_FACTORS` (`min`, `h`, `d`: an exact integer multiple of `s`) scales the
//! number: `1 h` is the base-unit Real `3600.0` ([`scale`]), and the model keeps the written `1`
//! beside it so `fmt` prints `1 h` again ([`crate::Assignment::scaled_from`]). Anything else is
//! refused with [`UNIT_NOT_BASE`] rather than read as a value the author did not mean: an affine
//! unit (`20 degC`), a prefixed one (`3 kPa`), a scaled symbol inside a compound (`m3/h`, `h2`),
//! where the factor would not be an integer multiple of one base unit, and any other symbol.
//! Whether a suffix fits the parameter it is written on is stage 1's (`unit-suffix-mismatch`).
//!
//! The grammar is a subset of Modelica's unit-expression syntax:
//! `suffix = numerator [ "/" factors ]`, `numerator = "1" | factors` (`1` only before `/`),
//! `factors = factor { "." factor }`, `factor = symbol [ [ "-" ] digits ]` with a non-zero
//! exponent (`m2`, `s-1`; `K0` and `m-0` are refused). There are no parentheses, and there is
//! at most one `/`, whose right-hand side is the whole denominator: `kg/m.s2` reads as
//! kg/(m·s²). That is a deliberate deviation from Modelica's left-to-right reading
//! ((kg/m)·s²), chosen so a suffix reads as an engineer writes a quotient (issue #482).

use oce_vocab::{ScaleFactor, scale_factor};

use crate::model::Literal;

use super::codes::{MALFORMED_SUFFIX, UNIT_NOT_BASE};

/// The unit symbols a suffix may use: the SI base units and the coherent derived units with
/// special names, none prefixed (`kg` is the base unit of mass).
pub const BASE_UNIT_SYMBOLS: &[&str] = &[
    "m", "kg", "s", "A", "K", "mol", "cd", "rad", "sr", "Hz", "N", "Pa", "J", "W", "C", "V", "F",
    "Ohm", "S", "Wb", "T", "H", "lm", "lx", "Bq", "Gy", "Sv", "kat",
];

/// What a well-formed suffix means for the number before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SuffixKind {
    /// A base-unit expression (`K`, `m/s2`): an annotation; the number is the value as written.
    Base,
    /// A scaled time unit written alone (`h`): the number is scaled to the row's base unit.
    Scaled(&'static ScaleFactor),
}

impl SuffixKind {
    /// The unit the suffix reduces to: the suffix itself for a base-unit expression, the base of
    /// the row (`s`) for a scaled symbol.
    pub(crate) fn reduced(self, suffix: &str) -> &str {
        match self {
            Self::Base => suffix,
            Self::Scaled(row) => row.base,
        }
    }
}

/// Judge a suffix: its kind, or the refusal code and its detail.
pub(crate) fn check_suffix(suffix: &str) -> Result<SuffixKind, (&'static str, String)> {
    if let Some(row) = scale_factor(suffix) {
        return Ok(SuffixKind::Scaled(row));
    }
    let malformed = |why: &str| {
        Err((
            MALFORMED_SUFFIX,
            format!("`{suffix}` is not a unit expression: {why}"),
        ))
    };
    let (numerator, denominator) = match suffix.split_once('/') {
        Some((n, d)) => (n, Some(d)),
        None => (suffix, None),
    };
    let mut symbols = Vec::new();
    if numerator == "1" {
        if denominator.is_none() {
            return malformed("a bare `1` is no unit; leave the suffix out");
        }
    } else if let Err(why) = factors(numerator, &mut symbols) {
        return malformed(why);
    }
    if let Some(denominator) = denominator
        && let Err(why) = factors(denominator, &mut symbols)
    {
        return malformed(why);
    }
    if let Some(symbol) = symbols.iter().find(|s| !BASE_UNIT_SYMBOLS.contains(s)) {
        let detail = if scale_factor(symbol).is_some() {
            format!(
                "`{symbol}` in `{suffix}` is a scaled unit, which is accepted only as the whole suffix (`1 {symbol}`); a compound with it is no integer multiple of one base unit"
            )
        } else {
            format!(
                "`{symbol}` in `{suffix}` is not a base unit; a suffix is built from base units, or is one of the scaled time units `min`, `h` and `d` alone (affine, prefixed and other scaled units are refused)"
            )
        };
        return Err((UNIT_NOT_BASE, detail));
    }
    Ok(SuffixKind::Base)
}

fn factors<'a>(text: &'a str, symbols: &mut Vec<&'a str>) -> Result<(), &'static str> {
    if text.is_empty() {
        return Err("a factor is missing");
    }
    for factor in text.split('.') {
        let letters = factor.bytes().take_while(u8::is_ascii_alphabetic).count();
        if letters == 0 {
            return Err("a factor starts with a unit symbol");
        }
        let exponent = &factor[letters..];
        let digits = exponent.strip_prefix('-').unwrap_or(exponent);
        if !digits.bytes().all(|b| b.is_ascii_digit())
            || (exponent.len() > digits.len() && digits.is_empty())
        {
            return Err("an exponent is an optional `-` and digits after the symbol");
        }
        if !digits.is_empty() && digits.bytes().all(|b| b == b'0') {
            return Err("an exponent is not zero");
        }
        symbols.push(&factor[..letters]);
    }
    Ok(())
}

/// Scale a written number by `factor` exactly and round once to binary64 (vocabulary note,
/// "Units"): `1.1 h` is `3960.0`, where multiplying the parsed `f64` by 3600 would give
/// `3960.0000000000005`.
///
/// An Integer is multiplied exactly (in `i128`, which cannot overflow: `|i64| * u32 < 2^95`) and
/// converted with one rounding to nearest, ties to even. A Real is scaled through its shortest
/// round-trip decimal, the spelling `fmt` prints for it, so the base value of a printed text is
/// the base value of the model it was printed from: its decimal digits times `factor` are an
/// exact decimal, which Rust's correctly rounded `f64` parser rounds once. The sign is kept, so
/// `-0.0 h` is `-0.0`.
///
/// `None` when the literal is not a number, the Real is not finite, or the result overflows
/// binary64. Pure; never panics.
pub(crate) fn scale(written: &Literal, factor: u32) -> Option<f64> {
    let scaled = match written {
        // The one rounding the scaling rule defines: an integer-to-float cast rounds to nearest.
        Literal::Integer(v) => (i128::from(*v) * i128::from(factor)) as f64,
        Literal::Real(v) if v.is_finite() => scale_decimal(&format!("{v:?}"), factor)?,
        _ => return None,
    };
    scaled.is_finite().then_some(scaled)
}

/// Scale a finite Rust `{:?}` spelling of an `f64` (`-1.5`, `1e-7`, `1.7976931348623157e308`)
/// exactly by `factor`, rounding once.
fn scale_decimal(spelling: &str, factor: u32) -> Option<f64> {
    let (negative, unsigned) = match spelling.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, spelling),
    };
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().ok()?),
        None => (unsigned, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() || !(whole.bytes().chain(fraction.bytes())).all(|b| b.is_ascii_digit()) {
        return None;
    }
    // At most 17 significant digits (the shortest round-trip spelling), so the digits fit `u128`
    // with room for the factor; leading zeros of a small value (`0.0001`) parse away.
    let digits: u128 = format!("{whole}{fraction}").parse().ok()?;
    let product = digits.checked_mul(u128::from(factor))?;
    let exponent = exponent.checked_sub(i32::try_from(fraction.len()).ok()?)?;
    let sign = if negative { "-" } else { "" };
    format!("{sign}{product}e{exponent}").parse::<f64>().ok()
}

#[cfg(test)]
#[path = "units_tests.rs"]
mod tests;
