//! Safe path, digest, OCI, run-log, and wrapper validation.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use super::schema::{Artifact, Manifest};

const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024;
const DOCKER_COMMAND: &str = "docker run --pull=never --platform linux/arm64 --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro";
const SIMULATION_OPTIONS: &str = "simulationOptions = \"startTime = 0.0, stopTime = 240.0, numberOfIntervals = 4, tolerance = 1e-9, method = 'dassl', fileNamePrefix = 'NandPilot', options = '', outputFormat = 'csv', variableFilter = '.*', cflags = '', simflags = ''\",";

pub(super) fn validate(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let mut paths = BTreeSet::new();
    let mut roles = BTreeSet::new();
    for artifact in &manifest.artifacts {
        if !roles.insert(artifact.role.as_str()) || !paths.insert(artifact.path.as_str()) {
            return Err("artifact role or path is reused".into());
        }
        validate_role_path(artifact)?;
        let path = resolve(&root, &artifact.path)?;
        if sha256_file(&path)? != artifact.sha256 {
            return Err(format!("artifact digest mismatch: {}", artifact.path));
        }
    }
    validate_runs(manifest, &root)?;
    validate_wrappers(manifest, &root)?;
    validate_oci(manifest, &root)?;
    validate_projection_mutation(manifest, &root)?;
    Ok(())
}

fn validate_role_path(artifact: &Artifact) -> Result<(), String> {
    valid_path(&artifact.path)?;
    let fixture = "crates/oce-conformance/tests/fixtures/open_modelica/logical_nand/";
    let canonicalizer = "crates/oce-cxf/tests/open_modelica_reference/";
    let tool = "tools/openmodelica-reference/";
    let nand_tool = "tools/openmodelica-reference/nand/";
    let allowed = match artifact.role.as_str() {
        "canonicalizer_source" => artifact.path.starts_with(canonicalizer),
        "regeneration_script"
        | "runner_script"
        | "semantic_control_wrapper_model"
        | "wrapper_model" => artifact.path.starts_with(nand_tool),
        "tool_cargo_lock" | "tool_cargo_toml" | "tool_main_source" => {
            artifact.path.starts_with(tool)
        }
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

fn resolve(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let joined = root.join(relative);
    let metadata = std::fs::symlink_metadata(&joined).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "artifact is not a regular non-symlink file: {relative}"
        ));
    }
    let canonical = joined.canonicalize().map_err(|error| error.to_string())?;
    if !canonical.starts_with(root) {
        return Err(format!("artifact resolves outside repository: {relative}"));
    }
    Ok(canonical)
}

fn validate_runs(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let raw = artifact(manifest, "raw_csv");
    for (run, role) in manifest.runs.iter().zip(["run_a_log", "run_b_log"]) {
        let log = artifact(manifest, role);
        if run.log_path != log.path || run.log_sha256 != log.sha256 || run.raw_sha256 != raw.sha256
        {
            return Err("run record is not bound to its log and raw artifact".into());
        }
        let text = read_text(&resolve(root, &log.path)?)?;
        validate_run_log(
            manifest,
            &text,
            &run.output_directory_token,
            &run.raw_sha256,
            "Nand",
        )?;
    }
    if manifest.runs[0].log_path == manifest.runs[1].log_path
        || manifest.runs[0].output_directory_token == manifest.runs[1].output_directory_token
    {
        return Err("repeat runs are not distinct".into());
    }
    let semantic_raw = artifact(manifest, "semantic_control_raw_csv");
    let semantic = read_text(&resolve(
        root,
        &artifact(manifest, "semantic_control_log").path,
    )?)?;
    validate_run_log(
        manifest,
        &semantic,
        "fresh-semantic-control",
        &semantic_raw.sha256,
        "And",
    )?;
    if raw.sha256 == semantic_raw.sha256 {
        return Err("And semantic control did not mutate raw output".into());
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
    macro_rules! required {
        ($key:literal, $value:expr) => {
            require_log_value(text, $key, $value)?
        };
    }

    let buildings = &manifest.sources[0];
    let modelica = &manifest.sources[1];
    required!("host_architecture", &manifest.image.host_architecture);
    required!(
        "docker_server_architecture",
        &manifest.image.docker_server_architecture
    );
    required!("image_index_digest", &manifest.image.index_digest);
    required!(
        "image_platform_manifest_digest",
        &manifest.image.platform_manifest_digest
    );
    required!("image_config_digest", &manifest.image.config_digest);
    required!("oci_metadata_validation", "raw_digest_and_pinned_graph");
    required!("pull_policy", &manifest.regeneration.pull);
    required!(
        "host_timeout_seconds",
        &manifest.regeneration.timeout_seconds.to_string()
    );
    required!("buildings_remote", &buildings.repository);
    required!("buildings_commit", &buildings.commit);
    required!("buildings_tree", &buildings.tree);
    required!("modelica_remote", &modelica.repository);
    required!("modelica_commit", &modelica.commit);
    required!("modelica_tree", &modelica.tree);
    required!(
        "source_materialization",
        "git_archive_exact_committed_bytes"
    );
    required!(
        "buildings_package_sha256",
        source_digest(buildings, "Buildings/package.mo")?
    );
    required!(
        "nand_source_sha256",
        source_digest(buildings, "Buildings/Controls/OBC/CDL/Logical/Nand.mo")?
    );
    required!(
        "and_source_sha256",
        source_digest(buildings, "Buildings/Controls/OBC/CDL/Logical/And.mo")?
    );
    required!(
        "modelica_package_sha256",
        source_digest(modelica, "Modelica/package.mo")?
    );
    required!(
        "boolean_table_source_sha256",
        source_digest(modelica, "Modelica/Blocks/Sources.mo")?
    );
    required!(
        "modelica_services_sha256",
        source_digest(modelica, "ModelicaServices/package.mo")?
    );
    required!("complex_sha256", source_digest(modelica, "Complex.mo")?);
    required!("docker_command", DOCKER_COMMAND);
    required!("output_directory_token", output_token);
    required!("selected_model", model);
    required!("container_architecture", "aarch64");
    validate_non_root_identity(text)?;
    required!("modelica_path", "");
    required!("root_write_probe", "read-only");
    required!("source_write_probe", "read-only");
    required!("network_route_lines", "1");
    required!(
        "cgroup_memory_max",
        &manifest.regeneration.memory_bytes.to_string()
    );
    required!(
        "cgroup_pids_max",
        &manifest.regeneration.pids_limit.to_string()
    );
    required!("cgroup_cpu_max", "400000 100000");
    required!(
        "per_file_limit_bytes",
        &manifest.regeneration.per_file_bytes.to_string()
    );
    required!(
        "output_directory_limit_bytes",
        &manifest.regeneration.output_directory_bytes.to_string()
    );
    validate_output_size(text, manifest.regeneration.output_directory_bytes)?;
    required!("gcc_version", &manifest.image.gcc_version);
    required!("binutils_version", &manifest.image.binutils_version);
    required!("glibc_version", &manifest.image.glibc_version);
    required!(
        "nand_source",
        "/sources/buildings/Buildings/Controls/OBC/CDL/Logical/Nand.mo"
    );
    required!(
        "boolean_table_source",
        "/sources/modelica/Modelica/Blocks/Sources.mo"
    );
    required!("Modelica", &modelica.version);
    required!("Buildings", &buildings.version);
    required!("omc_version", &manifest.image.omc_version);
    required!("raw_sha256", raw_sha256);
    required!("runner_complete", "1");
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

fn source_digest<'a>(source: &'a super::schema::Source, path: &str) -> Result<&'a str, String> {
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
    let prefix = "container_identity=";
    let values = text
        .lines()
        .filter_map(|line| line.strip_prefix(prefix))
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
        return Err("container identity is root".into());
    }
    Ok(())
}

fn validate_output_size(text: &str, limit_bytes: u64) -> Result<(), String> {
    let prefix = "output_directory_kib=";
    let values = text
        .lines()
        .filter_map(|line| line.strip_prefix(prefix))
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
        return Err("output-directory size exceeds its recorded limit".into());
    }
    Ok(())
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
    let nand = read_text(&resolve(root, &artifact(manifest, "wrapper_model").path)?)?;
    let and = read_text(&resolve(
        root,
        &artifact(manifest, "semantic_control_wrapper_model").path,
    )?)?;
    validate_wrapper_pair(&nand, &and)
}

fn validate_wrapper_pair(nand: &str, and: &str) -> Result<(), String> {
    if nand != wrapper_text("Nand") || and != wrapper_text("And") {
        return Err("wrapper does not match the closed revision-1 source".into());
    }
    let nand_token = "Buildings.Controls.OBC.CDL.Logical.Nand";
    if nand.matches(nand_token).count() != 1
        || nand.replace(nand_token, "Buildings.Controls.OBC.CDL.Logical.And") != and
    {
        return Err("semantic wrapper is not the one-token And substitution".into());
    }
    for forbidden in ["expected", "assert(", "FMU", "FMI", "oce-"] {
        if nand.contains(forbidden) || and.contains(forbidden) {
            return Err(format!("wrapper contains forbidden token {forbidden}"));
        }
    }
    Ok(())
}

fn wrapper_text(class: &str) -> String {
    format!(
        "model NandPilot\n  Modelica.Blocks.Sources.BooleanTable u1Source(\n    table={{120}},\n    startValue=false);\n  Modelica.Blocks.Sources.BooleanTable u2Source(\n    table={{60,120,180}},\n    startValue=false);\n  Buildings.Controls.OBC.CDL.Logical.{class} dut;\n\n  output Boolean u1;\n  output Boolean u2;\n  output Boolean y;\nequation\n  connect(u1Source.y, dut.u1);\n  connect(u2Source.y, dut.u2);\n  u1 = u1Source.y;\n  u2 = u2Source.y;\n  y = dut.y;\nend NandPilot;\n"
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
    let index: Index = serde_json::from_slice(&read_bytes(&resolve(root, &index_artifact.path)?)?)
        .map_err(|error| error.to_string())?;
    let arm = index
        .manifests
        .iter()
        .filter(|descriptor| {
            descriptor
                .platform
                .as_ref()
                .is_some_and(|platform| platform.os == "linux" && platform.architecture == "arm64")
        })
        .collect::<Vec<_>>();
    if arm.len() != 1 || arm[0].digest != manifest.image.platform_manifest_digest {
        return Err("OCI index does not select the pinned linux/arm64 manifest".into());
    }
    let platform: PlatformManifest =
        serde_json::from_slice(&read_bytes(&resolve(root, &platform_artifact.path)?)?)
            .map_err(|error| error.to_string())?;
    if platform.config.digest != manifest.image.config_digest {
        return Err("OCI manifest does not select the pinned config".into());
    }
    Ok(())
}

fn validate_projection_mutation(manifest: &Manifest, root: &Path) -> Result<(), String> {
    let log = read_text(&resolve(
        root,
        &artifact(manifest, "projection_mutation_log").path,
    )?)?;
    let canonicalizer = artifact(manifest, "canonicalizer_source");
    for required in [
        "projection_mutation=contiguous equal-time selection changed from last to first",
        "unmutated_result=PASS",
        "mutated_compile=PASS",
        "mutated_schedule_result=FAIL",
        "mutated_exact_control_result=PASS",
        "restoration_result=PASS",
        &format!("restored_canonicalizer_sha256={}", canonicalizer.sha256),
    ] {
        if !log.contains(required) {
            return Err(format!("projection-mutation evidence omits {required}"));
        }
    }
    Ok(())
}

fn artifact<'a>(manifest: &'a Manifest, role: &str) -> &'a Artifact {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.role == role)
        .unwrap_or_else(|| panic!("validated manifest omits role {role}"))
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file = open_bounded(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_ARTIFACT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_ARTIFACT_BYTES {
        return Err(artifact_size_error(path));
    }
    Ok(bytes)
}

fn open_bounded(path: &Path) -> Result<std::fs::File, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > MAX_ARTIFACT_BYTES {
        return Err(artifact_size_error(path));
    }
    Ok(file)
}

fn artifact_size_error(path: &Path) -> String {
    format!(
        "artifact exceeds {MAX_ARTIFACT_BYTES} bytes: {}",
        path.display()
    )
}

fn read_text(path: &Path) -> Result<String, String> {
    String::from_utf8(read_bytes(path)?).map_err(|error| error.to_string())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let file = open_bounded(path)?;
    let mut file = file.take(MAX_ARTIFACT_BYTES + 1);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_ARTIFACT_BYTES {
            return Err(artifact_size_error(path));
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex(&digest.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_paths_reject_ambiguous_and_dangerous_spellings() {
        for path in [
            "", "/tmp/x", "a//b", "a/./b", "a/../b", "a\\b", "a:b", "a*", "a?", "a. ", "a.",
        ] {
            assert!(valid_path(path).is_err(), "{path:?}");
        }
        assert!(valid_path("crates/example/file.csv").is_ok());
        let at_limit = format!("a/{}", "b".repeat(510));
        assert_eq!(at_limit.len(), 512);
        assert!(valid_path(&at_limit).is_ok());
        assert!(valid_path(&format!("{at_limit}b")).is_err());
    }

    #[test]
    fn final_and_escaping_intermediate_symlinks_are_rejected() {
        use std::os::unix::fs::symlink;

        let base =
            std::env::temp_dir().join(format!("oce-openmodelica-path-{}", std::process::id()));
        let outside = base.with_extension("outside");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(base.join("inside")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(base.join("inside/file"), b"ok").unwrap();
        std::fs::write(outside.join("file"), b"outside").unwrap();
        symlink(base.join("inside/file"), base.join("final-link")).unwrap();
        symlink(&outside, base.join("escape")).unwrap();
        let root = base.canonicalize().unwrap();
        assert!(resolve(&root, "inside/file").is_ok());
        assert!(
            resolve(&root, "final-link")
                .unwrap_err()
                .contains("non-symlink")
        );
        assert!(
            resolve(&root, "escape/file")
                .unwrap_err()
                .contains("outside repository")
        );
        std::fs::remove_dir_all(&base).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn run_log_values_are_exact_unique_and_non_root() {
        let valid = "key=value\ncontainer_identity=501:20\n";
        assert!(require_log_value(valid, "key", "value").is_ok());
        assert!(validate_non_root_identity(valid).is_ok());
        assert!(require_log_value("key=value\nkey=value\n", "key", "value").is_err());
        assert!(require_log_value("prefix-key=value\n", "key", "value").is_err());
        assert!(validate_non_root_identity("container_identity=0:20\n").is_err());
        assert!(validate_non_root_identity("container_identity=501:x\n").is_err());
        assert!(validate_output_size("output_directory_kib=4\n", 4096).is_ok());
        assert!(validate_output_size("output_directory_kib=5\n", 4096).is_err());
    }

    #[test]
    fn artifact_digest_accepts_limit_and_rejects_limit_plus_one() {
        let path = std::env::temp_dir().join(format!(
            "oce-openmodelica-artifact-size-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_ARTIFACT_BYTES).unwrap();
        assert!(sha256_file(&path).is_ok());
        file.set_len(MAX_ARTIFACT_BYTES + 1).unwrap();
        assert!(sha256_file(&path).unwrap_err().contains("artifact exceeds"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn wrapper_rejects_an_embedded_boolean_implementation() {
        let nand =
            wrapper_text("Nand").replace("y = dut.y;", "y = if u1 and u2 then dut.y else true;");
        let and =
            wrapper_text("And").replace("y = dut.y;", "y = if u1 and u2 then dut.y else true;");
        assert!(validate_wrapper_pair(&nand, &and).is_err());
    }
}
