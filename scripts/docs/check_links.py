#!/usr/bin/env python3
"""Deterministically validate internal links, assets, and anchors in built HTML."""

from __future__ import annotations

import argparse
import collections
import html
import posixpath
import sys
import tomllib
from dataclasses import dataclass
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


REFERENCE_ATTRIBUTES = {"href", "src"}


@dataclass(frozen=True, order=True)
class Reference:
    """One HTML attribute that points at another resource."""

    source: str
    tag: str
    attribute: str
    value: str


class PageParser(HTMLParser):
    """Collect ids and resource references from one generated HTML page."""

    def __init__(self, source: str) -> None:
        super().__init__(convert_charrefs=True)
        self.source = source
        self.identifiers: list[str] = []
        self.references: list[Reference] = []

    def handle_starttag(self, tag: str, attributes: list[tuple[str, str | None]]) -> None:
        self._collect(tag, attributes)

    def handle_startendtag(self, tag: str, attributes: list[tuple[str, str | None]]) -> None:
        self._collect(tag, attributes)

    def _collect(self, tag: str, attributes: list[tuple[str, str | None]]) -> None:
        for name, raw_value in attributes:
            if raw_value is None:
                continue
            value = html.unescape(raw_value).strip()
            if name == "id" or tag == "a" and name == "name":
                self.identifiers.append(value)
            if name in REFERENCE_ATTRIBUTES and tag != "base":
                self.references.append(Reference(self.source, tag, name, value))
            if name == "srcset":
                for candidate in value.split(","):
                    url = candidate.strip().split(maxsplit=1)[0]
                    if url:
                        self.references.append(Reference(self.source, tag, name, url))


def configured_site_prefix(output: Path) -> tuple[Path, str]:
    """Read the site URL path from the staged book configuration."""

    configuration = output.parent / "book.toml"
    with configuration.open("rb") as stream:
        book = tomllib.load(stream)

    try:
        site_prefix = book["output"]["html"]["site-url"]
    except (KeyError, TypeError) as error:
        raise ValueError(
            f"{configuration} must declare [output.html].site-url"
        ) from error
    if not isinstance(site_prefix, str):
        raise ValueError(f"{configuration} [output.html].site-url must be a string")
    if not site_prefix.startswith("/") or not site_prefix.endswith("/"):
        raise ValueError(
            f"{configuration} [output.html].site-url must begin and end with '/'"
        )
    return configuration, site_prefix


def agreed_site_prefix(output: Path, supplied: str) -> str:
    """Require the supplied prefix to match the staged book configuration."""

    configuration, configured = configured_site_prefix(output)
    if supplied != configured:
        raise ValueError(
            f"--site-prefix {supplied!r} disagrees with "
            f"{configuration} [output.html].site-url {configured!r}"
        )
    return configured


def normalized_target(source: str, value: str, site_prefix: str) -> tuple[str, str] | None:
    """Resolve one internal reference to an output-relative path and fragment."""

    parsed = urlsplit(value)
    if parsed.scheme or parsed.netloc or value.startswith("//"):
        return None

    target_path = unquote(parsed.path)
    fragment = unquote(parsed.fragment)
    if target_path.startswith("/"):
        if not target_path.startswith(site_prefix):
            raise ValueError(f"root-relative path is outside site prefix {site_prefix!r}")
        relative = target_path[len(site_prefix) :]
    elif target_path:
        relative = posixpath.join(posixpath.dirname(source), target_path)
    else:
        relative = source

    normalized = posixpath.normpath(relative)
    if normalized == ".." or normalized.startswith("../"):
        raise ValueError("path escapes the generated site")
    if target_path.endswith("/"):
        normalized = "index.html" if normalized in {"", "."} else posixpath.join(normalized, "index.html")
    elif normalized in {"", "."}:
        normalized = "index.html"
    return normalized, fragment


def parse_pages(output: Path) -> tuple[dict[str, set[str]], list[Reference], list[str]]:
    """Parse generated pages and report duplicate ids."""

    pages: dict[str, set[str]] = {}
    references: list[Reference] = []
    errors: list[str] = []
    html_files = sorted(output.rglob("*.html"), key=lambda path: path.as_posix())
    if not html_files:
        raise ValueError(f"no generated HTML files under {output}")

    for path in html_files:
        relative = path.relative_to(output).as_posix()
        parser = PageParser(relative)
        parser.feed(path.read_text(encoding="utf-8"))
        parser.close()
        counts = collections.Counter(parser.identifiers)
        for identifier, count in sorted(counts.items()):
            if identifier and count > 1:
                errors.append(f"{relative}: duplicate id {identifier!r} ({count} occurrences)")
        pages[relative] = set(parser.identifiers)
        references.extend(parser.references)
    return pages, references, errors


def check(output: Path, site_prefix: str) -> tuple[int, int, int, list[str]]:
    """Validate every internal reference and referenced HTML fragment."""

    pages, references, errors = parse_pages(output)
    files = {
        path.relative_to(output).as_posix()
        for path in output.rglob("*")
        if path.is_file() and not path.is_symlink()
    }
    internal_count = 0

    for reference in sorted(references):
        try:
            resolved = normalized_target(reference.source, reference.value, site_prefix)
        except ValueError as error:
            errors.append(
                f"{reference.source}: {reference.tag}[{reference.attribute}]={reference.value!r}: {error}"
            )
            continue
        if resolved is None:
            continue

        internal_count += 1
        target, fragment = resolved
        if target not in files:
            errors.append(
                f"{reference.source}: {reference.tag}[{reference.attribute}]={reference.value!r}: "
                f"missing {target!r}"
            )
            continue
        if fragment and target.endswith(".html") and fragment not in pages.get(target, set()):
            errors.append(
                f"{reference.source}: {reference.tag}[{reference.attribute}]={reference.value!r}: "
                f"missing anchor {fragment!r} in {target!r}"
            )

    anchor_count = sum(len(identifiers) for identifiers in pages.values())
    return len(pages), internal_count, anchor_count, sorted(set(errors))


def main() -> int:
    """CLI entry point."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="mdBook HTML output directory")
    parser.add_argument(
        "--site-prefix",
        required=True,
        help="site URL path; must match staged book.toml [output.html].site-url",
    )
    arguments = parser.parse_args()

    site_prefix = arguments.site_prefix
    if not site_prefix.startswith("/") or not site_prefix.endswith("/"):
        print("generated link check: FAIL: --site-prefix must begin and end with '/'", file=sys.stderr)
        return 2

    output = arguments.output.resolve()
    try:
        site_prefix = agreed_site_prefix(output, site_prefix)
        page_count, reference_count, anchor_count, errors = check(output, site_prefix)
    except (OSError, UnicodeError, ValueError) as error:
        print(f"generated link check: FAIL: {error}", file=sys.stderr)
        return 1

    if errors:
        print("generated link check: FAIL", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(
        "generated link check: PASS "
        f"({page_count} HTML files, {reference_count} internal references, {anchor_count} anchors)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
