#!/usr/bin/env python3
"""Hostile controls for the package, feature, and publication validator."""

from __future__ import annotations

import copy
import unittest

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


class PackagePolicyControls(unittest.TestCase):
    """Prove every required drift class turns the validator red."""

    @classmethod
    def setUpClass(cls) -> None:
        """Load one deterministic real-workspace fixture for all controls."""

        cls.ledger = validate.read_ledger()
        cls.metadata = validate.cargo_json(
            ["metadata", "--format-version", "1", "--locked", "--no-deps"]
        )
        cls.workflow = (
            validate.ROOT / cls.ledger["release_contract"]["workflow"]
        ).read_text(encoding="utf-8")

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
        validate.validate_release_workflow(ledger, self.workflow)

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

    def test_release_selection_count_and_manual_guard_drift_are_rejected(self) -> None:
        """The release workflow cannot widen selection or weaken manual authorization."""

        count = copy.deepcopy(self.ledger)
        count["release_contract"]["publishable_count"] = 13
        with self.assertRaisesRegex(validate.PolicyError, "release command, event, or count"):
            validate.validate_ledger(count)

        selection = self.workflow.replace(
            "cargo publish --workspace --locked --dry-run",
            "cargo publish -p oce-api --locked --dry-run",
            1,
        )
        with self.assertRaisesRegex(validate.PolicyError, "exact workspace selection"):
            validate.validate_release_workflow(self.ledger, selection)

        guard = self.workflow.replace(
            "if: github.event_name == 'workflow_dispatch'",
            "if: github.event_name == 'push'",
            1,
        )
        with self.assertRaisesRegex(validate.PolicyError, "manual dispatch"):
            validate.validate_release_workflow(self.ledger, guard)

    def test_release_environment_boundary_is_required(self) -> None:
        """Manual dispatch cannot bypass the tracked release environment boundary."""

        changed = self.workflow.replace("    environment: release\n", "", 1)
        self.assertNotEqual(changed, self.workflow)
        with self.assertRaisesRegex(validate.PolicyError, "environment: release"):
            validate.validate_release_workflow(self.ledger, changed)

    def test_checked_in_release_workflow_is_the_positive_control(self) -> None:
        """The unchanged live workflow satisfies every release guard at job scope."""

        validate.validate_release_workflow(self.ledger, self.workflow)

    def test_release_environment_cannot_move_under_job_env(self) -> None:
        """A same-value environment token under job env is not an authorization boundary."""

        changed = self.workflow.replace(
            "    environment: release\n    steps:\n",
            "    env:\n      environment: release\n    steps:\n",
            1,
        )
        self.assertNotEqual(changed, self.workflow)
        with self.assertRaisesRegex(validate.PolicyError, "environment: release"):
            validate.validate_release_workflow(self.ledger, changed)

    def test_manual_guard_cannot_move_under_publish_step(self) -> None:
        """A step-level manual condition cannot authorize the publish job itself."""

        changed = self.workflow.replace(
            "    if: github.event_name == 'workflow_dispatch'\n", "", 1
        )
        changed = changed.replace(
            "      - name: cargo publish --workspace\n",
            "      - name: cargo publish --workspace\n"
            "        if: github.event_name == 'workflow_dispatch'\n",
            1,
        )
        self.assertNotEqual(changed, self.workflow)
        with self.assertRaisesRegex(validate.PolicyError, "manual dispatch"):
            validate.validate_release_workflow(self.ledger, changed)

    def test_verify_dependency_cannot_move_under_job_env(self) -> None:
        """A nested needs token cannot establish the publish job dependency."""

        changed = self.workflow.replace(
            "    needs: verify\n",
            "    env:\n      needs: verify\n",
            1,
        )
        self.assertNotEqual(changed, self.workflow)
        with self.assertRaisesRegex(validate.PolicyError, "guarded by verify"):
            validate.validate_release_workflow(self.ledger, changed)

    def test_duplicate_publish_job_guards_are_rejected(self) -> None:
        """Duplicate protected keys fail instead of inheriting parser-dependent meaning."""

        controls = {
            "    needs: verify\n": "guarded by verify",
            "    environment: release\n": "environment: release",
            "    if: github.event_name == 'workflow_dispatch'\n": "manual dispatch",
        }
        for line, message in controls.items():
            with self.subTest(key=line.strip().partition(":")[0]):
                changed = self.workflow.replace(line, line * 2, 1)
                self.assertNotEqual(changed, self.workflow)
                with self.assertRaisesRegex(validate.PolicyError, message):
                    validate.validate_release_workflow(self.ledger, changed)


if __name__ == "__main__":
    unittest.main()
