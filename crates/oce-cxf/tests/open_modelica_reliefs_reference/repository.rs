//! Digest, native-record, run-log, OCI, and tool-contract validation.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::expectations::CANONICAL_SHA;
use super::schema::{Architecture, Artifact, GeneratorInputs, Manifest, NativeArchitectureRecord};

pub(super) fn validate(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut roles = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for artifact in &manifest.artifacts {
        if !roles.insert(&artifact.role) || !paths.insert(&artifact.path) {
            return Err("artifact role or path is reused".into());
        }
        valid_artifact_path(artifact)?;
        if sha256(&super::safe_read::read(&root, &artifact.path)?) != artifact.sha256 {
            return Err(format!("artifact digest mismatch: {}", artifact.path));
        }
    }
    validate_native_records(manifest, &root)?;
    validate_generator_bindings(manifest)?;
    validate_run_logs(manifest, &root)?;
    validate_wrappers(manifest, &root)?;
    validate_oci(manifest, &root)?;
    validate_cross_architecture(manifest, &root)?;
    validate_tool_contract(manifest, &root)
}

fn validate_native_records(manifest: &Manifest, root: &Path) -> Result<(), String> {
    for architecture in &manifest.architectures {
        let path = artifact(
            manifest,
            &format!("{}_architecture_record", architecture.name),
        );
        let record: NativeArchitectureRecord =
            serde_json::from_slice(&super::safe_read::read(root, &path.path)?)
                .map_err(|error| error.to_string())?;
        if record.format != "oce-openmodelica-reliefs-native-architecture-v4"
            || record.architecture != architecture.name
            || !native_matches_manifest(&record, architecture)
        {
            return Err(format!(
                "{} native architecture record drifted",
                architecture.name
            ));
        }
    }
    Ok(())
}

fn native_matches_manifest(record: &NativeArchitectureRecord, value: &Architecture) -> bool {
    record.platform == value.platform
        && record.host_architecture == value.host_architecture
        && record.docker_server_architecture == value.docker_server_architecture
        && record.container_architecture == value.container_architecture
        && record.platform_manifest_digest == value.platform_manifest_digest
        && record.config_digest == value.config_digest
        && record.repository_revision == value.repository_revision
        && record.generator_provenance_scope == value.generator_provenance_scope
        && record.generator_inputs == value.generator_inputs
        && record.artifact_toolchain == value.artifact_toolchain
        && record.source_materialization == value.source_materialization
        && record.source_files == value.source_files
        && record.omc_version == value.omc_version
        && record.gcc_version == value.gcc_version
        && record.binutils_version == value.binutils_version
        && record.glibc_version == value.glibc_version
        && record.raw_run_a_sha256 == value.raw_run_a_sha256
        && record.raw_run_b_sha256 == value.raw_run_b_sha256
        && record.canonical_sha256 == value.canonical_sha256
        && record.parameter_control_raw_sha256 == value.parameter_control_raw_sha256
        && record.parameter_control_canonical_sha256 == value.parameter_control_canonical_sha256
        && record.final_clamp_raw_sha256 == value.final_clamp_raw_sha256
        && record.final_clamp_canonical_sha256 == value.final_clamp_canonical_sha256
        && record.projection_mutation == value.projection_mutation
        && record.runs == value.runs
}

fn validate_generator_bindings(manifest: &Manifest) -> Result<(), String> {
    for architecture in &manifest.architectures {
        for (digest, role) in generator_bindings(&architecture.generator_inputs) {
            if digest != artifact(manifest, role).sha256 {
                return Err(format!(
                    "{} generator input does not bind {role}",
                    architecture.name
                ));
            }
        }
    }
    Ok(())
}

fn generator_bindings(inputs: &GeneratorInputs) -> [(&str, &str); 24] {
    [
        (&inputs.reliefs_pilot_sha256, "wrapper_model"),
        (
            &inputs.parameter_pilot_sha256,
            "parameter_control_wrapper_model",
        ),
        (&inputs.clamp_pilot_sha256, "final_clamp_wrapper_model"),
        (&inputs.runner_sha256, "runner_script"),
        (&inputs.regenerate_sha256, "regeneration_script"),
        (&inputs.canonicalizer_sha256, "canonicalizer_source"),
        (&inputs.tool_main_sha256, "tool_main_source"),
        (&inputs.tool_cargo_toml_sha256, "tool_cargo_toml"),
        (&inputs.tool_cargo_lock_sha256, "tool_cargo_lock"),
        (
            &inputs.architecture_generator_sha256,
            "architecture_generator_script",
        ),
        (
            &inputs.architecture_verifier_sha256,
            "evidence_validator_script",
        ),
        (
            &inputs.projection_verifier_sha256,
            "projection_validator_script",
        ),
        (&inputs.safe_file_helper_sha256, "safe_file_helper_script"),
        (&inputs.evidence_workflow_sha256, "evidence_workflow"),
        (&inputs.oci_materializer_sha256, "oci_materializer_script"),
        (&inputs.deadline_sha256, "deadline_script"),
        (&inputs.deadline_test_sha256, "deadline_test_script"),
        (&inputs.container_cleanup_sha256, "container_cleanup_script"),
        (
            &inputs.container_cleanup_test_sha256,
            "container_cleanup_test_script",
        ),
        (&inputs.output_publish_sha256, "output_publish_script"),
        (
            &inputs.output_publish_test_sha256,
            "output_publish_test_script",
        ),
        (&inputs.oci_index_source_sha256, "oci_index_source"),
        (
            &inputs.arm64_manifest_source_sha256,
            "arm64_manifest_source",
        ),
        (
            &inputs.amd64_manifest_source_sha256,
            "amd64_manifest_source",
        ),
    ]
}

fn validate_run_logs(manifest: &Manifest, root: &Path) -> Result<(), String> {
    for architecture in &manifest.architectures {
        let (host, machine) = if architecture.name == "arm64" {
            ("arm64", "aarch64")
        } else {
            ("amd64", "x86_64")
        };
        for (suffix, token, model, raw) in [
            (
                "run_a_log",
                "fresh-run-a",
                "Reliefs",
                &architecture.raw_run_a_sha256,
            ),
            (
                "run_b_log",
                "fresh-run-b",
                "Reliefs",
                &architecture.raw_run_b_sha256,
            ),
            (
                "parameter_control_log",
                "fresh-parameter-control",
                "ParameterControl",
                &architecture.parameter_control_raw_sha256,
            ),
            (
                "final_clamp_log",
                "fresh-final-clamp",
                "FinalClamp",
                &architecture.final_clamp_raw_sha256,
            ),
        ] {
            let role = format!("{}_{suffix}", architecture.name);
            let text = read_text(root, &artifact(manifest, &role).path)?;
            validate_run_log(&text, architecture, host, machine, token, model, raw)?;
        }
        for (run, suffix) in architecture.runs.iter().zip(["run_a_log", "run_b_log"]) {
            if run.log_sha256
                != artifact(manifest, &format!("{}_{suffix}", architecture.name)).sha256
            {
                return Err("native run log binding drifted".into());
            }
        }
        if artifact(manifest, &format!("{}_raw_run_a_csv", architecture.name)).sha256
            != architecture.raw_run_a_sha256
            || artifact(manifest, &format!("{}_raw_run_b_csv", architecture.name)).sha256
                != architecture.raw_run_b_sha256
            || artifact(
                manifest,
                &format!("{}_parameter_control_raw_csv", architecture.name),
            )
            .sha256
                != architecture.parameter_control_raw_sha256
            || artifact(
                manifest,
                &format!("{}_final_clamp_raw_csv", architecture.name),
            )
            .sha256
                != architecture.final_clamp_raw_sha256
        {
            return Err("raw artifact binding drifted".into());
        }
    }
    Ok(())
}

fn validate_run_log(
    text: &str,
    architecture: &Architecture,
    host: &str,
    machine: &str,
    token: &str,
    model: &str,
    raw: &str,
) -> Result<(), String> {
    for (key, expected) in [
        ("host_architecture", host),
        ("docker_server_architecture", machine),
        ("container_architecture", machine),
        (
            "image_index_digest",
            "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864",
        ),
        (
            "image_platform_manifest_digest",
            architecture.platform_manifest_digest.as_str(),
        ),
        ("image_config_digest", architecture.config_digest.as_str()),
        (
            "repository_revision",
            architecture.repository_revision.as_str(),
        ),
        (
            "generator_provenance_scope",
            "native_generation_and_publication",
        ),
        ("pull_policy", "never"),
        ("output_directory_token", token),
        ("selected_model", model),
        ("modelica_path", ""),
        ("root_write_probe", "read-only"),
        ("source_write_probe", "read-only"),
        ("reference_write_probe", "read-only"),
        ("network_route_lines", "1"),
        ("cgroup_memory_max", "2147483648"),
        ("cgroup_pids_max", "256"),
        ("cgroup_cpu_max", "400000 100000"),
        ("omc_version", "OpenModelica 1.25.1"),
        ("gcc_version", "11.4.0"),
        ("binutils_version", "2.38"),
        ("glibc_version", "2.35"),
        ("Modelica", "4.1.0"),
        ("Buildings", "14.0.0"),
        ("omc_warning_count", "0"),
        ("raw_sha256", raw),
        ("runner_complete", "1"),
    ] {
        require_log_value(text, key, expected)?;
    }
    for (digest, role) in generator_bindings(&architecture.generator_inputs) {
        let key = match role {
            "wrapper_model" => "reliefs_pilot_sha256",
            "parameter_control_wrapper_model" => "parameter_pilot_sha256",
            "final_clamp_wrapper_model" => "clamp_pilot_sha256",
            "runner_script" => "runner_sha256",
            "regeneration_script" => "regenerate_sha256",
            "canonicalizer_source" => "canonicalizer_sha256",
            "tool_main_source" => "tool_main_sha256",
            "tool_cargo_toml" => "tool_cargo_toml_sha256",
            "tool_cargo_lock" => "tool_cargo_lock_sha256",
            "architecture_generator_script" => "architecture_generator_sha256",
            "evidence_validator_script" => "architecture_verifier_sha256",
            "projection_validator_script" => "projection_verifier_sha256",
            "safe_file_helper_script" => "safe_file_helper_sha256",
            "evidence_workflow" => "evidence_workflow_sha256",
            "oci_materializer_script" => "oci_materializer_sha256",
            "deadline_script" => "deadline_sha256",
            "deadline_test_script" => "deadline_test_sha256",
            "container_cleanup_script" => "container_cleanup_sha256",
            "container_cleanup_test_script" => "container_cleanup_test_sha256",
            "output_publish_script" => "output_publish_sha256",
            "output_publish_test_script" => "output_publish_test_sha256",
            "oci_index_source" => "oci_index_source_sha256",
            "arm64_manifest_source" => "arm64_manifest_source_sha256",
            "amd64_manifest_source" => "amd64_manifest_source_sha256",
            other => other,
        };
        require_log_value(text, key, digest)?;
    }
    let sources: Vec<super::schema::NativeSourceFile> =
        serde_json::from_str(unique_log_value(text, "source_files_json")?)
            .map_err(|error| error.to_string())?;
    if sources != architecture.source_files {
        return Err("run log source cone drifted".into());
    }
    let identity = unique_log_value(text, "container_identity")?;
    let (uid, gid) = identity
        .split_once(':')
        .ok_or("container identity is not uid:gid")?;
    if uid.parse::<u32>().map_err(|_| "invalid container uid")? == 0 || gid.parse::<u32>().is_err()
    {
        return Err("container identity is root or invalid".into());
    }
    let peak = unique_log_value(text, "observed_cgroup_peak_bytes")?
        .parse::<u64>()
        .map_err(|_| "invalid cgroup peak")?;
    if peak == 0 || peak > 2_147_483_648 {
        return Err("cgroup memory peak is outside the bound".into());
    }
    if text
        .lines()
        .any(|line| line.to_ascii_lowercase().contains("warning") && line != "omc_warning_count=0")
    {
        return Err("OpenModelica warning found".into());
    }
    Ok(())
}

fn validate_wrappers(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let main = read_text(root, &artifact(manifest, "wrapper_model").path)?;
    let parameter = read_text(
        root,
        &artifact(manifest, "parameter_control_wrapper_model").path,
    )?;
    let clamp = read_text(root, &artifact(manifest, "final_clamp_wrapper_model").path)?;
    let parameter_token = "uOutDamMax=0.0,";
    let expected_clamp = main
        .replace("uOutDamMinSource(k=0.25)", "uOutDamMinSource(k=0.875)")
        .replace("uOutDamMaxSource(k=0.875)", "uOutDamMaxSource(k=0.25)")
        .replace("uRetDamMinSource(k=0.125)", "uRetDamMinSource(k=0.75)")
        .replace("uRetDamMaxSource(k=0.75)", "uRetDamMaxSource(k=0.125)");
    if main
        .matches("Subsequences.Modulations.Reliefs mod(")
        .count()
        != 1
        || main.replacen(parameter_token, "uOutDamMax=-0.125,", 1) != parameter
        || expected_clamp != clamp
        || [main.as_str(), parameter.as_str(), clamp.as_str()]
            .iter()
            .any(|text| {
                ["expected", "assert(", "FMU", "FMI", "oce-"]
                    .iter()
                    .any(|token| text.contains(token))
            })
    {
        return Err("wrapper control closure drifted".into());
    }
    Ok(())
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
    if format!("sha256:{}", index_artifact.sha256) != manifest.image.index_digest {
        return Err("OCI index identity drifted".into());
    }
    let index: Index = serde_json::from_slice(&super::safe_read::read(root, &index_artifact.path)?)
        .map_err(|error| error.to_string())?;
    for architecture in &manifest.architectures {
        let selected = index
            .manifests
            .iter()
            .filter(|entry| {
                entry.platform.as_ref().is_some_and(|platform| {
                    platform.os == "linux" && platform.architecture == architecture.name
                })
            })
            .collect::<Vec<_>>();
        if selected.len() != 1 || selected[0].digest != architecture.platform_manifest_digest {
            return Err("OCI platform selection drifted".into());
        }
        let platform: PlatformManifest = serde_json::from_slice(&super::safe_read::read(
            root,
            &artifact(
                manifest,
                &format!("{}_platform_image_manifest_json", architecture.name),
            )
            .path,
        )?)
        .map_err(|error| error.to_string())?;
        if platform.config.digest != architecture.config_digest {
            return Err("OCI config identity drifted".into());
        }
    }
    Ok(())
}

fn validate_cross_architecture(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let arm = artifact(manifest, "arm64_canonical_csv");
    let amd = artifact(manifest, "amd64_canonical_csv");
    if arm.sha256 != CANONICAL_SHA
        || amd.sha256 != CANONICAL_SHA
        || super::safe_read::read(root, &arm.path)? != super::safe_read::read(root, &amd.path)?
    {
        return Err("cross-architecture canonical bytes differ".into());
    }
    let expected = format!(
        "comparison=canonical bytes\narm64_sha256={CANONICAL_SHA}\namd64_sha256={CANONICAL_SHA}\nresult=PASS\n"
    );
    if read_text(root, &artifact(manifest, "cross_architecture_log").path)? != expected {
        return Err("cross-architecture log drifted".into());
    }
    Ok(())
}

fn validate_tool_contract(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let regeneration = read_text(root, &artifact(manifest, "regeneration_script").path)?;
    let runner = read_text(root, &artifact(manifest, "runner_script").path)?;
    let workflow = read_text(root, &artifact(manifest, "evidence_workflow").path)?;
    for required in [
        "--network none",
        "--read-only",
        "--cap-drop ALL",
        "--security-opt no-new-privileges",
        "--pull=never",
        "--memory 2g",
        "--memory-swap 2g",
        "--pids-limit 256",
        "deadline_call docker run",
        "cleanup_container",
    ] {
        if !regeneration.contains(required) {
            return Err(format!("regeneration omits {required}"));
        }
    }
    for required in [
        "stopTime=420.0",
        "numberOfIntervals=7",
        "simflags=\"\"",
        "omc_warning_count",
        "touch /out/.oce-complete",
    ] {
        if !runner.contains(required) {
            return Err(format!("runner omits {required}"));
        }
    }
    if runner.contains("-noEventEmit")
        || workflow.contains("pull_request:")
        || !workflow.contains("workflow_dispatch:")
    {
        return Err("event emission or manual workflow contract drifted".into());
    }
    Ok(())
}

fn valid_artifact_path(artifact: &Artifact) -> Result<(), String> {
    let value = artifact.path.as_str();
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
    let fixture = "crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs/";
    let allowed = match artifact.role.as_str() {
        "canonicalizer_source" => {
            value.starts_with("crates/oce-cxf/tests/open_modelica_reliefs_reference/")
        }
        "evidence_workflow" => value == ".github/workflows/openmodelica-reliefs-evidence.yml",
        "arm64_manifest_source" | "amd64_manifest_source" => {
            value.starts_with("tools/openmodelica-reliefs-reference/")
        }
        role if role.starts_with("arm64_")
            || role.starts_with("amd64_")
            || role == "image_index_json"
            || role == "cross_architecture_log" =>
        {
            value.starts_with(fixture)
        }
        _ => value.starts_with("tools/openmodelica-reliefs-reference/"),
    };
    allowed
        .then_some(())
        .ok_or_else(|| format!("artifact role escapes its subtree: {}", artifact.role))
}

fn unique_log_value<'a>(text: &'a str, key: &str) -> Result<&'a str, String> {
    let prefix = format!("{key}=");
    let values = text
        .lines()
        .filter_map(|line| line.strip_prefix(&prefix))
        .collect::<Vec<_>>();
    match values.as_slice() {
        [value] => Ok(value),
        _ => Err(format!("run log must contain exactly one {key}")),
    }
}
fn require_log_value(text: &str, key: &str, expected: &str) -> Result<(), String> {
    (unique_log_value(text, key)? == expected)
        .then_some(())
        .ok_or_else(|| format!("run log has unsupported {key}"))
}
fn artifact<'a>(manifest: &'a Manifest, role: &str) -> &'a Artifact {
    manifest
        .artifacts
        .iter()
        .find(|item| item.role == role)
        .unwrap_or_else(|| panic!("validated manifest omits {role}"))
}
fn read_text(root: &Path, relative: &str) -> Result<String, String> {
    String::from_utf8(super::safe_read::read(root, relative)?).map_err(|error| error.to_string())
}
fn sha256(bytes: &[u8]) -> String {
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
    fn ambiguous_repository_paths_are_rejected() {
        for path in [
            "", "/tmp/x", "a//b", "a/./b", "a/../b", "a\\b", "a:b", "a*", "a?", "a. ", "a.",
        ] {
            let artifact = Artifact {
                role: "runner_script".into(),
                path: path.into(),
                sha256: "0".repeat(64),
            };
            assert!(valid_artifact_path(&artifact).is_err(), "{path:?}");
        }
    }
}
