//! Safe path, digest, OCI graph, source, and run-log validation for Toggle evidence.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::safe_read;
use super::schema::{Artifact, Manifest, Source};

const DOCKER_COMMAND: &str = "docker run --pull=never --platform linux/arm64 --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro";
const SIMULATION_OPTIONS: &str = "simulationOptions = \"startTime = 0.0, stopTime = 600.0, numberOfIntervals = 10, tolerance = 1e-9, method = 'dassl', fileNamePrefix = 'TogglePilot', options = '', outputFormat = 'csv', variableFilter = '^(u|clr|y)$', cflags = '', simflags = ''\",";

pub(super) fn validate(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut paths = BTreeSet::new();
    let mut roles = BTreeSet::new();
    for artifact in &manifest.artifacts {
        if !roles.insert(artifact.role.as_str()) || !paths.insert(artifact.path.as_str()) {
            return Err("artifact role or path is reused".into());
        }
        validate_role_path(artifact)?;
        if sha256_bytes(&safe_read::read(&root, &artifact.path)?) != artifact.sha256 {
            return Err(format!("artifact digest mismatch: {}", artifact.path));
        }
    }
    validate_runs(manifest, &root)?;
    validate_wrappers(manifest, &root)?;
    validate_oci(manifest, &root)?;
    validate_projection_mutation(manifest, &root)
}

fn validate_role_path(artifact: &Artifact) -> Result<(), String> {
    valid_path(&artifact.path)?;
    let fixture = "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/";
    let canonicalizer = "crates/oce-cxf/tests/open_modelica_toggle_reference/";
    let tool = "tools/openmodelica-toggle-reference/";
    let wrapper = "tools/openmodelica-toggle-reference/toggle/";
    let allowed = match artifact.role.as_str() {
        "canonicalizer_source" => artifact.path.starts_with(canonicalizer),
        "regeneration_script"
        | "runner_script"
        | "semantic_control_wrapper_model"
        | "wrapper_model" => artifact.path.starts_with(wrapper),
        "tool_cargo_lock" | "tool_cargo_toml" | "tool_main_source" => {
            artifact.path.starts_with(tool)
        }
        "evidence_validator_script"
        | "manifest_generator_script"
        | "deadline_script"
        | "deadline_test_script"
        | "output_publish_script"
        | "output_publish_test_script"
        | "container_cleanup_script"
        | "container_cleanup_test_script" => artifact.path.starts_with(wrapper),
        _ => artifact.path.starts_with(fixture),
    };
    if !allowed
        || artifact.path.contains("/golden/")
        || artifact.path.contains("/goldens/")
        || artifact.path.contains("tier2")
    {
        return Err(format!(
            "artifact role escapes its subtree: {}",
            artifact.role
        ));
    }
    Ok(())
}

fn valid_path(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 512
        || !value.is_ascii()
        || value.starts_with('/')
        || value.contains(['\\', ':', '*', '?'])
        || value.split('/').any(|segment| {
            segment.is_empty() || segment == "." || segment == ".." || segment.ends_with([' ', '.'])
        })
    {
        return Err("invalid repository-relative artifact path".into());
    }
    Ok(())
}

fn validate_runs(manifest: &Manifest, root: &Path) -> Result<(), String> {
    for (run, log_role, raw_role, model) in [
        (&manifest.runs[0], "run_a_log", "raw_run_a_csv", "Toggle"),
        (&manifest.runs[1], "run_b_log", "raw_run_b_csv", "Toggle"),
    ] {
        let log = artifact(manifest, log_role);
        let raw = artifact(manifest, raw_role);
        if run.log_path != log.path
            || run.raw_path != raw.path
            || run.log_sha256 != log.sha256
            || run.raw_sha256 != raw.sha256
        {
            return Err("run record is not bound to its log and raw artifact".into());
        }
        validate_run_log(
            manifest,
            &read_text(root, &log.path)?,
            &run.output_directory_token,
            &run.raw_sha256,
            model,
        )?;
    }
    if manifest.runs[0].log_path == manifest.runs[1].log_path
        || manifest.runs[0].raw_path == manifest.runs[1].raw_path
        || manifest.runs[0].output_directory_token == manifest.runs[1].output_directory_token
    {
        return Err("repeat runs are not distinct".into());
    }
    let first = safe_read::read(root, &manifest.runs[0].raw_path)?;
    let second = safe_read::read(root, &manifest.runs[1].raw_path)?;
    if first != second {
        return Err("repeat raw runs differ by bytes".into());
    }
    let semantic_raw = artifact(manifest, "semantic_control_raw_csv");
    validate_run_log(
        manifest,
        &read_text(root, &artifact(manifest, "semantic_control_log").path)?,
        "fresh-semantic-control",
        &semantic_raw.sha256,
        "Latch",
    )?;
    if semantic_raw.sha256 != "2d760f33964d0f6e7de4abc5d236d2dafef9608727b00175c2871a565ef575d2" {
        return Err("Latch semantic control digest drifted".into());
    }
    Ok(())
}

fn validate_run_log(
    manifest: &Manifest,
    text: &str,
    output_token: &str,
    raw_sha256: &str,
    model: &str,
) -> Result<(), String> {
    let buildings = &manifest.sources[0];
    let modelica = &manifest.sources[1];
    for (key, value) in [
        (
            "host_architecture",
            manifest.image.host_architecture.as_str(),
        ),
        (
            "docker_server_architecture",
            manifest.image.docker_server_architecture.as_str(),
        ),
        ("image_index_digest", manifest.image.index_digest.as_str()),
        (
            "image_platform_manifest_digest",
            manifest.image.platform_manifest_digest.as_str(),
        ),
        ("image_config_digest", manifest.image.config_digest.as_str()),
        ("oci_metadata_validation", "raw_digest_and_pinned_graph"),
        ("pull_policy", manifest.regeneration.pull.as_str()),
        ("buildings_remote", buildings.repository.as_str()),
        ("buildings_commit", buildings.commit.as_str()),
        ("buildings_tree", buildings.tree.as_str()),
        ("modelica_remote", modelica.repository.as_str()),
        ("modelica_commit", modelica.commit.as_str()),
        ("modelica_tree", modelica.tree.as_str()),
        (
            "source_materialization",
            "git_archive_exact_committed_bytes",
        ),
        (
            "buildings_package_sha256",
            source_digest(buildings, "Buildings/package.mo")?,
        ),
        (
            "toggle_source_sha256",
            source_digest(buildings, "Buildings/Controls/OBC/CDL/Logical/Toggle.mo")?,
        ),
        (
            "latch_source_sha256",
            source_digest(buildings, "Buildings/Controls/OBC/CDL/Logical/Latch.mo")?,
        ),
        (
            "modelica_package_sha256",
            source_digest(modelica, "Modelica/package.mo")?,
        ),
        (
            "boolean_table_source_sha256",
            source_digest(modelica, "Modelica/Blocks/Sources.mo")?,
        ),
        (
            "modelica_services_sha256",
            source_digest(modelica, "ModelicaServices/package.mo")?,
        ),
        ("complex_sha256", source_digest(modelica, "Complex.mo")?),
        ("docker_command", DOCKER_COMMAND),
        ("output_directory_token", output_token),
        ("selected_model", model),
        ("container_architecture", "aarch64"),
        ("modelica_path", ""),
        ("root_write_probe", "read-only"),
        ("source_write_probe", "read-only"),
        ("network_route_lines", "1"),
        ("cgroup_cpu_max", "400000 100000"),
        ("gcc_version", manifest.image.gcc_version.as_str()),
        ("binutils_version", manifest.image.binutils_version.as_str()),
        ("glibc_version", manifest.image.glibc_version.as_str()),
        (
            "toggle_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Logical/Toggle.mo",
        ),
        (
            "latch_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Logical/Latch.mo",
        ),
        (
            "boolean_table_source",
            "/sources/modelica/Modelica/Blocks/Sources.mo",
        ),
        ("Modelica", modelica.version.as_str()),
        ("Buildings", buildings.version.as_str()),
        ("omc_version", manifest.image.omc_version.as_str()),
        ("raw_sha256", raw_sha256),
        ("runner_complete", "1"),
    ] {
        require_log_value(text, key, value)?;
    }
    for (key, value) in [
        (
            "host_timeout_seconds",
            manifest.regeneration.timeout_seconds,
        ),
        ("cgroup_memory_max", manifest.regeneration.memory_bytes),
        ("cgroup_pids_max", manifest.regeneration.pids_limit),
        ("per_file_limit_bytes", manifest.regeneration.per_file_bytes),
        (
            "output_directory_limit_bytes",
            manifest.regeneration.output_directory_bytes,
        ),
    ] {
        require_log_value(text, key, &value.to_string())?;
    }
    validate_non_root_identity(text)?;
    validate_output_size(text, manifest.regeneration.output_directory_bytes)?;
    require_trimmed_line(text, SIMULATION_OPTIONS)?;
    for success in [
        "The initialization finished successfully",
        "The simulation finished successfully",
    ] {
        if text.matches(success).count() != 1 {
            return Err(format!("run log must contain one {success:?} message"));
        }
    }
    Ok(())
}

fn source_digest<'a>(source: &'a Source, path: &str) -> Result<&'a str, String> {
    source
        .files
        .iter()
        .find(|file| file.path == path)
        .map(|file| file.sha256.as_str())
        .ok_or_else(|| format!("source manifest omits {path}"))
}

fn require_log_value(text: &str, key: &str, expected: &str) -> Result<(), String> {
    let prefix = format!("{key}=");
    let values = text
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix))
        .collect::<Vec<_>>();
    if values == [expected] {
        Ok(())
    } else {
        Err(format!(
            "run log must contain exactly one {key}={expected}; found {values:?}"
        ))
    }
}

fn validate_non_root_identity(text: &str) -> Result<(), String> {
    let values = text
        .lines()
        .filter_map(|line| line.strip_prefix("container_identity="))
        .collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        return Err("run log must contain one container identity".into());
    };
    let (uid, gid) = value
        .split_once(':')
        .ok_or("container identity must be uid:gid")?;
    let uid = uid
        .parse::<u32>()
        .map_err(|_| "container uid is not numeric")?;
    gid.parse::<u32>()
        .map_err(|_| "container gid is not numeric")?;
    if uid == 0 {
        Err("container identity is root".into())
    } else {
        Ok(())
    }
}

fn validate_output_size(text: &str, limit_bytes: u64) -> Result<(), String> {
    let values = text
        .lines()
        .filter_map(|line| line.strip_prefix("output_directory_kib="))
        .collect::<Vec<_>>();
    let [value] = values.as_slice() else {
        return Err("run log must contain one output-directory size".into());
    };
    let kib = value
        .parse::<u64>()
        .map_err(|_| "output-directory size is not numeric")?;
    if kib
        .checked_mul(1024)
        .is_none_or(|bytes| bytes > limit_bytes)
    {
        Err("output-directory size exceeds its recorded limit".into())
    } else {
        Ok(())
    }
}

fn require_trimmed_line(text: &str, expected: &str) -> Result<(), String> {
    let count = text.lines().filter(|line| line.trim() == expected).count();
    if count == 1 {
        Ok(())
    } else {
        Err(format!(
            "run log must contain exactly one line {expected:?}; found {count}"
        ))
    }
}

fn validate_wrappers(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let toggle = read_text(root, &artifact(manifest, "wrapper_model").path)?;
    let latch = read_text(
        root,
        &artifact(manifest, "semantic_control_wrapper_model").path,
    )?;
    if toggle != wrapper_text("Toggle") || latch != wrapper_text("Latch") {
        return Err("wrapper does not match the closed Toggle source".into());
    }
    let token = "Buildings.Controls.OBC.CDL.Logical.Toggle";
    if toggle.matches(token).count() != 1
        || toggle.replace(token, "Buildings.Controls.OBC.CDL.Logical.Latch") != latch
    {
        return Err("semantic wrapper is not the one-token Latch substitution".into());
    }
    for forbidden in ["expected", "assert(", "FMU", "FMI", "oce-"] {
        if toggle.contains(forbidden) || latch.contains(forbidden) {
            return Err(format!("wrapper contains forbidden token {forbidden}"));
        }
    }
    Ok(())
}

fn wrapper_text(class: &str) -> String {
    format!(
        "model TogglePilot\n  Modelica.Blocks.Sources.BooleanTable uSource(\n    table={{30,90,150,210,270,390,450,510}},\n    startValue=true);\n  Modelica.Blocks.Sources.BooleanTable clrSource(\n    table={{310,350,390,430}},\n    startValue=false);\n  Buildings.Controls.OBC.CDL.Logical.{class} dut;\n\n  output Boolean u;\n  output Boolean clr;\n  output Boolean y;\nequation\n  connect(uSource.y, dut.u);\n  connect(clrSource.y, dut.clr);\n  u = uSource.y;\n  clr = clrSource.y;\n  y = dut.y;\nend TogglePilot;\n"
    )
}

#[derive(Deserialize)]
struct Index {
    manifests: Vec<Descriptor>,
}
#[derive(Deserialize)]
struct Descriptor {
    digest: String,
    platform: Option<Platform>,
}
#[derive(Deserialize)]
struct Platform {
    architecture: String,
    os: String,
}
#[derive(Deserialize)]
struct PlatformManifest {
    config: Descriptor,
}

fn validate_oci(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let index_artifact = artifact(manifest, "image_index_json");
    let platform_artifact = artifact(manifest, "image_manifest_json");
    if format!("sha256:{}", index_artifact.sha256) != manifest.image.index_digest
        || format!("sha256:{}", platform_artifact.sha256) != manifest.image.platform_manifest_digest
    {
        return Err("OCI artifact digest does not match image identity".into());
    }
    let index: Index = serde_json::from_slice(&safe_read::read(root, &index_artifact.path)?)
        .map_err(|error| error.to_string())?;
    let arm = index
        .manifests
        .iter()
        .filter(|item| {
            item.platform
                .as_ref()
                .is_some_and(|platform| platform.os == "linux" && platform.architecture == "arm64")
        })
        .collect::<Vec<_>>();
    if arm.len() != 1 || arm[0].digest != manifest.image.platform_manifest_digest {
        return Err("OCI index does not select the pinned linux/arm64 manifest".into());
    }
    let platform: PlatformManifest =
        serde_json::from_slice(&safe_read::read(root, &platform_artifact.path)?)
            .map_err(|error| error.to_string())?;
    if platform.config.digest != manifest.image.config_digest {
        return Err("OCI manifest does not select the pinned config".into());
    }
    Ok(())
}

fn validate_projection_mutation(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let text = read_text(root, &artifact(manifest, "projection_mutation_log").path)?;
    let expected = [
        (
            "projection_mutation",
            "contiguous equal-time selection changed from last to first",
        ),
        ("working_tree_modified", "false"),
        ("mutated_compile", "PASS"),
        ("mutated_input", "toggle-run-a.raw.csv"),
        (
            "mutated_input_sha256",
            "cf9333debedf335ae28103b6bcc70229ac78afba4d99701a51d1771d9a3e6641",
        ),
        ("mutated_raw_rows", "34"),
        ("mutated_canonical_rows", "22"),
        (
            "mutated_group_sizes",
            "1,2,1,2,1,2,1,2,1,2,1,2,2,1,2,1,2,2,1,2,1,2",
        ),
        (
            "mutated_canonical_time_bits",
            "0000000000000000,403e000000001dff,404e000000000000,4056800000000780,405e000000000000,4062c000000003c1,4066800000000000,406a4000000003c1,406e000000000000,4070e000000001e0,4072c00000000000,4073600000000320,4075e000000002d0,4076800000000000,40786000000003c1,407a400000000000,407ae00000000320,407c2000000003c1,407e000000000000,407fe000000003c1,4080e00000000000,4082c00000000000",
        ),
        ("mutated_schedule_result", "FAIL"),
        (
            "mutated_schedule_mismatch_rows",
            "1,3,5,7,9,11,12,14,16,17,19",
        ),
        ("mutated_schedule_first_mismatch_row", "1"),
        (
            "mutated_schedule_first_mismatch_time_bits",
            "403e000000001dff",
        ),
        ("mutated_grouping_result", "PASS"),
        ("mutated_timestamp_bits_result", "PASS"),
        ("restoration_result", "PASS"),
        (
            "restored_canonicalizer_sha256",
            artifact(manifest, "canonicalizer_source").sha256.as_str(),
        ),
    ];
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() != expected.len() {
        return Err("projection-mutation log has an unexpected line count".into());
    }
    for (line, (key, value)) in lines.iter().zip(expected) {
        if *line != format!("{key}={value}") {
            return Err(format!("projection-mutation evidence has invalid {key}"));
        }
    }
    Ok(())
}

fn artifact<'a>(manifest: &'a Manifest, role: &str) -> &'a Artifact {
    manifest
        .artifacts
        .iter()
        .find(|item| item.role == role)
        .unwrap_or_else(|| panic!("validated manifest omits role {role}"))
}
fn read_text(root: &Path, relative: &str) -> Result<String, String> {
    String::from_utf8(safe_read::read(root, relative)?).map_err(|error| error.to_string())
}
fn sha256_bytes(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_paths_reject_ambiguous_spellings() {
        for path in [
            "", "/tmp/x", "a//b", "a/./b", "a/../b", "a\\b", "a:b", "a*", "a?", "a. ", "a.",
        ] {
            assert!(valid_path(path).is_err(), "{path:?}");
        }
        assert!(valid_path("crates/example/file.csv").is_ok());
    }

    #[test]
    fn log_values_are_unique_and_non_root() {
        assert!(require_log_value("key=value\n", "key", "value").is_ok());
        assert!(require_log_value("key=value\nkey=value\n", "key", "value").is_err());
        assert!(validate_non_root_identity("container_identity=501:20\n").is_ok());
        assert!(validate_non_root_identity("container_identity=0:20\n").is_err());
    }
}
