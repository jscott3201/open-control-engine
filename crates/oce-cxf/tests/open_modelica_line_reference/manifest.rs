//! Bounded parsing and fixed-literal validation for the Line manifest.

use super::expectations::*;
use super::schema::{Architecture, Manifest};

pub(super) const MAX_MANIFEST_BYTES: usize = 256 * 1024;

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

fn validate_literals(value: &Manifest) -> Result<(), String> {
    exact(
        &value.format,
        "oce-openmodelica-line-external-run-v1",
        "format",
    )?;
    let scope = &value.scope;
    exact(&scope.class, "CDL.Reals.Line", "scope.class")?;
    exact(
        &scope.scenario,
        "four_limit_modes_five_dyadic_regions",
        "scope.scenario",
    )?;
    exact_slice(
        &scope.inputs,
        &["x1", "f1", "x2", "f2", "u"],
        "scope.inputs",
    )?;
    exact_slice(
        &scope.outputs,
        &["yBoth", "yBelow", "yAbove", "yUnlimited"],
        "scope.outputs",
    )?;
    exact(
        &scope.comparison,
        "exact_finite_f64_bits",
        "scope.comparison",
    )?;
    exact(
        &scope.global_tier3_status,
        "skipped",
        "scope.global_tier3_status",
    )?;
    validate_image(value)?;
    validate_sources(value)?;
    validate_simulation(value)?;
    validate_projection(value)?;
    validate_architectures(value)?;
    validate_controls(value)?;
    validate_artifacts(value)?;
    validate_regeneration(value)
}

fn validate_image(value: &Manifest) -> Result<(), String> {
    let image = &value.image;
    exact(
        &image.repository,
        "openmodelica/openmodelica",
        "image.repository",
    )?;
    exact(&image.tag, "v1.25.1-minimal", "image.tag")?;
    exact(
        &image.index_digest,
        "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864",
        "image.index_digest",
    )?;
    require(image.platforms.len() == 2, "image platform count")?;
    let expected = [
        (
            "linux/arm64",
            "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4",
            "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666",
        ),
        (
            "linux/amd64",
            "sha256:92d0779a01e7d43ed4d5ecb4cfd9754cb259b30673ddb454b5a32e3eb8665f11",
            "sha256:0c81120bb392de44cab0e9ff6818d0a44afad657d5b401f25e148fa6c26e5347",
        ),
    ];
    for (platform, expected) in image.platforms.iter().zip(expected) {
        exact(&platform.platform, expected.0, "image.platform")?;
        exact(
            &platform.manifest_digest,
            expected.1,
            "image.manifest_digest",
        )?;
        exact(&platform.config_digest, expected.2, "image.config_digest")?;
    }
    Ok(())
}

fn validate_sources(value: &Manifest) -> Result<(), String> {
    require(value.sources.len() == 2, "source count")?;
    let buildings = &value.sources[0];
    let modelica = &value.sources[1];
    for (source, expected) in [
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
        exact(&source.name, expected.0, "source.name")?;
        exact(&source.repository, expected.1, "source.repository")?;
        exact(&source.commit, expected.2, "source.commit")?;
        exact(&source.tree, expected.3, "source.tree")?;
        exact(&source.package, expected.4, "source.package")?;
        exact(&source.version, expected.5, "source.version")?;
    }
    let files = [
        (
            "Buildings/package.mo",
            "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59",
        ),
        (
            "Buildings/Controls/OBC/CDL/Reals/Line.mo",
            "85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5",
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
    require(
        buildings.files.len() == 2 && modelica.files.len() == 4,
        "source file counts",
    )?;
    for (file, expected) in buildings.files.iter().chain(&modelica.files).zip(files) {
        exact(&file.path, expected.0, "source file path")?;
        exact(&file.sha256, expected.1, "source file digest")?;
    }
    Ok(())
}

fn validate_simulation(value: &Manifest) -> Result<(), String> {
    let simulation = &value.simulation;
    for (actual, expected, field) in [
        (&simulation.method, "dassl", "simulation.method"),
        (&simulation.start_time, "0", "simulation.start_time"),
        (&simulation.stop_time, "300", "simulation.stop_time"),
        (&simulation.tolerance, "1e-9", "simulation.tolerance"),
        (&simulation.output_format, "csv", "simulation.output_format"),
        (
            &simulation.variable_filter,
            "^(x1|f1|x2|f2|u|yBoth|yBelow|yAbove|yUnlimited)$",
            "simulation.variable_filter",
        ),
        (&simulation.simflags, "", "simulation.simflags"),
        (
            &simulation.raw_header,
            super::canonicalizer::RAW_HEADER,
            "simulation.raw_header",
        ),
    ] {
        exact(actual, expected, field)?;
    }
    require(
        simulation.number_of_intervals == 5 && simulation.event_emission,
        "simulation fixed values",
    )
}

fn validate_projection(value: &Manifest) -> Result<(), String> {
    let projection = &value.projection;
    exact_slice(
        &projection.columns,
        &[
            "time",
            "x1",
            "f1",
            "x2",
            "f2",
            "u",
            "yBoth",
            "yBelow",
            "yAbove",
            "yUnlimited",
        ],
        "projection.columns",
    )?;
    exact(
        &projection.grouping,
        "contiguous_equal_f64_bits",
        "projection.grouping",
    )?;
    exact(&projection.selection, "last", "projection.selection")?;
    require(
        !projection.normalize_times && projection.raw_rows == 15 && projection.canonical_rows == 10,
        "projection counts",
    )?;
    require(
        projection.group_sizes == [1, 1, 2, 1, 2, 1, 2, 1, 2, 2],
        "projection group sizes",
    )?;
    exact_slice(
        &projection.canonical_time_bits,
        TIME_BITS,
        "projection time bits",
    )?;
    for (actual, expected, field) in [
        (
            &projection.canonical_input_bits.x1,
            ["c000000000000000"; 10].as_slice(),
            "input x1",
        ),
        (
            &projection.canonical_input_bits.f1,
            ["3ff4000000000000"; 10].as_slice(),
            "input f1",
        ),
        (
            &projection.canonical_input_bits.x2,
            ["4000000000000000"; 10].as_slice(),
            "input x2",
        ),
        (
            &projection.canonical_input_bits.f2,
            ["400a000000000000"; 10].as_slice(),
            "input f2",
        ),
        (&projection.canonical_input_bits.u, U_BITS, "input u"),
        (&value.expected_output_bits.y_both, Y_BOTH, "output yBoth"),
        (
            &value.expected_output_bits.y_below,
            Y_BELOW,
            "output yBelow",
        ),
        (
            &value.expected_output_bits.y_above,
            Y_ABOVE,
            "output yAbove",
        ),
        (
            &value.expected_output_bits.y_unlimited,
            Y_UNLIMITED,
            "output yUnlimited",
        ),
    ] {
        exact_slice(actual, expected, field)?;
    }
    Ok(())
}

fn validate_architectures(value: &Manifest) -> Result<(), String> {
    require(value.architectures.len() == 2, "architecture count")?;
    validate_architecture(
        &value.architectures[0],
        (
            "arm64",
            "linux/arm64",
            "arm64",
            "aarch64",
            "aarch64",
            "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4",
            "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666",
        ),
    )?;
    validate_architecture(
        &value.architectures[1],
        (
            "amd64",
            "linux/amd64",
            "amd64",
            "x86_64",
            "x86_64",
            "sha256:92d0779a01e7d43ed4d5ecb4cfd9754cb259b30673ddb454b5a32e3eb8665f11",
            "sha256:0c81120bb392de44cab0e9ff6818d0a44afad657d5b401f25e148fa6c26e5347",
        ),
    )?;
    require(
        value.architectures[0].repository_revision == value.architectures[1].repository_revision
            && value.architectures[0].generator_inputs == value.architectures[1].generator_inputs,
        "cross-architecture generator provenance",
    )
}

fn validate_architecture(
    value: &Architecture,
    expected: (&str, &str, &str, &str, &str, &str, &str),
) -> Result<(), String> {
    for (actual, expected, field) in [
        (&value.name, expected.0, "architecture.name"),
        (&value.platform, expected.1, "architecture.platform"),
        (&value.host_architecture, expected.2, "architecture.host"),
        (
            &value.docker_server_architecture,
            expected.3,
            "architecture.server",
        ),
        (
            &value.container_architecture,
            expected.4,
            "architecture.container",
        ),
        (
            &value.platform_manifest_digest,
            expected.5,
            "architecture.manifest",
        ),
        (&value.config_digest, expected.6, "architecture.config"),
        (
            &value.omc_version,
            "OpenModelica 1.25.1",
            "architecture.omc",
        ),
        (&value.gcc_version, "11.4.0", "architecture.gcc"),
        (&value.binutils_version, "2.38", "architecture.binutils"),
        (&value.glibc_version, "2.35", "architecture.glibc"),
        (&value.raw_run_a_sha256, RAW_SHA, "architecture raw A"),
        (&value.raw_run_b_sha256, RAW_SHA, "architecture raw B"),
        (
            &value.flag_control_raw_sha256,
            CONTROL_SHA,
            "architecture control raw",
        ),
        (
            &value.canonical_sha256,
            CANONICAL_SHA,
            "architecture canonical",
        ),
        (
            &value.flag_control_canonical_sha256,
            CONTROL_CANONICAL_SHA,
            "architecture control canonical",
        ),
    ] {
        exact(actual, expected, field)?;
    }
    revision(&value.repository_revision)?;
    for digest_value in [
        &value.generator_inputs.line_pilot_sha256,
        &value.generator_inputs.line_flag_pilot_sha256,
        &value.generator_inputs.runner_sha256,
        &value.generator_inputs.regenerate_sha256,
        &value.generator_inputs.canonicalizer_sha256,
        &value.generator_inputs.tool_main_sha256,
        &value.generator_inputs.tool_cargo_toml_sha256,
        &value.generator_inputs.tool_cargo_lock_sha256,
        &value.generator_inputs.architecture_generator_sha256,
        &value.generator_inputs.architecture_verifier_sha256,
    ] {
        digest(digest_value)?;
    }
    require(value.runs.len() == 2, "run count")?;
    for (run, id, token) in [
        (&value.runs[0], "run-a", "fresh-run-a"),
        (&value.runs[1], "run-b", "fresh-run-b"),
    ] {
        exact(&run.id, id, "run.id")?;
        exact(&run.output_directory_token, token, "run.token")?;
        digest(&run.log_sha256)?;
        exact(&run.raw_sha256, RAW_SHA, "run.raw_sha256")?;
    }
    Ok(())
}

fn validate_controls(value: &Manifest) -> Result<(), String> {
    let control = &value.semantic_control;
    exact(
        &control.mutation,
        "yBelow limitAbove false to true",
        "semantic mutation",
    )?;
    exact(
        &control.first_mismatch_time_bits,
        TIME_BITS[8],
        "semantic time",
    )?;
    exact(
        &control.expected_comparison,
        "exact_mismatch",
        "semantic result",
    )?;
    require(
        control.first_mismatch_row == 8 && control.mismatch_rows == [8, 9],
        "semantic rows",
    )?;
    let cross = &value.cross_architecture;
    exact(&cross.comparison, "canonical_bytes", "cross comparison")?;
    exact(&cross.arm64_sha256, CANONICAL_SHA, "cross arm")?;
    exact(&cross.amd64_sha256, CANONICAL_SHA, "cross amd")?;
    exact(&cross.result, "pass", "cross result")
}

fn validate_artifacts(value: &Manifest) -> Result<(), String> {
    let expected = expected_artifacts();
    require(value.artifacts.len() == expected.len(), "artifact count")?;
    for (artifact, (role, path)) in value.artifacts.iter().zip(expected) {
        exact(&artifact.role, &role, "artifact role order")?;
        exact(&artifact.path, &path, "artifact role path")?;
        digest(&artifact.sha256)?;
    }
    Ok(())
}

fn validate_regeneration(value: &Manifest) -> Result<(), String> {
    let regeneration = &value.regeneration;
    for (actual, expected, field) in [
        (
            &regeneration.entrypoint,
            "tools/openmodelica-line-reference/line/regenerate.sh",
            "regeneration.entrypoint",
        ),
        (
            &regeneration.assembly_entrypoint,
            "tools/openmodelica-line-reference/line/assemble.sh",
            "regeneration.assembly",
        ),
        (
            &regeneration.evidence_workflow,
            ".github/workflows/openmodelica-line-evidence.yml",
            "regeneration.workflow",
        ),
        (
            &regeneration.network,
            "none_during_container_execution",
            "regeneration.network",
        ),
        (&regeneration.pull, "never", "regeneration.pull"),
        (
            &regeneration.source_materialization,
            "git_archive",
            "regeneration.materialization",
        ),
        (
            &regeneration.source_mounts,
            "read_only",
            "regeneration.mounts",
        ),
        (
            &regeneration.container_root,
            "read_only",
            "regeneration.root",
        ),
        (
            &regeneration.container_user,
            "non_root",
            "regeneration.user",
        ),
        (
            &regeneration.capabilities,
            "none",
            "regeneration.capabilities",
        ),
        (&regeneration.cpus, "4", "regeneration.cpus"),
    ] {
        exact(actual, expected, field)?;
    }
    exact_slice(
        &regeneration.platforms,
        &["linux/arm64", "linux/amd64"],
        "regeneration.platforms",
    )?;
    require(
        regeneration.no_new_privileges
            && regeneration.device_mounts == 0
            && !regeneration.docker_socket_mounted,
        "regeneration isolation",
    )?;
    require(
        regeneration.timeout_seconds == 120
            && regeneration.memory_bytes == 2_147_483_648
            && regeneration.memory_swap_bytes == 2_147_483_648,
        "regeneration time/memory",
    )?;
    require(
        regeneration.pids_limit == 256
            && regeneration.tmpfs_bytes == 268_435_456
            && regeneration.per_file_bytes == 67_108_864
            && regeneration.output_directory_bytes == 268_435_456,
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
fn revision(value: &str) -> Result<(), String> {
    require(
        value.len() == 40
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid repository revision",
    )
}
fn require(condition: bool, detail: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(detail.into())
    }
}
