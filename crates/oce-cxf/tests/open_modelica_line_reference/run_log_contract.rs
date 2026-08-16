//! Artifact-toolchain and source-materialization fields required in every native run log.

use super::schema::Architecture;

pub(super) fn validate(text: &str, architecture: &Architecture) -> Result<(), String> {
    let toolchain = &architecture.artifact_toolchain;
    let source = &architecture.source_materialization;
    for (key, expected) in [
        ("rustc_release", toolchain.rustc_release.as_str()),
        ("rustc_commit_hash", toolchain.rustc_commit_hash.as_str()),
        ("rustc_commit_date", toolchain.rustc_commit_date.as_str()),
        ("rustc_host", toolchain.rustc_host.as_str()),
        ("rustc_llvm_version", toolchain.rustc_llvm_version.as_str()),
        ("cargo_release", toolchain.cargo_release.as_str()),
        ("cargo_commit_hash", toolchain.cargo_commit_hash.as_str()),
        ("cargo_commit_date", toolchain.cargo_commit_date.as_str()),
        ("cargo_host", toolchain.cargo_host.as_str()),
        ("python_version", toolchain.python_version.as_str()),
        (
            "source_materialization",
            source.source_materialization.as_str(),
        ),
        (
            "buildings_materialization",
            source.buildings_materialization.as_str(),
        ),
        (
            "modelica_transform_path",
            source.modelica_transform_path.as_str(),
        ),
        (
            "modelica_transform_rule",
            source.modelica_transform_rule.as_str(),
        ),
        (
            "buildings_package_committed_sha256",
            source.buildings_package_committed_sha256.as_str(),
        ),
        (
            "buildings_package_materialized_sha256",
            source.buildings_package_materialized_sha256.as_str(),
        ),
        (
            "line_source_committed_sha256",
            source.line_source_committed_sha256.as_str(),
        ),
        (
            "line_source_materialized_sha256",
            source.line_source_materialized_sha256.as_str(),
        ),
        (
            "modelica_package_committed_sha256",
            source.modelica_package_committed_sha256.as_str(),
        ),
        (
            "modelica_package_materialized_sha256",
            source.modelica_package_materialized_sha256.as_str(),
        ),
        (
            "sources_source_committed_sha256",
            "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3",
        ),
        (
            "sources_source_materialized_sha256",
            "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3",
        ),
        (
            "modelica_services_committed_sha256",
            "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb",
        ),
        (
            "modelica_services_materialized_sha256",
            "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb",
        ),
        (
            "complex_committed_sha256",
            "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f",
        ),
        (
            "complex_materialized_sha256",
            "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f",
        ),
    ] {
        require_log_value(text, key, expected)?;
    }
    Ok(())
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
        Err(format!("run log has unsupported or repeated {key}"))
    }
}
