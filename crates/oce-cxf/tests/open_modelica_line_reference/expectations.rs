//! Fixed bit tables, identities, and artifact-role closure for Line evidence.

pub(super) const RAW_SHA: &str = "52691be2b8ed547f2f4d5c0b3efefb71d7d273bb9e48c43636bddb82b8247984";
pub(super) const CONTROL_SHA: &str =
    "e5cce191651ce4392d87da30fbab48c86032b7b9cab483ce914c33b45f6b0925";
pub(super) const CANONICAL_SHA: &str =
    "c6bcc946b3f029efe6bed13c32cb8ed2f558012576d5a900bd3feb56d8845c22";
pub(super) const CONTROL_CANONICAL_SHA: &str =
    "f006653ab32d503622ef8ca90bfc5fd6faeb40f0d4ffe9144d9d9f24e5e54d04";
pub(super) const TIME_BITS: &[&str] = &[
    "0000000000000000",
    "404e000000000000",
    "404e000000000eff",
    "405e000000000000",
    "405e000000000781",
    "4066800000000000",
    "40668000000003c1",
    "406e000000000000",
    "406e0000000003c1",
    "4072c00000000000",
];
pub(super) const U_BITS: &[&str] = &[
    "c010000000000000",
    "c010000000000000",
    "c000000000000000",
    "c000000000000000",
    "0000000000000000",
    "0000000000000000",
    "4000000000000000",
    "4000000000000000",
    "4010000000000000",
    "4010000000000000",
];
pub(super) const Y_BOTH: &[&str] = &[
    "3ff4000000000000",
    "3ff4000000000000",
    "3ff4000000000000",
    "3ff4000000000000",
    "4002000000000000",
    "4002000000000000",
    "400a000000000000",
    "400a000000000000",
    "400a000000000000",
    "400a000000000000",
];
pub(super) const Y_BELOW: &[&str] = &[
    "3ff4000000000000",
    "3ff4000000000000",
    "3ff4000000000000",
    "3ff4000000000000",
    "4002000000000000",
    "4002000000000000",
    "400a000000000000",
    "400a000000000000",
    "4011000000000000",
    "4011000000000000",
];
pub(super) const Y_ABOVE: &[&str] = &[
    "3fd0000000000000",
    "3fd0000000000000",
    "3ff4000000000000",
    "3ff4000000000000",
    "4002000000000000",
    "4002000000000000",
    "400a000000000000",
    "400a000000000000",
    "400a000000000000",
    "400a000000000000",
];
pub(super) const Y_UNLIMITED: &[&str] = &[
    "3fd0000000000000",
    "3fd0000000000000",
    "3ff4000000000000",
    "3ff4000000000000",
    "4002000000000000",
    "4002000000000000",
    "400a000000000000",
    "400a000000000000",
    "4011000000000000",
    "4011000000000000",
];

pub(super) fn expected_artifacts() -> Vec<(String, String)> {
    let fixture = "crates/oce-conformance/tests/fixtures/open_modelica/reals_line/";
    let mut values = vec![
        (
            "image_index_json".into(),
            format!("{fixture}image-index.json"),
        ),
        (
            "cross_architecture_log".into(),
            format!("{fixture}cross-architecture.log"),
        ),
    ];
    let files = [
        ("architecture_record", "architecture.json"),
        ("canonical_csv", "line.canonical.csv"),
        ("raw_run_a_csv", "line-run-a.raw.csv"),
        ("raw_run_b_csv", "line-run-b.raw.csv"),
        ("run_a_log", "run-a.log"),
        ("run_b_log", "run-b.log"),
        ("flag_control_canonical_csv", "flag-control.canonical.csv"),
        ("flag_control_raw_csv", "flag-control.raw.csv"),
        ("flag_control_log", "flag-control.log"),
        ("projection_mutation_log", "projection-mutation.log"),
        ("architecture_image_index_json", "image-index.json"),
        ("platform_image_manifest_json", "image-manifest.json"),
    ];
    for architecture in ["arm64", "amd64"] {
        values.extend(files.iter().map(|(role, file)| {
            (
                format!("{architecture}_{role}"),
                format!("{fixture}{architecture}/{file}"),
            )
        }));
    }
    let tracked = [
        (
            "canonicalizer_source",
            "crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs",
        ),
        (
            "tool_cargo_lock",
            "tools/openmodelica-line-reference/Cargo.lock",
        ),
        (
            "tool_cargo_toml",
            "tools/openmodelica-line-reference/Cargo.toml",
        ),
        (
            "tool_main_source",
            "tools/openmodelica-line-reference/src/main.rs",
        ),
        (
            "wrapper_model",
            "tools/openmodelica-line-reference/line/LinePilot.mo",
        ),
        (
            "flag_control_wrapper_model",
            "tools/openmodelica-line-reference/line/LineFlagPilot.mo",
        ),
        (
            "runner_script",
            "tools/openmodelica-line-reference/line/runner.sh",
        ),
        (
            "regeneration_script",
            "tools/openmodelica-line-reference/line/regenerate.sh",
        ),
        (
            "assembly_script",
            "tools/openmodelica-line-reference/line/assemble.sh",
        ),
        (
            "manifest_generator_script",
            "tools/openmodelica-line-reference/line/generate_manifest.py",
        ),
        (
            "architecture_generator_script",
            "tools/openmodelica-line-reference/line/generate_architecture.py",
        ),
        (
            "evidence_validator_script",
            "tools/openmodelica-line-reference/line/verify_evidence.py",
        ),
        (
            "safe_file_helper_script",
            "tools/openmodelica-line-reference/line/safe_files.py",
        ),
        (
            "oci_materializer_script",
            "tools/openmodelica-line-reference/line/materialize_oci.py",
        ),
        (
            "deadline_script",
            "tools/openmodelica-line-reference/line/deadline.sh",
        ),
        (
            "deadline_test_script",
            "tools/openmodelica-line-reference/line/deadline_test.sh",
        ),
        (
            "output_publish_script",
            "tools/openmodelica-line-reference/line/output_publish.py",
        ),
        (
            "output_publish_test_script",
            "tools/openmodelica-line-reference/line/output_publish_test.sh",
        ),
        (
            "container_cleanup_script",
            "tools/openmodelica-line-reference/line/container_cleanup.sh",
        ),
        (
            "container_cleanup_test_script",
            "tools/openmodelica-line-reference/line/container_cleanup_test.sh",
        ),
        (
            "oci_index_source",
            "tools/openmodelica-line-reference/line/image-index.json",
        ),
        (
            "arm64_manifest_source",
            "tools/openmodelica-line-reference/line/image-manifest-arm64.json",
        ),
        (
            "amd64_manifest_source",
            "tools/openmodelica-line-reference/line/image-manifest-amd64.json",
        ),
        (
            "evidence_workflow",
            ".github/workflows/openmodelica-line-evidence.yml",
        ),
    ];
    values.extend(
        tracked
            .into_iter()
            .map(|(role, path)| (role.into(), path.into())),
    );
    values
}
