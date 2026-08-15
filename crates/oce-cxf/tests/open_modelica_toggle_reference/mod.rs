//! Per-PR validation for the scoped OpenModelica Toggle evidence set.

pub(crate) mod canonicalizer;
mod manifest;
mod repository;
mod safe_read;
mod schema;
#[cfg(unix)]
#[path = "verifier_adversarial_tests.rs"]
mod verifier_adversarial_tests;

use canonicalizer::BooleanRow;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
use std::path::{Path, PathBuf};

const GROUP_SIZES: &[usize] = &[
    1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 2, 1, 2, 1, 2, 2, 1, 2, 1, 2,
];
const INPUT_ROWS: &[(u64, bool, bool)] = &[
    (0x0000_0000_0000_0000, true, false),
    (0x403e_0000_0000_1dff, false, false),
    (0x404e_0000_0000_0000, false, false),
    (0x4056_8000_0000_0780, true, false),
    (0x405e_0000_0000_0000, true, false),
    (0x4062_c000_0000_03c1, false, false),
    (0x4066_8000_0000_0000, false, false),
    (0x406a_4000_0000_03c1, true, false),
    (0x406e_0000_0000_0000, true, false),
    (0x4070_e000_0000_01e0, false, false),
    (0x4072_c000_0000_0000, false, false),
    (0x4073_6000_0000_0320, false, true),
    (0x4075_e000_0000_02d0, false, false),
    (0x4076_8000_0000_0000, false, false),
    (0x4078_6000_0000_03c1, true, true),
    (0x407a_4000_0000_0000, true, true),
    (0x407a_e000_0000_0320, true, false),
    (0x407c_2000_0000_03c1, false, false),
    (0x407e_0000_0000_0000, false, false),
    (0x407f_e000_0000_03c1, true, false),
    (0x4080_e000_0000_0000, true, false),
    (0x4082_c000_0000_0000, true, false),
];
const OUTPUT_ROWS: &[bool] = &[
    true, true, true, false, false, false, false, true, true, true, true, false, false, false,
    false, false, false, false, false, true, true, true,
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("oce-cxf is below repository root")
        .to_path_buf()
}
fn fixture(name: &str) -> PathBuf {
    repository_root()
        .join("crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle")
        .join(name)
}
fn fixture_relative(name: &str) -> String {
    format!("crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/{name}")
}
fn checked_manifest() -> schema::Manifest {
    let bytes = safe_read::read(&repository_root(), &fixture_relative("manifest.json"))
        .expect("checked Toggle manifest is bounded");
    manifest::parse(&bytes).expect("checked Toggle manifest parses")
}
fn read_manifest(path: &Path) -> Result<schema::Manifest, String> {
    let bytes = canonicalizer::read_bounded_path(path).map_err(|error| error.to_string())?;
    manifest::parse(&bytes)
}
fn assert_schedule(rows: &[BooleanRow]) {
    let observed = rows
        .iter()
        .map(|row| (row.time.to_bits(), row.u, row.clr))
        .collect::<Vec<_>>();
    assert_eq!(observed, INPUT_ROWS, "post-event Toggle schedule drifted");
}

#[test]
fn checked_toggle_runs_reproduce_projection_and_schedule() {
    let root = repository_root();
    let first_bytes = safe_read::read(&root, &fixture_relative("toggle-run-a.raw.csv")).unwrap();
    let second_bytes = safe_read::read(&root, &fixture_relative("toggle-run-b.raw.csv")).unwrap();
    let first =
        canonicalizer::canonicalize_bytes(&first_bytes, "openmodelica_logical_toggle").unwrap();
    let second =
        canonicalizer::canonicalize_bytes(&second_bytes, "openmodelica_logical_toggle").unwrap();
    assert_eq!(first.raw_rows.len(), 34);
    assert_eq!(first.rows.len(), 22);
    assert_eq!(first.group_sizes, GROUP_SIZES);
    assert_eq!(first, second);
    assert_schedule(&first.rows);
    assert_eq!(
        first.rows.iter().map(|row| row.y).collect::<Vec<_>>(),
        OUTPUT_ROWS
    );
    assert_eq!(
        first.bytes,
        canonicalizer::read_bounded_path(&fixture("toggle.canonical.csv")).unwrap()
    );
}

#[test]
fn latch_control_uses_identical_schedule_and_first_differs_at_row_three() {
    let root = repository_root();
    let toggle_bytes = safe_read::read(&root, &fixture_relative("toggle-run-a.raw.csv")).unwrap();
    let latch_bytes = safe_read::read(&root, &fixture_relative("latch.raw.csv")).unwrap();
    let toggle =
        canonicalizer::canonicalize_bytes(&toggle_bytes, "openmodelica_logical_toggle").unwrap();
    let latch =
        canonicalizer::canonicalize_bytes(&latch_bytes, "openmodelica_logical_latch").unwrap();
    assert_eq!(latch.raw_rows.len(), 34);
    assert_eq!(latch.rows.len(), 22);
    assert_eq!(latch.group_sizes, GROUP_SIZES);
    assert_schedule(&latch.rows);
    assert_eq!(
        latch.bytes,
        canonicalizer::read_bounded_path(&fixture("latch.canonical.csv")).unwrap()
    );
    let first = toggle
        .rows
        .iter()
        .zip(&latch.rows)
        .position(|(toggle, latch)| toggle.y != latch.y);
    assert_eq!(first, Some(3));
    assert_eq!(latch.rows[3].time.to_bits(), INPUT_ROWS[3].0);
}

#[cfg(unix)]
#[test]
fn keep_first_projection_is_compiled_and_fails_the_post_event_schedule() {
    let root = repository_root();
    let temporary = ClaimedTempDir::new("oce-toggle-projection-mutation");
    let temporary = temporary.path();
    let tool = temporary.join("repo/tools/openmodelica-toggle-reference");
    let module = temporary.join("repo/crates/oce-cxf/tests/open_modelica_toggle_reference");
    std::fs::create_dir_all(tool.join("src")).unwrap();
    std::fs::create_dir_all(&module).unwrap();
    for name in ["Cargo.toml", "Cargo.lock"] {
        std::fs::copy(
            root.join("tools/openmodelica-toggle-reference").join(name),
            tool.join(name),
        )
        .unwrap();
    }
    std::fs::copy(
        root.join("tools/openmodelica-toggle-reference/src/main.rs"),
        tool.join("src/main.rs"),
    )
    .unwrap();
    std::fs::copy(
        root.join("crates/oce-cxf/tests/open_modelica_toggle_reference/canonicalizer_tests.rs"),
        module.join("canonicalizer_tests.rs"),
    )
    .unwrap();
    let source_path =
        root.join("crates/oce-cxf/tests/open_modelica_toggle_reference/canonicalizer.rs");
    let original = std::fs::read_to_string(&source_path).unwrap();
    let keep_last = "            *rows.last_mut().expect(\"equal-time group exists\") = row;\n            *group_sizes.last_mut().expect(\"equal-time size exists\") += 1;";
    let keep_first = "            *group_sizes.last_mut().expect(\"equal-time size exists\") += 1;";
    assert_eq!(original.matches(keep_last).count(), 1);
    std::fs::write(
        module.join("canonicalizer.rs"),
        original.replace(keep_last, keep_first),
    )
    .unwrap();

    let output_path = temporary.join("mutated.canonical.csv");
    let metadata_path = temporary.join("mutated.metadata");
    let run = std::process::Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            tool.join("Cargo.toml").to_str().unwrap(),
            "--offline",
            "--locked",
            "--",
            "canonicalize-inspect",
            fixture("toggle-run-a.raw.csv").to_str().unwrap(),
            output_path.to_str().unwrap(),
            "openmodelica_logical_toggle_keep_first",
            metadata_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "mutated tool failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let metadata = std::fs::read_to_string(metadata_path).unwrap();
    for expected in [
        "raw_rows=34",
        "canonical_rows=22",
        "group_sizes=1,2,1,2,1,2,1,2,1,2,1,2,2,1,2,1,2,2,1,2,1,2",
        "canonical_time_bits=0000000000000000,403e000000001dff,404e000000000000,4056800000000780,405e000000000000,4062c000000003c1,4066800000000000,406a4000000003c1,406e000000000000,4070e000000001e0,4072c00000000000,4073600000000320,4075e000000002d0,4076800000000000,40786000000003c1,407a400000000000,407ae00000000320,407c2000000003c1,407e000000000000,407fe000000003c1,4080e00000000000,4082c00000000000",
    ] {
        assert!(metadata.lines().any(|line| line == expected));
    }
    let expected_raw_bits = INPUT_ROWS
        .iter()
        .zip(GROUP_SIZES)
        .flat_map(|((bits, _, _), size)| std::iter::repeat_n(format!("{bits:016x}"), *size))
        .collect::<Vec<_>>()
        .join(",");
    assert!(
        metadata
            .lines()
            .any(|line| line == format!("raw_time_bits={expected_raw_bits}")),
        "keep-first mutation changed raw timestamp bits"
    );
    let actual = std::fs::read_to_string(output_path)
        .unwrap()
        .lines()
        .skip(3)
        .map(|line| {
            let cells = line.split_ascii_whitespace().collect::<Vec<_>>();
            (cells[1] == "1.0", cells[2] == "1.0")
        })
        .collect::<Vec<_>>();
    let expected = INPUT_ROWS
        .iter()
        .map(|(_, u, clr)| (*u, *clr))
        .collect::<Vec<_>>();
    let mismatches = actual
        .iter()
        .zip(&expected)
        .enumerate()
        .filter_map(|(row, (actual, expected))| (actual != expected).then_some(row))
        .collect::<Vec<_>>();
    assert_eq!(mismatches, [1, 3, 5, 7, 9, 11, 12, 14, 16, 17, 19]);
    assert_eq!(std::fs::read_to_string(source_path).unwrap(), original);
}

#[cfg(unix)]
struct ClaimedTempDir {
    path: PathBuf,
    device: u64,
    inode: u64,
}

#[cfg(unix)]
impl ClaimedTempDir {
    fn new(prefix: &str) -> Self {
        let root = std::env::temp_dir()
            .canonicalize()
            .expect("temporary root resolves");
        for nonce in 0_u32..4096 {
            let candidate = root.join(format!("{prefix}-{}-{nonce}", std::process::id()));
            match std::fs::DirBuilder::new().mode(0o700).create(&candidate) {
                Ok(()) => {
                    let metadata = std::fs::symlink_metadata(&candidate).unwrap();
                    return Self {
                        path: candidate,
                        device: metadata.dev(),
                        inode: metadata.ino(),
                    };
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot claim mutation directory: {error}"),
            }
        }
        panic!("cannot claim a unique mutation directory")
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for ClaimedTempDir {
    fn drop(&mut self) {
        let metadata = std::fs::symlink_metadata(&self.path).expect("stat mutation directory");
        assert!(!metadata.file_type().is_symlink());
        assert_eq!((metadata.dev(), metadata.ino()), (self.device, self.inode));
        std::fs::remove_dir_all(&self.path).expect("remove claimed mutation directory");
    }
}

#[test]
fn wrappers_are_one_token_substitutions() {
    let root = repository_root();
    let toggle = std::fs::read_to_string(
        root.join("tools/openmodelica-toggle-reference/toggle/TogglePilot.mo"),
    )
    .unwrap();
    let latch = std::fs::read_to_string(
        root.join("tools/openmodelica-toggle-reference/toggle/LatchPilot.mo"),
    )
    .unwrap();
    let token = "Buildings.Controls.OBC.CDL.Logical.Toggle";
    assert_eq!(toggle.matches(token).count(), 1);
    assert_eq!(
        toggle.replace(token, "Buildings.Controls.OBC.CDL.Logical.Latch"),
        latch
    );
}

#[test]
fn closed_manifest_paths_digests_logs_and_oci_graph_validate() {
    let parsed = checked_manifest();
    repository::validate(&parsed, &repository_root()).expect("Toggle evidence graph validates");
    assert_eq!(parsed, checked_manifest());
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
        "  \"format\": \"oce-openmodelica-toggle-external-run-v1\",\n",
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
    let rebound = original.replacen("toggle.canonical.csv", "alternate.canonical.csv", 1);
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
    let long = original.replacen(
        "\"CDL.Logical.Toggle\"",
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
fn scoped_test_and_tool_add_no_public_surface() {
    let root = repository_root();
    for source in [
        root.join("crates/oce-conformance/tests/open_modelica_toggle.rs"),
        root.join("crates/oce-conformance/tests/open_modelica_toggle/support.rs"),
    ] {
        let text = std::fs::read_to_string(source).unwrap();
        for forbidden in [
            "known_divergence",
            "TierReport",
            "ConformanceReport",
            "assemble_report",
        ] {
            assert!(!text.contains(forbidden));
        }
        assert!(!text.lines().any(|line| line.starts_with("pub ")));
    }
    let workspace = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(!workspace.contains("tools/openmodelica-toggle-reference"));
    let tool = std::fs::read_to_string(root.join("tools/openmodelica-toggle-reference/Cargo.toml"))
        .unwrap();
    assert!(tool.contains("[workspace]"));
    assert!(!tool.contains("[dependencies]"));
    assert!(
        !root
            .join("tools/openmodelica-toggle-reference/build.rs")
            .exists()
    );
    let public_surface =
        std::fs::read_to_string(root.join("crates/oce-conformance/src/lib.rs")).unwrap();
    assert!(!public_surface.contains("open_modelica_toggle"));
}

#[test]
fn package_lists_include_evidence_and_exclude_the_tool() {
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
            "cargo package --list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let listing = String::from_utf8(output.stdout).unwrap();
        if package == "oce-cxf" {
            assert!(
                listing
                    .lines()
                    .any(|line| line == "tests/open_modelica_toggle_reference/canonicalizer.rs")
            );
        } else {
            assert!(listing.lines().any(|line| line == "tests/fixtures/open_modelica/logical_toggle/manifest.json"));
            for artifact in &parsed.artifacts {
                if let Some(required) = artifact.path.strip_prefix("crates/oce-conformance/") {
                    assert!(
                        listing.lines().any(|line| line == required),
                        "package omits {required}"
                    );
                }
            }
        }
        assert!(!listing.contains("tools/openmodelica-toggle-reference"));
    }
}

#[test]
fn regeneration_is_pinned_bounded_and_offline() {
    let root = repository_root();
    let regeneration = std::fs::read_to_string(
        root.join("tools/openmodelica-toggle-reference/toggle/regenerate.sh"),
    )
    .unwrap();
    let runner =
        std::fs::read_to_string(root.join("tools/openmodelica-toggle-reference/toggle/runner.sh"))
            .unwrap();
    for required in [
        "IMAGE=\"$IMAGE_REPOSITORY@$INDEX\"",
        "test \"$HOST_UID\" -ne 0",
        "deadline_call docker run -d --cidfile \"$ACTIVE_CID_FILE\"",
        "monotonic_seconds",
        "--tmpfs \"/out:rw,exec,nosuid,nodev,size=256m",
        "docker exec \"$ACTIVE_CID\" cat /out/TogglePilot_res.csv",
        "grep -Fqx 'runner_complete=1' \"$runner_log\"",
        "python3 \"$PUBLISH_HELPER\" cleanup \"$OUTPUT_PRIVATE\" \"$OUTPUT_DEVICE\" \"$OUTPUT_INODE\"",
        "CONTAINER_PREFIX=${OUTPUT_TOKEN#.}",
        "--label \"oce.toggle.run=$RUN_LABEL\"",
        ". \"$SCRIPT_DIR/container_cleanup.sh\"",
        "\"$IMAGE\" /reference/runner.sh",
        "arm64 | aarch64) HOST_ARCHITECTURE=arm64",
        "mv \"$OUTPUT/run-a/TogglePilot_res.csv\" \"$OUTPUT/toggle-run-a.raw.csv\"",
        "mv \"$OUTPUT/semantic-control/run.log\" \"$OUTPUT/latch.log\"",
        "projection-mutation.log",
        "manifest.json",
        "python3 \"$SCRIPT_DIR/verify_evidence.py\" \"$OUTPUT\" \"$REPO_ROOT\"",
        "cleanup_container \"$RUN_LABEL\"",
        "valid_container_id \"$ACTIVE_CID\"",
        "trap '' HUP INT TERM",
    ] {
        assert!(
            regeneration.contains(required),
            "regeneration omits {required}"
        );
    }
    assert!(!regeneration.contains("\"$IMAGE_TAG\" /reference/runner.sh"));
    assert!(!regeneration.contains("docker rm -f \"$RUN_"));
    assert!(!regeneration.contains("docker rm -f \"$RUN"));
    assert!(!regeneration.contains("docker kill \"$container\""));
    assert!(!regeneration.contains("docker rm -f \"$RUN_A_CONTAINER\""));
    for required in [
        "simflags=\"\"",
        "variableFilter=\"^(u|clr|y)$\"",
        "grep -Fq 'resultFile = \"/out/TogglePilot_res.csv\",'",
        "grep -Fq 'The simulation finished successfully.'",
        "touch /out/.oce-complete",
    ] {
        assert!(runner.contains(required), "runner omits {required}");
    }
    assert!(!runner.contains("-noEventEmit"));
}

#[cfg(unix)]
#[test]
fn deadline_regression_runs_in_the_structural_suite() {
    let root = repository_root();
    let output = std::process::Command::new("sh")
        .arg(root.join("tools/openmodelica-toggle-reference/toggle/deadline_test.sh"))
        .output()
        .expect("execute deadline regression");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"deadline accounting test passed\n");
}

#[cfg(unix)]
#[test]
fn output_publication_regression_runs_in_the_structural_suite() {
    let root = repository_root();
    let output = std::process::Command::new("sh")
        .arg(root.join("tools/openmodelica-toggle-reference/toggle/output_publish_test.sh"))
        .output()
        .expect("execute output publication regression");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"output publication test passed\n");
}

#[cfg(unix)]
#[test]
fn container_cleanup_regression_runs_in_the_structural_suite() {
    let root = repository_root();
    let output = std::process::Command::new("sh")
        .arg(root.join("tools/openmodelica-toggle-reference/toggle/container_cleanup_test.sh"))
        .output()
        .expect("execute container cleanup regression");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"container cleanup test passed\n");
}

#[cfg(unix)]
#[test]
fn checked_manifest_reader_rejects_symlink_and_fifo_without_blocking() {
    use std::os::unix::fs::symlink;

    let temporary = ClaimedTempDir::new("oce-toggle-manifest-path");
    let regular = temporary.path().join("manifest.json");
    let link = temporary.path().join("manifest-link.json");
    let fifo = temporary.path().join("manifest.fifo");
    std::fs::copy(fixture("manifest.json"), &regular).unwrap();
    symlink(&regular, &link).unwrap();
    assert!(read_manifest(&link).unwrap_err().contains("non-symlink"));
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert!(read_manifest(&fifo).unwrap_err().contains("regular"));
}
