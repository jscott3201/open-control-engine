//! Closed parser for the native OpenModelica execution record.

use std::collections::BTreeMap;
use std::path::Path;

use super::expectations::SOURCE_FILES;
use super::schema::{Architecture, GeneratorInputs};

const IMAGE_INDEX: &str = "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864";

const SIMULATION_OPTIONS: &str = "    simulationOptions = \"startTime = 0.0, stopTime = 420.0, numberOfIntervals = 7, tolerance = 1e-9, method = 'dassl', fileNamePrefix = 'ReliefsPilot', options = '', outputFormat = 'csv', variableFilter = '^(uTSup|uOutDam_min|uOutDam_max|uRetDam_min|uRetDam_max|yOutDam|yRetDam)$', cflags = '', simflags = ''\",";
const INITIALIZATION_MESSAGE: &str = "    messages = \"LOG_SUCCESS       | info    | The initialization finished successfully without homotopy method.";
const SIMULATION_MESSAGE: &str =
    "LOG_SUCCESS       | info    | The simulation finished successfully.";

pub(super) fn validate(
    path: &Path,
    text: &str,
    architecture: &Architecture,
    token: &str,
    model: &str,
    raw_digest: &str,
) -> Result<(), String> {
    if text.contains('\r') {
        return Err(format!("{} has invalid line endings", path.display()));
    }
    let lines = text
        .lines()
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.last() != Some(&"runner_complete=1") {
        return Err(format!(
            "{} has no terminal completion marker",
            path.display()
        ));
    }
    let (result_start, result_end) = validate_simulation_result(path, &lines)?;
    let mut observed = BTreeMap::new();
    for line in lines
        .iter()
        .take(result_start)
        .chain(lines.iter().skip(result_end + 1))
    {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("{} has an unknown log record", path.display()))?;
        if key.is_empty() || observed.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!(
                "{} has a duplicate or invalid log field {key}",
                path.display()
            ));
        }
    }

    let identity = take_dynamic(&mut observed, "container_identity")?;
    let peak = take_dynamic(&mut observed, "observed_cgroup_peak_bytes")?;
    let size = take_dynamic(&mut observed, "output_directory_kib")?;
    validate_identity(path, &identity)?;
    if !bounded_integer(&peak, 2_147_483_648, true) {
        return Err(format!("{} has invalid cgroup memory peak", path.display()));
    }
    if !bounded_integer(&size, 262_144, false) {
        return Err(format!("{} has invalid output size", path.display()));
    }

    let expected = expected_metadata(architecture, token, model, raw_digest)?;
    if observed != expected {
        return Err(format!(
            "{} metadata fields or values are not closed",
            path.display()
        ));
    }
    Ok(())
}

fn validate_simulation_result(path: &Path, lines: &[&str]) -> Result<(usize, usize), String> {
    let starts = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| **line == "record SimulationResult")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let ends = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| **line == "end SimulationResult;")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if starts.len() != 1 || ends.len() != 1 || ends[0].checked_sub(starts[0]) != Some(13) {
        return Err(format!(
            "{} has an invalid SimulationResult closure",
            path.display()
        ));
    }
    let start = starts[0];
    let expected = [
        "record SimulationResult",
        "    resultFile = \"/out/ReliefsPilot_res.csv\",",
        SIMULATION_OPTIONS,
        INITIALIZATION_MESSAGE,
        SIMULATION_MESSAGE,
        "\",",
    ];
    if lines.get(start..start + expected.len()) != Some(expected.as_slice()) {
        return Err(format!(
            "{} has invalid SimulationResult values",
            path.display()
        ));
    }
    let timings = [
        "timeFrontend",
        "timeBackend",
        "timeSimCode",
        "timeTemplates",
        "timeCompile",
        "timeSimulation",
        "timeTotal",
    ];
    for (offset, key) in timings.iter().enumerate() {
        let line = lines[start + expected.len() + offset];
        let prefix = format!("    {key} = ");
        let value = line
            .strip_prefix(&prefix)
            .ok_or_else(|| format!("{} has invalid {key} timing", path.display()))?;
        let value = if *key == "timeTotal" {
            value
        } else {
            value
                .strip_suffix(',')
                .ok_or_else(|| format!("{} has invalid {key} timing", path.display()))?
        };
        if !bounded_timing(value, *key == "timeTotal") {
            return Err(format!("{} has invalid {key} timing", path.display()));
        }
    }
    Ok((start, ends[0]))
}

fn bounded_timing(value: &str, nonzero: bool) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_digit()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || b".eE+-".contains(&byte))
        && value.parse::<f64>().is_ok_and(|number| {
            number.is_finite() && number >= 0.0 && number <= 120.0 && (!nonzero || number > 0.0)
        })
}

fn bounded_integer(value: &str, maximum: u64, nonzero: bool) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value
            .parse::<u64>()
            .is_ok_and(|number| number <= maximum && (!nonzero || number > 0))
}

fn take_dynamic(values: &mut BTreeMap<String, String>, key: &str) -> Result<String, String> {
    values
        .remove(key)
        .ok_or_else(|| format!("run log has no {key} field"))
}

fn validate_identity(path: &Path, identity: &str) -> Result<(), String> {
    let parts = identity.split(':').collect::<Vec<_>>();
    if parts.len() != 2
        || parts.iter().any(|part| {
            part.is_empty()
                || part.len() > 10
                || !part.bytes().all(|byte| byte.is_ascii_digit())
                || part.parse::<u32>().is_err()
        })
        || parts[0].parse::<u32>() == Ok(0)
    {
        return Err(format!("{} has invalid container identity", path.display()));
    }
    Ok(())
}

fn expected_metadata(
    architecture: &Architecture,
    token: &str,
    model: &str,
    raw_digest: &str,
) -> Result<BTreeMap<String, String>, String> {
    validate_source_cone(architecture)?;
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    let platform = architecture.platform.as_str();
    let docker = format!(
        "docker run --pull=never --platform {platform} --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro"
    );
    for (key, value) in [
        ("host_architecture", architecture.host_architecture.as_str()),
        (
            "docker_server_architecture",
            architecture.docker_server_architecture.as_str(),
        ),
        ("image_index_digest", IMAGE_INDEX),
        (
            "image_platform_manifest_digest",
            architecture.platform_manifest_digest.as_str(),
        ),
        ("image_config_digest", architecture.config_digest.as_str()),
        ("oci_metadata_validation", "raw_digest_and_pinned_graph"),
        ("pull_policy", "never"),
        ("host_timeout_seconds", "120"),
        (
            "buildings_remote",
            "https://github.com/lbl-srg/modelica-buildings.git",
        ),
        (
            "buildings_commit",
            "a131864e4c4df22ebcd52bb8da439de0087ac365",
        ),
        ("buildings_tree", "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09"),
        (
            "modelica_remote",
            "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git",
        ),
        (
            "modelica_commit",
            "7a4bf7de77a3986e8eb1e88cbb515d646f78f834",
        ),
        ("modelica_tree", "43d7d8fc1a991358e9e5e91976e27cdc4280173f"),
        (
            "repository_revision",
            architecture.repository_revision.as_str(),
        ),
        (
            "generator_provenance_scope",
            "native_generation_and_publication",
        ),
        (
            "source_materialization",
            "git_archive_with_pinned_modelica_export_subst",
        ),
        (
            "buildings_materialization",
            "git_archive_without_local_attribute_override",
        ),
        ("modelica_transform_path", "Modelica/package.mo"),
        (
            "modelica_transform_rule",
            "Modelica/package.mo -export-subst",
        ),
        (
            "modelica_package_committed_sha256",
            "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
        ),
        (
            "modelica_package_materialized_sha256",
            "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
        ),
        ("output_directory_token", token),
        ("selected_model", model),
        (
            "container_architecture",
            architecture.container_architecture.as_str(),
        ),
        ("modelica_path", ""),
        ("events_enabled", "default_true"),
        ("simflags", "empty"),
        ("root_write_probe", "read-only"),
        ("source_write_probe", "read-only"),
        ("reference_write_probe", "read-only"),
        ("network_route_lines", "1"),
        ("cgroup_memory_max", "2147483648"),
        ("cgroup_pids_max", "256"),
        ("cgroup_cpu_max", "400000 100000"),
        ("per_file_limit_bytes", "67108864"),
        ("output_directory_limit_bytes", "268435456"),
        ("gcc_version", "11.4.0"),
        ("binutils_version", "2.38"),
        ("glibc_version", "2.35"),
        ("omc_version", "OpenModelica 1.25.1"),
        (
            "reliefs_source",
            "/sources/buildings/Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo",
        ),
        (
            "line_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Line.mo",
        ),
        (
            "min_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Min.mo",
        ),
        (
            "max_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Max.mo",
        ),
        (
            "cdl_constant_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo",
        ),
        (
            "real_input_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Interfaces/RealInput.mo",
        ),
        (
            "real_output_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Interfaces/RealOutput.mo",
        ),
        (
            "msl_constant_source",
            "/sources/modelica/Modelica/Blocks/Sources.mo",
        ),
        (
            "time_table_source",
            "/sources/modelica/Modelica/Blocks/Sources.mo",
        ),
        ("Modelica", "4.1.0"),
        ("Buildings", "14.0.0"),
        ("omc_warning_count", "0"),
        ("raw_sha256", raw_digest),
        ("runner_complete", "1"),
    ] {
        values.insert(key.to_owned(), value.to_owned());
    }
    values.insert(
        "source_files_json".to_owned(),
        source_files_json(architecture)?,
    );
    values.insert("docker_command".to_owned(), docker);
    add_generator_inputs(&mut values, &architecture.generator_inputs);
    add_toolchain(&mut values, architecture);
    Ok(values)
}

fn source_files_json(architecture: &Architecture) -> Result<String, String> {
    let records = architecture
        .source_files
        .iter()
        .map(|file| {
            BTreeMap::from([
                ("committed_sha256", file.committed_sha256.as_str()),
                ("materialized_sha256", file.materialized_sha256.as_str()),
                ("path", file.path.as_str()),
                ("source", file.source.as_str()),
            ])
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&records).map_err(|error| error.to_string())
}

fn add_generator_inputs(values: &mut BTreeMap<String, String>, input: &GeneratorInputs) {
    for (key, value) in [
        ("reliefs_pilot_sha256", &input.reliefs_pilot_sha256),
        ("parameter_pilot_sha256", &input.parameter_pilot_sha256),
        ("clamp_pilot_sha256", &input.clamp_pilot_sha256),
        ("runner_sha256", &input.runner_sha256),
        ("regenerate_sha256", &input.regenerate_sha256),
        ("canonicalizer_sha256", &input.canonicalizer_sha256),
        ("tool_main_sha256", &input.tool_main_sha256),
        ("tool_cargo_toml_sha256", &input.tool_cargo_toml_sha256),
        ("tool_cargo_lock_sha256", &input.tool_cargo_lock_sha256),
        (
            "architecture_generator_sha256",
            &input.architecture_generator_sha256,
        ),
        (
            "architecture_verifier_sha256",
            &input.architecture_verifier_sha256,
        ),
        (
            "projection_verifier_sha256",
            &input.projection_verifier_sha256,
        ),
        ("safe_file_helper_sha256", &input.safe_file_helper_sha256),
        ("evidence_workflow_sha256", &input.evidence_workflow_sha256),
        ("oci_materializer_sha256", &input.oci_materializer_sha256),
        ("deadline_sha256", &input.deadline_sha256),
        ("deadline_test_sha256", &input.deadline_test_sha256),
        ("container_cleanup_sha256", &input.container_cleanup_sha256),
        (
            "container_cleanup_test_sha256",
            &input.container_cleanup_test_sha256,
        ),
        ("output_publish_sha256", &input.output_publish_sha256),
        (
            "output_publish_test_sha256",
            &input.output_publish_test_sha256,
        ),
        ("oci_index_source_sha256", &input.oci_index_source_sha256),
        (
            "arm64_manifest_source_sha256",
            &input.arm64_manifest_source_sha256,
        ),
        (
            "amd64_manifest_source_sha256",
            &input.amd64_manifest_source_sha256,
        ),
    ] {
        values.insert(key.to_owned(), value.clone());
    }
}

fn add_toolchain(values: &mut BTreeMap<String, String>, architecture: &Architecture) {
    let toolchain = &architecture.artifact_toolchain;
    for (key, value) in [
        ("rustc_release", &toolchain.rustc_release),
        ("rustc_commit_hash", &toolchain.rustc_commit_hash),
        ("rustc_commit_date", &toolchain.rustc_commit_date),
        ("rustc_host", &toolchain.rustc_host),
        ("rustc_llvm_version", &toolchain.rustc_llvm_version),
        ("cargo_release", &toolchain.cargo_release),
        ("cargo_commit_hash", &toolchain.cargo_commit_hash),
        ("cargo_commit_date", &toolchain.cargo_commit_date),
        ("cargo_host", &toolchain.cargo_host),
        ("python_version", &toolchain.python_version),
    ] {
        values.insert(key.to_owned(), value.clone());
    }
}

fn validate_source_cone(architecture: &Architecture) -> Result<(), String> {
    if architecture.source_files.len() != SOURCE_FILES.len()
        || architecture
            .source_files
            .iter()
            .zip(SOURCE_FILES)
            .any(|(actual, expected)| {
                actual.source != expected.0
                    || actual.path != expected.1
                    || actual.committed_sha256 != expected.2
                    || actual.materialized_sha256 != expected.2
            })
    {
        return Err("native source cone identity drifted".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_timing_rejects_nonfinite_signed_and_out_of_range_values() {
        for value in ["NaN", "inf", "-1", "+1", "121", "1-2", ""] {
            assert!(!bounded_timing(value, false), "accepted {value}");
        }
        assert!(bounded_timing("0.000001", false));
        assert!(bounded_timing("1.23e1", true));
    }
}
