//! Bounded parsing and fixed-literal validation for the Reliefs manifest.

use super::expectations::*;
use super::schema::{Architecture, GenerationRevisionContract, Manifest};

pub(super) const MAX_MANIFEST_BYTES: usize = 256 * 1024;

pub(super) fn parse_generation_contract(
    input: &[u8],
) -> Result<GenerationRevisionContract, String> {
    if input.len() > 4096 {
        return Err("generation revision contract exceeds 4096 bytes".into());
    }
    scan_json_strings(input)?;
    let contract: GenerationRevisionContract =
        serde_json::from_slice(input).map_err(|error| error.to_string())?;
    require(
        contract.format == "oce-openmodelica-reliefs-generation-revision-v1"
            && contract.relationship
                == "candidate_native_artifact_producer_and_ancestor_of_retained_head",
        "generation revision contract literals",
    )?;
    revision(&contract.revision)?;
    require(
        contract.revision != "0".repeat(40),
        "generation revision contract commit",
    )?;
    Ok(contract)
}

pub(super) fn parse(input: &[u8], generation_revision: &str) -> Result<Manifest, String> {
    if input.len() > MAX_MANIFEST_BYTES {
        return Err("manifest exceeds 256 KiB".into());
    }
    scan_json_strings(input)?;
    let value: Manifest = serde_json::from_slice(input).map_err(|error| error.to_string())?;
    validate(&value, generation_revision)?;
    Ok(value)
}

fn scan_json_strings(input: &[u8]) -> Result<(), String> {
    let (mut quoted, mut escaped, mut length) = (false, false, 0_usize);
    for &byte in input {
        if !quoted {
            if byte == b'"' {
                quoted = true;
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
            quoted = false;
        } else {
            length += 1;
        }
        if length > 4096 {
            return Err("manifest string exceeds 4096 UTF-8 bytes".into());
        }
    }
    Ok(())
}

fn validate(value: &Manifest, generation_revision: &str) -> Result<(), String> {
    exact(
        &value.format,
        "oce-openmodelica-reliefs-external-run-v1",
        "format",
    )?;
    validate_scope(value)?;
    validate_image(value)?;
    validate_sources(value)?;
    validate_simulation(value)?;
    validate_projection(value)?;
    validate_architectures(value, generation_revision)?;
    validate_controls(value)?;
    validate_artifacts(value)?;
    validate_regeneration(value)
}

fn validate_scope(value: &Manifest) -> Result<(), String> {
    let scope = &value.scope;
    exact(
        &scope.class,
        "Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Modulations.Reliefs",
        "scope.class",
    )?;
    exact(
        &scope.scenario,
        "source_default_dyadic_regions",
        "scope.scenario",
    )?;
    require(
        scope.parameters.u_min == "-0.25"
            && scope.parameters.u_max == "0.25"
            && scope.parameters.u_out_dam_max == "0"
            && scope.parameters.u_ret_dam_min == "0",
        "scope parameters",
    )?;
    exact_slice(
        &scope.inputs,
        &[
            "uTSup",
            "uOutDam_min",
            "uOutDam_max",
            "uRetDam_min",
            "uRetDam_max",
        ],
        "scope inputs",
    )?;
    exact_slice(&scope.outputs, &["yOutDam", "yRetDam"], "scope outputs")?;
    exact(
        &scope.comparison,
        "exact_finite_f64_bits",
        "scope comparison",
    )?;
    exact(
        &scope.global_tier3_status,
        "skipped",
        "global Tier-3 status",
    )
}

fn validate_image(value: &Manifest) -> Result<(), String> {
    let image = &value.image;
    require(
        image.repository == "openmodelica/openmodelica"
            && image.tag == "v1.25.1-minimal"
            && image.index_digest
                == "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864"
            && image.platforms.len() == 2,
        "image identity",
    )?;
    for (platform, expected) in image.platforms.iter().zip([
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
    ]) {
        require(
            platform.platform == expected.0
                && platform.manifest_digest == expected.1
                && platform.config_digest == expected.2,
            "image platform identity",
        )?;
    }
    Ok(())
}

fn validate_sources(value: &Manifest) -> Result<(), String> {
    require(value.sources.len() == 2, "source count")?;
    for (source, expected) in value.sources.iter().zip([
        (
            "buildings",
            "https://github.com/lbl-srg/modelica-buildings.git",
            "a131864e4c4df22ebcd52bb8da439de0087ac365",
            "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09",
            "Buildings",
            "14.0.0",
            "git_archive_without_local_attribute_override",
        ),
        (
            "modelica",
            "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git",
            "7a4bf7de77a3986e8eb1e88cbb515d646f78f834",
            "43d7d8fc1a991358e9e5e91976e27cdc4280173f",
            "Modelica",
            "4.1.0",
            "git_archive_with_pinned_modelica_export_subst",
        ),
    ]) {
        require(
            source.name == expected.0
                && source.repository == expected.1
                && source.commit == expected.2
                && source.tree == expected.3
                && source.package == expected.4
                && source.version == expected.5
                && source.materialization == expected.6,
            "source identity",
        )?;
    }
    require(
        value.sources[0].transforms.is_empty(),
        "Buildings transforms",
    )?;
    require(
        value.sources[1].transforms.len() == 1
            && value.sources[1].transforms[0].path == "Modelica/package.mo"
            && value.sources[1].transforms[0].rule == "Modelica/package.mo -export-subst",
        "Modelica transform",
    )?;
    let files = value
        .sources
        .iter()
        .flat_map(|source| {
            source
                .files
                .iter()
                .map(move |file| (source.name.as_str(), file))
        })
        .collect::<Vec<_>>();
    require(files.len() == SOURCE_FILES.len(), "source file count")?;
    for ((source, file), expected) in files.iter().zip(SOURCE_FILES) {
        require(
            *source == expected.0
                && file.path == expected.1
                && file.committed_sha256 == expected.2
                && file.materialized_sha256 == expected.2,
            "source file identity",
        )?;
    }
    Ok(())
}

fn validate_simulation(value: &Manifest) -> Result<(), String> {
    let simulation = &value.simulation;
    require(
        simulation.method == "dassl"
            && simulation.start_time == "0"
            && simulation.stop_time == "420"
            && simulation.number_of_intervals == 7
            && simulation.tolerance == "1e-9"
            && simulation.output_format == "csv"
            && simulation.variable_filter
                == "^(uTSup|uOutDam_min|uOutDam_max|uRetDam_min|uRetDam_max|yOutDam|yRetDam)$"
            && simulation.simflags.is_empty()
            && simulation.event_emission
            && simulation.raw_header == super::canonicalizer::RAW_HEADER,
        "simulation contract",
    )
}

fn validate_projection(value: &Manifest) -> Result<(), String> {
    let projection = &value.projection;
    exact_slice(
        &projection.columns,
        &[
            "time",
            "uTSup",
            "uOutDam_min",
            "uOutDam_max",
            "uRetDam_min",
            "uRetDam_max",
            "yOutDam",
            "yRetDam",
        ],
        "projection columns",
    )?;
    require(
        projection.grouping == "contiguous_equal_f64_bits"
            && projection.group_selection == "last"
            && projection.tuple_selection == "initial_then_first_complete_five_input_tuple_change"
            && !projection.normalize_times
            && projection.raw_rows == 21
            && projection.grouped_rows == 14
            && projection.canonical_rows == 7
            && projection.group_sizes == [1, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 2]
            && projection.selected_source_rows == [0, 3, 6, 9, 12, 15, 18],
        "projection rules and counts",
    )?;
    exact_slice(
        &projection.canonical_time_bits,
        TIME_BITS,
        "projection times",
    )?;
    exact_slice(
        &projection.raw_time_bits,
        RAW_TIME_BITS,
        "raw timestamp bits",
    )?;
    for (actual, expected, name) in [
        (
            &projection.canonical_input_bits.u_t_sup,
            U_T_SUP_BITS,
            "uTSup",
        ),
        (
            &projection.canonical_input_bits.u_out_dam_min,
            &["3fd0000000000000"; 7],
            "uOutDam_min",
        ),
        (
            &projection.canonical_input_bits.u_out_dam_max,
            &["3fec000000000000"; 7],
            "uOutDam_max",
        ),
        (
            &projection.canonical_input_bits.u_ret_dam_min,
            &["3fc0000000000000"; 7],
            "uRetDam_min",
        ),
        (
            &projection.canonical_input_bits.u_ret_dam_max,
            &["3fe8000000000000"; 7],
            "uRetDam_max",
        ),
        (&value.expected_output_bits.y_out_dam, Y_OUT_BITS, "yOutDam"),
        (&value.expected_output_bits.y_ret_dam, Y_RET_BITS, "yRetDam"),
    ] {
        exact_slice(actual, expected, name)?;
    }
    Ok(())
}

fn validate_architectures(value: &Manifest, generation_revision: &str) -> Result<(), String> {
    require(value.architectures.len() == 2, "architecture count")?;
    validate_architecture(
        &value.architectures[0],
        "arm64",
        "linux/arm64",
        "arm64",
        "aarch64",
        generation_revision,
    )?;
    validate_architecture(
        &value.architectures[1],
        "amd64",
        "linux/amd64",
        "amd64",
        "x86_64",
        generation_revision,
    )?;
    require(
        value.architectures[0].repository_revision == value.architectures[1].repository_revision
            && value.architectures[0].generator_inputs == value.architectures[1].generator_inputs
            && value.architectures[0].source_files == value.architectures[1].source_files,
        "cross-architecture provenance",
    )
}

fn validate_architecture(
    value: &Architecture,
    name: &str,
    platform: &str,
    host: &str,
    machine: &str,
    generation_revision: &str,
) -> Result<(), String> {
    require(
        value.name == name
            && value.platform == platform
            && value.host_architecture == host
            && value.docker_server_architecture == machine
            && value.container_architecture == machine
            && value.generator_provenance_scope == "native_generation_and_publication"
            && value.omc_version == "OpenModelica 1.25.1"
            && value.gcc_version == "11.4.0"
            && value.binutils_version == "2.38"
            && value.glibc_version == "2.35"
            && value.canonical_sha256 == CANONICAL_SHA
            && value.raw_run_a_sha256 == value.raw_run_b_sha256,
        "architecture identity",
    )?;
    revision(&value.repository_revision)?;
    require(
        value.repository_revision == generation_revision,
        "architecture generation revision",
    )?;
    for digest_value in architecture_digests(value) {
        digest(digest_value)?;
    }
    require(value.runs.len() == 2, "native run count")?;
    for (run, id, token) in [
        (&value.runs[0], "run-a", "fresh-run-a"),
        (&value.runs[1], "run-b", "fresh-run-b"),
    ] {
        require(
            run.id == id
                && run.output_directory_token == token
                && run.raw_sha256 == value.raw_run_a_sha256,
            "native run identity",
        )?;
        digest(&run.log_sha256)?;
    }
    require(
        value.projection_mutation.control == "explicit_keep_first"
            && value.projection_mutation.selected_source_rows == [0, 4, 7, 10, 13, 16, 19]
            && value
                .projection_mutation
                .selected_time_bits
                .iter()
                .map(String::as_str)
                .eq(KEEP_FIRST_TIME_BITS.iter().copied())
            && value.projection_mutation.expected_source_rows == [0, 3, 6, 9, 12, 15, 18],
        "projection mutation identity",
    )?;
    validate_toolchain(value, name)
}

fn architecture_digests(value: &Architecture) -> Vec<&str> {
    let input = &value.generator_inputs;
    vec![
        &value.raw_run_a_sha256,
        &value.raw_run_b_sha256,
        &value.parameter_control_raw_sha256,
        &value.parameter_control_canonical_sha256,
        &value.final_clamp_raw_sha256,
        &value.final_clamp_canonical_sha256,
        &input.reliefs_pilot_sha256,
        &input.parameter_pilot_sha256,
        &input.clamp_pilot_sha256,
        &input.runner_sha256,
        &input.regenerate_sha256,
        &input.canonicalizer_sha256,
        &input.tool_main_sha256,
        &input.tool_cargo_toml_sha256,
        &input.tool_cargo_lock_sha256,
        &input.architecture_generator_sha256,
        &input.architecture_verifier_sha256,
        &input.projection_verifier_sha256,
        &input.safe_file_helper_sha256,
        &input.evidence_workflow_sha256,
        &input.oci_materializer_sha256,
        &input.deadline_sha256,
        &input.deadline_test_sha256,
        &input.container_cleanup_sha256,
        &input.container_cleanup_test_sha256,
        &input.output_publish_sha256,
        &input.output_publish_test_sha256,
        &input.oci_index_source_sha256,
        &input.arm64_manifest_source_sha256,
        &input.amd64_manifest_source_sha256,
    ]
}

fn validate_toolchain(value: &Architecture, name: &str) -> Result<(), String> {
    let host = if name == "arm64" {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let tool = &value.artifact_toolchain;
    require(
        tool.rustc_release == "1.97.1"
            && tool.rustc_commit_hash == "8bab26f4f68e0e26f0bb7960be334d5b520ea452"
            && tool.rustc_commit_date == "2026-07-14"
            && tool.rustc_host == host
            && tool.rustc_llvm_version == "22.1.6"
            && tool.cargo_release == "1.97.1"
            && tool.cargo_commit_hash == "c980f4866141969fab6254a680546a277789d6f0"
            && tool.cargo_commit_date == "2026-06-30"
            && tool.cargo_host == host
            && tool.python_version == "Python 3.13.7",
        "artifact toolchain",
    )
}

fn validate_controls(value: &Manifest) -> Result<(), String> {
    let controls = &value.controls;
    require(
        controls.parameter.mutation == "uOutDamMax 0 to -0.125"
            && controls.parameter.output == "yOutDam"
            && controls.parameter.first_mismatch_row == 2
            && controls.parameter.first_mismatch_time_bits == TIME_BITS[2]
            && controls.parameter.expected_bits == "3fe2000000000000"
            && controls.parameter.observed_bits == "3fec000000000000"
            && controls.mapping.mutation == "swap yOutDam and yRetDam"
            && controls.mapping.expected_comparison == "exact_mismatch"
            && controls.declared_path.mutation
                == "replace yRetDam root with nonexistent declared root"
            && controls.declared_path.expected_error
                == "unknown point/connector 'http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.yMissing'",
        "negative controls",
    )?;
    let clamp = &controls.final_clamp;
    require(
        clamp.inputs.u_out_dam_min == "3fec000000000000"
            && clamp.inputs.u_out_dam_max == "3fd0000000000000"
            && clamp.inputs.u_ret_dam_min == "3fe8000000000000"
            && clamp.inputs.u_ret_dam_max == "3fc0000000000000"
            && clamp.y_out_dam == "3fd0000000000000"
            && clamp.y_ret_dam == "3fe8000000000000"
            && clamp.rows == 7,
        "final clamp control",
    )?;
    require(
        value.cross_architecture.comparison == "canonical_bytes"
            && value.cross_architecture.arm64_sha256 == CANONICAL_SHA
            && value.cross_architecture.amd64_sha256 == CANONICAL_SHA
            && value.cross_architecture.result == "pass",
        "cross-architecture record",
    )
}

fn validate_artifacts(value: &Manifest) -> Result<(), String> {
    let expected = expected_artifacts();
    require(value.artifacts.len() == expected.len(), "artifact count")?;
    for (artifact, (role, path)) in value.artifacts.iter().zip(expected) {
        require(
            artifact.role == role && artifact.path == path,
            "artifact role path",
        )?;
        digest(&artifact.sha256)?;
    }
    Ok(())
}

fn validate_regeneration(value: &Manifest) -> Result<(), String> {
    let regeneration = &value.regeneration;
    require(
        regeneration.entrypoint == "tools/openmodelica-reliefs-reference/reliefs/regenerate.sh"
            && regeneration.assembly_entrypoint
                == "tools/openmodelica-reliefs-reference/reliefs/assemble.sh"
            && regeneration.evidence_workflow
                == ".github/workflows/openmodelica-reliefs-evidence.yml"
            && regeneration.network == "none_during_container_execution"
            && regeneration.pull == "never"
            && regeneration.platforms == ["linux/arm64", "linux/amd64"]
            && regeneration.source_materialization
                == "git_archive_with_pinned_modelica_export_subst"
            && regeneration.source_mounts == "read_only"
            && regeneration.container_root == "read_only"
            && regeneration.container_user == "non_root"
            && regeneration.capabilities == "none"
            && regeneration.no_new_privileges
            && regeneration.device_mounts == 0
            && !regeneration.docker_socket_mounted
            && regeneration.timeout_seconds == 120
            && regeneration.cpus == "4"
            && regeneration.memory_bytes == 2_147_483_648
            && regeneration.memory_swap_bytes == 2_147_483_648
            && regeneration.memory_measurement == "cgroup_memory_peak"
            && regeneration.pids_limit == 256
            && regeneration.tmpfs_bytes == 268_435_456
            && regeneration.per_file_bytes == 67_108_864
            && regeneration.output_directory_bytes == 268_435_456,
        "regeneration contract",
    )
}

fn exact(actual: &str, expected: &str, name: &str) -> Result<(), String> {
    require(actual == expected, &format!("unsupported {name}"))
}
fn exact_slice(actual: &[String], expected: &[&str], name: &str) -> Result<(), String> {
    require(
        actual
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied()),
        &format!("unsupported {name}"),
    )
}
pub(super) fn digest(value: &str) -> Result<(), String> {
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
    condition.then_some(()).ok_or_else(|| detail.into())
}
