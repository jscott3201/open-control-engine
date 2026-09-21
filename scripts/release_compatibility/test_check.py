#!/usr/bin/env python3
"""Adversarial evidence checks; expected policy is not obtained by blessing output."""

import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import check


class EvidenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.raw = check.read(check.ROOT, check.MATRIX)
        cls.matrix = json.loads(cls.raw)

    def validate_bytes(self, raw):
        read = check.read
        with patch.object(check, "read", side_effect=lambda root, path:
                          raw if path == check.MATRIX else read(root, path)):
            return check.validate(check.ROOT)

    def rejects(self, document):
        raw = check.canonical(document)
        self.assertNotEqual(raw, self.raw, "mutation must alter evidence")
        for _ in range(2):
            with self.assertRaisesRegex(check.Refusal, "matrix: evidence differs"):
                self.validate_bytes(raw)

    def test_same_version_never_authorizes_cross_candidate_support(self):
        expected = b'{"package":"0.1.0","cross_candidate":"host-envelope-refusal-before-decode"}\n'
        check.enforce(expected, expected)
        changed = expected.replace(b"host-envelope-refusal-before-decode", b"accept")
        with self.assertRaisesRegex(check.Refusal, "matrix: evidence differs"):
            check.enforce(changed, expected)

    def test_retained_matrix_has_the_independent_closed_direction_table(self):
        # Independent owner-policy oracle, not check.outcome or its artifact/direction constants.
        a, u, n, h = "accept", "unsupported-unqualified", "unavailable-in-producer", "host-envelope-refusal-before-decode"
        expected = [
            ("package-public-api", [a, u, u, u]),
            ("facade-catalog-schema", [a, n, u, n]),
            ("public-descriptor", [a, n, h, n]),
            ("diagnostics", [a, n, u, n]),
            ("execution-profile", [a, u, u, u]),
            ("executable-frame", [a, n, u, n]),
            ("snapshot", [a, n, h, n]),
            ("replay", [a, n, h, n]),
            ("strict-bit-qualification", [a, n, u, n]),
        ]
        directions = [("current", "current"), ("historical", "current"),
                      ("current", "historical"), ("historical", "historical")]
        rows = [(r["artifact"], r["producer"], r["consumer"], r["status"]) for r in self.matrix["rows"]]
        self.assertEqual(rows, [(artifact, p, c, status) for artifact, statuses in expected
                                for (p, c), status in zip(directions, statuses, strict=True)])
        self.assertEqual(len(rows), 36)
        self.assertEqual(self.matrix["current_facts"]["package"], "0.1.0")
        self.assertEqual(self.matrix["candidates"]["historical"]["package"], "0.1.0")
        self.assertEqual(self.matrix["typed_controls"]["status"], "typed-refusal")
        self.assertEqual(self.matrix["current_facts"]["state_format"], 2)
        self.assertEqual(self.matrix["current_facts"]["execution_abi"], 2)
        self.assertEqual(self.matrix["current_facts"]["replay_format"], 1)
        self.assertEqual(self.matrix["current_facts"]["descriptor_revision"], 1)

    def test_cli_and_repeated_candidate_bytes_match_the_retained_golden(self):
        command = [sys.executable, str(Path(check.__file__))]
        for _ in range(2):
            result = subprocess.run(command, capture_output=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, b"release compatibility: OK (36 directed rows; current/current only; no N-1)\n")
            result = subprocess.run(command + ["--candidate"], capture_output=True, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, self.raw)
        self.assertEqual(check.read(check.ROOT, check.MATRIX), self.raw, "checker never rewrites")

    def test_every_row_field_status_and_direction_is_enforced(self):
        for index, row in enumerate(self.matrix["rows"]):
            for field in row:
                with self.subTest(index=index, field=field):
                    changed = copy.deepcopy(self.matrix)
                    changed["rows"][index][field] = "accept" if field == "status" and row[field] != "accept" else "unknown"
                    self.rejects(changed)

    def test_removed_extra_duplicate_and_reordered_rows_refuse(self):
        for edit in (lambda rows: rows.pop(), lambda rows: rows.append(dict(rows[0])),
                     lambda rows: rows.__setitem__(1, dict(rows[0])), lambda rows: rows.reverse()):
            changed = copy.deepcopy(self.matrix)
            edit(changed["rows"])
            self.rejects(changed)

    def test_tag_source_baseline_and_same_version_equivalence_mutations_refuse(self):
        for candidate, field, value in (
            ("historical", "tag", "v0.1.1"), ("historical", "tag_object", "0" * 40),
            ("historical", "source", "0" * 40), ("historical", "kind", "supported-n-minus-one"),
            ("historical", "package", "0.1.1"), ("current", "implementation_baseline", "0" * 40),
        ):
            changed = copy.deepcopy(self.matrix)
            changed["candidates"][candidate][field] = value
            self.rejects(changed)
        changed = copy.deepcopy(self.matrix)
        changed["policy"] = "same-package-version-is-compatible"
        self.rejects(changed)

    def test_wire_abi_revisions_exactness_placement_and_qualification_mutations_refuse(self):
        for field in self.matrix["current_facts"]:
            changed = copy.deepcopy(self.matrix)
            changed["current_facts"][field] = "mutated"
            self.rejects(changed)
        for field in self.matrix["typed_controls"]["cases"]:
            changed = copy.deepcopy(self.matrix)
            changed["typed_controls"]["cases"][field] = "accept"
            self.rejects(changed)

    def test_missing_extra_mutated_and_reordered_evidence_refuses(self):
        for edit in (lambda rows: rows.pop(), lambda rows: rows.append(dict(rows[0])),
                     lambda rows: rows.reverse(), lambda rows: rows[0].update(sha256="0" * 64)):
            changed = copy.deepcopy(self.matrix)
            edit(changed["evidence"])
            self.rejects(changed)

    def test_malformed_duplicate_keys_noncanonical_encoding_and_extra_fields_refuse(self):
        for raw in (b"", b"\xff", self.raw[:-1], self.raw + b"\n", self.raw.replace(b"\n", b"\r\n"),
                    self.raw.replace(b'"revision": 1,', b'"revision": 1, "revision": 1,', 1),
                    self.raw.replace(b'"revision": 1,', b'"revision": true,', 1),
                    self.raw.replace(b'"policy":', b'"extra": null, "policy":', 1)):
            with self.subTest(raw=raw[:30]):
                with self.assertRaisesRegex(check.Refusal, "matrix: evidence differs"):
                    self.validate_bytes(raw)
        changed = dict(reversed(list(self.matrix.items())))
        self.rejects(changed)

    def test_live_source_changes_cannot_be_hidden_by_unmodified_matrix(self):
        original = check.read
        for path in ("Cargo.toml", "crates/oce-api/src/state.rs", "crates/oce-api/src/replay_codec.rs",
                     "crates/oce-api/src/compatibility.rs", "crates/oce-api/contracts/diagnostics.schema.json"):
            with self.subTest(path=path):
                with patch.object(check, "read", side_effect=lambda root, name:
                                  original(root, name) + (b"\n" if name == path else b"")):
                    with self.assertRaisesRegex(check.Refusal, "source: implementation differs"):
                        check.validate(check.ROOT)

    def test_missing_and_changed_physical_evidence_is_not_a_locator_only_check(self):
        original = check.read
        for path in (check.HISTORY, *check.EVIDENCE):
            with self.subTest(path=path):
                def missing(root, name):
                    if name == path:
                        raise check.Refusal("source: missing or unreadable")
                    return original(root, name)
                with patch.object(check, "read", side_effect=missing):
                    with self.assertRaisesRegex(check.Refusal, "source: missing"):
                        check.validate(check.ROOT)
                with patch.object(check, "read", side_effect=lambda root, name:
                                  original(root, name) + (b"\n" if name == path else b"")):
                    with self.assertRaisesRegex(check.Refusal, "history: retained receipt changed|matrix: evidence differs|facts: descriptor grammar"):
                        check.validate(check.ROOT)

    def test_inventory_addition_or_removal_changes_the_pinned_implementation(self):
        original = check.git
        inventory = original(check.ROOT, "ls-files", "-z", "--cached", "--others", "--exclude-standard")
        for changed in (inventory.replace(b"crates/oce-api/src/state.rs\0", b""),
                        inventory + b"crates/oce-api/src/unknown.rs\0"):
            with patch.object(check, "git", return_value=changed):
                with self.assertRaisesRegex(check.Refusal, "source: implementation differs|source: missing"):
                    check.validate(check.ROOT)

    def test_wrong_git_tag_or_peeled_source_refuses_without_fetching_or_reindexing(self):
        # Exercise the exact queries and fail BEFORE any expensive historical source sweep.
        for outputs, cause in (([b"0" * 40 + b"\n"], "wrong tag object"),
                               ([check.TAG.encode() + b"\n", b"0" * 40 + b"\n"], "wrong peeled source")):
            with patch.object(check, "git", side_effect=outputs):
                with self.assertRaisesRegex(check.Refusal, cause):
                    check.verify_history(check.ROOT)

    def test_symlinks_directories_and_missing_paths_refuse(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "regular").write_bytes(b"evidence")
            (root / "link").symlink_to("regular")
            (root / "directory").mkdir()
            for name, reason in (("link", "symlink"), ("directory", "not regular"), ("missing", "missing")):
                with self.assertRaisesRegex(check.Refusal, reason):
                    check.read(root, name)

    def test_no_op_enforcement_is_killed_by_real_matrix_hostility(self):
        changed = copy.deepcopy(self.matrix)
        changed["rows"][1]["status"] = "accept"
        self.rejects(changed)
        with patch.object(check, "enforce", return_value=None):
            with self.assertRaises(AssertionError):
                self.rejects(changed)

    def test_cli_failure_is_nonzero_deterministic_and_has_no_bless_option(self):
        for _ in range(2):
            with patch.object(sys, "argv", ["check.py"]), patch.object(check, "validate", side_effect=check.Refusal("matrix: evidence differs")):
                with patch("sys.stderr") as stderr:
                    self.assertEqual(check.main(), 1)
                self.assertEqual("".join(call.args[0] for call in stderr.write.call_args_list),
                                 "release compatibility: FAIL: matrix: evidence differs\n")
        result = subprocess.run([sys.executable, str(Path(check.__file__)), "--bless"], capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn(b"unrecognized arguments", result.stderr)


if __name__ == "__main__":
    unittest.main()
