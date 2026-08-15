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
for log in logs:
    if one_value(log, "repository_revision") != revision:
        raise ValueError("native log source revision does not match the executing repository")
    if one_value(log, "generator_provenance_scope") != "native_generation_and_publication":
        raise ValueError("native log has an unsupported generator provenance scope")
    for key, expected in recorded_inputs.items():
        if one_value(log, key) != expected:
            raise ValueError(f"native log {key} does not match the executing repository")
payload = {
    "format": "oce-openmodelica-line-native-architecture-v3",
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
