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

use std::path::Path;

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, OutputPattern, PartialTolerances, PointEnd, PointMapEntry, ReferenceSpec,
    Series, Tolerances, ValueKind, VerifyConfig, compare, drive_trace_with_options,
};

/// Real-algebraic `[A]` last-ULP relative band (`_spec/07 §8`): pure f64 `Add`/`Gain`/`Limiter`/
/// `Switch`/routing/conversion ops differ from a closed-form oracle only in the last ULP, so the one
/// legitimate slack is a tiny *relative value* band (`rtoly`). Algebraic signals carry **no** time
/// slack (`atolx = rtolx = 0`) — they are evaluated at exact instants (`_spec/07 §8`, `[A]` row). A
/// reviewer can derive this from "these are exact f64 arithmetic ops" with no reference to engine
/// output. On a multi-sample reference it yields a live band of `1e-9 * range_y`; on a single-sample
/// reference (`range_y == 0`) it correctly collapses to exact (`_spec/07 §8` requirement 2 — exact
/// where the math is exact). The band is relative, so its width is `1e-9 * range_y`: the widest it
/// reaches on this corpus is `1e-9 * the 12.5 K max reference range ≈ 1.25e-8 K`, which is ~7.9 orders
/// of magnitude below the 1 K high-limit bucket granularity (Title24 fixed buckets are
/// 294.15/295.15/296.15/297.15 K), so no authored band can admit a wrong bucket, offset, or level.
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

/// One point in a G36 funnel-band routing call: its reference-CSV column name, the engine-side
/// connector id the mapping keys on, and its CDL value kind.
#[derive(Clone, Copy)]
pub(crate) struct FunnelPoint {
    pub(crate) reference_name: &'static str,
    pub(crate) cdl_name: &'static str,
    pub(crate) kind: ValueKind,
}

/// One G36 sequence's funnel-band routing inputs, bundled into a struct to keep the call under the
/// `clippy::too_many_arguments` threshold. `inputs` are replayed regardless of kind; `outputs` is
/// filtered to Real before the mapping is built, so Boolean/Integer outputs are excluded from the
/// funnel entirely (`_spec/07 §9.3`). `kind_to_str` is the consumer file's own `kind_name` so the
/// funnel mapping's `PointEnd.kind` strings match that file's exact-oracle mapping byte-for-byte.
pub(crate) struct FunnelRouting<'a> {
    pub(crate) sequence: &'a str,
    pub(crate) cxf: &'a [u8],
    pub(crate) inputs: &'a [FunnelPoint],
    pub(crate) outputs: &'a [FunnelPoint],
    pub(crate) instants: &'a [f64],
    pub(crate) sample_step: f64,
    pub(crate) reference_csv: &'a Path,
    pub(crate) kind_to_str: fn(ValueKind) -> &'static str,
}

/// Route only the Real (`[A]`) outputs of one G36 Tier-A sequence through the L1 funnel band with the
/// recorded last-ULP band, keeping Boolean/Integer outputs OFF the type-blind funnel (`_spec/07 §9.3`)
/// by omitting them from the mapping. Asserts, per Real output, that the band is the live comparison
/// mechanism and actually resolved onto the signal, then runs the falsifiability control on the first
/// Real output whose reference column varies.
///
/// # Panics
/// Panics if there are no Real outputs (an all-discrete sequence must be skipped by the caller), if
/// `instants.len()` disagrees with the reference row count, if the driver run or any per-output band
/// assertion fails, or if no Real output varies (the falsifiability control needs a `range_y > 0`
/// column; every sequence in this corpus has one, so an all-constant sequence fails loudly rather
/// than silently dropping coverage).
pub(crate) fn route_real_outputs_through_funnel_band(routing: &FunnelRouting) {
    let real: Vec<FunnelPoint> = routing
        .outputs
        .iter()
        .copied()
        .filter(|point| matches!(point.kind, ValueKind::Real))
        .collect();
    assert!(
        !real.is_empty(),
        "{}: no Real output to route (all-discrete sequences must be skipped by the caller)",
        routing.sequence
    );

    let reference = CombiTimeTable::read(routing.reference_csv)
        .unwrap_or_else(|err| panic!("{} reference read failed: {err}", routing.sequence));
    assert_eq!(
        routing.instants.len(),
        reference.n_rows,
        "{}: driven instant count must match the reference row count (compared_points relies on \
         instant/time bit-equality)",
        routing.sequence
    );

    let mapping: Vec<PointMapEntry> = routing
        .inputs
        .iter()
        .chain(real.iter())
        .map(|point| PointMapEntry {
            cdl: point_end(point.cdl_name, point.kind, routing.kind_to_str),
            device: point_end(point.reference_name, point.kind, routing.kind_to_str),
        })
        .collect();
    let overrides: Vec<OutputPattern> = real
        .iter()
        .map(|point| real_algebraic_override(point.cdl_name))
        .collect();
    let config = VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: routing.sequence.to_string(),
            point_name_mapping: mapping,
        }],
        tolerances: zero_base(),
        outputs: overrides,
        indicators: Vec::new(),
        sampling: Some(routing.sample_step),
        run_controller: true,
    };

    let run = drive_trace_with_options(
        routing.cxf,
        &config,
        &reference,
        &DriverOptions {
            cadence: DriveCadence::EventAligned {
                instants: routing.instants.to_vec(),
            },
            input_replay: DriverInputReplay::ReferenceTable,
            comparison: ComparisonMode::Funnel,
        },
    )
    .unwrap_or_else(|err| panic!("{} funnel driver run failed: {err}", routing.sequence));

    // Exactly one comparison per Real output is the machine-checked proof that every discrete output
    // was excluded from the funnel (omitted from the mapping), never banded.
    assert_eq!(
        run.comparisons.len(),
        real.len(),
        "{}: funnel comparison count (discrete outputs must be excluded)",
        routing.sequence
    );

    for point in &real {
        let comparison = run
            .comparisons
            .iter()
            .find(|comparison| comparison.reference_column == point.reference_name)
            .unwrap_or_else(|| {
                panic!(
                    "{} missing funnel comparison for {}",
                    routing.sequence, point.reference_name
                )
            });
        assert_eq!(
            comparison.output, point.cdl_name,
            "{} output",
            routing.sequence
        );
        assert!(
            !comparison.masked,
            "{} {} must be an unmasked funnel comparison",
            routing.sequence, point.reference_name
        );
        assert_recorded_real_algebraic_band(
            routing.sequence,
            point.reference_name,
            &comparison.tolerance,
        );
        match &comparison.result {
            ComparisonResult::Funnel(result) => {
                assert!(
                    result.passed,
                    "{} funnel comparison failed for {}: {:?}",
                    routing.sequence, point.reference_name, result.first_failure_x
                );
                assert_eq!(
                    result.compared_points, reference.n_rows,
                    "{} {} funnel compared points",
                    routing.sequence, point.reference_name
                );
                assert!(
                    result.compared_points > 0,
                    "{} {} funnel passed vacuously",
                    routing.sequence,
                    point.reference_name
                );
                assert_eq!(
                    result.max_error.to_bits(),
                    0.0f64.to_bits(),
                    "{} {} funnel max error",
                    routing.sequence,
                    point.reference_name
                );
                assert_eq!(
                    result.first_failure_x, None,
                    "{} {} funnel first failure",
                    routing.sequence, point.reference_name
                );
            }
            other => panic!(
                "{} {} used non-funnel comparison: {other:?}",
                routing.sequence, point.reference_name
            ),
        }
    }

    assert_first_varying_real_output_band_is_falsifiable(&reference, &real, routing.sequence);
}

/// Run the anti-tautology falsifiability control on the first Real output whose reference column
/// varies (`range_y > 0`). A missing varying column is a hard failure, not a silent skip, so a future
/// regression that flatlines every Real output surfaces rather than quietly dropping coverage.
fn assert_first_varying_real_output_band_is_falsifiable(
    reference: &CombiTimeTable,
    real: &[FunnelPoint],
    sequence: &str,
) {
    let columns = reference.col_names.as_ref().expect("reference columns");
    let times: Vec<f64> = (0..reference.n_rows)
        .map(|row| reference.data[row * reference.n_cols])
        .collect();
    let varying = real.iter().find_map(|point| {
        let idx = columns
            .iter()
            .position(|name| name == point.reference_name)?;
        let column: Vec<f64> = (0..reference.n_rows)
            .map(|row| reference.data[row * reference.n_cols + idx])
            .collect();
        let min = column.iter().copied().fold(f64::INFINITY, f64::min);
        let max = column.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (max > min).then_some(column)
    });
    let varying = varying.unwrap_or_else(|| {
        panic!("{sequence}: no varying Real output to exercise the falsifiable band")
    });
    assert_real_algebraic_band_is_falsifiable(&times, &varying);
}

fn point_end(name: &str, kind: ValueKind, kind_to_str: fn(ValueKind) -> &'static str) -> PointEnd {
    PointEnd {
        name: name.to_string(),
        unit: None,
        kind: Some(kind_to_str(kind).to_string()),
    }
}

fn assert_recorded_real_algebraic_band(sequence: &str, signal: &str, tolerance: &Tolerances) {
    assert_eq!(
        tolerance.rtoly.to_bits(),
        REAL_ALGEBRAIC_LAST_ULP.to_bits(),
        "{sequence} {signal} recorded rtoly"
    );
    for (label, value) in [
        ("atolx", tolerance.atolx),
        ("atoly", tolerance.atoly),
        ("rtolx", tolerance.rtolx),
        ("ltolx", tolerance.ltolx),
        ("ltoly", tolerance.ltoly),
    ] {
        assert_eq!(
            value.to_bits(),
            0.0f64.to_bits(),
            "{sequence} {signal} {label} must stay zero"
        );
    }
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
