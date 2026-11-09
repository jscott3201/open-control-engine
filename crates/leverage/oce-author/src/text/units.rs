//! Unit suffixes: base units only in M2 (decision 13).
//!
//! A suffix is text-only: it never reaches the wire form, the emitter or the engine, and the
//! number is the value in the base unit as written. Until vocabulary v1 supplies scale factors
//! there is no conversion table, so a suffix is accepted only when every factor is a coherent,
//! unprefixed SI unit ([`BASE_UNIT_SYMBOLS`]); `1 h`, `20 degC` and `3 kPa` are refused with
//! [`UNIT_NOT_BASE`] rather than read as a value the author did not mean.
//!
//! The grammar is Modelica's unit-expression syntax without parentheses:
//! `suffix = numerator [ "/" factors ]`, `numerator = "1" | factors` (`1` only before `/`),
//! `factors = factor { "." factor }`, `factor = symbol [ [ "-" ] digits ]` (`m2`, `s-1`).

use super::codes::{MALFORMED_SUFFIX, UNIT_NOT_BASE};

/// The unit symbols a suffix may use: the SI base units and the coherent derived units with
/// special names, none prefixed (`kg` is the base unit of mass).
pub const BASE_UNIT_SYMBOLS: &[&str] = &[
    "m", "kg", "s", "A", "K", "mol", "cd", "rad", "sr", "Hz", "N", "Pa", "J", "W", "C", "V", "F",
    "Ohm", "S", "Wb", "T", "H", "lm", "lx", "Bq", "Gy", "Sv", "kat",
];

/// Judge a suffix: `Ok` for a base-unit expression, else the refusal code and its detail.
pub(super) fn check_suffix(suffix: &str) -> Result<(), (&'static str, String)> {
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
        return Err((
            UNIT_NOT_BASE,
            format!(
                "`{symbol}` in `{suffix}` is not a base unit; M2 accepts base units only, and scaled or offset units wait for vocabulary v1 (decision 13)"
            ),
        ));
    }
    Ok(())
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
        symbols.push(&factor[..letters]);
    }
    Ok(())
}
