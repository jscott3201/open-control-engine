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

use crate::model::{Literal, ParamValue};

use super::codes::{MALFORMED_SUFFIX, MAX_TOKEN_BYTES, UNIT_NOT_BASE};

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

/// Whether a value, its unit suffix and the written number a scaled suffix keeps agree, so the
/// text form reads the printed value back as the same model: a suffix is at most
/// [`MAX_TOKEN_BYTES`] bytes (the lexer's token bound), follows a number and is well formed; a
/// base-unit suffix keeps no written number; a scaled suffix keeps a written
/// number that [`scale`]s to exactly the value, a Real, bit for bit; and no written number stands
/// without its scaled suffix. The text printer refuses a model that breaks one
/// ([`crate::PrintError::Unspellable`]) and stage 1 reports it as `unit-suffix-mismatch`, so a
/// model built in code with an annotation that no longer matches its value is caught before it is
/// emitted. `Err` says which rule broke. Pure; never panics.
pub(crate) fn agreement(
    value: &ParamValue,
    suffix: Option<&str>,
    scaled_from: Option<&Literal>,
) -> Result<(), String> {
    let Some(suffix) = suffix else {
        return match scaled_from {
            Some(written) => Err(format!(
                "a written number `{}` for a scaled suffix, but no suffix",
                spelling(written)
            )),
            None => Ok(()),
        };
    };
    if suffix.len() > MAX_TOKEN_BYTES {
        return Err(format!(
            "the suffix is {} bytes; a token is at most {MAX_TOKEN_BYTES}",
            suffix.len()
        ));
    }
    if !matches!(
        value,
        ParamValue::Literal(Literal::Integer(_) | Literal::Real(_))
    ) {
        return Err(format!(
            "the suffix `{suffix}` follows a value that is not a number"
        ));
    }
    let row = match check_suffix(suffix).map_err(|(_, detail)| detail)? {
        SuffixKind::Base => {
            return match scaled_from {
                Some(written) => Err(format!(
                    "the suffix `{suffix}` is a base unit, so it keeps no written number to scale (`{}`)",
                    spelling(written)
                )),
                None => Ok(()),
            };
        }
        SuffixKind::Scaled(row) => row,
    };
    let Some(written) = scaled_from else {
        return Err(format!(
            "the scaled suffix `{suffix}` has no written number"
        ));
    };
    if !matches!(written, Literal::Integer(_) | Literal::Real(_)) {
        return Err(format!(
            "the written number before `{suffix}` is {}, not a number",
            written.kind_name()
        ));
    }
    let Some(base) = scale(written, row.factor) else {
        return Err(format!(
            "`{} {suffix}` is not finite once scaled to `{}`",
            spelling(written),
            row.base
        ));
    };
    match value {
        ParamValue::Literal(Literal::Real(v)) if v.to_bits() == base.to_bits() => Ok(()),
        ParamValue::Literal(found) => Err(format!(
            "`{} {suffix}` is `{base:?}` in `{}`, but the value is {} `{}`",
            spelling(written),
            row.base,
            found.kind_name(),
            spelling(found)
        )),
        ParamValue::Ref(_) => Err(format!(
            "the suffix `{suffix}` follows a value that is not a number"
        )),
    }
}

/// How a literal reads in a disagreement's detail: a number as the text form spells it, anything
/// else by its kind.
fn spelling(literal: &Literal) -> String {
    match literal {
        Literal::Integer(v) => v.to_string(),
        Literal::Real(v) => format!("{v:?}"),
        other => other.kind_name().to_owned(),
    }
}

/// Scale a written number by `factor` and round once to binary64 (vocabulary note, "Units"):
/// `1.1 h` is `3960.0`, where multiplying the parsed `f64` by 3600 would give
/// `3960.0000000000005`.
///
/// An Integer is multiplied exactly (in `i128`, which cannot overflow: `|i64| * u32 < 2^95`) and
/// converted with one rounding to nearest, ties to even (`4803839602528536 min`, exactly halfway,
/// is `288230376151712128.0`).
///
/// A Real is scaled from the binary64 it parsed to, not from the digits as typed: through that
/// value's shortest round-trip decimal, the spelling `fmt` prints, whose digits times `factor` are
/// an exact decimal that Rust's correctly rounded `f64` parser rounds once. That is what makes the
/// base value of a printed text the base value of the model it was printed from, so `fmt` never
/// moves a value. Two consequences follow. A literal typed with more digits than binary64 holds
/// is rounded when it is parsed and again when it is scaled, so its base value can differ from
/// the exact product of the typed digits by about an ulp, more for a subnormal
/// (`1.10000000000000009 h` is `3960.0`, as `1.1 h` is). And an Integer and a Real spelling of one
/// number past 2^53 scale differently: `9007199254740993 min` is `540431955284459584.0` s, while
/// `9007199254740993.0 min` parses to 2^53 first and is `540431955284459520.0` s. A scaled Integer
/// past 2^53 is accepted although the same Integer unscaled on a Real parameter is
/// `parameter-out-of-range`, because a scaled value is a rounded Real by construction. At the top
/// of the range the double rounding can refuse a value: a literal whose typed digits' exact
/// product rounds to `f64::MAX` can scale, through its shortest decimal, past it, and is then
/// `non-finite-number` (`2.99615522477052631e306 min`).
///
/// The sign is kept, so `-0.0 h` is `-0.0`.
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
