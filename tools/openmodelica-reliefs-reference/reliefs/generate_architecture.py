#!/usr/bin/env python3
"""Record one native Reliefs generation without trusting caller metadata."""

import hashlib
import json
import os
import pathlib
import subprocess
import sys

import projection_evidence
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
    "arm64": ("linux/arm64", "arm64", "aarch64", "aarch64", "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4", "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666"),
    "amd64": ("linux/amd64", "amd64", "x86_64", "x86_64", "sha256:92d0779a01e7d43ed4d5ecb4cfd9754cb259b30673ddb454b5a32e3eb8665f11", "sha256:0c81120bb392de44cab0e9ff6818d0a44afad657d5b401f25e148fa6c26e5347"),
}[architecture]
logs = [output / name for name in ["run-a.log", "run-b.log", "parameter-control.log", "final-clamp.log"]]
revision = subprocess.run(["git", "-C", root, "rev-parse", "HEAD"], check=True, capture_output=True, text=True).stdout.strip()
if subprocess.run(["git", "-C", root, "status", "--porcelain", "--untracked-files=no"], check=True, capture_output=True, text=True).stdout:
    raise ValueError("repository tracked files changed during native generation")
generator_paths = {
    "reliefs_pilot_sha256": "tools/openmodelica-reliefs-reference/reliefs/ReliefsPilot.mo",
    "parameter_pilot_sha256": "tools/openmodelica-reliefs-reference/reliefs/ReliefsParameterPilot.mo",
    "clamp_pilot_sha256": "tools/openmodelica-reliefs-reference/reliefs/ReliefsClampPilot.mo",
    "runner_sha256": "tools/openmodelica-reliefs-reference/reliefs/runner.sh",
    "regenerate_sha256": "tools/openmodelica-reliefs-reference/reliefs/regenerate.sh",
    "canonicalizer_sha256": "crates/oce-cxf/tests/open_modelica_reliefs_reference/canonicalizer.rs",
    "tool_main_sha256": "tools/openmodelica-reliefs-reference/src/main.rs",
    "tool_cargo_toml_sha256": "tools/openmodelica-reliefs-reference/Cargo.toml",
    "tool_cargo_lock_sha256": "tools/openmodelica-reliefs-reference/Cargo.lock",
    "architecture_generator_sha256": "tools/openmodelica-reliefs-reference/reliefs/generate_architecture.py",
    "architecture_verifier_sha256": "tools/openmodelica-reliefs-reference/reliefs/verify_evidence.py",
    "projection_verifier_sha256": "tools/openmodelica-reliefs-reference/reliefs/projection_evidence.py",
    "safe_file_helper_sha256": "tools/openmodelica-reliefs-reference/reliefs/safe_files.py",
    "evidence_workflow_sha256": ".github/workflows/openmodelica-reliefs-evidence.yml",
    "oci_materializer_sha256": "tools/openmodelica-reliefs-reference/reliefs/materialize_oci.py",
    "deadline_sha256": "tools/openmodelica-reliefs-reference/reliefs/deadline.sh",
    "deadline_test_sha256": "tools/openmodelica-reliefs-reference/reliefs/deadline_test.sh",
    "container_cleanup_sha256": "tools/openmodelica-reliefs-reference/reliefs/container_cleanup.sh",
    "container_cleanup_test_sha256": "tools/openmodelica-reliefs-reference/reliefs/container_cleanup_test.sh",
    "output_publish_sha256": "tools/openmodelica-reliefs-reference/reliefs/output_publish.py",
    "output_publish_test_sha256": "tools/openmodelica-reliefs-reference/reliefs/output_publish_test.sh",
    "oci_index_source_sha256": "tools/openmodelica-reliefs-reference/reliefs/image-index.json",
    "arm64_manifest_source_sha256": "tools/openmodelica-reliefs-reference/reliefs/image-manifest-arm64.json",
    "amd64_manifest_source_sha256": "tools/openmodelica-reliefs-reference/reliefs/image-manifest-amd64.json",
}
generator_inputs = {key: sha(root / path) for key, path in generator_paths.items()}
artifact_toolchain = {key: one_value(logs[0], key) for key in [
    "rustc_release", "rustc_commit_hash", "rustc_commit_date", "rustc_host", "rustc_llvm_version",
    "cargo_release", "cargo_commit_hash", "cargo_commit_date", "cargo_host", "python_version",
]}
expected_host = "aarch64-unknown-linux-gnu" if architecture == "arm64" else "x86_64-unknown-linux-gnu"
expected_toolchain = {
    "rustc_release": "1.97.1", "rustc_commit_hash": "8bab26f4f68e0e26f0bb7960be334d5b520ea452",
    "rustc_commit_date": "2026-07-14", "rustc_host": expected_host, "rustc_llvm_version": "22.1.6",
    "cargo_release": "1.97.1", "cargo_commit_hash": "c980f4866141969fab6254a680546a277789d6f0",
    "cargo_commit_date": "2026-06-30", "cargo_host": expected_host, "python_version": "Python 3.13.7",
}
if artifact_toolchain != expected_toolchain:
    raise ValueError("native artifact toolchain does not match the pinned workflow")
source_materialization = {key: one_value(logs[0], key) for key in [
    "source_materialization", "buildings_materialization", "modelica_transform_path",
    "modelica_transform_rule", "modelica_package_committed_sha256", "modelica_package_materialized_sha256",
]}
expected_materialization = {
    "source_materialization": "git_archive_with_pinned_modelica_export_subst",
    "buildings_materialization": "git_archive_without_local_attribute_override",
    "modelica_transform_path": "Modelica/package.mo",
    "modelica_transform_rule": "Modelica/package.mo -export-subst",
    "modelica_package_committed_sha256": "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
    "modelica_package_materialized_sha256": "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
}
if source_materialization != expected_materialization:
    raise ValueError("native source materialization record is unsupported")
source_files = json.loads(one_value(logs[0], "source_files_json"))
if not isinstance(source_files, list) or len(source_files) != 26:
    raise ValueError("native source cone is not complete")
container_toolchain = {"omc_version": "OpenModelica 1.25.1", "gcc_version": "11.4.0", "binutils_version": "2.38", "glibc_version": "2.35"}
for log in logs:
    if one_value(log, "repository_revision") != revision:
        raise ValueError("native log source revision does not match executing repository")
    if one_value(log, "generator_provenance_scope") != "native_generation_and_publication":
        raise ValueError("native log has an unsupported generator provenance scope")
    for key, expected in generator_inputs.items():
        if one_value(log, key) != expected:
            raise ValueError(f"native log {key} does not match executing repository")
    for records in [artifact_toolchain, source_materialization, container_toolchain]:
        for key, expected in records.items():
            if one_value(log, key) != expected:
                raise ValueError(f"native log {key} changed across runs")
    if json.loads(one_value(log, "source_files_json")) != source_files:
        raise ValueError("native source cone changed across runs")
payload = {
    "format": "oce-openmodelica-reliefs-native-architecture-v4",
    "architecture": architecture, "platform": facts[0], "host_architecture": facts[1],
    "docker_server_architecture": facts[2], "container_architecture": facts[3],
    "platform_manifest_digest": facts[4], "config_digest": facts[5],
    "repository_revision": revision, "generator_provenance_scope": "native_generation_and_publication",
    "generator_inputs": generator_inputs, "artifact_toolchain": artifact_toolchain,
    "source_materialization": source_materialization, "source_files": source_files, **container_toolchain,
    "raw_run_a_sha256": sha(output / "reliefs-run-a.raw.csv"),
    "raw_run_b_sha256": sha(output / "reliefs-run-b.raw.csv"),
    "canonical_sha256": sha(output / "reliefs.canonical.csv"),
    "parameter_control_raw_sha256": sha(output / "parameter-control.raw.csv"),
    "parameter_control_canonical_sha256": sha(output / "parameter-control.canonical.csv"),
    "final_clamp_raw_sha256": sha(output / "final-clamp.raw.csv"),
    "final_clamp_canonical_sha256": sha(output / "final-clamp.canonical.csv"),
    "projection_mutation": projection_evidence.record(output, sha(output / "reliefs-run-a.raw.csv"), sha),
    "runs": [
        {"id": run_id, "output_directory_token": token, "log_sha256": sha(output / log_name), "raw_sha256": sha(output / raw_name)}
        for run_id, token, log_name, raw_name in [
            ("run-a", "fresh-run-a", "run-a.log", "reliefs-run-a.raw.csv"),
            ("run-b", "fresh-run-b", "run-b.log", "reliefs-run-b.raw.csv"),
        ]
    ],
}
checkout_head = subprocess.run(
    ["git", "-C", root, "rev-parse", "HEAD"],
    check=True,
    capture_output=True,
    text=True,
).stdout.strip()
if payload["repository_revision"] != checkout_head:
    raise ValueError("architecture record revision is not the generation checkout HEAD")
encoded = (json.dumps(payload, indent=2) + "\n").encode()
descriptor = os.open(output / "architecture.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
try:
    os.write(descriptor, encoded)
finally:
    os.close(descriptor)
