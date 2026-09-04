#!/usr/bin/env python3
"""Validate the closed workspace package, feature, and publication contract."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
LEDGER_PATH = ROOT / "docs" / "package-publication-ledger.json"

PACKAGE_CATEGORIES = (
    "host-facade",
    "conditional-adapter-port",
    "transitional-companion",
    "implementation-dependency",
    "test-support",
    "private-reference-adapter",
    "verification-tooling",
    "reserved-panic-only",
    "experimental-reserved",
)
EXPECTED_PACKAGE_CLASSIFICATIONS = {
    "oce-api": ("host-facade", True),
    "oce-bless": ("test-support", False),
    "oce-blocks": ("transitional-companion", True),
    "oce-conformance": ("verification-tooling", False),
    "oce-cxf": ("implementation-dependency", True),
    "oce-diag": ("implementation-dependency", True),
    "oce-docs": ("reserved-panic-only", False),
    "oce-expr": ("implementation-dependency", True),
    "oce-extension": ("experimental-reserved", False),
    "oce-flatten": ("implementation-dependency", True),
    "oce-graph": ("implementation-dependency", True),
    "oce-model": ("implementation-dependency", True),
    "oce-reference-wal-adapter": ("private-reference-adapter", False),
    "oce-semantics": ("implementation-dependency", True),
    "oce-store": ("conditional-adapter-port", True),
    "oce-store-mem": ("implementation-dependency", True),
    "oce-validate": ("implementation-dependency", True),
}
PUBLISHABLE_CATEGORIES = frozenset(PACKAGE_CATEGORIES[:4])
EXPECTED_SELECTIONS = {
    "default": ([], ["default", "mem"]),
    "explicit-mem": (["--no-default-features", "--features", "mem"], ["mem"]),
    "no-default-features": (["--no-default-features"], []),
}
EXPECTED_FEATURES = {"default": ["mem"], "mem": []}
FORBIDDEN_DEPENDENCIES = ["async-std", "selene-db", "tokio"]
UNKNOWN_FEATURE_CONTROL = "unsupported-control"


class PolicyError(ValueError):
    """A deterministic package-policy contract violation."""


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    """Build a JSON object while refusing duplicate keys."""

    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise PolicyError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_ledger(path: Path = LEDGER_PATH) -> dict[str, Any]:
    """Read the ledger with duplicate-key rejection."""

    parsed = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=reject_duplicate_keys)
    if not isinstance(parsed, dict):
        raise PolicyError("ledger root must be an object")
    return parsed


def exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    """Require one exact object shape."""

    actual = set(value)
    if actual != expected:
        raise PolicyError(
            f"{label} fields differ: missing={sorted(expected - actual)}, "
            f"extra={sorted(actual - expected)}"
        )


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


def validate_ledger(ledger: dict[str, Any]) -> dict[str, Any]:
    """Validate the ledger schema and its closed vocabularies."""

    exact_keys(
        ledger,
        {
            "schema",
            "authority",
            "publication_state",
            "package_categories",
            "packages",
            "feature_contract",
            "release_contract",
            "downstream_constraints",
        },
        "ledger",
    )
    if ledger["schema"] != "oce-package-publication-ledger/v1":
        raise PolicyError("unsupported ledger schema")
    if ledger["authority"] != "docs/package-publication-policy.md":
        raise PolicyError("ledger authority path drifted")
    if ledger["publication_state"] != "deferred-no-crates-published":
        raise PolicyError("publication state must remain explicitly deferred")
    if ledger["package_categories"] != list(PACKAGE_CATEGORIES):
        raise PolicyError("package category vocabulary drifted")

    packages = require_list(ledger["packages"], "packages")
    package_names: list[str] = []
    for index, item in enumerate(packages):
        if not isinstance(item, dict):
            raise PolicyError(f"packages[{index}] must be an object")
        exact_keys(item, {"name", "category", "publish"}, f"packages[{index}]")
        name = require_nonempty_string(item["name"], f"packages[{index}].name")
        category = require_nonempty_string(item["category"], f"packages[{index}].category")
        if category not in PACKAGE_CATEGORIES:
            raise PolicyError(f"invalid package category for {name}: {category}")
        if not isinstance(item["publish"], bool):
            raise PolicyError(f"publish classification for {name} must be boolean")
        if item["publish"] != (category in PUBLISHABLE_CATEGORIES):
            raise PolicyError(f"support/publication category mismatch for {name}")
        package_names.append(name)
    if len(package_names) != len(set(package_names)):
        raise PolicyError("duplicate workspace package classification")
    if package_names != sorted(package_names):
        raise PolicyError("package classifications must be name-sorted")
    actual_classifications = {
        item["name"]: (item["category"], item["publish"]) for item in packages
    }
    for name in sorted(set(EXPECTED_PACKAGE_CLASSIFICATIONS) | set(actual_classifications)):
        expected = EXPECTED_PACKAGE_CLASSIFICATIONS.get(name)
        actual = actual_classifications.get(name)
        if actual != expected:
            raise PolicyError(
                f"owner-approved package mapping drift for {name}: "
                f"expected={expected!r}, actual={actual!r}"
            )

    feature = ledger["feature_contract"]
    if not isinstance(feature, dict):
        raise PolicyError("feature_contract must be an object")
    exact_keys(
        feature,
        {
            "package",
            "declared_features",
            "selections",
            "normal_oce_dependency_closure",
            "forbidden_normal_dependency_names",
        },
        "feature_contract",
    )
    if feature["package"] != "oce-api" or feature["declared_features"] != EXPECTED_FEATURES:
        raise PolicyError("oce-api declared feature contract drifted")
    selections = require_list(feature["selections"], "feature selections")
    seen_selections: set[str] = set()
    for index, selection in enumerate(selections):
        if not isinstance(selection, dict):
            raise PolicyError(f"feature selection {index} must be an object")
        exact_keys(
            selection,
            {"name", "cargo_args", "enabled_features", "classification"},
            f"feature selection {index}",
        )
        name = require_nonempty_string(selection["name"], f"feature selection {index}.name")
        if name in seen_selections:
            raise PolicyError(f"duplicate feature selection classification: {name}")
        seen_selections.add(name)
        expected = EXPECTED_SELECTIONS.get(name)
        if expected is None:
            raise PolicyError(f"unsupported feature selection classification: {name}")
        if (selection["cargo_args"], selection["enabled_features"]) != expected:
            raise PolicyError(f"feature selection mechanics drifted: {name}")
        if selection["classification"] != "supported":
            raise PolicyError(f"feature selection must be supported: {name}")
    if seen_selections != set(EXPECTED_SELECTIONS):
        raise PolicyError("supported feature selection coverage is incomplete")

    closure = require_list(feature["normal_oce_dependency_closure"], "normal OCE closure")
    if closure != sorted(set(closure)) or "oce-store-mem" not in closure:
        raise PolicyError("normal OCE closure must be unique, sorted, and include oce-store-mem")
    if feature["forbidden_normal_dependency_names"] != FORBIDDEN_DEPENDENCIES:
        raise PolicyError("forbidden normal dependency vocabulary drifted")

    release = ledger["release_contract"]
    if not isinstance(release, dict):
        raise PolicyError("release_contract must be an object")
    exact_keys(
        release,
        {
            "workflow",
            "publishable_count",
            "private_count",
            "validator_command",
            "dry_run_command",
            "publish_command",
            "publish_event",
        },
        "release_contract",
    )
    expected_release = {
        "workflow": ".github/workflows/release.yml",
        "publishable_count": 12,
        "private_count": 5,
        "validator_command": "python3 scripts/package_policy/validate.py",
        "dry_run_command": "cargo publish --workspace --locked --dry-run",
        "publish_command": "cargo publish --workspace --locked",
        "publish_event": "workflow_dispatch",
    }
    if release != expected_release:
        raise PolicyError("release command, event, or count contract drifted")
    publish_count = sum(item["publish"] for item in packages)
    if publish_count != release["publishable_count"]:
        raise PolicyError("publishable package count disagrees with package classifications")
    if len(packages) - publish_count != release["private_count"]:
        raise PolicyError("private package count disagrees with package classifications")

    constraints = require_list(ledger["downstream_constraints"], "downstream_constraints")
    consumers: set[str] = set()
    for index, constraint in enumerate(constraints):
        if not isinstance(constraint, dict):
            raise PolicyError(f"downstream constraint {index} must be an object")
        exact_keys(
            constraint,
            {"consumer", "direct_packages", "constraint"},
            f"downstream constraint {index}",
        )
        consumer = require_nonempty_string(constraint["consumer"], "consumer")
        if consumer in consumers:
            raise PolicyError(f"duplicate downstream constraint: {consumer}")
        consumers.add(consumer)
        direct = require_list(constraint["direct_packages"], f"{consumer} direct_packages")
        if not direct or len(direct) != len(set(direct)) or not set(direct) <= set(package_names):
            raise PolicyError(f"invalid direct package constraint for {consumer}")
        require_nonempty_string(constraint["constraint"], f"{consumer} constraint")
    if not constraints:
        raise PolicyError("downstream migration constraints must not be empty")
    return ledger


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


def validate_workspace(
    ledger: dict[str, Any], metadata: dict[str, Any]
) -> tuple[dict[str, dict[str, Any]], list[str]]:
    """Validate Cargo membership, publish flags, dependency safety, and ordering."""

    packages = metadata_package_map(metadata)
    classified = {item["name"]: item for item in ledger["packages"]}
    if set(packages) != set(classified):
        raise PolicyError(
            f"workspace classification coverage differs: "
            f"missing={sorted(set(packages) - set(classified))}, "
            f"extra={sorted(set(classified) - set(packages))}"
        )

    for name, package in packages.items():
        publish_field = package.get("publish")
        if publish_field is None:
            actual_publish = True
        elif publish_field == []:
            actual_publish = False
        else:
            raise PolicyError(f"{name} has a registry-restricted publish value: {publish_field!r}")
        if actual_publish != classified[name]["publish"]:
            raise PolicyError(f"publish flag drift for {name}")
    publishable = {name for name, item in classified.items() if item["publish"]}
    private = set(packages) - publishable
    release = ledger["release_contract"]
    if (len(publishable), len(private)) != (
        release["publishable_count"],
        release["private_count"],
    ):
        raise PolicyError("Cargo publish/private counts drifted")

    feature_package = packages[ledger["feature_contract"]["package"]]
    if feature_package.get("features") != ledger["feature_contract"]["declared_features"]:
        raise PolicyError("Cargo-declared oce-api features disagree with the ledger")

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

    expected_api_closure = set(ledger["feature_contract"]["normal_oce_dependency_closure"])
    actual_api_closure = transitive_dependencies(normal_graph, "oce-api")
    if actual_api_closure != expected_api_closure:
        raise PolicyError(
            f"metadata oce-api normal closure drifted: expected={sorted(expected_api_closure)}, "
            f"actual={sorted(actual_api_closure)}"
        )
    return packages, derived_publish_order(release_graph, publishable)


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
    ledger: dict[str, Any], observations: dict[str, dict[str, list[str]]], private: set[str]
) -> None:
    """Require all supported selections to retain one safe normal closure."""

    feature = ledger["feature_contract"]
    expected_closure = feature["normal_oce_dependency_closure"]
    selection_map = {selection["name"]: selection for selection in feature["selections"]}
    if set(observations) != set(selection_map):
        raise PolicyError("feature observation coverage differs from supported selections")
    observed_closures: set[tuple[str, ...]] = set()
    for name in sorted(selection_map):
        observation = observations[name]
        if observation["enabled_features"] != selection_map[name]["enabled_features"]:
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


def workflow_job(workflow: str, name: str) -> list[str]:
    """Extract one top-level job body from the repository's fixed YAML style."""

    lines = workflow.splitlines()
    header = f"  {name}:"
    try:
        start = lines.index(header) + 1
    except ValueError as error:
        raise PolicyError(f"release workflow is missing job {name}") from error
    body: list[str] = []
    for line in lines[start:]:
        if re.match(r"^  [^ ]", line):
            break
        if not line.lstrip().startswith("#"):
            body.append(line.strip())
    return body


def validate_release_workflow(ledger: dict[str, Any], workflow: str) -> None:
    """Validate exact workspace selection and manual publication guards."""

    release = ledger["release_contract"]
    active = [line.strip() for line in workflow.splitlines() if not line.lstrip().startswith("#")]
    verify = workflow_job(workflow, "verify")
    publish = workflow_job(workflow, "publish")
    expected_runs = {
        f"run: {release['validator_command']}",
        f"run: {release['dry_run_command']}",
    }
    if not expected_runs <= set(verify):
        raise PolicyError("release verify job does not validate and dry-run the exact workspace selection")
    if f"run: {release['publish_command']}" not in publish:
        raise PolicyError("release publish job does not use the exact workspace selection")
    if "needs: verify" not in publish:
        raise PolicyError("release publish job is not guarded by verify")
    if "environment: release" not in publish:
        raise PolicyError("release publish job is not bound to environment: release")
    if f"if: github.event_name == '{release['publish_event']}'" not in publish:
        raise PolicyError("release publication is not restricted to manual dispatch")
    if "workflow_dispatch:" not in active:
        raise PolicyError("release workflow has no manual dispatch trigger")
    publish_runs = sorted(line for line in active if line.startswith("run: cargo publish"))
    expected_publish_runs = sorted(
        [f"run: {release['dry_run_command']}", f"run: {release['publish_command']}"]
    )
    if publish_runs != expected_publish_runs:
        raise PolicyError("release workflow cargo publish command set drifted")


def main() -> int:
    """Validate the checked-in policy against live Cargo and workflow facts."""

    try:
        ledger = validate_ledger(read_ledger())
        metadata = cargo_json(["metadata", "--format-version", "1", "--locked", "--no-deps"])
        packages, order = validate_workspace(ledger, metadata)
        private = {item["name"] for item in ledger["packages"] if not item["publish"]}
        observations = {
            selection["name"]: cargo_tree_observation(
                ledger["feature_contract"]["package"], selection["cargo_args"]
            )
            for selection in ledger["feature_contract"]["selections"]
        }
        validate_feature_observations(ledger, observations, private)
        validate_unknown_feature_refusal()
        workflow_path = ROOT / ledger["release_contract"]["workflow"]
        validate_release_workflow(ledger, workflow_path.read_text(encoding="utf-8"))
    except (OSError, PolicyError) as error:
        print(f"package publication contract: FAIL: {error}", file=sys.stderr)
        return 1

    publishable = [item["name"] for item in ledger["packages"] if item["publish"]]
    print(
        "package publication contract: PASS "
        f"({len(packages)} members; {len(publishable)} publishable; {len(private)} private)"
    )
    print("derived leaf-to-facade publish order: " + " -> ".join(order))
    print(
        "supported oce-api normal OCE closure: "
        + ", ".join(ledger["feature_contract"]["normal_oce_dependency_closure"])
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
