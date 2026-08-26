"""Focused golden, determinism, source, and mutation tests for the baseline tool."""

from __future__ import annotations

import copy
import contextlib
import importlib.util
import io
import json
import pathlib
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
MODULE_PATH = pathlib.Path(__file__).with_name("baseline.py")
SPEC = importlib.util.spec_from_file_location("stability_baseline", MODULE_PATH)
assert SPEC and SPEC.loader
baseline = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(baseline)


class BaselineTests(unittest.TestCase):
    """Exercise exact bytes and every contract-specific refusal class."""

    def assert_rejected(self, document, message: str | None = None):
        """Assert that canonical bytes for a mutated document fail closed."""
        with self.assertRaisesRegex(baseline.BaselineError, message or "stability baseline"):
            baseline.check_payload(baseline.render(document))

    def test_python_minimum_accepts_3_11_without_a_diagnostic(self):
        diagnostic = io.StringIO()
        with contextlib.redirect_stderr(diagnostic):
            baseline.require_supported_python((3, 11, 0))
        self.assertEqual(diagnostic.getvalue(), "")

    def test_python_below_minimum_refuses_with_stable_actionable_diagnostic(self):
        diagnostic = io.StringIO()
        with contextlib.redirect_stderr(diagnostic), self.assertRaises(SystemExit) as refusal:
            baseline.require_supported_python((3, 10, 14))
        self.assertEqual(refusal.exception.code, 2)
        self.assertEqual(
            diagnostic.getvalue(),
            "error: tools/stability_baseline/baseline.py requires Python 3.11 or newer "
            "(found 3.10); run it with a Python 3.11+ interpreter.\n",
        )

    def test_python_version_guard_runs_before_tomllib_import(self):
        source = MODULE_PATH.read_text(encoding="utf-8")
        self.assertLess(
            source.index("require_supported_python(sys.version_info)"),
            source.index("\nimport tomllib\n"),
        )

    def test_checked_in_artifact_is_the_bit_exact_golden(self):
        payload = baseline.ARTIFACT.read_bytes()
        baseline.check_payload(payload)
        self.assertEqual(payload, baseline.render())

    def test_repeated_render_is_byte_stable_and_path_independent(self):
        first = baseline.render()
        second = baseline.render()
        self.assertEqual(first, second)
        self.assertTrue(first.endswith(b"\n"))
        self.assertNotIn(str(ROOT).encode(), first)
        self.assertNotIn(b"/Users/", first)
        self.assertNotIn(b"/home/", first)

    def test_write_round_trip_is_byte_stable(self):
        with tempfile.TemporaryDirectory() as directory:
            first = pathlib.Path(directory) / "first.json"
            second = pathlib.Path(directory) / "second.json"
            baseline.write_artifact(first)
            baseline.write_artifact(second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            baseline.check_payload(first.read_bytes())

    def test_duplicate_keys_and_unknown_fields_are_rejected(self):
        with self.assertRaisesRegex(baseline.BaselineError, "duplicate JSON key"):
            baseline.parse(b'{"schema":"a","schema":"b"}\n')
        document = baseline.snapshot()
        document["unexpected"] = True
        self.assert_rejected(document, "closed schema")

    def test_shortened_and_non_hex_shas_are_rejected(self):
        shortened = baseline.snapshot()
        shortened["open_control_engine"]["refs"]["development"]["commit"] = "41e997f"
        self.assert_rejected(shortened, "full lowercase 40-hex SHA")
        non_hex = baseline.snapshot()
        non_hex["open_control_engine"]["refs"]["main"]["tree"] = "z" * 40
        self.assert_rejected(non_hex, "full lowercase 40-hex SHA")

    def test_tree_mismatch_and_exact_pin_mutation_are_rejected(self):
        tree_mismatch = baseline.snapshot()
        tree_mismatch["downstream_and_adjacent"]["library"]["oce_dependency"]["tree"] = "1" * 40
        self.assert_rejected(tree_mismatch, "Library OCE pin tree")
        pin_mutation = baseline.snapshot()
        pin_mutation["downstream_and_adjacent"]["library"]["oce_dependency"]["commit"] = "2" * 40
        self.assert_rejected(pin_mutation, "fixed 2026-08-26 capture")

    def test_studio_product_locator_and_program_head_cannot_be_conflated(self):
        document = baseline.snapshot()
        studio = document["downstream_and_adjacent"]["studio"]
        studio["program_repository_head"]["commit"] = studio["selected_program_locator"]["commit"]
        self.assert_rejected(document, "must remain distinct")

    def test_issue_and_pull_request_inventories_cannot_be_conflated_or_omitted(self):
        missing_issue = baseline.snapshot()
        del missing_issue["open_control_engine"]["repository_state"]["open_issues"]
        self.assert_rejected(missing_issue, "closed schema")
        oce_pr = baseline.snapshot()
        oce_pr["open_control_engine"]["repository_state"]["open_pull_requests"]["numbers"] = [28]
        self.assert_rejected(oce_pr, "list shape")

    def test_no_pin_is_distinct_from_unknown_and_cannot_carry_a_pin(self):
        unknown = baseline.snapshot()
        unknown["downstream_and_adjacent"]["sim"]["oce_dependency"]["state"] = "UNKNOWN"
        self.assert_rejected(unknown, "distinct from unknown")
        invented = baseline.snapshot()
        invented["downstream_and_adjacent"]["cxf_json"]["oce_dependency"]["commit"] = "3" * 40
        self.assert_rejected(invented, "closed schema")

    def test_assignment_and_policy_unknowns_cannot_be_inferred(self):
        owner = baseline.snapshot()
        owner["open_control_engine"]["repository_state"]["open_issues"]["issues"][0][
            "program_owner"
        ] = "issue_author"
        self.assert_rejected(owner, "cannot be inferred")
        policy = baseline.snapshot()
        policy["open_control_engine"]["repository_state"]["branch_protection_and_rulesets"][
            "state"
        ] = "none"
        self.assert_rejected(policy, "must remain UNKNOWN")

    def test_noncanonical_serialization_is_rejected(self):
        payload = json.dumps(baseline.snapshot(), sort_keys=True, separators=(",", ":")).encode()
        with self.assertRaisesRegex(baseline.BaselineError, "field order|canonical"):
            baseline.check_payload(payload)

    def test_local_oce_history_verifies_exact_objects_not_moving_heads(self):
        baseline.verify_oce_source(ROOT)

    def test_dependency_scanner_sees_active_declarations_but_not_comments(self):
        manifest = {
            "dependencies": {"serde": "1"},
            "target": {"cfg(unix)": {"dev-dependencies": {"oce-api": {"version": "0.1"}}}},
        }
        self.assertIn("oce-api", list(baseline.dependency_names(manifest)))
        commented_future_dependency = {"package": {"name": "empty-scaffold"}, "dependencies": {}}
        self.assertNotIn("oce-api", list(baseline.dependency_names(commented_future_dependency)))

    def test_mutation_does_not_change_the_generator_source_object(self):
        document = baseline.snapshot()
        original = copy.deepcopy(document)
        document["open_control_engine"]["refs"]["development"]["tree"] = "4" * 40
        self.assertEqual(original, baseline.snapshot())


if __name__ == "__main__":
    unittest.main()
