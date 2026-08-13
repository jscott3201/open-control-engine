#!/usr/bin/env python3
"""Validate one freshly assembled Toggle evidence directory."""

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
FIXTURE = "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/"
TOOL = "tools/openmodelica-toggle-reference/"
ROLES = [
    ("canonical_csv", FIXTURE + "toggle.canonical.csv"),
    ("canonicalizer_source", "crates/oce-cxf/tests/open_modelica_toggle_reference/canonicalizer.rs"),
    ("image_index_json", FIXTURE + "image-index.json"),
    ("image_manifest_json", FIXTURE + "image-manifest.json"),
    ("projection_mutation_log", FIXTURE + "projection-mutation.log"),
    ("raw_run_a_csv", FIXTURE + "toggle-run-a.raw.csv"),
    ("raw_run_b_csv", FIXTURE + "toggle-run-b.raw.csv"),
    ("regeneration_script", TOOL + "toggle/regenerate.sh"),
    ("run_a_log", FIXTURE + "run-a.log"),
    ("run_b_log", FIXTURE + "run-b.log"),
    ("runner_script", TOOL + "toggle/runner.sh"),
    ("semantic_control_canonical_csv", FIXTURE + "latch.canonical.csv"),
    ("semantic_control_log", FIXTURE + "latch.log"),
    ("semantic_control_raw_csv", FIXTURE + "latch.raw.csv"),
    ("semantic_control_wrapper_model", TOOL + "toggle/LatchPilot.mo"),
    ("tool_cargo_lock", TOOL + "Cargo.lock"),
    ("tool_cargo_toml", TOOL + "Cargo.toml"),
    ("tool_main_source", TOOL + "src/main.rs"),
    ("evidence_validator_script", TOOL + "toggle/verify_evidence.py"),
    ("manifest_generator_script", TOOL + "toggle/generate_manifest.py"),
    ("deadline_script", TOOL + "toggle/deadline.sh"),
    ("deadline_test_script", TOOL + "toggle/deadline_test.sh"),
    ("output_publish_script", TOOL + "toggle/output_publish.py"),
    ("output_publish_test_script", TOOL + "toggle/output_publish_test.sh"),
    ("container_cleanup_script", TOOL + "toggle/container_cleanup.sh"),
    ("container_cleanup_test_script", TOOL + "toggle/container_cleanup_test.sh"),
    ("wrapper_model", TOOL + "toggle/TogglePilot.mo"),
]
FILES = {
    "manifest.json", "toggle.canonical.csv", "latch.canonical.csv",
    "toggle-run-a.raw.csv", "toggle-run-b.raw.csv", "latch.raw.csv",
    "run-a.log", "run-b.log", "latch.log", "image-index.json",
    "image-manifest.json", "projection-mutation.log",
}
TIME_BITS = [
    "0000000000000000", "403e000000001dff", "404e000000000000", "4056800000000780",
    "405e000000000000", "4062c000000003c1", "4066800000000000", "406a4000000003c1",
    "406e000000000000", "4070e000000001e0", "4072c00000000000", "4073600000000320",
    "4075e000000002d0", "4076800000000000", "40786000000003c1", "407a400000000000",
    "407ae00000000320", "407c2000000003c1", "407e000000000000", "407fe000000003c1",
    "4080e00000000000", "4082c00000000000",
]
GROUPS = [1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 1, 2, 2, 1, 2, 1, 2, 2, 1, 2, 1, 2]
TOGGLE_DIGEST = "cf9333debedf335ae28103b6bcc70229ac78afba4d99701a51d1771d9a3e6641"
LATCH_DIGEST = "2d760f33964d0f6e7de4abc5d236d2dafef9608727b00175c2871a565ef575d2"
INDEX = "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864"
PLATFORM = "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4"
CONFIG = "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666"
SIMULATION = "simulationOptions = \"startTime = 0.0, stopTime = 600.0, numberOfIntervals = 10, tolerance = 1e-9, method = 'dassl', fileNamePrefix = 'TogglePilot', options = '', outputFormat = 'csv', variableFilter = '^(u|clr|y)$', cflags = '', simflags = ''\","
DOCKER = "docker run --pull=never --platform linux/arm64 --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro"
SOURCE_FILES = [
    ("Buildings/package.mo", "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59"),
    ("Buildings/Controls/OBC/CDL/Logical/Toggle.mo", "49d33a88242163def412dff83e3f49e23b187fe56859a01649edbf5fb2bab60c"),
    ("Buildings/Controls/OBC/CDL/Logical/Latch.mo", "2ab7e15a0026d5a6c0089d7af88329869fc3e397398a7b86ad5ea49fd1d9b271"),
    ("Complex.mo", "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f"),
    ("Modelica/package.mo", "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191"),
    ("Modelica/Blocks/Sources.mo", "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3"),
    ("ModelicaServices/package.mo", "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb"),
]


def fail(detail: str) -> NoReturn:
    raise ValueError(f"Toggle evidence verification failed: {detail}")


def read_bounded(path: pathlib.Path, limit: int = MAX_FILE) -> bytes:
    path = pathlib.Path(path)
    if limit <= 0 or limit > MAX_FILE:
        fail("invalid file-size bound")
    try:
        before = path.lstat()
    except OSError as error:
        fail(f"cannot stat {path}: {error}")
    if not stat.S_ISREG(before.st_mode) or path.is_symlink():
        fail(f"not a regular non-symlink file: {path}")
    if before.st_size > limit:
        fail(f"file exceeds {limit} bytes: {path}")
    flags = os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        fail(f"cannot safely open {path}: {error}")
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode):
            fail(f"opened file is not regular: {path}")
        if opened.st_size > limit:
            fail(f"opened file exceeds {limit} bytes: {path}")
        chunks, total = [], 0
        while True:
            chunk = os.read(descriptor, min(65536, limit + 1 - total))
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > limit:
                fail(f"file exceeds {limit} bytes while reading: {path}")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def text(path: pathlib.Path, limit: int = MAX_FILE, encoding: str = "utf-8") -> str:
    try:
        return read_bounded(path, limit).decode(encoding)
    except UnicodeError as error:
        fail(f"invalid {encoding} in {path}: {error}")


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(read_bounded(path)).hexdigest()


def closed(value, keys, name):
    if not isinstance(value, dict) or set(value) != set(keys):
        fail(f"{name} fields are not closed")


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


def bits(value):
    return f"{struct.unpack('>Q', struct.pack('>d', value))[0]:016x}"


def artifact_bytes(output, root, relative):
    base, suffix = (output, relative.removeprefix(FIXTURE)) if relative.startswith(FIXTURE) else (root, relative)
    components = pathlib.PurePosixPath(suffix).parts
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_DIRECTORY", 0)
    directory = os.open(base, flags)
    try:
        for component in components[:-1]:
            try:
                following = os.open(component, flags, dir_fd=directory)
            except OSError as error:
                fail(f"artifact ancestor cannot be opened without following links: {relative}: {error}")
            os.close(directory)
            directory = following
        file_flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
        try:
            descriptor = os.open(components[-1], file_flags, dir_fd=directory)
        except OSError as error:
            fail(f"artifact cannot be opened without following links: {relative}: {error}")
        try:
            opened = os.fstat(descriptor)
            if not stat.S_ISREG(opened.st_mode) or opened.st_size > MAX_FILE:
                fail(f"artifact is not a bounded regular file: {relative}")
            chunks, total = [], 0
            while True:
                chunk = os.read(descriptor, min(65536, MAX_FILE + 1 - total))
                if not chunk: break
                chunks.append(chunk); total += len(chunk)
                if total > MAX_FILE: fail(f"artifact exceeds bound: {relative}")
            return b"".join(chunks)
        finally: os.close(descriptor)
    finally: os.close(directory)


def safe_directory(path, name):
    path = pathlib.Path(path).absolute()
    current = pathlib.Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        try:
            metadata = current.lstat()
        except OSError as error:
            fail(f"cannot stat {name} component {current}: {error}")
        if current.is_symlink():
            fail(f"{name} contains symlink component: {current}")
    if not stat.S_ISDIR(path.lstat().st_mode): fail(f"{name} must be a directory")
    return path


def parse_raw(path, expected_digest):
    raw = read_bounded(path)
    if hashlib.sha256(raw).hexdigest() != expected_digest:
        fail(f"unexpected raw digest for {path.name}")
    body = raw.decode("ascii")
    if "\r" in body or not body.endswith("\n"):
        fail(f"invalid raw line endings for {path.name}")
    lines = body.splitlines()
    if not lines or lines[0] != '"time","clr","u","y"':
        fail(f"unexpected raw header for {path.name}")
    try:
        csv_rows = list(csv.reader(io.StringIO(body), strict=True))
    except csv.Error as error:
        fail(f"invalid raw CSV for {path.name}: {error}")
    if csv_rows[0] != ["time", "clr", "u", "y"] or len(csv_rows) != 35:
        fail(f"unexpected raw shape for {path.name}")
    rows = []
    for index, (line, row) in enumerate(zip(lines[1:], csv_rows[1:])):
        lexical = line.split(",")
        if len(row) != 4 or row != lexical or lexical[1:] not in (["0", "0", "0"], ["0", "0", "1"], ["0", "1", "0"], ["0", "1", "1"], ["1", "0", "0"], ["1", "0", "1"], ["1", "1", "0"], ["1", "1", "1"]):
            fail(f"raw row {index} has invalid quoting, width, or Boolean cells")
        try:
            time = float(lexical[0])
        except ValueError:
            fail(f"raw row {index} time is invalid")
        if not math.isfinite(time):
            fail(f"raw row {index} time is non-finite")
        rows.append((time, lexical[2] == "1", lexical[1] == "1", lexical[3] == "1"))
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


def parse_canonical(path, table):
    lines = text(path, encoding="ascii").splitlines()
    if lines[:3] != ["#1", "# columns: time u clr y", f"double {table}(22,4)"] or len(lines) != 25:
        fail(f"unexpected canonical header or shape for {path.name}")
    rows = []
    for index, line in enumerate(lines[3:]):
        cells = line.split(" ")
        if len(cells) != 4 or cells[1:] not in (["0.0", "0.0", "0.0"], ["0.0", "0.0", "1.0"], ["0.0", "1.0", "0.0"], ["0.0", "1.0", "1.0"], ["1.0", "0.0", "0.0"], ["1.0", "0.0", "1.0"], ["1.0", "1.0", "0.0"], ["1.0", "1.0", "1.0"]):
            fail(f"canonical row {index} is not exact Boolean four-column data")
        try:
            time = float(cells[0])
        except ValueError:
            fail(f"canonical row {index} time is invalid")
        rows.append((bits(time), cells[1] == "1.0", cells[2] == "1.0", cells[3] == "1.0"))
    return rows


def expected_projection(groups, keep_last):
    return [(group[0], row[1], row[2], row[3]) for group in groups for row in [group[1][-1 if keep_last else 0]]]


def exact_wrapper(class_name):
    return f"""model TogglePilot
  Modelica.Blocks.Sources.BooleanTable uSource(
    table={{30,90,150,210,270,390,450,510}},
    startValue=true);
  Modelica.Blocks.Sources.BooleanTable clrSource(
    table={{310,350,390,430}},
    startValue=false);
  Buildings.Controls.OBC.CDL.Logical.{class_name} dut;

  output Boolean u;
  output Boolean clr;
  output Boolean y;
equation
  connect(uSource.y, dut.u);
  connect(clrSource.y, dut.clr);
  u = uSource.y;
  clr = clrSource.y;
  y = dut.y;
end TogglePilot;
"""


def validate_manifest(manifest):
    closed(manifest, ["format", "scope", "image", "sources", "simulation", "projection", "runs", "semantic_control", "artifacts", "regeneration"], "manifest")
    if manifest["format"] != "oce-openmodelica-toggle-external-run-v1": fail("unsupported manifest format")
    scope = {"class": "CDL.Logical.Toggle", "scenario": "repeated_rises_initial_true_and_clear_priority", "inputs": ["u", "clr"], "output": "y", "comparison": "exact_boolean", "global_tier3_status": "skipped"}
    if manifest["scope"] != scope: fail("unsupported scope")
    image = {"repository": "openmodelica/openmodelica", "tag": "v1.25.1-minimal", "index_digest": INDEX, "platform_manifest_digest": PLATFORM, "config_digest": CONFIG, "platform": "linux/arm64", "host_architecture": "arm64", "docker_server_architecture": "aarch64", "omc_version": "OpenModelica 1.25.1", "gcc_version": "11.4.0", "binutils_version": "2.38", "glibc_version": "2.35"}
    if manifest["image"] != image: fail("unsupported image literals")
    expected_sources = [
        {"name": "buildings", "repository": "https://github.com/lbl-srg/modelica-buildings.git", "commit": "a131864e4c4df22ebcd52bb8da439de0087ac365", "tree": "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09", "package": "Buildings", "version": "14.0.0", "files": [{"path": p, "sha256": d} for p, d in SOURCE_FILES[:3]]},
        {"name": "modelica", "repository": "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git", "commit": "7a4bf7de77a3986e8eb1e88cbb515d646f78f834", "tree": "43d7d8fc1a991358e9e5e91976e27cdc4280173f", "package": "Modelica", "version": "4.1.0", "files": [{"path": p, "sha256": d} for p, d in SOURCE_FILES[3:]]},
    ]
    if manifest["sources"] != expected_sources: fail("unsupported source identities")
    simulation = {"method": "dassl", "start_time": "0", "stop_time": "600", "number_of_intervals": 10, "tolerance": "1e-9", "output_format": "csv", "variable_filter": "^(u|clr|y)$", "simflags": "", "event_emission": True, "raw_header": '"time","clr","u","y"'}
    if manifest["simulation"] != simulation: fail("unsupported simulation literals")
    projection = {"columns": ["time", "u", "clr", "y"], "grouping": "contiguous_equal_f64_bits", "selection": "last", "normalize_times": False, "raw_rows": 34, "canonical_rows": 22, "group_sizes": GROUPS, "canonical_time_bits": TIME_BITS}
    if manifest["projection"] != projection: fail("unsupported projection literals")
    for value, keys, name in [(manifest["image"], image, "image"), (manifest["simulation"], simulation, "simulation"), (manifest["projection"], projection, "projection")]: closed(value, keys, name)
    for source in manifest["sources"]:
        closed(source, ["name", "repository", "commit", "tree", "package", "version", "files"], "source")
        for source_file in source["files"]: closed(source_file, ["path", "sha256"], "source file")
    if len(manifest["runs"]) != 2: fail("run count")
    for run, expected in zip(manifest["runs"], [("run-a", "fresh-run-a", "run-a.log", "toggle-run-a.raw.csv"), ("run-b", "fresh-run-b", "run-b.log", "toggle-run-b.raw.csv")]):
        closed(run, ["id", "output_directory_token", "log_path", "raw_path", "log_sha256", "raw_sha256"], "run")
        if (run["id"], run["output_directory_token"], run["log_path"], run["raw_path"], run["raw_sha256"]) != (expected[0], expected[1], FIXTURE + expected[2], FIXTURE + expected[3], TOGGLE_DIGEST): fail("run literals")
    semantic = {"substitution_class": "CDL.Logical.Latch", "first_mismatch_row": 3, "first_mismatch_time_bits": TIME_BITS[3], "raw_rows": 34, "canonical_rows": 22, "expected_comparison": "exact_mismatch"}
    if manifest["semantic_control"] != semantic: fail("semantic control literals")
    closed(manifest["semantic_control"], semantic, "semantic control")
    regeneration = {"entrypoint": TOOL + "toggle/regenerate.sh", "network": "none", "pull": "never", "platform": "linux/arm64", "source_materialization": "git_archive", "source_mounts": "read_only", "container_root": "read_only", "container_user": "non_root", "capabilities": "none", "no_new_privileges": True, "device_mounts": 0, "docker_socket_mounted": False, "timeout_seconds": 120, "cpus": "4", "memory_bytes": 2147483648, "memory_swap_bytes": 2147483648, "pids_limit": 256, "tmpfs_bytes": 268435456, "per_file_bytes": 67108864, "output_directory_bytes": 268435456}
    if manifest["regeneration"] != regeneration: fail("regeneration literals")
    closed(manifest["regeneration"], regeneration, "regeneration")
    artifacts = manifest["artifacts"]
    if [(item.get("role"), item.get("path")) for item in artifacts] != ROLES: fail("artifact roles or paths")
    if len({item["role"] for item in artifacts}) != len(ROLES) or len({item["path"] for item in artifacts}) != len(ROLES): fail("artifact role/path reuse")
    for item in artifacts: closed(item, ["role", "path", "sha256"], "artifact")


def log_contract(path, manifest, token, model, raw_digest):
    body = text(path)
    buildings, modelica = manifest["sources"]
    source = dict(SOURCE_FILES)
    required = {
        "host_architecture": "arm64", "docker_server_architecture": "aarch64",
        "image_index_digest": INDEX, "image_platform_manifest_digest": PLATFORM,
        "image_config_digest": CONFIG, "oci_metadata_validation": "raw_digest_and_pinned_graph",
        "pull_policy": "never", "host_timeout_seconds": "120", "buildings_remote": buildings["repository"],
        "buildings_commit": buildings["commit"], "buildings_tree": buildings["tree"],
        "modelica_remote": modelica["repository"], "modelica_commit": modelica["commit"],
        "modelica_tree": modelica["tree"], "source_materialization": "git_archive_exact_committed_bytes",
        "buildings_package_sha256": source["Buildings/package.mo"], "toggle_source_sha256": source["Buildings/Controls/OBC/CDL/Logical/Toggle.mo"],
        "latch_source_sha256": source["Buildings/Controls/OBC/CDL/Logical/Latch.mo"], "modelica_package_sha256": source["Modelica/package.mo"],
        "boolean_table_source_sha256": source["Modelica/Blocks/Sources.mo"], "modelica_services_sha256": source["ModelicaServices/package.mo"],
        "complex_sha256": source["Complex.mo"], "docker_command": DOCKER, "output_directory_token": token,
        "selected_model": model, "container_architecture": "aarch64", "modelica_path": "",
        "root_write_probe": "read-only", "source_write_probe": "read-only", "network_route_lines": "1",
        "cgroup_memory_max": "2147483648", "cgroup_pids_max": "256", "cgroup_cpu_max": "400000 100000",
        "per_file_limit_bytes": "67108864", "output_directory_limit_bytes": "268435456",
        "gcc_version": "11.4.0", "binutils_version": "2.38", "glibc_version": "2.35",
        "omc_version": "OpenModelica 1.25.1", "toggle_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Logical/Toggle.mo",
        "latch_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Logical/Latch.mo", "boolean_table_source": "/sources/modelica/Modelica/Blocks/Sources.mo",
        "Modelica": "4.1.0", "Buildings": "14.0.0", "raw_sha256": raw_digest, "runner_complete": "1",
    }
    lines = body.splitlines()
    for key, value in required.items():
        if [line for line in lines if line.startswith(key + "=")] != [key + "=" + value]: fail(f"{path.name} invalid {key}")
    identities = [line.removeprefix("container_identity=") for line in lines if line.startswith("container_identity=")]
    if len(identities) != 1 or not identities[0].replace(":", "", 1).isdigit() or identities[0].split(":")[0] == "0": fail(f"{path.name} container identity")
    sizes = [line.removeprefix("output_directory_kib=") for line in lines if line.startswith("output_directory_kib=")]
    if len(sizes) != 1 or not sizes[0].isdigit() or int(sizes[0]) * 1024 > 268435456: fail(f"{path.name} output size")
    if [line.strip() for line in lines].count(SIMULATION) != 1: fail(f"{path.name} simulation options")
    for success in ["The initialization finished successfully", "The simulation finished successfully"]:
        if body.count(success) != 1: fail(f"{path.name} success record")


def projection_contract(path, groups, canonicalizer_digest):
    keep_first = expected_projection(groups, False)
    keep_last = expected_projection(groups, True)
    mismatches = [index for index, (first, last) in enumerate(zip(keep_first, keep_last)) if first[1:3] != last[1:3]]
    expected = [
        "projection_mutation=contiguous equal-time selection changed from last to first",
        "working_tree_modified=false", "mutated_compile=PASS", "mutated_input=toggle-run-a.raw.csv",
        f"mutated_input_sha256={TOGGLE_DIGEST}", "mutated_raw_rows=34", "mutated_canonical_rows=22",
        "mutated_group_sizes=" + ",".join(map(str, GROUPS)), "mutated_canonical_time_bits=" + ",".join(TIME_BITS),
        "mutated_schedule_result=FAIL", "mutated_schedule_mismatch_rows=" + ",".join(map(str, mismatches)),
        "mutated_schedule_first_mismatch_row=1", f"mutated_schedule_first_mismatch_time_bits={TIME_BITS[1]}",
        "mutated_grouping_result=PASS", "mutated_timestamp_bits_result=PASS", "restoration_result=PASS",
        f"restored_canonicalizer_sha256={canonicalizer_digest}",
    ]
    if text(path, encoding="ascii").splitlines() != expected: fail("projection mutation record")


def validate(output, root):
    output = safe_directory(output, "evidence directory")
    root = safe_directory(root, "repository root")
    if {entry.name for entry in os.scandir(output)} != FILES: fail("evidence directory entries")
    manifest = json_file(output / "manifest.json", MAX_MANIFEST)
    validate_manifest(manifest)
    artifacts = {item["role"]: item for item in manifest["artifacts"]}
    for item in manifest["artifacts"]:
        if hashlib.sha256(artifact_bytes(output, root, item["path"])).hexdigest() != item["sha256"]: fail(f"artifact digest {item['role']}")
    toggle_rows, toggle_groups = parse_raw(output / "toggle-run-a.raw.csv", TOGGLE_DIGEST)
    second_rows, _ = parse_raw(output / "toggle-run-b.raw.csv", TOGGLE_DIGEST)
    latch_rows, latch_groups = parse_raw(output / "latch.raw.csv", LATCH_DIGEST)
    if toggle_rows != second_rows: fail("repeat raw byte/row equality")
    toggle_expected, latch_expected = expected_projection(toggle_groups, True), expected_projection(latch_groups, True)
    if parse_canonical(output / "toggle.canonical.csv", "openmodelica_logical_toggle") != toggle_expected: fail("Toggle canonical is not raw keep-last projection")
    if parse_canonical(output / "latch.canonical.csv", "openmodelica_logical_latch") != latch_expected: fail("Latch canonical is not raw keep-last projection")
    differences = [i for i, (toggle, latch) in enumerate(zip(toggle_expected, latch_expected)) if toggle[3] != latch[3]]
    if not differences or differences[0] != 3 or toggle_expected[3][0] != TIME_BITS[3]: fail("Latch first mismatch")
    projection_contract(output / "projection-mutation.log", toggle_groups, artifacts["canonicalizer_source"]["sha256"])
    for run, token, name in zip(manifest["runs"], ["fresh-run-a", "fresh-run-b"], ["run-a.log", "run-b.log"]):
        if run["log_sha256"] != digest(output / name) or run["raw_sha256"] != TOGGLE_DIGEST: fail("run digest binding")
        log_contract(output / name, manifest, token, "Toggle", TOGGLE_DIGEST)
    log_contract(output / "latch.log", manifest, "fresh-semantic-control", "Latch", LATCH_DIGEST)
    toggle_wrapper = text(root / TOOL / "toggle/TogglePilot.mo", encoding="ascii")
    latch_wrapper = text(root / TOOL / "toggle/LatchPilot.mo", encoding="ascii")
    if toggle_wrapper != exact_wrapper("Toggle") or latch_wrapper != exact_wrapper("Latch") or toggle_wrapper.replace("CDL.Logical.Toggle", "CDL.Logical.Latch") != latch_wrapper: fail("wrapper source or substitution")
    index, image = json_file(output / "image-index.json"), json_file(output / "image-manifest.json")
    if "sha256:" + digest(output / "image-index.json") != INDEX or "sha256:" + digest(output / "image-manifest.json") != PLATFORM: fail("OCI metadata digests")
    arm = [item for item in index.get("manifests", []) if item.get("platform") == {"architecture": "arm64", "os": "linux"}]
    if len(arm) != 1 or arm[0].get("digest") != PLATFORM or image.get("config", {}).get("digest") != CONFIG: fail("OCI graph")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("usage: verify_evidence.py EVIDENCE_DIRECTORY REPOSITORY_ROOT", file=sys.stderr)
        raise SystemExit(2)
    try:
        validate(sys.argv[1], sys.argv[2])
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1)
    print("Toggle evidence verification passed")
