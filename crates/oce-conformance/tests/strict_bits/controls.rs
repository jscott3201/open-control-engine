//! Mutation controls through the facade driver, not comparator-only unit tests.

use super::evidence;
use crate::block_harness;
use oce_conformance::{CombiTimeTable, ComparisonResult};

fn assert_mutation(
    case: &block_harness::BlockCase,
    sequence: &str,
    reference: &CombiTimeTable,
    output: usize,
    column: usize,
    sample: usize,
    changed: f64,
) {
    let mut mutated = reference.clone();
    let expected = reference.data[sample * reference.n_cols + column];
    mutated.data[sample * reference.n_cols + column] = changed;
    let run = block_harness::drive_case_with_external_reference(case, sequence, &mutated);
    if cfg!(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )) {
        let selected = block_harness::drive_case_with_corpus_reference(case, sequence, &mutated);
        // The four existing suites must use the SAME strict result, not just this audit helper.
        let ComparisonResult::Exact(selected_result) = &selected.comparisons[output].result else {
            panic!("{}: per-signal suite policy lost strict wiring", case.slug);
        };
        assert!(!selected_result.passed);
        assert_eq!(selected_result.first_mismatch.unwrap().index, sample);
    }
    assert_eq!(run.comparisons.len(), case.outputs.len());
    for (i, comparison) in run.comparisons.iter().enumerate() {
        let ComparisonResult::Exact(result) = &comparison.result else {
            panic!("{}: strict facade wiring lost", case.slug);
        };
        assert_eq!(comparison.output, block_harness::output_point(case, i));
        assert!(!comparison.masked);
        assert_eq!(result.compared_points, reference.n_rows);
        if i == output {
            assert!(
                !result.passed,
                "{}.{} sample {sample}: mutation escaped",
                case.slug, case.outputs[i].name
            );
            let mismatch = result.first_mismatch.expect("exact mismatch");
            assert_eq!(mismatch.index, sample, "{}", comparison.output);
            assert_eq!(mismatch.expected.to_bits(), changed.to_bits());
            assert!(evidence::equal(
                &evidence::word(mismatch.actual),
                &evidence::word(expected)
            ));
        } else {
            assert!(result.passed, "unmodified output: {}", comparison.output);
        }
    }
}

#[test]
fn every_corpus_sample_uses_exact_facade_comparison_and_rejects_mutations() {
    let mut signals = 0;
    let mut finite = 0;
    let mut zero = 0;
    let mut nonfinite = 0;
    for (case, family, sequence) in evidence::cases() {
        let reference = block_harness::read_reference(case, family);
        // This strict audit runs on macOS too; it is an observation, not a policy promotion.
        block_harness::assert_cases_match_exact_oracle(&[*case], family, sequence);
        for (output, port) in case.outputs.iter().enumerate() {
            if port.kind != block_harness::SignalKind::Real {
                continue;
            }
            signals += 1;
            let column = reference
                .col_names
                .as_ref()
                .unwrap()
                .iter()
                .position(|name| name == port.name)
                .unwrap();
            for sample in 0..reference.n_rows {
                let value = reference.data[sample * reference.n_cols + column];
                if value.is_finite() {
                    finite += 1;
                    assert_mutation(
                        case,
                        sequence,
                        &reference,
                        output,
                        column,
                        sample,
                        value.next_up(),
                    );
                    if value == 0.0 {
                        zero += 1;
                        assert_mutation(case, sequence, &reference, output, column, sample, -value);
                    }
                } else {
                    nonfinite += 1;
                    let changed = if value.is_nan() {
                        f64::INFINITY
                    } else {
                        -value
                    };
                    assert_mutation(case, sequence, &reference, output, column, sample, changed);
                }
            }
        }
    }
    assert_eq!(signals, 21);
    assert_eq!((finite, zero, nonfinite), (101, 18, 60));
}

#[test]
fn manifest_refuses_pin_inventory_sample_and_provenance_mutations() {
    let retained = evidence::read_retained();
    let mut bad = retained.clone();
    bad.libm = "0.2.17".into();
    assert_eq!(
        evidence::validate(&bad).unwrap_err(),
        "version pin mismatch: explicit evidence refresh required"
    );
    let mut bad = retained.clone();
    bad.rustc = "rustc unqualified".into();
    assert_eq!(
        evidence::validate(&bad).unwrap_err(),
        "version pin mismatch: explicit evidence refresh required"
    );
    let mut bad = retained.clone();
    bad.signals.pop();
    assert_eq!(
        evidence::validate(&bad).unwrap_err(),
        "inventory coverage mismatch"
    );
    let mut bad = retained.clone();
    bad.signals[1] = bad.signals[0].clone();
    assert_eq!(
        evidence::validate(&bad).unwrap_err(),
        "duplicate or unordered signal"
    );
    let mut bad = retained.clone();
    bad.signals[0].actual.pop();
    assert!(
        evidence::validate(&bad)
            .unwrap_err()
            .ends_with(": sample coverage mismatch")
    );
    let mut bad = retained.clone();
    bad.signals[0].reference_sha256 = "changed".into();
    assert_eq!(
        evidence::same_contract(&retained, &bad).unwrap_err(),
        "corpus provenance/inventory changed: explicit refresh required"
    );
    let mut bad = retained.clone();
    bad.signals[0].actual[0] = evidence::word(0.0);
    assert!(
        evidence::validate(&bad)
            .unwrap_err()
            .ends_with(": hidden mismatch")
    );
}

#[test]
fn bit_labels_and_schema_are_closed_and_lossless() {
    for bits in [
        0,
        1,
        1_u64 << 63,
        f64::MAX.to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        0x7ff8_0000_0000_0123,
        0xfff8_0000_0000_0321,
    ] {
        assert_eq!(
            evidence::value(&evidence::word(f64::from_bits(bits)))
                .unwrap()
                .to_bits(),
            bits
        );
    }
    for invalid in [
        "0",
        "nan:0000000000000000",
        "finite:0",
        "finite:7ff0000000000000",
        "finite:xyz",
    ] {
        assert!(evidence::value(invalid).is_err(), "{invalid}");
    }
    let mut json = serde_json::to_value(evidence::read_retained()).unwrap();
    json["unexpected"] = true.into();
    assert!(serde_json::from_value::<evidence::Corpus>(json).is_err());
}

#[test]
fn qualified_linux_signals_are_exact_and_other_targets_keep_the_aligned_band() {
    let qualified = cfg!(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ));
    let mut signals = 0;
    for (case, family, sequence) in evidence::cases() {
        let reference = block_harness::read_reference(case, family);
        let run = block_harness::drive_case_with_corpus_reference(case, sequence, &reference);
        for (output, comparison) in case.outputs.iter().zip(&run.comparisons) {
            if output.kind != block_harness::SignalKind::Real {
                continue;
            }
            signals += 1;
            assert!(!comparison.masked);
            assert!(comparison.result.passed());
            assert_eq!(comparison.result.compared_points(), reference.n_rows);
            assert_eq!(
                matches!(comparison.result, ComparisonResult::Exact(_)),
                qualified
            );
            assert_eq!(
                matches!(comparison.result, ComparisonResult::AlignedTolerance(_)),
                !qualified
            );
            let t = comparison.tolerance;
            assert_eq!([t.atolx, t.rtolx, t.ltolx], [0.0; 3]);
            assert_eq!(
                [t.atoly, t.rtoly, t.ltoly],
                [if qualified { 0.0 } else { 1e-12 }; 3]
            );
        }
    }
    assert_eq!(signals, 21);
}

#[test]
fn source_oracle_and_input_drift_refuse_but_later_checkout_identity_is_allowed() {
    let retained = evidence::read_retained();
    let mut later = retained.clone();
    later.git_revision = "0".repeat(40);
    assert_eq!(evidence::same_contract(&retained, &later), Ok(()));
    for path in retained.source_sha256.keys() {
        let mut changed = later.clone();
        changed.source_sha256.insert(path.clone(), "0".repeat(64));
        assert_eq!(
            evidence::same_contract(&retained, &changed).unwrap_err(),
            "corpus provenance/inventory changed: explicit refresh required"
        );
    }
    for index in 0..21 {
        for field in [
            "provenance_sha256",
            "reference_sha256",
            "cxf_sha256",
            "oracle",
            "times",
        ] {
            let mut changed = serde_json::to_value(&later).unwrap();
            if field == "oracle" || field == "times" {
                changed["signals"][index][field][0] = evidence::word(42.0).into();
            } else {
                changed["signals"][index][field] = "0".repeat(64).into();
            }
            let mut changed: evidence::Corpus = serde_json::from_value(changed).unwrap();
            // An attacker can recompute a truthful mismatch list, but cannot change the oracle.
            let signal = &mut changed.signals[index];
            signal.mismatches = signal
                .oracle
                .iter()
                .zip(&signal.actual)
                .enumerate()
                .filter_map(|(i, (a, b))| (!evidence::equal(a, b)).then_some(i))
                .collect();
            assert_eq!(
                evidence::same_contract(&retained, &changed).unwrap_err(),
                "corpus provenance/inventory changed: explicit refresh required"
            );
        }
    }
}
