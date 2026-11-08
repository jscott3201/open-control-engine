#!/usr/bin/env python3
"""Hostile controls for the package, feature, and publication validator."""

from __future__ import annotations

import copy
import hashlib
import io
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest.mock import patch

import release_workflow
import validate


EXPECTED_PUBLISHABLE = {
    "oce-api",
    "oce-blocks",
    "oce-cxf",
    "oce-diag",
    "oce-expr",
    "oce-graph",
    "oce-model",
    "oce-semantics",
    "oce-store",
    "oce-validate",
}
EXPECTED_PRIVATE = {
    "oce-bless",
    "oce-conformance",
}


# Independent raw-byte golden from release.yml as re-pinned for GitHub #391, not read from the
# candidate or checker.
# Non-ASCII bytes are escaped explicitly; line endings and the final LF are pinned.
APPROVED_WORKFLOW = b"""# release \xe2\x80\x94 the crates.io publish gate.
#
# Two deliberately-decoupled triggers, because the `v*` tag doubles as an
# external git-pin ref (verdant-runtime pins `tag = "v0.1.0"`): cutting or
# moving that tag must NEVER auto-publish.
#
#   * tag push (`v*`)        \xe2\x86\x92 VERIFY only: tag\xe2\x86\x94version match, fmt/clippy/
#                              tests, and a full `cargo publish --dry-run`.
#                              No token, no publish. Safe to re-cut the tag.
#   * workflow_dispatch      \xe2\x86\x92 VERIFY then PUBLISH. Publishing is an explicit
#                              manual act: run the workflow (pick the tag ref
#                              in the "Run workflow" dialog) to release.
#
# The publish job runs in the `release` GitHub Environment, which scopes
# CARGO_REGISTRY_TOKEN to this workflow (repo Settings \xe2\x86\x92 Environments \xe2\x86\x92
# release). Enabling "Required reviewers" there adds a second human gate.
#
# All 10 publishable members ship in one dependency-ordered
# `cargo publish --workspace` (Rust 1.90+). Two private members are skipped by
# their own `publish = false`: `oce-bless` and `oce-conformance`. crates.io
# rejects re-publishing an existing version, so a repeat dispatch is a
# no-op-or-fail, never a clobber.
#
# Security: no `${{ github.event.* }}` in any `run:` (confined to if:/with:/env:).
# No crate is published yet; actual publication remains deferred until explicit
# owner authorization for the release milestone.

name: release

on:
  push:
    tags: ["v*"]
  workflow_dispatch:

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always

jobs:
  verify:
    name: verify (tag \xe2\x86\x94 version, gate, publish dry-run)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@1.97.1
        with:
          components: rustfmt, clippy

      - name: tag matches workspace version
        if: github.event_name == 'push'
        env:
          TAG: ${{ github.ref_name }}
        run: |
          VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
          if [ "v${VERSION}" != "${TAG}" ]; then
            echo "::error::tag ${TAG} != v${VERSION} (workspace version) \xe2\x80\x94 bump Cargo.toml before tagging"
            exit 1
          fi
          echo "tag ${TAG} matches workspace version ${VERSION}"

      - name: fmt
        run: cargo fmt --all --check

      - name: clippy (-D warnings)
        run: cargo clippy --workspace --all-targets --locked -- -D warnings

      - name: tests
        run: cargo test --workspace --locked

      - name: package, feature, and publication contract (10 publishable, 2 private)
        run: python3 scripts/package_policy/validate.py

      - name: publish dry-run (workspace, dependency-ordered)
        run: cargo publish --workspace --locked --dry-run

  publish:
    name: publish to crates.io
    needs: verify
    if: github.event_name == 'workflow_dispatch'
    runs-on: ubuntu-latest
    environment: release
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@1.97.1

      - name: cargo publish --workspace
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: cargo publish --workspace --locked
"""
DRIFT_MESSAGE = "release workflow bytes differ from the approved SHA-256"


class PackagePolicyControls(unittest.TestCase):
    """Prove every required drift class turns the validator red."""

    @classmethod
    def setUpClass(cls) -> None:
        """Load one deterministic real-workspace fixture for all controls."""

        cls.metadata = validate.cargo_json(
            ["metadata", "--format-version", "1", "--locked", "--no-deps"]
        )
        cls.workspace = validate.validate_workspace(copy.deepcopy(cls.metadata))

    def observations(self) -> dict[str, dict[str, list[str]]]:
        """Return feature observations matching the live workspace closure."""

        closure = self.workspace["api_closure"]
        return {
            name: {
                "enabled_features": list(enabled),
                "workspace_closure": list(closure),
                "all_packages": [validate.FEATURE_PACKAGE, *closure, "serde", "thiserror"],
            }
            for name, (_, enabled) in validate.EXPECTED_SELECTIONS.items()
        }

    def package(self, metadata: dict, name: str) -> dict:
        """Return one mutable workspace package from a metadata fixture."""

        return next(package for package in metadata["packages"] if package["name"] == name)

    def test_live_workspace_reaches_every_pure_validator(self) -> None:
        """The positive fixture traverses Cargo, feature, and workflow checks."""

        workspace = validate.validate_workspace(copy.deepcopy(self.metadata))
        validate.validate_feature_observations(
            self.observations(), workspace["api_closure"], workspace["private"]
        )
        validate.validate_release_workflow(APPROVED_WORKFLOW)

        self.assertEqual(set(workspace["order"]), workspace["publishable"])
        self.assertEqual(workspace["order"][-1], "oce-api")
        for storage in validate.FORBIDDEN_CLOSURE_MEMBERS:
            self.assertNotIn(storage, workspace["api_closure"])
        self.assertEqual(workspace, validate.validate_workspace(copy.deepcopy(self.metadata)))

    def test_publish_and_private_sets_follow_the_manifests(self) -> None:
        """Cargo, not prose, supplies the publication bit; release.yml names the same split."""

        self.assertEqual(self.workspace["publishable"], EXPECTED_PUBLISHABLE)
        self.assertEqual(self.workspace["private"], EXPECTED_PRIVATE)

    def test_manifest_publish_flag_flip_moves_the_member(self) -> None:
        """Flipping `publish` moves a crate between the sets and exposes private leakage."""

        changed = copy.deepcopy(self.metadata)
        self.package(changed, "oce-model")["publish"] = []
        with self.assertRaisesRegex(validate.PolicyError, "private normal/build dependency leakage"):
            validate.validate_workspace(changed)

        changed = copy.deepcopy(self.metadata)
        self.package(changed, "oce-api")["publish"] = []
        with self.assertRaisesRegex(validate.PolicyError, "oce-api must stay registry-ready"):
            validate.validate_workspace(changed)

    def test_registry_restricted_publish_value_is_rejected(self) -> None:
        """Only crates.io or nothing: an alternate registry list is not a supported state."""

        changed = copy.deepcopy(self.metadata)
        self.package(changed, "oce-store")["publish"] = ["private-registry"]
        with self.assertRaisesRegex(validate.PolicyError, "registry-restricted publish value"):
            validate.validate_workspace(changed)

    def test_duplicate_and_foreign_members_are_rejected(self) -> None:
        """Workspace membership maps one-to-one to oce-* package names."""

        duplicate = copy.deepcopy(self.metadata)
        clone = copy.deepcopy(self.package(duplicate, "oce-diag"))
        clone["id"] = "path+file:///fixture/oce-diag#0.1.0"
        duplicate["packages"].append(clone)
        duplicate["workspace_members"].append(clone["id"])
        with self.assertRaisesRegex(validate.PolicyError, "duplicate Cargo workspace package name"):
            validate.validate_workspace(duplicate)

        foreign = copy.deepcopy(self.metadata)
        extra = {
            "id": "path+file:///fixture/unprefixed#0.1.0",
            "name": "unprefixed",
            "publish": [],
            "features": {},
            "dependencies": [],
        }
        foreign["packages"].append(extra)
        foreign["workspace_members"].append(extra["id"])
        with self.assertRaisesRegex(validate.PolicyError, "outside the oce-\\* namespace"):
            validate.validate_workspace(foreign)

    def test_declared_feature_drift_is_rejected(self) -> None:
        """The oce-api feature table is closed: it declares no features, so any spelling drifts."""

        for features in ({"default": []}, {"default": ["mem"], "mem": []}, {"db": []}):
            with self.subTest(features=features):
                changed = copy.deepcopy(self.metadata)
                self.package(changed, "oce-api")["features"] = features
                with self.assertRaisesRegex(validate.PolicyError, "features drifted"):
                    validate.validate_workspace(changed)

    def test_private_normal_dependency_leakage_is_rejected(self) -> None:
        """A publishable package cannot pull a private workspace member into registry closure."""

        changed = copy.deepcopy(self.metadata)
        self.package(changed, "oce-api")["dependencies"].append(
            {
                "name": "oce-conformance",
                "kind": None,
                "path": "/fixture/oce-conformance",
                "req": "^0.1.0",
            }
        )
        with self.assertRaisesRegex(validate.PolicyError, "private normal/build dependency leakage"):
            validate.validate_workspace(changed)

    def test_unversioned_publishable_path_dependency_is_rejected(self) -> None:
        """Registry publication cannot depend on a path-only normal edge."""

        changed = copy.deepcopy(self.metadata)
        api = self.package(changed, "oce-api")
        dependency = next(item for item in api["dependencies"] if item["name"] == "oce-model")
        dependency["req"] = "*"
        with self.assertRaisesRegex(validate.PolicyError, "lacks a registry version"):
            validate.validate_workspace(changed)

        changed = copy.deepcopy(self.metadata)
        api = self.package(changed, "oce-api")
        dependency = next(item for item in api["dependencies"] if item["name"] == "oce-model")
        del dependency["path"]
        with self.assertRaisesRegex(validate.PolicyError, "is not path-backed"):
            validate.validate_workspace(changed)

    def test_api_closure_must_exclude_the_storage_port(self) -> None:
        """The engine is pure: a direct or transitive edge to a storage crate turns it red."""

        for storage in validate.FORBIDDEN_CLOSURE_MEMBERS:
            changed = copy.deepcopy(self.metadata)
            api = self.package(changed, "oce-api")
            edge = copy.deepcopy(next(item for item in api["dependencies"] if item["name"] == "oce-model"))
            edge["name"] = storage
            api["dependencies"].append(edge)
            with self.assertRaisesRegex(validate.PolicyError, "must exclude storage crates"):
                validate.validate_workspace(changed)

        changed = copy.deepcopy(self.metadata)
        model = self.package(changed, "oce-model")
        edge = copy.deepcopy(next(item for item in model["dependencies"] if item["name"] == "serde"))
        edge["name"] = "oce-store"
        edge["path"] = "crates/oce-store"
        edge["req"] = "^0.2.0"
        model["dependencies"].append(edge)
        with self.assertRaisesRegex(validate.PolicyError, "must exclude storage crates"):
            validate.validate_workspace(changed)

    def test_feature_closure_drift_and_private_leakage_are_rejected(self) -> None:
        """The default build retains the public normal closure and enables no feature."""

        closure = self.workspace["api_closure"]
        drifted = self.observations()
        drifted["default"]["workspace_closure"].remove("oce-model")
        with self.assertRaisesRegex(validate.PolicyError, "feature closure drift"):
            validate.validate_feature_observations(drifted, closure, EXPECTED_PRIVATE)

        leaked = self.observations()
        leaked["default"]["workspace_closure"].append("oce-conformance")
        leaked["default"]["workspace_closure"].sort()
        with self.assertRaisesRegex(validate.PolicyError, "feature closure drift"):
            validate.validate_feature_observations(leaked, closure, EXPECTED_PRIVATE)

        enabled = self.observations()
        enabled["default"]["enabled_features"] = ["mem"]
        with self.assertRaisesRegex(validate.PolicyError, "enabled feature drift"):
            validate.validate_feature_observations(enabled, closure, EXPECTED_PRIVATE)

        missing = self.observations()
        del missing["default"]
        with self.assertRaisesRegex(validate.PolicyError, "observation coverage differs"):
            validate.validate_feature_observations(missing, closure, EXPECTED_PRIVATE)

    def test_forbidden_runtime_dependency_is_rejected(self) -> None:
        """The supported selection cannot add a database or async runtime."""

        for forbidden in validate.FORBIDDEN_DEPENDENCIES:
            with self.subTest(dependency=forbidden):
                changed = self.observations()
                changed["default"]["all_packages"].append(forbidden)
                with self.assertRaisesRegex(validate.PolicyError, "forbidden normal dependency"):
                    validate.validate_feature_observations(
                        changed, self.workspace["api_closure"], EXPECTED_PRIVATE
                    )


class WorkflowApprovalControls(unittest.TestCase):
    """Exercise byte approval and the real policy entry point; shell samples are data only."""

    def run_policy(self, candidate: bytes | None) -> tuple[int, str, str]:
        """Read a real candidate file through main; isolate only unrelated Cargo observations."""

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            workflow = root / validate.RELEASE_WORKFLOW
            workflow.parent.mkdir(parents=True)
            if candidate is not None:
                workflow.write_bytes(candidate)
            stdout, stderr = io.StringIO(), io.StringIO()
            with (
                patch.object(validate, "ROOT", root),
                patch.object(validate, "cargo_json", return_value={}),
                patch.object(validate, "validate_workspace", return_value={
                    "packages": EXPECTED_PUBLISHABLE | EXPECTED_PRIVATE,
                    "publishable": EXPECTED_PUBLISHABLE,
                    "private": EXPECTED_PRIVATE,
                    "order": [],
                    "api_closure": [],
                }),
                patch.object(validate, "cargo_tree_observation", return_value={}),
                patch.object(validate, "validate_feature_observations"),
                patch.object(validate, "validate_unknown_feature_refusal"),
                redirect_stdout(stdout), redirect_stderr(stderr),
            ):
                result = validate.main()
            return result, stdout.getvalue(), stderr.getvalue()

    def assert_policy_drift(self, candidate: bytes) -> None:
        """A changed file must fail through the entry point, not just the hash utility."""

        self.assertNotEqual(candidate, APPROVED_WORKFLOW)
        result, stdout, stderr = self.run_policy(candidate)
        self.assertEqual(result, 1)
        self.assertEqual(stdout, "")
        self.assertEqual(stderr, f"package publication contract: FAIL: {DRIFT_MESSAGE}\n")

    def assert_byte_drift(self, candidate: bytes) -> None:
        """Check the focused exception, policy translation, and file-reading integration."""

        self.assertNotEqual(candidate, APPROVED_WORKFLOW)
        with self.assertRaises(release_workflow.WorkflowError) as caught:
            release_workflow.validate(candidate)
        self.assertEqual(str(caught.exception), DRIFT_MESSAGE)
        with self.assertRaises(validate.PolicyError) as translated:
            validate.validate_release_workflow(candidate)
        self.assertEqual(str(translated.exception), DRIFT_MESSAGE)
        self.assertIsInstance(translated.exception.__cause__, release_workflow.WorkflowError)
        self.assert_policy_drift(candidate)

    def test_frozen_reference_and_live_candidate_agree_and_repeat(self) -> None:
        """Approval is independent of the candidate and repeated acceptance is deterministic."""

        candidate = (validate.ROOT / validate.RELEASE_WORKFLOW).read_bytes()
        self.assertEqual(candidate, APPROVED_WORKFLOW)
        self.assertEqual(hashlib.sha256(APPROVED_WORKFLOW).hexdigest(), release_workflow.EXPECTED_SHA256)
        reports = []
        for workflow in (APPROVED_WORKFLOW, candidate, APPROVED_WORKFLOW):
            self.assertIsNone(release_workflow.validate(workflow))
            self.assertIsNone(validate.validate_release_workflow(workflow))
            report = self.run_policy(workflow)
            self.assertEqual(report[0], 0)
            self.assertEqual(report[2], "")
            self.assertIn("PASS (12 members; 10 publishable; 2 private)", report[1])
            reports.append(report)
        self.assertEqual(reports, [reports[0]] * 3)

    def test_all_byte_boundaries_are_closed(self) -> None:
        """Even inert edits and malformed encodings must get the same focused drift error."""

        controls = {
            "one-byte": b"!" + APPROVED_WORKFLOW[1:],
            "comment": b"# harmless comment\n" + APPROVED_WORKFLOW,
            "trailing-byte": APPROVED_WORKFLOW + b" ",
            "extra-newline": APPROVED_WORKFLOW + b"\n",
            "missing-newline": APPROVED_WORKFLOW[:-1],
            "crlf": APPROVED_WORKFLOW.replace(b"\n", b"\r\n"),
            "bare-cr": APPROVED_WORKFLOW.replace(b"\n", b"\r"),
            "invalid-utf8": b"\xff" + APPROVED_WORKFLOW,
            "utf8-bom": b"\xef\xbb\xbf" + APPROVED_WORKFLOW,
            "empty": b"",
            "truncated": APPROVED_WORKFLOW[:len(APPROVED_WORKFLOW) // 2],
        }
        for name, candidate in controls.items():
            with self.subTest(drift=name):
                self.assert_byte_drift(candidate)

    def test_crlf_candidate_fails_without_text_normalization(self) -> None:
        """The real read path must preserve CRLF bytes rather than re-create the LF golden."""

        self.assert_policy_drift(APPROVED_WORKFLOW.replace(b"\n", b"\r\n"))

    def test_environment_split_publish_counterexamples_are_rejected(self) -> None:
        """Retaining every old literal guard cannot authorize an env-split tag publication."""

        controls = {
            "split-command-and-verb": (b"          C: cargo\n          P: publish\n", b"$C $P"),
            "split-command": (b"          PUBLISHER: cargo\n", b"$PUBLISHER publish"),
        }
        for name, (environment, command) in controls.items():
            with self.subTest(attack=name):
                candidate = APPROVED_WORKFLOW.replace(
                    b"    runs-on: ubuntu-latest\n",
                    b"    runs-on: ubuntu-latest\n    environment: release\n", 1,
                )
                step = (
                    b"      - name: unapproved tag publication\n"
                    b"        env:\n"
                    b"          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n"
                    + environment + b"        run: " + command + b" --workspace --locked\n\n"
                )
                candidate = candidate.replace(b"      - name: fmt\n", step + b"      - name: fmt\n", 1)
                self.assertEqual(candidate.split(b"\n  publish:\n")[1], APPROVED_WORKFLOW.split(b"\n  publish:\n")[1])
                self.assertIn(b"        run: cargo publish --workspace --locked --dry-run\n", candidate)
                self.assertIn(b"    environment: release\n", candidate.split(b"\n  publish:\n")[0])
                self.assertIn(step, candidate)
                self.assert_byte_drift(candidate)

    def test_extra_execution_and_credentials_are_rejected(self) -> None:
        """Added actions, run bodies, jobs, and token mappings are all unapproved bytes."""

        additions = (
            b"      - uses: unapproved/action@v1\n",
            b"      - run: ./unapproved-script.sh\n",
            b"      - run: cargo publish --workspace --locked\n",
            b"      - run: >-\n          cargo\n          publish --workspace --locked\n",
            b"      - run: |\n          car\\\n          go pub\\\n          lish --workspace --locked\n",
            b"      - run: cargo\n          publish --workspace --locked\n",
        )
        for addition in additions:
            with self.subTest(addition=addition):
                self.assert_byte_drift(APPROVED_WORKFLOW.replace(b"      - name: fmt\n", addition + b"      - name: fmt\n", 1))
        self.assert_byte_drift(APPROVED_WORKFLOW + b"\n  unapproved:\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n")
        self.assert_byte_drift(APPROVED_WORKFLOW.replace(
            b"  CARGO_TERM_COLOR: always\n",
            b"  CARGO_TERM_COLOR: always\n  CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}\n", 1,
        ))

    def test_selection_and_protected_job_drift_are_rejected(self) -> None:
        """Selection, authorization, and structural decoys cannot change the approved bytes."""

        for old, new in (
            (b"cargo publish --workspace --locked --dry-run", b"cargo publish -p oce-api --locked --dry-run"),
            (b"    environment: release\n", b""),
            (b"    environment: release\n", b"    env:\n      environment: release\n"),
            (b"    needs: verify\n", b"    env:\n      needs: verify\n"),
            (b"    if: github.event_name == 'workflow_dispatch'\n", b"    if: github.event_name == 'push'\n"),
            (b"  verify:\n", b"  verify: {}\n  verify:\n"),
            (b"  publish:\n", b"  deployment:\n"),
            (b"  publish:\n", b"  'publish':\n"),
        ):
            with self.subTest(replacement=new):
                self.assert_byte_drift(APPROVED_WORKFLOW.replace(old, new, 1))
        for line in (b"    needs: verify\n", b"    environment: release\n",
                     b"    if: github.event_name == 'workflow_dispatch'\n"):
            with self.subTest(duplicate=line):
                self.assert_byte_drift(APPROVED_WORKFLOW.replace(line, line * 2, 1))
        self.assert_byte_drift(b"decoy: |\n  jobs:\n  publish:\n    needs: verify\n\n" + APPROVED_WORKFLOW)

    def test_missing_workflow_has_a_file_diagnostic(self) -> None:
        """Missing input fails at main without a traceback or a misleading byte-drift error."""

        result, stdout, stderr = self.run_policy(None)
        self.assertEqual(result, 1)
        self.assertEqual(stdout, "")
        self.assertIn("package publication contract: FAIL: [Errno 2]", stderr)
        self.assertIn(".github/workflows/release.yml", stderr)
        self.assertNotIn(DRIFT_MESSAGE, stderr)

    def test_crlf_control_detects_allow_all_self_blessing_and_text_read_regressions(self) -> None:
        """Mutation controls fail by assertion, not tooling errors; patches cannot escape scope."""

        def candidate_as_expected(workflow: bytes) -> None:
            candidate = (validate.ROOT / ".github/workflows/release.yml").read_bytes()
            if hashlib.sha256(workflow).digest() != hashlib.sha256(candidate).digest():
                raise release_workflow.WorkflowError(DRIFT_MESSAGE)

        def normalized_read(path: Path) -> bytes:
            return path.read_text(encoding="utf-8").encode("utf-8")

        mutations = {
            "allow-all": patch.object(release_workflow, "validate", return_value=None),
            "candidate-as-expected": patch.object(release_workflow, "validate", candidate_as_expected),
            "read-text-regression": patch.object(Path, "read_bytes", normalized_read),
        }
        for name, mutation in mutations.items():
            with self.subTest(regression=name), mutation:
                result = unittest.TestResult()
                WorkflowApprovalControls("test_crlf_candidate_fails_without_text_normalization").run(result)
                self.assertEqual(result.testsRun, 1)
                self.assertEqual(result.errors, [])
                self.assertEqual(len(result.failures), 1)
                self.assertIn("AssertionError: 0 != 1", result.failures[0][1])
        # Explicit unmutated control after every patch has restored the real implementation.
        self.test_crlf_candidate_fails_without_text_normalization()
        self.assertEqual(self.run_policy(APPROVED_WORKFLOW)[0], 0)


if __name__ == "__main__":
    unittest.main()
