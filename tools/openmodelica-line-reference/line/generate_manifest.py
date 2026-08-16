#!/usr/bin/env python3
"""Generate the closed two-architecture Line manifest from retained files."""

import hashlib
import json
import os
import pathlib
import stat
import sys
import tempfile

MAX_FILE = 1024 * 1024
FIXTURE = "crates/oce-conformance/tests/fixtures/open_modelica/reals_line/"
TOOL = "tools/openmodelica-line-reference/"


def safe_directory(path, name):
    path = pathlib.Path(path).absolute()
    lexical_temp = pathlib.Path(tempfile.gettempdir()).absolute()
    resolved_temp = lexical_temp.resolve()
    try:
        path = resolved_temp / path.relative_to(lexical_temp)
    except ValueError:
        pass
    current = pathlib.Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        if current.is_symlink():
            raise ValueError(f"{name} contains a symlink component")
    if not stat.S_ISDIR(path.lstat().st_mode):
        raise ValueError(f"{name} is not a directory")
    return path


def read_bounded(path, trusted):
    path, trusted = pathlib.Path(path).absolute(), pathlib.Path(trusted).absolute()
    try:
        path.relative_to(trusted)
    except ValueError as error:
        raise ValueError("hash path escapes its trusted root") from error
    current = trusted
    for part in path.relative_to(trusted).parts:
        current /= part
        if current.is_symlink():
            raise ValueError("hash path contains a symlink component")
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or before.st_size > MAX_FILE:
        raise ValueError("hash input is not a bounded regular file")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0))
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode) or opened.st_size > MAX_FILE:
            raise ValueError("opened hash input is not a bounded regular file")
        data = os.read(descriptor, MAX_FILE + 1)
        if len(data) > MAX_FILE:
            raise ValueError("hash input exceeds bound")
        return data
    finally:
        os.close(descriptor)


output, root = safe_directory(sys.argv[1], "output"), safe_directory(sys.argv[2], "repository root")


def sha(path):
    path = pathlib.Path(path).absolute()
    trusted = output if path.is_relative_to(output) else root
    return hashlib.sha256(read_bounded(path, trusted)).hexdigest()


def architecture(name):
    path = output / name / "architecture.json"
    record = json.loads(read_bounded(path, output))
    return {
        "name": name,
        "platform": record["platform"],
        "host_architecture": record["host_architecture"],
        "docker_server_architecture": record["docker_server_architecture"],
        "container_architecture": record["container_architecture"],
        "platform_manifest_digest": record["platform_manifest_digest"],
        "config_digest": record["config_digest"],
        "repository_revision": record["repository_revision"],
        "generator_provenance_scope": record["generator_provenance_scope"],
        "generator_inputs": record["generator_inputs"],
        "artifact_toolchain": record["artifact_toolchain"],
        "source_materialization": record["source_materialization"],
        "omc_version": record["omc_version"],
        "gcc_version": record["gcc_version"],
        "binutils_version": record["binutils_version"],
        "glibc_version": record["glibc_version"],
        "raw_run_a_sha256": record["raw_run_a_sha256"],
        "raw_run_b_sha256": record["raw_run_b_sha256"],
        "flag_control_raw_sha256": record["flag_control_raw_sha256"],
        "canonical_sha256": record["canonical_sha256"],
        "flag_control_canonical_sha256": record["flag_control_canonical_sha256"],
        "runs": [
            {"id": "run-a", "output_directory_token": "fresh-run-a", "log_sha256": sha(output / name / "run-a.log"), "raw_sha256": record["raw_run_a_sha256"]},
            {"id": "run-b", "output_directory_token": "fresh-run-b", "log_sha256": sha(output / name / "run-b.log"), "raw_sha256": record["raw_run_b_sha256"]},
        ],
    }


roles = [
    ("image_index_json", FIXTURE + "image-index.json", output / "image-index.json"),
    ("cross_architecture_log", FIXTURE + "cross-architecture.log", output / "cross-architecture.log"),
]
architecture_files = [
    ("architecture_record", "architecture.json"),
    ("canonical_csv", "line.canonical.csv"),
    ("raw_run_a_csv", "line-run-a.raw.csv"),
    ("raw_run_b_csv", "line-run-b.raw.csv"),
    ("run_a_log", "run-a.log"),
    ("run_b_log", "run-b.log"),
    ("flag_control_canonical_csv", "flag-control.canonical.csv"),
    ("flag_control_raw_csv", "flag-control.raw.csv"),
    ("flag_control_log", "flag-control.log"),
    ("projection_mutation_log", "projection-mutation.log"),
    ("architecture_image_index_json", "image-index.json"),
    ("platform_image_manifest_json", "image-manifest.json"),
]
for name in ["arm64", "amd64"]:
    roles.extend((f"{name}_{role}", f"{FIXTURE}{name}/{file}", output / name / file) for role, file in architecture_files)
tracked = [
    ("canonicalizer_source", "crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"),
    ("tool_cargo_lock", TOOL + "Cargo.lock"),
    ("tool_cargo_toml", TOOL + "Cargo.toml"),
    ("tool_main_source", TOOL + "src/main.rs"),
    ("wrapper_model", TOOL + "line/LinePilot.mo"),
    ("flag_control_wrapper_model", TOOL + "line/LineFlagPilot.mo"),
    ("runner_script", TOOL + "line/runner.sh"),
    ("regeneration_script", TOOL + "line/regenerate.sh"),
    ("assembly_script", TOOL + "line/assemble.sh"),
    ("manifest_generator_script", TOOL + "line/generate_manifest.py"),
    ("architecture_generator_script", TOOL + "line/generate_architecture.py"),
    ("evidence_validator_script", TOOL + "line/verify_evidence.py"),
    ("safe_file_helper_script", TOOL + "line/safe_files.py"),
    ("oci_materializer_script", TOOL + "line/materialize_oci.py"),
    ("deadline_script", TOOL + "line/deadline.sh"),
    ("deadline_test_script", TOOL + "line/deadline_test.sh"),
    ("output_publish_script", TOOL + "line/output_publish.py"),
    ("output_publish_test_script", TOOL + "line/output_publish_test.sh"),
    ("container_cleanup_script", TOOL + "line/container_cleanup.sh"),
    ("container_cleanup_test_script", TOOL + "line/container_cleanup_test.sh"),
    ("oci_index_source", TOOL + "line/image-index.json"),
    ("arm64_manifest_source", TOOL + "line/image-manifest-arm64.json"),
    ("amd64_manifest_source", TOOL + "line/image-manifest-amd64.json"),
    ("evidence_workflow", ".github/workflows/openmodelica-line-evidence.yml"),
]
roles.extend((role, path, root / path) for role, path in tracked)

time_bits = "0000000000000000 404e000000000000 404e000000000eff 405e000000000000 405e000000000781 4066800000000000 40668000000003c1 406e000000000000 406e0000000003c1 4072c00000000000".split()
u_bits = "c010000000000000 c010000000000000 c000000000000000 c000000000000000 0000000000000000 0000000000000000 4000000000000000 4000000000000000 4010000000000000 4010000000000000".split()
outputs = {
    "yBoth": ["3ff4000000000000"] * 4 + ["4002000000000000"] * 2 + ["400a000000000000"] * 4,
    "yBelow": ["3ff4000000000000"] * 4 + ["4002000000000000"] * 2 + ["400a000000000000"] * 2 + ["4011000000000000"] * 2,
    "yAbove": ["3fd0000000000000"] * 2 + ["3ff4000000000000"] * 2 + ["4002000000000000"] * 2 + ["400a000000000000"] * 4,
    "yUnlimited": ["3fd0000000000000"] * 2 + ["3ff4000000000000"] * 2 + ["4002000000000000"] * 2 + ["400a000000000000"] * 2 + ["4011000000000000"] * 2,
}


def source_file(path, digest):
    return {"path": path, "committed_sha256": digest, "materialized_sha256": digest}


arm, amd = architecture("arm64"), architecture("amd64")
canonical_sha = arm["canonical_sha256"]
manifest = {
    "format": "oce-openmodelica-line-external-run-v1",
    "scope": {"class": "CDL.Reals.Line", "scenario": "four_limit_modes_five_dyadic_regions", "inputs": ["x1", "f1", "x2", "f2", "u"], "outputs": ["yBoth", "yBelow", "yAbove", "yUnlimited"], "comparison": "exact_finite_f64_bits", "global_tier3_status": "skipped"},
    "image": {"repository": "openmodelica/openmodelica", "tag": "v1.25.1-minimal", "index_digest": "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864", "platforms": [{"platform": item["platform"], "manifest_digest": item["platform_manifest_digest"], "config_digest": item["config_digest"]} for item in [arm, amd]]},
    "sources": [
        {"name": "buildings", "repository": "https://github.com/lbl-srg/modelica-buildings.git", "commit": "a131864e4c4df22ebcd52bb8da439de0087ac365", "tree": "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09", "package": "Buildings", "version": "14.0.0", "materialization": "git_archive_without_local_attribute_override", "transforms": [], "files": [source_file("Buildings/package.mo", "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59"), source_file("Buildings/Controls/OBC/CDL/Reals/Line.mo", "85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5")]},
        {"name": "modelica", "repository": "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git", "commit": "7a4bf7de77a3986e8eb1e88cbb515d646f78f834", "tree": "43d7d8fc1a991358e9e5e91976e27cdc4280173f", "package": "Modelica", "version": "4.1.0", "materialization": "git_archive_with_pinned_modelica_export_subst", "transforms": [{"path": "Modelica/package.mo", "rule": "Modelica/package.mo -export-subst"}], "files": [source_file("Complex.mo", "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f"), source_file("Modelica/package.mo", "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191"), source_file("Modelica/Blocks/Sources.mo", "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3"), source_file("ModelicaServices/package.mo", "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb")]},
    ],
    "simulation": {"method": "dassl", "start_time": "0", "stop_time": "300", "number_of_intervals": 5, "tolerance": "1e-9", "output_format": "csv", "variable_filter": "^(x1|f1|x2|f2|u|yBoth|yBelow|yAbove|yUnlimited)$", "simflags": "", "event_emission": True, "raw_header": '"time","f1","f2","u","x1","x2","yAbove","yBelow","yBoth","yUnlimited"'},
    "projection": {"columns": ["time", "x1", "f1", "x2", "f2", "u", "yBoth", "yBelow", "yAbove", "yUnlimited"], "grouping": "contiguous_equal_f64_bits", "selection": "last", "normalize_times": False, "raw_rows": 15, "canonical_rows": 10, "group_sizes": [1, 1, 2, 1, 2, 1, 2, 1, 2, 2], "canonical_time_bits": time_bits, "canonical_input_bits": {"x1": ["c000000000000000"] * 10, "f1": ["3ff4000000000000"] * 10, "x2": ["4000000000000000"] * 10, "f2": ["400a000000000000"] * 10, "u": u_bits}},
    "expected_output_bits": outputs,
    "architectures": [arm, amd],
    "semantic_control": {"mutation": "yBelow limitAbove false to true", "first_mismatch_row": 8, "first_mismatch_time_bits": time_bits[8], "expected_comparison": "exact_mismatch", "mismatch_rows": [8, 9]},
    "cross_architecture": {"comparison": "canonical_bytes", "arm64_sha256": canonical_sha, "amd64_sha256": canonical_sha, "result": "pass"},
    "artifacts": [{"role": role, "path": path, "sha256": sha(file)} for role, path, file in roles],
    "regeneration": {"entrypoint": TOOL + "line/regenerate.sh", "assembly_entrypoint": TOOL + "line/assemble.sh", "evidence_workflow": ".github/workflows/openmodelica-line-evidence.yml", "network": "none_during_container_execution", "pull": "never", "platforms": ["linux/arm64", "linux/amd64"], "source_materialization": "git_archive_with_pinned_modelica_export_subst", "source_mounts": "read_only", "container_root": "read_only", "container_user": "non_root", "capabilities": "none", "no_new_privileges": True, "device_mounts": 0, "docker_socket_mounted": False, "timeout_seconds": 120, "cpus": "4", "memory_bytes": 2147483648, "memory_swap_bytes": 2147483648, "pids_limit": 256, "tmpfs_bytes": 268435456, "per_file_bytes": 67108864, "output_directory_bytes": 268435456},
}
payload = (json.dumps(manifest, indent=2) + "\n").encode()
descriptor = os.open(output / "manifest.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
try:
    os.write(descriptor, payload)
finally:
    os.close(descriptor)
