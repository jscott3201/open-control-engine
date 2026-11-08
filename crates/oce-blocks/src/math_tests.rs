//! Oracle cross-checks for [`crate::math`]: the helpers it defines in place of `std`-only `f64`
//! methods return the bits those methods return. The re-exported `libm` functions are checked
//! against `std` in `crates/oce-blocks/tests/libm_std_parity.rs`.

use crate::math;

/// The `no_std` `rem_euclid` helper is the expression `f64::rem_euclid` evaluates, so it returns
/// the method's bits for every operand pair the periodic table can see: signed, zero, subnormal,
/// non-finite and NaN dividends against positive, negative and non-finite periods.
#[test]
// The oracle is the `std` method itself, which the crate root denies everywhere else.
#[allow(clippy::disallowed_methods)]
fn rem_euclid_helper_reproduces_the_std_method_bit_for_bit() {
    let operands = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        3.0,
        -3.0,
        7.25,
        -7.25,
        86_400.0,
        -86_400.0,
        1e-300,
        -1e-300,
        f64::from_bits(1),
        1e300,
        -1e300,
        f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];
    for x in operands {
        for rhs in operands {
            let (got, want) = (math::rem_euclid(x, rhs), x.rem_euclid(rhs));
            assert!(
                got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
                "rem_euclid({x:e}, {rhs:e}): helper {got:e}, std {want:e}"
            );
        }
    }
}
