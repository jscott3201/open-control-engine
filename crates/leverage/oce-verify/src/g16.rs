//! C's `printf("%.16g")` for a binary64, decided by this crate's own arithmetic (commissioning
//! note, decision 13).
//!
//! pyfunnel writes every curve with `%.16g`, so the claim that the funnel port matches it is a
//! claim about this text, and the same text is what a bounds file is written in. Nothing here
//! calls a platform `printf` or the standard library's float formatting: the digits come from the
//! exact decimal expansion of the double, so the output is the same on every target.
//!
//! The rule, for a finite non-zero value: take the exact decimal expansion, round it to 16
//! significant digits (a tie goes to the even digit), and let `X` be the decimal exponent of the
//! rounded value. If `-4 <= X < 16` the value is written in fixed notation, otherwise as
//! `d.ddde±XX` with at least two exponent digits; trailing zeros of the fraction are dropped,
//! and the decimal point with them when no fraction is left. Zero is `0` or `-0`; the
//! non-finite values are `inf`, `-inf`, `nan` and `-nan`, as glibc spells them.
//!
//! Sixteen digits do not identify a double (seventeen do), so this text is not a round trip.

/// The significant digits `%.16g` keeps.
const DIGITS: usize = 16;

/// The base of one limb of the exact expansion: nine decimal digits.
const LIMB_BASE: u64 = 1_000_000_000;

/// `value` as C's `printf("%.16g", value)` writes it under glibc.
///
/// At most 16 significant digits, fixed notation for a decimal exponent from -4 to 15 and
/// `e±XX` outside it, no trailing zero. Deterministic and allocation-bounded for every bit
/// pattern, a NaN or an infinity included; it never panics. The text does not always read back
/// as the same double.
///
/// ```
/// use oce_verify::format_g16;
///
/// assert_eq!(format_g16(0.1), "0.1");
/// assert_eq!(format_g16(-0.0), "-0");
/// assert_eq!(format_g16(1e16), "1e+16");
/// assert_eq!(format_g16(0.0001), "0.0001");
/// assert_eq!(format_g16(0.00001), "1e-05");
/// assert_eq!(format_g16(0.1 + 0.2), "0.3"); // 0.30000000000000004 has 17 digits
/// ```
#[must_use]
pub fn format_g16(value: f64) -> String {
    let mut text = String::with_capacity(24);
    write_g16(&mut text, value);
    text
}

/// Appends [`format_g16`] of `value` to `text`.
pub(crate) fn write_g16(text: &mut String, value: f64) {
    if value.is_sign_negative() {
        text.push('-');
    }
    if value.is_nan() {
        text.push_str("nan");
        return;
    }
    if value.is_infinite() {
        text.push_str("inf");
        return;
    }
    if value == 0.0 {
        text.push('0');
        return;
    }
    let (digits, exponent) = rounded_digits(value);
    if (-4..DIGITS as i32).contains(&exponent) {
        write_fixed(text, &digits, exponent);
    } else {
        write_exponential(text, &digits, exponent);
    }
}

/// The 16 significant decimal digits of `|value|`, rounded half to even from its exact
/// expansion, and the decimal exponent `X` of the rounded value (`d.ddd × 10^X`). `value` is
/// finite and not zero.
fn rounded_digits(value: f64) -> ([u8; DIGITS], i32) {
    let (expansion, scale) = exact_expansion(value);
    // `expansion` is an integer's digits and the value is that integer over `10^scale`.
    let mut exponent = expansion.len() as i32 - 1 - scale;
    let mut digits = [0_u8; DIGITS];
    let kept = expansion.len().min(DIGITS);
    digits[..kept].copy_from_slice(&expansion[..kept]);
    if expansion.len() > DIGITS && rounds_up(&expansion, digits[DIGITS - 1]) {
        let mut index = DIGITS;
        loop {
            if index == 0 {
                // Every digit was a nine: the value rounds to the next power of ten.
                digits[0] = 1;
                exponent += 1;
                break;
            }
            index -= 1;
            if digits[index] == 9 {
                digits[index] = 0;
            } else {
                digits[index] += 1;
                break;
            }
        }
    }
    (digits, exponent)
}

/// Whether the digits after the sixteenth round the sixteenth up: more than half a unit, or
/// exactly half with the sixteenth odd.
fn rounds_up(expansion: &[u8], last_kept: u8) -> bool {
    let rest = &expansion[DIGITS..];
    match rest[0] {
        0..=4 => false,
        5 => rest[1..].iter().any(|digit| *digit != 0) || last_kept % 2 == 1,
        _ => true,
    }
}

/// The exact decimal expansion of `|value|` as an integer's digits (most significant first, no
/// leading zero) and a scale: `|value|` is that integer divided by `10^scale`. `value` is finite
/// and not zero.
///
/// A binary64 is `m × 2^e` with `m < 2^53`. For `e >= 0` the integer is `m × 2^e`, scale 0; for
/// `e < 0` it is `m × 5^(-e)`, scale `-e`, because `m / 2^k = m × 5^k / 10^k`. The integer has at
/// most 767 digits (the smallest subnormal), held in limbs of nine digits.
fn exact_expansion(value: f64) -> (Vec<u8>, i32) {
    let bits = value.to_bits();
    let fraction = bits & ((1 << 52) - 1);
    let biased = ((bits >> 52) & 0x7ff) as i32;
    let (mantissa, exponent) = if biased == 0 {
        (fraction, -1074)
    } else {
        (fraction | (1 << 52), biased - 1075)
    };

    // Little-endian limbs of `mantissa`, which is below 2^53 < 10^18.
    let mut limbs = vec![(mantissa % LIMB_BASE) as u32, (mantissa / LIMB_BASE) as u32];
    let (factor, mut steps, per_step, scale) = if exponent >= 0 {
        // 2^29 is the largest power of two below the limb base.
        (2_u64, exponent.unsigned_abs(), 29, 0)
    } else {
        // 5^13 is the largest power of five below 2^32.
        (5_u64, exponent.unsigned_abs(), 13, -exponent)
    };
    while steps > 0 {
        let step = steps.min(per_step);
        multiply(&mut limbs, factor.pow(step));
        steps -= step;
    }

    while limbs.len() > 1 && limbs[limbs.len() - 1] == 0 {
        limbs.pop();
    }
    let mut digits = Vec::with_capacity(limbs.len() * 9);
    for (position, limb) in limbs.iter().rev().enumerate() {
        let mut limb_digits = [0_u8; 9];
        let mut rest = *limb;
        for digit in limb_digits.iter_mut().rev() {
            *digit = (rest % 10) as u8;
            rest /= 10;
        }
        let skip = if position == 0 {
            limb_digits.iter().take_while(|digit| **digit == 0).count()
        } else {
            0
        };
        digits.extend_from_slice(&limb_digits[skip..]);
    }
    (digits, scale)
}

/// Multiplies the little-endian base-10^9 number `limbs` by `factor`, which is below 2^32.
fn multiply(limbs: &mut Vec<u32>, factor: u64) {
    let mut carry = 0_u64;
    for limb in limbs.iter_mut() {
        // Below 10^9 × 2^32 + 2^32, which fits in 64 bits.
        let product = u64::from(*limb) * factor + carry;
        *limb = (product % LIMB_BASE) as u32;
        carry = product / LIMB_BASE;
    }
    while carry > 0 {
        limbs.push((carry % LIMB_BASE) as u32);
        carry /= LIMB_BASE;
    }
}

/// The digits without their trailing zeros, keeping at least `keep` of them.
fn trimmed(digits: &[u8; DIGITS], keep: usize) -> &[u8] {
    let mut end = DIGITS;
    while end > keep && digits[end - 1] == 0 {
        end -= 1;
    }
    &digits[..end]
}

/// Fixed notation, for `-4 <= exponent < 16`.
fn write_fixed(text: &mut String, digits: &[u8; DIGITS], exponent: i32) {
    if exponent < 0 {
        text.push_str("0.");
        for _ in 1..exponent.unsigned_abs() {
            text.push('0');
        }
        push_digits(text, trimmed(digits, 1));
    } else {
        let integer = exponent.unsigned_abs() as usize + 1;
        let kept = trimmed(digits, integer);
        push_digits(text, &kept[..integer]);
        if kept.len() > integer {
            text.push('.');
            push_digits(text, &kept[integer..]);
        }
    }
}

/// Exponential notation: `d[.ddd]e±XX`, the exponent in at least two digits.
fn write_exponential(text: &mut String, digits: &[u8; DIGITS], exponent: i32) {
    let kept = trimmed(digits, 1);
    push_digits(text, &kept[..1]);
    if kept.len() > 1 {
        text.push('.');
        push_digits(text, &kept[1..]);
    }
    text.push('e');
    text.push(if exponent < 0 { '-' } else { '+' });
    let magnitude = exponent.unsigned_abs();
    if magnitude < 10 {
        text.push('0');
    }
    text.push_str(&magnitude.to_string());
}

/// Appends decimal digits as ASCII.
fn push_digits(text: &mut String, digits: &[u8]) {
    text.extend(digits.iter().map(|digit| char::from(b'0' + digit)));
}

#[cfg(test)]
#[path = "g16_tests.rs"]
mod tests;
