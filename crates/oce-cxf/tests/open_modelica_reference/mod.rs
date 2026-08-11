//! Per-PR validation for the scoped OpenModelica Nand evidence set.

pub(crate) mod canonicalizer;
mod manifest;
mod repository;
mod schema;

use canonicalizer::{BooleanRow, canonicalize_path};
use std::path::{Path, PathBuf};

const GROUP_SIZES: &[usize] = &[1, 1, 2, 1, 2, 1, 2, 2];
const AND_OUTPUT_ROWS: &[bool] = &[false, false, false, false, false, false, true, true];
const INPUT_ROWS: &[(u64, bool, bool)] = &[
    (0x0000_0000_0000_0000, false, false),
    (0x404e_0000_0000_0000, false, false),
    (0x404e_0000_0000_0eff, false, true),
    (0x405e_0000_0000_0000, false, true),
    (0x405e_0000_0000_0781, true, false),
    (0x4066_8000_0000_0000, true, false),
    (0x4066_8000_0000_03c1, true, true),
    (0x406e_0000_0000_0000, true, true),
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oce-cxf is below the repository root")
        .to_path_buf()
}

fn fixture(name: &str) -> PathBuf {
    repository_root()
        .join("crates/oce-conformance/tests/fixtures/open_modelica/logical_nand")
        .join(name)
}

fn checked_manifest() -> schema::Manifest {
    manifest::parse(&std::fs::read(fixture("manifest.json")).unwrap())
        .expect("checked external-run manifest parses")
}

fn assert_schedule(rows: &[BooleanRow]) {
    let observed = rows
        .iter()
        .map(|row| (row.time.to_bits(), row.u1, row.u2))
        .collect::<Vec<_>>();
    assert_eq!(observed, INPUT_ROWS, "post-event source schedule drifted");
}

#[test]
fn checked_nand_artifacts_reproduce_projection_and_schedule() {
    let projected = canonicalize_path(&fixture("nand.raw.csv"), "openmodelica_logical_nand")
        .expect("checked Nand raw CSV canonicalizes");
    assert_eq!(projected.raw_rows.len(), 12);
    assert_eq!(projected.rows.len(), 8);
    assert_eq!(projected.group_sizes, GROUP_SIZES);
    assert_schedule(&projected.rows);
    assert_eq!(
        projected.bytes,
        canonicalizer::read_bounded_path(&fixture("nand.canonical.csv")).unwrap()
    );
}

#[test]
fn semantic_control_uses_the_same_projection_and_exact_and_output() {
    let projected = canonicalize_path(&fixture("and.raw.csv"), "openmodelica_logical_and")
        .expect("checked And raw CSV canonicalizes");
    assert_eq!(projected.raw_rows.len(), 12);
    assert_eq!(projected.rows.len(), 8);
    assert_eq!(projected.group_sizes, GROUP_SIZES);
    assert_schedule(&projected.rows);
    assert_eq!(
        projected.bytes,
        canonicalizer::read_bounded_path(&fixture("and.canonical.csv")).unwrap()
    );
    assert_eq!(
        projected.rows.iter().map(|row| row.y).collect::<Vec<_>>(),
        AND_OUTPUT_ROWS
    );
}

#[test]
fn wrappers_differ_only_by_the_external_class_token() {
    let root = repository_root();
    let nand = std::fs::read_to_string(root.join("tools/openmodelica-reference/nand/NandPilot.mo"))
        .unwrap();
    let and = std::fs::read_to_string(root.join("tools/openmodelica-reference/nand/AndPilot.mo"))
        .unwrap();
    let token = "Buildings.Controls.OBC.CDL.Logical.Nand";
    assert_eq!(nand.matches(token).count(), 1);
    assert_eq!(
        nand.replace(token, "Buildings.Controls.OBC.CDL.Logical.And"),
        and
    );
}

#[test]
fn closed_manifest_paths_digests_logs_and_oci_graph_validate() {
    let parsed = checked_manifest();
    repository::validate(&parsed, &repository_root())
        .expect("external-run evidence graph validates");
    assert_eq!(
        parsed,
        checked_manifest(),
        "manifest validation is deterministic"
    );
}

#[test]
fn malformed_manifest_fails_closed_at_schema_and_bounds() {
    let original = std::fs::read_to_string(fixture("manifest.json")).unwrap();
    let unknown = original.replacen("\"format\":", "\"unknown\": true, \"format\":", 1);
    assert!(
        manifest::parse(unknown.as_bytes())
            .unwrap_err()
            .contains("unknown field")
    );
    let duplicate = original.replacen("\"format\":", "\"format\": \"wrong\", \"format\":", 1);
    assert!(
        manifest::parse(duplicate.as_bytes())
            .unwrap_err()
            .contains("duplicate field")
    );
    let missing = original.replacen(
        "  \"format\": \"oce-openmodelica-external-run-v1\",\n",
        "",
        1,
    );
    assert!(
        manifest::parse(missing.as_bytes())
            .unwrap_err()
            .contains("missing field")
    );
    let wrong = original.replacen("exact_boolean", "funnel", 1);
    assert!(
        manifest::parse(wrong.as_bytes())
            .unwrap_err()
            .contains("scope.comparison")
    );
    let rebound = original.replacen("nand.canonical.csv", "alternate.canonical.csv", 1);
    assert!(
        manifest::parse(rebound.as_bytes())
            .unwrap_err()
            .contains("artifact role path")
    );
    let oversized = vec![b' '; manifest::MAX_MANIFEST_BYTES + 1];
    assert_eq!(
        manifest::parse(&oversized).unwrap_err(),
        "manifest exceeds 256 KiB"
    );
    let mut at_limit = original.as_bytes().to_vec();
    at_limit.extend(std::iter::repeat_n(
        b' ',
        manifest::MAX_MANIFEST_BYTES - at_limit.len(),
    ));
    assert!(manifest::parse(&at_limit).is_ok());
    let long = original.replacen(
        "\"CDL.Logical.Nand\"",
        &format!("\"{}\"", "x".repeat(4097)),
        1,
    );
    assert_eq!(
        manifest::parse(long.as_bytes()).unwrap_err(),
        "manifest string exceeds 4096 UTF-8 bytes"
    );
}

#[test]
fn repository_mutations_fail_path_digest_and_role_binding() {
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
fn scoped_test_surface_has_no_policy_or_public_report_consumption() {
    let root = repository_root();
    let sources = [
        root.join("crates/oce-conformance/tests/open_modelica_nand.rs"),
        root.join("crates/oce-conformance/tests/open_modelica_nand/support.rs"),
    ];
    for source in sources {
        let text = std::fs::read_to_string(source).unwrap();
        for forbidden in [
            "known_divergence",
            "TierReport",
            "ConformanceReport",
            "assemble_report",
        ] {
            assert!(
                !text.contains(forbidden),
                "scoped test consumes {forbidden}"
            );
        }
        assert!(
            !text.lines().any(|line| line.starts_with("pub ")),
            "scoped test exports a public item"
        );
    }
    let workspace = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!workspace.contains("tools/openmodelica-reference"));
    let tool =
        std::fs::read_to_string(root.join("tools/openmodelica-reference/Cargo.toml")).unwrap();
    assert!(tool.contains("[workspace]"));
    assert!(!tool.contains("[dependencies]"));
    assert!(!root.join("tools/openmodelica-reference/build.rs").exists());
    let public_surface = std::fs::read_to_string(root.join("crates/oce-conformance/src/lib.rs"))
        .expect("conformance public surface");
    assert!(!public_surface.contains("open_modelica"));
}

#[test]
fn package_lists_include_evidence_and_exclude_the_off_workspace_tool() {
    let root = repository_root();
    let manifest = checked_manifest();
    for package in ["oce-cxf", "oce-conformance"] {
        let output = std::process::Command::new("cargo")
            .args(["package", "--list", "-p", package, "--allow-dirty"])
            .current_dir(&root)
            .env("CARGO_NET_OFFLINE", "true")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "cargo package --list failed for {package}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let listing = String::from_utf8(output.stdout).unwrap();
        if package == "oce-cxf" {
            assert!(
                listing
                    .lines()
                    .any(|line| line == "tests/open_modelica_reference/canonicalizer.rs")
            );
        } else {
            assert!(
                listing
                    .lines()
                    .any(|line| line == "tests/fixtures/open_modelica/logical_nand/manifest.json"),
                "oce-conformance package omits the evidence manifest"
            );
            for artifact in &manifest.artifacts {
                let Some(required) = artifact.path.strip_prefix("crates/oce-conformance/") else {
                    continue;
                };
                assert!(
                    listing.lines().any(|line| line == required),
                    "oce-conformance package omits {required}"
                );
            }
        }
        assert!(!listing.contains("tools/openmodelica-reference"));
    }
}

#[test]
fn regeneration_uses_immutable_images_bounded_commands_and_a_hard_output_mount() {
    let root = repository_root();
    let regeneration =
        std::fs::read_to_string(root.join("tools/openmodelica-reference/nand/regenerate.sh"))
            .unwrap();
    let runner =
        std::fs::read_to_string(root.join("tools/openmodelica-reference/nand/runner.sh")).unwrap();
    for required in [
        "IMAGE=\"$IMAGE_REPOSITORY@$INDEX\"",
        "test \"$HOST_UID\" -ne 0",
        "run_timed 120 docker run",
        "monotonic_seconds",
        "run_timed \"$poll_timeout\" docker exec",
        "--tmpfs \"/out:rw,exec,nosuid,nodev,size=256m",
        "docker exec \"$container\" cat \"/out/$raw\"",
        "grep -Fqx 'runner_complete=1' \"$runner_log\"",
        "rm -rf \"$OUTPUT\"",
        "\"$IMAGE\" /reference/runner.sh",
    ] {
        assert!(
            regeneration.contains(required),
            "regeneration omits {required}"
        );
    }
    assert!(!regeneration.contains("\"$IMAGE_TAG\" /reference/runner.sh"));
    for required in [
        "grep -Fq 'resultFile = \"/out/NandPilot_res.csv\",'",
        "grep -Fq 'The simulation finished successfully.'",
        "touch /out/.oce-complete",
    ] {
        assert!(runner.contains(required), "runner omits {required}");
    }
}
