//! Tiered conformance report model.
//!
//! Tiers 0 through 4 describe the ladder from static parse/validate checks to full sequence
//! comparison. The report records each tier's outcome without owning the future driver/oracle logic.

use oce_diag::{Diagnostic, Severity};

/// The conformance tier being reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConformanceTier {
    /// Tier 0: static parse/validate/build checks, no execution.
    Tier0,
    /// Tier 1: per-block comparison against the reference oracle.
    Tier1,
    /// Tier 2: bit-exact regression locking against a blessed prior-self trace.
    Tier2,
    /// Tier 3: cross-implementation differential testing.
    Tier3,
    /// Tier 4: full sequence-trace comparison.
    Tier4,
}

impl ConformanceTier {
    /// Stable display label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ConformanceTier::Tier0 => "tier-0",
            ConformanceTier::Tier1 => "tier-1",
            ConformanceTier::Tier2 => "tier-2",
            ConformanceTier::Tier3 => "tier-3",
            ConformanceTier::Tier4 => "tier-4",
        }
    }
}

/// Outcome status for one conformance tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TierStatus {
    /// The tier ran and has no diagnostics.
    Passed,
    /// The tier ran and produced only advisory diagnostics.
    Advisory,
    /// The tier ran and produced at least one hard failure.
    Failed,
    /// The tier is intentionally not executed in this harness yet.
    Skipped,
}

impl TierStatus {
    /// Whether this status fails the report.
    #[must_use]
    pub fn is_failure(self) -> bool {
        matches!(self, TierStatus::Failed)
    }
}

/// Report for one conformance tier.
#[derive(Clone, Debug, PartialEq)]
pub struct TierReport {
    /// Tier being reported.
    pub tier: ConformanceTier,
    /// Outcome status for the tier.
    pub status: TierStatus,
    /// Human-readable tier summary.
    pub summary: String,
    /// Diagnostics surfaced by the tier.
    pub diagnostics: Vec<Diagnostic>,
}

impl TierReport {
    /// Build a passed tier report.
    #[must_use]
    pub fn passed(tier: ConformanceTier, summary: impl Into<String>) -> Self {
        Self {
            tier,
            status: TierStatus::Passed,
            summary: summary.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Build a skipped tier report.
    ///
    /// This is used for Tier 3 until the cross-implementation toolchain is wired in.
    #[must_use]
    pub fn skipped(tier: ConformanceTier, summary: impl Into<String>) -> Self {
        Self {
            tier,
            status: TierStatus::Skipped,
            summary: summary.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Classify a tier from diagnostics.
    ///
    /// A [`Severity::Error`] diagnostic is a shall-level hard failure. Warning and info diagnostics
    /// are should-level/advisory signals and do not fail the tier.
    #[must_use]
    pub fn from_diagnostics(
        tier: ConformanceTier,
        summary: impl Into<String>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        let status = classify_diagnostics(&diagnostics);
        Self {
            tier,
            status,
            summary: summary.into(),
            diagnostics,
        }
    }

    /// Classify a Tier 0 static report from validate/build diagnostics.
    #[must_use]
    pub fn tier0(summary: impl Into<String>, diagnostics: Vec<Diagnostic>) -> Self {
        Self::from_diagnostics(ConformanceTier::Tier0, summary, diagnostics)
    }

    /// Whether this tier report fails.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.status.is_failure()
    }
}

/// Complete Tier 0-4 conformance report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConformanceReport {
    /// Tier reports, typically ordered Tier 0 through Tier 4.
    pub tiers: Vec<TierReport>,
}

impl ConformanceReport {
    /// Construct a report from tier records.
    #[must_use]
    pub fn new(tiers: Vec<TierReport>) -> Self {
        Self { tiers }
    }

    /// Append one tier report.
    pub fn push(&mut self, tier: TierReport) {
        self.tiers.push(tier);
    }

    /// Return the report for a specific tier, when present.
    #[must_use]
    pub fn tier(&self, tier: ConformanceTier) -> Option<&TierReport> {
        self.tiers.iter().find(|report| report.tier == tier)
    }

    /// True when no tier failed with a shall-level diagnostic or mismatch.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.tiers.iter().all(|tier| !tier.failed())
    }
}

fn classify_diagnostics(diagnostics: &[Diagnostic]) -> TierStatus {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        TierStatus::Failed
    } else if diagnostics.is_empty() {
        TierStatus::Passed
    } else {
        TierStatus::Advisory
    }
}
