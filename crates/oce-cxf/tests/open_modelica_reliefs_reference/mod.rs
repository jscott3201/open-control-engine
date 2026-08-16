//! Per-PR validation for the scoped two-architecture OpenModelica Reliefs evidence.

pub(crate) mod canonicalizer;
mod expectations;
mod manifest;
mod repository;
mod run_log;
mod safe_read;
mod schema;
#[cfg(unix)]
mod verifier_adversarial_tests;

use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oce-cxf is below repository root")
        .to_path_buf()
}

fn fixture_relative(name: &str) -> String {
    format!("crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs/{name}")
}

fn fixture(name: &str) -> PathBuf {
    repository_root().join(fixture_relative(name))
}

fn checked_manifest() -> schema::Manifest {
    let contract = checked_generation_contract();
    let bytes = safe_read::read(&repository_root(), &fixture_relative("manifest.json"))
        .expect("checked Reliefs manifest is bounded");
    manifest::parse(&bytes, &contract.revision).expect("checked Reliefs manifest parses")
}

fn checked_generation_contract() -> schema::GenerationRevisionContract {
    let bytes = safe_read::read(&repository_root(), expectations::GENERATION_CONTRACT_PATH)
        .expect("generation revision contract is bounded");
    manifest::parse_generation_contract(&bytes)
        .expect("generation revision contract has a closed schema")
}

#[test]
fn native_raw_runs_reproduce_exact_projection_and_cross_architecture_bytes() {
    let root = repository_root();
    let mut cross_architecture = None;
    for architecture in ["arm64", "amd64"] {
        let first_bytes = safe_read::read(
            &root,
            &fixture_relative(&format!("{architecture}/reliefs-run-a.raw.csv")),
        )
        .unwrap();
        let second_bytes = safe_read::read(
            &root,
            &fixture_relative(&format!("{architecture}/reliefs-run-b.raw.csv")),
        )
        .unwrap();
        assert_eq!(first_bytes, second_bytes, "{architecture} repeat raw bytes");
        let first =
            canonicalizer::canonicalize_bytes(&first_bytes, "openmodelica_g36_reliefs").unwrap();
        let second =
            canonicalizer::canonicalize_bytes(&second_bytes, "openmodelica_g36_reliefs").unwrap();
        assert_eq!(first, second);
        assert_eq!(first.raw_rows.len(), 21);
        assert_eq!(first.grouped_rows.len(), 14);
        assert_eq!(first.rows.len(), 7);
        assert_eq!(
            first.group_sizes,
            [1, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 2]
        );
        for (index, row) in first.rows.iter().enumerate() {
            let parse = |value: &str| u64::from_str_radix(value, 16).unwrap();
            assert_eq!(row.time.to_bits(), parse(expectations::TIME_BITS[index]));
            assert_eq!(
                row.input_bits(),
                [
                    parse(expectations::U_T_SUP_BITS[index]),
                    0x3fd0_0000_0000_0000,
                    0x3fec_0000_0000_0000,
                    0x3fc0_0000_0000_0000,
                    0x3fe8_0000_0000_0000,
                ]
            );
            assert_eq!(
                row.output_bits(),
                [
                    parse(expectations::Y_OUT_BITS[index]),
                    parse(expectations::Y_RET_BITS[index]),
                ]
            );
        }
        let retained = safe_read::read(
            &root,
            &fixture_relative(&format!("{architecture}/reliefs.canonical.csv")),
        )
        .unwrap();
        assert_eq!(first.bytes, retained);
        if let Some(expected) = &cross_architecture {
            assert_eq!(expected, &retained);
        } else {
            cross_architecture = Some(retained);
        }
    }
}

#[test]
fn parameter_final_clamp_and_projection_controls_are_live_on_both_architectures() {
    let root = repository_root();
    for architecture in ["arm64", "amd64"] {
        let read_raw = |name: &str, table: &str| {
            canonicalizer::canonicalize_bytes(
                &safe_read::read(&root, &fixture_relative(&format!("{architecture}/{name}")))
                    .unwrap(),
                table,
            )
            .unwrap()
        };
        let main = read_raw("reliefs-run-a.raw.csv", "main");
        let parameter = read_raw("parameter-control.raw.csv", "parameter");
        let mismatch = main
            .rows
            .iter()
            .zip(&parameter.rows)
            .position(|(expected, observed)| expected.output_bits() != observed.output_bits());
        assert_eq!(mismatch, Some(2));
        assert_eq!(main.rows[2].y_out_dam.to_bits(), 0x3fe2_0000_0000_0000);
        assert_eq!(parameter.rows[2].y_out_dam.to_bits(), 0x3fec_0000_0000_0000);
        for (expected, observed) in main.rows.iter().zip(&parameter.rows) {
            assert_eq!(expected.input_bits(), observed.input_bits());
        }

        let clamp = read_raw("final-clamp.raw.csv", "clamp");
        let parse = |value: &str| u64::from_str_radix(value, 16).unwrap();
        for (index, row) in clamp.raw_rows.iter().enumerate() {
            assert_eq!(row.source_index, index);
            assert_eq!(
                row.time.to_bits(),
                parse(expectations::RAW_TIME_BITS[index])
            );
            assert_eq!(
                row.input_bits(),
                [
                    parse(expectations::U_T_SUP_BITS[index / 3]),
                    0x3fec_0000_0000_0000,
                    0x3fd0_0000_0000_0000,
                    0x3fe8_0000_0000_0000,
                    0x3fc0_0000_0000_0000,
                ]
            );
            assert_eq!(
                row.output_bits(),
                [0x3fd0_0000_0000_0000, 0x3fe8_0000_0000_0000]
            );
        }
        for (index, row) in clamp.rows.iter().enumerate() {
            assert_eq!(row.source_index, [0, 3, 6, 9, 12, 15, 18][index]);
            assert_eq!(row.time.to_bits(), parse(expectations::TIME_BITS[index]));
            assert_eq!(
                row.input_bits(),
                [
                    parse(expectations::U_T_SUP_BITS[index]),
                    0x3fec_0000_0000_0000,
                    0x3fd0_0000_0000_0000,
                    0x3fe8_0000_0000_0000,
                    0x3fc0_0000_0000_0000,
                ]
            );
        }

        let keep_first = canonicalizer::canonicalize_path_with_selection(
            &fixture(&format!("{architecture}/reliefs-run-a.raw.csv")),
            "openmodelica_g36_reliefs",
            canonicalizer::ProjectionSelection::First,
        )
        .unwrap();
        assert_ne!(keep_first.bytes, main.bytes);
        assert_eq!(
            keep_first.bytes,
            safe_read::read(
                &root,
                &fixture_relative(&format!(
                    "{architecture}/projection-keep-first.canonical.csv"
                )),
            )
            .unwrap()
        );
        assert_eq!(
            keep_first
                .rows
                .iter()
                .map(|row| row.input_bits())
                .collect::<Vec<_>>(),
            main.rows
                .iter()
                .map(|row| row.input_bits())
                .collect::<Vec<_>>()
        );
        let metadata = std::fs::read_to_string(fixture(&format!(
            "{architecture}/projection-keep-first.metadata"
        )))
        .unwrap();
        let one = |key: &str| {
            let prefix = format!("{key}=");
            let values = metadata
                .lines()
                .filter_map(|line| line.strip_prefix(&prefix))
                .collect::<Vec<_>>();
            assert_eq!(values.len(), 1, "metadata key {key}");
            values[0]
        };
        assert_eq!(one("selection"), "first");
        assert_eq!(one("raw_rows"), "21");
        assert_eq!(one("grouped_rows"), "14");
        assert_eq!(one("canonical_rows"), "7");
        assert_eq!(one("group_sizes"), "1,1,2,1,2,1,2,1,2,1,2,1,2,2");
        assert_eq!(one("selected_source_rows"), "0,4,7,10,13,16,19");
        assert_eq!(one("raw_time_bits"), expectations::RAW_TIME_BITS.join(","));
        assert_eq!(
            one("selected_time_bits"),
            expectations::KEEP_FIRST_TIME_BITS.join(",")
        );
    }
}

#[test]
fn closed_manifest_digests_native_records_logs_sources_tools_and_oci_graph() {
    let parsed = checked_manifest();
    repository::validate(&parsed, &repository_root()).expect("Reliefs evidence graph validates");
    assert_eq!(parsed, checked_manifest());
}

#[cfg(unix)]
#[test]
fn retained_validation_survives_squash_without_git_metadata() {
    let source_root = repository_root();
    let manifest = checked_manifest();
    let temporary_root = (0_u32..4096)
        .find_map(|nonce| {
            let path = std::env::temp_dir()
                .join(format!("oce-reliefs-squash-{}-{nonce}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => Some(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => panic!("cannot create synthetic checkout: {error}"),
            }
        })
        .expect("synthetic checkout nonce is available");
    for artifact in &manifest.artifacts {
        let source = source_root.join(&artifact.path);
        let destination = temporary_root.join(&artifact.path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::copy(source, destination).unwrap();
    }
    let manifest_path = fixture_relative("manifest.json");
    let destination = temporary_root.join(&manifest_path);
    std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
    std::fs::copy(source_root.join(&manifest_path), destination).unwrap();
    assert!(!temporary_root.join(".git").exists());
    repository::validate(&manifest, &temporary_root)
        .expect("retained bytes validate without revision history");
    let python = std::process::Command::new("python3")
        .arg(source_root.join("tools/openmodelica-reliefs-reference/reliefs/verify_evidence.py"))
        .arg("final")
        .arg(temporary_root.join("crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs"))
        .arg(&temporary_root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .unwrap();
    assert!(
        python.status.success(),
        "{}",
        String::from_utf8_lossy(&python.stderr)
    );

    let mut unrelated_revision = manifest.clone();
    unrelated_revision.architectures[0].repository_revision =
        "f9e4ba93e39c6be8b4118d77c52a8a6ed1c88abb".into();
    assert!(repository::validate(&unrelated_revision, &temporary_root).is_err());
    std::fs::write(
        temporary_root.join("tools/openmodelica-reliefs-reference/reliefs/ReliefsPilot.mo"),
        b"mutated generator input\n",
    )
    .unwrap();
    assert!(repository::validate(&manifest, &temporary_root).is_err());
    std::fs::remove_dir_all(temporary_root).unwrap();
}

#[cfg(unix)]
#[test]
fn independent_python_validator_accepts_the_retained_graph() {
    let root = repository_root();
    let bytecode = root.join("tools/openmodelica-reliefs-reference/reliefs/__pycache__");
    assert!(!bytecode.exists());
    let output = std::process::Command::new("python3")
        .args([
            "tools/openmodelica-reliefs-reference/reliefs/verify_evidence.py",
            "final",
            "crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs",
            ".",
        ])
        .current_dir(root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        b"Reliefs assembled evidence verification passed\n"
    );
    assert!(!bytecode.exists());
}

#[test]
fn malformed_manifest_fails_closed_at_schema_literals_types_and_bounds() {
    let original = std::fs::read_to_string(fixture("manifest.json")).unwrap();
    let generation_revision = checked_generation_contract().revision;
    for (mutated, fragment) in [
        (
            original.replacen("\"format\":", "\"unknown\": true, \"format\":", 1),
            "unknown field",
        ),
        (
            original.replacen("\"format\":", "\"format\": \"wrong\", \"format\":", 1),
            "duplicate field",
        ),
        (
            original.replacen(
                "  \"format\": \"oce-openmodelica-reliefs-external-run-v1\",\n",
                "",
                1,
            ),
            "missing field",
        ),
        (
            original.replacen("\"raw_rows\": 21", "\"raw_rows\": \"21\"", 1),
            "invalid type",
        ),
        (
            original.replacen("exact_finite_f64_bits", "funnel", 1),
            "scope comparison",
        ),
        (
            original.replacen("\"raw_rows\": 21", "\"raw_rows\": 22", 1),
            "projection rules",
        ),
    ] {
        let error = manifest::parse(mutated.as_bytes(), &generation_revision).unwrap_err();
        assert!(error.contains(fragment), "{fragment}: {error}");
    }
    assert_eq!(
        manifest::parse(
            &vec![b' '; manifest::MAX_MANIFEST_BYTES + 1],
            &generation_revision,
        )
        .unwrap_err(),
        "manifest exceeds 256 KiB"
    );
    let long = original.replacen(
        "\"source_default_dyadic_regions\"",
        &format!("\"{}\"", "x".repeat(4097)),
        1,
    );
    assert_eq!(
        manifest::parse(long.as_bytes(), &generation_revision).unwrap_err(),
        "manifest string exceeds 4096 UTF-8 bytes"
    );
    let unrelated = original.replacen(
        &generation_revision,
        "f9e4ba93e39c6be8b4118d77c52a8a6ed1c88abb",
        1,
    );
    assert!(
        manifest::parse(unrelated.as_bytes(), &generation_revision)
            .unwrap_err()
            .contains("generation revision")
    );
    let contract =
        std::fs::read_to_string(repository_root().join(expectations::GENERATION_CONTRACT_PATH))
            .unwrap();
    assert!(
        manifest::parse_generation_contract(
            contract
                .replace(&generation_revision, &"0".repeat(40))
                .as_bytes()
        )
        .unwrap_err()
        .contains("contract revision")
    );
}

#[test]
fn repository_mutations_fail_digest_path_role_and_native_binding() {
    let root = repository_root();
    let mut digest = checked_manifest();
    digest.artifacts[0].sha256.replace_range(..1, "0");
    assert!(
        repository::validate(&digest, &root)
            .unwrap_err()
            .contains("digest mismatch")
    );

    let mut traversal = checked_manifest();
    traversal.artifacts[0].path = "../outside".into();
    assert!(
        repository::validate(&traversal, &root)
            .unwrap_err()
            .contains("invalid repository-relative")
    );

    let mut reused = checked_manifest();
    reused.artifacts[1].path = reused.artifacts[0].path.clone();
    assert!(
        repository::validate(&reused, &root)
            .unwrap_err()
            .contains("reused")
    );

    let mut native = checked_manifest();
    native.architectures[0]
        .raw_run_a_sha256
        .replace_range(..1, "f");
    let error = repository::validate(&native, &root).unwrap_err();
    assert!(error.contains("native architecture"), "{error}");
}

#[test]
fn package_lists_include_reliefs_evidence_and_exclude_the_tool() {
    let root = repository_root();
    let parsed = checked_manifest();
    for package in ["oce-cxf", "oce-conformance"] {
        let output = std::process::Command::new("cargo")
            .args(["package", "--list", "-p", package, "--allow-dirty"])
            .current_dir(&root)
            .env("CARGO_NET_OFFLINE", "true")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let listing = String::from_utf8(output.stdout).unwrap();
        if package == "oce-cxf" {
            for artifact in &parsed.artifacts {
                if let Some(required) = artifact.path.strip_prefix("crates/oce-cxf/") {
                    assert!(
                        listing.lines().any(|line| line == required),
                        "package omits {required}"
                    );
                }
            }
        } else {
            assert!(
                listing.lines().any(|line| {
                    line == "tests/fixtures/open_modelica/g36_reliefs/manifest.json"
                })
            );
            for artifact in &parsed.artifacts {
                if let Some(required) = artifact.path.strip_prefix("crates/oce-conformance/") {
                    assert!(
                        listing.lines().any(|line| line == required),
                        "package omits {required}"
                    );
                }
            }
        }
        assert!(!listing.contains("tools/openmodelica-reliefs-reference"));
    }
}

#[test]
fn scoped_evidence_adds_no_report_or_public_runtime_surface() {
    let root = repository_root();
    let test =
        std::fs::read_to_string(root.join("crates/oce-conformance/tests/open_modelica_reliefs.rs"))
            .unwrap();
    for forbidden in [
        "known_divergence",
        "TierReport",
        "ConformanceReport",
        "assemble_report",
    ] {
        assert!(!test.contains(forbidden));
    }
    assert!(!test.lines().any(|line| line.starts_with("pub ")));
    let public_surface =
        std::fs::read_to_string(root.join("crates/oce-conformance/src/lib.rs")).unwrap();
    assert!(!public_surface.contains("open_modelica_reliefs"));
    let workspace = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!workspace.contains("tools/openmodelica-reliefs-reference"));
}

#[cfg(unix)]
#[test]
fn helper_regressions_run_inside_the_structural_suite() {
    let root = repository_root();
    for (script, expected) in [
        (
            "deadline_test.sh",
            b"deadline accounting test passed\n".as_slice(),
        ),
        (
            "output_publish_test.sh",
            b"output publication test passed\n".as_slice(),
        ),
        (
            "container_cleanup_test.sh",
            b"container cleanup test passed\n".as_slice(),
        ),
    ] {
        let output = std::process::Command::new("sh")
            .arg(
                root.join("tools/openmodelica-reliefs-reference/reliefs")
                    .join(script),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{script}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected, "{script}");
    }
}

#[cfg(unix)]
#[test]
fn descriptor_reader_rejects_ancestor_symlink_and_final_fifo() {
    use std::os::unix::fs::{DirBuilderExt as _, symlink};

    let root = std::env::temp_dir().join(format!("oce-reliefs-safe-read-{}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&root)
        .unwrap();
    std::fs::create_dir(root.join("real")).unwrap();
    std::fs::write(root.join("real/file"), b"ok").unwrap();
    symlink(root.join("real"), root.join("link")).unwrap();
    assert!(safe_read::read(&root, "link/file").is_err());
    let fifo = root.join("real/fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        safe_read::read(&root, "real/fifo")
            .unwrap_err()
            .contains("regular")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn windows_reader_rejects_hardlinks_and_reparse_points() {
    use std::os::windows::fs::symlink_file;

    let root = std::env::temp_dir().join(format!("oce-reliefs-safe-read-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let regular = root.join("regular");
    std::fs::write(&regular, b"ok").unwrap();
    std::fs::hard_link(&regular, root.join("alias")).unwrap();
    assert!(safe_read::read(&root, "regular").is_err());
    std::fs::remove_file(root.join("alias")).unwrap();
    if symlink_file(&regular, root.join("reparse")).is_ok() {
        assert!(safe_read::read(&root, "reparse").is_err());
    }
    std::fs::remove_dir_all(root).unwrap();
}
