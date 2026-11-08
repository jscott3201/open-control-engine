//! Fail-closed comparison of eight independently produced native Linux captures.

use super::evidence::{self, CELLS, Corpus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const HISTORICAL_DIRECTORY: &str = "crates/oce-conformance/tests/fixtures/strict_bits/linux";
const NATIVE_DIRECTORY: &str = "crates/oce-conformance/tests/fixtures/strict_bits/qualified-linux";

/// Reviewed Git and assembled-matrix identity of one native Linux capture set.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub git_revision: String,
    pub matrix_sha256: String,
}

/// Whether the current receipt still binds today's selected sources. Absent means admitted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum EvidenceStatus {
    #[default]
    Admitted,
    RecapturePending,
}

/// Reviewed record of an open re-capture window; data only, never a checker switch by itself.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Pending {
    pub since: String,
    pub reason: String,
    pub opened_by_issue: u32,
    pub closed_by_issue: u32,
    pub supersedes_matrix_sha256: String,
}

/// Closed receipt data: historical and optional current identities plus the evidence status.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipts {
    pub historical: Receipt,
    pub current: Option<Receipt>,
    #[serde(default)]
    pub status: EvidenceStatus,
    #[serde(default)]
    pub pending: Option<Pending>,
}

pub(super) fn receipts() -> Receipts {
    // Terminal review data, not executable checker semantics. No receipt values are hashed into
    // their own captures. Final admission changes this JSON and raw fixtures, never checker code.
    serde_json::from_str(include_str!("../fixtures/strict_bits/receipts.json")).unwrap()
}

/// The only path to a receipt that may admit or source-qualify native evidence: an admitted status.
pub(super) fn current_receipt(receipts: Receipts) -> Result<Receipt, String> {
    if let Some(pending) = pending_window(&receipts)? {
        return Err(format!(
            "RECAPTURE_PENDING: status recapture-pending (GitHub #{}) cannot admit or source-qualify \
             native evidence; admission requires status admitted and exact source digests",
            pending.opened_by_issue
        ));
    }
    receipts.current.ok_or_else(|| {
        "PENDING_NATIVE_EVIDENCE: checker-complete Linux receipt has not been admitted".into()
    })
}

/// Validates the status record. `None` is the admitted status; `Some` is a well-formed window.
pub(super) fn pending_window(receipts: &Receipts) -> Result<Option<&Pending>, String> {
    let pending = match (receipts.status, &receipts.pending) {
        (EvidenceStatus::Admitted, None) => return Ok(None),
        (EvidenceStatus::Admitted, Some(_)) => {
            return Err("admitted status must not carry a pending record".into());
        }
        (EvidenceStatus::RecapturePending, None) => {
            return Err("recapture-pending status requires a pending record".into());
        }
        (EvidenceStatus::RecapturePending, Some(pending)) => pending,
    };
    let superseded = receipts
        .current
        .as_ref()
        .ok_or("recapture-pending status must supersede a current receipt")?;
    if pending.supersedes_matrix_sha256 != superseded.matrix_sha256 {
        return Err("pending record names a receipt other than the current one".into());
    }
    Ok(Some(pending))
}

/// Whether a retained report compares captured source digests with this checkout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceBinding {
    Required,
    PendingRecapture,
}

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
    evidence::same_reference_inventory(&evidence::read_retained(), &current).unwrap();
    // Candidate assembly is independent of receipt admission. Compare every captured source
    // digest to this checkout, including this checker; never substitute the historical map.
    let result = compare(&current, &runs);
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

pub(super) fn synthetic_runs() -> (Corpus, BTreeMap<String, Corpus>) {
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

// The pending-window notice is written to stderr on purpose; the workspace denies `print_stderr`.
#[allow(clippy::print_stderr)]
#[test]
fn retained_native_linux_evidence_is_complete_exact_and_source_bound() {
    let receipts = receipts();
    let Some(pending) = pending_window(&receipts).expect("well-formed evidence status") else {
        let receipt = current_receipt(receipts).expect("current native qualification required");
        let runs = read_native_runs(&evidence::root().join(NATIVE_DIRECTORY));
        qualified_report(&runs, &receipt, &evidence::collect())
            .expect("accepted checker-complete native Linux evidence");
        return;
    };
    // Every check except the source-digest comparison still runs against the superseded receipt.
    let receipt = receipts
        .current
        .as_ref()
        .expect("validated superseded receipt");
    let runs = read_native_runs(&evidence::root().join(NATIVE_DIRECTORY));
    let current = evidence::collect();
    report(&runs, receipt, &current, SourceBinding::PendingRecapture)
        .expect("superseded native Linux evidence still exact, complete and digest-pinned");
    let drifted: Vec<_> = current
        .source_sha256
        .iter()
        .filter(|(path, digest)| {
            runs["linux-x86_64-debug-first"].source_sha256.get(*path) != Some(*digest)
        })
        .map(|(path, _)| path.as_str())
        .collect();
    eprintln!(
        "RECAPTURE_PENDING: source binding of native receipt {} NOT enforced since {} \
         (opened by GitHub #{}, closes with GitHub #{}): {} Bound sources drifted from the \
         receipt: {drifted:?}. Numerical, topology, repeat, inventory and matrix-digest checks ran.",
        receipt.matrix_sha256,
        pending.since,
        pending.opened_by_issue,
        pending.closed_by_issue,
        pending.reason
    );
}

#[test]
fn historical_native_receipt_remains_verbatim_but_cannot_qualify_current_sources() {
    let runs = read_native_runs(&evidence::root().join(HISTORICAL_DIRECTORY));
    let receipt = receipts().historical;
    pinned_report(&evidence::read_retained(), &runs, &receipt).unwrap();
    assert_eq!(
        qualified_report(&runs, &receipt, &evidence::collect()).unwrap_err(),
        "source inventory changed: new native evidence required"
    );
}

fn read_native_runs(directory: &std::path::Path) -> BTreeMap<String, Corpus> {
    let mut runs = BTreeMap::new();
    for cell in CELLS {
        let first = directory.join(format!("{cell}-first.json"));
        let repeat = directory.join(format!("{cell}-repeat.json"));
        for (label, path) in [("first", first), ("repeat", repeat)] {
            let run = evidence::read(&path);
            runs.insert(format!("{cell}-{label}"), run);
        }
        // read() bounds the input and verifies that its bytes equal this canonical encoding.
        assert_eq!(
            evidence::encode(&runs[&format!("{cell}-first")]),
            evidence::encode(&runs[&format!("{cell}-repeat")])
        );
    }
    runs
}

pub(super) fn qualified_report(
    runs: &BTreeMap<String, Corpus>,
    receipt: &Receipt,
    current: &Corpus,
) -> Result<Vec<u8>, String> {
    report(runs, receipt, current, SourceBinding::Required)
}

pub(super) fn report(
    runs: &BTreeMap<String, Corpus>,
    receipt: &Receipt,
    current: &Corpus,
    binding: SourceBinding,
) -> Result<Vec<u8>, String> {
    evidence::same_reference_inventory(&evidence::read_retained(), current)?;
    for run in runs.values() {
        if binding == SourceBinding::Required {
            evidence::require_sources(run, &current.source_sha256)?;
        }
        evidence::same_reference_inventory(current, run)?;
    }
    let baseline = runs
        .get("linux-x86_64-debug-first")
        .ok_or("missing native baseline")?;
    pinned_report(baseline, runs, receipt)
}

fn pinned_report(
    baseline: &Corpus,
    runs: &BTreeMap<String, Corpus>,
    receipt: &Receipt,
) -> Result<Vec<u8>, String> {
    let comparison = compare(baseline, runs);
    if !comparison.as_ref()?.is_empty() {
        return Err("accepted native matrix has numerical mismatches".into());
    }
    for run in runs.values() {
        if run.git_revision != receipt.git_revision {
            return Err("accepted native checkout provenance changed".into());
        }
        if run.signals.iter().map(|s| s.actual.len()).sum::<usize>() != 161 {
            return Err("accepted native sample coverage changed".into());
        }
    }
    let report = serde_json::json!({
        "schema": 1,
        "claim": "pinned-corpus-only-not-mathematical-correctness",
        "macos_status": "unqualified-until-M06-PR02",
        "runs": runs,
        "comparison": comparison,
    });
    let bytes = serde_json::to_vec(&report).unwrap();
    if evidence::digest(&bytes) != receipt.matrix_sha256 {
        return Err("accepted native matrix digest changed".into());
    }
    Ok(bytes)
}

#[test]
fn native_receipt_refuses_relabeling_and_raw_payload_reblessing() {
    let runs = read_native_runs(&evidence::root().join(HISTORICAL_DIRECTORY));
    let baseline = evidence::read_retained();
    let receipt = receipts().historical;
    let mut relabeled = runs.clone();
    for run in relabeled.values_mut() {
        run.git_revision = "0".repeat(40);
    }
    assert_eq!(
        pinned_report(&baseline, &relabeled, &receipt).unwrap_err(),
        "accepted native checkout provenance changed"
    );
    let mut changed = runs;
    for run in changed.values_mut() {
        let signal = &mut run.signals[0];
        let sample = signal
            .actual
            .iter()
            .position(|w| w.starts_with("nan:"))
            .unwrap();
        signal.actual[sample] = evidence::word(f64::from_bits(0x7ff8_0000_0000_0123));
    }
    // This preserves the comparator's NaN-class agreement and every first/repeat pair.
    // It still cannot rewrite the historical raw payloads under the accepted receipt.
    assert_eq!(
        pinned_report(&baseline, &changed, &receipt).unwrap_err(),
        "accepted native matrix digest changed"
    );
}

#[test]
fn pending_receipt_is_an_explicit_refusal_not_historical_acceptance() {
    let pending = Receipts {
        historical: receipts().historical,
        current: None,
        status: EvidenceStatus::Admitted,
        pending: None,
    };
    assert_eq!(
        current_receipt(pending).unwrap_err(),
        "PENDING_NATIVE_EVIDENCE: checker-complete Linux receipt has not been admitted"
    );
    let invalid =
        r#"{"historical":{"git_revision":"x","matrix_sha256":"y","bypass":true},"current":null}"#;
    assert!(serde_json::from_str::<Receipts>(invalid).is_err());
}

/// Synthetic captures bound to this checkout's exact sources, with the receipt they assemble to.
/// No files are written and no native claim is made.
pub(super) fn synthetic_bound_admission() -> (BTreeMap<String, Corpus>, Corpus, Receipt) {
    let (_, mut runs) = synthetic_runs();
    let current = evidence::collect();
    for run in runs.values_mut() {
        run.source_sha256.clone_from(&current.source_sha256);
    }
    let report = serde_json::json!({
        "schema": 1,
        "claim": "pinned-corpus-only-not-mathematical-correctness",
        "macos_status": "unqualified-until-M06-PR02",
        "runs": runs,
        "comparison": {"Ok": []},
    });
    let receipt = Receipt {
        git_revision: runs["linux-x86_64-debug-first"].git_revision.clone(),
        matrix_sha256: evidence::digest(&serde_json::to_vec(&report).unwrap()),
    };
    (runs, current, receipt)
}

#[test]
fn every_bound_source_change_refuses_admission_even_when_raw_bits_still_match() {
    let (runs, current, receipt) = synthetic_bound_admission();
    let sources = &current.source_sha256;
    qualified_report(&runs, &receipt, &current).unwrap();
    for path in sources.keys() {
        let mut changed = current.clone();
        let original = std::fs::read_to_string(evidence::root().join(path)).unwrap();
        let mutated = match path.as_str() {
            "crates/oce-conformance/tests/strict_bits/evidence.rs" => original.replace(
                "(left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()",
                "true",
            ),
            "crates/oce-conformance/tests/strict_bits/matrix.rs" => original.replace(
                "runs.len() != 8 || expected_keys.iter().any(|key| !runs.contains_key(key))",
                "false",
            ),
            _ => format!("{original}\n"),
        };
        assert_ne!(mutated, original, "source mutation must apply: {path}");
        changed
            .source_sha256
            .insert(path.clone(), evidence::digest(mutated.as_bytes()));
        assert_eq!(
            qualified_report(&runs, &receipt, &changed).unwrap_err(),
            format!("source digest changed: {path}: new native evidence required")
        );
        let mut missing = runs.clone();
        for run in missing.values_mut() {
            run.source_sha256.remove(path);
        }
        assert_eq!(
            qualified_report(&missing, &receipt, &current).unwrap_err(),
            "source inventory changed: new native evidence required"
        );
    }
}

#[test]
#[ignore = "explicit local admission of the digest-pinned downloaded native artifact only"]
fn admit_downloaded_native_matrix_without_rewriting_provenance() {
    use std::io::Write;

    assert!(std::env::var_os("CI").is_none(), "CI cannot admit evidence");
    let receipt =
        current_receipt(receipts()).expect("review the new native receipt before admission");
    let source = std::env::var_os("OCE_STRICT_MATRIX_DIR").expect("download directory required");
    let source = evidence::root().join(source);
    let matrix = source.join("matrix.json");
    assert!(std::fs::metadata(&matrix).unwrap().len() <= 1024 * 1024);
    let downloaded = std::fs::read(matrix).unwrap();
    assert_eq!(evidence::digest(&downloaded), receipt.matrix_sha256);
    let runs = read_native_runs(&source);
    assert_eq!(
        qualified_report(&runs, &receipt, &evidence::collect()).unwrap(),
        downloaded
    );

    // Admission copies validated native bytes, never engine output. Refuse replacement and
    // keep the same one-object-per-signal encoding; the aggregate is exactly reconstructible.
    let destination = evidence::root().join(NATIVE_DIRECTORY);
    std::fs::create_dir(&destination).expect("never replace admitted native evidence");
    for (key, run) in runs {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join(format!("{key}.json")))
            .unwrap();
        file.write_all(&evidence::encode(&run)).unwrap();
    }
}
