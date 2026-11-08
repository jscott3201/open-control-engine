//! Fail-closed comparison of eight independently produced native Linux captures.
//!
//! The per-PR native matrix is the standing evidence: every pull request recaptures all four
//! Linux architecture/codegen cells on native runners, twice each, and
//! `native_linux_matrix_is_complete_and_exact` compares them with the Tier-A references and with
//! each other. Nothing is admitted from stored digests. `receipts.json` is a log of the last
//! green native run plus the identities of the retained capture archives; it gates nothing.

use super::evidence::{self, CELLS, Corpus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const FIXTURES: &str = "crates/oce-conformance/tests/fixtures/strict_bits";

/// Name of the record the per-PR fan-in job writes beside the captures for the receipt log.
pub(super) const CANDIDATE_FILE: &str = "receipt-candidate.json";

/// Identity of one retained archive of eight native captures: history, never current admission.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Archive {
    /// Directory under the strict-bit fixtures holding the eight captures.
    pub directory: String,
    /// GitHub Actions run that produced the captures.
    pub run_id: u64,
    /// Pull request head the run tested.
    pub head_sha: String,
    /// Synthetic merge checkout recorded in every capture.
    pub git_revision: String,
    /// SHA-256 of the reconstructed `matrix.json` report.
    pub matrix_sha256: String,
}

/// SHA-256 of the canonical bytes of one cell's two captures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CellCaptures {
    pub first_sha256: String,
    pub repeat_sha256: String,
}

/// Log record of one green native eight-cell matrix, exactly as the fan-in job writes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeRun {
    /// GitHub Actions run id.
    pub run_id: u64,
    /// Pull request head the run tested.
    pub head_sha: String,
    /// Checkout the captures were taken from (the synthetic merge commit on a PR run).
    pub merge_sha: String,
    pub rustc: String,
    pub libm: String,
    /// Exactly the four native Linux cells.
    pub cells: BTreeMap<String, CellCaptures>,
    /// SHA-256 of the assembled `matrix.json` report.
    pub matrix_sha256: String,
}

/// Closed receipt log: data only, never a checker switch.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipts {
    pub last_green_native_run: NativeRun,
    pub archives: Vec<Archive>,
}

pub(super) fn receipts() -> Receipts {
    serde_json::from_str(include_str!("../fixtures/strict_bits/receipts.json")).unwrap()
}

fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Structural validation of a logged or candidate native run.
pub(super) fn validate_native_run(run: &NativeRun) -> Result<(), String> {
    if run.run_id == 0 || !is_hex(&run.head_sha, 40) || !is_hex(&run.merge_sha, 40) {
        return Err("native run identity: run id and 40-hex head and merge SHAs required".into());
    }
    if !run.rustc.starts_with("rustc ") || run.libm.is_empty() {
        return Err("native run toolchain: rustc and libm required".into());
    }
    if !run.cells.keys().map(String::as_str).eq(sorted_cells()) {
        return Err("native run cells: exactly the four native Linux cells required".into());
    }
    let mut seen = Vec::new();
    for (cell, captures) in &run.cells {
        if !is_hex(&captures.first_sha256, 64) || !is_hex(&captures.repeat_sha256, 64) {
            return Err(format!("{cell}: capture digests must be 64-hex SHA-256"));
        }
        if captures.first_sha256 != captures.repeat_sha256 {
            return Err(format!(
                "{cell}: first and repeat captures must be byte-identical"
            ));
        }
        // Each capture names its own cell, so equal digests mean a copied or relabeled capture.
        if seen.contains(&captures.first_sha256) {
            return Err(format!("{cell}: capture duplicates another cell"));
        }
        seen.push(captures.first_sha256.clone());
    }
    if !is_hex(&run.matrix_sha256, 64) {
        return Err("native run matrix digest must be 64-hex SHA-256".into());
    }
    Ok(())
}

fn sorted_cells() -> impl Iterator<Item = &'static str> {
    let mut cells = CELLS;
    cells.sort_unstable();
    cells.into_iter()
}

/// The log record for an assembled matrix: per-cell capture digests and the report digest.
pub(super) fn native_run_record(
    runs: &BTreeMap<String, Corpus>,
    report: &[u8],
    run_id: u64,
    head_sha: &str,
) -> NativeRun {
    let baseline = &runs["linux-x86_64-debug-first"];
    let capture = |key: String| evidence::digest(&evidence::encode(&runs[&key]));
    NativeRun {
        run_id,
        head_sha: head_sha.into(),
        merge_sha: baseline.git_revision.clone(),
        rustc: baseline.rustc.clone(),
        libm: baseline.libm.clone(),
        cells: CELLS
            .into_iter()
            .map(|cell| {
                let captures = CellCaptures {
                    first_sha256: capture(format!("{cell}-first")),
                    repeat_sha256: capture(format!("{cell}-repeat")),
                };
                (cell.to_owned(), captures)
            })
            .collect(),
        matrix_sha256: evidence::digest(report),
    }
}

/// The assembled report whose SHA-256 is the matrix digest.
pub(super) fn matrix_report(
    runs: &BTreeMap<String, Corpus>,
    comparison: &Result<Vec<Mismatch>, String>,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "claim": "pinned-corpus-only-not-mathematical-correctness",
        "macos_status": "unqualified-until-M06-PR02",
        "runs": runs,
        "comparison": comparison,
    }))
    .unwrap()
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

/// Every capture must come from the checkout the comparison itself runs in.
pub(super) fn require_checkout_revision(
    runs: &BTreeMap<String, Corpus>,
    current: &Corpus,
) -> Result<(), String> {
    if runs
        .values()
        .any(|run| run.git_revision != current.git_revision)
    {
        return Err("matrix must describe this checkout revision".into());
    }
    Ok(())
}

// Run per PR by the strict-bit-compare fan-in in pr-gate.yml over the eight native captures.
#[ignore = "requires eight native CI captures; synthetic controls run everywhere"]
#[test]
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
    require_checkout_revision(&runs, &current).unwrap();
    evidence::same_reference_inventory(&evidence::read_retained(), &current).unwrap();
    // Every capture is compared with a fresh collection from this checkout: the same provenance,
    // inventory and oracle bytes, then every sample against the Tier-A reference and the
    // x86_64 debug capture. No stored digest is consulted.
    let result = compare(&current, &runs);
    // Retain all raw bits even on a numerical disagreement. No oracle is rewritten, and a
    // disagreement cannot silently downgrade a comparator or produce a passing gate.
    let report = matrix_report(&runs, &result);
    std::fs::write(directory.join("matrix.json"), &report).unwrap();
    let mismatches = result.expect("complete native matrix");
    assert!(
        mismatches.is_empty(),
        "per-signal adjudication required; retain 1e-12 band: {mismatches:#?}"
    );
    // The candidate log record, written only for a green matrix and only when the workflow names
    // the run. It is pasted into receipts.json by review; nothing reads it back as admission.
    let run_id = std::env::var("OCE_STRICT_RUN_ID").ok();
    let head_sha = std::env::var("OCE_STRICT_HEAD_SHA").ok();
    if let (Some(run_id), Some(head_sha)) = (run_id, head_sha) {
        let run_id = run_id.parse().expect("numeric run id");
        let record = native_run_record(&runs, &report, run_id, &head_sha);
        validate_native_run(&record).expect("well-formed candidate record");
        let mut bytes = serde_json::to_vec_pretty(&record).unwrap();
        bytes.push(b'\n');
        std::fs::write(directory.join(CANDIDATE_FILE), bytes).unwrap();
    }
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

#[test]
fn receipts_log_records_a_green_native_run_on_exactly_the_four_native_cells() {
    let receipts = receipts();
    let log = &receipts.last_green_native_run;
    validate_native_run(log).unwrap();
    assert_eq!(log.rustc, evidence::RUSTC);
    assert_eq!(log.libm, evidence::LIBM);
    // A logged run whose captures are retained must describe exactly those bytes.
    if let Some(archive) = receipts.archives.iter().find(|a| a.run_id == log.run_id) {
        let runs = read_native_runs(&evidence::root().join(FIXTURES).join(&archive.directory));
        let report = pinned_report(&runs, archive).unwrap();
        assert_eq!(
            &native_run_record(&runs, &report, archive.run_id, &archive.head_sha),
            log
        );
    }
}

#[test]
fn receipts_log_refuses_absent_extra_mislabeled_and_unrepeated_cells() {
    let log = receipts().last_green_native_run;
    let refuse = |change: &dyn Fn(&mut NativeRun)| {
        let mut bad = log.clone();
        change(&mut bad);
        validate_native_run(&bad).unwrap_err()
    };
    let cells = "native run cells: exactly the four native Linux cells required";
    for cell in CELLS {
        assert_eq!(refuse(&|run| drop(run.cells.remove(cell))), cells);
    }
    assert_eq!(
        refuse(&|run| {
            let captures = run.cells["linux-x86_64-debug"].clone();
            run.cells.insert("macos-aarch64-debug".into(), captures);
        }),
        cells
    );
    assert_eq!(
        refuse(&|run| {
            let captures = run.cells.remove("linux-aarch64-release").unwrap();
            run.cells.insert("linux-aarch64-emulated".into(), captures);
        }),
        cells
    );
    assert_eq!(
        refuse(&|run| {
            let captures = run.cells.get_mut("linux-aarch64-debug").unwrap();
            captures.repeat_sha256 = "0".repeat(64);
        }),
        "linux-aarch64-debug: first and repeat captures must be byte-identical"
    );
    assert_eq!(
        refuse(&|run| {
            let captures = run.cells["linux-x86_64-debug"].clone();
            run.cells.insert("linux-x86_64-release".into(), captures);
        }),
        "linux-x86_64-release: capture duplicates another cell"
    );
    assert_eq!(
        refuse(&|run| run
            .cells
            .get_mut("linux-x86_64-debug")
            .unwrap()
            .first_sha256 = "x".into()),
        "linux-x86_64-debug: capture digests must be 64-hex SHA-256"
    );
    let identity = "native run identity: run id and 40-hex head and merge SHAs required";
    assert_eq!(refuse(&|run| run.run_id = 0), identity);
    assert_eq!(refuse(&|run| run.head_sha.truncate(39)), identity);
    assert_eq!(
        refuse(&|run| run.merge_sha = run.merge_sha.to_uppercase()),
        identity
    );
    assert_eq!(
        refuse(&|run| run.matrix_sha256.clear()),
        "native run matrix digest must be 64-hex SHA-256"
    );
    // The retired admission vocabulary cannot return as data: the log schema is closed.
    let mut json: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/strict_bits/receipts.json")).unwrap();
    for (field, value) in [
        ("status", serde_json::json!("recapture-pending")),
        (
            "current",
            serde_json::json!({"git_revision": "0", "matrix_sha256": "0"}),
        ),
        ("source_sha256", serde_json::json!({})),
    ] {
        let mut changed = json.clone();
        changed[field] = value;
        assert!(
            serde_json::from_value::<Receipts>(changed).is_err(),
            "{field}"
        );
    }
    json["last_green_native_run"]["bypass"] = true.into();
    assert!(serde_json::from_value::<Receipts>(json).is_err());
}

#[test]
fn retained_native_archives_reconstruct_their_recorded_matrix_digests() {
    let archives = receipts().archives;
    assert_eq!(
        archives
            .iter()
            .map(|a| a.directory.as_str())
            .collect::<Vec<_>>(),
        ["linux", "qualified-linux"]
    );
    for archive in &archives {
        let runs = read_native_runs(&evidence::root().join(FIXTURES).join(&archive.directory));
        pinned_report(&runs, archive).unwrap();
    }
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

/// Reassembles an archive's report and refuses any change to its raw bits or provenance.
fn pinned_report(runs: &BTreeMap<String, Corpus>, archive: &Archive) -> Result<Vec<u8>, String> {
    let retained = evidence::read_retained();
    for run in runs.values() {
        evidence::same_reference_inventory(&retained, run)?;
    }
    let baseline = runs
        .get("linux-x86_64-debug-first")
        .ok_or("missing native baseline")?;
    let comparison = compare(baseline, runs);
    if !comparison.as_ref()?.is_empty() {
        return Err("accepted native matrix has numerical mismatches".into());
    }
    for run in runs.values() {
        if run.git_revision != archive.git_revision {
            return Err("accepted native checkout provenance changed".into());
        }
        if run.signals.iter().map(|s| s.actual.len()).sum::<usize>() != 161 {
            return Err("accepted native sample coverage changed".into());
        }
    }
    let bytes = matrix_report(runs, &comparison);
    if evidence::digest(&bytes) != archive.matrix_sha256 {
        return Err("accepted native matrix digest changed".into());
    }
    Ok(bytes)
}

#[test]
fn native_receipt_refuses_relabeling_and_raw_payload_reblessing() {
    let archive = &receipts().archives[0];
    let runs = read_native_runs(&evidence::root().join(FIXTURES).join(&archive.directory));
    let mut relabeled = runs.clone();
    for run in relabeled.values_mut() {
        run.git_revision = "0".repeat(40);
    }
    assert_eq!(
        pinned_report(&relabeled, archive).unwrap_err(),
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
    // It still cannot rewrite the historical raw payloads under the recorded digest.
    assert_eq!(
        pinned_report(&changed, archive).unwrap_err(),
        "accepted native matrix digest changed"
    );
}

#[test]
fn a_capture_from_another_revision_is_refused_as_mixed() {
    let (retained, mut runs) = synthetic_runs();
    for repeat in ["first", "repeat"] {
        runs.get_mut(&format!("linux-aarch64-release-{repeat}"))
            .unwrap()
            .git_revision = "f".repeat(40);
    }
    assert_eq!(
        compare(&retained, &runs).unwrap_err(),
        "linux-aarch64-release: mixed Git revisions"
    );
}

#[test]
fn a_matrix_captured_at_a_foreign_revision_does_not_describe_this_checkout() {
    let (retained, runs) = synthetic_runs();
    require_checkout_revision(&runs, &retained).unwrap();
    let mut foreign = runs;
    for run in foreign.values_mut() {
        run.git_revision = "f".repeat(40);
    }
    // Internally consistent, so the cross-cell comparison alone would accept it.
    assert_eq!(compare(&retained, &foreign), Ok(vec![]));
    assert_eq!(
        require_checkout_revision(&foreign, &retained).unwrap_err(),
        "matrix must describe this checkout revision"
    );
}
