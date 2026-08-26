#!/usr/bin/env python3
"""Render and verify the dated Open Control Engine stability baseline."""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import stat
import subprocess
import sys
import tomllib
from typing import Any, NoReturn


CAPTURE_DATE = "2026-08-26"
ROOT = pathlib.Path(__file__).resolve().parents[2]
ARTIFACT = ROOT / "docs" / "stability-baseline-2026-08-26.json"
MAX_ARTIFACT_BYTES = 128 * 1024
SHA_PATTERN = re.compile(r"^[0-9a-f]{40}$")
LOCAL_PATH_PATTERN = re.compile(r"(?:/Users/|/home/)")


class BaselineError(ValueError):
    """A closed-schema, canonicalization, or source verification failure."""


def fail(detail: str) -> NoReturn:
    """Raise a consistently typed verifier failure."""
    raise BaselineError(f"stability baseline verification failed: {detail}")


def source(repository: str, revision: str, *paths: str) -> dict[str, Any]:
    """Build an exact-revision source locator."""
    return {
        "kind": "git_paths_at_exact_revision",
        "repository": repository,
        "revision": revision,
        "paths": list(paths),
        "captured_on": CAPTURE_DATE,
    }


def github_observation(repository: str, endpoint: str) -> dict[str, str]:
    """Build a dated GitHub API observation locator."""
    return {
        "kind": "github_api_observation",
        "repository": repository,
        "endpoint": endpoint,
        "captured_on": CAPTURE_DATE,
    }


def captured_ref(branch: str, commit: str, tree: str, role: str) -> dict[str, str]:
    """Build one dated branch observation with exact commit and tree identity."""
    return {
        "branch": branch,
        "role": role,
        "commit": commit,
        "tree": tree,
        "captured_on": CAPTURE_DATE,
    }


def snapshot() -> dict[str, Any]:
    """Return the fixed facts captured from primary sources on 2026-08-26."""
    oce_repository = "jscott3201/open-control-engine"
    oce_development = "41e997fd130c5e454446b40bcc3ba576429876b4"
    oce_tree = "f0a3162736c2a2d51120b671cdaa65d7ad197a2e"
    studio_repository = "jscott3201/open-control-studio"
    studio_commit = "e0caf54c47d6b15f5d89fd7c2f05d82ba8fed326"
    library_repository = "jscott3201/open-control-library"
    library_commit = "f8f82ce5dcf613e22c25798125aa45654b2050e8"
    sim_repository = "jscott3201/open-control-sim"
    sim_commit = "1df1c32d2740a6736bb69c0f6c3a01289f3c10f5"
    cxf_repository = "jscott3201/cxf-json"
    cxf_commit = "6edbddea17c2f57456c2d431db6449eb90ec20d8"

    return {
        "schema": "oce-stability-baseline-v1",
        "captured_on": CAPTURE_DATE,
        "classification": "dated_historical_evidence",
        "authority": {
            "precedence": "Live repository source and repository policy remain authoritative.",
            "purpose": "This artifact records evidence; it does not create product or program authority.",
            "supersession_rule": "Never rewrite this capture silently; add a new dated capture that names what it supersedes.",
            "dated_input": {
                "location": "_spec/open-control-engine-2026-08-25/CURRENT-BASELINE.md",
                "availability": "local_only_not_present_in_clean_checkout",
                "treatment": "superseded_as_baseline_input_by_this_2026-08-26_capture",
                "quoted_differences": [
                    "Studio product main moved from c756d320aa495ddd66630f0987202c6d852f27f5 to e0caf54c47d6b15f5d89fd7c2f05d82ba8fed326.",
                    "Studio's selected program locator moved from 5bc001ff9dbf980e5fb7f106e2d5eb9329e842c3 to 5dd50c97472020142503ec8fdb79823fc0e56c78.",
                    "The program repository head moved from 5dd50c97472020142503ec8fdb79823fc0e56c78 to e54e7c3e63e3b5a78abd38402b6d12b6d561758f.",
                ],
            },
        },
        "open_control_engine": {
            "repository": oce_repository,
            "refs": {
                "development": captured_ref(
                    "development", oce_development, oce_tree, "default_integration"
                ),
                "main": captured_ref(
                    "main",
                    "63f3f2ab3ec478c5b0a023848c4f8da0dba48796",
                    "61ba7a25e42ecf70ee114539402dbbd099db3e69",
                    "release",
                ),
            },
            "relationship": {
                "classification": "diverged",
                "merge_base": "c3377250b55bf5f3d2018f20035daba698877999",
                "development_ahead": 6,
                "development_behind": 5,
                "captured_on": CAPTURE_DATE,
                "source": github_observation(
                    oce_repository,
                    "compare/63f3f2ab3ec478c5b0a023848c4f8da0dba48796...41e997fd130c5e454446b40bcc3ba576429876b4",
                ),
            },
            "package": {
                "workspace_version": "0.1.0",
                "declared_msrv": "1.97.0",
                "pinned_toolchain": "1.97.1",
                "facade_package": "oce-api",
                "source": source(
                    oce_repository,
                    oce_development,
                    "Cargo.toml",
                    "rust-toolchain.toml",
                    "crates/oce-api/Cargo.toml",
                ),
            },
            "publication": {
                "tag": {
                    "name": "v0.1.0",
                    "target_commit": "909a8ba699e6a2fccf3de6ac0616a9e83a04060f",
                    "captured_on": CAPTURE_DATE,
                    "source": github_observation(oce_repository, "git/ref/tags/v0.1.0"),
                },
                "github_releases": {
                    "state": "none_at_capture",
                    "captured_on": CAPTURE_DATE,
                    "source": github_observation(oce_repository, "releases"),
                },
                "tag_release_distinction": "A Git tag exists; the GitHub Releases list was empty.",
            },
            "repository_state": {
                "open_pull_requests": {
                    "state": "known",
                    "numbers": [],
                    "captured_on": CAPTURE_DATE,
                    "source": github_observation(oce_repository, "pulls?state=open"),
                },
                "open_issues": {
                    "state": "known",
                    "issues": [
                        {
                            "number": number,
                            "state": "open",
                            "assignees": [],
                            "program_owner": "UNKNOWN",
                        }
                        for number in (242, 249, 250, 254)
                    ],
                    "captured_on": CAPTURE_DATE,
                    "source": github_observation(oce_repository, "issues?state=open"),
                },
                "branch_protection_and_rulesets": {
                    "state": "UNKNOWN",
                    "reason": "Available API evidence was authorization-limited; no absence of protection is inferred.",
                    "admin_verification": "A repository administrator must inspect Settings > Rules > Rulesets and Settings > Branches, then record every active rule applying to development and main.",
                    "captured_on": CAPTURE_DATE,
                },
            },
        },
        "downstream_and_adjacent": {
            "studio": {
                "product": {
                    "repository": studio_repository,
                    "branch": "main",
                    "commit": studio_commit,
                    "captured_on": CAPTURE_DATE,
                    "semantics": "The product revision that defines reviewed Studio behavior.",
                },
                "selected_program_locator": {
                    "repository": "jscott3201/open-control-studio-program",
                    "commit": "5dd50c97472020142503ec8fdb79823fc0e56c78",
                    "captured_on": CAPTURE_DATE,
                    "semantics": "The immutable program revision selected by product source.",
                    "source": source(
                        studio_repository, studio_commit, "docs/roadmap/README.md"
                    ),
                },
                "program_repository_head": {
                    "repository": "jscott3201/open-control-studio-program",
                    "branch": "main",
                    "commit": "e54e7c3e63e3b5a78abd38402b6d12b6d561758f",
                    "captured_on": CAPTURE_DATE,
                    "semantics": "Newer program work; it does not define product behavior without a reviewed locator change.",
                },
                "oce_dependencies": {
                    "state": "pinned",
                    "dependencies": [
                        {
                            "package": package,
                            "commit": oce_development,
                            "tree": oce_tree,
                            "captured_on": CAPTURE_DATE,
                        }
                        for package in ("oce-api", "oce-blocks")
                    ],
                    "source": source(studio_repository, studio_commit, "Cargo.toml"),
                },
                "open_pull_requests": {
                    "scope": "downstream_not_oce",
                    "pull_requests": [
                        {
                            "number": 28,
                            "state": "open",
                            "head_branch": "deliver/m00-pr04-reconciled",
                            "head_commit": "00dc45a22fe7a906c0eba4e7c85db37bb9e8d000",
                            "base_branch": "main",
                            "base_commit": studio_commit,
                        }
                    ],
                    "captured_on": CAPTURE_DATE,
                    "source": github_observation(studio_repository, "pulls/28"),
                },
            },
            "library": {
                "repository": library_repository,
                "ref": captured_ref("main", library_commit, "f7fdb344141926588ce1aed3fcb2117691d35ed1", "default"),
                "oce_dependency": {
                    "state": "pinned",
                    "declaration": "ENGINE_PIN",
                    "commit": "e2ff2f84577d9be65a49e6cb5440c223f6126817",
                    "tree": oce_tree,
                    "captured_on": CAPTURE_DATE,
                    "source": source(library_repository, library_commit, "ENGINE_PIN"),
                    "identity_note": "The declared commit differs from OCE development even though their tree identities are equal.",
                },
            },
            "sim": {
                "repository": sim_repository,
                "ref": captured_ref("development", sim_commit, "ed2a3b25f6c52e9df1d9aa3b9caa6c838bb01075", "default"),
                "oce_dependency": {
                    "state": "none_at_revision",
                    "captured_on": CAPTURE_DATE,
                    "evidence": "No active OCE dependency; ocs-bridge is an empty scaffold and its future dependency is commented out.",
                    "source": source(
                        sim_repository,
                        sim_commit,
                        "Cargo.toml",
                        "crates/ocs-bridge/Cargo.toml",
                        "crates/ocs-bridge/src/lib.rs",
                    ),
                },
            },
            "cxf_json": {
                "repository": cxf_repository,
                "ref": captured_ref("main", cxf_commit, "c9e304bc375a436cff7ae513ebb0469c3ee252a3", "default"),
                "oce_dependency": {
                    "state": "none_at_revision",
                    "captured_on": CAPTURE_DATE,
                    "evidence": "No OCE dependency; this is adjacent CXF contract groundwork, not a supported OCE parser or adapter.",
                    "source": source(cxf_repository, cxf_commit, "Cargo.toml", "crates/*/Cargo.toml"),
                },
            },
        },
    }


def render(document: dict[str, Any] | None = None) -> bytes:
    """Serialize a document in the sole canonical byte representation."""
    value = snapshot() if document is None else document
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def pairs(values: list[tuple[str, Any]]) -> dict[str, Any]:
    """Reject duplicate JSON object keys while parsing."""
    output: dict[str, Any] = {}
    for key, value in values:
        if key in output:
            fail(f"duplicate JSON key {key!r}")
        output[key] = value
    return output


def parse(payload: bytes) -> dict[str, Any]:
    """Parse bounded UTF-8 JSON while rejecting duplicate keys."""
    if len(payload) > MAX_ARTIFACT_BYTES:
        fail(f"artifact exceeds {MAX_ARTIFACT_BYTES} bytes")
    try:
        text = payload.decode("utf-8")
    except UnicodeDecodeError as error:
        fail(f"artifact is not UTF-8: {error}")
    try:
        value = json.loads(text, object_pairs_hook=pairs)
    except json.JSONDecodeError as error:
        fail(f"artifact is not JSON: {error}")
    if not isinstance(value, dict):
        fail("artifact root must be an object")
    return value


def validate_shape(actual: Any, expected: Any, location: str = "$") -> None:
    """Validate a recursively closed schema without accepting extra fields."""
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or list(actual) != list(expected):
            fail(f"{location} fields or field order do not match the closed schema")
        for key in expected:
            validate_shape(actual[key], expected[key], f"{location}.{key}")
        return
    if isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            fail(f"{location} list shape does not match the closed schema")
        for index, (item, expected_item) in enumerate(zip(actual, expected)):
            validate_shape(item, expected_item, f"{location}[{index}]")
        return
    if type(actual) is not type(expected):
        fail(f"{location} has the wrong scalar type")


def walk(value: Any, location: str = "$"):
    """Yield every scalar together with its key and document location."""
    if isinstance(value, dict):
        for key, child in value.items():
            if isinstance(child, (dict, list)):
                yield from walk(child, f"{location}.{key}")
            else:
                yield key, child, f"{location}.{key}"
    elif isinstance(value, list):
        for index, child in enumerate(value):
            if isinstance(child, (dict, list)):
                yield from walk(child, f"{location}[{index}]")
            else:
                yield "", child, f"{location}[{index}]"


def validate(document: dict[str, Any]) -> None:
    """Validate closed schema and cross-field identity invariants."""
    expected = snapshot()
    validate_shape(document, expected)
    sha_keys = {
        "commit",
        "tree",
        "merge_base",
        "target_commit",
        "head_commit",
        "base_commit",
        "revision",
    }
    for key, value, location in walk(document):
        if key in sha_keys and (not isinstance(value, str) or not SHA_PATTERN.fullmatch(value)):
            fail(f"{location} must be a full lowercase 40-hex SHA")
        if isinstance(value, str) and LOCAL_PATH_PATTERN.search(value):
            fail(f"{location} contains an absolute local path")

    studio = document["downstream_and_adjacent"]["studio"]
    identities = {
        studio["product"]["commit"],
        studio["selected_program_locator"]["commit"],
        studio["program_repository_head"]["commit"],
    }
    if len(identities) != 3:
        fail("Studio product, selected locator, and program head must remain distinct")

    baseline = document["open_control_engine"]["refs"]["development"]
    dependencies = studio["oce_dependencies"]
    if dependencies["state"] != "pinned":
        fail("Studio OCE dependency state must be pinned")
    if [item["package"] for item in dependencies["dependencies"]] != ["oce-api", "oce-blocks"]:
        fail("Studio must retain separate oce-api and oce-blocks pin records")
    for item in dependencies["dependencies"]:
        if (item["commit"], item["tree"]) != (baseline["commit"], baseline["tree"]):
            fail(f"Studio {item['package']} commit/tree does not match the captured pin")

    library = document["downstream_and_adjacent"]["library"]["oce_dependency"]
    if library["state"] != "pinned" or library["commit"] == baseline["commit"]:
        fail("Library must retain its distinct declared OCE commit identity")
    if library["tree"] != baseline["tree"]:
        fail("Library OCE pin tree must match its captured OCE tree identity")

    for name in ("sim", "cxf_json"):
        dependency = document["downstream_and_adjacent"][name]["oce_dependency"]
        if dependency["state"] != "none_at_revision":
            fail(f"{name} no-pin evidence must remain distinct from unknown")
        if "commit" in dependency or "tree" in dependency:
            fail(f"{name} no-pin evidence cannot carry a dependency identity")

    state = document["open_control_engine"]["repository_state"]
    if state["open_pull_requests"]["numbers"] != []:
        fail("OCE open pull requests must remain separate from downstream PR state")
    if [issue["number"] for issue in state["open_issues"]["issues"]] != [242, 249, 250, 254]:
        fail("OCE open issue inventory is incomplete or reordered")
    for issue in state["open_issues"]["issues"]:
        if issue["assignees"] or issue["program_owner"] != "UNKNOWN":
            fail("issue assignment and program ownership cannot be inferred")
    if state["branch_protection_and_rulesets"]["state"] != "UNKNOWN":
        fail("authorization-limited repository policy must remain UNKNOWN")


def check_payload(payload: bytes) -> None:
    """Require valid, canonical bytes equal to the fixed dated capture."""
    document = parse(payload)
    validate(document)
    if payload != render(document):
        fail("artifact serialization is not canonical")
    if payload != render():
        fail("artifact facts differ from the fixed 2026-08-26 capture")


def read_regular(path: pathlib.Path) -> bytes:
    """Read one bounded regular non-symlink artifact."""
    try:
        metadata = path.lstat()
    except OSError as error:
        fail(f"cannot stat artifact: {error}")
    if path.is_symlink() or not stat.S_ISREG(metadata.st_mode):
        fail("artifact must be a regular non-symlink file")
    if metadata.st_size > MAX_ARTIFACT_BYTES:
        fail(f"artifact exceeds {MAX_ARTIFACT_BYTES} bytes")
    return path.read_bytes()


def git(repository: pathlib.Path, *arguments: str) -> str:
    """Run one bounded, non-network Git source query."""
    try:
        result = subprocess.run(
            ["git", "-C", os.fspath(repository), *arguments],
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        fail(f"Git source query could not run: {error}")
    if result.returncode != 0:
        detail = result.stderr.strip().splitlines()
        fail(f"Git source query failed: {detail[-1] if detail else 'no diagnostic'}")
    return result.stdout


def show(repository: pathlib.Path, revision: str, path: str) -> str:
    """Read a tracked path from an exact commit without consulting checkout HEAD."""
    return git(repository, "show", f"{revision}:{path}")


def require_commit(repository: pathlib.Path, commit: str) -> None:
    """Require an exact object to exist and be a commit."""
    if git(repository, "cat-file", "-t", commit).strip() != "commit":
        fail(f"source object {commit} is not a commit")


def require_tree(repository: pathlib.Path, commit: str, tree: str) -> None:
    """Require an exact commit to resolve to the captured tree identity."""
    require_commit(repository, commit)
    actual = git(repository, "rev-parse", f"{commit}^{{tree}}").strip()
    if actual != tree:
        fail(f"commit {commit} resolves to tree {actual}, expected {tree}")


def verify_oce_source(repository: pathlib.Path) -> None:
    """Verify local OCE object topology and exact-revision package claims."""
    document = snapshot()["open_control_engine"]
    development = document["refs"]["development"]
    main = document["refs"]["main"]
    require_tree(repository, development["commit"], development["tree"])
    require_tree(repository, main["commit"], main["tree"])
    relationship = document["relationship"]
    merge_base = git(
        repository, "merge-base", development["commit"], main["commit"]
    ).strip()
    if merge_base != relationship["merge_base"]:
        fail("captured OCE merge base does not match the exact commits")
    counts = git(
        repository,
        "rev-list",
        "--left-right",
        "--count",
        f"{main['commit']}...{development['commit']}",
    ).split()
    if counts != [str(relationship["development_behind"]), str(relationship["development_ahead"])]:
        fail("captured OCE divergence counts do not match the exact commits")

    package = document["package"]
    root_manifest = tomllib.loads(show(repository, development["commit"], "Cargo.toml"))
    workspace_package = root_manifest["workspace"]["package"]
    if workspace_package["version"] != package["workspace_version"]:
        fail("workspace version differs at the captured OCE revision")
    if workspace_package["rust-version"] != package["declared_msrv"]:
        fail("declared MSRV differs at the captured OCE revision")
    toolchain = tomllib.loads(
        show(repository, development["commit"], "rust-toolchain.toml")
    )
    if toolchain["toolchain"]["channel"] != package["pinned_toolchain"]:
        fail("pinned toolchain differs at the captured OCE revision")
    facade = tomllib.loads(
        show(repository, development["commit"], "crates/oce-api/Cargo.toml")
    )
    if facade["package"]["name"] != package["facade_package"]:
        fail("facade package differs at the captured OCE revision")

    require_commit(repository, document["publication"]["tag"]["target_commit"])
    library_pin = snapshot()["downstream_and_adjacent"]["library"]["oce_dependency"]
    require_tree(repository, library_pin["commit"], library_pin["tree"])


def dependency_names(manifest: Any):
    """Yield active dependency names from all TOML dependency tables."""
    if not isinstance(manifest, dict):
        return
    for key, value in manifest.items():
        if key in {"dependencies", "dev-dependencies", "build-dependencies"}:
            if isinstance(value, dict):
                for dependency, declaration in value.items():
                    yield dependency
                    if isinstance(declaration, dict) and isinstance(declaration.get("package"), str):
                        yield declaration["package"]
                    if isinstance(declaration, dict) and isinstance(declaration.get("git"), str):
                        yield declaration["git"]
        elif isinstance(value, dict):
            yield from dependency_names(value)


def require_no_oce_dependencies(repository: pathlib.Path, revision: str) -> None:
    """Require every exact-revision Cargo manifest to contain no active OCE dependency."""
    paths = [
        path
        for path in git(repository, "ls-tree", "-r", "--name-only", revision).splitlines()
        if pathlib.PurePosixPath(path).name == "Cargo.toml"
    ]
    if not paths:
        fail("source revision contains no Cargo manifests")
    for path in paths:
        manifest = tomllib.loads(show(repository, revision, path))
        names = list(dependency_names(manifest))
        if any(name.startswith("oce-") or "open-control-engine" in name for name in names):
            fail(f"active OCE dependency found in {path}")


def verify_downstream_source(name: str, repository: pathlib.Path) -> None:
    """Verify one downstream claim from an explicit exact-object checkout."""
    downstream = snapshot()["downstream_and_adjacent"]
    if name == "open-control-studio":
        record = downstream["studio"]
        revision = record["product"]["commit"]
        require_commit(repository, revision)
        manifest = tomllib.loads(show(repository, revision, "Cargo.toml"))
        dependencies = manifest["workspace"]["dependencies"]
        expected = record["oce_dependencies"]["dependencies"]
        for item in expected:
            declaration = dependencies.get(item["package"])
            if (
                not isinstance(declaration, dict)
                or declaration.get("git")
                != "https://github.com/jscott3201/open-control-engine"
                or declaration.get("rev") != item["commit"]
            ):
                fail(f"Studio {item['package']} declaration differs at the captured revision")
        roadmap = show(repository, revision, "docs/roadmap/README.md")
        if record["selected_program_locator"]["commit"] not in roadmap:
            fail("Studio selected program locator differs at the captured revision")
        return
    if name == "open-control-studio-program":
        require_commit(repository, downstream["studio"]["program_repository_head"]["commit"])
        return
    if name == "open-control-library":
        record = downstream["library"]
        revision = record["ref"]["commit"]
        require_tree(repository, revision, record["ref"]["tree"])
        pin = show(repository, revision, "ENGINE_PIN").strip()
        if pin != record["oce_dependency"]["commit"]:
            fail("Library ENGINE_PIN differs at the captured revision")
        return
    if name == "open-control-sim":
        record = downstream["sim"]
        revision = record["ref"]["commit"]
        require_tree(repository, revision, record["ref"]["tree"])
        require_no_oce_dependencies(repository, revision)
        bridge = show(repository, revision, "crates/ocs-bridge/src/lib.rs")
        if "Scaffold status: empty" not in bridge:
            fail("Sim ocs-bridge is not the captured empty scaffold")
        return
    if name == "cxf-json":
        record = downstream["cxf_json"]
        revision = record["ref"]["commit"]
        require_tree(repository, revision, record["ref"]["tree"])
        require_no_oce_dependencies(repository, revision)
        return
    fail(f"unsupported source checkout name {name!r}")


def source_argument(value: str) -> tuple[str, pathlib.Path]:
    """Parse NAME=PATH without resolving the path into generated output."""
    name, separator, path = value.partition("=")
    if not separator or not name or not path:
        raise argparse.ArgumentTypeError("source must be NAME=PATH")
    return name, pathlib.Path(path)


def write_artifact(path: pathlib.Path) -> None:
    """Write canonical bytes without replacing a symlink or non-regular file."""
    if path.exists() or path.is_symlink():
        metadata = path.lstat()
        if path.is_symlink() or not stat.S_ISREG(metadata.st_mode):
            fail("output must be absent or a regular non-symlink file")
    path.write_bytes(render())


def main(arguments: list[str] | None = None) -> int:
    """Run the deterministic artifact and optional exact-source verifier."""
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group()
    action.add_argument("--check", action="store_true", help="check the committed artifact (default)")
    action.add_argument("--write", action="store_true", help="write canonical artifact bytes")
    action.add_argument("--render", action="store_true", help="write canonical bytes to stdout")
    parser.add_argument("--artifact", type=pathlib.Path, default=ARTIFACT)
    parser.add_argument(
        "--source",
        action="append",
        default=[],
        type=source_argument,
        metavar="NAME=PATH",
        help="verify exact objects in an explicit local checkout; never fetches",
    )
    options = parser.parse_args(arguments)
    if options.render:
        if options.source:
            parser.error("--render cannot be combined with --source")
        sys.stdout.buffer.write(render())
        return 0
    try:
        if options.write:
            write_artifact(options.artifact)
        else:
            check_payload(read_regular(options.artifact))
        seen: set[str] = set()
        for name, path in options.source:
            if name in seen:
                fail(f"duplicate source checkout name {name!r}")
            seen.add(name)
            if name == "open-control-engine":
                verify_oce_source(path)
            else:
                verify_downstream_source(name, path)
    except (BaselineError, KeyError, OSError, tomllib.TOMLDecodeError) as error:
        print(error, file=sys.stderr)
        return 1
    print("stability baseline verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
