//! The one place `oce-expr` gets floating-point math that `core` lacks (issue #399).
//!
//! The crate builds `no_std + alloc`; on the pinned Rust 1.97.1 toolchain `floor`, `ceil`,
//! `trunc` and `sqrt` are `std` methods, so the evaluator calls `math::<fn>` and this module
//! re-exports the pinned pure-Rust `libm` 0.2.16 implementation — the same functions
//! `oce-blocks`' `math` module re-exports, mirrored because neither crate depends on the other.
//! All four are exactly specified by IEEE 754, so they return the `std` method's bits
//! (`crates/oce-blocks/tests/libm_std_parity.rs`). `clippy.toml` lists the `std` methods and the
//! crate root denies `clippy::disallowed_methods`, so a `std` float method anywhere in the crate's
//! `src/` tree, unit tests included, fails clippy; files under `tests/` are separate crates outside
//! the lint, deliberately, because that is where the `std` methods serve as the oracle. `sqrt` is
//! currently `libm`'s software routine (its `arch` feature is off); feature unification could turn
//! `arch` on, which changes speed only, since `sqrt` is correctly rounded either way. The `core`-versus-`std` method table is in `TESTING.md` "Conventions".

/// IEEE 754 functions `core` lacks, from the pinned `libm`.
pub(crate) use libm::{ceil, floor, sqrt, trunc};
