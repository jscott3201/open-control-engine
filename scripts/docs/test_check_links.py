#!/usr/bin/env python3
"""Regression tests for generated-document link checks."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


CHECKER = Path(__file__).with_name("check_links.py")


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
