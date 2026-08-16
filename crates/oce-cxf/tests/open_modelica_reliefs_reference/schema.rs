//! Closed JSON schema for the retained Reliefs evidence graph.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationRevisionContract {
    pub(super) format: String,
    pub(super) revision: String,
    pub(super) relationship: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub(super) format: String,
    pub(super) scope: Scope,
    pub(super) image: Image,
    pub(super) sources: Vec<Source>,
    pub(super) simulation: Simulation,
    pub(super) projection: Projection,
    pub(super) expected_output_bits: OutputBits,
    pub(super) architectures: Vec<Architecture>,
    pub(super) controls: Controls,
    pub(super) cross_architecture: CrossArchitecture,
    pub(super) artifacts: Vec<Artifact>,
    pub(super) regeneration: Regeneration,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Scope {
    pub(super) class: String,
    pub(super) scenario: String,
    pub(super) parameters: Parameters,
    pub(super) inputs: Vec<String>,
    pub(super) outputs: Vec<String>,
    pub(super) comparison: String,
    pub(super) global_tier3_status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    #[serde(rename = "uMin")]
    pub(super) u_min: String,
    #[serde(rename = "uMax")]
    pub(super) u_max: String,
    #[serde(rename = "uOutDamMax")]
    pub(super) u_out_dam_max: String,
    #[serde(rename = "uRetDamMin")]
    pub(super) u_ret_dam_min: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Image {
    pub(super) repository: String,
    pub(super) tag: String,
    pub(super) index_digest: String,
    pub(super) platforms: Vec<ImagePlatform>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ImagePlatform {
    pub(super) platform: String,
    pub(super) manifest_digest: String,
    pub(super) config_digest: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Source {
    pub(super) name: String,
    pub(super) repository: String,
    pub(super) commit: String,
    pub(super) tree: String,
    pub(super) package: String,
    pub(super) version: String,
    pub(super) materialization: String,
    pub(super) transforms: Vec<SourceTransform>,
    pub(super) files: Vec<SourceFile>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceTransform {
    pub(super) path: String,
    pub(super) rule: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceFile {
    pub(super) path: String,
    pub(super) committed_sha256: String,
    pub(super) materialized_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Simulation {
    pub(super) method: String,
    pub(super) start_time: String,
    pub(super) stop_time: String,
    pub(super) number_of_intervals: u64,
    pub(super) tolerance: String,
    pub(super) output_format: String,
    pub(super) variable_filter: String,
    pub(super) simflags: String,
    pub(super) event_emission: bool,
    pub(super) raw_header: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Projection {
    pub(super) columns: Vec<String>,
    pub(super) grouping: String,
    pub(super) group_selection: String,
    pub(super) tuple_selection: String,
    pub(super) normalize_times: bool,
    pub(super) raw_rows: u64,
    pub(super) grouped_rows: u64,
    pub(super) canonical_rows: u64,
    pub(super) group_sizes: Vec<u64>,
    pub(super) raw_time_bits: Vec<String>,
    pub(super) selected_source_rows: Vec<u64>,
    pub(super) canonical_time_bits: Vec<String>,
    pub(super) canonical_input_bits: InputBits,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct InputBits {
    #[serde(rename = "uTSup")]
    pub(super) u_t_sup: Vec<String>,
    #[serde(rename = "uOutDam_min")]
    pub(super) u_out_dam_min: Vec<String>,
    #[serde(rename = "uOutDam_max")]
    pub(super) u_out_dam_max: Vec<String>,
    #[serde(rename = "uRetDam_min")]
    pub(super) u_ret_dam_min: Vec<String>,
    #[serde(rename = "uRetDam_max")]
    pub(super) u_ret_dam_max: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct OutputBits {
    #[serde(rename = "yOutDam")]
    pub(super) y_out_dam: Vec<String>,
    #[serde(rename = "yRetDam")]
    pub(super) y_ret_dam: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Architecture {
    pub(super) name: String,
    pub(super) platform: String,
    pub(super) host_architecture: String,
    pub(super) docker_server_architecture: String,
    pub(super) container_architecture: String,
    pub(super) platform_manifest_digest: String,
    pub(super) config_digest: String,
    pub(super) repository_revision: String,
    pub(super) generator_provenance_scope: String,
    pub(super) generator_inputs: GeneratorInputs,
    pub(super) artifact_toolchain: ArtifactToolchain,
    pub(super) source_materialization: SourceMaterialization,
    pub(super) source_files: Vec<NativeSourceFile>,
    pub(super) omc_version: String,
    pub(super) gcc_version: String,
    pub(super) binutils_version: String,
    pub(super) glibc_version: String,
    pub(super) raw_run_a_sha256: String,
    pub(super) raw_run_b_sha256: String,
    pub(super) canonical_sha256: String,
    pub(super) parameter_control_raw_sha256: String,
    pub(super) parameter_control_canonical_sha256: String,
    pub(super) final_clamp_raw_sha256: String,
    pub(super) final_clamp_canonical_sha256: String,
    pub(super) projection_mutation: ProjectionMutation,
    pub(super) runs: Vec<Run>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct GeneratorInputs {
    pub(super) reliefs_pilot_sha256: String,
    pub(super) parameter_pilot_sha256: String,
    pub(super) clamp_pilot_sha256: String,
    pub(super) runner_sha256: String,
    pub(super) regenerate_sha256: String,
    pub(super) canonicalizer_sha256: String,
    pub(super) tool_main_sha256: String,
    pub(super) tool_cargo_toml_sha256: String,
    pub(super) tool_cargo_lock_sha256: String,
    pub(super) architecture_generator_sha256: String,
    pub(super) architecture_verifier_sha256: String,
    pub(super) projection_verifier_sha256: String,
    pub(super) safe_file_helper_sha256: String,
    pub(super) evidence_workflow_sha256: String,
    pub(super) oci_materializer_sha256: String,
    pub(super) deadline_sha256: String,
    pub(super) deadline_test_sha256: String,
    pub(super) container_cleanup_sha256: String,
    pub(super) container_cleanup_test_sha256: String,
    pub(super) output_publish_sha256: String,
    pub(super) output_publish_test_sha256: String,
    pub(super) oci_index_source_sha256: String,
    pub(super) arm64_manifest_source_sha256: String,
    pub(super) amd64_manifest_source_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactToolchain {
    pub(super) rustc_release: String,
    pub(super) rustc_commit_hash: String,
    pub(super) rustc_commit_date: String,
    pub(super) rustc_host: String,
    pub(super) rustc_llvm_version: String,
    pub(super) cargo_release: String,
    pub(super) cargo_commit_hash: String,
    pub(super) cargo_commit_date: String,
    pub(super) cargo_host: String,
    pub(super) python_version: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceMaterialization {
    pub(super) source_materialization: String,
    pub(super) buildings_materialization: String,
    pub(super) modelica_transform_path: String,
    pub(super) modelica_transform_rule: String,
    pub(super) modelica_package_committed_sha256: String,
    pub(super) modelica_package_materialized_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeSourceFile {
    pub(super) source: String,
    pub(super) path: String,
    pub(super) committed_sha256: String,
    pub(super) materialized_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectionMutation {
    pub(super) control: String,
    pub(super) input: String,
    pub(super) input_sha256: String,
    pub(super) canonical_output: String,
    pub(super) canonical_sha256: String,
    pub(super) metadata: String,
    pub(super) metadata_sha256: String,
    pub(super) log: String,
    pub(super) log_sha256: String,
    pub(super) selected_source_rows: Vec<u64>,
    pub(super) selected_time_bits: Vec<String>,
    pub(super) expected_source_rows: Vec<u64>,
    pub(super) expected_time_bits: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Run {
    pub(super) id: String,
    pub(super) output_directory_token: String,
    pub(super) log_sha256: String,
    pub(super) raw_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Controls {
    pub(super) parameter: ParameterControl,
    pub(super) mapping: MappingControl,
    pub(super) declared_path: DeclaredPathControl,
    pub(super) final_clamp: FinalClampControl,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ParameterControl {
    pub(super) mutation: String,
    pub(super) output: String,
    pub(super) first_mismatch_row: u64,
    pub(super) first_mismatch_time_bits: String,
    pub(super) expected_bits: String,
    pub(super) observed_bits: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct MappingControl {
    pub(super) mutation: String,
    pub(super) expected_comparison: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct DeclaredPathControl {
    pub(super) mutation: String,
    pub(super) expected_error: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct FinalClampControl {
    pub(super) inputs: FinalClampInputs,
    pub(super) rows: u64,
    #[serde(rename = "yOutDam")]
    pub(super) y_out_dam: String,
    #[serde(rename = "yRetDam")]
    pub(super) y_ret_dam: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct FinalClampInputs {
    #[serde(rename = "uOutDam_min")]
    pub(super) u_out_dam_min: String,
    #[serde(rename = "uOutDam_max")]
    pub(super) u_out_dam_max: String,
    #[serde(rename = "uRetDam_min")]
    pub(super) u_ret_dam_min: String,
    #[serde(rename = "uRetDam_max")]
    pub(super) u_ret_dam_max: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct CrossArchitecture {
    pub(super) comparison: String,
    pub(super) arm64_sha256: String,
    pub(super) amd64_sha256: String,
    pub(super) result: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    pub(super) role: String,
    pub(super) path: String,
    pub(super) sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Regeneration {
    pub(super) entrypoint: String,
    pub(super) assembly_entrypoint: String,
    pub(super) evidence_workflow: String,
    pub(super) network: String,
    pub(super) pull: String,
    pub(super) platforms: Vec<String>,
    pub(super) source_materialization: String,
    pub(super) source_mounts: String,
    pub(super) container_root: String,
    pub(super) container_user: String,
    pub(super) capabilities: String,
    pub(super) no_new_privileges: bool,
    pub(super) device_mounts: u64,
    pub(super) docker_socket_mounted: bool,
    pub(super) timeout_seconds: u64,
    pub(super) cpus: String,
    pub(super) memory_bytes: u64,
    pub(super) memory_swap_bytes: u64,
    pub(super) pids_limit: u64,
    pub(super) tmpfs_bytes: u64,
    pub(super) per_file_bytes: u64,
    pub(super) output_directory_bytes: u64,
    pub(super) memory_measurement: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeArchitectureRecord {
    pub(super) format: String,
    pub(super) architecture: String,
    pub(super) platform: String,
    pub(super) host_architecture: String,
    pub(super) docker_server_architecture: String,
    pub(super) container_architecture: String,
    pub(super) platform_manifest_digest: String,
    pub(super) config_digest: String,
    pub(super) repository_revision: String,
    pub(super) generator_provenance_scope: String,
    pub(super) generator_inputs: GeneratorInputs,
    pub(super) artifact_toolchain: ArtifactToolchain,
    pub(super) source_materialization: SourceMaterialization,
    pub(super) source_files: Vec<NativeSourceFile>,
    pub(super) omc_version: String,
    pub(super) gcc_version: String,
    pub(super) binutils_version: String,
    pub(super) glibc_version: String,
    pub(super) raw_run_a_sha256: String,
    pub(super) raw_run_b_sha256: String,
    pub(super) canonical_sha256: String,
    pub(super) parameter_control_raw_sha256: String,
    pub(super) parameter_control_canonical_sha256: String,
    pub(super) final_clamp_raw_sha256: String,
    pub(super) final_clamp_canonical_sha256: String,
    pub(super) projection_mutation: ProjectionMutation,
    pub(super) runs: Vec<Run>,
}
