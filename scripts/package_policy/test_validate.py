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
    "oce-flatten",
    "oce-graph",
    "oce-model",
    "oce-semantics",
    "oce-store",
    "oce-store-mem",
    "oce-validate",
}
EXPECTED_PRIVATE = {
    "oce-bless",
    "oce-conformance",
    "oce-docs",
    "oce-extension",
    "oce-reference-wal-adapter",
}


# Independent raw-byte golden from release.yml at
# 2bab88acbc96862f1808b34d305b795f521b3614, not read from the candidate or checker.
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
# All 12 publishable members ship in one dependency-ordered
# `cargo publish --workspace` (Rust 1.90+). Five private members are skipped by
# their own `publish = false`: `oce-bless`, `oce-conformance`, `oce-docs`,
# `oce-extension`, and `oce-reference-wal-adapter`. crates.io rejects
# re-publishing an existing version, so a repeat dispatch is a no-op-or-fail,
# never a clobber.
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

      - name: package, feature, and publication contract (12 publishable, 5 private)
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

        cls.ledger = validate.read_ledger()
        cls.metadata = validate.cargo_json(
            ["metadata", "--format-version", "1", "--locked", "--no-deps"]
        )

    def observations(self) -> dict[str, dict[str, list[str]]]:
        """Return feature observations matching the checked-in contract."""

        feature = self.ledger["feature_contract"]
        closure = feature["normal_oce_dependency_closure"]
        return {
            selection["name"]: {
                "enabled_features": list(selection["enabled_features"]),
                "workspace_closure": list(closure),
                "all_packages": [feature["package"], *closure, "serde", "thiserror"],
            }
            for selection in feature["selections"]
        }

    def test_checked_in_contract_reaches_every_pure_validator(self) -> None:
        """The positive fixture traverses schema, Cargo, feature, and workflow checks."""

        ledger = validate.validate_ledger(copy.deepcopy(self.ledger))
        packages, order = validate.validate_workspace(ledger, copy.deepcopy(self.metadata))
        private = set(packages) - EXPECTED_PUBLISHABLE
        validate.validate_feature_observations(ledger, self.observations(), private)
        validate.validate_release_workflow(APPROVED_WORKFLOW)

        self.assertEqual(set(order), EXPECTED_PUBLISHABLE)
        self.assertEqual(private, EXPECTED_PRIVATE)

    def test_ratified_publish_and_private_sets_are_exact(self) -> None:
        """Implementation publication does not imply independent support or admit private roots."""

        packages = {item["name"]: item for item in self.ledger["packages"]}
        publishable = {name for name, item in packages.items() if item["publish"]}
        private = set(packages) - publishable

        self.assertEqual(publishable, EXPECTED_PUBLISHABLE)
        self.assertEqual(private, EXPECTED_PRIVATE)
        self.assertEqual(packages["oce-cxf"]["category"], "implementation-dependency")
        self.assertEqual(packages["oce-blocks"]["category"], "transitional-companion")

    def test_owner_approved_categories_cannot_drift_with_same_publish_bit(self) -> None:
        """A valid publishable category cannot replace either approved host-facing role."""

        mutations = {
            "oce-api": "implementation-dependency",
            "oce-store": "implementation-dependency",
        }
        for package, category in mutations.items():
            with self.subTest(package=package):
                changed = copy.deepcopy(self.ledger)
                item = next(entry for entry in changed["packages"] if entry["name"] == package)
                item["category"] = category
                with self.assertRaisesRegex(
                    validate.PolicyError,
                    f"owner-approved package mapping drift for {package}",
                ):
                    validate.validate_ledger(changed)

    def test_missing_extra_and_duplicate_member_classifications_are_rejected(self) -> None:
        """Workspace coverage is a one-to-one mapping, never a best-effort list."""

        missing = copy.deepcopy(self.metadata)
        api_id = next(
            package["id"] for package in missing["packages"] if package["name"] == "oce-api"
        )
        missing["workspace_members"].remove(api_id)
        with self.assertRaisesRegex(validate.PolicyError, "classification coverage differs"):
            validate.validate_workspace(validate.validate_ledger(self.ledger), missing)

        extra = copy.deepcopy(self.metadata)
        extra_package = {
            "id": "path+file:///fixture/oce-unclassified#0.1.0",
            "name": "oce-unclassified",
            "publish": [],
            "features": {},
            "dependencies": [],
        }
        extra["packages"].append(extra_package)
        extra["workspace_members"].append(extra_package["id"])
        with self.assertRaisesRegex(validate.PolicyError, "classification coverage differs"):
            validate.validate_workspace(validate.validate_ledger(self.ledger), extra)

        duplicate = copy.deepcopy(self.ledger)
        duplicate["packages"].append(copy.deepcopy(duplicate["packages"][0]))
        with self.assertRaisesRegex(validate.PolicyError, "duplicate workspace package"):
            validate.validate_ledger(duplicate)

    def test_category_outside_closed_vocabulary_is_rejected(self) -> None:
        """A newly invented support status cannot bypass review."""

        changed = copy.deepcopy(self.ledger)
        changed["packages"][0]["category"] = "accidentally-public"
        with self.assertRaisesRegex(validate.PolicyError, "invalid package category"):
            validate.validate_ledger(changed)

    def test_manifest_publish_flag_drift_is_rejected(self) -> None:
        """Cargo, not prose, supplies the observed publication bit."""

        changed = copy.deepcopy(self.metadata)
        api = next(package for package in changed["packages"] if package["name"] == "oce-api")
        api["publish"] = []
        with self.assertRaisesRegex(validate.PolicyError, "publish flag drift for oce-api"):
            validate.validate_workspace(validate.validate_ledger(self.ledger), changed)

    def test_private_normal_dependency_leakage_is_rejected(self) -> None:
        """A publishable package cannot pull a private workspace member into registry closure."""

        changed = copy.deepcopy(self.metadata)
        api = next(package for package in changed["packages"] if package["name"] == "oce-api")
        api["dependencies"].append(
            {
                "name": "oce-docs",
                "kind": None,
                "path": "/fixture/oce-docs",
                "req": "^0.1.0",
            }
        )
        with self.assertRaisesRegex(validate.PolicyError, "private normal/build dependency leakage"):
            validate.validate_workspace(validate.validate_ledger(self.ledger), changed)

    def test_unversioned_publishable_path_dependency_is_rejected(self) -> None:
        """Registry publication cannot depend on a path-only normal edge."""

        changed = copy.deepcopy(self.metadata)
        api = next(package for package in changed["packages"] if package["name"] == "oce-api")
        dependency = next(item for item in api["dependencies"] if item["name"] == "oce-model")
        dependency["req"] = "*"
        with self.assertRaisesRegex(validate.PolicyError, "lacks a registry version"):
            validate.validate_workspace(validate.validate_ledger(self.ledger), changed)

    def test_feature_closure_drift_and_private_leakage_are_rejected(self) -> None:
        """Every supported spelling retains the same public normal closure."""

        drifted = self.observations()
        drifted["no-default-features"]["workspace_closure"].remove("oce-store-mem")
        with self.assertRaisesRegex(validate.PolicyError, "feature closure drift"):
            validate.validate_feature_observations(self.ledger, drifted, EXPECTED_PRIVATE)

        leaked = self.observations()
        leaked["explicit-mem"]["workspace_closure"].append("oce-docs")
        leaked["explicit-mem"]["workspace_closure"].sort()
        with self.assertRaisesRegex(validate.PolicyError, "feature closure drift"):
            validate.validate_feature_observations(self.ledger, leaked, EXPECTED_PRIVATE)

    def test_forbidden_runtime_dependency_is_rejected(self) -> None:
        """Supported feature selections cannot add a database or async runtime."""

        changed = self.observations()
        changed["default"]["all_packages"].append("tokio")
        with self.assertRaisesRegex(validate.PolicyError, "forbidden normal dependency"):
            validate.validate_feature_observations(self.ledger, changed, EXPECTED_PRIVATE)

    def test_release_path_and_counts_cannot_be_reconfigured(self) -> None:
        """The minimal release ledger still fixes its path and both selection counts."""

        for key, value in {"workflow": "candidate.yml", "publishable_count": 13, "private_count": 4}.items():
            with self.subTest(key=key):
                changed = copy.deepcopy(self.ledger)
                changed["release_contract"][key] = value
                self.assertNotEqual(changed, self.ledger)
                with self.assertRaisesRegex(validate.PolicyError, "release workflow path or count"):
                    validate.validate_ledger(changed)

    def test_release_schema_rejects_missing_fields_and_alternate_authorities(self) -> None:
        """No command grammar, configurable digest, or arbitrary field can enter the ledger."""

        for key in ("validator_command", "dry_run_command", "publish_command", "publish_event",
                    "expected_sha256", "digest_path", "extra"):
            with self.subTest(extra=key):
                changed = copy.deepcopy(self.ledger)
                changed["release_contract"][key] = "unapproved"
                with self.assertRaisesRegex(validate.PolicyError, "release_contract fields differ"):
                    validate.validate_ledger(changed)
        for key in self.ledger["release_contract"]:
            with self.subTest(missing=key):
                changed = copy.deepcopy(self.ledger)
                del changed["release_contract"][key]
                with self.assertRaisesRegex(validate.PolicyError, "release_contract fields differ"):
                    validate.validate_ledger(changed)
        with self.assertRaisesRegex(validate.PolicyError, "duplicate JSON key: workflow"):
            validate.reject_duplicate_keys([("workflow", "approved"), ("workflow", "candidate")])


class WorkflowApprovalControls(unittest.TestCase):
    """Exercise byte approval and the real policy entry point; shell samples are data only."""

    def run_policy(self, candidate: bytes | None) -> tuple[int, str, str]:
        """Read a real candidate file through main; isolate only unrelated Cargo observations."""

        ledger = validate.read_ledger()
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            workflow = root / ledger["release_contract"]["workflow"]
            workflow.parent.mkdir(parents=True)
            if candidate is not None:
                workflow.write_bytes(candidate)
            stdout, stderr = io.StringIO(), io.StringIO()
            with (
                patch.object(validate, "ROOT", root),
                patch.object(validate, "read_ledger", return_value=ledger),
                patch.object(validate, "cargo_json", return_value={}),
                patch.object(validate, "validate_workspace", return_value=(EXPECTED_PUBLISHABLE | EXPECTED_PRIVATE, [])),
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

        ledger = validate.read_ledger()
        candidate = (validate.ROOT / ledger["release_contract"]["workflow"]).read_bytes()
        self.assertEqual(candidate, APPROVED_WORKFLOW)
        self.assertEqual(hashlib.sha256(APPROVED_WORKFLOW).hexdigest(), release_workflow.EXPECTED_SHA256)
        reports = []
        for workflow in (APPROVED_WORKFLOW, candidate, APPROVED_WORKFLOW):
            self.assertIsNone(release_workflow.validate(workflow))
            self.assertIsNone(validate.validate_release_workflow(workflow))
            report = self.run_policy(workflow)
            self.assertEqual(report[0], 0)
            self.assertEqual(report[2], "")
            self.assertIn("PASS (17 members; 12 publishable; 5 private)", report[1])
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
