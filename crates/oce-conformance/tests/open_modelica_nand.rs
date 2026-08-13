//! Scoped OpenModelica differential for `CDL.Logical.Nand`.

mod block_harness;
#[path = "open_modelica_nand/support.rs"]
mod scoped;

use oce_conformance::{AuditDiscrepancy, CombiTimeTable, ComparedParty};

#[test]
fn external_nand_matches_facade_exactly_at_every_observation() {
    let reference = scoped::read_reference("nand.canonical.csv").unwrap();
    let outcome = scoped::evaluate(&reference);
    assert_eq!(outcome.producer, "OpenModelica 1.25.1");
    assert_eq!(outcome.class, "CDL.Logical.Nand");
    assert_eq!(outcome.scenario, "all_boolean_input_pairs_evented");
    assert!(outcome.manifest.ends_with("logical_nand/manifest.json"));
    assert!(
        outcome.exact_match,
        "{}",
        mismatch_diagnostic(
            &reference,
            "nand.canonical.csv",
            outcome.first_mismatch.unwrap()
        )
    );
    assert_eq!(outcome.compared_points, 8);
    assert_eq!(outcome.first_mismatch, None);
    assert_eq!(outcome.load_warning_count, 0);
    let analytical = scoped::analytical_discrepancies(&reference, &outcome.engine).unwrap();
    assert!(
        analytical.is_empty(),
        "{}",
        analytical_mismatch_diagnostic(&reference, &analytical[0])
    );
}

fn analytical_mismatch_diagnostic(
    reference: &CombiTimeTable,
    mismatch: &AuditDiscrepancy,
) -> String {
    let row = reference
        .data
        .chunks_exact(reference.n_cols)
        .nth(mismatch.sample)
        .expect("analytical mismatch sample occurs in the external reference");
    let party = match mismatch.party {
        ComparedParty::TierA => "OpenModelica",
        ComparedParty::Engine => "engine",
    };
    format!(
        "discrepancy detected; adjudication required: class=CDL.Logical.Nand, scenario=all_boolean_input_pairs_evented, pair=({},{}) observation_time={} expected=Boolean({}) observed=Boolean({}) party={party} reference=crates/oce-conformance/tests/fixtures/open_modelica/logical_nand/nand.canonical.csv manifest=crates/oce-conformance/tests/fixtures/open_modelica/logical_nand/manifest.json regime=exact Boolean",
        mismatch.inputs.0, mismatch.inputs.1, row[0], mismatch.derived, mismatch.observed,
    )
}

#[test]
fn buildings_and_substitution_is_detected_as_an_exact_mismatch() {
    let reference = scoped::read_reference("and.canonical.csv").unwrap();
    let outcome = scoped::evaluate(&reference);
    assert!(!outcome.exact_match);
    assert!(outcome.first_mismatch.is_some());
    let mismatch = outcome.first_mismatch.unwrap();
    let row = reference
        .data
        .chunks_exact(reference.n_cols)
        .nth(mismatch.index)
        .unwrap();
    let diagnostic = mismatch_diagnostic(&reference, "and.canonical.csv", mismatch);
    assert_eq!(row[0].to_bits(), mismatch.time_bits);
    assert!(diagnostic.contains(&format!("expected=Boolean({})", mismatch.expected)));
    assert!(diagnostic.contains(&format!("observed=Boolean({})", mismatch.observed)));
    assert!(diagnostic.contains("adjudication required"));
    assert!(!diagnostic.contains("engine correct"));
    assert!(!diagnostic.contains("OpenModelica correct"));
}

fn mismatch_diagnostic(
    reference: &CombiTimeTable,
    artifact: &str,
    mismatch: scoped::ScopedMismatch,
) -> String {
    let row = reference
        .data
        .chunks_exact(reference.n_cols)
        .nth(mismatch.index)
        .expect("mismatch row");
    format!(
        "discrepancy detected; adjudication required: class=CDL.Logical.Nand, scenario=all_boolean_input_pairs_evented, pair=({},{}) observation_time={} expected=Boolean({}) observed=Boolean({}) index={} reference=crates/oce-conformance/tests/fixtures/open_modelica/logical_nand/{artifact} manifest=crates/oce-conformance/tests/fixtures/open_modelica/logical_nand/manifest.json regime=exact Boolean",
        row[1] == 1.0,
        row[2] == 1.0,
        f64::from_bits(mismatch.time_bits),
        mismatch.expected,
        mismatch.observed,
        mismatch.index,
    )
}

#[test]
fn scoped_evaluation_and_diagnostics_are_deterministic() {
    let nand = scoped::read_reference("nand.canonical.csv").unwrap();
    let and = scoped::read_reference("and.canonical.csv").unwrap();
    assert_eq!(scoped::evaluate(&nand), scoped::evaluate(&nand));
    assert_eq!(scoped::evaluate(&and), scoped::evaluate(&and));
}

#[test]
fn analytical_check_covers_repeated_input_pair_observations() {
    let mut reference = scoped::read_reference("nand.canonical.csv").unwrap();
    let mut engine = scoped::evaluate(&reference).engine;
    let y = reference
        .col_names
        .as_ref()
        .unwrap()
        .iter()
        .position(|name| name == "y")
        .unwrap();
    reference.data[reference.n_cols + y] = 0.0;
    engine[1] = false;
    let discrepancies = scoped::analytical_discrepancies(&reference, &engine).unwrap();
    assert!(
        discrepancies
            .iter()
            .any(|mismatch| mismatch.sample == 1 && mismatch.party == ComparedParty::TierA)
    );
}

#[test]
fn canonical_reference_size_is_bounded_before_parse() {
    let at_limit = vec![b' '; scoped::MAX_REFERENCE_BYTES];
    assert!(
        !scoped::parse_reference_bytes(&at_limit)
            .unwrap_err()
            .contains("exceeds")
    );
    assert!(
        scoped::parse_reference_bytes(&[at_limit, vec![b' ']].concat())
            .unwrap_err()
            .contains("exceeds")
    );
}
