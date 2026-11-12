//! Canonical, bounded raw-bit corpus and version-pinned provenance.

use crate::block_harness::aligned_cases;
use crate::block_harness::{self, BlockCase, SignalKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) const RUSTC: &str = "rustc 1.97.1 (8bab26f4f 2026-07-14)";
pub(super) const LIBM: &str = "0.2.16";
pub(super) const CELLS: [&str; 4] = [
    "linux-x86_64-debug",
    "linux-x86_64-release",
    "linux-aarch64-debug",
    "linux-aarch64-release",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Signal {
    pub id: String,
    pub class: String,
    pub output: String,
    pub reference: String,
    pub provenance_sha256: String,
    pub reference_sha256: String,
    pub cxf_sha256: String,
    pub rule: String,
    pub math_library: String,
    pub linux_regime: String,
    pub other_regime: String,
    pub times: Vec<String>,
    pub oracle: Vec<String>,
    pub actual: Vec<String>,
    pub mismatches: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Corpus {
    pub schema: u32,
    pub rustc: String,
    pub libm: String,
    pub git_revision: String,
    pub cell: String,
    pub macos_status: String,
    pub linux_status: String,
    /// Selected-source digests that captures before GitHub #392 carried to bind a retained
    /// receipt. Retained historical captures keep theirs verbatim; new captures carry none and
    /// omit the field, because native evidence is re-established from scratch on every PR.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_sha256: BTreeMap<String, String>,
    pub signals: Vec<Signal>,
}

pub(super) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn read(path: &Path) -> Corpus {
    assert!(
        std::fs::metadata(path)
            .expect("raw-bit corpus must exist")
            .len()
            <= 128 * 1024,
        "bounded corpus"
    );
    let bytes = std::fs::read(path).expect("raw-bit corpus must exist");
    assert!(bytes.len() <= 128 * 1024, "bounded corpus");
    let corpus: Corpus = serde_json::from_slice(&bytes).expect("closed corpus schema");
    assert_eq!(bytes, encode(&corpus), "canonical corpus encoding");
    validate(&corpus).expect("valid corpus");
    corpus
}

pub(super) fn read_retained() -> Corpus {
    read(&root().join("crates/oce-conformance/tests/fixtures/strict_bits/corpus.json"))
}

// One object per signal keeps raw bits reviewable without thousands of pretty-printed lines.
pub(super) fn encode(corpus: &Corpus) -> Vec<u8> {
    let mut header = corpus.clone();
    header.signals.clear();
    let mut text = serde_json::to_string(&header).unwrap();
    text.truncate(text.len() - 2); // retain the opening signals array
    for (i, signal) in corpus.signals.iter().enumerate() {
        if i != 0 {
            text.push(',');
        }
        text.push('\n');
        text.push_str(&serde_json::to_string(signal).unwrap());
    }
    text.push_str("\n]}\n");
    text.into_bytes()
}

pub(super) fn cases() -> Vec<(&'static BlockCase, &'static str, &'static str)> {
    [
        (aligned_cases::REALS, "CDL/Reals", "single-block-reals"),
        (
            aligned_cases::SOURCE_SIN,
            "CDL/Reals",
            "single-block-reals-source-transcendental",
        ),
        (
            aligned_cases::PSYCHROMETRICS,
            "CDL/Psychrometrics",
            "single-block-psychrometrics",
        ),
        (
            aligned_cases::SUN,
            "CDL/Utilities",
            "single-block-utilities",
        ),
    ]
    .into_iter()
    .flat_map(|(cases, family, sequence)| cases.iter().map(move |case| (case, family, sequence)))
    .collect()
}

pub(super) fn word(value: f64) -> String {
    let label = if value.is_nan() {
        "nan"
    } else if value == f64::INFINITY {
        "+inf"
    } else if value == f64::NEG_INFINITY {
        "-inf"
    } else if value.to_bits() == 0 {
        "+zero"
    } else if value.to_bits() == 1_u64 << 63 {
        "-zero"
    } else {
        "finite"
    };
    format!("{label}:{:016x}", value.to_bits())
}

pub(super) fn value(word: &str) -> Result<f64, String> {
    let (_, bits) = word.split_once(':').ok_or("missing bit label")?;
    let value = f64::from_bits(u64::from_str_radix(bits, 16).map_err(|_| "invalid u64 bits")?);
    if self::word(value) != word {
        return Err("noncanonical bit label".into());
    }
    Ok(value)
}

pub(super) fn equal(left: &str, right: &str) -> bool {
    let left = value(left).expect("validated bits");
    let right = value(right).expect("validated bits");
    (left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()
}

pub(super) fn collect() -> Corpus {
    for key in ["RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"] {
        assert!(
            std::env::var_os(key).is_none_or(|value| value.is_empty()),
            "{key}: nonbaseline codegen requires explicit qualification"
        );
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root())
        .output()
        .expect("Git provenance");
    assert!(revision.status.success());
    let version = Command::new("rustc")
        .arg("--version")
        .output()
        .expect("rustc provenance");
    assert!(version.status.success());
    assert_eq!(String::from_utf8(version.stdout).unwrap().trim(), RUSTC);
    for lock in ["Cargo.lock", "tools/golden-gen/Cargo.lock"] {
        assert!(
            std::fs::read_to_string(root().join(lock))
                .unwrap()
                .contains(&format!("name = \"libm\"\nversion = \"{LIBM}\"")),
            "libm refresh required"
        );
    }
    let mut signals = Vec::new();
    for (case, family, sequence) in cases() {
        let (reference, run) = block_harness::drive_case_for_audit(case, family, sequence);
        let path = block_harness::reference_path(case, family);
        for (i, output) in case.outputs.iter().enumerate() {
            if output.kind != SignalKind::Real {
                continue;
            }
            let provenance_bytes =
                std::fs::read(path.with_file_name(format!("{}.prov.json", output.name))).unwrap();
            let provenance: serde_json::Value = serde_json::from_slice(&provenance_bytes).unwrap();
            assert_eq!(provenance["tier"], "A");
            assert_eq!(provenance["depends_on_oce_blocks"], false);
            assert_eq!(provenance["class_path"], case.class_path);
            assert_eq!(provenance["signal"], output.name);
            assert_eq!(provenance["n_samples"], reference.n_rows);
            let column = reference
                .col_names
                .as_ref()
                .unwrap()
                .iter()
                .position(|name| name == output.name)
                .unwrap();
            let oracle: Vec<_> = reference
                .data
                .chunks_exact(reference.n_cols)
                .map(|row| word(row[column]))
                .collect();
            let captured = run
                .trace
                .column(&block_harness::output_point(case, i))
                .unwrap();
            let actual: Vec<_> = captured.values.iter().map(|v| word(*v)).collect();
            let mismatches = oracle
                .iter()
                .zip(&actual)
                .enumerate()
                .filter_map(|(i, (a, b))| (!equal(a, b)).then_some(i))
                .collect();
            signals.push(Signal {
                id: format!("{}.{}", case.slug, output.name),
                class: case.class_path.into(),
                output: output.name.into(),
                reference: path.strip_prefix(root()).unwrap().to_str().unwrap().into(),
                provenance_sha256: digest(&provenance_bytes),
                reference_sha256: digest(&std::fs::read(path.clone()).unwrap()),
                cxf_sha256: digest(block_harness::build_cxf(case).as_bytes()),
                rule: provenance["reference_rule"].as_str().unwrap().into(),
                math_library: provenance["math_library"].as_str().unwrap().into(),
                linux_regime: "exact-candidate-pending-matrix".into(),
                other_regime: "aligned-atoly-rtoly-ltoly-1e-12".into(),
                times: run.trace.times.iter().map(|v| word(*v)).collect(),
                oracle,
                actual,
                mismatches,
            });
        }
    }
    signals.sort_by(|a, b| a.id.cmp(&b.id));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let corpus = Corpus {
        schema: 1,
        rustc: RUSTC.into(),
        libm: LIBM.into(),
        git_revision: String::from_utf8(revision.stdout).unwrap().trim().into(),
        cell: format!(
            "{}-{}-{profile}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        macos_status: "unqualified-until-M06-PR02".into(),
        linux_status: "candidate-requires-all-four-cells-and-two-runs-per-cell".into(),
        source_sha256: BTreeMap::new(),
        signals,
    };
    validate(&corpus).expect("complete collected corpus");
    corpus
}

pub(super) fn validate(corpus: &Corpus) -> Result<(), String> {
    if corpus.schema != 1 || corpus.rustc != RUSTC || corpus.libm != LIBM {
        return Err("version pin mismatch: explicit evidence refresh required".into());
    }
    if corpus.macos_status != "unqualified-until-M06-PR02"
        || corpus.linux_status != "candidate-requires-all-four-cells-and-two-runs-per-cell"
    {
        return Err("qualification boundary changed".into());
    }
    if corpus.git_revision.len() != 40
        || !corpus.git_revision.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("invalid Git provenance".into());
    }
    if corpus.signals.len() != 21 || corpus.source_sha256.len() > 128 {
        return Err("inventory coverage mismatch".into());
    }
    let mut previous = "";
    for signal in &corpus.signals {
        if signal.id.as_str() <= previous {
            return Err("duplicate or unordered signal".into());
        }
        previous = &signal.id;
        let n = signal.times.len();
        if !(1..=128).contains(&n) || signal.oracle.len() != n || signal.actual.len() != n {
            return Err(format!("{}: sample coverage mismatch", signal.id));
        }
        for word in signal
            .times
            .iter()
            .chain(&signal.oracle)
            .chain(&signal.actual)
        {
            value(word)?;
        }
        if signal.times.iter().any(|w| !value(w).unwrap().is_finite()) {
            return Err(format!("{}: non-finite time", signal.id));
        }
        let mismatches: Vec<_> = signal
            .oracle
            .iter()
            .zip(&signal.actual)
            .enumerate()
            .filter_map(|(i, (a, b))| (!equal(a, b)).then_some(i))
            .collect();
        if signal.mismatches != mismatches {
            return Err(format!("{}: hidden mismatch", signal.id));
        }
        if signal.linux_regime != "exact-candidate-pending-matrix"
            || signal.other_regime != "aligned-atoly-rtoly-ltoly-1e-12"
        {
            return Err(format!("{}: unreviewed regime", signal.id));
        }
    }
    Ok(())
}

// The historical corpus owns the unchanged reference/input inventory, not current qualification.
// Current qualification is the per-PR native matrix, which recaptures every cell from this checkout.
pub(super) fn same_reference_inventory(expected: &Corpus, actual: &Corpus) -> Result<(), String> {
    validate(expected)?;
    validate(actual)?;
    let mut signals = actual.signals.clone();
    for (a, e) in signals.iter_mut().zip(&expected.signals) {
        a.actual.clone_from(&e.actual);
        a.mismatches.clone_from(&e.mismatches);
    }
    if signals != expected.signals {
        return Err("reference/input inventory changed: explicit review required".into());
    }
    Ok(())
}

// A retained archive is history. Its harness documents were built with the namespace IRI the
// engine bound when the run was captured, so its `cxf_sha256` values are kept verbatim and are no
// longer compared with the corpus (GitHub #527), as its source map has not been since GitHub #392.
// Every other inventory field (reference, provenance, rule, times, oracle) still has to match.
pub(super) fn same_archived_inventory(expected: &Corpus, archived: &Corpus) -> Result<(), String> {
    let mut normalized = archived.clone();
    for (a, e) in normalized.signals.iter_mut().zip(&expected.signals) {
        a.cxf_sha256.clone_from(&e.cxf_sha256);
    }
    same_reference_inventory(expected, &normalized)
}

// Compare metadata and every oracle/time cell separately from observations. This prevents a
// missing output, sample, changed input or re-blessed oracle from masquerading as equality.
pub(super) fn same_contract(expected: &Corpus, actual: &Corpus) -> Result<(), String> {
    validate(expected)?;
    validate(actual)?;
    let mut normalized = actual.clone();
    normalized.cell.clone_from(&expected.cell);
    normalized.git_revision.clone_from(&expected.git_revision);
    for (a, e) in normalized.signals.iter_mut().zip(&expected.signals) {
        a.actual.clone_from(&e.actual);
        a.mismatches.clone_from(&e.mismatches);
    }
    if &normalized != expected {
        return Err("corpus provenance/inventory changed: explicit refresh required".into());
    }
    Ok(())
}
