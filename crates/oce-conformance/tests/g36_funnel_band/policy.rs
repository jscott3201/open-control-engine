//! Recorded per-signal conformance tolerance policy for routing G36 Tier-A goldens through the
//! oce-conformance L1 funnel band (`_spec/07 §8`).
//!
//! This module is the in-repo, auditable system-of-record for the abs/rel/time tolerances each G36
//! funnel-band conformance claim assumes (`_spec/07 §8` requirement 4). Every value is authored from
//! the signal's CDL block class and physical meaning — **never** from observed engine output — so the
//! band is engine-independent and the routing cannot become a self-pin.
//!
//! It is a shared test-support module included by the G36 funnel-band suites via
//! `#[path = "g36_funnel_band/policy.rs"] mod funnel_band_policy;`, and lives in a subdirectory so
//! Cargo does not compile it as its own integration-test binary. Each consumer uses a different
//! subset of the policy, so `#![allow(dead_code)]` (the `block_harness` convention) covers items
//! that are unused in any one binary.
//!
//! Discrete signals are deliberately **absent**: Boolean/Integer G36 outputs are compared exactly
//! (`_spec/07 §9.3`) and are kept on `ComparisonMode::Exact`, never routed through the type-blind
//! funnel with a non-zero band, so a value between two discrete levels can never be admitted "within
//! tolerance". `DISCRETE_EXACT` records that collapsed band for any future funnel-mode run that must
//! carry a discrete output alongside a Real one.
#![allow(dead_code)]

use oce_conformance::{OutputPattern, PartialTolerances, Series, Tolerances, compare};

/// Real-algebraic `[A]` last-ULP relative band (`_spec/07 §8`): pure f64 `Add`/`Gain`/`Limiter`/
/// `Switch`/routing/conversion ops differ from a closed-form oracle only in the last ULP, so the one
/// legitimate slack is a tiny *relative value* band (`rtoly`). Algebraic signals carry **no** time
/// slack (`atolx = rtolx = 0`) — they are evaluated at exact instants (`_spec/07 §8`, `[A]` row). A
/// reviewer can derive this from "these are exact f64 arithmetic ops" with no reference to engine
/// output. On a multi-sample reference it yields a live band of `1e-9 * range_y`; on a single-sample
/// reference (`range_y == 0`) it correctly collapses to exact (`_spec/07 §8` requirement 2 — exact
/// where the math is exact). The value is >10 orders of magnitude below the smallest control-decision
/// granularity on this corpus (high-limit cutoff buckets are 1 K apart; integer requests/levels
/// differ by 1), so no authored band can admit a wrong bucket, offset, or level.
pub const REAL_ALGEBRAIC_LAST_ULP: f64 = 1e-9;

/// The all-zero (collapsed) band for exact-compared Boolean/Integer signals (`_spec/07 §9.3`). These
/// signals stay on `ComparisonMode::Exact`; this constant exists so that a mixed-kind sequence that
/// must run in funnel mode can pin its discrete outputs to an exact band rather than inherit any
/// Real band.
pub const DISCRETE_EXACT: PartialTolerances = PartialTolerances {
    atolx: Some(0.0),
    atoly: Some(0.0),
    rtolx: Some(0.0),
    rtoly: Some(0.0),
    ltolx: Some(0.0),
    ltoly: Some(0.0),
};

/// A fully-zero base band. G36 funnel configs start from this and layer per-output overrides, so an
/// output with no override is compared exactly rather than inheriting a stray default.
#[must_use]
pub fn zero_base() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

/// The per-output tolerance override for a Real-algebraic signal, keyed on the engine-side connector
/// id the driver resolves through `VerifyConfig::tolerance_for_output`. The pattern is anchored so it
/// matches only the intended connector and cannot bleed onto another signal in the same config.
#[must_use]
pub fn real_algebraic_override(cdl_connector: &str) -> OutputPattern {
    OutputPattern {
        pattern: format!("^{}$", escape_regex(cdl_connector)),
        tolerances: PartialTolerances {
            atolx: Some(0.0),
            atoly: Some(0.0),
            rtolx: Some(0.0),
            rtoly: Some(REAL_ALGEBRAIC_LAST_ULP),
            ltolx: Some(0.0),
            ltoly: Some(0.0),
        },
    }
}

/// The resolved Real-algebraic band a driver records on each `SignalComparison` — the exact tolerance
/// that shipped, so tests can assert the recorded band actually landed on the routed signal.
#[must_use]
pub fn real_algebraic_tolerance() -> Tolerances {
    real_algebraic_override("_")
        .tolerances
        .apply_to(zero_base())
}

/// Anti-tautology control: prove the recorded Real-algebraic band is *falsifiable*, not decorative.
///
/// Because the engine reproduces every G36 Tier-A reference bit-for-bit, a band over that reference
/// never binds on the engine trace itself. This helper instead perturbs a synthetic test trace off
/// the reference by a fraction of the band (must pass) and by a multiple of the band (must fail),
/// with the perturbation derived from the reference's own range and the authored `rtoly` — never from
/// engine output. A passing call demonstrates the band has the width `1e-9 * range` we claim and is
/// neither zero-width (tautological) nor wider than the control granularity.
///
/// # Panics
/// Panics if `reference_y` is not a multi-sample trace with a strictly positive range (the relative
/// band is inert on a single sample), or if either boundary trace lands on the wrong side of the band.
pub fn assert_real_algebraic_band_is_falsifiable(times: &[f64], reference_y: &[f64]) {
    assert!(
        reference_y.len() >= 2,
        "falsifiability needs a multi-sample reference"
    );
    let min = reference_y.iter().copied().fold(f64::INFINITY, f64::min);
    let max = reference_y
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    assert!(
        range > 0.0,
        "reference range must be positive to exercise the relative band"
    );
    let half_band = REAL_ALGEBRAIC_LAST_ULP * range;
    let tol = real_algebraic_tolerance();
    let reference = Series {
        x: times,
        y: reference_y,
    };

    let within: Vec<f64> = reference_y.iter().map(|y| y + 0.25 * half_band).collect();
    let within_result = compare(
        reference,
        Series {
            x: times,
            y: &within,
        },
        &tol,
    );
    assert!(
        within_result.passed && within_result.max_error == 0.0,
        "a trace within the recorded band must pass (first failure {:?})",
        within_result.first_failure_x
    );

    let beyond: Vec<f64> = reference_y.iter().map(|y| y + 4.0 * half_band).collect();
    let beyond_result = compare(
        reference,
        Series {
            x: times,
            y: &beyond,
        },
        &tol,
    );
    assert!(
        !beyond_result.passed && beyond_result.max_error > 0.0,
        "a trace outside the recorded band must fail — the band is not falsifiable"
    );
}

/// Escape the regex metacharacters that could appear in an engine connector id so an override pattern
/// matches its connector literally. Connector ids are `conn#<n>`; `#` and digits are already literal,
/// but escaping defensively keeps the keying robust if the id form ever changes.
fn escape_regex(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        if "\\.^$|?*+()[]{}".contains(ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}
