//! The one place `oce-blocks` gets floating-point math that `core` lacks (issue #399).
//!
//! The crate builds `no_std + alloc`, and on the pinned Rust 1.97.1 toolchain `core` carries only
//! the exact sign/compare/scale `f64` methods. Every other `f64` operation is a `std` method there,
//! so this module re-exports the pinned pure-Rust `libm` 0.2.16 implementation instead, and every
//! block calls `math::<fn>`. The strict-bit evidence (`docs/strict-bit-evidence.md`) is bound to
//! these exact `libm` functions, so changing one here is a numerics change, not a refactor.
//! `clippy.toml` lists the `std` methods and the crate root denies `clippy::disallowed_methods`, so
//! a `std` float method anywhere else in the crate fails clippy.
//!
//! What `core` provides on 1.97.1, probed per method with a `no_std` build for
//! `thumbv7em-none-eabihf` (`scripts/ci/test-math-lint.sh` repeats the probe):
//!
//! | Source | `f64` methods |
//! | --- | --- |
//! | `core` (call the method) | `abs`, `signum`, `copysign`, `min`, `max`, `clamp`, `recip`, `to_degrees`, `to_radians`, `total_cmp`, `midpoint`, the `is_*` classifiers, `to_bits`/`from_bits` |
//! | `std` only (call `math`) | `floor`, `ceil`, `round`, `round_ties_even`, `trunc`, `fract`, `sqrt`, `cbrt`, `mul_add`, `powi`, `powf`, `div_euclid`, `rem_euclid`, `hypot`, `exp`, `exp2`, `exp_m1`, `ln`, `ln_1p`, `log`, `log2`, `log10`, the trigonometric and hyperbolic functions |
//!
//! `floor`, `ceil`, `round`, `trunc` and `sqrt` are exactly specified by IEEE 754, so `libm`
//! returns the `std` method's bits; `crates/oce-blocks/tests/libm_std_parity.rs` checks that on a
//! boundary table and a 2^20 bit-pattern sweep. `sqrt` stays on `libm` in `std` builds too: it is
//! software there (the `libm` `arch` feature is off), about 2.3 ns slower per call on Apple M5,
//! which no tick measurement could separate from run-to-run noise (`docs/benchmarks.md`).

/// IEEE 754 functions `core` lacks, from the pinned `libm`. Names match `libm` (C `math.h`):
/// `pow` is `f64::powf`, `log` is `f64::ln`.
pub(crate) use libm::{
    acos, asin, atan, atan2, ceil, cos, exp, floor, log, log10, pow, round, sin, sqrt, tan,
};

/// `f64::rem_euclid` without `std`: the expression the 1.97 standard library evaluates
/// (`core::f64::math::rem_euclid`), so the result is bit-identical to the method it replaces.
/// `%` and `abs` are `core` operations. Never panics; a zero or NaN `rhs` gives NaN, as the
/// method does.
pub(crate) fn rem_euclid(x: f64, rhs: f64) -> f64 {
    let r = x % rhs;
    if r < 0.0 { r + rhs.abs() } else { r }
}
