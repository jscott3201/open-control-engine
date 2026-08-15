#!/usr/bin/env python3
"""Generate the closed Toggle manifest from fixed facts and fresh files."""

import hashlib, json, os, pathlib, stat, sys

MAX_FILE = 1024 * 1024


def safe_directory(path, name):
    path = pathlib.Path(path).absolute()
    current = pathlib.Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        metadata = current.lstat()
        if current.is_symlink(): raise ValueError(f"{name} contains symlink component")
    if not stat.S_ISDIR(path.lstat().st_mode): raise ValueError(f"{name} is not a directory")
    return path


def read_bounded(path, trusted):
    path = pathlib.Path(path).absolute()
    try: path.relative_to(trusted)
    except ValueError as error: raise ValueError("hash path escapes its trusted root") from error
    current = trusted
    for part in path.relative_to(trusted).parts:
        current /= part
        if current.is_symlink(): raise ValueError("hash path contains symlink component")
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or before.st_size > MAX_FILE: raise ValueError("hash input is not a bounded regular file")
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0))
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode) or opened.st_size > MAX_FILE: raise ValueError("opened hash input is not a bounded regular file")
        chunks, total = [], 0
        while True:
            chunk = os.read(descriptor, min(65536, MAX_FILE + 1 - total))
            if not chunk: break
            chunks.append(chunk); total += len(chunk)
            if total > MAX_FILE: raise ValueError("hash input exceeds bound")
        return b"".join(chunks)
    finally: os.close(descriptor)


out, root = safe_directory(sys.argv[1], "output"), safe_directory(sys.argv[2], "repository root")
fixture = "crates/oce-conformance/tests/fixtures/open_modelica/logical_toggle/"
tool = "tools/openmodelica-toggle-reference/"
roles = [
    ("canonical_csv", fixture + "toggle.canonical.csv", out / "toggle.canonical.csv"),
    ("canonicalizer_source", "crates/oce-cxf/tests/open_modelica_toggle_reference/canonicalizer.rs", root / "crates/oce-cxf/tests/open_modelica_toggle_reference/canonicalizer.rs"),
    ("image_index_json", fixture + "image-index.json", out / "image-index.json"),
    ("image_manifest_json", fixture + "image-manifest.json", out / "image-manifest.json"),
    ("projection_mutation_log", fixture + "projection-mutation.log", out / "projection-mutation.log"),
    ("raw_run_a_csv", fixture + "toggle-run-a.raw.csv", out / "toggle-run-a.raw.csv"),
    ("raw_run_b_csv", fixture + "toggle-run-b.raw.csv", out / "toggle-run-b.raw.csv"),
    ("regeneration_script", tool + "toggle/regenerate.sh", root / tool / "toggle/regenerate.sh"),
    ("run_a_log", fixture + "run-a.log", out / "run-a.log"),
    ("run_b_log", fixture + "run-b.log", out / "run-b.log"),
    ("runner_script", tool + "toggle/runner.sh", root / tool / "toggle/runner.sh"),
    ("semantic_control_canonical_csv", fixture + "latch.canonical.csv", out / "latch.canonical.csv"),
    ("semantic_control_log", fixture + "latch.log", out / "latch.log"),
    ("semantic_control_raw_csv", fixture + "latch.raw.csv", out / "latch.raw.csv"),
    ("semantic_control_wrapper_model", tool + "toggle/LatchPilot.mo", root / tool / "toggle/LatchPilot.mo"),
    ("tool_cargo_lock", tool + "Cargo.lock", root / tool / "Cargo.lock"),
    ("tool_cargo_toml", tool + "Cargo.toml", root / tool / "Cargo.toml"),
    ("tool_main_source", tool + "src/main.rs", root / tool / "src/main.rs"),
    ("evidence_validator_script", tool + "toggle/verify_evidence.py", root / tool / "toggle/verify_evidence.py"),
    ("manifest_generator_script", tool + "toggle/generate_manifest.py", root / tool / "toggle/generate_manifest.py"),
    ("deadline_script", tool + "toggle/deadline.sh", root / tool / "toggle/deadline.sh"),
    ("deadline_test_script", tool + "toggle/deadline_test.sh", root / tool / "toggle/deadline_test.sh"),
    ("output_publish_script", tool + "toggle/output_publish.py", root / tool / "toggle/output_publish.py"),
    ("output_publish_test_script", tool + "toggle/output_publish_test.sh", root / tool / "toggle/output_publish_test.sh"),
    ("container_cleanup_script", tool + "toggle/container_cleanup.sh", root / tool / "toggle/container_cleanup.sh"),
    ("container_cleanup_test_script", tool + "toggle/container_cleanup_test.sh", root / tool / "toggle/container_cleanup_test.sh"),
    ("wrapper_model", tool + "toggle/TogglePilot.mo", root / tool / "toggle/TogglePilot.mo"),
]
sha = lambda path: hashlib.sha256(read_bounded(path, out if pathlib.Path(path).absolute().is_relative_to(out) else root)).hexdigest()
source_files = [
    ("Buildings/package.mo", "f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59"),
    ("Buildings/Controls/OBC/CDL/Logical/Toggle.mo", "49d33a88242163def412dff83e3f49e23b187fe56859a01649edbf5fb2bab60c"),
    ("Buildings/Controls/OBC/CDL/Logical/Latch.mo", "2ab7e15a0026d5a6c0089d7af88329869fc3e397398a7b86ad5ea49fd1d9b271"),
    ("Complex.mo", "9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f"),
    ("Modelica/package.mo", "c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191"),
    ("Modelica/Blocks/Sources.mo", "565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3"),
    ("ModelicaServices/package.mo", "7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb"),
]
time_bits = "0000000000000000 403e000000001dff 404e000000000000 4056800000000780 405e000000000000 4062c000000003c1 4066800000000000 406a4000000003c1 406e000000000000 4070e000000001e0 4072c00000000000 4073600000000320 4075e000000002d0 4076800000000000 40786000000003c1 407a400000000000 407ae00000000320 407c2000000003c1 407e000000000000 407fe000000003c1 4080e00000000000 4082c00000000000".split()
manifest = {
    "format": "oce-openmodelica-toggle-external-run-v1",
    "scope": {"class": "CDL.Logical.Toggle", "scenario": "repeated_rises_initial_true_and_clear_priority", "inputs": ["u", "clr"], "output": "y", "comparison": "exact_boolean", "global_tier3_status": "skipped"},
    "image": {"repository": "openmodelica/openmodelica", "tag": "v1.25.1-minimal", "index_digest": "sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864", "platform_manifest_digest": "sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4", "config_digest": "sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666", "platform": "linux/arm64", "host_architecture": "arm64", "docker_server_architecture": "aarch64", "omc_version": "OpenModelica 1.25.1", "gcc_version": "11.4.0", "binutils_version": "2.38", "glibc_version": "2.35"},
    "sources": [
        {"name": "buildings", "repository": "https://github.com/lbl-srg/modelica-buildings.git", "commit": "a131864e4c4df22ebcd52bb8da439de0087ac365", "tree": "a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09", "package": "Buildings", "version": "14.0.0", "files": [{"path": p, "sha256": d} for p, d in source_files[:3]]},
        {"name": "modelica", "repository": "https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git", "commit": "7a4bf7de77a3986e8eb1e88cbb515d646f78f834", "tree": "43d7d8fc1a991358e9e5e91976e27cdc4280173f", "package": "Modelica", "version": "4.1.0", "files": [{"path": p, "sha256": d} for p, d in source_files[3:]]},
    ],
    "simulation": {"method": "dassl", "start_time": "0", "stop_time": "600", "number_of_intervals": 10, "tolerance": "1e-9", "output_format": "csv", "variable_filter": "^(u|clr|y)$", "simflags": "", "event_emission": True, "raw_header": '"time","clr","u","y"'},
    "projection": {"columns": ["time", "u", "clr", "y"], "grouping": "contiguous_equal_f64_bits", "selection": "last", "normalize_times": False, "raw_rows": 34, "canonical_rows": 22, "group_sizes": [1,2,1,2,1,2,1,2,1,2,1,2,2,1,2,1,2,2,1,2,1,2], "canonical_time_bits": time_bits},
    "runs": [
        {"id": "run-a", "output_directory_token": "fresh-run-a", "log_path": fixture + "run-a.log", "raw_path": fixture + "toggle-run-a.raw.csv", "log_sha256": sha(out / "run-a.log"), "raw_sha256": sha(out / "toggle-run-a.raw.csv")},
        {"id": "run-b", "output_directory_token": "fresh-run-b", "log_path": fixture + "run-b.log", "raw_path": fixture + "toggle-run-b.raw.csv", "log_sha256": sha(out / "run-b.log"), "raw_sha256": sha(out / "toggle-run-b.raw.csv")},
    ],
    "semantic_control": {"substitution_class": "CDL.Logical.Latch", "first_mismatch_row": 3, "first_mismatch_time_bits": time_bits[3], "raw_rows": 34, "canonical_rows": 22, "expected_comparison": "exact_mismatch"},
    "artifacts": [{"role": role, "path": path, "sha256": sha(file)} for role, path, file in roles],
    "regeneration": {"entrypoint": tool + "toggle/regenerate.sh", "network": "none", "pull": "never", "platform": "linux/arm64", "source_materialization": "git_archive", "source_mounts": "read_only", "container_root": "read_only", "container_user": "non_root", "capabilities": "none", "no_new_privileges": True, "device_mounts": 0, "docker_socket_mounted": False, "timeout_seconds": 120, "cpus": "4", "memory_bytes": 2147483648, "memory_swap_bytes": 2147483648, "pids_limit": 256, "tmpfs_bytes": 268435456, "per_file_bytes": 67108864, "output_directory_bytes": 268435456},
}
payload = (json.dumps(manifest, indent=2) + "\n").encode("utf-8")
flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
descriptor = os.open(out / "manifest.json", flags, 0o600)
try:
    written = 0
    while written < len(payload):
        written += os.write(descriptor, payload[written:])
finally:
    os.close(descriptor)
