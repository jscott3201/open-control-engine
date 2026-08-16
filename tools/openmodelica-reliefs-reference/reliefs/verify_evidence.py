#!/usr/bin/env python3
"""Validate one native or assembled Reliefs evidence directory."""

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

import projection_evidence
import safe_files

MAX_FILE = 1024 * 1024
MAX_MANIFEST = 256 * 1024
INDEX = "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864"
ARM_RAW = "0ade646840638ef01778856188c8aa411f41ac6d8b5316d82aa0b96f617a2046"
ARM_CLAMP_RAW = "012a8a7040aa869ff99292216f7ad62fe92dc63dc02de4e43a45f727bf2c4a90"
HEADER = '"time","uOutDam_max","uOutDam_min","uRetDam_max","uRetDam_min","uTSup","yOutDam","yRetDam"'
COLUMNS = ["time", "uTSup", "uOutDam_min", "uOutDam_max", "uRetDam_min", "uRetDam_max", "yOutDam", "yRetDam"]
TIME_BITS = projection_evidence.KEEP_LAST_TIMES
RAW_TIME_BITS = projection_evidence.RAW_TIME_BITS
GROUPS = projection_evidence.GROUP_SIZES
SOURCE_ROWS = projection_evidence.KEEP_LAST_SOURCES
U_T_SUP_BITS = projection_evidence.U_T_SUP_BITS
CONSTANT_INPUT_BITS = projection_evidence.CONSTANT_INPUT_BITS
Y_OUT = "3fd0000000000000 3fd0000000000000 3fe2000000000000 3fec000000000000 3fec000000000000 3fec000000000000 3fec000000000000".split()
Y_RET = "3fe8000000000000 3fe8000000000000 3fe8000000000000 3fe8000000000000 3fdc000000000000 3fc0000000000000 3fc0000000000000".split()
ARCH = {
    "arm64": ("linux/arm64", "arm64", "aarch64", "aarch64", "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4", "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666"),
    "amd64": ("linux/amd64", "amd64", "x86_64", "x86_64", "sha256:92d0779a01e7d43ed4d5ecb4cfd9754cb259b30673ddb454b5a32e3eb8665f11", "sha256:0c81120bb392de44cab0e9ff6818d0a44afad657d5b401f25e148fa6c26e5347"),
}
ARCH_FILES = {
    "architecture.json", "reliefs.canonical.csv", "reliefs-run-a.raw.csv", "reliefs-run-b.raw.csv",
    "run-a.log", "run-b.log", "parameter-control.canonical.csv", "parameter-control.raw.csv",
    "parameter-control.log", "final-clamp.canonical.csv", "final-clamp.raw.csv", "final-clamp.log",
    "projection-mutation.log", "projection-keep-first.canonical.csv", "projection-keep-first.metadata",
    "image-index.json", "image-manifest.json",
}
GENERATOR_INPUT_PATHS = {
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


def fail(detail: str) -> NoReturn:
    raise ValueError(f"Reliefs evidence verification failed: {detail}")


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


def bits(value):
    return f"{struct.unpack('>Q', struct.pack('>d', value))[0]:016x}"


def safe_directory(path, name):
    path = pathlib.Path(path).absolute()
    try:
        descriptor = safe_files.open_directory(path); os.close(descriptor)
    except (OSError, ValueError) as error:
        fail(f"invalid {name}: {error}")
    return path


def source_files():
    buildings = [
        ("Buildings/package.mo", "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59"),
        ("Buildings/Controls/package.mo", "17f0bba8aa51f7051fa43d5cac6dcef1f33ca8f811fd6a6474bd3ed1263f61cd"),
        ("Buildings/Controls/OBC/package.mo", "a86253df85e5531235ccb81ece569eedac973d8c4eae52be912877e7bd0d321c"),
        ("Buildings/Controls/OBC/ASHRAE/package.mo", "88b99ba4667c09e5a23c5ac21c88fe18e39af67c22cc2efc6dbab26db09e8e6b"),
        ("Buildings/Controls/OBC/ASHRAE/G36/package.mo", "ae1fe5bfca73fd59ad4253aaea5e8c927ce1e1824cdce9790db3a24a20853881"),
        ("Buildings/Controls/OBC/ASHRAE/G36/AHUs/package.mo", "266b09bcb8a3266467c6728ee7a5d9872cdf3dad405af91bac14a697320176a2"),
        ("Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/package.mo", "de4908f31fb15838b54dc41473b82059201ace000c2615ded47a1071dd718560"),
        ("Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/package.mo", "290c0e49356bc000364b644cac4baf353fc4d4a4ed5c77cb5e1145cdf3ab56e7"),
        ("Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/package.mo", "41ee31e3ed5ec6fd88a46447b73a5d5c55cd3cce06a899c25df3aadcba5b3b3b"),
        ("Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/package.mo", "adebe030dcdd18a8777558b18e56084ed19546c375f678d083987a4480952216"),
        ("Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/package.mo", "0e2f3d3129ed06fc93655e75fda3597bba6f17f924117bb4b47c5dca7f3c3508"),
        ("Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo", "177fd5f2802bfd29072bc221756dd8846cd05b552f8fdf368a2c87a56593cb41"),
        ("Buildings/Controls/OBC/CDL/package.mo", "3ceda191a859e2513c4d3df322bec753ed8df406968cf4354c5488f4dcd79256"),
        ("Buildings/Controls/OBC/CDL/Interfaces/package.mo", "a4b3a6831deb68e8209435e2b0f0067d227e3bff2be76845bd2f3690f13c82e4"),
        ("Buildings/Controls/OBC/CDL/Interfaces/RealInput.mo", "0f4afeda8d50035b722a79e6d6b48c86034facd3adcfc7f95e2b15cbd1ddc87a"),
        ("Buildings/Controls/OBC/CDL/Interfaces/RealOutput.mo", "ba27a80bc46bf8b9550655b54a93679f5322b33786cc220daa59f7d39243d98f"),
        ("Buildings/Controls/OBC/CDL/Reals/package.mo", "3b9a58569701c9f7d44347d6304aeb60cea28902332fb16acc15e0fd61e19a8a"),
        ("Buildings/Controls/OBC/CDL/Reals/Line.mo", "85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5"),
        ("Buildings/Controls/OBC/CDL/Reals/Min.mo", "e5dcf1e50d752365d05e44bc54eb743c116b87de240c1253c2126fcdbcbcbb04"),
        ("Buildings/Controls/OBC/CDL/Reals/Max.mo", "499e5162b21fa776c61065a46c4ba5d646ed887b227adaae93214d97750efca1"),
        ("Buildings/Controls/OBC/CDL/Reals/Sources/package.mo", "373e79eb61b6ace1527a93253ad0a3dfeb520dab0a1b6644dd4b0dd7419c9b20"),
        ("Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo", "f3a131c5c6eb372ea48dec67ed5eb075eef1a485901143a338c4361511eed05e"),
    ]
    modelica = [
        ("Complex.mo", "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f"),
        ("Modelica/package.mo", "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191"),
        ("Modelica/Blocks/Sources.mo", "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3"),
        ("ModelicaServices/package.mo", "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb"),
    ]
    return [
        {"source": source, "path": path, "committed_sha256": digest, "materialized_sha256": digest}
        for source, rows in [("buildings", buildings), ("modelica", modelica)] for path, digest in rows
    ]


def expected_toolchain(architecture):
    host = "aarch64-unknown-linux-gnu" if architecture == "arm64" else "x86_64-unknown-linux-gnu"
    return {"rustc_release": "1.97.1", "rustc_commit_hash": "8bab26f4f68e0e26f0bb7960be334d5b520ea452", "rustc_commit_date": "2026-07-14", "rustc_host": host, "rustc_llvm_version": "22.1.6", "cargo_release": "1.97.1", "cargo_commit_hash": "c980f4866141969fab6254a680546a277789d6f0", "cargo_commit_date": "2026-06-30", "cargo_host": host, "python_version": "Python 3.13.7"}


def expected_materialization():
    return {"source_materialization": "git_archive_with_pinned_modelica_export_subst", "buildings_materialization": "git_archive_without_local_attribute_override", "modelica_transform_path": "Modelica/package.mo", "modelica_transform_rule": "Modelica/package.mo -export-subst", "modelica_package_committed_sha256": "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191", "modelica_package_materialized_sha256": "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191"}


def parse_raw(path, expected_digest=None):
    raw = read_bounded(path)
    if expected_digest and hashlib.sha256(raw).hexdigest() != expected_digest:
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
    if parsed[0] != ["time", "uOutDam_max", "uOutDam_min", "uRetDam_max", "uRetDam_min", "uTSup", "yOutDam", "yRetDam"] or len(parsed) != 22:
        fail(f"unexpected raw shape for {path.name}")
    rows = []
    for index, (line, row) in enumerate(zip(lines[1:], parsed[1:])):
        if row != line.split(",") or len(row) != 8 or any(not cell or len(cell) > 128 for cell in row):
            fail(f"raw row {index} has invalid quoting, width, or cell bound")
        try:
            values = [float(cell) for cell in row]
        except ValueError:
            fail(f"raw row {index} has a non-numeric cell")
        if not all(math.isfinite(value) for value in values):
            fail(f"raw row {index} has a non-finite cell")
        rows.append((values[0], values[5], values[2], values[1], values[4], values[3], values[6], values[7], index))
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
    if [len(group[1]) for group in groups] != GROUPS or [bits(row[0]) for row in rows] != RAW_TIME_BITS:
        fail(f"raw groups or timestamp bits drifted for {path.name}")
    return rows, groups


def project(groups, keep_last=True):
    return projection_evidence.selected(groups, keep_last)


def parse_canonical(path, table):
    body = read_bounded(path)
    if b"\r" in body or not body.endswith(b"\n"):
        fail(f"invalid canonical line endings for {path.name}")
    lines = body.decode("ascii").splitlines()
    if lines[:3] != ["#1", "# columns: " + " ".join(COLUMNS), f"double {table}(7,8)"] or len(lines) != 10:
        fail(f"unexpected canonical header or shape for {path.name}")
    rows = []
    for index, line in enumerate(lines[3:]):
        cells = line.split(" ")
        if len(cells) != 8 or any(not cell or len(cell) > 128 for cell in cells):
            fail(f"canonical row {index} width or cell bound")
        try:
            values = [float(cell) for cell in cells]
        except ValueError:
            fail(f"canonical row {index} has an invalid cell")
        if not all(math.isfinite(value) for value in values):
            fail(f"canonical row {index} has a non-finite cell")
        rows.append([bits(value) for value in values])
    return rows


def expected_rows():
    return [[TIME_BITS[index], U_T_SUP_BITS[index], *CONSTANT_INPUT_BITS, Y_OUT[index], Y_RET[index]] for index in range(7)]


def one_log_value(body, key, expected):
    values = [line.removeprefix(key + "=") for line in body.splitlines() if line.startswith(key + "=")]
    if values != [expected]:
        fail(f"log must contain exactly one {key}={expected}; found {values}")


def log_contract(path, architecture, token, model, raw_digest, revision, generator_inputs, source_cone):
    body = text(path); platform, host, server, container, manifest, config = ARCH[architecture]
    required = {
        "host_architecture": host, "docker_server_architecture": server, "image_index_digest": INDEX,
        "image_platform_manifest_digest": manifest, "image_config_digest": config,
        "oci_metadata_validation": "raw_digest_and_pinned_graph", "pull_policy": "never", "host_timeout_seconds": "120",
        "buildings_remote": "https://github.com/lbl-srg/modelica-buildings.git", "buildings_commit": "a131864e4c4df22ebcd52bb8da439de0087ac365", "buildings_tree": "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09",
        "modelica_remote": "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git", "modelica_commit": "7a4bf7de77a3986e8eb1e88cbb515d646f78f834", "modelica_tree": "43d7d8fc1a991358e9e5e91976e27cdc4280173f",
        "repository_revision": revision, "generator_provenance_scope": "native_generation_and_publication",
        "source_files_json": json.dumps(source_cone, separators=(",", ":")), "output_directory_token": token,
        "selected_model": model, "container_architecture": container, "modelica_path": "", "events_enabled": "default_true", "simflags": "empty",
        "root_write_probe": "read-only", "source_write_probe": "read-only", "reference_write_probe": "read-only",
        "network_route_lines": "1", "cgroup_memory_max": "2147483648", "cgroup_pids_max": "256", "cgroup_cpu_max": "400000 100000",
        "per_file_limit_bytes": "67108864", "output_directory_limit_bytes": "268435456", "gcc_version": "11.4.0", "binutils_version": "2.38", "glibc_version": "2.35", "omc_version": "OpenModelica 1.25.1",
        "reliefs_source": "/sources/buildings/Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo",
        "line_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Line.mo", "min_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Min.mo", "max_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Max.mo", "cdl_constant_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo", "real_input_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Interfaces/RealInput.mo", "real_output_source": "/sources/buildings/Buildings/Controls/OBC/CDL/Interfaces/RealOutput.mo", "msl_constant_source": "/sources/modelica/Modelica/Blocks/Sources.mo", "time_table_source": "/sources/modelica/Modelica/Blocks/Sources.mo",
        "Modelica": "4.1.0", "Buildings": "14.0.0", "omc_warning_count": "0", "raw_sha256": raw_digest, "runner_complete": "1",
    }
    required.update(generator_inputs); required.update(expected_toolchain(architecture)); required.update(expected_materialization())
    required["docker_command"] = f"docker run --pull=never --platform {platform} --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro"
    for key, expected in required.items():
        one_log_value(body, key, expected)
    peak = [line.removeprefix("observed_cgroup_peak_bytes=") for line in body.splitlines() if line.startswith("observed_cgroup_peak_bytes=")]
    if len(peak) != 1 or not peak[0].isdigit() or not 0 < int(peak[0]) <= 2147483648:
        fail(f"{path.name} cgroup memory peak")
    identities = [line.removeprefix("container_identity=") for line in body.splitlines() if line.startswith("container_identity=")]
    if len(identities) != 1 or not identities[0].replace(":", "", 1).isdigit() or identities[0].split(":")[0] == "0":
        fail(f"{path.name} container identity")
    size = [line.removeprefix("output_directory_kib=") for line in body.splitlines() if line.startswith("output_directory_kib=")]
    if len(size) != 1 or not size[0].isdigit() or int(size[0]) * 1024 > 268435456:
        fail(f"{path.name} output size")
    simulation = "simulationOptions = \"startTime = 0.0, stopTime = 420.0, numberOfIntervals = 7, tolerance = 1e-9, method = 'dassl', fileNamePrefix = 'ReliefsPilot', options = '', outputFormat = 'csv', variableFilter = '^(uTSup|uOutDam_min|uOutDam_max|uRetDam_min|uRetDam_max|yOutDam|yRetDam)$', cflags = '', simflags = ''\","
    if [line.strip() for line in body.splitlines()].count(simulation) != 1:
        fail(f"{path.name} simulation options")
    if any("warning" in line.lower() and line != "omc_warning_count=0" for line in body.splitlines()):
        fail(f"{path.name} contains an OpenModelica warning")


def validate_wrappers(root):
    main = text(root / GENERATOR_INPUT_PATHS["reliefs_pilot_sha256"])
    parameter = text(root / GENERATOR_INPUT_PATHS["parameter_pilot_sha256"])
    clamp = text(root / GENERATOR_INPUT_PATHS["clamp_pilot_sha256"])
    if main.replace("uOutDamMax=0.0", "uOutDamMax=-0.125") != parameter:
        fail("parameter wrapper is not the one-token external mutation")
    expected_clamp = main.replace("uOutDamMinSource(k=0.25)", "uOutDamMinSource(k=0.875)").replace("uOutDamMaxSource(k=0.875)", "uOutDamMaxSource(k=0.25)").replace("uRetDamMinSource(k=0.125)", "uRetDamMinSource(k=0.75)").replace("uRetDamMaxSource(k=0.75)", "uRetDamMaxSource(k=0.125)")
    if clamp != expected_clamp:
        fail("final-clamp wrapper changes more than the four authored inputs")
    class_name = "Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Modulations.Reliefs"
    if main.count(class_name) != 1:
        fail("wrapper must contain exactly one upstream Reliefs instance")
    for wrapper in [main, parameter, clamp]:
        for forbidden in ["expected", "assert(", "FMU", "FMI", "oce-"]:
            if forbidden in wrapper:
                fail(f"wrapper contains forbidden token {forbidden}")


def validate_oci(directory, architecture):
    index_path, manifest_path = directory / "image-index.json", directory / "image-manifest.json"
    if "sha256:" + sha(index_path) != INDEX or "sha256:" + sha(manifest_path) != ARCH[architecture][4]:
        fail("OCI metadata digest")
    index, manifest = json_file(index_path), json_file(manifest_path)
    selected = [item for item in index.get("manifests", []) if item.get("platform") == {"architecture": architecture, "os": "linux"}]
    if len(selected) != 1 or selected[0].get("digest") != ARCH[architecture][4] or manifest.get("config", {}).get("digest") != ARCH[architecture][5]:
        fail("OCI graph")


def artifact_bytes(output, root, relative):
    fixture = "crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs/"
    base, suffix = (output, relative.removeprefix(fixture)) if relative.startswith(fixture) else (root, relative)
    pure = pathlib.PurePosixPath(suffix)
    if not suffix or pure.is_absolute() or any(part in ("", ".", "..") for part in pure.parts):
        fail("invalid repository-relative artifact path")
    directory = os.open(base, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0))
    try:
        for component in pure.parts[:-1]:
            following = os.open(component, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0), dir_fd=directory); os.close(directory); directory = following
        descriptor = os.open(pure.parts[-1], os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0), dir_fd=directory)
        try:
            metadata = os.fstat(descriptor)
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > MAX_FILE:
                fail(f"artifact is not a bounded regular file: {relative}")
            data = os.read(descriptor, MAX_FILE + 1)
            if len(data) > MAX_FILE: fail(f"artifact exceeds bound: {relative}")
            return data
        finally: os.close(descriptor)
    finally: os.close(directory)


def strict_boundary(directory, root):
    command = ["cargo", "run", "--manifest-path", str(root / "tools/openmodelica-reliefs-reference/Cargo.toml"), "--offline", "--locked", "--quiet", "--", "verify-architecture-canonical", str(directory)]
    environment = os.environ.copy(); environment["CARGO_NET_OFFLINE"] = "true"
    result = subprocess.run(command, cwd=root, env=environment, capture_output=True, text=True)
    if result.returncode != 0 or result.stdout != "strict canonical boundary passed\n":
        fail(f"strict Rust canonical boundary: {result.stderr.strip()}")


def expected_runs(directory, record):
    return [{"id": "run-a", "output_directory_token": "fresh-run-a", "log_sha256": sha(directory / "run-a.log"), "raw_sha256": record["raw_run_a_sha256"]}, {"id": "run-b", "output_directory_token": "fresh-run-b", "log_sha256": sha(directory / "run-b.log"), "raw_sha256": record["raw_run_b_sha256"]}]


def validate_architecture(directory, root, architecture):
    directory, root = safe_directory(directory, "architecture evidence"), safe_directory(root, "repository root")
    try: safe_files.read_closed_directory(directory, ARCH_FILES, MAX_FILE)
    except (OSError, ValueError) as error: fail(f"unsafe architecture evidence: {error}")
    record = json_file(directory / "architecture.json")
    fields = ["format", "architecture", "platform", "host_architecture", "docker_server_architecture", "container_architecture", "platform_manifest_digest", "config_digest", "repository_revision", "generator_provenance_scope", "generator_inputs", "artifact_toolchain", "source_materialization", "source_files", "omc_version", "gcc_version", "binutils_version", "glibc_version", "raw_run_a_sha256", "raw_run_b_sha256", "canonical_sha256", "parameter_control_raw_sha256", "parameter_control_canonical_sha256", "final_clamp_raw_sha256", "final_clamp_canonical_sha256", "projection_mutation", "runs"]
    closed(record, fields, "architecture record")
    if [record["format"], record["architecture"], record["platform"], record["host_architecture"], record["docker_server_architecture"], record["container_architecture"], record["platform_manifest_digest"], record["config_digest"], record["generator_provenance_scope"]] != ["oce-openmodelica-reliefs-native-architecture-v4", architecture, *ARCH[architecture], "native_generation_and_publication"]:
        fail("architecture literals")
    revision = record["repository_revision"]
    if not isinstance(revision, str) or len(revision) != 40 or any(char not in "0123456789abcdef" for char in revision): fail("architecture repository revision")
    closed(record["generator_inputs"], GENERATOR_INPUT_PATHS, "architecture generator inputs")
    expected_inputs = {key: hashlib.sha256(artifact_bytes(directory, root, path)).hexdigest() for key, path in GENERATOR_INPUT_PATHS.items()}
    if record["generator_inputs"] != expected_inputs: fail("native generator inputs do not match assembly repository bytes")
    if not type_exact_equal(record["artifact_toolchain"], expected_toolchain(architecture)): fail("native artifact toolchain identity")
    if not type_exact_equal(record["source_materialization"], expected_materialization()): fail("native source materialization identity")
    if not type_exact_equal(record["source_files"], source_files()): fail("native source cone identity")
    strict_boundary(directory, root)
    bindings = [("reliefs-run-a.raw.csv", "raw_run_a_sha256"), ("reliefs-run-b.raw.csv", "raw_run_b_sha256"), ("reliefs.canonical.csv", "canonical_sha256"), ("parameter-control.raw.csv", "parameter_control_raw_sha256"), ("parameter-control.canonical.csv", "parameter_control_canonical_sha256"), ("final-clamp.raw.csv", "final_clamp_raw_sha256"), ("final-clamp.canonical.csv", "final_clamp_canonical_sha256")]
    for name, key in bindings:
        if sha(directory / name) != record[key]: fail(f"architecture digest binding for {name}")
    if record["raw_run_a_sha256"] != record["raw_run_b_sha256"]: fail("native repeat raw digests differ")
    if not type_exact_equal(record["runs"], expected_runs(directory, record)): fail("native repeat run records")
    if architecture == "arm64" and (record["raw_run_a_sha256"] != ARM_RAW or record["final_clamp_raw_sha256"] != ARM_CLAMP_RAW): fail("arm64 raw digest drifted from measured spike")
    rows_a, groups = parse_raw(directory / "reliefs-run-a.raw.csv", record["raw_run_a_sha256"]); rows_b, _ = parse_raw(directory / "reliefs-run-b.raw.csv", record["raw_run_b_sha256"])
    if rows_a != rows_b: fail("repeat raw rows differ")
    projected_values = project(groups); projected = [[bits(value) for value in row[:8]] for row in projected_values]
    if projected != expected_rows() or [row[8] for row in projected_values] != SOURCE_ROWS: fail("main selected rows or independent bits drifted")
    if parse_canonical(directory / "reliefs.canonical.csv", "openmodelica_g36_reliefs") != projected: fail("canonical output is not raw keep-last projection")
    mutation = projection_evidence.record(directory, record["raw_run_a_sha256"], sha)
    if not type_exact_equal(record["projection_mutation"], mutation): fail("native projection mutation record")
    mutation_rows = parse_canonical(directory / mutation["canonical_output"], "openmodelica_g36_reliefs")
    projection_evidence.validate(rows_a, groups, projected, mutation_rows, text(directory / mutation["metadata"]), text(directory / mutation["log"]), record["raw_run_a_sha256"], mutation["canonical_sha256"], mutation["metadata_sha256"], expected_inputs["canonicalizer_sha256"])
    parameter_raw, parameter_groups = parse_raw(directory / "parameter-control.raw.csv", record["parameter_control_raw_sha256"]); parameter_values = project(parameter_groups); parameter = [[bits(value) for value in row[:8]] for row in parameter_values]
    if parse_canonical(directory / "parameter-control.canonical.csv", "openmodelica_g36_reliefs_parameter_control") != parameter: fail("parameter control canonical reproduction")
    if [row[:6] for row in parameter] != [row[:6] for row in projected] or parameter[2][6] != "3fec000000000000" or projected[2][6] != "3fe2000000000000" or next((i for i, (a, b) in enumerate(zip(projected, parameter)) if a[6:] != b[6:]), None) != 2: fail("parameter control first mismatch")
    clamp_raw, clamp_groups = parse_raw(directory / "final-clamp.raw.csv", record["final_clamp_raw_sha256"]); clamp_values = project(clamp_groups); clamp = [[bits(value) for value in row[:8]] for row in clamp_values]
    if parse_canonical(directory / "final-clamp.canonical.csv", "openmodelica_g36_reliefs_final_clamp") != clamp: fail("final clamp canonical reproduction")
    if any(bits(row[6]) != "3fd0000000000000" or bits(row[7]) != "3fe8000000000000" for row in clamp_raw): fail("final clamp did not overwrite every raw output")
    for name, token, model, digest in [("run-a.log", "fresh-run-a", "Reliefs", record["raw_run_a_sha256"]), ("run-b.log", "fresh-run-b", "Reliefs", record["raw_run_b_sha256"]), ("parameter-control.log", "fresh-parameter-control", "ParameterControl", record["parameter_control_raw_sha256"]), ("final-clamp.log", "fresh-final-clamp", "FinalClamp", record["final_clamp_raw_sha256"])]:
        log_contract(directory / name, architecture, token, model, digest, revision, expected_inputs, record["source_files"])
    validate_oci(directory, architecture); validate_wrappers(root)
    return record, projected


def expected_artifact_roles():
    fixture = "crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs/"; tool = "tools/openmodelica-reliefs-reference/"
    roles = [("image_index_json", fixture + "image-index.json"), ("cross_architecture_log", fixture + "cross-architecture.log")]
    files = [("architecture_record", "architecture.json"), ("canonical_csv", "reliefs.canonical.csv"), ("raw_run_a_csv", "reliefs-run-a.raw.csv"), ("raw_run_b_csv", "reliefs-run-b.raw.csv"), ("run_a_log", "run-a.log"), ("run_b_log", "run-b.log"), ("parameter_control_canonical_csv", "parameter-control.canonical.csv"), ("parameter_control_raw_csv", "parameter-control.raw.csv"), ("parameter_control_log", "parameter-control.log"), ("final_clamp_canonical_csv", "final-clamp.canonical.csv"), ("final_clamp_raw_csv", "final-clamp.raw.csv"), ("final_clamp_log", "final-clamp.log"), ("projection_mutation_log", "projection-mutation.log"), ("projection_keep_first_canonical_csv", "projection-keep-first.canonical.csv"), ("projection_keep_first_metadata", "projection-keep-first.metadata"), ("architecture_image_index_json", "image-index.json"), ("platform_image_manifest_json", "image-manifest.json")]
    for architecture in ["arm64", "amd64"]: roles.extend((f"{architecture}_{role}", f"{fixture}{architecture}/{file}") for role, file in files)
    tracked = [("canonicalizer_source", "crates/oce-cxf/tests/open_modelica_reliefs_reference/canonicalizer.rs"), ("tool_cargo_lock", tool + "Cargo.lock"), ("tool_cargo_toml", tool + "Cargo.toml"), ("tool_main_source", tool + "src/main.rs"), ("wrapper_model", tool + "reliefs/ReliefsPilot.mo"), ("parameter_control_wrapper_model", tool + "reliefs/ReliefsParameterPilot.mo"), ("final_clamp_wrapper_model", tool + "reliefs/ReliefsClampPilot.mo"), ("runner_script", tool + "reliefs/runner.sh"), ("regeneration_script", tool + "reliefs/regenerate.sh"), ("assembly_script", tool + "reliefs/assemble.sh"), ("manifest_generator_script", tool + "reliefs/generate_manifest.py"), ("architecture_generator_script", tool + "reliefs/generate_architecture.py"), ("evidence_validator_script", tool + "reliefs/verify_evidence.py"), ("projection_validator_script", tool + "reliefs/projection_evidence.py"), ("safe_file_helper_script", tool + "reliefs/safe_files.py"), ("oci_materializer_script", tool + "reliefs/materialize_oci.py"), ("deadline_script", tool + "reliefs/deadline.sh"), ("deadline_test_script", tool + "reliefs/deadline_test.sh"), ("output_publish_script", tool + "reliefs/output_publish.py"), ("output_publish_test_script", tool + "reliefs/output_publish_test.sh"), ("container_cleanup_script", tool + "reliefs/container_cleanup.sh"), ("container_cleanup_test_script", tool + "reliefs/container_cleanup_test.sh"), ("oci_index_source", tool + "reliefs/image-index.json"), ("arm64_manifest_source", tool + "reliefs/image-manifest-arm64.json"), ("amd64_manifest_source", tool + "reliefs/image-manifest-amd64.json"), ("evidence_workflow", ".github/workflows/openmodelica-reliefs-evidence.yml")]
    return roles + tracked


def expected_sources():
    records = source_files()
    return [
        {"name": "buildings", "repository": "https://github.com/lbl-srg/modelica-buildings.git", "commit": "a131864e4c4df22ebcd52bb8da439de0087ac365", "tree": "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09", "package": "Buildings", "version": "14.0.0", "materialization": "git_archive_without_local_attribute_override", "transforms": [], "files": [{key: value for key, value in item.items() if key != "source"} for item in records if item["source"] == "buildings"]},
        {"name": "modelica", "repository": "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git", "commit": "7a4bf7de77a3986e8eb1e88cbb515d646f78f834", "tree": "43d7d8fc1a991358e9e5e91976e27cdc4280173f", "package": "Modelica", "version": "4.1.0", "materialization": "git_archive_with_pinned_modelica_export_subst", "transforms": [{"path": "Modelica/package.mo", "rule": "Modelica/package.mo -export-subst"}], "files": [{key: value for key, value in item.items() if key != "source"} for item in records if item["source"] == "modelica"]},
    ]


def expected_manifest_records(canonical_sha):
    scope = {"class": "Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Modulations.Reliefs", "scenario": "source_default_dyadic_regions", "parameters": {"uMin": "-0.25", "uMax": "0.25", "uOutDamMax": "0", "uRetDamMin": "0"}, "inputs": ["uTSup", "uOutDam_min", "uOutDam_max", "uRetDam_min", "uRetDam_max"], "outputs": ["yOutDam", "yRetDam"], "comparison": "exact_finite_f64_bits", "global_tier3_status": "skipped"}
    image = {"repository": "openmodelica/openmodelica", "tag": "v1.25.1-minimal", "index_digest": INDEX, "platforms": [{"platform": ARCH[name][0], "manifest_digest": ARCH[name][4], "config_digest": ARCH[name][5]} for name in ["arm64", "amd64"]]}
    simulation = {"method": "dassl", "start_time": "0", "stop_time": "420", "number_of_intervals": 7, "tolerance": "1e-9", "output_format": "csv", "variable_filter": "^(uTSup|uOutDam_min|uOutDam_max|uRetDam_min|uRetDam_max|yOutDam|yRetDam)$", "simflags": "", "event_emission": True, "raw_header": HEADER}
    projection = {"columns": COLUMNS, "grouping": "contiguous_equal_f64_bits", "group_selection": "last", "tuple_selection": "initial_then_first_complete_five_input_tuple_change", "normalize_times": False, "raw_rows": 21, "grouped_rows": 14, "canonical_rows": 7, "group_sizes": GROUPS, "raw_time_bits": RAW_TIME_BITS, "selected_source_rows": SOURCE_ROWS, "canonical_time_bits": TIME_BITS, "canonical_input_bits": {"uTSup": U_T_SUP_BITS, "uOutDam_min": [CONSTANT_INPUT_BITS[0]] * 7, "uOutDam_max": [CONSTANT_INPUT_BITS[1]] * 7, "uRetDam_min": [CONSTANT_INPUT_BITS[2]] * 7, "uRetDam_max": [CONSTANT_INPUT_BITS[3]] * 7}}
    outputs = {"yOutDam": Y_OUT, "yRetDam": Y_RET}
    controls = {"parameter": {"mutation": "uOutDamMax 0 to -0.125", "first_mismatch_row": 2, "first_mismatch_time_bits": TIME_BITS[2], "output": "yOutDam", "expected_bits": "3fe2000000000000", "observed_bits": "3fec000000000000"}, "mapping": {"mutation": "swap yOutDam and yRetDam", "expected_comparison": "exact_mismatch"}, "declared_path": {"mutation": "replace yRetDam root with nonexistent declared root", "expected_error": "unknown point/connector 'http://example.org#g36.source.multizone_vav_economizer_modulations_reliefs.yMissing'"}, "final_clamp": {"inputs": {"uOutDam_min": "3fec000000000000", "uOutDam_max": "3fd0000000000000", "uRetDam_min": "3fe8000000000000", "uRetDam_max": "3fc0000000000000"}, "yOutDam": "3fd0000000000000", "yRetDam": "3fe8000000000000", "rows": 7}}
    regeneration = {"entrypoint": "tools/openmodelica-reliefs-reference/reliefs/regenerate.sh", "assembly_entrypoint": "tools/openmodelica-reliefs-reference/reliefs/assemble.sh", "evidence_workflow": ".github/workflows/openmodelica-reliefs-evidence.yml", "network": "none_during_container_execution", "pull": "never", "platforms": ["linux/arm64", "linux/amd64"], "source_materialization": "git_archive_with_pinned_modelica_export_subst", "source_mounts": "read_only", "container_root": "read_only", "container_user": "non_root", "capabilities": "none", "no_new_privileges": True, "device_mounts": 0, "docker_socket_mounted": False, "timeout_seconds": 120, "cpus": "4", "memory_bytes": 2147483648, "memory_swap_bytes": 2147483648, "memory_measurement": "cgroup_memory_peak", "pids_limit": 256, "tmpfs_bytes": 268435456, "per_file_bytes": 67108864, "output_directory_bytes": 268435456}
    cross = {"comparison": "canonical_bytes", "arm64_sha256": canonical_sha, "amd64_sha256": canonical_sha, "result": "pass"}
    return scope, image, simulation, projection, outputs, controls, regeneration, cross


def architecture_manifest(name, record):
    return {key: value for key, value in record.items() if key not in {"format", "architecture"}} | {"name": name}


def validate_final(output, root):
    output, root = safe_directory(output, "assembled evidence"), safe_directory(root, "repository root")
    if {entry.name for entry in os.scandir(output)} != {"manifest.json", "image-index.json", "cross-architecture.log", "arm64", "amd64"}: fail("assembled evidence entries are not closed")
    manifest = json_file(output / "manifest.json", MAX_MANIFEST)
    fields = ["format", "scope", "image", "sources", "simulation", "projection", "expected_output_bits", "architectures", "controls", "cross_architecture", "artifacts", "regeneration"]
    closed(manifest, fields, "manifest")
    if manifest["format"] != "oce-openmodelica-reliefs-external-run-v1": fail("unsupported manifest format")
    roles = expected_artifact_roles(); artifacts = manifest["artifacts"]
    if not isinstance(artifacts, list) or len(artifacts) != len(roles): fail("artifact closure count")
    for item, expected in zip(artifacts, roles):
        closed(item, ["role", "path", "sha256"], "artifact")
        if (item["role"], item["path"]) != expected: fail("unknown or misplaced artifact role/path")
        if hashlib.sha256(artifact_bytes(output, root, item["path"])).hexdigest() != item["sha256"]: fail(f"artifact digest mismatch: {item['path']}")
    arm, arm_rows = validate_architecture(output / "arm64", root, "arm64"); amd, amd_rows = validate_architecture(output / "amd64", root, "amd64")
    if arm["repository_revision"] != amd["repository_revision"] or arm["generator_inputs"] != amd["generator_inputs"] or arm["source_files"] != amd["source_files"]: fail("native architectures used different generator provenance")
    if arm_rows != amd_rows or read_bounded(output / "arm64/reliefs.canonical.csv") != read_bounded(output / "amd64/reliefs.canonical.csv"): fail("cross-architecture canonical bytes differ")
    canonical_sha = sha(output / "arm64/reliefs.canonical.csv"); scope, image, simulation, projection, outputs, controls, regeneration, cross = expected_manifest_records(canonical_sha)
    for field, expected in [("scope", scope), ("image", image), ("sources", expected_sources()), ("simulation", simulation), ("projection", projection), ("expected_output_bits", outputs), ("controls", controls), ("regeneration", regeneration), ("cross_architecture", cross)]:
        if not type_exact_equal(manifest[field], expected): fail(f"unsupported or open {field} record")
    expected_architectures = [architecture_manifest("arm64", arm), architecture_manifest("amd64", amd)]
    if not type_exact_equal(manifest["architectures"], expected_architectures): fail("manifest architecture records")
    expected_cross = ["comparison=canonical bytes", f"arm64_sha256={canonical_sha}", f"amd64_sha256={canonical_sha}", "result=PASS"]
    if text(output / "cross-architecture.log").splitlines() != expected_cross: fail("cross-architecture record")


def type_exact_equal(actual, expected):
    if type(actual) is not type(expected): return False
    if isinstance(expected, dict): return set(actual) == set(expected) and all(type_exact_equal(actual[key], value) for key, value in expected.items())
    if isinstance(expected, list): return len(actual) == len(expected) and all(type_exact_equal(left, right) for left, right in zip(actual, expected))
    return actual == expected


def main(arguments):
    if len(arguments) == 2 and arguments[0] == "precopy":
        safe_files.read_closed_directory(arguments[1], ARCH_FILES, MAX_FILE); print("Reliefs architecture pre-copy validation passed")
    elif len(arguments) == 3 and arguments[0] == "copy-architecture":
        safe_files.copy_closed_directory(arguments[1], arguments[2], ARCH_FILES, MAX_FILE); print("Reliefs architecture bounded copy passed")
    elif len(arguments) == 4 and arguments[0] == "architecture":
        validate_architecture(arguments[1], arguments[2], arguments[3]); print(f"Reliefs {arguments[3]} architecture evidence verification passed")
    elif len(arguments) == 3 and arguments[0] == "final":
        validate_final(arguments[1], arguments[2]); print("Reliefs assembled evidence verification passed")
    else:
        print("usage: verify_evidence.py precopy EVIDENCE | copy-architecture SOURCE DESTINATION | architecture EVIDENCE REPOSITORY_ROOT ARCH | final EVIDENCE REPOSITORY_ROOT", file=sys.stderr); raise SystemExit(2)


if __name__ == "__main__":
    try: main(sys.argv[1:])
    except (KeyError, OSError, ValueError) as error:
        print(error, file=sys.stderr); raise SystemExit(1)
