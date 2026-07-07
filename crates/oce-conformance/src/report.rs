//! Assemble a populated Tier 0–4 [`ConformanceReport`] from a real driven sequence run.
//!
//! [`tiers`](crate::tiers) owns the report *model* and deliberately stays driver/oracle-free; the
//! [`driver`](crate::driver) owns *execution* and returns a [`DriverRun`]. This module is the bridge
//! that imports both: it drives one real sequence, maps the outcomes onto the five conformance tiers,
//! and returns the ordered report. It is intentionally a **single-sequence** bridge — a whole-suite
//! roll-up (aggregating the per-block corpus and every determinism suite) is a separate concern.
//!
//! Tier mapping (`_spec/07 §7`, matching the [`ConformanceTier`] doc comments):
//! - **Tier 0** — static parse/validate/build. Populated from the load outcome: the `should`-level
//!   [`DriverRun::load_warnings`] on success, or the load `Err` on a `shall`-level failure.
//! - **Tier 1** — per-block "same response" vs the Buildings library. `Skipped`: that is the
//!   per-block corpus, not a full-sequence run.
//! - **Tier 2** — bit-exact regression vs a blessed prior-self trace. Populated by a separate
//!   `Exact`-mode run whose reference is derived from the committed self-output golden; the Exact
//!   comparisons passing *is* the bit-exact regression check. `Skipped` when not wired.
//! - **Tier 3** — cross-implementation differential. `Skipped`: the external toolchain is deferred
//!   (`_spec/07 §10.1`).
//! - **Tier 4** — full-sequence trace vs the Tier-A reference oracle, through the L1 funnel band.
//!
//! The assembler always emits all five tiers in order, because an empty [`ConformanceReport`] passes
//! vacuously ([`ConformanceReport::passed`] is `all(!failed)`).

use crate::{
    CombiTimeTable, ComparisonMode, ConformanceReport, ConformanceTier, DriverError, DriverOptions,
    DriverRun, SignalComparison, TierReport, VerifyConfig, drive_trace_with_options,
};

/// The inputs needed to drive one real sequence run: the tolerance/mapping config, the reference
/// table to compare against, and the cadence/replay/comparison-mode options.
pub struct RunInputs<'a> {
    /// Verification config (point mapping + per-output tolerances) for this run.
    pub config: &'a VerifyConfig,
    /// Reference table the run compares against.
    pub reference: &'a CombiTimeTable,
    /// Driver cadence / input-replay / comparison-mode options.
    pub options: &'a DriverOptions,
}

/// How Tier 2 (bit-exact regression vs a blessed prior-self trace) is populated for this report.
pub enum Tier2Source<'a> {
    /// Drive a separate `Exact`-mode run whose `reference` is derived from the committed self-output
    /// golden. Because the reference *is* the blessed golden, the run's Exact comparisons passing is
    /// exactly the bit-exact regression check — no separate table compare (and its column-naming
    /// pitfalls) is needed.
    Golden(RunInputs<'a>),
    /// Tier 2 is intentionally not wired into this report (reported as `Skipped`, does not fail it).
    Skipped,
}

/// One real sequence run to assemble into a Tier 0–4 [`ConformanceReport`].
pub struct ConformanceRun<'a> {
    /// CXF bytes: loaded once (Tier 0) and driven for Tier 4.
    pub cxf: &'a [u8],
    /// Tier 4 inputs: full-sequence funnel comparison vs the Tier-A reference oracle.
    pub tier4: RunInputs<'a>,
    /// Tier 2 source: bit-exact regression vs the blessed self-output golden, or `Skipped`.
    pub tier2: Tier2Source<'a>,
}

/// Drive `run` and assemble its ordered Tier 0–4 [`ConformanceReport`].
///
/// This never turns a harness/configuration fault into a conformance verdict: only a `shall`-level
/// load failure ([`DriverError::Engine`]) becomes a failed Tier 0 (with Tiers 1–4 `Skipped`, since
/// nothing executed). Every other driver error (bad tolerance config, malformed reference CSV, no
/// mapped outputs, an unmasked zero-compare) is a caller/harness bug and is returned as `Err` — it
/// is not a statement about the model's conformance.
///
/// # Errors
/// Returns any non-load [`DriverError`] from driving the Tier 4 run or the Tier 2 regression run.
pub fn assemble_report(run: &ConformanceRun<'_>) -> Result<ConformanceReport, DriverError> {
    let mut tiers = Vec::with_capacity(5);
    match drive_trace_with_options(
        run.cxf,
        run.tier4.config,
        run.tier4.reference,
        run.tier4.options,
    ) {
        Ok(driver_run) => {
            tiers.push(tier0_passed(&driver_run));
            tiers.push(tier1_skipped());
            tiers.push(tier2_report(run.cxf, &run.tier2)?);
            tiers.push(tier3_skipped());
            tiers.push(classify_comparisons(
                ConformanceTier::Tier4,
                &driver_run.comparisons,
                "full-sequence funnel comparison vs the Tier-A reference oracle",
            ));
        }
        Err(DriverError::Engine(err)) => {
            tiers.push(tier0_load_failed(&err));
            tiers.push(tier1_skipped());
            tiers.push(TierReport::skipped(
                ConformanceTier::Tier2,
                "not executed: the model failed to load",
            ));
            tiers.push(tier3_skipped());
            tiers.push(TierReport::skipped(
                ConformanceTier::Tier4,
                "not executed: the model failed to load",
            ));
        }
        Err(other) => return Err(other),
    }
    Ok(ConformanceReport::new(tiers))
}

/// Tier 0 from a successful load: `should`-level warnings classify to `Passed` (none) or `Advisory`.
fn tier0_passed(run: &DriverRun) -> TierReport {
    TierReport::tier0("static parse/validate/build", run.load_warnings.clone())
}

/// Tier 0 from a `shall`-level load failure. A validation error already carries structured
/// diagnostics; every other load error (CXF/flatten/build/load) carries only a message, so — per the
/// summary-and-status reporting choice — it becomes a `Failed` tier with the detail in the summary
/// and no synthesized diagnostic code.
fn tier0_load_failed(err: &oce_api::OcError) -> TierReport {
    match err {
        oce_api::OcError::Validate(validation) => TierReport::from_diagnostics(
            ConformanceTier::Tier0,
            "static validate",
            validation.diagnostics.clone(),
        ),
        other => TierReport::failure(
            ConformanceTier::Tier0,
            format!("static parse/validate/build failed: {other}"),
        ),
    }
}

fn tier1_skipped() -> TierReport {
    TierReport::skipped(
        ConformanceTier::Tier1,
        "per-block Buildings-oracle comparison is the per-block corpus, not a full-sequence run",
    )
}

fn tier3_skipped() -> TierReport {
    TierReport::skipped(
        ConformanceTier::Tier3,
        "cross-implementation differential toolchain is deferred (_spec/07 §10.1)",
    )
}

/// Tier 2 from the blessed-golden regression run, or `Skipped`.
fn tier2_report(cxf: &[u8], source: &Tier2Source<'_>) -> Result<TierReport, DriverError> {
    match source {
        Tier2Source::Golden(inputs) => {
            // Tier 2 is bit-exact regression by definition, so the assembler *owns* the comparison
            // mode: force `Exact` regardless of the caller's options, keeping the emitted "bit-exact
            // regression" verdict truthful by construction even if a caller passes a banded mode.
            let mut options = inputs.options.clone();
            options.comparison = ComparisonMode::Exact;
            let run = drive_trace_with_options(cxf, inputs.config, inputs.reference, &options)?;
            Ok(classify_comparisons(
                ConformanceTier::Tier2,
                &run.comparisons,
                "bit-exact regression vs the blessed self-output golden",
            ))
        }
        Tier2Source::Skipped => Ok(TierReport::skipped(
            ConformanceTier::Tier2,
            "bit-exact regression golden not wired into this report",
        )),
    }
}

/// Classify a tier from a run's comparisons, precedence any-mismatch ▸ any-zero-compared ▸ pass.
/// Failing signals carry their detail in the summary (status-and-summary reporting), not synthesized
/// diagnostics. An empty comparison set is `NoData`, never a vacuous pass.
fn classify_comparisons(
    tier: ConformanceTier,
    comparisons: &[SignalComparison],
    context: &str,
) -> TierReport {
    if comparisons.is_empty() {
        return TierReport::no_data(tier, format!("{context}: no signals were compared"));
    }
    let total = comparisons.len();
    if let Some(first) = comparisons.iter().find(|c| !c.result.passed()) {
        let mismatched = comparisons.iter().filter(|c| !c.result.passed()).count();
        return TierReport::failure(
            tier,
            format!(
                "{context}: {mismatched} of {total} signals mismatched (first: {})",
                first.reference_column
            ),
        );
    }
    if let Some(first) = comparisons.iter().find(|c| c.result.compared_points() == 0) {
        let empty = comparisons
            .iter()
            .filter(|c| c.result.compared_points() == 0)
            .count();
        return TierReport::no_data(
            tier,
            format!(
                "{context}: {empty} of {total} signals compared zero points (first: {})",
                first.reference_column
            ),
        );
    }
    TierReport::passed(tier, format!("{context}: {total} signals within tolerance"))
}
