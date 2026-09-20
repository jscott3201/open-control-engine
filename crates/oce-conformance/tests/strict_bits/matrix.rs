//! Fail-closed comparison of eight independently produced native Linux captures.

use super::evidence::{self, CELLS, Corpus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Mismatch {
    pub signal: String,
    pub sample: usize,
    pub cell: String,
    pub expected: String,
    pub actual: String,
}

pub(super) fn compare(
    retained: &Corpus,
    runs: &BTreeMap<String, Corpus>,
) -> Result<Vec<Mismatch>, String> {
    let expected_keys: Vec<_> = CELLS
        .iter()
        .flat_map(|cell| [format!("{cell}-first"), format!("{cell}-repeat")])
        .collect();
    if runs.len() != 8 || expected_keys.iter().any(|key| !runs.contains_key(key)) {
        return Err("requires exactly four Linux cells, each with first and repeat".into());
    }
    for cell in CELLS {
        let first = &runs[&format!("{cell}-first")];
        let repeat = &runs[&format!("{cell}-repeat")];
        if first.cell != cell || repeat.cell != cell {
            return Err(format!("{cell}: native cell identity mismatch"));
        }
        evidence::same_contract(retained, first)?;
        evidence::same_contract(retained, repeat)?;
        if first.git_revision != runs["linux-x86_64-debug-first"].git_revision {
            return Err(format!("{cell}: mixed Git revisions"));
        }
        // Raw NaN payloads are retained, and must repeat within a single native cell.
        if first != repeat {
            for (left, right) in first.signals.iter().zip(&repeat.signals) {
                for (sample, (expected, actual)) in
                    left.actual.iter().zip(&right.actual).enumerate()
                {
                    if expected != actual {
                        return Err(format!(
                            "{cell}: repeated raw capture differs: {} sample {sample}: {expected} != {actual}",
                            left.id
                        ));
                    }
                }
            }
            return Err(format!("{cell}: repeated provenance differs"));
        }
    }
    let mut mismatches = Vec::new();
    let baseline = &runs["linux-x86_64-debug-first"];
    for cell in CELLS {
        let run = &runs[&format!("{cell}-first")];
        for (signal, reference) in run.signals.iter().zip(&baseline.signals) {
            for (sample, (actual, oracle)) in signal.actual.iter().zip(&signal.oracle).enumerate() {
                // Equal to the independent reference AND across architecture/codegen. Neither
                // comparison alone proves both. NaN class, signed infinity and signed zero match
                // the exact driver's existing semantics; all raw payloads remain in the artifact.
                for expected in [oracle, &reference.actual[sample]] {
                    if !evidence::equal(expected, actual) {
                        mismatches.push(Mismatch {
                            signal: signal.id.clone(),
                            sample,
                            cell: cell.into(),
                            expected: expected.clone(),
                            actual: actual.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(mismatches)
}

#[test]
#[ignore = "requires eight native CI captures; synthetic controls run everywhere"]
fn native_linux_matrix_is_complete_and_exact() {
    let directory = std::env::var_os("OCE_STRICT_MATRIX_DIR").expect("matrix directory required");
    let directory = evidence::root().join(directory);
    let mut runs = BTreeMap::new();
    for cell in CELLS {
        for repeat in ["first", "repeat"] {
            let key = format!("{cell}-{repeat}");
            runs.insert(
                key.clone(),
                evidence::read(&directory.join(format!("{key}.json"))),
            );
        }
    }
    let current = evidence::collect();
    assert!(
        runs.values()
            .all(|run| run.git_revision == current.git_revision),
        "matrix must describe this checkout revision"
    );
    evidence::same_contract(&evidence::read_retained(), &current).unwrap();
    let result = compare(&evidence::read_retained(), &runs);
    // Retain all raw bits even on a numerical disagreement. No oracle is rewritten, and a
    // disagreement cannot silently downgrade a comparator or produce a passing gate.
    let report = serde_json::json!({
        "schema": 1,
        "claim": "pinned-corpus-only-not-mathematical-correctness",
        "macos_status": "unqualified-until-M06-PR02",
        "runs": runs,
        "comparison": result,
    });
    std::fs::write(
        directory.join("matrix.json"),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    let mismatches = result.expect("complete native matrix");
    assert!(
        mismatches.is_empty(),
        "per-signal adjudication required; retain 1e-12 band: {mismatches:#?}"
    );
}

fn synthetic_runs() -> (Corpus, BTreeMap<String, Corpus>) {
    let retained = evidence::read_retained();
    let runs = CELLS
        .into_iter()
        .flat_map(|cell| {
            let mut run = retained.clone();
            run.cell = cell.into();
            [
                (format!("{cell}-first"), run.clone()),
                (format!("{cell}-repeat"), run),
            ]
        })
        .collect();
    (retained, runs)
}

#[test]
fn complete_synthetic_matrix_passes_without_claiming_native_evidence() {
    let (retained, runs) = synthetic_runs();
    assert_eq!(compare(&retained, &runs), Ok(vec![]));
}

#[test]
fn absent_extra_duplicate_and_mislabeled_cells_are_refused() {
    let (retained, runs) = synthetic_runs();
    for key in runs.keys() {
        let mut bad = runs.clone();
        bad.remove(key);
        assert_eq!(
            compare(&retained, &bad).unwrap_err(),
            "requires exactly four Linux cells, each with first and repeat"
        );
    }
    let mut extra = runs.clone();
    extra.insert("macos-aarch64-debug-first".into(), retained.clone());
    assert_eq!(
        compare(&retained, &extra).unwrap_err(),
        "requires exactly four Linux cells, each with first and repeat"
    );
    let mut bad = runs;
    bad.get_mut("linux-aarch64-release-first").unwrap().cell = "linux-x86_64-release".into();
    assert_eq!(
        compare(&retained, &bad).unwrap_err(),
        "linux-aarch64-release: native cell identity mismatch"
    );
}

#[test]
fn repeated_generation_drift_is_refused_before_qualification() {
    let (retained, mut runs) = synthetic_runs();
    let signal = &mut runs
        .get_mut("linux-aarch64-release-repeat")
        .unwrap()
        .signals[0];
    signal.actual[0] = evidence::word(evidence::value(&signal.actual[0]).unwrap().next_up());
    signal.mismatches = vec![0];
    let detail = compare(&retained, &runs).unwrap_err();
    assert!(detail.starts_with("linux-aarch64-release: repeated raw capture differs:"));
    assert!(detail.contains(&format!("{} sample 0:", retained.signals[0].id)));
}

#[test]
fn every_signal_and_cell_reports_the_exact_finite_mutation() {
    let (retained, original) = synthetic_runs();
    for index in 0..21 {
        for cell in CELLS {
            let mut runs = original.clone();
            let original_signal = &retained.signals[index];
            let sample = original_signal
                .actual
                .iter()
                .position(|w| evidence::value(w).unwrap().is_finite())
                .unwrap();
            let changed = evidence::word(
                evidence::value(&original_signal.actual[sample])
                    .unwrap()
                    .next_up(),
            );
            for repeat in ["first", "repeat"] {
                let signal = &mut runs.get_mut(&format!("{cell}-{repeat}")).unwrap().signals[index];
                signal.actual[sample] = changed.clone();
                signal.mismatches = vec![sample];
            }
            let failures = compare(&retained, &runs).unwrap();
            assert!(
                failures.contains(&Mismatch {
                    signal: original_signal.id.clone(),
                    sample,
                    cell: cell.into(),
                    expected: original_signal.oracle[sample].clone(),
                    actual: changed,
                }),
                "signal/cell mutation escaped: {index} {cell}"
            );
        }
    }
}

#[test]
fn nan_payload_is_not_a_cross_architecture_identity_claim() {
    let (retained, mut runs) = synthetic_runs();
    for repeat in ["first", "repeat"] {
        let signal = &mut runs
            .get_mut(&format!("linux-aarch64-release-{repeat}"))
            .unwrap()
            .signals[0];
        let sample = signal
            .actual
            .iter()
            .position(|w| evidence::value(w).unwrap().is_nan())
            .unwrap();
        signal.actual[sample] = evidence::word(f64::from_bits(0xfff8_0000_0000_0123));
    }
    assert_eq!(compare(&retained, &runs), Ok(vec![]));
}
