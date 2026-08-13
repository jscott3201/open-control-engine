//! Bounded parsing and closed literal validation for the Toggle manifest.

use super::schema::Manifest;

pub(super) const MAX_MANIFEST_BYTES: usize = 256 * 1024;
pub(super) const ROLES: &[&str] = &[
    "canonical_csv",
    "canonicalizer_source",
    "image_index_json",
    "image_manifest_json",
    "projection_mutation_log",
    "raw_run_a_csv",
    "raw_run_b_csv",
    "regeneration_script",
    "run_a_log",
    "run_b_log",
    "runner_script",
    "semantic_control_canonical_csv",
    "semantic_control_log",
    "semantic_control_raw_csv",
    "semantic_control_wrapper_model",
    "tool_cargo_lock",
    "tool_cargo_toml",
    "tool_main_source",
    "evidence_validator_script",
    "manifest_generator_script",
    "deadline_script",
    "deadline_test_script",
    "output_publish_script",
    "output_publish_test_script",
    "container_cleanup_script",
    "container_cleanup_test_script",
    "wrapper_model",
];
pub(super) const PATHS: &[&str] = &[
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/toggle.canonical.csv",
    "crates/oce-cxf/tests/open_modelica_toggle_reference/canonicalizer.rs",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/image-index.json",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/image-manifest.json",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/projection-mutation.log",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/toggle-run-a.raw.csv",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/toggle-run-b.raw.csv",
    "tools/openmodelica-toggle-reference/toggle/regenerate.sh",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/run-a.log",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/run-b.log",
    "tools/openmodelica-toggle-reference/toggle/runner.sh",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/latch.canonical.csv",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/latch.log",
    "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/latch.raw.csv",
    "tools/openmodelica-toggle-reference/toggle/LatchPilot.mo",
    "tools/openmodelica-toggle-reference/Cargo.lock",
    "tools/openmodelica-toggle-reference/Cargo.toml",
    "tools/openmodelica-toggle-reference/src/main.rs",
    "tools/openmodelica-toggle-reference/toggle/verify_evidence.py",
    "tools/openmodelica-toggle-reference/toggle/generate_manifest.py",
    "tools/openmodelica-toggle-reference/toggle/deadline.sh",
    "tools/openmodelica-toggle-reference/toggle/deadline_test.sh",
    "tools/openmodelica-toggle-reference/toggle/output_publish.py",
    "tools/openmodelica-toggle-reference/toggle/output_publish_test.sh",
    "tools/openmodelica-toggle-reference/toggle/container_cleanup.sh",
    "tools/openmodelica-toggle-reference/toggle/container_cleanup_test.sh",
    "tools/openmodelica-toggle-reference/toggle/TogglePilot.mo",
];
pub(super) const TIME_BITS: &[&str] = &[
    "0000000000000000",
    "403e000000001dff",
    "404e000000000000",
    "4056800000000780",
    "405e000000000000",
    "4062c000000003c1",
    "4066800000000000",
    "406a4000000003c1",
    "406e000000000000",
    "4070e000000001e0",
    "4072c00000000000",
    "4073600000000320",
    "4075e000000002d0",
    "4076800000000000",
    "40786000000003c1",
    "407a400000000000",
    "407ae00000000320",
    "407c2000000003c1",
    "407e000000000000",
    "407fe000000003c1",
    "4080e00000000000",
    "4082c00000000000",
];

pub(super) fn parse(input: &[u8]) -> Result<Manifest, String> {
    if input.len() > MAX_MANIFEST_BYTES {
        return Err("manifest exceeds 256 KiB".into());
    }
    scan_json_strings(input)?;
    let manifest: Manifest = serde_json::from_slice(input).map_err(|error| error.to_string())?;
    validate_literals(&manifest)?;
    Ok(manifest)
}

fn scan_json_strings(input: &[u8]) -> Result<(), String> {
    let mut in_string = false;
    let mut escaped = false;
    let mut length = 0_usize;
    for &byte in input {
        if !in_string {
            if byte == b'"' {
                in_string = true;
                length = 0;
            }
            continue;
        }
        if escaped {
            escaped = false;
            length += 1;
        } else if byte == b'\\' {
            escaped = true;
            length += 1;
        } else if byte == b'"' {
            in_string = false;
        } else {
            length += 1;
        }
        if length > 4096 {
            return Err("manifest string exceeds 4096 UTF-8 bytes".into());
        }
    }
    Ok(())
}

fn validate_literals(manifest: &Manifest) -> Result<(), String> {
    exact(
        &manifest.format,
        "oce-openmodelica-toggle-external-run-v1",
        "format",
    )?;
    let scope = &manifest.scope;
    exact(&scope.class, "CDL.Logical.Toggle", "scope.class")?;
    exact(
        &scope.scenario,
        "repeated_rises_initial_true_and_clear_priority",
        "scope.scenario",
    )?;
    exact_slice(&scope.inputs, &["u", "clr"], "scope.inputs")?;
    exact(&scope.output, "y", "scope.output")?;
    exact(&scope.comparison, "exact_boolean", "scope.comparison")?;
    exact(
        &scope.global_tier3_status,
        "skipped",
        "scope.global_tier3_status",
    )?;
    validate_image(manifest)?;
    validate_sources(manifest)?;

    let simulation = &manifest.simulation;
    exact(&simulation.method, "dassl", "simulation.method")?;
    exact(&simulation.start_time, "0", "simulation.start_time")?;
    exact(&simulation.stop_time, "600", "simulation.stop_time")?;
    exact(&simulation.tolerance, "1e-9", "simulation.tolerance")?;
    exact(&simulation.output_format, "csv", "simulation.output_format")?;
    exact(
        &simulation.variable_filter,
        "^(u|clr|y)$",
        "simulation.variable_filter",
    )?;
    exact(&simulation.simflags, "", "simulation.simflags")?;
    exact(
        &simulation.raw_header,
        "\"time\",\"clr\",\"u\",\"y\"",
        "simulation.raw_header",
    )?;
    require(
        simulation.number_of_intervals == 10 && simulation.event_emission,
        "simulation literals",
    )?;

    let projection = &manifest.projection;
    exact_slice(
        &projection.columns,
        &["time", "u", "clr", "y"],
        "projection.columns",
    )?;
    exact(
        &projection.grouping,
        "contiguous_equal_f64_bits",
        "projection.grouping",
    )?;
    exact(&projection.selection, "last", "projection.selection")?;
    require(
        !projection.normalize_times && projection.raw_rows == 34 && projection.canonical_rows == 22,
        "projection counts",
    )?;
    require(
        projection.group_sizes
            == [
                1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 2, 1, 2, 1, 2, 2, 1, 2, 1, 2,
            ],
        "projection.group_sizes",
    )?;
    exact_slice(
        &projection.canonical_time_bits,
        TIME_BITS,
        "projection.canonical_time_bits",
    )?;

    require(manifest.runs.len() == 2, "runs length")?;
    for (run, id, token, log, raw) in [
        (
            &manifest.runs[0],
            "run-a",
            "fresh-run-a",
            PATHS[8],
            PATHS[5],
        ),
        (
            &manifest.runs[1],
            "run-b",
            "fresh-run-b",
            PATHS[9],
            PATHS[6],
        ),
    ] {
        exact(&run.id, id, "run.id")?;
        exact(
            &run.output_directory_token,
            token,
            "run.output_directory_token",
        )?;
        exact(&run.log_path, log, "run.log_path")?;
        exact(&run.raw_path, raw, "run.raw_path")?;
        digest(&run.log_sha256)?;
        exact(
            &run.raw_sha256,
            "cf9333debedf335ae28103b6bcc70229ac78afba4d99701a51d1771d9a3e6641",
            "run.raw_sha256",
        )?;
    }
    require(
        manifest.runs[0].raw_sha256 == manifest.runs[1].raw_sha256,
        "run raw digests differ",
    )?;

    let semantic = &manifest.semantic_control;
    exact(
        &semantic.substitution_class,
        "CDL.Logical.Latch",
        "semantic_control.substitution_class",
    )?;
    exact(
        &semantic.first_mismatch_time_bits,
        TIME_BITS[3],
        "semantic_control.first_mismatch_time_bits",
    )?;
    exact(
        &semantic.expected_comparison,
        "exact_mismatch",
        "semantic_control.expected_comparison",
    )?;
    require(
        semantic.first_mismatch_row == 3
            && semantic.raw_rows == 34
            && semantic.canonical_rows == 22,
        "semantic control literals",
    )?;

    require(manifest.artifacts.len() == ROLES.len(), "artifact count")?;
    for ((artifact, role), path) in manifest.artifacts.iter().zip(ROLES).zip(PATHS) {
        exact(&artifact.role, role, "artifact role order")?;
        exact(&artifact.path, path, "artifact role path")?;
        digest(&artifact.sha256)?;
    }
    validate_regeneration(manifest)
}

fn validate_image(manifest: &Manifest) -> Result<(), String> {
    let image = &manifest.image;
    for (actual, expected, field) in [
        (
            &image.repository,
            "openmodelica/openmodelica",
            "image.repository",
        ),
        (&image.tag, "v1.25.1-minimal", "image.tag"),
        (
            &image.index_digest,
            "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864",
            "image.index_digest",
        ),
        (
            &image.platform_manifest_digest,
            "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4",
            "image.platform_manifest_digest",
        ),
        (
            &image.config_digest,
            "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666",
            "image.config_digest",
        ),
        (&image.platform, "linux/arm64", "image.platform"),
        (&image.host_architecture, "arm64", "image.host_architecture"),
        (
            &image.docker_server_architecture,
            "aarch64",
            "image.docker_server_architecture",
        ),
        (
            &image.omc_version,
            "OpenModelica 1.25.1",
            "image.omc_version",
        ),
        (&image.gcc_version, "11.4.0", "image.gcc_version"),
        (&image.binutils_version, "2.38", "image.binutils_version"),
        (&image.glibc_version, "2.35", "image.glibc_version"),
    ] {
        exact(actual, expected, field)?;
    }
    Ok(())
}

fn validate_sources(manifest: &Manifest) -> Result<(), String> {
    require(manifest.sources.len() == 2, "sources length")?;
    let buildings = &manifest.sources[0];
    let modelica = &manifest.sources[1];
    for (source, values) in [
        (
            buildings,
            (
                "buildings",
                "https://github.com/lbl-srg/modelica-buildings.git",
                "a131864e4c4df22ebcd52bb8da439de0087ac365",
                "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09",
                "Buildings",
                "14.0.0",
            ),
        ),
        (
            modelica,
            (
                "modelica",
                "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git",
                "7a4bf7de77a3986e8eb1e88cbb515d646f78f834",
                "43d7d8fc1a991358e9e5e91976e27cdc4280173f",
                "Modelica",
                "4.1.0",
            ),
        ),
    ] {
        exact(&source.name, values.0, "source.name")?;
        exact(&source.repository, values.1, "source.repository")?;
        exact(&source.commit, values.2, "source.commit")?;
        exact(&source.tree, values.3, "source.tree")?;
        exact(&source.package, values.4, "source.package")?;
        exact(&source.version, values.5, "source.version")?;
    }
    require(
        buildings.files.len() == 3 && modelica.files.len() == 4,
        "source file counts",
    )?;
    let expected = [
        (
            "Buildings/package.mo",
            "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59",
        ),
        (
            "Buildings/Controls/OBC/CDL/Logical/Toggle.mo",
            "49d33a88242163def412dff83e3f49e23b187fe56859a01649edbf5fb2bab60c",
        ),
        (
            "Buildings/Controls/OBC/CDL/Logical/Latch.mo",
            "2ab7e15a0026d5a6c0089d7af88329869fc3e397398a7b86ad5ea49fd1d9b271",
        ),
        (
            "Complex.mo",
            "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f",
        ),
        (
            "Modelica/package.mo",
            "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
        ),
        (
            "Modelica/Blocks/Sources.mo",
            "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3",
        ),
        (
            "ModelicaServices/package.mo",
            "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb",
        ),
    ];
    for (file, (path, sha256)) in buildings.files.iter().chain(&modelica.files).zip(expected) {
        exact(&file.path, path, "source file path")?;
        exact(&file.sha256, sha256, "source file digest")?;
    }
    Ok(())
}

fn validate_regeneration(manifest: &Manifest) -> Result<(), String> {
    let value = &manifest.regeneration;
    for (actual, expected, field) in [
        (
            &value.entrypoint,
            "tools/openmodelica-toggle-reference/toggle/regenerate.sh",
            "regeneration.entrypoint",
        ),
        (&value.network, "none", "regeneration.network"),
        (&value.pull, "never", "regeneration.pull"),
        (&value.platform, "linux/arm64", "regeneration.platform"),
        (
            &value.source_materialization,
            "git_archive",
            "regeneration.source_materialization",
        ),
        (
            &value.source_mounts,
            "read_only",
            "regeneration.source_mounts",
        ),
        (
            &value.container_root,
            "read_only",
            "regeneration.container_root",
        ),
        (
            &value.container_user,
            "non_root",
            "regeneration.container_user",
        ),
        (&value.capabilities, "none", "regeneration.capabilities"),
        (&value.cpus, "4", "regeneration.cpus"),
    ] {
        exact(actual, expected, field)?;
    }
    require(
        value.no_new_privileges && value.device_mounts == 0 && !value.docker_socket_mounted,
        "regeneration isolation",
    )?;
    require(
        value.timeout_seconds == 120
            && value.memory_bytes == 2_147_483_648
            && value.memory_swap_bytes == 2_147_483_648,
        "regeneration memory/time",
    )?;
    require(
        value.pids_limit == 256
            && value.tmpfs_bytes == 268_435_456
            && value.per_file_bytes == 67_108_864
            && value.output_directory_bytes == 268_435_456,
        "regeneration bounds",
    )
}

fn exact(actual: &str, expected: &str, field: &str) -> Result<(), String> {
    require(actual == expected, &format!("unsupported {field}"))
}
fn exact_slice(actual: &[String], expected: &[&str], field: &str) -> Result<(), String> {
    require(
        actual
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied()),
        &format!("unsupported {field}"),
    )
}
fn digest(value: &str) -> Result<(), String> {
    require(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid SHA-256",
    )
}
fn require(condition: bool, detail: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(detail.into())
    }
}
