#!/usr/bin/env python3
"""Generate the closed two-architecture Reliefs manifest from retained files."""

import hashlib
import json
import os
import pathlib
import sys

import safe_files
import verify_evidence as verifier

MAX_FILE = 1024 * 1024
FIXTURE = "crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs/"


def read(path):
    return safe_files.read_bounded(path, MAX_FILE)


def sha(path):
    return hashlib.sha256(read(path)).hexdigest()


output = pathlib.Path(sys.argv[1])
root = pathlib.Path(sys.argv[2])
for directory, label in [(output, "output"), (root, "repository root")]:
    descriptor = safe_files.open_directory(directory)
    os.close(descriptor)


def native(name):
    record = json.loads(read(output / name / "architecture.json"), object_pairs_hook=verifier.pairs)
    return verifier.architecture_manifest(name, record), record


roles = [
    ("image_index_json", FIXTURE + "image-index.json", output / "image-index.json"),
    ("cross_architecture_log", FIXTURE + "cross-architecture.log", output / "cross-architecture.log"),
]
architecture_files = [
    ("architecture_record", "architecture.json"),
    ("canonical_csv", "reliefs.canonical.csv"),
    ("raw_run_a_csv", "reliefs-run-a.raw.csv"),
    ("raw_run_b_csv", "reliefs-run-b.raw.csv"),
    ("run_a_log", "run-a.log"),
    ("run_b_log", "run-b.log"),
    ("parameter_control_canonical_csv", "parameter-control.canonical.csv"),
    ("parameter_control_raw_csv", "parameter-control.raw.csv"),
    ("parameter_control_log", "parameter-control.log"),
    ("final_clamp_canonical_csv", "final-clamp.canonical.csv"),
    ("final_clamp_raw_csv", "final-clamp.raw.csv"),
    ("final_clamp_log", "final-clamp.log"),
    ("projection_mutation_log", "projection-mutation.log"),
    ("projection_keep_first_canonical_csv", "projection-keep-first.canonical.csv"),
    ("projection_keep_first_metadata", "projection-keep-first.metadata"),
    ("architecture_image_index_json", "image-index.json"),
    ("platform_image_manifest_json", "image-manifest.json"),
]
for name in ["arm64", "amd64"]:
    roles.extend(
        (f"{name}_{role}", f"{FIXTURE}{name}/{file}", output / name / file)
        for role, file in architecture_files
    )
for role, path in verifier.expected_artifact_roles()[len(roles):]:
    roles.append((role, path, root / path))

arm, arm_record = native("arm64")
amd, _ = native("amd64")
canonical_sha = arm_record["canonical_sha256"]
scope, image, simulation, projection, expected_outputs, controls, regeneration, cross = verifier.expected_manifest_records(canonical_sha)
manifest = {
    "format": "oce-openmodelica-reliefs-external-run-v1",
    "scope": scope,
    "image": image,
    "sources": verifier.expected_sources(),
    "simulation": simulation,
    "projection": projection,
    "expected_output_bits": expected_outputs,
    "architectures": [arm, amd],
    "controls": controls,
    "cross_architecture": cross,
    "artifacts": [
        {"role": role, "path": path, "sha256": sha(file)} for role, path, file in roles
    ],
    "regeneration": regeneration,
}
payload = (json.dumps(manifest, indent=2) + "\n").encode()
descriptor = os.open(output / "manifest.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0), 0o600)
try:
    os.write(descriptor, payload)
finally:
    os.close(descriptor)
