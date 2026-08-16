//! Per-PR validation for the scoped two-architecture OpenModelica Line evidence.

#[cfg(unix)]
mod assembly_boundary_tests;
pub(crate) mod canonicalizer;
mod expectations;
mod manifest;
mod manifest_contract_tests;
mod native_records;
mod repository;
mod run_log_contract;
mod safe_read;
mod schema;
#[cfg(unix)]
mod verifier_adversarial_tests;

use canonicalizer::RealRow;
use std::path::{Path, PathBuf};

const GROUP_SIZES: &[usize] = &[1, 1, 2, 1, 2, 1, 2, 1, 2, 2];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oce-cxf is below repository root")
        .to_path_buf()
}

fn fixture_relative(name: &str) -> String {
    format!("crates/oce-conformance/tests/fixtures/open_modelica/reals_line/{name}")
}

fn fixture(name: &str) -> PathBuf {
    repository_root().join(fixture_relative(name))
}

fn checked_manifest() -> schema::Manifest {
    let bytes = safe_read::read(&repository_root(), &fixture_relative("manifest.json"))
        .expect("checked Line manifest is bounded");
    manifest::parse(&bytes).expect("checked Line manifest parses")
}

fn assert_schedule(rows: &[RealRow]) {
    assert_eq!(rows.len(), expectations::TIME_BITS.len());
    for (index, row) in rows.iter().enumerate() {
        let observed = [
            row.time.to_bits(),
            row.x1.to_bits(),
            row.f1.to_bits(),
            row.x2.to_bits(),
            row.f2.to_bits(),
            row.u.to_bits(),
            row.y_both.to_bits(),
            row.y_below.to_bits(),
            row.y_above.to_bits(),
            row.y_unlimited.to_bits(),
        ];
        let parse = |value: &str| u64::from_str_radix(value, 16).unwrap();
        let expected = [
            parse(expectations::TIME_BITS[index]),
            0xc000_0000_0000_0000,
            0x3ff4_0000_0000_0000,
            0x4000_0000_0000_0000,
            0x400a_0000_0000_0000,
            parse(expectations::U_BITS[index]),
            parse(expectations::Y_BOTH[index]),
            parse(expectations::Y_BELOW[index]),
            parse(expectations::Y_ABOVE[index]),
            parse(expectations::Y_UNLIMITED[index]),
        ];
        assert_eq!(
            observed, expected,
            "Line schedule or expected bits at row {index}"
        );
    }
}

#[test]
fn native_runs_reproduce_projection_schedule_and_cross_architecture_bytes() {
    let root = repository_root();
    let mut canonical = None;
    for architecture in ["arm64", "amd64"] {
        let first_bytes = safe_read::read(
            &root,
            &fixture_relative(&format!("{architecture}/line-run-a.raw.csv")),
        )
        .unwrap();
        let second_bytes = safe_read::read(
            &root,
            &fixture_relative(&format!("{architecture}/line-run-b.raw.csv")),
        )
        .unwrap();
        assert_eq!(first_bytes, second_bytes, "{architecture} repeat raw bytes");
        let first =
            canonicalizer::canonicalize_bytes(&first_bytes, "openmodelica_reals_line").unwrap();
        let second =
            canonicalizer::canonicalize_bytes(&second_bytes, "openmodelica_reals_line").unwrap();
        assert_eq!(first.raw_rows.len(), 15);
        assert_eq!(first.rows.len(), 10);
        assert_eq!(first.group_sizes, GROUP_SIZES);
        assert_eq!(first, second);
        assert_schedule(&first.rows);
        assert_eq!(
            first.bytes,
            safe_read::read(
                &root,
                &fixture_relative(&format!("{architecture}/line.canonical.csv"))
            )
            .unwrap()
        );
        if let Some(expected) = &canonical {
            assert_eq!(
                expected, &first.bytes,
                "canonical bytes differ by architecture"
            );
        } else {
            canonical = Some(first.bytes);
        }
    }
}

#[test]
fn external_flag_control_changes_only_below_mode_at_above_range_rows() {
    let root = repository_root();
    for architecture in ["arm64", "amd64"] {
        let main = canonicalizer::canonicalize_bytes(
            &safe_read::read(
                &root,
                &fixture_relative(&format!("{architecture}/line-run-a.raw.csv")),
            )
            .unwrap(),
            "main",
        )
        .unwrap();
        let control = canonicalizer::canonicalize_bytes(
            &safe_read::read(
                &root,
                &fixture_relative(&format!("{architecture}/flag-control.raw.csv")),
            )
            .unwrap(),
            "control",
        )
        .unwrap();
        let mismatches = main
            .rows
            .iter()
            .zip(&control.rows)
            .enumerate()
            .filter_map(|(index, (main, control))| {
                (main.y_below.to_bits() != control.y_below.to_bits()).then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(mismatches, [8, 9]);
        for (main, control) in main.rows.iter().zip(&control.rows) {
            assert_eq!(
                [
                    main.time.to_bits(),
                    main.x1.to_bits(),
                    main.f1.to_bits(),
                    main.x2.to_bits(),
                    main.f2.to_bits(),
                    main.u.to_bits(),
                    main.y_both.to_bits(),
                    main.y_above.to_bits(),
                    main.y_unlimited.to_bits(),
                ],
                [
                    control.time.to_bits(),
                    control.x1.to_bits(),
                    control.f1.to_bits(),
                    control.x2.to_bits(),
                    control.f2.to_bits(),
                    control.u.to_bits(),
                    control.y_both.to_bits(),
                    control.y_above.to_bits(),
                    control.y_unlimited.to_bits(),
                ]
            );
        }
        assert_eq!(
            control.rows[8].y_below.to_bits(),
            u64::from_str_radix(expectations::Y_BOTH[8], 16).unwrap()
        );
    }
}

#[test]
fn closed_manifest_digests_sources_logs_tools_and_oci_graph_validate() {
    let parsed = checked_manifest();
    repository::validate(&parsed, &repository_root()).expect("Line evidence graph validates");
    assert_eq!(parsed, checked_manifest());
}

#[test]
fn malformed_manifest_fails_closed_at_schema_literals_and_bounds() {
    let original = std::fs::read_to_string(fixture("manifest.json")).unwrap();
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
                "  \"format\": \"oce-openmodelica-line-external-run-v1\",\n",
                "",
                1,
            ),
            "missing field",
        ),
        (
            original.replacen("exact_finite_f64_bits", "funnel", 1),
            "scope.comparison",
        ),
        (
            original.replacen("line.canonical.csv", "alternate.canonical.csv", 1),
            "artifact role path",
        ),
    ] {
        assert!(
            manifest::parse(mutated.as_bytes())
                .unwrap_err()
                .contains(fragment)
        );
    }
    assert_eq!(
        manifest::parse(&vec![b' '; manifest::MAX_MANIFEST_BYTES + 1]).unwrap_err(),
        "manifest exceeds 256 KiB"
    );
    let long = original.replacen(
        "\"CDL.Reals.Line\"",
        &format!("\"{}\"", "x".repeat(4097)),
        1,
    );
    assert_eq!(
        manifest::parse(long.as_bytes()).unwrap_err(),
        "manifest string exceeds 4096 UTF-8 bytes"
    );
}

#[test]
fn repository_mutations_fail_digest_path_and_role_binding() {
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
}

#[test]
fn scoped_test_and_tool_add_no_public_surface_or_workspace_dependency() {
    let root = repository_root();
    let source =
        std::fs::read_to_string(root.join("crates/oce-conformance/tests/open_modelica_line.rs"))
            .unwrap();
    for forbidden in [
        "known_divergence",
        "TierReport",
        "ConformanceReport",
        "assemble_report",
    ] {
        assert!(!source.contains(forbidden));
    }
    assert!(!source.lines().any(|line| line.starts_with("pub ")));
    let workspace = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!workspace.contains("tools/openmodelica-line-reference"));
    let tool =
        std::fs::read_to_string(root.join("tools/openmodelica-line-reference/Cargo.toml")).unwrap();
    assert!(tool.contains("[workspace]"));
    assert!(tool.contains("rustix"));
    assert!(
        !root
            .join("tools/openmodelica-line-reference/build.rs")
            .exists()
    );
    let public_surface =
        std::fs::read_to_string(root.join("crates/oce-conformance/src/lib.rs")).unwrap();
    assert!(!public_surface.contains("open_modelica_line"));
}

#[test]
fn package_lists_include_line_evidence_and_exclude_the_tool() {
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
            assert!(
                listing
                    .lines()
                    .any(|line| line == "tests/open_modelica_line_reference/canonicalizer.rs")
            );
        } else {
            assert!(
                listing
                    .lines()
                    .any(|line| line == "tests/fixtures/open_modelica/reals_line/manifest.json")
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
        assert!(!listing.contains("tools/openmodelica-line-reference"));
    }
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
                root.join("tools/openmodelica-line-reference/line")
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

    let root = std::env::temp_dir().join(format!("oce-line-safe-read-{}", std::process::id()));
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
