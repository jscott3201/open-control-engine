//! Deterministic `Real` helpers shared by block execution and scalar expression folding.
//!
//! CDL §7.16 requires bit-reproducible outputs for identical inputs. IEEE-754 leaves NaN
//! payload/sign selection and some zero-sign selections target-dependent, so the engine pins the
//! few choices that affect observable `Value::Real` bits.

/// Canonical quiet-NaN bit pattern for every `Real` result.
///
/// This constant is explicit because Rust only guarantees [`f64::NAN`] is *a* quiet NaN, not a
/// specific encoding. It carries no units: it is the deterministic payload representation for an
/// otherwise dimensioned CDL `Real` value.
pub const CANONICAL_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;

/// Canonicalize a `Real` value's NaN encoding.
///
/// Any NaN, including negative, signaling, or payload-carrying NaNs, maps to
/// [`CANONICAL_NAN_BITS`]. Finite values, infinities, and both signed zeros are returned
/// bit-unchanged. The function is idempotent, allocation-free, and never panics.
#[inline]
#[must_use]
pub fn canonicalize_real(y: f64) -> f64 {
    if y.is_nan() {
        f64::from_bits(CANONICAL_NAN_BITS)
    } else {
        y
    }
}

/// Deterministic minimum for CDL `Real` values.
///
/// A single NaN operand is dropped and the non-NaN operand is returned; a NaN-only result is
/// canonicalized through [`canonicalize_real`]. For signed zeros, this pins the
/// IEEE-754-2019 `minimumNumber` convention: `min(+0.0, -0.0) == -0.0`. The function is
/// allocation-free and never panics.
#[inline]
#[must_use]
pub fn det_min(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return canonicalize_real(b);
    }
    if b.is_nan() {
        return canonicalize_real(a);
    }
    let m = a.min(b);
    if m == 0.0 && (a.is_sign_negative() || b.is_sign_negative()) {
        -0.0
    } else {
        m
    }
}

/// Deterministic maximum for CDL `Real` values.
///
/// A single NaN operand is dropped and the non-NaN operand is returned; a NaN-only result is
/// canonicalized through [`canonicalize_real`]. For signed zeros, this pins the
/// IEEE-754-2019 `maximumNumber` convention: `max(+0.0, -0.0) == +0.0`. The function is
/// allocation-free and never panics.
#[inline]
#[must_use]
pub fn det_max(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return canonicalize_real(b);
    }
    if b.is_nan() {
        return canonicalize_real(a);
    }
    let m = a.max(b);
    if m == 0.0 && (a.is_sign_positive() || b.is_sign_positive()) {
        0.0
    } else {
        m
    }
}

#[cfg(test)]
mod tests {
    use super::{CANONICAL_NAN_BITS, canonicalize_real, det_max, det_min};

    fn assert_bits(got: f64, want: f64) {
        assert_eq!(got.to_bits(), want.to_bits(), "got {got:?}, want {want:?}");
    }

    #[test]
    fn canonical_real_nan_encoding_is_single_positive_quiet_nan() {
        for bits in [
            0xfff8_0000_0000_0000,
            0x7ff0_0000_0000_0001,
            0x7fff_dead_beef_cafe,
            CANONICAL_NAN_BITS,
        ] {
            assert_eq!(
                canonicalize_real(f64::from_bits(bits)).to_bits(),
                CANONICAL_NAN_BITS
            );
        }
    }

    #[test]
    fn canonical_real_passes_non_nan_values_bit_unchanged() {
        for bits in [
            (-0.0f64).to_bits(),
            0.0f64.to_bits(),
            1.5f64.to_bits(),
            (-2.25f64).to_bits(),
            f64::INFINITY.to_bits(),
            f64::NEG_INFINITY.to_bits(),
        ] {
            assert_eq!(canonicalize_real(f64::from_bits(bits)).to_bits(), bits);
        }
    }

    #[test]
    fn deterministic_min_pins_signed_zero_and_nan_policy() {
        assert_bits(det_min(0.0, 0.0), 0.0);
        assert_bits(det_min(-0.0, 0.0), -0.0);
        assert_bits(det_min(0.0, -0.0), -0.0);
        assert_bits(det_min(-0.0, -0.0), -0.0);

        assert_bits(det_min(f64::from_bits(0xfff8_0000_0000_0000), 2.0), 2.0);
        assert_bits(det_min(2.0, f64::from_bits(0x7ff0_0000_0000_0001)), 2.0);
        assert_eq!(
            det_min(
                f64::from_bits(0xfff8_0000_0000_0000),
                f64::from_bits(0x7ff0_0000_0000_0001)
            )
            .to_bits(),
            CANONICAL_NAN_BITS
        );
    }

    #[test]
    fn deterministic_max_pins_signed_zero_and_nan_policy() {
        assert_bits(det_max(0.0, 0.0), 0.0);
        assert_bits(det_max(-0.0, 0.0), 0.0);
        assert_bits(det_max(0.0, -0.0), 0.0);
        assert_bits(det_max(-0.0, -0.0), -0.0);

        assert_bits(det_max(f64::from_bits(0xfff8_0000_0000_0000), 2.0), 2.0);
        assert_bits(det_max(2.0, f64::from_bits(0x7ff0_0000_0000_0001)), 2.0);
        assert_eq!(
            det_max(
                f64::from_bits(0xfff8_0000_0000_0000),
                f64::from_bits(0x7ff0_0000_0000_0001)
            )
            .to_bits(),
            CANONICAL_NAN_BITS
        );
    }
}
