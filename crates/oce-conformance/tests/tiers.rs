//! Tier report and shall/should classification coverage.

use oce_conformance::{ConformanceReport, ConformanceTier, TierReport, TierStatus};
use oce_diag::{DiagCode, Diagnostic, Severity};

#[test]
fn tier0_shall_error_is_a_hard_failure() {
    let report = TierReport::tier0(
        "static validation",
        vec![Diagnostic::error(
            DiagCode::SingleAssignment,
            "input has two drivers",
        )],
    );
    assert_eq!(report.status, TierStatus::Failed);
    assert!(report.failed());

    let full = ConformanceReport::new(vec![report]);
    assert!(!full.passed());
}

#[test]
fn tier0_should_warning_is_advisory_not_failure() {
    let report = TierReport::tier0(
        "static validation",
        vec![Diagnostic::warning(
            DiagCode::DisplayUnitDivergence,
            "displayUnit differs",
        )],
    );
    assert_eq!(report.status, TierStatus::Advisory);
    assert!(!report.failed());

    let full = ConformanceReport::new(vec![report]);
    assert!(full.passed());
}

#[test]
fn tier0_info_is_advisory_not_failure() {
    let report = TierReport::tier0(
        "static validation",
        vec![Diagnostic::info(
            DiagCode::AnalogCoercedToReal,
            "analog connector was normalized",
        )],
    );
    assert_eq!(report.status, TierStatus::Advisory);
    assert!(!report.failed());
}

#[test]
fn tier0_error_dominates_mixed_diagnostics_even_when_last() {
    let report = TierReport::tier0(
        "static validation",
        vec![
            Diagnostic::warning(DiagCode::DisplayUnitDivergence, "displayUnit differs"),
            Diagnostic::info(DiagCode::AnalogCoercedToReal, "analog connector normalized"),
            Diagnostic::new(
                Severity::Error,
                DiagCode::SingleAssignment,
                "input has two drivers",
            ),
        ],
    );
    assert_eq!(report.status, TierStatus::Failed);
    assert!(report.failed());
}

#[test]
fn tier_report_can_record_zero_comparison_no_data() {
    let report = TierReport::no_data(
        ConformanceTier::Tier4,
        "masked comparison checked no points",
    );
    assert_eq!(report.status, TierStatus::NoData);
    assert!(!report.failed());
    assert!(ConformanceReport::new(vec![report]).passed());
}

#[test]
fn report_shape_captures_tiers_zero_through_four() {
    let report = ConformanceReport::new(vec![
        TierReport::passed(ConformanceTier::Tier0, "parse/validate clean"),
        TierReport::passed(ConformanceTier::Tier1, "per-block oracle clean"),
        TierReport::passed(ConformanceTier::Tier2, "bit-exact self regression clean"),
        TierReport::skipped(
            ConformanceTier::Tier3,
            "cross-implementation differential testing deferred",
        ),
        TierReport::passed(ConformanceTier::Tier4, "full sequence funnel clean"),
    ]);
    assert!(report.passed());
    assert_eq!(
        report.tier(ConformanceTier::Tier0).map(|tier| tier.status),
        Some(TierStatus::Passed)
    );
    assert_eq!(
        report.tier(ConformanceTier::Tier3).map(|tier| tier.status),
        Some(TierStatus::Skipped)
    );
    assert_eq!(
        report
            .tier(ConformanceTier::Tier4)
            .map(|tier| tier.summary.as_str()),
        Some("full sequence funnel clean")
    );
}

#[test]
fn tier_labels_are_stable() {
    assert_eq!(ConformanceTier::Tier0.as_str(), "tier-0");
    assert_eq!(ConformanceTier::Tier1.as_str(), "tier-1");
    assert_eq!(ConformanceTier::Tier2.as_str(), "tier-2");
    assert_eq!(ConformanceTier::Tier3.as_str(), "tier-3");
    assert_eq!(ConformanceTier::Tier4.as_str(), "tier-4");
}
