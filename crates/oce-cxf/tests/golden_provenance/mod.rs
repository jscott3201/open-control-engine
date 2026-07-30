//! Integrity checks for G36 determinism golden provenance records.
//!
//! `content_sha256` binds each record to its checked-in CSV bytes, not to the
//! engine that produced them. Editing a CSV together with its digest passes by
//! design; this guard detects drift between the two checked-in artifacts.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

/// Mirrors `BLESS_DISABLED_VALUES` in
/// `oce-conformance/tests/g36_determinism/support.rs`, the source of truth.
const BLESS_DISABLED_VALUES: &[&str] = &["", "0", "false"];
const SUPPORT_RS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../oce-conformance/tests/g36_determinism/support.rs"
));
const BLESS_MODULE_RS: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/bless/mod.rs"));

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenProvenance {
    tier: String,
    source: String,
    depends_on_oce_blocks: bool,
    content_sha256: String,
    reference_columns: Vec<String>,
}

struct GoldenPairs {
    root: PathBuf,
    csvs: BTreeSet<PathBuf>,
    provenance: BTreeSet<PathBuf>,
}

fn golden_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../oce-conformance/tests/fixtures/golden/g36_traces")
}

fn enumerate(root: PathBuf) -> GoldenPairs {
    let mut csvs = BTreeSet::new();
    let mut provenance = BTreeSet::new();
    let children = fs::read_dir(&root)
        .unwrap_or_else(|error| panic!("cannot enumerate {}: {error}", root.display()));
    for child in children {
        let path = child
            .unwrap_or_else(|error| panic!("cannot enumerate {}: {error}", root.display()))
            .path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("csv") {
            csvs.insert(path);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".prov.json"))
        {
            provenance.insert(path);
        }
    }
    GoldenPairs {
        root,
        csvs,
        provenance,
    }
}

fn csv_for(provenance: &Path) -> PathBuf {
    let file_name = provenance
        .file_name()
        .and_then(|name| name.to_str())
        .expect("provenance file name is UTF-8");
    provenance.with_file_name(format!(
        "{}.csv",
        file_name
            .strip_suffix(".prov.json")
            .expect("enumerated provenance suffix")
    ))
}

fn provenance_for(csv: &Path) -> PathBuf {
    csv.with_extension("prov.json")
}

fn assert_pairing(pairs: &GoldenPairs) {
    for provenance in &pairs.provenance {
        let csv = csv_for(provenance);
        assert!(
            pairs.csvs.contains(&csv),
            "{} has no sibling {}",
            provenance.display(),
            csv.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("*.csv")
        );
    }
    for csv in &pairs.csvs {
        let provenance = provenance_for(csv);
        assert!(
            pairs.provenance.contains(&provenance),
            "{} has no sibling {}",
            csv.display(),
            provenance
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("*.prov.json")
        );
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn blessing_is_enabled(value: &str) -> bool {
    !BLESS_DISABLED_VALUES
        .iter()
        .any(|disabled| value.eq_ignore_ascii_case(disabled))
}

#[test]
fn every_provenance_record_matches_its_golden_bytes() {
    let pairs = enumerate(golden_root());
    assert_pairing(&pairs);
    assert!(
        !pairs.provenance.is_empty(),
        "expected at least one golden provenance record in {}, found {}",
        pairs
            .root
            .canonicalize()
            .unwrap_or_else(|error| panic!("cannot resolve {}: {error}", pairs.root.display()))
            .display(),
        pairs.provenance.len()
    );

    for provenance_path in &pairs.provenance {
        let csv_path = csv_for(provenance_path);
        let provenance_bytes = fs::read(provenance_path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", provenance_path.display()));
        let record: GoldenProvenance =
            serde_json::from_slice(&provenance_bytes).unwrap_or_else(|error| {
                panic!("cannot deserialize {}: {error}", provenance_path.display())
            });
        let GoldenProvenance {
            tier,
            source,
            depends_on_oce_blocks,
            content_sha256: recorded,
            reference_columns,
        } = record;
        let _ = (tier, source, depends_on_oce_blocks, reference_columns);
        let csv_bytes = fs::read(&csv_path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", csv_path.display()));
        let recomputed = hex(&Sha256::digest(csv_bytes));
        assert_eq!(
            recorded,
            recomputed,
            "{}: content_sha256 does not match {} (recorded {}, recomputed {})",
            provenance_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("provenance file name is UTF-8"),
            csv_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("CSV file name is UTF-8"),
            recorded,
            recomputed
        );
    }
}

#[test]
fn bless_truthiness_vocabulary_is_pinned() {
    let values = BLESS_DISABLED_VALUES
        .iter()
        .map(|value| format!("{value:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let expected = format!(
        "const BLESS_DISABLED_VALUES: [&str; {}] = [{values}];",
        BLESS_DISABLED_VALUES.len()
    );
    assert!(
        SUPPORT_RS.contains(&expected),
        "expected support.rs to contain {expected:?}; reconcile support.rs with the guard vocabulary"
    );
    assert!(
        BLESS_MODULE_RS.contains(&expected),
        "expected bless/mod.rs to contain {expected:?}; reconcile bless/mod.rs with the guard vocabulary"
    );

    let cases = [
        ("", false),
        ("0", false),
        ("false", false),
        ("FALSE", false),
        ("False", false),
        ("1", true),
        ("true", true),
        ("yes", true),
        ("0.0", true),
    ];
    for (value, expected) in cases {
        assert_eq!(
            blessing_is_enabled(value),
            expected,
            "OCE_BLESS_G36 truthiness for {value:?}"
        );
    }
}

fn visit_rust_sources(root: &Path, sources: &mut Vec<PathBuf>) {
    let children = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("cannot enumerate {}: {error}", root.display()));
    for child in children {
        let path = child
            .unwrap_or_else(|error| panic!("cannot enumerate {}: {error}", root.display()))
            .path();
        if path.is_dir() {
            visit_rust_sources(&path, sources);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            sources.push(path);
        }
    }
}

/// Pins the centralized `OCE_BLESS` reader and its comparison marker.
///
/// This is a source-text check, not a semantic one: environment iteration or a runtime-assembled
/// variable name evades it, the predicate body is pinned only by the presence of
/// `eq_ignore_ascii_case`, and readers outside `crates/oce-cxf/tests/` are out of scope.
#[test]
fn oce_bless_environment_reads_are_centralized() {
    let tests_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut sources = Vec::new();
    visit_rust_sources(&tests_root, &mut sources);
    assert!(
        sources.len() >= 40,
        "expected to visit at least 40 Rust sources under {}, visited {}",
        tests_root.display(),
        sources.len()
    );

    let variable = ["OCE", "BLESS"].join("_");
    let needles = [
        format!("var_os(\"{variable}\")"),
        format!("var(\"{variable}\")"),
    ];
    let mut matching_files = BTreeSet::new();
    let mut match_count = 0;
    for path in &sources {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let matches = needles
            .iter()
            .map(|needle| source.matches(needle).count())
            .sum::<usize>();
        if matches != 0 {
            matching_files.insert(
                path.strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
                    .expect("walked source is under the crate root")
                    .to_path_buf(),
            );
            match_count += matches;
        }
    }

    assert_eq!(
        matching_files,
        BTreeSet::from([PathBuf::from("tests/bless/mod.rs")]),
        "OCE_BLESS environment reads must remain centralized"
    );
    assert_eq!(
        match_count, 1,
        "expected exactly one OCE_BLESS environment read"
    );
    assert!(
        BLESS_MODULE_RS.contains("eq_ignore_ascii_case"),
        "bless/mod.rs must retain the case-insensitive disabling-value comparison"
    );
}

/// Refuses armed golden regeneration in the gate environment.
#[test]
fn gate_environment_does_not_arm_golden_blessing() {
    if let Ok(value) = std::env::var("OCE_BLESS_G36") {
        assert!(
            !blessing_is_enabled(&value),
            "OCE_BLESS_G36={value:?} enables blessing in the gate environment"
        );
    }
    assert!(
        !crate::bless::enabled(),
        "OCE_BLESS arms golden regeneration; use the name-filtered commands \
         `OCE_BLESS=1 cargo test -p oce-cxf --test fixture_structural_oracle verdict_table` or \
         `OCE_BLESS=1 cargo test -p oce-cxf --test fixture_structural_oracle \
         checked_in_manifest_bytes_equal_fresh_render`"
    );
}
