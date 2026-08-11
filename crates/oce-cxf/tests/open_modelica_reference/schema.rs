//! Closed revision-1 OpenModelica evidence schema.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub(super) format: String,
    pub(super) scope: Scope,
    pub(super) image: Image,
    pub(super) sources: Vec<Source>,
    pub(super) simulation: Simulation,
    pub(super) projection: Projection,
    pub(super) runs: Vec<Run>,
    pub(super) semantic_control: SemanticControl,
    pub(super) artifacts: Vec<Artifact>,
    pub(super) regeneration: Regeneration,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Scope {
    pub(super) class: String,
    pub(super) scenario: String,
    pub(super) inputs: Vec<String>,
    pub(super) output: String,
    pub(super) comparison: String,
    pub(super) global_tier3_status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Image {
    pub(super) repository: String,
    pub(super) tag: String,
    pub(super) index_digest: String,
    pub(super) platform_manifest_digest: String,
    pub(super) config_digest: String,
    pub(super) platform: String,
    pub(super) host_architecture: String,
    pub(super) docker_server_architecture: String,
    pub(super) omc_version: String,
    pub(super) gcc_version: String,
    pub(super) binutils_version: String,
    pub(super) glibc_version: String,
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
    pub(super) files: Vec<SourceFile>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceFile {
    pub(super) path: String,
    pub(super) sha256: String,
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
    pub(super) event_emission: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Projection {
    pub(super) columns: Vec<String>,
    pub(super) grouping: String,
    pub(super) selection: String,
    pub(super) normalize_times: bool,
    pub(super) raw_rows: u64,
    pub(super) canonical_rows: u64,
    pub(super) group_sizes: Vec<u64>,
    pub(super) canonical_time_bits: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Run {
    pub(super) id: String,
    pub(super) output_directory_token: String,
    pub(super) log_path: String,
    pub(super) log_sha256: String,
    pub(super) raw_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct SemanticControl {
    pub(super) substitution_class: String,
    pub(super) raw_rows: u64,
    pub(super) canonical_rows: u64,
    pub(super) expected_comparison: String,
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
    pub(super) network: String,
    pub(super) pull: String,
    pub(super) platform: String,
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
}
