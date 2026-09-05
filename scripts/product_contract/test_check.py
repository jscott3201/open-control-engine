#!/usr/bin/env python3
"""Hostile traceability controls; mutations touch temporary fixture copies only.

The expected report is hand-written, not obtained from the checker or blessed.
There is no standards oracle for this repository-specific grammar. Explicit
accept/refuse cases are its independent specification; engine semantics remain
covered by their existing tests, not by this lexical checker.
"""

from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import check


ROOT = Path(__file__).resolve().parents[2]
GOLDEN = (
    "product contract: OK\n"
    "Document revision: 3\n"
    "Grounding SHA: 97156fddc15e6f12650a060623c9cde84b98ecc9\n"
    "Requirements: 40\n"
    "CURRENT: 24\n"
    "HOST-OBLIGATION: 5\n"
    "FUTURE: 11\n"
    "Future outcomes: 11\n"
    "Integration pointers: 6\n"
    "Scope: traceability only; semantics and host compliance are not proven.\n"
)


class TraceabilityTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.document = (ROOT / check.DOCUMENT).read_text()
        paths: set[str] = set(check.POINTERS) | set(check.PENDING)
        for match in check.LINK.finditer(self.document):
            parsed = check.urlsplit(match[2])
            if not parsed.scheme and parsed.path:
                paths.add(check.local_path(check.DOCUMENT, parsed.path))
        for path in sorted(paths):
            destination = self.root / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / path, destination)
        self.tracked = paths - check.PENDING
        self.pending: set[str] = set(check.PENDING)
        self.ignored = set()

    def repository(self):
        # Inventory protocol is isolated here; actual Git integration is tested below.
        def inventory(_root, *arguments):
            paths = self.pending if "--others" in arguments else self.tracked
            return ("\0".join(sorted(paths)) + "\0").encode()

        with patch.object(check, "git", side_effect=inventory):
            return check.Repository(self.root)

    def run_fixture(self):
        repository = self.repository()

        def ignore_query(command, **_kwargs):
            self.assertEqual(command[:4], ["git", "check-ignore", "--no-index", "--quiet"])
            return SimpleNamespace(returncode=0 if command[-1] in self.ignored else 1)

        with patch.object(check.subprocess, "run", side_effect=ignore_query):
            return check.validate(repository)

    def write_document(self, text):
        (self.root / check.DOCUMENT).write_bytes(text.encode())

    def change_cell(self, identifier, column, value):
        lines = self.document.splitlines()
        matches = [index for index, line in enumerate(lines) if line.startswith(f"| {identifier} |")]
        self.assertEqual(len(matches), 1, "mutation must apply exactly once")
        index = matches[0]
        cells = lines[index].split("|")
        cells[column + 1] = f" {value} "
        lines[index] = "|".join(cells)
        changed = "\n".join(lines) + "\n"
        self.assertNotEqual(changed, self.document, "mutation must change bytes")
        self.write_document(changed)

    def rejects(self, message):
        with self.assertRaisesRegex(check.ContractError, message):
            self.run_fixture()

    def test_report_is_an_independent_byte_golden(self):
        before = {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        self.assertEqual(self.run_fixture().encode(), GOLDEN.encode())
        self.assertEqual(self.run_fixture().encode(), GOLDEN.encode())
        after = {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}
        self.assertEqual(before, after, "checking never rewrites sources")

    def test_real_checkout_and_cli_match_the_golden(self):
        self.assertEqual(check.validate(check.Repository(ROOT)), GOLDEN)
        command = [sys.executable, str(ROOT / "scripts/product_contract/check.py")]
        for _ in range(2):
            result = subprocess.run(command, cwd=self.root, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, GOLDEN.encode())
            self.assertEqual(result.stderr, b"")

    def test_no_op_checker_cannot_satisfy_refusal_controls(self):
        self.assertEqual(self.run_fixture(), GOLDEN)
        self.change_cell("PC-006", 1, "SHIPPED")
        self.rejects("unknown status")
        with patch.object(check, "validate", return_value=GOLDEN):
            with self.assertRaises(AssertionError):
                self.rejects("unknown status")

    def test_all_cells_are_required(self):
        for column in range(8):
            for empty in ("", "   ", "-", "..."):
                with self.subTest(column=column, empty=empty):
                    self.change_cell("PC-006", column, empty)
                    self.rejects("empty field")

    def test_status_vocabulary_is_closed(self):
        for value in ("current", "STABLE", "IMPLEMENTED", "HOST", "FUTURE-ish"):
            with self.subTest(value=value):
                self.change_cell("PC-006", 1, value)
                self.rejects("unknown status")

    def test_actor_and_owner_are_named_roles(self):
        for column in (2, 3):
            for value in ("123", "`role`", "[role](#change-record)", "N/A"):
                with self.subTest(column=column, value=value):
                    self.change_cell("PC-006", column, value)
                    self.rejects("actor and owner required")
        self.change_cell("PC-006", 3, "UNKNOWN")
        self.rejects("undelegated owner")

    def test_ids_are_unique_ordered_and_without_gaps(self):
        for value, message in (("PC-005", "duplicate ID"), ("PC-099", "omitted or unordered ID"),
                               ("PC-6", "invalid ID"), ("PC-000", "omitted or unordered ID")):
            with self.subTest(value=value):
                self.change_cell("PC-006", 0, value)
                self.rejects(message)
        rows = self.document.splitlines(keepends=True)
        changed = "".join(row for row in rows if not row.startswith("| PC-006 |"))
        self.assertNotEqual(changed, self.document)
        self.write_document(changed)
        self.rejects("omitted or unordered ID")

    def test_orphan_ids_cannot_hide_in_prose_or_cells(self):
        self.write_document(self.document + "\nPC-999 is missing.\n")
        self.rejects("orphan ID")
        self.change_cell("PC-006", 5, "See PC-999 for an unstated limitation.")
        self.rejects("orphan ID")

    def test_each_row_has_exactly_one_obligation(self):
        for value in ("May evaluate once.", "must evaluate once.", "MUST evaluate and MUST update."):
            with self.subTest(value=value):
                self.change_cell("PC-006", 4, value)
                self.rejects("one obligation required")

    def test_obligations_cannot_hide_outside_the_requirement_cell(self):
        for suffix in ("MUST execute.", "MUST NOT execute.", "must execute.", "M**UST execute.",
                       "`MUST` execute.", "MUST\nNOT execute."):
            with self.subTest(suffix=suffix):
                self.write_document(self.document + "\n" + suffix + "\n")
                self.rejects("obligation outside Requirement cell")
        for value in ("Host MUST qualify this.", "Host M**UST qualify this."):
            self.change_cell("PC-006", 5, value)
            self.rejects("obligation outside Requirement cell")

    def test_hidden_markup_and_expansions_are_not_a_second_grammar(self):
        for text in ("<!-- MUST execute -->", "M&#85;ST execute", "{{#include absent.rs}}",
                     "```\nMUST execute\n```", "~~~\nMUST execute\n~~~", "M\\UST execute",
                     "[reference][target]", "[target]: #change-record", "<a href='absent'>link</a>"):
            with self.subTest(text=text):
                self.write_document(self.document + "\n" + text + "\n")
                self.rejects("unsupported markup|unsupported link spelling")

    def test_table_boundary_header_and_column_damage_refuse(self):
        for old, new, message in (
            (check.HEADER, check.HEADER + "\n" + check.HEADER, "one Requirements table"),
            (check.HEADER, check.HEADER.replace("Limitation", "Notes"), "one Requirements table"),
            (check.SEPARATOR, "| --- |", "header or separator"),
            ("## Requirements", "## Claims", "one Requirements table"),
            ("| PC-006 |", "| spare | PC-006 |", "eight cells"),
        ):
            with self.subTest(new=new):
                self.assertIn(old, self.document)
                self.write_document(self.document.replace(old, new, 1))
                self.rejects(message)

    def test_empty_table_and_deleted_status_group_refuse(self):
        for omitted, message in (("| PC-", "empty requirements"),
                                 ("| PC-0", "empty requirements")):
            self.write_document("\n".join(line for line in self.document.splitlines()
                                          if not line.startswith(omitted)) + "\n")
            self.rejects(message)
        self.write_document(self.document.replace("| HOST-OBLIGATION |", "| CURRENT |"))
        self.rejects("all statuses required")

    def test_revision_grounding_and_change_record_are_required(self):
        for old, new, message in (
            ("Document revision: 3", "Document revision: 0", "metadata:"),
            ("Document revision: 3", "Document revision: 4", "current revision change record"),
            ("Document revision: 3", "Document revision: 3\nDocument revision: 3", "metadata:"),
            ("Grounding SHA: 97156fddc15e6f12650a060623c9cde84b98ecc9", "Grounding SHA: d2111be", "metadata:"),
            ("## Change record", "## History", "change record required"),
        ):
            with self.subTest(new=new):
                self.write_document(self.document.replace(old, new, 1))
                self.rejects(message)

    def test_line_endings_and_final_newline_are_explicit(self):
        for text in (self.document.replace("\n", "\r\n"), self.document.rstrip("\n"),
                     self.document + "\ttext\n"):
            self.write_document(text)
            self.rejects("expected LF text")

    def test_missing_test_wrong_fragment_and_non_test_declaration_refuse(self):
        path = "../crates/oce-api/src/tests/pre_execution_profile_tests.rs"
        for value, message in (
            (f"test [absent_test]({path}#L100-L128)", "test not in range"),
            (f"test [parameter_seed_is_first_call_output_and_equal_time_calls_advance_memory]({path}#L1)",
             "test not in range"),
            (f"test [a_test]({path})", "test needs line anchor"),
            ("test [load_cxf](../crates/oce-api/src/engine.rs#L221-L257)", "test not in range"),
            ("test [parse_document](../crates/oce-cxf/src/lib.rs#L81-L83)", "test not in range"),
        ):
            with self.subTest(value=value):
                self.change_cell("PC-006", 7, value)
                self.rejects(message)

    def test_removed_ignored_and_duplicate_rust_test_declarations_refuse(self):
        path = self.root / "crates/oce-api/src/tests/pre_execution_profile_tests.rs"
        original = path.read_text()
        needle = "#[test]\nfn parameter_seed_is_first_call_output_and_equal_time_calls_advance_memory()"
        self.assertIn(needle, original)
        for replacement in (needle.replace("#[test]", "// test removed"),
                            needle.replace("#[test]", "#[test]\n#[ignore]")):
            with self.subTest(replacement=replacement):
                path.write_text(original.replace(needle, replacement))
                self.rejects("unique test declaration")
        path.write_text(original + "\n" + needle + " {}\n")
        self.rejects("unique test declaration")

    def test_python_test_declaration_is_not_just_a_label(self):
        path = self.root / "scripts/product_contract/test_check.py"
        original = path.read_text()
        needle = "    def test_report_is_an_independent_byte_golden(self):"
        self.assertIn(needle, original)
        path.write_text(original.replace(needle, "    def renamed_property(self):"))
        self.rejects("test not in range")

    def test_future_rows_require_assignments_not_existing_test_promises(self):
        self.change_cell("PC-030", 7,
                         "test [retired_facade_symbols_are_absent]"
                         "(../crates/oce-api/tests/public_surface_contract.rs#L507-L512)")
        self.rejects("future outcome assignment required")
        self.change_cell("PC-006", 7, "source [tick](../crates/oce-api/src/engine.rs#L279-L312)")
        self.rejects("test or future assignment required")

    def test_future_assignment_requires_named_local_outcome_and_description(self):
        for value, message in (
            ("future [later](#bounded-admission-and-replacement)", "one named future assignment"),
            ("future [M01-PR04](#complete-frame-contract)", "missing future description"),
            ("future [M01-PR04](public-surface-contract.md)", "future outcome must be in this document"),
            ("future [M01-PR04](#absent)", "missing heading"),
        ):
            with self.subTest(value=value):
                self.change_cell("PC-030", 7, value)
                self.rejects(message)
        self.write_document(self.document.replace("M01-PR04:", "M01-PR09:"))
        self.rejects("missing future description")
        section = check.headings(self.document)["bounded-admission-and-replacement"]
        self.write_document(self.document.replace(section, "M01-PR04: Later."))
        self.rejects("missing future description")

    def test_duplicate_and_orphan_future_outcomes_refuse(self):
        for suffix in ("M01-PR04: Duplicated outcome.", "M09-PR99: Orphan outcome."):
            self.write_document(self.document + "\n" + suffix + "\n")
            self.rejects("duplicate or orphan future outcome")

    def test_grounding_is_required_to_be_a_local_link_list(self):
        for value, message in (("some source", "expected link list"),
                               ("[source](https://example.org/source)", "local evidence required"),
                               ("[source](../crates/oce-api/src/engine.rs), notes", "expected link list")):
            with self.subTest(value=value):
                self.change_cell("PC-006", 6, value)
                self.rejects(message)

    def test_broken_paths_fragments_and_line_bounds_refuse(self):
        for target, message in (
            ("absent.md", "not clone-visible"),
            ("execution-profile.md#missing", "missing heading"),
            ("execution-profile.md#", "malformed fragment"),
            ("execution-profile.md#hosttick-v1#other", "malformed fragment"),
            ("../crates/oce-api/src/engine.rs#L0", "expected line anchor"),
            ("../crates/oce-api/src/engine.rs#L99999", "line range"),
            ("../crates/oce-api/src/engine.rs#L50-L20", "line range"),
            ("../crates/oce-api/src/engine.rs#tick_with", "expected line anchor"),
        ):
            with self.subTest(target=target):
                self.change_cell("PC-006", 6, f"[source]({target})")
                self.rejects(message)

    def test_absolute_escaping_encoded_and_platform_paths_refuse(self):
        for target in ("/tmp/source", "../../source", "file:///tmp/source", "C:/source",
                       "//example.org/source", "..%2fsource", "..\\source", "execution-profile.md?raw=1"):
            with self.subTest(target=target):
                self.change_cell("PC-006", 6, f"[source]({target})")
                self.rejects("unsafe path|escaping path|local evidence required|unsupported external URL|unsupported markup")

    def test_missing_directory_and_symlink_targets_refuse(self):
        path = self.root / "crates/oce-api/src/engine.rs"
        original = path.read_bytes()
        path.unlink()
        self.rejects("target: missing")
        path.mkdir()
        self.rejects("target: not regular")
        path.rmdir()
        target = self.root / "regular.rs"
        target.write_bytes(original)
        path.symlink_to(target)
        self.rejects("target: symlink")

    def test_symlinked_parent_is_rejected_even_for_internal_targets(self):
        parent = self.root / "crates/oce-api/src/tests"
        destination = self.root / "regular-tests"
        parent.rename(destination)
        parent.symlink_to(destination, target_is_directory=True)
        self.rejects("target: symlink")

    def test_only_explicit_pending_files_are_eligible(self):
        self.assertEqual(self.run_fixture(), GOLDEN)
        path = "crates/oce-api/src/engine.rs"
        self.tracked.remove(path)
        self.pending.add(path)
        self.rejects("not clone-visible")

    def test_committed_contract_is_equivalent_to_pending_contract(self):
        self.tracked.update(self.pending)
        self.pending.clear()
        self.assertEqual(self.run_fixture(), GOLDEN)

    def test_ignored_targets_refuse_even_if_forced_into_inventory(self):
        for path in (check.DOCUMENT, "crates/oce-api/src/engine.rs"):
            with self.subTest(path=path):
                self.ignored = {path}
                self.rejects("ignored or unverifiable")

    def test_real_git_ignore_boundary_rejects_local_spec(self):
        repository = check.Repository(ROOT)
        path = "_spec/product-contract-hidden.md"
        repository.tracked.add(path)  # Model a forced tracked entry, without any source write.
        with self.assertRaisesRegex(check.ContractError, "ignored or unverifiable"):
            repository.read(path)

    def test_each_integration_pointer_is_required(self):
        for pointer in check.POINTERS:
            with self.subTest(pointer=pointer):
                path = self.root / pointer
                original = path.read_text()
                self.assertIn("product-contract.md", original)
                path.write_text(original.replace("product-contract.md", "absent-contract.md"))
                self.rejects("pointer: missing product contract")
                path.write_text(original)

    def test_invalid_encoding_and_cli_options_fail_without_writes(self):
        (self.root / check.DOCUMENT).write_bytes(b"\xff")
        self.rejects("unreadable UTF-8")
        result = subprocess.run([sys.executable, str(ROOT / "scripts/product_contract/check.py"),
                                 "--bless"], capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn(b"unrecognized arguments", result.stderr)

    def test_refusal_output_is_deterministic_and_nonzero(self):
        self.change_cell("PC-006", 1, "SHIPPED")
        # Exercise the CLI entry point; mock only repository inventory/ignore queries.
        for _ in range(2):
            with patch.object(check, "Repository", return_value=self.repository()):
                with patch.object(check.subprocess, "run", return_value=SimpleNamespace(returncode=1)):
                    with patch.object(sys, "argv", ["check.py"]):
                        with patch("sys.stderr") as stderr:
                            self.assertEqual(check.main(), 1)
            self.assertEqual("".join(call.args[0] for call in stderr.write.call_args_list),
                             "product contract: FAIL: table: unknown status: PC-006\n")

    def test_fulfilled_outcome_cannot_be_reintroduced_as_an_orphan_assignment(self):
        self.write_document(self.document + "\nM01-PR02: Retired assignment reintroduced.\n")
        self.rejects("duplicate or orphan future outcome")

    def test_pending_transition_paths_are_exact_and_still_require_regular_nonignored_files(self):
        self.assertEqual(check.PENDING, frozenset((
            "docs/product-contract.md", "scripts/product_contract/check.py",
            "scripts/product_contract/test_check.py", "docs/facade-migration.md",
            "crates/oce-api/tests/sim_assertions.rs",
        )))
        for path in ("docs/facade-migration.md", "crates/oce-api/tests/sim_assertions.rs"):
            with self.subTest(path=path):
                self.ignored = {path}
                self.rejects("ignored or unverifiable")
        self.ignored.clear()
        path = self.root / "crates/oce-api/tests/sim_assertions.rs"
        path.unlink()
        path.symlink_to(self.root / "crates/oce-api/src/engine.rs")
        self.rejects("target: symlink")

    def assert_readme_claim_boundary(self, stale, corrected, historical, error):
        # Mutate the real public-document fixture, never the live README or API source.
        path = self.root / "README.md"
        original = path.read_text()
        self.assertEqual(original.count(stale) + original.count(corrected), 1)
        valid = original.replace(stale, corrected)
        for suffix in ("", historical, historical + historical):
            positive = valid + "\n" + suffix
            path.write_text(positive)
            for _ in range(2):
                self.assertEqual(self.run_fixture(), GOLDEN)
                self.assertEqual(path.read_text(), positive, "validation is read-only")
        for statement in (stale, stale.replace("\n  ", " "), stale.upper()):
            mutated = valid.replace(corrected, statement)
            self.assertNotEqual(mutated, valid)
            path.write_text(mutated)
            for _ in range(2):
                self.rejects(error)
                self.assertEqual(path.read_text(), mutated, "refusal is read-only")

    def test_readme_refuses_callable_removed_loaders_but_accepts_migration_history(self):
        self.assert_readme_claim_boundary(
            "- **Two stable loader signatures are placeholders.** `load_from_semantic` and `load_modelica`\n"
            "  always return `OcError::Load`; use `load_cxf` for working ingest today.\n",
            "- **Deferred loader signatures have been removed.** `load_from_semantic` and `load_modelica`\n"
            "  are no longer callable; prepare supported CXF externally and use `load_cxf`.\n"
            "  See [facade migration](docs/facade-migration.md) for the pre-release source break.\n",
            "\n## Migration history\n\n"
            "- Previously, `load_from_semantic` and `load_modelica` always returned `OcError::Load`.\n"
            "  Both placeholder signatures have been removed; `load_cxf` remains available.\n",
            "facade: callable removed loaders in README.md",
        )

    def test_readme_refuses_public_error_severity_but_accepts_warning_default_history(self):
        self.assert_readme_claim_boundary(
            "- **Assertion events are warning-only today.** Although `AssertLevel::Error` is public for surface\n"
            "  stability, the engine never produces it; hosts must not build escalation logic on that variant.\n",
            "- **Assertion events and `AssertLevel` are Warning-only.** `AssertLevel::default()` is now\n"
            "  `Warning`; the never-emitted `AssertLevel::Error` variant was removed. This adds no escalation\n"
            "  or safety policy. See [facade migration](docs/facade-migration.md).\n",
            "\n## Migration history\n\n"
            "- Previously, `AssertLevel::Error` was public but never emitted; its removal intentionally\n"
            "  changes `AssertLevel::default()` from Error to Warning. No escalation is added.\n",
            "facade: removed assertion severity advertised as public in README.md",
        )


if __name__ == "__main__":
    unittest.main()
