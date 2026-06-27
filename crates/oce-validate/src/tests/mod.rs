//! Deep-gate test matrix for `oce-validate`. Per the safety-critical testing standard, this covers
//! each rule adversarially (boundary in-degrees, wrong directions, cross-type joins, mistyped ports),
//! the §7.10 cluster algorithm (conflict / one-sided propagation / fan-out / exact-string +
//! signed-zero tripwires / min/max bounds), the panic-free contract on malformed hand-built graphs
//! (out-of-range ids, tag-invariant violations), and determinism (repeat runs bit-identical; the full
//! sort key).
//!
//! Split across submodules to honor the 700-LOC/file cap; shared builders live in [`common`].

mod common;
mod determinism;
mod params;
mod params_sources;
mod reals_matrix;
mod routing_real;
mod routing_typed;
mod structural;
mod unification;
