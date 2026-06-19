#![forbid(unsafe_code)]
//! `oce-conformance` — the funnel-style conformance harness for the Open Control Engine
//! (`07-conformance-and-verification.md`).
//!
//! Reimplements the `funnel`-style L1 tolerance band (per-signal absolute/relative tolerance +
//! time tolerance), a golden-trace driver, indicator/don't-care masking, and conformance
//! Tier 0–4 scaffolding. It is the substrate for the determinism contract (CDL §7.16): identical
//! inputs/params ⇒ identical traces. It is **Group A**-adjacent (no store, no database); the M2
//! runner will bind this tolerance logic through the public engine facade rather than direct graph
//! or block dependencies.
//!
//! Status: **Deferred to M2.** The tolerance-band DTOs are in place; `compare` and the golden driver
//! land with the M2 conformance harness.

/// A per-signal L1 tolerance band (`07` §3/§8).
#[derive(Clone, Copy, Debug)]
pub struct ToleranceBand {
    /// Absolute tolerance on the value axis.
    pub atol: f64,
    /// Relative tolerance (scaled by the signal's `nominal`).
    pub rtol: f64,
    /// Time tolerance on the time axis (event-timing slack, `atolx`).
    pub atolx: f64,
}

/// The outcome of comparing a candidate trace against a golden reference within a band.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConformanceResult {
    /// Candidate stays within the band everywhere.
    Pass,
    /// Candidate leaves the band somewhere.
    Fail,
}

/// Compare a candidate signal trace against a golden reference within a tolerance band.
#[must_use]
pub fn compare(
    _candidate: &[(f64, f64)],
    _golden: &[(f64, f64)],
    _band: ToleranceBand,
) -> ConformanceResult {
    unimplemented!("oce-conformance::compare — deferred to M2 conformance harness")
}
