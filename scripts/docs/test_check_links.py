#!/usr/bin/env python3
"""Regression tests for generated-document link checks."""

from __future__ import annotations

import json
import html
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

    def test_staged_navigation_includes_strict_bit_evidence_exactly_once(self) -> None:
        """A copied Markdown file alone does not produce the linked HTML chapter."""
        with tempfile.TemporaryDirectory() as temporary:
            staged, _, revision = docs_stage.stage_book(Path(temporary) / "book", "a" * 40)
            summary = (staged / "src" / "SUMMARY.md").read_text()
            self.assertEqual(summary.splitlines().count(
                "- [Pinned strict-bit signal evidence](docs/strict-bit-evidence.md)"), 1)
            self.assertEqual(summary.count("(docs/strict-bit-evidence.md)"), 1)
            chapter = (staged / "src" / "docs" / "strict-bit-evidence.md").read_text()
            self.assertIn(f"/blob/{revision}/crates/oce-conformance/tests/fixtures/strict_bits/", chapter)

    def test_staged_navigation_includes_complete_frame_contract_exactly_once(self) -> None:
        """The linked frame contract is staged and has one navigation entry."""

        with tempfile.TemporaryDirectory() as temporary:
            staged, _, _ = docs_stage.stage_book(Path(temporary) / "book", "a" * 40)

            summary = (staged / "src" / "SUMMARY.md").read_text(encoding="utf-8")
            chapter = "- [Complete-frame contract](docs/complete-frame-contract.md)"
            self.assertIn(chapter, summary.splitlines())
            # Count the destination too: a duplicate under a different title is still a duplicate.
            self.assertEqual(summary.count("(docs/complete-frame-contract.md)"), 1)
            self.assertTrue((staged / "src" / "docs" / "complete-frame-contract.md").is_file())

    def test_staged_navigation_includes_state_compatibility_exactly_once(self) -> None:
        """The linked state contract needs a chapter entry, not just a staged copy."""

        with tempfile.TemporaryDirectory() as temporary:
            staged, _, _ = docs_stage.stage_book(Path(temporary) / "book", "a" * 40)

            summary = (staged / "src" / "SUMMARY.md").read_text(encoding="utf-8")
            chapter = "- [State continuation and portability](docs/state-compatibility.md)"
            self.assertEqual(summary.splitlines().count(chapter), 1)
            self.assertEqual(summary.count("(docs/state-compatibility.md)"), 1)
            self.assertTrue((staged / "src" / "docs" / "state-compatibility.md").is_file())

    def test_staged_navigation_includes_replay_record_exactly_once(self) -> None:
        """The replay contract is staged and reachable through one generated chapter."""

        with tempfile.TemporaryDirectory() as temporary:
            staged, _, _ = docs_stage.stage_book(Path(temporary) / "book", "a" * 40)

            summary = (staged / "src" / "SUMMARY.md").read_text(encoding="utf-8")
            chapter = "- [Canonical replay record](docs/replay-record.md)"
            self.assertEqual(summary.splitlines().count(chapter), 1)
            self.assertEqual(summary.count("(docs/replay-record.md)"), 1)
            self.assertTrue((staged / "src" / "docs" / "replay-record.md").is_file())

    def test_authority_projection_navigation_and_inert_history(self):
        """Stage the index byte-exactly; history locators must not become links."""
        with tempfile.TemporaryDirectory() as temporary:
            staged, _, revision = docs_stage.stage_book(Path(temporary) / "book", "a" * 40)
            source = staged / "src" / "docs"
            summary = (staged / "src" / "SUMMARY.md").read_text()
            self.assertEqual(summary.splitlines().count(
                "- [Authority claims and supersession](docs/authority-claims.md)"), 1)
            self.assertEqual((source / "authority-claims.json").read_bytes(),
                             (docs_stage.repository_root() / "docs/authority-claims.json").read_bytes())
            projection = (source / "authority-claims.md").read_text()
            self.assertIn(f"/blob/{revision}/scripts/authority_claims/check.py", projection)
            self.assertNotRegex(projection, r"\]\([^)]*(?:_spec/|_research/)")
            self.assertIn("<code>_spec/", html.unescape(projection))

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

    def test_staged_navigation_includes_package_publication_authority(self) -> None:
        """The package policy is a chapter and its exhaustive ledger is an asset."""

        with tempfile.TemporaryDirectory() as temporary:
            staged, _, _ = docs_stage.stage_book(Path(temporary) / "book", "a" * 40)

            source = staged / "src" / "docs"
            summary = (staged / "src" / "SUMMARY.md").read_text(encoding="utf-8")
            chapter = (
                "- [Package, feature, and publication policy]"
                "(docs/package-publication-policy.md)"
            )
            self.assertEqual(summary.splitlines().count(chapter), 1)
            self.assertTrue((source / "package-publication-policy.md").is_file())
            self.assertEqual(
                (source / "package-publication-ledger.json").read_bytes(),
                (
                    docs_stage.repository_root()
                    / "docs"
                    / "package-publication-ledger.json"
                ).read_bytes(),
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

    def test_package_policy_evidence_paths_resolve(self) -> None:
        """The package ledger's authority and executable validator are clone-visible."""

        root = docs_stage.repository_root()
        ledger = json.loads(
            (root / "docs" / "package-publication-ledger.json").read_text(encoding="utf-8")
        )

        self.assertTrue((root / ledger["authority"]).is_file())
        self.assertTrue((root / "scripts" / "package_policy" / "validate.py").is_file())
        self.assertEqual(len(ledger["packages"]), 17)


class VerificationSummaryTests(unittest.TestCase):
    """Narrow sentinels for the strict-bit comparison and PR-coverage summaries."""

    def assert_numeric_boundary(self, text):
        text = " ".join(text.lower().split())
        for fact in ("qualified linux", "unqualified", "21", "1e-12", "strict-bit-evidence.md"):
            self.assertIn(fact, text)

    def assert_ci_boundary(self, text):
        text = " ".join(text.lower().split())
        for fact in ("scoped", "strict-bit", "per-pr", "remainder", "release"):
            self.assertIn(fact, text)

    def test_public_numeric_and_ci_summaries_distinguish_the_scoped_subset(self):
        root = docs_stage.repository_root()
        readme = (root / "README.md").read_text().split("## How it is verified\n", 1)[1].split("\n---", 1)[0]
        evidence = (root / "docs/verification-evidence.md").read_text()
        summary = evidence.split("## What this adds up to\n", 1)[1]
        for text in (readme, summary):
            self.assert_numeric_boundary(text)
            self.assert_ci_boundary(text)
        row = next(line for line in evidence.splitlines() if line.startswith("| Tier-1 per-block"))
        for fact in ("278", "257", "21", "qualified Linux", "unqualified"):
            self.assertIn(fact, row)
        ci = evidence.split("### The CI split, read in the dangerous direction\n", 1)[1].split("\n###", 1)[0]
        self.assert_ci_boundary(ci)
        ci_page = (root / "docs/ci-and-the-gate.md").read_text().split("## Dev-light, release-heavy\n", 1)[1].split("\n##", 1)[0]
        self.assert_ci_boundary(ci_page)

    def test_historical_tolerance_only_and_no_conformance_pr_claims_are_not_accepted(self):
        for stale in (
            "389 are compared bit-exactly and the 21 transcendental, psychrometric, and solar "
            "Real goldens use a documented 1e-12 aligned-tolerance band.",
            "390 oracle comparisons (369 bit-exact, 21 under the documented 1e-12 aligned-tolerance band)",
        ):
            with self.assertRaises(AssertionError):
                self.assert_numeric_boundary(stale)
        with self.assertRaises(AssertionError):
            self.assert_ci_boundary("The oce-api comparison tests run per PR, but oce-conformance/tests/ "
                                    "does not; the complete set runs only on release PRs.")


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
