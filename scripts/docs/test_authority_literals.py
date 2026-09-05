#!/usr/bin/env python3
"""Exercise literal claims through real staging and the pinned mdBook renderer.

Run explicitly with --mdbook; a missing/wrong tool or empty suite is a failure.
This suite belongs to docs-pages, not the standard-library-only light gate.
"""

import argparse
import copy
import html
from html.parser import HTMLParser
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import stage

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "scripts/authority_claims"))
import render as claims_render
from model import OUTPUT, validate
from observe import Repository, observe

REVISION = "a" * 40
SENTINEL = "HISTORICAL_FIXTURE_CONTENT_MUST_NOT_BE_INCLUDED"


def html_only_code(value):
    """Retained faulty formatter for in-memory, end-to-end sensitivity controls."""
    return "<code>" + html.escape(str(value)).replace("|", "&#124;").replace("`", "&#96;") + "</code>"


class MainDocument(HTMLParser):
    """Read browser-decoded main content, code text, and actual element topology."""

    def __init__(self, source):
        super().__init__(convert_charrefs=True)
        self.in_main = False
        self.in_code = False
        self.elements = []
        self.codes = []
        self.content = []
        self.feed(source)
        self.close()

    def handle_starttag(self, tag, attrs):
        if tag == "main":
            self.in_main = True
        if self.in_main:
            self.elements.append((tag, attrs))
            if tag == "code":
                self.in_code = True
                self.codes.append("")

    def handle_endtag(self, tag):
        if tag == "main":
            self.in_main = False
        if tag == "code":
            self.in_code = False

    def handle_data(self, data):
        if self.in_main:
            self.content.append(data)
            if self.in_code:
                self.codes[-1] += data


class AuthorityLiteralTests(unittest.TestCase):
    mdbook = "mdbook"

    @classmethod
    def setUpClass(cls):
        version = subprocess.run([cls.mdbook, "--version"], check=True, capture_output=True, text=True)
        expected = "mdbook v" + (ROOT / "site/mdbook-version").read_text().strip()
        if version.stdout.strip() != expected:
            raise AssertionError(f"expected {expected}, got {version.stdout.strip()}")
        cls.index, cls.observed = observe(Repository(ROOT))

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        # Stage the real book first. Only the disposable projection copy is replaced
        # below, then passed through the exact rewrite step used by stage_book.
        self.book, _, _ = stage.stage_book(self.root / "book", REVISION)
        self.projection = self.book / "src" / OUTPUT
        # Isolate repository-link lookups, including hostile ../_spec locators.
        # Current source bindings are real source copies; no history is materialized.
        self.source = self.root / "source"
        self.source.mkdir()
        paths = {"scripts/authority_claims/check.py"}
        paths.update(row["path"] for row in self.observed["public-surface"]["value"]["baselines"])
        for row in self.index["facts"]:
            for field in ("source", "verifier"):
                paths.add(row[field])
        for name in paths:
            target = self.source / name
            if (ROOT / name).is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / name, target)

    def build(self, mutation=False):
        result = subprocess.run([self.mdbook, "build", str(self.book)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("ERROR", result.stderr, result.stderr)
        if not mutation:
            self.assertNotIn("WARN", result.stderr, result.stderr)
        return MainDocument((self.book / "book/docs/authority-claims.html").read_bytes().decode())

    def stage_literal(self, literal, field="historical_locator"):
        index = copy.deepcopy(self.index)
        if field == "limit":
            next(row for row in index["facts"] if row["mode"] == "review-only")[field] = literal
        else:
            index["supersession"][0][field] = literal
        validate(index)  # Exercise accepted input, not a schema-rejected shortcut.
        self.projection.write_bytes(claims_render.render(index, self.observed))
        stage.rewrite_staged_links(self.source, self.projection, PurePosixPath(OUTPUT), REVISION)

    def stage_fragment(self, literal):
        # The schema currently rejects control characters, but the shared display
        # encoder must not let line breaks restore Markdown or helper syntax.
        source = claims_render.render(self.index, self.observed)
        self.projection.write_bytes(source + ("\n" + claims_render.code(literal) + "\n").encode())
        stage.rewrite_staged_links(self.source, self.projection, PurePosixPath(OUTPUT), REVISION)

    def assert_literal_render(self, literal, control):
        rendered = self.build()
        self.assertEqual(rendered.elements, control.elements, "literal introduced an element/attribute")
        self.assertEqual(rendered.codes.count(literal), 1, "displayed text changed or was interpreted")
        self.assert_current_authority_links(rendered)
        self.assertNotIn(SENTINEL, (self.book / "book/docs/authority-claims.html").read_text())

    def assert_current_authority_links(self, document):
        links = [dict(attrs).get("href") for tag, attrs in document.elements if tag == "a"]
        self.assertIn("public-surface-contract.html", links)
        self.assertIn(f"{stage.REPOSITORY_URL}/blob/{REVISION}/crates/oce-api/src/state.rs", links)

    def test_unmodified_projection_builds_with_current_authority_links(self):
        self.stage_literal(self.index["supersession"][0]["historical_locator"])
        self.assert_current_authority_links(self.build())

    def test_missing_historical_markdown_target_is_only_visible_text(self):
        control = self.build()
        literal = "[historical](../_spec/absent.md)"
        self.assertFalse((self.source / "_spec").exists())
        try:
            self.stage_literal(literal)
        except ValueError as error:
            self.fail(f"historical literal became a source dependency: {error}")
        self.assert_literal_render(literal, control)

    def test_markdown_html_entities_and_images_stay_literal_in_every_display_field(self):
        control = self.build()
        literals = [
            "![historical](../_spec/absent.png)",
            "[historical][reference] [reference][] [reference]",
            "<https://example.invalid/historical> <historical@example.invalid>",
            "https://example.invalid/historical",
            '<a href="../_spec/absent.md">link</a><img src="../_spec/absent.png">',
            '</code><script>alert("literal")</script><code>',
            "&#91;historical&#93; &#123;&#123;#include ../../_spec/absent.md&#125;&#125; &amp; &#x60;",
            r"\[historical](../_spec/absent.md) \{{#include ../../_spec/absent.md}}",
            "`code` **strong** _emphasis_ ~~strike~~ | table | # heading {braces} [brackets] 'quotes'",
            "plain Unicode — Δ 中文, repeated [[{{**``!!<>}}]]",
        ]
        for field in ("historical_locator", "subject", "reason", "responsibility_owner", "limit"):
            for literal in literals:
                with self.subTest(field=field, literal=literal):
                    self.stage_literal(literal, field)
                    self.assert_literal_render(literal, control)

    def test_absent_mdbook_helper_targets_never_become_file_dependencies(self):
        control = self.build()
        for helper in ("include", "rustdoc_include", "playground", "playpen"):
            for selector in ("", ":1", ":1:2", ":section"):
                literal = "{{#" + helper + " ../../_spec/absent.rs" + selector + "}}"
                with self.subTest(literal=literal):
                    self.assertFalse((self.book / "_spec/absent.rs").exists())
                    self.stage_literal(literal)
                    self.assert_literal_render(literal, control)
        for literal in ("{{#include    ../../_spec/absent.rs   }}",
                        "{{#playground ../../_spec/absent.rs editable}}"):
            with self.subTest(literal=literal):
                self.stage_literal(literal)
                self.assert_literal_render(literal, control)

    def test_line_separators_and_reference_definitions_cannot_restore_syntax(self):
        self.stage_fragment("plain control")
        control = self.build()
        for literal in (
            "[historical][reference]\n\n[reference]: ../_spec/absent.md",
            "![historical][reference]\r\n[reference]: ../_spec/absent.png",
            "```rust\n{{#include ../../_spec/absent.rs}}\n```",
            "{{#include\n../../_spec/absent.rs}}\r{{#rustdoc_include\t../../_spec/absent.rs}}",
            "</code>\n\n# heading\n\n---\n\n<img src='../_spec/absent.png'>",
            "literal entities: &NewLine; &#10; &#13; &#x5b; &quot; &lt; &#38;#123;",
            "space  tab\tLF\nCR\rCRLF\r\n'\"\\|[]{}*`~_!",
        ):
            with self.subTest(literal=literal):
                self.stage_fragment(literal)
                self.assert_literal_render(literal, control)

    def test_html_only_mutation_reproduces_historical_lookup_and_restores_cleanly(self):
        literal = "[historical](../_spec/absent.md)"
        with patch.object(claims_render, "code", html_only_code):
            with self.assertRaisesRegex(ValueError, r"does not exist: ../_spec/absent\.md"):
                self.stage_literal(literal)
        self.stage_literal(literal)
        self.assertEqual(self.build().codes.count(literal), 1)

    def test_live_helpers_read_a_sentinel_only_when_encoding_is_disabled(self):
        control = self.build()
        # Outside src/, so mdBook cannot copy this sentinel as an ordinary asset.
        sentinel = self.book / "_spec/sentinel.rs"
        sentinel.parent.mkdir()
        sentinel.write_text("// " + SENTINEL + "\n")
        for helper in ("include", "rustdoc_include", "playground", "playpen"):
            literal = "{{#" + helper + " ../../_spec/sentinel.rs}}"
            with self.subTest(helper=helper):
                with patch.object(claims_render, "code", html_only_code):
                    self.stage_literal(literal)
                    interpreted = self.build(mutation=True)
                    self.assertIn(SENTINEL, "".join(interpreted.content), "helper control did not actually read its file")
                    self.assertNotIn(literal, interpreted.codes)
                self.stage_literal(literal)
                self.assert_literal_render(literal, control)
        sentinel.unlink()
        self.stage_literal(literal)
        self.assert_literal_render(literal, control)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mdbook", required=True, help="installed checksum/version-pinned mdBook executable")
    args = parser.parse_args()
    AuthorityLiteralTests.mdbook = args.mdbook
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(AuthorityLiteralTests)
    if not suite.countTestCases():
        raise SystemExit("authority literal pipeline: no tests discovered")
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
