//! Clean-room derivation records and discrepancy detection.

use std::fmt;

use serde::Deserialize;

/// One Boolean sample frozen before the compared implementations are inspected.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub struct BooleanDerivationRow {
    /// Zero-based sample index.
    pub sample: usize,
    /// First Boolean input.
    pub u1: bool,
    /// Second Boolean input.
    pub u2: bool,
    /// Recorded intermediate conjunction.
    pub and: bool,
    /// Analytically derived output.
    pub y: bool,
}

/// Reviewer-visible provenance for an exact Boolean derivation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct BooleanDerivation {
    /// Format identifier used to reject unrelated JSON records.
    pub format: String,
    /// Canonical CDL class path.
    pub class: String,
    /// Scenario identity within the class.
    pub scenario: String,
    /// Frozen rows, in sample order.
    pub rows: Vec<BooleanDerivationRow>,
}

/// The party whose value differs at a sampled row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComparedParty {
    /// Checked-in Tier-A expected output.
    TierA,
    /// Output captured from the engine.
    Engine,
}

/// A disagreement detected against a frozen analytical derivation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditDiscrepancy {
    /// Stable identifier suitable for an adjudication record.
    pub id: String,
    /// Canonical CDL class path.
    pub class: String,
    /// Scenario identity.
    pub scenario: String,
    /// Zero-based sample index.
    pub sample: usize,
    /// Input pair at the discrepant sample.
    pub inputs: (bool, bool),
    /// Value frozen by the clean-room derivation.
    pub derived: bool,
    /// Party that disagreed with the derivation.
    pub party: ComparedParty,
    /// Disagreeing value.
    pub observed: bool,
}

impl fmt::Display for AuditDiscrepancy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: discrepancy detected; Tier-A adjudication required: class={}, scenario={}, sample={}, inputs=Boolean({:?}), derived=Boolean({}), party={:?}, observed=Boolean({}); derivation=third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Logical/Nand.mo:15; tier_a=tools/golden-gen/goldens/CDL/Logical/Nand/y.prov.json; regime=exact Boolean",
            self.id,
            self.class,
            self.scenario,
            self.sample,
            self.inputs,
            self.derived,
            self.party,
            self.observed
        )
    }
}

/// Compare frozen Boolean rows with Tier-A and engine output without assigning a verdict.
///
/// Missing or extra samples are reported as an error because their inputs cannot be safely
/// inferred. Boolean comparison is exact and performs no rounding.
///
/// # Errors
/// Returns a descriptive error for an unsupported record format or unequal sample counts.
pub fn compare_boolean_derivation(
    derivation: &BooleanDerivation,
    tier_a: &[bool],
    engine: &[bool],
) -> Result<Vec<AuditDiscrepancy>, String> {
    if derivation.format != "oce-clean-room-derivation-v1" {
        return Err(format!(
            "unsupported derivation format: {}",
            derivation.format
        ));
    }
    let expected = derivation.rows.len();
    if tier_a.len() != expected || engine.len() != expected {
        return Err(format!(
            "sample count mismatch: derivation={expected}, tier_a={}, engine={}",
            tier_a.len(),
            engine.len()
        ));
    }

    let mut discrepancies = Vec::new();
    for (index, row) in derivation.rows.iter().enumerate() {
        for (party, observed) in [
            (ComparedParty::TierA, tier_a[index]),
            (ComparedParty::Engine, engine[index]),
        ] {
            if observed != row.y {
                discrepancies.push(AuditDiscrepancy {
                    id: format!(
                        "clean-room:{}:{}:{index}",
                        derivation.class, derivation.scenario
                    ),
                    class: derivation.class.clone(),
                    scenario: derivation.scenario.clone(),
                    sample: row.sample,
                    inputs: (row.u1, row.u2),
                    derived: row.y,
                    party,
                    observed,
                });
            }
        }
    }
    Ok(discrepancies)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derivation() -> BooleanDerivation {
        BooleanDerivation {
            format: "oce-clean-room-derivation-v1".into(),
            class: "CDL.Logical.Nand".into(),
            scenario: "all_boolean_input_pairs".into(),
            rows: vec![BooleanDerivationRow {
                sample: 0,
                u1: true,
                u2: true,
                and: true,
                y: false,
            }],
        }
    }

    #[test]
    fn mismatching_party_opens_adjudication() {
        let discrepancies = compare_boolean_derivation(&derivation(), &[true], &[false]).unwrap();
        assert_eq!(discrepancies.len(), 1);
        assert_eq!(discrepancies[0].party, ComparedParty::TierA);
        assert!(
            discrepancies[0]
                .to_string()
                .contains("Tier-A adjudication required")
        );
    }

    #[test]
    fn matching_parties_produce_no_discrepancy() {
        assert!(
            compare_boolean_derivation(&derivation(), &[false], &[false])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn unequal_sample_counts_cannot_pass_vacuously() {
        let error = compare_boolean_derivation(&derivation(), &[], &[false]).unwrap_err();
        assert!(error.contains("sample count mismatch"));
    }
}
