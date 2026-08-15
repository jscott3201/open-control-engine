//! Digest, OCI, source, wrapper, run-log, and cross-architecture validation.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::expectations::{CANONICAL_SHA, CONTROL_SHA, RAW_SHA};
use super::safe_read;
use super::schema::{Artifact, Manifest};

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
    validate_native_provenance(manifest)?;
    validate_runs(manifest, &root)?;
    validate_wrappers(manifest, &root)?;
    validate_oci(manifest, &root)?;
    validate_projection_records(manifest, &root)?;
    validate_cross_architecture(manifest, &root)?;
    validate_tool_contract(manifest, &root)
}

fn validate_native_provenance(manifest: &Manifest) -> Result<(), String> {
    for architecture in &manifest.architectures {
        let inputs = &architecture.generator_inputs;
        for (actual, role) in [
            (&inputs.line_pilot_sha256, "wrapper_model"),
            (&inputs.line_flag_pilot_sha256, "flag_control_wrapper_model"),
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
        ] {
            if actual != &artifact(manifest, role).sha256 {
                return Err(format!(
                    "{} native provenance does not bind {role}",
                    architecture.name
                ));
            }
        }
    }
    Ok(())
}

fn validate_role_path(artifact: &Artifact) -> Result<(), String> {
    valid_path(&artifact.path)?;
    let fixture = "crates/oce-conformance/tests/fixtures/open_modelica/reals_line/";
    let canonicalizer = "crates/oce-cxf/tests/open_modelica_line_reference/";
    let tool = "tools/openmodelica-line-reference/";
    let allowed = match artifact.role.as_str() {
        "canonicalizer_source" => artifact.path.starts_with(canonicalizer),
        "evidence_workflow" => artifact.path == ".github/workflows/openmodelica-line-evidence.yml",
        "arm64_manifest_source" | "amd64_manifest_source" => artifact.path.starts_with(tool),
        role if role.starts_with("arm64_") || role.starts_with("amd64_") => {
            artifact.path.starts_with(fixture)
        }
        "image_index_json" | "cross_architecture_log" => artifact.path.starts_with(fixture),
        _ => artifact.path.starts_with(tool),
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
    for (architecture, expected) in manifest
        .architectures
        .iter()
        .zip([("arm64", "aarch64", "arm64"), ("amd64", "x86_64", "amd64")])
    {
        let prefix = expected.0;
        for (run, role, token) in [
            (
                &architecture.runs[0],
                format!("{prefix}_run_a_log"),
                "fresh-run-a",
            ),
            (
                &architecture.runs[1],
                format!("{prefix}_run_b_log"),
                "fresh-run-b",
            ),
        ] {
            let log = artifact(manifest, &role);
            if run.log_sha256 != log.sha256 || run.raw_sha256 != RAW_SHA {
                return Err("run record is not bound to its retained artifacts".into());
            }
            validate_run_log(
                &read_text(root, &log.path)?,
                architecture,
                token,
                "Line",
                RAW_SHA,
                expected.1,
                expected.2,
            )?;
        }
        let raw_a = artifact(manifest, &format!("{prefix}_raw_run_a_csv"));
        let raw_b = artifact(manifest, &format!("{prefix}_raw_run_b_csv"));
        if raw_a.sha256 != RAW_SHA
            || raw_b.sha256 != RAW_SHA
            || safe_read::read(root, &raw_a.path)? != safe_read::read(root, &raw_b.path)?
        {
            return Err(format!("{prefix} repeat raw runs differ"));
        }
        let control = artifact(manifest, &format!("{prefix}_flag_control_raw_csv"));
        if control.sha256 != CONTROL_SHA {
            return Err("flag-control raw digest drifted".into());
        }
        validate_run_log(
            &read_text(
                root,
                &artifact(manifest, &format!("{prefix}_flag_control_log")).path,
            )?,
            architecture,
            "fresh-flag-control",
            "FlagControl",
            CONTROL_SHA,
            expected.1,
            expected.2,
        )?;
    }
    Ok(())
}

fn validate_run_log(
    text: &str,
    architecture: &super::schema::Architecture,
    token: &str,
    model: &str,
    raw_sha: &str,
    container: &str,
    host: &str,
) -> Result<(), String> {
    for (key, expected) in [
        ("host_architecture", host),
        ("docker_server_architecture", container),
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
            "line_pilot_sha256",
            architecture.generator_inputs.line_pilot_sha256.as_str(),
        ),
        (
            "line_flag_pilot_sha256",
            architecture
                .generator_inputs
                .line_flag_pilot_sha256
                .as_str(),
        ),
        (
            "runner_sha256",
            architecture.generator_inputs.runner_sha256.as_str(),
        ),
        (
            "regenerate_sha256",
            architecture.generator_inputs.regenerate_sha256.as_str(),
        ),
        (
            "canonicalizer_sha256",
            architecture.generator_inputs.canonicalizer_sha256.as_str(),
        ),
        (
            "tool_main_sha256",
            architecture.generator_inputs.tool_main_sha256.as_str(),
        ),
        (
            "tool_cargo_toml_sha256",
            architecture
                .generator_inputs
                .tool_cargo_toml_sha256
                .as_str(),
        ),
        (
            "tool_cargo_lock_sha256",
            architecture
                .generator_inputs
                .tool_cargo_lock_sha256
                .as_str(),
        ),
        (
            "architecture_generator_sha256",
            architecture
                .generator_inputs
                .architecture_generator_sha256
                .as_str(),
        ),
        (
            "architecture_verifier_sha256",
            architecture
                .generator_inputs
                .architecture_verifier_sha256
                .as_str(),
        ),
        ("pull_policy", "never"),
        ("output_directory_token", token),
        ("selected_model", model),
        ("container_architecture", container),
        ("modelica_path", ""),
        ("root_write_probe", "read-only"),
        ("source_write_probe", "read-only"),
        ("reference_write_probe", "read-only"),
        ("network_route_lines", "1"),
        ("cgroup_memory_max", "2147483648"),
        ("cgroup_pids_max", "256"),
        ("cgroup_cpu_max", "400000 100000"),
        ("gcc_version", "11.4.0"),
        ("binutils_version", "2.38"),
        ("glibc_version", "2.35"),
        ("omc_version", "OpenModelica 1.25.1"),
        (
            "line_source",
            "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Line.mo",
        ),
        (
            "constant_source",
            "/sources/modelica/Modelica/Blocks/Sources.mo",
        ),
        (
            "time_table_source",
            "/sources/modelica/Modelica/Blocks/Sources.mo",
        ),
        ("Modelica", "4.1.0"),
        ("Buildings", "14.0.0"),
        ("omc_warning_count", "0"),
        ("raw_sha256", raw_sha),
        ("runner_complete", "1"),
    ] {
        require_log_value(text, key, expected)?;
    }
    let identity = unique_log_value(text, "container_identity")?;
    let (uid, gid) = identity
        .split_once(':')
        .ok_or("container identity must be uid:gid")?;
    if uid.parse::<u32>().map_err(|_| "container uid is invalid")? == 0
        || gid.parse::<u32>().is_err()
    {
        return Err("container identity is root or invalid".into());
    }
    let output_kib = unique_log_value(text, "output_directory_kib")?
        .parse::<u64>()
        .map_err(|_| "output size is invalid")?;
    if output_kib * 1024 > 268_435_456 {
        return Err("output size exceeds bound".into());
    }
    if text
        .lines()
        .any(|line| line.to_ascii_lowercase().contains("warning") && line != "omc_warning_count=0")
    {
        return Err("OpenModelica warning found".into());
    }
    let simulation = "simulationOptions = \"startTime = 0.0, stopTime = 300.0, numberOfIntervals = 5, tolerance = 1e-9, method = 'dassl', fileNamePrefix = 'LinePilot', options = '', outputFormat = 'csv', variableFilter = '^(x1|f1|x2|f2|u|yBoth|yBelow|yAbove|yUnlimited)$', cflags = '', simflags = ''\",";
    if text
        .lines()
        .filter(|line| line.trim() == simulation)
        .count()
        != 1
    {
        return Err("simulation options missing or repeated".into());
    }
    Ok(())
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
    if unique_log_value(text, key)? == expected {
        Ok(())
    } else {
        Err(format!("run log has unsupported {key}"))
    }
}

fn validate_wrappers(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let wrapper = read_text(root, &artifact(manifest, "wrapper_model").path)?;
    let control = read_text(root, &artifact(manifest, "flag_control_wrapper_model").path)?;
    let token = "Line below(limitBelow=true, limitAbove=false);";
    let mutation = "Line below(limitBelow=true, limitAbove=true);";
    if wrapper
        .matches("Buildings.Controls.OBC.CDL.Reals.Line")
        .count()
        != 4
        || wrapper.matches(token).count() != 1
        || wrapper.replace(token, mutation) != control
    {
        return Err("Line wrappers do not close over the one-token flag control".into());
    }
    for forbidden in ["expected", "assert(", "FMU", "FMI", "oce-"] {
        if wrapper.contains(forbidden) || control.contains(forbidden) {
            return Err(format!("wrapper contains forbidden token {forbidden}"));
        }
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
        return Err("OCI index artifact is not the image identity".into());
    }
    let index: Index = serde_json::from_slice(&safe_read::read(root, &index_artifact.path)?)
        .map_err(|error| error.to_string())?;
    for architecture in &manifest.architectures {
        let selected = index
            .manifests
            .iter()
            .filter(|item| {
                item.platform.as_ref().is_some_and(|platform| {
                    platform.os == "linux" && platform.architecture == architecture.name
                })
            })
            .collect::<Vec<_>>();
        if selected.len() != 1 || selected[0].digest != architecture.platform_manifest_digest {
            return Err(format!(
                "OCI index does not select linux/{}",
                architecture.name
            ));
        }
        let role = format!("{}_platform_image_manifest_json", architecture.name);
        let platform_artifact = artifact(manifest, &role);
        if format!("sha256:{}", platform_artifact.sha256) != architecture.platform_manifest_digest {
            return Err("OCI platform artifact identity mismatch".into());
        }
        let platform: PlatformManifest =
            serde_json::from_slice(&safe_read::read(root, &platform_artifact.path)?)
                .map_err(|error| error.to_string())?;
        if platform.config.digest != architecture.config_digest {
            return Err("OCI platform config identity mismatch".into());
        }
    }
    Ok(())
}

fn validate_projection_records(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let canonicalizer = artifact(manifest, "canonicalizer_source");
    let expected = [
        "projection_mutation=contiguous equal-time selection changed from last to first".to_string(),
        "working_tree_modified=false".into(), "mutated_compile=PASS".into(),
        "mutated_input=line-run-a.raw.csv".into(), format!("mutated_input_sha256={RAW_SHA}"),
        "mutated_raw_rows=15".into(), "mutated_canonical_rows=10".into(),
        "mutated_group_sizes=1,1,2,1,2,1,2,1,2,2".into(),
        "mutated_canonical_time_bits=0000000000000000,404e000000000000,404e000000000eff,405e000000000000,405e000000000781,4066800000000000,40668000000003c1,406e000000000000,406e0000000003c1,4072c00000000000".into(),
        "mutated_schedule_result=FAIL".into(), "mutated_schedule_mismatch_rows=2,4,6,8".into(),
        "mutated_schedule_first_mismatch_row=2".into(), "mutated_schedule_first_mismatch_time_bits=404e000000000eff".into(),
        "mutated_grouping_result=PASS".into(), "mutated_timestamp_bits_result=PASS".into(),
        "restoration_result=PASS".into(), format!("restored_canonicalizer_sha256={}", canonicalizer.sha256),
    ];
    for architecture in ["arm64", "amd64"] {
        let text = read_text(
            root,
            &artifact(manifest, &format!("{architecture}_projection_mutation_log")).path,
        )?;
        if text.lines().ne(expected.iter().map(String::as_str)) {
            return Err(format!(
                "{architecture} projection-mutation evidence drifted"
            ));
        }
    }
    Ok(())
}

fn validate_cross_architecture(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let arm = artifact(manifest, "arm64_canonical_csv");
    let amd = artifact(manifest, "amd64_canonical_csv");
    if arm.sha256 != CANONICAL_SHA
        || amd.sha256 != CANONICAL_SHA
        || safe_read::read(root, &arm.path)? != safe_read::read(root, &amd.path)?
    {
        return Err("cross-architecture canonical bytes differ".into());
    }
    let expected = format!(
        "comparison=canonical bytes\narm64_sha256={CANONICAL_SHA}\namd64_sha256={CANONICAL_SHA}\nresult=PASS\n"
    );
    if read_text(root, &artifact(manifest, "cross_architecture_log").path)? != expected {
        return Err("cross-architecture record drifted".into());
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
        "--pids-limit 256",
        "deadline_call docker run",
        "cleanup_container",
        "generate_architecture.py",
        "verify_evidence.py\" architecture",
    ] {
        if !regeneration.contains(required) {
            return Err(format!("regeneration omits {required}"));
        }
    }
    for required in [
        "simflags=\"\"",
        "stopTime=300.0",
        "numberOfIntervals=5",
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
        return Err("event emission or manual-only workflow contract drifted".into());
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
}
