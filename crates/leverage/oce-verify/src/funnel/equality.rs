//! funnel's equality of two doubles, `equ` (`data_structure.h`), reproduced exactly.

/// Whether `a` and `b` are equal to within `1e-10` relative to the larger magnitude, and
/// absolutely below magnitude one: `|a - b| <= 1e-10 * max(1, |a|, |b|)`.
///
/// Every "equal" in the tube construction is this relation, never `==`: two reference points
/// are the same point, a slope has not changed, a half-size is zero, an intersection is already
/// in the list. It is not transitive. A NaN is equal to nothing.
pub(super) fn equ(a: f64, b: f64) -> bool {
    let magnitude_a = a.abs();
    let magnitude_b = b.abs();
    let magnitude = if magnitude_a > magnitude_b {
        magnitude_a
    } else {
        magnitude_b
    };
    (a - b).abs() <= 1e-10 * if magnitude > 1.0 { magnitude } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::equ;

    #[test]
    fn up_to_magnitude_one_the_tolerance_is_an_absolute_1e_minus_10() {
        assert!(equ(0.0, 1e-10));
        assert!(equ(0.0, -1e-10));
        assert!(!equ(0.0, 1.000_000_1e-10));
        assert!(equ(0.5, 0.5 + 5e-11));
        assert!(!equ(0.5, 0.5 + 1.5e-10));
        assert!(equ(1.0, 1.0 - 5e-11));
        assert!(!equ(1.0, 1.0 - 1.5e-10));
        assert!(equ(-0.25, -0.25));
        assert!(equ(0.0, -0.0));
    }

    #[test]
    fn above_magnitude_one_the_tolerance_scales_with_the_larger_operand() {
        // A year in seconds: an absolute 1e-10 would separate adjacent doubles here.
        let year = 3.15e7;
        assert!(equ(year, year + 3e-3));
        assert!(!equ(year, year + 4e-3));
        assert!(equ(-year, -year - 3e-3));
        // The larger of the two magnitudes sets the scale, whichever side it is on.
        assert!(equ(1e15, 1e15 + 1e5));
        assert!(equ(1e15 + 1e5, 1e15));
        assert!(!equ(1e15, -1e15));
    }

    #[test]
    fn the_slope_sentinels_differ_from_every_finite_slope_of_ordinary_size() {
        assert!(!equ(1e15, 0.0));
        assert!(!equ(-1e15, 1e15));
        assert!(equ(1e15, 1e15));
        // A slope within 1e-10 relative of the sentinel is taken for it, as in funnel.
        assert!(equ(1e15, 1e15 * (1.0 + 5e-11)));
    }

    #[test]
    fn a_nan_is_equal_to_nothing_and_an_infinity_to_everything_else_as_in_c() {
        assert!(!equ(f64::NAN, f64::NAN));
        assert!(!equ(f64::NAN, 0.0));
        assert!(!equ(0.0, f64::NAN));
        // inf - inf is NaN, and no comparison with a NaN holds.
        assert!(!equ(f64::INFINITY, f64::INFINITY));
        // An infinity against anything else: the difference is infinite and so is the tolerance,
        // 1e-10 × inf, and `inf <= inf` holds. The C says the same. Nothing infinite reaches the
        // tube: samples and tolerances are checked to be finite before it is built, and its
        // bounds after.
        assert!(equ(f64::INFINITY, f64::NEG_INFINITY));
        assert!(equ(f64::INFINITY, f64::MAX));
        assert!(equ(f64::NEG_INFINITY, 0.0));
    }
}
