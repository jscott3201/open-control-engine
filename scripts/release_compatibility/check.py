#!/usr/bin/env python3
"""Read-only, standard-library candidate evidence checker (Python 3.11+).

--candidate prints deterministic review input; it never writes/blesses a fixture.
--verify-history additionally requires the exact Git objects (not a shallow clone).
Normal checks bind live implementation bytes and evidence, not the delivery HEAD.
This is a drift guard, not protection against coordinated edits or host authentication.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import stat
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
MATRIX = "docs/release-compatibility.json"
HISTORY = "docs/release-compatibility-history.json"
BASELINE = "e81480b02271456719d55cbe1e5090b0dea6d63c"
TAG = "7f3b614dc0e466d54cab4677ce4bb08a5bfaf033"
HISTORICAL = "909a8ba699e6a2fccf3de6ac0616a9e83a04060f"
# Reviewed implementation boundary at BASELINE; filled from exact Git source, not HEAD.
IMPLEMENTATION = "250ba0454a530207d5f4387373f3fd797cf1d6ea0fd51130349aea108c420dc9"
HISTORY_DIGEST = "483e21edadd18cc8dac264e6ef76bbe342a09c2305fbc69642ce67dce97d41c5"
ARTIFACTS = (
    "package-public-api", "facade-catalog-schema", "public-descriptor", "diagnostics",
    "execution-profile", "executable-frame", "snapshot", "replay", "strict-bit-qualification",
)
UNAVAILABLE = frozenset(("facade-catalog-schema", "public-descriptor", "diagnostics",
                         "executable-frame", "snapshot", "replay", "strict-bit-qualification"))
DIRECTIONS = (("current", "current"), ("historical", "current"),
              ("current", "historical"), ("historical", "historical"))
# Enforcement-source digests live only in MATRIX. Neither bound source embeds its own digest
# or the matrix digest; the tests' runtime fixture read does not make source hashing recursive.
EVIDENCE = (
    "crates/oce-api/tests/public-api.txt",
    "crates/oce-store/tests/public-api.txt",
    "crates/oce-api/tests/public_surface_contract.rs",
    "crates/oce-api/tests/catalog_contract.rs",
    "crates/oce-api/tests/compatibility.rs",
    "crates/oce-api/tests/fixtures/compatibility.txt",
    "crates/oce-api/tests/state_contract.rs",
    "crates/oce-api/tests/replay.rs",
    "crates/oce-api/tests/replay_codec.rs",
    "crates/oce-api/tests/release_fallback.rs",
    "scripts/release_compatibility/test_check.py",
    "crates/oce-api/tests/release_compatibility.rs",
    "crates/oce-conformance/tests/strict_bits/matrix.rs",
    "crates/oce-conformance/tests/fixtures/strict_bits/receipts.json",
)


class Refusal(ValueError):
    """Deterministic evidence refusal."""


def enforce(actual: bytes, expected: bytes) -> None:
    """Require exactly the independently selected candidate evidence bytes."""
    if actual != expected:
        raise Refusal("matrix: evidence differs (missing, extra, reordered or mutated bytes)")


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise Refusal(detail)


def read(root: Path, path: str) -> bytes:
    """Only fixed repository-relative, regular, nonsymlink evidence paths."""
    require(not Path(path).is_absolute() and ".." not in Path(path).parts, "source: unsafe path")
    current = root
    for part in Path(path).parts:
        current /= part
        require(not current.is_symlink(), f"source: symlink: {path}")
    try:
        require(stat.S_ISREG(current.stat().st_mode), f"source: not regular: {path}")
        return current.read_bytes()
    except OSError as error:
        raise Refusal(f"source: missing or unreadable: {path}") from error


def git(root: Path, *args: str) -> bytes:
    result = subprocess.run(["git", *args], cwd=root, capture_output=True, timeout=30)
    require(result.returncode == 0, "history: required Git objects/query unavailable")
    return result.stdout


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def implementation_path(path: str) -> bool:
    return (path in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml")
            or (path.startswith("crates/") and
                ("/src/" in path or "/contracts/" in path
                 or path.endswith(("/Cargo.toml", "/build.rs")))))


def implementation(root: Path, revision: str | None = None) -> dict:
    """Closed sorted path/byte boundary, including added untracked source; no build-ID claim."""
    inventory = (git(root, "ls-tree", "-rz", "--name-only", revision) if revision else
                 git(root, "ls-files", "-z", "--cached", "--others", "--exclude-standard"))
    paths = sorted({p for p in inventory.decode().split("\0") if implementation_path(p)})
    require(len(paths) > 100, "source: empty/incomplete implementation inventory")
    records = []
    for path in paths:
        data = git(root, "show", f"{revision}:{path}") if revision else read(root, path)
        records.append(f"{path}\0{digest(data)}\n")
    return {"files": len(paths), "sha256": digest("".join(records).encode())}


def one(text: str, pattern: str) -> str:
    values = re.findall(pattern, text)
    require(len(values) == 1, "facts: required unique source declaration missing")
    return values[0]


def current_facts(root: Path) -> dict:
    text = lambda p: read(root, f"crates/oce-api/{p}").decode()
    package = tomllib.loads(read(root, "Cargo.toml").decode())["workspace"]["package"]["version"]
    state = text("src/state.rs")
    compatibility = text("src/compatibility.rs")
    codec = text("src/replay_codec.rs")
    lines = text("tests/fixtures/compatibility.txt").splitlines()
    require(len(lines) == 10 and all(":" in line for line in lines), "facts: descriptor grammar")
    descriptor = dict(line.split(":", 1) for line in lines)
    require(len(descriptor) == 10, "facts: duplicate descriptor fields")
    domains = ("diagnostics", "io", "values", "parameters", "assertions", "execution-profile")
    schemas = {name: json.loads(text(f"contracts/{name}.schema.json"))["revision"] for name in domains}
    require(descriptor["oce-api-version"] == package, "facts: package/descriptor disagreement")
    revision = int(one(compatibility, r"\brevision: ([0-9]+),"))
    profile = one(compatibility, r'execution_profile: "([^"]+)",')
    require(descriptor["oce-compatibility"] == str(revision)
            and descriptor["execution-profile"] == profile, "facts: descriptor disagreement")
    require("0 => ()," in codec and "UnsupportedExactness { tag }" in codec,
            "facts: exact-bits admission missing")
    require('w.write(&[0])?; // exact raw bits' in codec, "facts: exact-bits encoding missing")
    return {
        "package": package,
        "descriptor_revision": revision,
        "descriptor": descriptor,
        "domain_revisions": schemas,
        "state_format": int(one(state, r"const FORMAT_REVISION: u32 = ([0-9]+);")),
        "execution_abi": int(one(state, r"const EXECUTION_ABI_REVISION: u32 = ([0-9]+);")),
        "replay_format": int(one(codec, r"if revision != ([0-9]+) \{")),
        "replay_exactness": "ExactBits",
        "placement": ["Portable", "TargetBound(arch,os)"],
        "strict_bit_class": "retained-21-signal-linux-x86_64-aarch64-debug-release-only",
        "other_targets": "unsupported-unqualified",
    }


def outcome(artifact: str, producer: str, consumer: str) -> str:
    if producer == consumer == "current":
        return "accept"
    if producer == "historical" and artifact in UNAVAILABLE:
        return "unavailable-in-producer"
    if producer == consumer:
        return "unsupported-unqualified"
    if artifact in ("snapshot", "replay", "public-descriptor"):
        return "host-envelope-refusal-before-decode"
    return "unsupported-unqualified"


def candidate(root: Path) -> dict:
    history = read(root, HISTORY)
    require(digest(history) == HISTORY_DIGEST, "history: retained receipt changed")
    boundary = implementation(root)
    require(boundary["sha256"] == IMPLEMENTATION, "source: implementation differs from selected baseline")
    return {
        "revision": 1,
        "policy": "fail-closed-no-n-minus-one",
        "candidates": {
            "current": {"implementation_baseline": BASELINE, "implementation": boundary},
            "historical": {"tag": "v0.1.0", "tag_object": TAG, "source": HISTORICAL,
                           "kind": "historical-git-pin-not-published-not-supported-n-minus-one",
                           "package": "0.1.0", "receipt_sha256": digest(history)},
        },
        "current_facts": current_facts(root),
        "conditions": {
            "accept": "current/current only, within each delegated artifact contract and host qualification; not arbitrary build equality",
            "strict-bit-qualification": "only the retained finite Linux corpus; other targets and whole-executable exactness remain unqualified",
            "unavailable-in-producer": "the named current artifact is absent; not a decoder refusal or missing older equivalent",
            "host-envelope-refusal-before-decode": "host rejects cross-candidate bytes before any OCE decoder, restore or execution",
            "unsupported-unqualified": "no support or equivalence claim even when source compiles",
        },
        "rows": [{"artifact": artifact, "producer": producer, "consumer": consumer,
                  "status": outcome(artifact, producer, consumer)}
                 for artifact in ARTIFACTS for producer, consumer in DIRECTIONS],
        "typed_controls": {"status": "typed-refusal", "cases": {
            "snapshot-revision-one-not-from-v0.1.0": "EngineStateError::UnsupportedFormat { revision: 1 }",
            "snapshot-abi": "EngineStateError::IncompatibleExecution",
            "snapshot-placement": "EngineStateError::TargetDomainMismatch",
            "descriptor-field": "CompatibilityMismatch (exact first cause)",
            "replay-revision": "ReplayError::UnsupportedFormat",
            "replay-exactness": "ReplayError::UnsupportedExactness",
            "replay-descriptor": "ReplayError::DescriptorMismatch",
            "replay-placement": "ReplayError::TargetMismatch",
        }},
        "evidence": [{"path": path, "sha256": digest(read(root, path))} for path in EVIDENCE],
    }


def canonical(document: dict) -> bytes:
    """One reviewable section/row per line, fixed key order, UTF-8 and final LF."""
    lines = []
    for key, value in document.items():
        if isinstance(value, list):
            body = "[\n" + ",\n".join("    " + json.dumps(row, ensure_ascii=True) for row in value) + "\n  ]"
        else:
            body = json.dumps(value, ensure_ascii=True)
        lines.append(f'  "{key}": {body}')
    return ("{\n" + ",\n".join(lines) + "\n}\n").encode()


def verify_history(root: Path) -> None:
    require(git(root, "rev-parse", "v0.1.0").decode().strip() == TAG, "history: wrong tag object")
    require(git(root, "rev-parse", "v0.1.0^{}").decode().strip() == HISTORICAL,
            "history: wrong peeled source")
    require(git(root, "cat-file", "-t", TAG) == b"tag\n", "history: not annotated")
    require(implementation(root, BASELINE)["sha256"] == IMPLEMENTATION,
            "history: baseline implementation differs")
    receipt = json.loads(read(root, HISTORY))
    for path, expected in receipt["source_sha256"].items():
        require(digest(git(root, "show", f"{HISTORICAL}:{path}")) == expected,
                f"history: source differs: {path}")
    baseline = git(root, "show", f"{HISTORICAL}:crates/oce-api/tests/public-api.txt").decode()
    require("pub fn oce_api::Engine<S>::tick(" in baseline, "history: retired execution not found")
    for symbol in ("EngineStateSnapshot", "ReplayRecord", "CompatibilityDescriptor", "PreparedInputFrame",
                   "DiagnosticReceipt", "ContractDescriptor"):
        require(f"oce_api::{symbol}" not in baseline, f"history: assumed absent surface exists: {symbol}")
    package = tomllib.loads(git(root, "show", f"{HISTORICAL}:Cargo.toml").decode())
    require(package["workspace"]["package"]["version"] == "0.1.0", "history: package version differs")


def validate(root: Path) -> str:
    enforce(read(root, MATRIX), canonical(candidate(root)))
    return "release compatibility: OK (36 directed rows; current/current only; no N-1)\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", action="store_true")
    parser.add_argument("--verify-history", action="store_true")
    parser.add_argument("--source-boundary", action="store_true",
                        help="print live and selected-baseline boundaries for manual review")
    args = parser.parse_args()
    try:
        if args.source_boundary:
            print(json.dumps({"live": implementation(ROOT),
                              "baseline": implementation(ROOT, BASELINE)}))
            return 0
        if args.verify_history:
            verify_history(ROOT)
        if args.candidate:
            sys.stdout.buffer.write(canonical(candidate(ROOT)))
        else:
            print(validate(ROOT), end="")
    except (Refusal, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"release compatibility: FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
