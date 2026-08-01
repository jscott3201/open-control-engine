//! Clean-room derivation records and discrepancy detection.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;

const FORMAT: &str = "oce-clean-room-derivation-v1";
const NAND_CLASS: &str = "CDL.Logical.Nand";
const NAND_SCENARIO: &str = "all_boolean_input_pairs";
const NAND_SOURCE: &str =
    "third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Logical/Nand.mo";
const NAND_EQUATION: &str = "y = not (u1 and u2)";

/// Vendored source citation used by a derivation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DerivationSource {
    /// Repository-relative source path.
    pub file: String,
    /// One-based source line.
    pub line: usize,
    /// Equation transcribed from the cited line.
    pub equation: String,
}

/// Human roles that authored and independently reviewed the derivation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DerivationRoles {
    /// Identity of the clean-room derivation author.
    pub derivation_author: String,
    /// Completed independent-review attestation, including reviewer and date.
    pub independent_reviewer: String,
}

/// Information-flow assertions plus the externally reviewable evidence supporting them.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InformationSeparation {
    /// Author attestation that engine implementation was not read before the freeze.
    pub engine_implementation_read: bool,
    /// Author attestation that the golden generator was not read before the freeze.
    pub golden_generator_read: bool,
    /// Author attestation that expected outputs were not read before the freeze.
    pub existing_expected_output_read: bool,
    /// Whether the derivation and engine share a math kernel.
    pub shared_math_kernel: bool,
    /// Explicit limitation of in-repository validation of historical reading.
    pub limitation: String,
    /// Frozen commit and external read-log review evidence.
    pub external_evidence: Vec<String>,
}

/// Exact comparison regime declared by the derivation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DerivationComparison {
    /// CDL value type compared.
    pub value_type: String,
    /// Comparison method.
    pub method: String,
    /// Absolute output tolerance, zero for Boolean values.
    pub atoly: i64,
    /// Rounding declaration.
    pub rounding: String,
}

/// One Boolean sample frozen before the compared implementations are inspected.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct BooleanDerivation {
    /// Format identifier used to reject unrelated JSON records.
    pub format: String,
    /// Canonical CDL class path.
    pub class: String,
    /// Scenario identity within the class.
    pub scenario: String,
    /// Vendored equation citation.
    pub source: DerivationSource,
    /// Author and completed reviewer attestations.
    pub roles: DerivationRoles,
    /// Information-flow assertions and their external evidence.
    pub information_separation: InformationSeparation,
    /// Declared exact comparison regime.
    pub comparison: DerivationComparison,
    /// Reviewer-visible Boolean-algebra working.
    pub derivation: Vec<String>,
    /// Frozen rows, in sample order.
    pub rows: Vec<BooleanDerivationRow>,
}

/// One complete row parsed from the Tier-A reference table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BooleanReferenceRow {
    /// Sample time retained from Tier A.
    pub time: f64,
    /// First Boolean input retained from Tier A.
    pub u1: bool,
    /// Second Boolean input retained from Tier A.
    pub u2: bool,
    /// Tier-A output.
    pub y: bool,
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
    /// Validated zero-based sample index.
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
            "{}: discrepancy detected; Tier-A adjudication required: class={}, scenario={}, sample={}, inputs=Boolean({:?}), derived=Boolean({}), party={:?}, observed=Boolean({}); derivation={}:15; tier_a=tools/golden-gen/goldens/CDL/Logical/Nand/y.prov.json; regime=exact Boolean",
            self.id,
            self.class,
            self.scenario,
            self.sample,
            self.inputs,
            self.derived,
            self.party,
            self.observed,
            NAND_SOURCE
        )
    }
}

/// Validate all internally checkable claims in a frozen Nand derivation.
///
/// Historical non-reading remains an external audit property; this function validates that the
/// attestation is explicit and points to the frozen commit and audited read log.
///
/// # Errors
/// Returns the first schema-semantic, attestation, scenario, or Boolean-working inconsistency.
pub fn validate_boolean_derivation(derivation: &BooleanDerivation) -> Result<(), String> {
    if derivation.format != FORMAT {
        return Err(format!(
            "unsupported derivation format: {}",
            derivation.format
        ));
    }
    if derivation.class != NAND_CLASS || derivation.scenario != NAND_SCENARIO {
        return Err("derivation class or scenario is not the bounded Nand audit".into());
    }
    if derivation.source.file != NAND_SOURCE
        || derivation.source.line != 15
        || derivation.source.equation != NAND_EQUATION
    {
        return Err("source citation does not identify the vendored Nand equation".into());
    }
    if derivation.roles.derivation_author.trim().is_empty()
        || !derivation.roles.independent_reviewer.contains("completed")
        || !derivation.roles.independent_reviewer.contains("2026-08-01")
    {
        return Err("author and completed independent-review attestations are required".into());
    }
    let separation = &derivation.information_separation;
    if separation.engine_implementation_read
        || separation.golden_generator_read
        || separation.existing_expected_output_read
        || separation.shared_math_kernel
    {
        return Err("information-separation attestation contradicts clean-room eligibility".into());
    }
    if !separation
        .limitation
        .contains("cannot prove historical non-reading")
        || !separation
            .external_evidence
            .iter()
            .any(|item| item.contains("f234758"))
        || !separation
            .external_evidence
            .iter()
            .any(|item| item.contains("pre-freeze read log"))
    {
        return Err("external information-separation evidence or limitation is incomplete".into());
    }
    if derivation.comparison.value_type != "Boolean"
        || derivation.comparison.method != "exact"
        || derivation.comparison.atoly != 0
        || derivation.comparison.rounding != "none; Boolean operations are exact"
    {
        return Err("comparison regime is not exact Boolean with no rounding".into());
    }
    if derivation.derivation.len() != 6
        || derivation
            .derivation
            .iter()
            .any(|step| step.trim().is_empty())
    {
        return Err("Boolean-algebra derivation must contain six non-empty working steps".into());
    }

    let mut pairs = BTreeSet::new();
    for (index, row) in derivation.rows.iter().enumerate() {
        if row.sample != index {
            return Err(format!("sample identity must be contiguous at row {index}"));
        }
        if !pairs.insert((row.u1, row.u2)) {
            return Err(format!("duplicate Boolean input pair at sample {index}"));
        }
        if row.and != (row.u1 && row.u2) {
            return Err(format!(
                "recorded conjunction is inconsistent at sample {index}"
            ));
        }
        if row.y != !row.and {
            return Err(format!(
                "recorded negation is inconsistent at sample {index}"
            ));
        }
    }
    let exhaustive = [(false, false), (false, true), (true, false), (true, true)]
        .into_iter()
        .collect();
    if pairs != exhaustive {
        return Err("all_boolean_input_pairs is not exhaustive".into());
    }
    Ok(())
}

/// Validate that the source citation resolves to the recorded equation.
///
/// # Errors
/// Returns an error if the cited one-based line is absent or does not contain the equation.
pub fn validate_derivation_source(
    derivation: &BooleanDerivation,
    source_text: &str,
) -> Result<(), String> {
    let line = source_text
        .lines()
        .nth(derivation.source.line.saturating_sub(1))
        .ok_or_else(|| "source citation line does not exist".to_string())?;
    let compact = line.split_whitespace().collect::<String>();
    if compact != "y=not(u1andu2);" {
        return Err("source citation line does not contain the recorded Nand equation".into());
    }
    Ok(())
}

/// Compare frozen Boolean rows with complete Tier-A rows and engine output without a verdict.
///
/// # Errors
/// Returns a descriptive error for invalid provenance, scenario identity, Tier-A row alignment,
/// non-increasing/non-finite time, or unequal sample counts.
pub fn compare_boolean_derivation(
    derivation: &BooleanDerivation,
    tier_a: &[BooleanReferenceRow],
    engine: &[bool],
) -> Result<Vec<AuditDiscrepancy>, String> {
    validate_boolean_derivation(derivation)?;
    let expected = derivation.rows.len();
    if tier_a.len() != expected || engine.len() != expected {
        return Err(format!(
            "sample count mismatch: derivation={expected}, tier_a={}, engine={}",
            tier_a.len(),
            engine.len()
        ));
    }

    let mut previous_time = None;
    let mut discrepancies = Vec::new();
    for (index, row) in derivation.rows.iter().enumerate() {
        let reference = tier_a[index];
        if !reference.time.is_finite()
            || previous_time.is_some_and(|previous| reference.time <= previous)
        {
            return Err(format!(
                "Tier-A time is not finite and increasing at sample {index}"
            ));
        }
        previous_time = Some(reference.time);
        if (reference.u1, reference.u2) != (row.u1, row.u2) {
            return Err(format!(
                "Tier-A inputs do not match derivation at sample {index}"
            ));
        }
        for (party, observed) in [
            (ComparedParty::TierA, reference.y),
            (ComparedParty::Engine, engine[index]),
        ] {
            if observed != row.y {
                discrepancies.push(AuditDiscrepancy {
                    id: format!(
                        "clean-room:{}:{}:{}",
                        derivation.class, derivation.scenario, row.sample
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
        serde_json::from_str(include_str!(
            "../tests/fixtures/clean_room/logical_nand.derivation.json"
        ))
        .unwrap()
    }

    fn reference() -> Vec<BooleanReferenceRow> {
        derivation()
            .rows
            .iter()
            .map(|row| BooleanReferenceRow {
                time: row.sample as f64 * 60.0,
                u1: row.u1,
                u2: row.u2,
                y: row.y,
            })
            .collect()
    }

    #[test]
    fn mismatching_party_opens_adjudication() {
        let mut tier_a = reference();
        tier_a[0].y = false;
        let engine = derivation()
            .rows
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>();
        let discrepancies = compare_boolean_derivation(&derivation(), &tier_a, &engine).unwrap();
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
        let engine = derivation()
            .rows
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>();
        assert!(
            compare_boolean_derivation(&derivation(), &reference(), &engine)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn input_substitution_is_rejected_by_exhaustive_pair_assertion() {
        let mut record = derivation();
        record.rows[0].u1 = true;
        let error = validate_boolean_derivation(&record).unwrap_err();
        assert!(error.contains("duplicate Boolean input pair"));
    }

    #[test]
    fn missing_boolean_pair_is_rejected_by_sample_count_assertion() {
        let mut record = derivation();
        record.rows.pop();
        let error = validate_boolean_derivation(&record).unwrap_err();
        assert!(error.contains("not exhaustive"));
    }

    #[test]
    fn corrupted_sample_index_is_rejected_by_contiguous_identity_assertion() {
        let mut record = derivation();
        record.rows[1].sample = 7;
        let error = validate_boolean_derivation(&record).unwrap_err();
        assert!(error.contains("sample identity must be contiguous"));
    }

    #[test]
    fn corrupted_intermediate_is_rejected_by_conjunction_assertion() {
        let mut record = derivation();
        record.rows[0].and = true;
        let error = validate_boolean_derivation(&record).unwrap_err();
        assert!(error.contains("recorded conjunction is inconsistent"));
    }

    #[test]
    fn tier_a_input_corruption_is_rejected_by_row_alignment_assertion() {
        let mut tier_a = reference();
        tier_a[0].u1 = true;
        let engine = derivation()
            .rows
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>();
        let error = compare_boolean_derivation(&derivation(), &tier_a, &engine).unwrap_err();
        assert!(error.contains("Tier-A inputs do not match derivation"));
    }

    #[test]
    fn provenance_corruption_is_rejected_by_separation_assertion() {
        let mut record = derivation();
        record.information_separation.engine_implementation_read = true;
        let error = validate_boolean_derivation(&record).unwrap_err();
        assert!(error.contains("information-separation attestation contradicts"));
    }

    #[test]
    fn unknown_provenance_field_is_rejected_by_strict_schema() {
        let mutated = include_str!("../tests/fixtures/clean_room/logical_nand.derivation.json")
            .replacen("\"format\":", "\"unknown\": true, \"format\":", 1);
        let error = serde_json::from_str::<BooleanDerivation>(&mutated).unwrap_err();
        assert!(error.to_string().contains("unknown field `unknown`"));
    }
}
