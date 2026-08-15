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
import sys
from typing import Any, NoReturn

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


def fail(detail: str) -> NoReturn:
    raise ValueError(f"Line evidence verification failed: {detail}")


def read_bounded(path: pathlib.Path, limit: int = MAX_FILE) -> bytes:
    path = pathlib.Path(path)
    before = path.lstat()
    if path.is_symlink() or not stat.S_ISREG(before.st_mode) or before.st_size > limit:
        fail(f"not a bounded regular file: {path}")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0))
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode) or opened.st_size > limit:
            fail(f"opened input is not a bounded regular file: {path}")
        chunks, total = [], 0
        while True:
            chunk = os.read(descriptor, min(65536, limit + 1 - total))
            if not chunk:
                break
            chunks.append(chunk); total += len(chunk)
            if total > limit:
                fail(f"input exceeded its bound: {path}")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


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


def bits(value):
    return f"{struct.unpack('>Q', struct.pack('>d', value))[0]:016x}"


def safe_directory(path, name):
    path = pathlib.Path(path).absolute()
    current = pathlib.Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        metadata = current.lstat()
        if current.is_symlink():
            fail(f"{name} contains a symlink component")
    if not stat.S_ISDIR(path.lstat().st_mode):
        fail(f"{name} is not a directory")
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


def log_contract(path, architecture, token, model, raw_digest):
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
        "source_materialization": "git_archive_exact_committed_bytes",
        "buildings_package_sha256": "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59",
        "line_source_sha256": "85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5",
        "modelica_package_sha256": "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191",
        "sources_source_sha256": "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3",
        "modelica_services_sha256": "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb",
        "complex_sha256": "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f",
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


def validate_architecture(directory, root, architecture):
    directory, root = safe_directory(directory, "architecture evidence"), safe_directory(root, "repository root")
    if {entry.name for entry in os.scandir(directory)} != ARCH_FILES:
        fail("architecture evidence entries are not closed")
    record = json_file(directory / "architecture.json")
    fields = ["format", "architecture", "platform", "host_architecture", "docker_server_architecture", "container_architecture", "platform_manifest_digest", "config_digest", "raw_run_a_sha256", "raw_run_b_sha256", "flag_control_raw_sha256", "canonical_sha256", "flag_control_canonical_sha256"]
    closed(record, fields, "architecture record")
    expected = ARCH[architecture]
    if [record["format"], record["architecture"], record["platform"], record["host_architecture"], record["docker_server_architecture"], record["container_architecture"], record["platform_manifest_digest"], record["config_digest"]] != ["oce-openmodelica-line-native-architecture-v1", architecture, *expected]:
        fail("architecture literals")
    for name, key in [("line-run-a.raw.csv", "raw_run_a_sha256"), ("line-run-b.raw.csv", "raw_run_b_sha256"), ("flag-control.raw.csv", "flag_control_raw_sha256"), ("line.canonical.csv", "canonical_sha256"), ("flag-control.canonical.csv", "flag_control_canonical_sha256")]:
        if sha(directory / name) != record[key]:
            fail(f"architecture digest binding for {name}")
    if record["raw_run_a_sha256"] != record["raw_run_b_sha256"]:
        fail("native repeat raw digests differ")
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
        log_contract(directory / name, architecture, token, model, digest)
    validate_oci(directory, architecture)
    validate_wrappers(root)
    validate_projection(directory / "projection-mutation.log", record["raw_run_a_sha256"], sha(root / "crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"))
    return record, projected


def valid_relative(path):
    pure = pathlib.PurePosixPath(path)
    return bool(path) and len(path) <= 512 and not pure.is_absolute() and all(part not in ("", ".", "..") and not part.endswith((" ", ".")) for part in pure.parts) and not any(char in path for char in "\\:*?")


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


def validate_final(output, root):
    output, root = safe_directory(output, "assembled evidence"), safe_directory(root, "repository root")
    if {entry.name for entry in os.scandir(output)} != {"manifest.json", "image-index.json", "cross-architecture.log", "arm64", "amd64"}:
        fail("assembled evidence entries are not closed")
    manifest = json_file(output / "manifest.json", MAX_MANIFEST)
    closed(manifest, ["format", "scope", "image", "sources", "simulation", "projection", "expected_output_bits", "architectures", "semantic_control", "cross_architecture", "artifacts", "regeneration"], "manifest")
    if manifest["format"] != "oce-openmodelica-line-external-run-v1":
        fail("unsupported manifest format")
    if manifest["scope"] != {"class": "CDL.Reals.Line", "scenario": "four_limit_modes_five_dyadic_regions", "inputs": ["x1", "f1", "x2", "f2", "u"], "outputs": ["yBoth", "yBelow", "yAbove", "yUnlimited"], "comparison": "exact_finite_f64_bits", "global_tier3_status": "skipped"}:
        fail("unsupported scope")
    artifacts = manifest["artifacts"]
    if not isinstance(artifacts, list) or len(artifacts) != len({item.get("role") for item in artifacts}) or len(artifacts) != len({item.get("path") for item in artifacts}):
        fail("artifact role/path reuse")
    for item in artifacts:
        closed(item, ["role", "path", "sha256"], "artifact")
        if not valid_relative(item["path"]) or len(item["sha256"]) != 64:
            fail("invalid artifact literal")
        if hashlib.sha256(artifact_bytes(output, root, item["path"])).hexdigest() != item["sha256"]:
            fail(f"artifact digest mismatch: {item['path']}")
    arm, arm_rows = validate_architecture(output / "arm64", root, "arm64")
    amd, amd_rows = validate_architecture(output / "amd64", root, "amd64")
    if arm_rows != amd_rows or read_bounded(output / "arm64/line.canonical.csv") != read_bounded(output / "amd64/line.canonical.csv"):
        fail("cross-architecture canonical bytes differ")
    canonical_sha = sha(output / "arm64/line.canonical.csv")
    expected_cross = ["comparison=canonical bytes", f"arm64_sha256={canonical_sha}", f"amd64_sha256={canonical_sha}", "result=PASS"]
    if text(output / "cross-architecture.log").splitlines() != expected_cross:
        fail("cross-architecture record")
    if manifest["expected_output_bits"] != OUTPUT_BITS:
        fail("independent output-bit table")
    if manifest["cross_architecture"] != {"comparison": "canonical_bytes", "arm64_sha256": canonical_sha, "amd64_sha256": canonical_sha, "result": "pass"}:
        fail("cross-architecture manifest record")
    if [item["raw_run_a_sha256"] for item in manifest["architectures"]] != [arm["raw_run_a_sha256"], amd["raw_run_a_sha256"]]:
        fail("manifest architecture records are not bound to evidence")


def main(arguments):
    if len(arguments) == 4 and arguments[0] == "architecture":
        validate_architecture(arguments[1], arguments[2], arguments[3])
        print(f"Line {arguments[3]} architecture evidence verification passed")
    elif len(arguments) == 3 and arguments[0] == "final":
        validate_final(arguments[1], arguments[2])
        print("Line assembled evidence verification passed")
    else:
        print("usage: verify_evidence.py architecture EVIDENCE REPOSITORY_ROOT ARCH | final EVIDENCE REPOSITORY_ROOT", file=sys.stderr)
        raise SystemExit(2)


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (KeyError, OSError, ValueError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1)
