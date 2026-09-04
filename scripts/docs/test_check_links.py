#!/usr/bin/env python3
"""Regression tests for generated-document link checks."""

from __future__ import annotations

import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import stage as docs_stage


CHECKER = Path(__file__).with_name("check_links.py")


class GeneratedNavigationTests(unittest.TestCase):
    """Exercise the generated mdBook navigation."""

    def test_staged_navigation_includes_tracked_stability_baseline(self) -> None:
        """The dated stability snapshot is both staged and included as a chapter."""

        with tempfile.TemporaryDirectory() as temporary:
            staged, _, _ = docs_stage.stage_book(Path(temporary) / "book", "a" * 40)

            summary = (staged / "src" / "SUMMARY.md").read_text(encoding="utf-8")
            chapter = "- [Stability baseline](docs/stability-baseline.md)"
            self.assertEqual(summary.splitlines().count(chapter), 1)
            self.assertTrue((staged / "src" / "docs" / "stability-baseline.md").is_file())

    def test_staged_navigation_includes_public_surface_authority(self) -> None:
        """The contract is a chapter and its machine-readable ledger is an asset."""

        with tempfile.TemporaryDirectory() as temporary:
            staged, _, _ = docs_stage.stage_book(Path(temporary) / "book", "a" * 40)

            source = staged / "src" / "docs"
            summary = (staged / "src" / "SUMMARY.md").read_text(encoding="utf-8")
            chapter = "- [Public surface contract](docs/public-surface-contract.md)"
            self.assertEqual(summary.splitlines().count(chapter), 1)
            self.assertTrue((source / "public-surface-contract.md").is_file())
            self.assertEqual(
                (source / "public-surface-ledger.json").read_bytes(),
                (docs_stage.repository_root() / "docs" / "public-surface-ledger.json").read_bytes(),
            )


class ContractEvidenceTests(unittest.TestCase):
    """Require qualified test evidence named by the contract to resolve."""

    def test_qualified_test_evidence_exists(self) -> None:
        """Every module-qualified test anchor names an actual Rust test function."""

        root = docs_stage.repository_root()
        contract = (root / "docs" / "public-surface-contract.md").read_text(encoding="utf-8")
        anchors = set(
            re.findall(
                r"`([a-z][a-z0-9_]*(?:_tests|_adapter))::([a-z][a-z0-9_]*)`",
                contract,
            )
        )
        self.assertTrue(anchors, "contract must name qualified test evidence")
        for module, test in sorted(anchors):
            matches = list((root / "crates").rglob(f"{module}.rs"))
            self.assertEqual(len(matches), 1, f"{module} must resolve to one source file")
            source = matches[0].read_text(encoding="utf-8")
            self.assertRegex(
                source,
                rf"(?m)^\s*#\[test\]\s*\n\s*fn\s+{re.escape(test)}\s*\(",
                f"{module}::{test} must resolve to a Rust test",
            )


class SitePrefixAgreementTests(unittest.TestCase):
    """Exercise agreement between the CLI and staged mdBook configuration."""

    def run_checker(self, site_prefix: str) -> subprocess.CompletedProcess[str]:
        """Run the checker against a minimal staged book."""

        with tempfile.TemporaryDirectory() as temporary:
            stage = Path(temporary)
            output = stage / "book"
            output.mkdir()
            (stage / "book.toml").write_text(
                '[output.html]\nsite-url = "/configured/"\n',
                encoding="utf-8",
            )
            (output / "index.html").write_text(
                '<!doctype html><html><body><h1 id="landing">Landing</h1></body></html>',
                encoding="utf-8",
            )
            return subprocess.run(
                [sys.executable, str(CHECKER), str(output), "--site-prefix", site_prefix],
                check=False,
                capture_output=True,
                text=True,
            )

    def test_disagreement_fails_loudly(self) -> None:
        """A CLI prefix cannot disagree with the configuration used by mdBook."""

        checked = self.run_checker("/supplied/")

        self.assertEqual(checked.returncode, 1, checked.stdout + checked.stderr)
        self.assertIn(
            "--site-prefix '/supplied/' disagrees with",
            checked.stderr,
        )
        self.assertIn("[output.html].site-url '/configured/'", checked.stderr)

    def test_agreement_reaches_link_validation(self) -> None:
        """Matching prefixes permit the generated-document checks to run."""

        checked = self.run_checker("/configured/")

        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertIn(
            "generated link check: PASS (1 HTML files, 0 internal references, 1 anchors)",
            checked.stdout,
        )


if __name__ == "__main__":
    unittest.main()
