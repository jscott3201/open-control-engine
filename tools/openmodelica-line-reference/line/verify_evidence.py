#!/usr/bin/env python3
"""Validate one native or assembled Line evidence directory."""

import csv
import hashlib
import io
import json
import math
import os
import pathlib
import stat
import struct
import subprocess
import sys
from typing import Any, NoReturn

import safe_files

MAX_FILE = 1024 * 1024
MAX_MANIFEST = 256 * 1024
INDEX = "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864"
ARM_RAW = "52691be2b8ed547f2f4d5c0b3efefb71d7d273bb9e48c43636bddb82b8247984"
HEADER = '"time","f1","f2","u","x1","x2","yAbove","yBelow","yBoth","yUnlimited"'
COLUMNS = ["time", "x1", "f1", "x2", "f2", "u", "yBoth", "yBelow", "yAbove", "yUnlimited"]
TIME_BITS = [
    "0000000000000000", "404e000000000000", "404e000000000eff", "405e000000000000",
    "405e000000000781", "4066800000000000", "40668000000003c1", "406e000000000000",
    "406e0000000003c1", "4072c00000000000",
]
GROUPS = [1, 1, 2, 1, 2, 1, 2, 1, 2, 2]
U_BITS = ["c010000000000000", "c010000000000000", "c000000000000000", "c000000000000000", "0000000000000000", "0000000000000000", "4000000000000000", "4000000000000000", "4010000000000000", "4010000000000000"]
OUTPUT_BITS = {
    "yBoth": ["3ff4000000000000"] * 4 + ["4002000000000000"] * 2 + ["400a000000000000"] * 4,
    "yBelow": ["3ff4000000000000"] * 4 + ["4002000000000000"] * 2 + ["400a000000000000"] * 2 + ["4011000000000000"] * 2,
    "yAbove": ["3fd0000000000000"] * 2 + ["3ff4000000000000"] * 2 + ["4002000000000000"] * 2 + ["400a000000000000"] * 4,
    "yUnlimited": ["3fd0000000000000"] * 2 + ["3ff4000000000000"] * 2 + ["4002000000000000"] * 2 + ["400a000000000000"] * 2 + ["4011000000000000"] * 2,
}
ARCH = {
    "arm64": ("linux/arm64", "arm64", "aarch64", "aarch64", "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4", "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666"),
    "amd64": ("linux/amd64", "amd64", "x86_64", "x86_64", "sha256:92d0779a01e7d43ed4d5ecb4cfd9754cb259b30673ddb454b5a32e3eb8665f11", "sha256:0c81120bb392de44cab0e9ff6818d0a44afad657d5b401f25e148fa6c26e5347"),
}
ARCH_FILES = {
    "architecture.json", "line.canonical.csv", "line-run-a.raw.csv", "line-run-b.raw.csv",
    "run-a.log", "run-b.log", "flag-control.canonical.csv", "flag-control.raw.csv",
    "flag-control.log", "projection-mutation.log", "image-index.json", "image-manifest.json",
}
GENERATOR_INPUT_PATHS = {
    "line_pilot_sha256": "tools/openmodelica-line-reference/line/LinePilot.mo",
    "line_flag_pilot_sha256": "tools/openmodelica-line-reference/line/LineFlagPilot.mo",
    "runner_sha256": "tools/openmodelica-line-reference/line/runner.sh",
    "regenerate_sha256": "tools/openmodelica-line-reference/line/regenerate.sh",
    "canonicalizer_sha256": "crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs",
    "tool_main_sha256": "tools/openmodelica-line-reference/src/main.rs",
    "tool_cargo_toml_sha256": "tools/openmodelica-line-reference/Cargo.toml",
    "tool_cargo_lock_sha256": "tools/openmodelica-line-reference/Cargo.lock",
    "architecture_generator_sha256": "tools/openmodelica-line-reference/line/generate_architecture.py",
    "architecture_verifier_sha256": "tools/openmodelica-line-reference/line/verify_evidence.py",
    "safe_file_helper_sha256": "tools/openmodelica-line-reference/line/safe_files.py",
    "evidence_workflow_sha256": ".github/workflows/openmodelica-line-evidence.yml",
    "oci_materializer_sha256": "tools/openmodelica-line-reference/line/materialize_oci.py",
    "deadline_sha256": "tools/openmodelica-line-reference/line/deadline.sh",
    "deadline_test_sha256": "tools/openmodelica-line-reference/line/deadline_test.sh",
    "container_cleanup_sha256": "tools/openmodelica-line-reference/line/container_cleanup.sh",
    "container_cleanup_test_sha256": "tools/openmodelica-line-reference/line/container_cleanup_test.sh",
    "output_publish_sha256": "tools/openmodelica-line-reference/line/output_publish.py",
    "output_publish_test_sha256": "tools/openmodelica-line-reference/line/output_publish_test.sh",
    "oci_index_source_sha256": "tools/openmodelica-line-reference/line/image-index.json",
    "arm64_manifest_source_sha256": "tools/openmodelica-line-reference/line/image-manifest-arm64.json",
    "amd64_manifest_source_sha256": "tools/openmodelica-line-reference/line/image-manifest-amd64.json",
}


def fail(detail: str) -> NoReturn:
    raise ValueError(f"Line evidence verification failed: {detail}")


def read_bounded(path: pathlib.Path, limit: int = MAX_FILE) -> bytes:
    try:
        return safe_files.read_bounded(path, limit)
    except ValueError as error:
        fail(str(error))


def text(path: pathlib.Path, limit: int = MAX_FILE) -> str:
    try:
        return read_bounded(path, limit).decode("utf-8")
    except UnicodeError as error:
        fail(f"invalid UTF-8 in {path}: {error}")


def sha(path):
    return hashlib.sha256(read_bounded(path)).hexdigest()


def pairs(values):
    output = {}
    for key, value in values:
        if key in output:
            fail(f"duplicate JSON key {key}")
        output[key] = value
    return output


def json_file(path: pathlib.Path, limit: int = MAX_FILE) -> Any:
    try:
        return json.loads(text(path, limit), object_pairs_hook=pairs)
    except json.JSONDecodeError as error:
        fail(f"invalid JSON in {path}: {error}")


def closed(value, keys, name):
    if not isinstance(value, dict) or set(value) != set(keys):
        fail(f"{name} fields are not closed")


def expected_artifact_toolchain(architecture):
    host = "aarch64-unknown-linux-gnu" if architecture == "arm64" else "x86_64-unknown-linux-gnu"
    return {
        "rustc_release": "1.97.1",
        "rustc_commit_hash": "8bab26f4f68e0e26f0bb7960be334d5b520ea452",
        "rustc_commit_date": "2026-07-14", "rustc_host": host,
        "rustc_llvm_version": "22.1.6", "cargo_release": "1.97.1",
        "cargo_commit_hash": "c980f4866141969fab6254a680546a277789d6f0",
        "cargo_commit_date": "2026-06-30", "cargo_host": host,
        "python_version": "Python 3.13.7",
    }


def expected_source_materialization():
    return {
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


def bits(value):
    return f"{struct.unpack('>Q', struct.pack('>d', value))[0]:016x}"


def safe_directory(path, name):
    path = pathlib.Path(path).absolute()
    try:
        descriptor = safe_files.open_directory(path)
        os.close(descriptor)
    except (OSError, ValueError) as error:
        fail(f"invalid {name}: {error}")
    return path


def parse_raw(path, expected_digest=None):
    raw = read_bounded(path)
    if expected_digest is not None and hashlib.sha256(raw).hexdigest() != expected_digest:
        fail(f"unexpected raw digest for {path.name}")
    try:
        body = raw.decode("ascii")
    except UnicodeError as error:
        fail(f"raw CSV is not ASCII: {error}")
    if "\r" in body or not body.endswith("\n"):
        fail(f"invalid raw line endings for {path.name}")
    lines = body.splitlines()
    if not lines or lines[0] != HEADER:
        fail(f"unexpected raw header for {path.name}")
    try:
        parsed = list(csv.reader(io.StringIO(body), strict=True))
    except csv.Error as error:
        fail(f"invalid raw CSV for {path.name}: {error}")
    if parsed[0] != ["time", "f1", "f2", "u", "x1", "x2", "yAbove", "yBelow", "yBoth", "yUnlimited"] or len(parsed) != 16:
        fail(f"unexpected raw shape for {path.name}")
    rows = []
    for index, (line, row) in enumerate(zip(lines[1:], parsed[1:])):
        lexical = line.split(",")
        if row != lexical or len(row) != 10 or any(cell == "" for cell in row):
            fail(f"raw row {index} has invalid quoting or width")
        try:
            values = [float(cell) for cell in row]
        except ValueError:
            fail(f"raw row {index} has a non-numeric cell")
        if not all(math.isfinite(value) for value in values):
            fail(f"raw row {index} has a non-finite cell")
        # Raw OMC order is projected into the canonical order here.
        rows.append((values[0], values[4], values[1], values[5], values[2], values[3], values[8], values[7], values[6], values[9]))
    groups = []
    prior = None
    for index, row in enumerate(rows):
        if prior is not None and row[0] < prior:
            fail(f"raw time decreases at row {index}")
        time_bits = bits(row[0])
        if groups and groups[-1][0] == time_bits:
            groups[-1][1].append(row)
        else:
            if any(group[0] == time_bits for group in groups):
                fail(f"raw equal-time group is noncontiguous at row {index}")
            groups.append((time_bits, [row]))
        prior = row[0]
    if [len(group[1]) for group in groups] != GROUPS or [group[0] for group in groups] != TIME_BITS:
        fail(f"raw groups or timestamp bits drifted for {path.name}")
    return rows, groups


def expected_projection(groups, keep_last=True):
    return [[bits(value) for value in group[1][-1 if keep_last else 0]] for group in groups]


def parse_canonical(path, table):
    lines = text(path).splitlines()
    expected_header = ["#1", "# columns: " + " ".join(COLUMNS), f"double {table}(10,10)"]
    if lines[:3] != expected_header or len(lines) != 13:
        fail(f"unexpected canonical header or shape for {path.name}")
    rows = []
    for index, line in enumerate(lines[3:]):
        cells = line.split(" ")
        if len(cells) != 10 or any(cell == "" for cell in cells):
            fail(f"canonical row {index} width")
        try:
            values = [float(cell) for cell in cells]
        except ValueError:
            fail(f"canonical row {index} has an invalid cell")
        if not all(math.isfinite(value) for value in values):
            fail(f"canonical row {index} has a non-finite cell")
        rows.append([bits(value) for value in values])
    return rows


def assert_schedule(rows):
    for index, row in enumerate(rows):
        expected = [TIME_BITS[index], "c000000000000000", "3ff4000000000000", "4000000000000000", "400a000000000000", U_BITS[index]]
        if row[:6] != expected:
            fail(f"canonical input schedule drifted at row {index}")
        for column, name in enumerate(COLUMNS[6:], 6):
            if row[column] != OUTPUT_BITS[name][index]:
                fail(f"independent output table mismatch at row {index} column {name}")


def one_log_value(body, key, expected):
    values = [line.removeprefix(key + "=") for line in body.splitlines() if line.startswith(key + "=")]
    if values != [expected]:
        fail(f"log must contain exactly one {key}={expected}; found {values}")


def log_contract(path, architecture, token, model, raw_digest, revision, generator_inputs):
    body = text(path)
    platform, host, server, container, manifest, config = ARCH[architecture]
    required = {
        "host_architecture": host, "docker_server_architecture": server, "image_index_digest": INDEX,
        "image_platform_manifest_digest": manifest, "image_config_digest": config,
        "oci_metadata_validation": "raw_digest_and_pinned_graph", "pull_policy": "never",
        "host_timeout_seconds": "120", "buildings_remote": "https://github.com/lbl-srg/modelica-buildings.git",
        "buildings_commit": "a131864e4c4df22ebcd52bb8da439de0087ac365", "buildings_tree": "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09",
        "modelica_remote": "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git",
        "modelica_commit": "7a4bf7de77a3986e8eb1e88cbb515d646f78f834", "modelica_tree": "43d7d8fc1a991358e9e5e91976e27cdc4280173f",
        "repository_revision": revision,
        "generator_provenance_scope": "native_generation_and_publication",
        "sources_source_committed_sha256": "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3",
        "sources_source_materialized_sha256": "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3",
        "modelica_services_committed_sha256": "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb",
        "modelica_services_materialized_sha256": "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb",
        "complex_committed_sha256": "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f",
        "complex_materialized_sha256": "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f",
        "output_directory_token": token, "selected_model": model, "container_architecture": container,
        "modelica_path": "", "root_write_probe": "read-only", "source_write_probe": "read-only",
        "reference_write_probe": "read-only", "network_route_lines": "1", "cgroup_memory_max": "2147483648",
        "cgroup_pids_max": "256", "cgroup_cpu_max": "400000 100000", "per_file_limit_bytes": "67108864",
        "output_directory_limit_bytes": "268435456", "gcc_version": "11.4.0", "binutils_version": "2.38",
        "glibc_version": "2.35", "omc_version": "OpenModelica 1.25.1",
        "line_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Line.mo",
        "constant_source": "/sources/modelica/Modelica/Blocks/Sources.mo",
        "time_table_source": "/sources/modelica/Modelica/Blocks/Sources.mo", "Modelica": "4.1.0",
        "Buildings": "14.0.0", "omc_warning_count": "0", "raw_sha256": raw_digest, "runner_complete": "1",
    }
    required.update(generator_inputs)
    required.update(expected_artifact_toolchain(architecture))
    required.update(expected_source_materialization())
    command = f"docker run --pull=never --platform {platform} --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro"
    required["docker_command"] = command
    for key, expected in required.items():
        one_log_value(body, key, expected)
    identities = [line.removeprefix("container_identity=") for line in body.splitlines() if line.startswith("container_identity=")]
    if len(identities) != 1 or not identities[0].replace(":", "", 1).isdigit() or identities[0].split(":")[0] == "0":
        fail(f"{path.name} container identity")
    sizes = [line.removeprefix("output_directory_kib=") for line in body.splitlines() if line.startswith("output_directory_kib=")]
    if len(sizes) != 1 or not sizes[0].isdigit() or int(sizes[0]) * 1024 > 268435456:
        fail(f"{path.name} output size")
    simulation = "simulationOptions = \"startTime = 0.0, stopTime = 300.0, numberOfIntervals = 5, tolerance = 1e-9, method = 'dassl', fileNamePrefix = 'LinePilot', options = '', outputFormat = 'csv', variableFilter = '^(x1|f1|x2|f2|u|yBoth|yBelow|yAbove|yUnlimited)$', cflags = '', simflags = ''\","
    if [line.strip() for line in body.splitlines()].count(simulation) != 1:
        fail(f"{path.name} simulation options")
    if any("warning" in line.lower() and line != "omc_warning_count=0" for line in body.splitlines()):
        fail(f"{path.name} contains an OpenModelica warning")
    for success in ["The initialization finished successfully", "The simulation finished successfully"]:
        if body.count(success) != 1:
            fail(f"{path.name} success record")


def validate_wrappers(root):
    main = text(root / "tools/openmodelica-line-reference/line/LinePilot.mo")
    control = text(root / "tools/openmodelica-line-reference/line/LineFlagPilot.mo")
    anchor = "Line below(limitBelow=true, limitAbove=false);"
    mutated = "Line below(limitBelow=true, limitAbove=true);"
    if main.count(anchor) != 1 or main.replace(anchor, mutated) != control:
        fail("flag control wrapper is not the one-token external flag mutation")
    if main.count("Buildings.Controls.OBC.CDL.Reals.Line") != 4:
        fail("wrapper must contain exactly four upstream Line instances")
    for forbidden in ["expected", "assert(", "FMU", "FMI", "oce-"]:
        if forbidden in main or forbidden in control:
            fail(f"wrapper contains forbidden token {forbidden}")


def validate_oci(directory, architecture):
    index_path, manifest_path = directory / "image-index.json", directory / "image-manifest.json"
    if "sha256:" + sha(index_path) != INDEX or "sha256:" + sha(manifest_path) != ARCH[architecture][4]:
        fail("OCI metadata digest")
    index, manifest = json_file(index_path), json_file(manifest_path)
    selected = [item for item in index.get("manifests", []) if item.get("platform") == {"architecture": architecture, "os": "linux"}]
    if len(selected) != 1 or selected[0].get("digest") != ARCH[architecture][4] or manifest.get("config", {}).get("digest") != ARCH[architecture][5]:
        fail("OCI graph")


def validate_projection(path, raw_digest, canonicalizer_digest):
    expected = [
        "projection_mutation=contiguous equal-time selection changed from last to first", "working_tree_modified=false",
        "mutated_compile=PASS", "mutated_input=line-run-a.raw.csv", f"mutated_input_sha256={raw_digest}",
        "mutated_raw_rows=15", "mutated_canonical_rows=10", "mutated_group_sizes=1,1,2,1,2,1,2,1,2,2",
        "mutated_canonical_time_bits=" + ",".join(TIME_BITS), "mutated_schedule_result=FAIL",
        "mutated_schedule_mismatch_rows=2,4,6,8", "mutated_schedule_first_mismatch_row=2",
        "mutated_schedule_first_mismatch_time_bits=404e000000000eff", "mutated_grouping_result=PASS",
        "mutated_timestamp_bits_result=PASS", "restoration_result=PASS", f"restored_canonicalizer_sha256={canonicalizer_digest}",
    ]
    if text(path).splitlines() != expected:
        fail("projection mutation record")


def strict_canonical_boundary(directory, root):
    command = [
        "cargo", "run", "--manifest-path",
        str(root / "tools/openmodelica-line-reference/Cargo.toml"),
        "--offline", "--locked", "--quiet", "--",
        "verify-architecture-canonical", str(directory),
    ]
    environment = os.environ.copy()
    environment["CARGO_NET_OFFLINE"] = "true"
    result = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True)
    if result.returncode != 0 or result.stdout != "strict canonical boundary passed\n":
        fail(f"strict Rust canonical boundary: {result.stderr.strip()}")


def validate_architecture(directory, root, architecture):
    directory, root = safe_directory(directory, "architecture evidence"), safe_directory(root, "repository root")
    try:
        safe_files.read_closed_directory(directory, ARCH_FILES, MAX_FILE)
    except (OSError, ValueError) as error:
        fail(f"unsafe architecture evidence: {error}")
    record = json_file(directory / "architecture.json")
    fields = ["format", "architecture", "platform", "host_architecture", "docker_server_architecture", "container_architecture", "platform_manifest_digest", "config_digest", "repository_revision", "generator_provenance_scope", "generator_inputs", "artifact_toolchain", "source_materialization", "omc_version", "gcc_version", "binutils_version", "glibc_version", "raw_run_a_sha256", "raw_run_b_sha256", "flag_control_raw_sha256", "canonical_sha256", "flag_control_canonical_sha256", "runs"]
    closed(record, fields, "architecture record")
    expected = ARCH[architecture]
    if [record["format"], record["architecture"], record["platform"], record["host_architecture"], record["docker_server_architecture"], record["container_architecture"], record["platform_manifest_digest"], record["config_digest"], record["generator_provenance_scope"]] != ["oce-openmodelica-line-native-architecture-v4", architecture, *expected, "native_generation_and_publication"]:
        fail("architecture literals")
    revision = record["repository_revision"]
    if not isinstance(revision, str) or len(revision) != 40 or any(char not in "0123456789abcdef" for char in revision):
        fail("architecture repository revision")
    closed(record["generator_inputs"], GENERATOR_INPUT_PATHS, "architecture generator inputs")
    expected_inputs = {
        key: hashlib.sha256(artifact_bytes(directory, root, path)).hexdigest()
        for key, path in GENERATOR_INPUT_PATHS.items()
    }
    if record["generator_inputs"] != expected_inputs:
        fail("native generator inputs do not match assembly repository bytes")
    if not type_exact_equal(record["artifact_toolchain"], expected_artifact_toolchain(architecture)):
        fail("native artifact toolchain identity")
    if not type_exact_equal(record["source_materialization"], expected_source_materialization()):
        fail("native source materialization identity")
    if [record["omc_version"], record["gcc_version"], record["binutils_version"], record["glibc_version"]] != ["OpenModelica 1.25.1", "11.4.0", "2.38", "2.35"]:
        fail("native container toolchain identity")
    strict_canonical_boundary(directory, root)
    for name, key in [("line-run-a.raw.csv", "raw_run_a_sha256"), ("line-run-b.raw.csv", "raw_run_b_sha256"), ("flag-control.raw.csv", "flag_control_raw_sha256"), ("line.canonical.csv", "canonical_sha256"), ("flag-control.canonical.csv", "flag_control_canonical_sha256")]:
        if sha(directory / name) != record[key]:
            fail(f"architecture digest binding for {name}")
    if record["raw_run_a_sha256"] != record["raw_run_b_sha256"]:
        fail("native repeat raw digests differ")
    if not type_exact_equal(record["runs"], expected_runs(directory, record)):
        fail("native repeat run records")
    if architecture == "arm64" and record["raw_run_a_sha256"] != ARM_RAW:
        fail("arm64 raw digest drifted from the measured spike")
    rows_a, groups = parse_raw(directory / "line-run-a.raw.csv", record["raw_run_a_sha256"])
    rows_b, _ = parse_raw(directory / "line-run-b.raw.csv", record["raw_run_b_sha256"])
    control_rows, control_groups = parse_raw(directory / "flag-control.raw.csv", record["flag_control_raw_sha256"])
    if rows_a != rows_b:
        fail("repeat raw rows differ")
    projected = expected_projection(groups)
    assert_schedule(projected)
    if parse_canonical(directory / "line.canonical.csv", "openmodelica_reals_line") != projected:
        fail("canonical output is not the raw keep-last projection")
    control_projected = expected_projection(control_groups)
    if parse_canonical(directory / "flag-control.canonical.csv", "openmodelica_reals_line_flag_control") != control_projected:
        fail("flag-control canonical output is not the raw keep-last projection")
    if any(row[:6] != control[:6] for row, control in zip(projected, control_projected)):
        fail("flag control changed the input schedule")
    differences = [index for index, (row, control) in enumerate(zip(projected, control_projected)) if row[7] != control[7]]
    if differences != [8, 9] or control_projected[8][7] != OUTPUT_BITS["yBoth"][8]:
        fail("external flag control did not fail at the pinned above-range rows")
    for name, token, model, digest in [("run-a.log", "fresh-run-a", "Line", record["raw_run_a_sha256"]), ("run-b.log", "fresh-run-b", "Line", record["raw_run_b_sha256"]), ("flag-control.log", "fresh-flag-control", "FlagControl", record["flag_control_raw_sha256"])]:
        log_contract(directory / name, architecture, token, model, digest, revision, expected_inputs)
    validate_oci(directory, architecture)
    validate_wrappers(root)
    validate_projection(directory / "projection-mutation.log", record["raw_run_a_sha256"], sha(root / "crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"))
    return record, projected


def valid_relative(path):
    pure = pathlib.PurePosixPath(path)
    return bool(path) and len(path) <= 512 and not pure.is_absolute() and all(part not in ("", ".", "..") and not part.endswith((" ", ".")) for part in pure.parts) and not any(char in path for char in "\\:*?")


def expected_artifact_roles():
    fixture = "crates/oce-conformance/tests/fixtures/open_modelica/reals_line/"
    roles = [
        ("image_index_json", fixture + "image-index.json"),
        ("cross_architecture_log", fixture + "cross-architecture.log"),
    ]
    files = [
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
    for architecture in ["arm64", "amd64"]:
        roles.extend(
            (f"{architecture}_{role}", f"{fixture}{architecture}/{file}")
            for role, file in files
        )
    tracked = [
        ("canonicalizer_source", "crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"),
        ("tool_cargo_lock", "tools/openmodelica-line-reference/Cargo.lock"),
        ("tool_cargo_toml", "tools/openmodelica-line-reference/Cargo.toml"),
        ("tool_main_source", "tools/openmodelica-line-reference/src/main.rs"),
        ("wrapper_model", "tools/openmodelica-line-reference/line/LinePilot.mo"),
        ("flag_control_wrapper_model", "tools/openmodelica-line-reference/line/LineFlagPilot.mo"),
        ("runner_script", "tools/openmodelica-line-reference/line/runner.sh"),
        ("regeneration_script", "tools/openmodelica-line-reference/line/regenerate.sh"),
        ("assembly_script", "tools/openmodelica-line-reference/line/assemble.sh"),
        ("manifest_generator_script", "tools/openmodelica-line-reference/line/generate_manifest.py"),
        ("architecture_generator_script", "tools/openmodelica-line-reference/line/generate_architecture.py"),
        ("evidence_validator_script", "tools/openmodelica-line-reference/line/verify_evidence.py"),
        ("safe_file_helper_script", "tools/openmodelica-line-reference/line/safe_files.py"),
        ("oci_materializer_script", "tools/openmodelica-line-reference/line/materialize_oci.py"),
        ("deadline_script", "tools/openmodelica-line-reference/line/deadline.sh"),
        ("deadline_test_script", "tools/openmodelica-line-reference/line/deadline_test.sh"),
        ("output_publish_script", "tools/openmodelica-line-reference/line/output_publish.py"),
        ("output_publish_test_script", "tools/openmodelica-line-reference/line/output_publish_test.sh"),
        ("container_cleanup_script", "tools/openmodelica-line-reference/line/container_cleanup.sh"),
        ("container_cleanup_test_script", "tools/openmodelica-line-reference/line/container_cleanup_test.sh"),
        ("oci_index_source", "tools/openmodelica-line-reference/line/image-index.json"),
        ("arm64_manifest_source", "tools/openmodelica-line-reference/line/image-manifest-arm64.json"),
        ("amd64_manifest_source", "tools/openmodelica-line-reference/line/image-manifest-amd64.json"),
        ("evidence_workflow", ".github/workflows/openmodelica-line-evidence.yml"),
    ]
    return roles + tracked


def artifact_bytes(output, root, relative):
    fixture = "crates/oce-conformance/tests/fixtures/open_modelica/reals_line/"
    base, suffix = (output, relative.removeprefix(fixture)) if relative.startswith(fixture) else (root, relative)
    if not valid_relative(suffix):
        fail("invalid repository-relative artifact path")
    components = pathlib.PurePosixPath(suffix).parts
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_DIRECTORY", 0)
    directory = os.open(base, flags)
    try:
        for component in components[:-1]:
            following = os.open(component, flags, dir_fd=directory)
            os.close(directory); directory = following
        descriptor = os.open(components[-1], os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0), dir_fd=directory)
        try:
            opened = os.fstat(descriptor)
            if not stat.S_ISREG(opened.st_mode) or opened.st_size > MAX_FILE:
                fail(f"artifact is not a bounded regular file: {relative}")
            data = os.read(descriptor, MAX_FILE + 1)
            if len(data) > MAX_FILE:
                fail(f"artifact exceeds bound: {relative}")
            return data
        finally:
            os.close(descriptor)
    finally:
        os.close(directory)


def expected_architecture_manifest(output, name, record):
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
        "runs": record["runs"],
    }


def expected_runs(base, record):
    return [{"id": "run-a", "output_directory_token": "fresh-run-a", "log_sha256": sha(base / "run-a.log"), "raw_sha256": record["raw_run_a_sha256"]}, {"id": "run-b", "output_directory_token": "fresh-run-b", "log_sha256": sha(base / "run-b.log"), "raw_sha256": record["raw_run_b_sha256"]}]


def expected_sources():
    source_file = lambda path, digest: {
        "path": path, "committed_sha256": digest, "materialized_sha256": digest,
    }
    return [
        {"name": "buildings", "repository": "https://github.com/lbl-srg/modelica-buildings.git", "commit": "a131864e4c4df22ebcd52bb8da439de0087ac365", "tree": "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09", "package": "Buildings", "version": "14.0.0", "materialization": "git_archive_without_local_attribute_override", "transforms": [], "files": [
            source_file("Buildings/package.mo", "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59"),
            source_file("Buildings/Controls/OBC/CDL/Reals/Line.mo", "85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5"),
        ]},
        {"name": "modelica", "repository": "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git", "commit": "7a4bf7de77a3986e8eb1e88cbb515d646f78f834", "tree": "43d7d8fc1a991358e9e5e91976e27cdc4280173f", "package": "Modelica", "version": "4.1.0", "materialization": "git_archive_with_pinned_modelica_export_subst", "transforms": [{"path": "Modelica/package.mo", "rule": "Modelica/package.mo -export-subst"}], "files": [
            source_file("Complex.mo", "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f"),
            source_file("Modelica/package.mo", "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191"),
            source_file("Modelica/Blocks/Sources.mo", "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3"),
            source_file("ModelicaServices/package.mo", "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb"),
        ]},
    ]


def validate_final(output, root):
    output, root = safe_directory(output, "assembled evidence"), safe_directory(root, "repository root")
    if {entry.name for entry in os.scandir(output)} != {"manifest.json", "image-index.json", "cross-architecture.log", "arm64", "amd64"}:
        fail("assembled evidence entries are not closed")
    manifest = json_file(output / "manifest.json", MAX_MANIFEST)
    closed(manifest, ["format", "scope", "image", "sources", "simulation", "projection", "expected_output_bits", "architectures", "semantic_control", "cross_architecture", "artifacts", "regeneration"], "manifest")
    if manifest["format"] != "oce-openmodelica-line-external-run-v1":
        fail("unsupported manifest format")
    expected_scope = {"class": "CDL.Reals.Line", "scenario": "four_limit_modes_five_dyadic_regions", "inputs": ["x1", "f1", "x2", "f2", "u"], "outputs": ["yBoth", "yBelow", "yAbove", "yUnlimited"], "comparison": "exact_finite_f64_bits", "global_tier3_status": "skipped"}
    if not type_exact_equal(manifest["scope"], expected_scope):
        fail("unsupported scope")
    artifacts = manifest["artifacts"]
    roles = expected_artifact_roles()
    if not isinstance(artifacts, list) or len(artifacts) != len(roles):
        fail("artifact closure count")
    for item, expected_role_path in zip(artifacts, roles):
        closed(item, ["role", "path", "sha256"], "artifact")
        if (item["role"], item["path"]) != expected_role_path:
            fail("unknown or misplaced artifact role/path")
        if not valid_relative(item["path"]) or not isinstance(item["sha256"], str) or len(item["sha256"]) != 64 or any(char not in "0123456789abcdef" for char in item["sha256"]):
            fail("invalid artifact literal")
        if hashlib.sha256(artifact_bytes(output, root, item["path"])).hexdigest() != item["sha256"]:
            fail(f"artifact digest mismatch: {item['path']}")

    expected_image = {
        "repository": "openmodelica/openmodelica",
        "tag": "v1.25.1-minimal",
        "index_digest": INDEX,
        "platforms": [
            {"platform": ARCH[name][0], "manifest_digest": ARCH[name][4], "config_digest": ARCH[name][5]}
            for name in ["arm64", "amd64"]
        ],
    }
    expected_simulation = {
        "method": "dassl", "start_time": "0", "stop_time": "300", "number_of_intervals": 5,
        "tolerance": "1e-9", "output_format": "csv",
        "variable_filter": "^(x1|f1|x2|f2|u|yBoth|yBelow|yAbove|yUnlimited)$",
        "simflags": "", "event_emission": True, "raw_header": HEADER,
    }
    expected_projection_record = {
        "columns": COLUMNS,
        "grouping": "contiguous_equal_f64_bits", "selection": "last", "normalize_times": False,
        "raw_rows": 15, "canonical_rows": 10, "group_sizes": GROUPS,
        "canonical_time_bits": TIME_BITS,
        "canonical_input_bits": {
            "x1": ["c000000000000000"] * 10,
            "f1": ["3ff4000000000000"] * 10,
            "x2": ["4000000000000000"] * 10,
            "f2": ["400a000000000000"] * 10,
            "u": U_BITS,
        },
    }
    expected_semantic_control = {
        "mutation": "yBelow limitAbove false to true", "first_mismatch_row": 8,
        "first_mismatch_time_bits": TIME_BITS[8], "expected_comparison": "exact_mismatch",
        "mismatch_rows": [8, 9],
    }
    expected_regeneration = {
        "entrypoint": "tools/openmodelica-line-reference/line/regenerate.sh",
        "assembly_entrypoint": "tools/openmodelica-line-reference/line/assemble.sh",
        "evidence_workflow": ".github/workflows/openmodelica-line-evidence.yml",
        "network": "none_during_container_execution", "pull": "never",
        "platforms": ["linux/arm64", "linux/amd64"],
        "source_materialization": "git_archive_with_pinned_modelica_export_subst",
        "source_mounts": "read_only", "container_root": "read_only", "container_user": "non_root",
        "capabilities": "none", "no_new_privileges": True, "device_mounts": 0,
        "docker_socket_mounted": False, "timeout_seconds": 120, "cpus": "4",
        "memory_bytes": 2147483648, "memory_swap_bytes": 2147483648, "pids_limit": 256,
        "tmpfs_bytes": 268435456, "per_file_bytes": 67108864,
        "output_directory_bytes": 268435456,
    }
    for field, expected in [
        ("image", expected_image),
        ("sources", expected_sources()),
        ("simulation", expected_simulation),
        ("projection", expected_projection_record),
        ("expected_output_bits", OUTPUT_BITS),
        ("semantic_control", expected_semantic_control),
        ("regeneration", expected_regeneration),
    ]:
        if not type_exact_equal(manifest[field], expected):
            fail(f"unsupported or open {field} record")

    arm, arm_rows = validate_architecture(output / "arm64", root, "arm64")
    amd, amd_rows = validate_architecture(output / "amd64", root, "amd64")
    if arm["repository_revision"] != amd["repository_revision"] or arm["generator_inputs"] != amd["generator_inputs"]:
        fail("native architectures used different generator revisions or inputs")
    shared_toolchain = lambda value: {
        key: item for key, item in value.items() if key not in {"rustc_host", "cargo_host"}
    }
    if shared_toolchain(arm["artifact_toolchain"]) != shared_toolchain(amd["artifact_toolchain"]):
        fail("native architectures used different artifact toolchain releases")
    if arm["source_materialization"] != amd["source_materialization"]:
        fail("native architectures used different source materialization")
    if arm_rows != amd_rows or read_bounded(output / "arm64/line.canonical.csv") != read_bounded(output / "amd64/line.canonical.csv"):
        fail("cross-architecture canonical bytes differ")
    canonical_sha = sha(output / "arm64/line.canonical.csv")
    expected_cross = ["comparison=canonical bytes", f"arm64_sha256={canonical_sha}", f"amd64_sha256={canonical_sha}", "result=PASS"]
    if text(output / "cross-architecture.log").splitlines() != expected_cross:
        fail("cross-architecture record")
    expected_cross_record = {"comparison": "canonical_bytes", "arm64_sha256": canonical_sha, "amd64_sha256": canonical_sha, "result": "pass"}
    if not type_exact_equal(manifest["cross_architecture"], expected_cross_record):
        fail("cross-architecture manifest record")
    expected_architectures = [
        expected_architecture_manifest(output, "arm64", arm),
        expected_architecture_manifest(output, "amd64", amd),
    ]
    if not type_exact_equal(manifest["architectures"], expected_architectures):
        fail("manifest architecture records are open or not bound to native evidence")


def type_exact_equal(actual, expected):
    if type(actual) is not type(expected):
        return False
    if isinstance(expected, dict):
        return set(actual) == set(expected) and all(
            type_exact_equal(actual[key], value) for key, value in expected.items()
        )
    if isinstance(expected, list):
        return len(actual) == len(expected) and all(
            type_exact_equal(left, right) for left, right in zip(actual, expected)
        )
    return actual == expected


def main(arguments):
    if len(arguments) == 2 and arguments[0] == "precopy":
        safe_files.read_closed_directory(arguments[1], ARCH_FILES, MAX_FILE)
        print("Line architecture pre-copy validation passed")
    elif len(arguments) == 3 and arguments[0] == "copy-architecture":
        safe_files.copy_closed_directory(arguments[1], arguments[2], ARCH_FILES, MAX_FILE)
        print("Line architecture bounded copy passed")
    elif len(arguments) == 4 and arguments[0] == "architecture":
        validate_architecture(arguments[1], arguments[2], arguments[3])
        print(f"Line {arguments[3]} architecture evidence verification passed")
    elif len(arguments) == 3 and arguments[0] == "final":
        validate_final(arguments[1], arguments[2])
        print("Line assembled evidence verification passed")
    else:
        print("usage: verify_evidence.py precopy EVIDENCE | copy-architecture SOURCE DESTINATION | architecture EVIDENCE REPOSITORY_ROOT ARCH | final EVIDENCE REPOSITORY_ROOT", file=sys.stderr)
        raise SystemExit(2)


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (KeyError, OSError, ValueError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1)
