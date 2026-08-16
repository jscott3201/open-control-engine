#!/usr/bin/env python3
"""Record one native Line generation without trusting caller-supplied metadata."""

import hashlib
import json
import os
import pathlib
import subprocess
import sys

import safe_files

MAX_FILE = 1024 * 1024


def read_bounded(path):
    return safe_files.read_bounded(path, MAX_FILE)


def sha(path):
    return hashlib.sha256(read_bounded(path)).hexdigest()


def one_value(path, key):
    prefix = (key + "=").encode()
    values = [line[len(prefix):].decode() for line in read_bounded(path).splitlines() if line.startswith(prefix)]
    if len(values) != 1:
        raise ValueError(f"log must contain exactly one {key}")
    return values[0]


if len(sys.argv) != 4:
    raise SystemExit("usage: generate_architecture.py OUTPUT ARCHITECTURE REPOSITORY_ROOT")
output = pathlib.Path(sys.argv[1])
architecture = sys.argv[2]
root = pathlib.Path(sys.argv[3])
facts = {
    "arm64": {
        "platform": "linux/arm64",
        "host": "arm64",
        "server": "aarch64",
        "container": "aarch64",
        "manifest": "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4",
        "config": "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666",
    },
    "amd64": {
        "platform": "linux/amd64",
        "host": "amd64",
        "server": "x86_64",
        "container": "x86_64",
        "manifest": "sha256:92d0779a01e7d43ed4d5ecb4cfd9754cb259b30673ddb454b5a32e3eb8665f11",
        "config": "sha256:0c81120bb392de44cab0e9ff6818d0a44afad657d5b401f25e148fa6c26e5347",
    },
}[architecture]
logs = [output / name for name in ["run-a.log", "run-b.log", "flag-control.log"]]
run_a = logs[0]
for key, expected in [("host_architecture", facts["host"]), ("docker_server_architecture", facts["server"]), ("container_architecture", facts["container"])]:
    if one_value(run_a, key) != expected:
        raise ValueError(f"observed {key} does not match native architecture")
revision = subprocess.run(
    ["git", "-C", root, "rev-parse", "HEAD"],
    check=True,
    capture_output=True,
    text=True,
).stdout.strip()
if subprocess.run(
    ["git", "-C", root, "status", "--porcelain", "--untracked-files=no"],
    check=True,
    capture_output=True,
    text=True,
).stdout:
    raise ValueError("repository tracked files changed during native generation")
inputs = {
    "line_pilot_sha256": root / "tools/openmodelica-line-reference/line/LinePilot.mo",
    "line_flag_pilot_sha256": root / "tools/openmodelica-line-reference/line/LineFlagPilot.mo",
    "runner_sha256": root / "tools/openmodelica-line-reference/line/runner.sh",
    "regenerate_sha256": root / "tools/openmodelica-line-reference/line/regenerate.sh",
    "canonicalizer_sha256": root / "crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs",
    "tool_main_sha256": root / "tools/openmodelica-line-reference/src/main.rs",
    "tool_cargo_toml_sha256": root / "tools/openmodelica-line-reference/Cargo.toml",
    "tool_cargo_lock_sha256": root / "tools/openmodelica-line-reference/Cargo.lock",
    "architecture_generator_sha256": root / "tools/openmodelica-line-reference/line/generate_architecture.py",
    "architecture_verifier_sha256": root / "tools/openmodelica-line-reference/line/verify_evidence.py",
    "safe_file_helper_sha256": root / "tools/openmodelica-line-reference/line/safe_files.py",
    "evidence_workflow_sha256": root / ".github/workflows/openmodelica-line-evidence.yml",
    "oci_materializer_sha256": root / "tools/openmodelica-line-reference/line/materialize_oci.py",
    "deadline_sha256": root / "tools/openmodelica-line-reference/line/deadline.sh",
    "deadline_test_sha256": root / "tools/openmodelica-line-reference/line/deadline_test.sh",
    "container_cleanup_sha256": root / "tools/openmodelica-line-reference/line/container_cleanup.sh",
    "container_cleanup_test_sha256": root / "tools/openmodelica-line-reference/line/container_cleanup_test.sh",
    "output_publish_sha256": root / "tools/openmodelica-line-reference/line/output_publish.py",
    "output_publish_test_sha256": root / "tools/openmodelica-line-reference/line/output_publish_test.sh",
    "oci_index_source_sha256": root / "tools/openmodelica-line-reference/line/image-index.json",
    "arm64_manifest_source_sha256": root / "tools/openmodelica-line-reference/line/image-manifest-arm64.json",
    "amd64_manifest_source_sha256": root / "tools/openmodelica-line-reference/line/image-manifest-amd64.json",
}
recorded_inputs = {key: sha(path) for key, path in inputs.items()}
artifact_toolchain = {
    key: one_value(run_a, key)
    for key in [
        "rustc_release", "rustc_commit_hash", "rustc_commit_date", "rustc_host",
        "rustc_llvm_version", "cargo_release", "cargo_commit_hash", "cargo_commit_date",
        "cargo_host", "python_version",
    ]
}
source_materialization = {
    key: one_value(run_a, key)
    for key in [
        "source_materialization", "buildings_materialization", "modelica_transform_path",
        "modelica_transform_rule", "buildings_package_committed_sha256",
        "buildings_package_materialized_sha256", "line_source_committed_sha256",
        "line_source_materialized_sha256", "modelica_package_committed_sha256",
        "modelica_package_materialized_sha256",
    ]
}
expected_host = "aarch64-unknown-linux-gnu" if architecture == "arm64" else "x86_64-unknown-linux-gnu"
expected_toolchain = {
    "rustc_release": "1.97.1",
    "rustc_commit_hash": "8bab26f4f68e0e26f0bb7960be334d5b520ea452",
    "rustc_commit_date": "2026-07-14",
    "rustc_host": expected_host,
    "rustc_llvm_version": "22.1.6",
    "cargo_release": "1.97.1",
    "cargo_commit_hash": "c980f4866141969fab6254a680546a277789d6f0",
    "cargo_commit_date": "2026-06-30",
    "cargo_host": expected_host,
    "python_version": "Python 3.13.7",
}
if artifact_toolchain != expected_toolchain:
    raise ValueError("native artifact toolchain does not match the pinned workflow")
expected_materialization = {
    "source_materialization": "git_archive_with_pinned_modelica_export_subst",
    "buildings_materialization": "git_archive_without_local_attribute_override",
    "modelica_transform_path": "Modelica/package.mo",
    "modelica_transform_rule": "Modelica/package.mo -export-subst",
    "buildings_package_committed_sha256": "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59",
    "buildings_package_materialized_sha256": "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59",
    "line_source_committed_sha256": "85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5",
    "line_source_materialized_sha256": "85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5",
    "modelica_package_committed_sha256": "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
    "modelica_package_materialized_sha256": "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
}
if source_materialization != expected_materialization:
    raise ValueError("native source materialization record is unsupported")
container_toolchain = {
    "omc_version": "OpenModelica 1.25.1",
    "gcc_version": "11.4.0",
    "binutils_version": "2.38",
    "glibc_version": "2.35",
}
for log in logs:
    if one_value(log, "repository_revision") != revision:
        raise ValueError("native log source revision does not match the executing repository")
    if one_value(log, "generator_provenance_scope") != "native_generation_and_publication":
        raise ValueError("native log has an unsupported generator provenance scope")
    for key, expected in recorded_inputs.items():
        if one_value(log, key) != expected:
            raise ValueError(f"native log {key} does not match the executing repository")
    for key, expected in artifact_toolchain.items():
        if one_value(log, key) != expected:
            raise ValueError(f"native log {key} changed across runs")
    for key, expected in source_materialization.items():
        if one_value(log, key) != expected:
            raise ValueError(f"native log {key} changed across runs")
    for key, expected in container_toolchain.items():
        if one_value(log, key) != expected:
            raise ValueError(f"native log {key} changed across runs")
payload = {
    "format": "oce-openmodelica-line-native-architecture-v4",
    "architecture": architecture,
    "platform": facts["platform"],
    "host_architecture": facts["host"],
    "docker_server_architecture": facts["server"],
    "container_architecture": facts["container"],
    "platform_manifest_digest": facts["manifest"],
    "config_digest": facts["config"],
    "repository_revision": revision,
    "generator_provenance_scope": "native_generation_and_publication",
    "generator_inputs": recorded_inputs,
    "artifact_toolchain": artifact_toolchain,
    "source_materialization": source_materialization,
    **container_toolchain,
    "raw_run_a_sha256": sha(output / "line-run-a.raw.csv"),
    "raw_run_b_sha256": sha(output / "line-run-b.raw.csv"),
    "flag_control_raw_sha256": sha(output / "flag-control.raw.csv"),
    "canonical_sha256": sha(output / "line.canonical.csv"),
    "flag_control_canonical_sha256": sha(output / "flag-control.canonical.csv"),
}
encoded = (json.dumps(payload, indent=2) + "\n").encode()
descriptor = os.open(output / "architecture.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
try:
    os.write(descriptor, encoded)
finally:
    os.close(descriptor)
