#!/usr/bin/env python3
"""Validate the workspace package, feature, and publication structure from live Cargo facts.

Every workspace member is either private (`publish = false`) or registry-ready: its normal and
build dependencies on other members are path-backed with a registry version, and none of them is
private. The `oce-api` feature selections must keep one shared normal closure with no database or
async runtime, and the release workflow bytes stay approved. Publication state comes from each
crate's `Cargo.toml`; nothing else restates it before 1.0.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

import release_workflow


ROOT = Path(__file__).resolve().parents[2]
RELEASE_WORKFLOW = ".github/workflows/release.yml"
FEATURE_PACKAGE = "oce-api"
EXPECTED_FEATURES = {"default": ["mem"], "mem": []}
EXPECTED_SELECTIONS = {
    "default": ([], ["default", "mem"]),
    "explicit-mem": (["--no-default-features", "--features", "mem"], ["mem"]),
    "no-default-features": (["--no-default-features"], []),
}
REQUIRED_CLOSURE_MEMBER = "oce-store-mem"
FORBIDDEN_DEPENDENCIES = ["async-std", "selene-db", "tokio"]
UNKNOWN_FEATURE_CONTROL = "unsupported-control"


class PolicyError(ValueError):
    """A deterministic package-policy contract violation."""


def require_list(value: Any, label: str) -> list[Any]:
    """Require a JSON array."""

    if not isinstance(value, list):
        raise PolicyError(f"{label} must be an array")
    return value


def require_nonempty_string(value: Any, label: str) -> str:
    """Require a non-empty JSON string."""

    if not isinstance(value, str) or not value.strip():
        raise PolicyError(f"{label} must be a non-empty string")
    return value


def cargo_json(arguments: list[str]) -> dict[str, Any]:
    """Run a Cargo JSON query from the repository root."""

    command = ["cargo", *arguments]
    completed = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
    if completed.returncode != 0:
        raise PolicyError(
            f"{' '.join(command)} failed ({completed.returncode}): {completed.stderr.strip()}"
        )
    try:
        parsed = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise PolicyError(f"{' '.join(command)} emitted invalid JSON: {error}") from error
    if not isinstance(parsed, dict):
        raise PolicyError("cargo metadata root must be an object")
    return parsed


def metadata_package_map(metadata: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Return the exact workspace package map from Cargo metadata."""

    member_ids = set(require_list(metadata.get("workspace_members"), "workspace_members"))
    packages = require_list(metadata.get("packages"), "metadata packages")
    workspace_packages = [package for package in packages if package.get("id") in member_ids]
    by_name: dict[str, dict[str, Any]] = {}
    for package in workspace_packages:
        name = require_nonempty_string(package.get("name"), "metadata package name")
        if name in by_name:
            raise PolicyError(f"duplicate Cargo workspace package name: {name}")
        by_name[name] = package
    if len(by_name) != len(member_ids):
        raise PolicyError("Cargo workspace members do not map one-to-one to package names")
    return by_name


def dependency_graph(
    packages: dict[str, dict[str, Any]], kinds: set[str | None]
) -> dict[str, set[str]]:
    """Build a workspace dependency graph from Cargo metadata declarations."""

    graph = {name: set() for name in packages}
    for name, package in packages.items():
        for dependency in require_list(package.get("dependencies"), f"{name} dependencies"):
            dependency_name = dependency.get("name")
            if dependency.get("kind") in kinds and dependency_name in packages:
                graph[name].add(dependency_name)
    return graph


def transitive_dependencies(graph: dict[str, set[str]], start: str) -> set[str]:
    """Return every dependency reachable from one package."""

    reached: set[str] = set()
    pending = list(graph[start])
    while pending:
        dependency = pending.pop()
        if dependency not in reached:
            reached.add(dependency)
            pending.extend(graph[dependency])
    return reached


def derived_publish_order(graph: dict[str, set[str]], publishable: set[str]) -> list[str]:
    """Derive a deterministic leaf-to-dependent publication order."""

    remaining = {name: graph[name] & publishable for name in publishable}
    order: list[str] = []
    while remaining:
        ready = sorted(name for name, dependencies in remaining.items() if not dependencies)
        if not ready:
            raise PolicyError("publishable normal/build dependency graph contains a cycle")
        order.extend(ready)
        for name in ready:
            del remaining[name]
        for dependencies in remaining.values():
            dependencies.difference_update(ready)
    return order


def publish_flag(name: str, package: dict[str, Any]) -> bool:
    """Return whether Cargo would publish one member to crates.io."""

    publish_field = package.get("publish")
    if publish_field is None:
        return True
    if publish_field == []:
        return False
    raise PolicyError(f"{name} has a registry-restricted publish value: {publish_field!r}")


def validate_workspace(metadata: dict[str, Any]) -> dict[str, Any]:
    """Validate publish flags, dependency safety, the oce-api features, and ordering.

    Returns the observed `packages` map, the `publishable` and `private` name sets, the derived
    leaf-to-facade publish `order`, and the `api_closure` of workspace members `oce-api` reaches
    through normal dependencies.
    """

    packages = metadata_package_map(metadata)
    for name in packages:
        if not name.startswith("oce-"):
            raise PolicyError(f"workspace member outside the oce-* namespace: {name}")
    publishable = {name for name, package in packages.items() if publish_flag(name, package)}
    private = set(packages) - publishable

    feature_package = packages.get(FEATURE_PACKAGE)
    if feature_package is None:
        raise PolicyError(f"workspace has no {FEATURE_PACKAGE} member")
    if feature_package.get("features") != EXPECTED_FEATURES:
        raise PolicyError(f"Cargo-declared {FEATURE_PACKAGE} features drifted")
    if FEATURE_PACKAGE not in publishable:
        raise PolicyError(f"{FEATURE_PACKAGE} must stay registry-ready")

    release_graph = dependency_graph(packages, {None, "build"})
    normal_graph = dependency_graph(packages, {None})
    for source in sorted(publishable):
        leaked = transitive_dependencies(release_graph, source) & private
        if leaked:
            raise PolicyError(f"private normal/build dependency leakage from {source}: {sorted(leaked)}")
        for dependency in require_list(packages[source].get("dependencies"), f"{source} dependencies"):
            target = dependency.get("name")
            if dependency.get("kind") not in {None, "build"} or target not in packages:
                continue
            if not dependency.get("path"):
                raise PolicyError(f"workspace dependency {source} -> {target} is not path-backed")
            requirement = dependency.get("req")
            if not isinstance(requirement, str) or not requirement or requirement == "*":
                raise PolicyError(
                    f"publishable path dependency {source} -> {target} lacks a registry version"
                )

    api_closure = transitive_dependencies(normal_graph, FEATURE_PACKAGE)
    if REQUIRED_CLOSURE_MEMBER not in api_closure:
        raise PolicyError(f"{FEATURE_PACKAGE} normal closure must include {REQUIRED_CLOSURE_MEMBER}")
    return {
        "packages": packages,
        "publishable": publishable,
        "private": private,
        "order": derived_publish_order(release_graph, publishable),
        "api_closure": sorted(api_closure),
    }


def cargo_tree_observation(package: str, cargo_args: list[str]) -> dict[str, list[str]]:
    """Observe one selected package's normal closure and enabled root features."""

    command = [
        "cargo",
        "tree",
        "-p",
        package,
        "--locked",
        "-e",
        "normal",
        "--prefix",
        "none",
        "--format",
        "{p}|{f}",
        *cargo_args,
    ]
    completed = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
    if completed.returncode != 0:
        raise PolicyError(
            f"{' '.join(command)} failed ({completed.returncode}): {completed.stderr.strip()}"
        )
    rows: list[tuple[str, list[str]]] = []
    for line in completed.stdout.splitlines():
        display, separator, encoded_features = line.partition("|")
        if not separator or not display.split():
            raise PolicyError(f"unparseable cargo tree row: {line!r}")
        features = sorted(filter(None, encoded_features.split(",")))
        rows.append((display.split()[0], features))
    roots = [features for name, features in rows if name == package]
    if len(roots) != 1:
        raise PolicyError(f"cargo tree returned {len(roots)} root rows for {package}")
    return {
        "enabled_features": roots[0],
        "workspace_closure": sorted({name for name, _ in rows if name.startswith("oce-") and name != package}),
        "all_packages": sorted({name for name, _ in rows}),
    }


def validate_feature_observations(
    observations: dict[str, dict[str, list[str]]], expected_closure: list[str], private: set[str]
) -> None:
    """Require all supported selections to retain one safe normal closure."""

    if set(observations) != set(EXPECTED_SELECTIONS):
        raise PolicyError("feature observation coverage differs from supported selections")
    observed_closures: set[tuple[str, ...]] = set()
    for name in sorted(EXPECTED_SELECTIONS):
        observation = observations[name]
        if observation["enabled_features"] != EXPECTED_SELECTIONS[name][1]:
            raise PolicyError(f"enabled feature drift for {name}")
        if observation["workspace_closure"] != expected_closure:
            raise PolicyError(f"normal OCE feature closure drift for {name}")
        leakage = set(observation["workspace_closure"]) & private
        if leakage:
            raise PolicyError(f"private feature-closure leakage for {name}: {sorted(leakage)}")
        forbidden = set(observation["all_packages"]) & set(FORBIDDEN_DEPENDENCIES)
        if forbidden:
            raise PolicyError(f"forbidden normal dependency for {name}: {sorted(forbidden)}")
        observed_closures.add(tuple(observation["workspace_closure"]))
    if len(observed_closures) != 1:
        raise PolicyError("supported feature selections do not have equal normal OCE closures")


def validate_unknown_feature_refusal() -> None:
    """Require Cargo to reject an unclassified oce-api feature spelling."""

    command = [
        "cargo",
        "tree",
        "-p",
        "oce-api",
        "--locked",
        "--no-default-features",
        "--features",
        UNKNOWN_FEATURE_CONTROL,
    ]
    completed = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
    diagnostic = completed.stderr.lower()
    if completed.returncode == 0 or UNKNOWN_FEATURE_CONTROL not in diagnostic or "feature" not in diagnostic:
        raise PolicyError("Cargo did not specifically refuse the unknown oce-api feature control")


def validate_release_workflow(workflow: bytes) -> None:
    """Require the approved workflow bytes without interpreting execution syntax."""

    try:
        release_workflow.validate(workflow)
    except release_workflow.WorkflowError as error:
        raise PolicyError(str(error)) from error


def main() -> int:
    """Validate the workspace structure against live Cargo and workflow facts."""

    try:
        metadata = cargo_json(["metadata", "--format-version", "1", "--locked", "--no-deps"])
        workspace = validate_workspace(metadata)
        observations = {
            name: cargo_tree_observation(FEATURE_PACKAGE, cargo_args)
            for name, (cargo_args, _) in EXPECTED_SELECTIONS.items()
        }
        validate_feature_observations(observations, workspace["api_closure"], workspace["private"])
        validate_unknown_feature_refusal()
        validate_release_workflow((ROOT / RELEASE_WORKFLOW).read_bytes())
    except (OSError, PolicyError) as error:
        print(f"package publication contract: FAIL: {error}", file=sys.stderr)
        return 1

    print(
        "package publication contract: PASS "
        f"({len(workspace['packages'])} members; {len(workspace['publishable'])} publishable; "
        f"{len(workspace['private'])} private)"
    )
    print("derived leaf-to-facade publish order: " + " -> ".join(workspace["order"]))
    print("supported oce-api normal OCE closure: " + ", ".join(workspace["api_closure"]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
