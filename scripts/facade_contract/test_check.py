#!/usr/bin/env python3
"""Diagnostic/artifact hostile controls; real compiler controls run in check.py."""

import copy
from pathlib import Path
import unittest

import check


class CompilerControls(unittest.TestCase):
    def setUp(self):
        self.case = check.CASES[0]
        self.source = Path("consumer.rs")
        self.diagnostic = {
            "level": "error", "code": {"code": "E0599"},
            "message": "no function named `load_modelica` found",
            "spans": [{"is_primary": True, "file_name": "consumer.rs", "line_start": 3,
                       "line_end": 3, "text": [{"text": "    let _ = Host::load_modelica;"}]}],
        }

    def test_intended_error_at_the_consumer_use_is_accepted(self):
        check.intended_refusal(1, [self.diagnostic], self.case, self.source)

    def test_reintroduced_symbol_cannot_pass_by_compiling(self):
        with self.assertRaisesRegex(check.ContractError, "unexpectedly compiled"):
            check.intended_refusal(0, [], self.case, self.source)

    def test_unrelated_errors_and_wrong_locations_are_rejected(self):
        mutations = [
            lambda d: d["code"].update(code="E0433"),
            lambda d: d.update(message="no function named `unrelated` found"),
            lambda d: d["spans"][0].update(file_name="dependency.rs"),
            lambda d: d["spans"][0].update(line_start=2),
            lambda d: d["spans"][0].update(line_end=4),
            lambda d: d["spans"][0].update(is_primary=False),
            lambda d: d["spans"][0].update(text=[{"text": "unrelated()"}]),
        ]
        for mutation in mutations:
            diagnostic = copy.deepcopy(self.diagnostic)
            mutation(diagnostic)
            with self.subTest(diagnostic=diagnostic), self.assertRaises(check.ContractError):
                check.intended_refusal(1, [diagnostic], self.case, self.source)

    def test_signal_failure_missing_and_multiple_errors_are_rejected(self):
        for status, errors in [(-9, []), (1, []), (1, [self.diagnostic] * 2)]:
            with self.subTest(status=status, errors=errors), self.assertRaises(check.ContractError):
                check.intended_refusal(status, errors, self.case, self.source)

    def test_only_unique_cargo_reported_libraries_are_selected(self):
        message = {"reason": "compiler-artifact", "target": {"name": "oce_api"},
                   "filenames": ["deps/liboce_api.rlib", "deps/liboce_api.rmeta"]}
        self.assertEqual(check.artifact([message], "oce_api"), Path("deps/liboce_api.rlib"))
        for messages in [[], [dict(message, target={"name": "other"})],
                         [message, dict(message, filenames=["stale/liboce_api.rlib"])]]:
            with self.subTest(messages=messages), self.assertRaises(check.ContractError):
                check.artifact(messages, "oce_api")


if __name__ == "__main__":
    unittest.main()
