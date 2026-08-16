//! Closed semantic binding between retained native records and the final manifest.

use std::path::Path;

use super::safe_read;
use super::schema::{Architecture, Manifest, NativeArchitectureRecord};

pub(super) fn validate(manifest: &Manifest, root: &Path) -> Result<(), String> {
    for architecture in &manifest.architectures {
        let role = format!("{}_architecture_record", architecture.name);
        let artifact = manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.role == role)
            .ok_or_else(|| format!("manifest omits {role}"))?;
        let bytes = safe_read::read(root, &artifact.path)?;
        let record: NativeArchitectureRecord =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        validate_one(&record, architecture)?;
    }
    Ok(())
}

fn validate_one(
    record: &NativeArchitectureRecord,
    architecture: &Architecture,
) -> Result<(), String> {
    if record.format != "oce-openmodelica-line-native-architecture-v4"
        || record.architecture != architecture.name
        || record.platform != architecture.platform
        || record.host_architecture != architecture.host_architecture
        || record.docker_server_architecture != architecture.docker_server_architecture
        || record.container_architecture != architecture.container_architecture
        || record.platform_manifest_digest != architecture.platform_manifest_digest
        || record.config_digest != architecture.config_digest
        || record.repository_revision != architecture.repository_revision
        || record.generator_provenance_scope != architecture.generator_provenance_scope
        || record.generator_inputs != architecture.generator_inputs
        || record.artifact_toolchain != architecture.artifact_toolchain
        || record.source_materialization != architecture.source_materialization
        || record.omc_version != architecture.omc_version
        || record.gcc_version != architecture.gcc_version
        || record.binutils_version != architecture.binutils_version
        || record.glibc_version != architecture.glibc_version
        || record.raw_run_a_sha256 != architecture.raw_run_a_sha256
        || record.raw_run_b_sha256 != architecture.raw_run_b_sha256
        || record.flag_control_raw_sha256 != architecture.flag_control_raw_sha256
        || record.canonical_sha256 != architecture.canonical_sha256
        || record.flag_control_canonical_sha256 != architecture.flag_control_canonical_sha256
        || record.runs != architecture.runs
    {
        return Err(format!(
            "{} native architecture record does not match final manifest semantics",
            architecture.name
        ));
    }
    Ok(())
}
