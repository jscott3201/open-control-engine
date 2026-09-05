#!/usr/bin/env python3
"""Check one prescribed Markdown traceability grammar, not product semantics.

Read-only, standard-library only. Existing evidence is tracked and regular; only
explicitly enumerated contract/transition evidence may be untracked before commit.
No rendering, generation, blessing, compilation or external URL fetch is performed.
"""

from __future__ import annotations

import argparse
import ast
from collections import Counter
from dataclasses import dataclass
import posixpath
from pathlib import Path
import re
import stat
import subprocess
import sys
from urllib.parse import urlsplit


DOCUMENT = "docs/product-contract.md"
PENDING = frozenset((DOCUMENT, "scripts/product_contract/check.py",
                      "scripts/product_contract/test_check.py",
                      "docs/facade-migration.md", "crates/oce-api/tests/sim_assertions.rs"))
POINTERS = ("README.md", "AGENTS.md", "TESTING.md", "docs/architecture.md",
            "docs/host-responsibilities.md", "docs/README.md")
HEADER = "| ID | Status | Actor | Owner | Requirement | Limitation | Grounding | Evidence |"
SEPARATOR = "| --- | --- | --- | --- | --- | --- | --- | --- |"
STATUSES = ("CURRENT", "HOST-OBLIGATION", "FUTURE")
LINK = re.compile(r"\[([^\[\]\n]+)\]\(([^()\s]+)\)")
IDENTIFIER = re.compile(r"\bPC-[0-9]+\b")
OBLIGATION = re.compile(r"\bmust\b", re.IGNORECASE)
ASSIGNMENT = re.compile(r"M[0-9]{2}-PR[0-9]{2}")
LINE_ANCHOR = re.compile(r"L([1-9][0-9]*)(?:-L([1-9][0-9]*))?")


class ContractError(ValueError):
    """A deterministic traceability refusal, not a semantic product verdict."""


def require(condition: bool, detail: str) -> None:
    if not condition:
        raise ContractError(detail)


def git(root: Path, *args: str) -> bytes:
    result = subprocess.run(["git", *args], cwd=root, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, check=False)
    require(result.returncode == 0, "repository: read-only Git query failed")
    return result.stdout


def local_path(source: str, destination: str) -> str:
    """Resolve the deliberately narrow repository-relative path spelling."""
    require(bool(destination) and not destination.startswith("/")
            and re.fullmatch(r"[A-Za-z0-9_./-]+", destination) is not None,
            "target: unsafe path")
    path = posixpath.normpath(posixpath.join(posixpath.dirname(source), destination))
    require(path != ".." and not path.startswith("../"), "target: escaping path")
    return path


class Repository:
    """Filesystem and clone-visibility checks; no graph completeness assumption."""

    def __init__(self, root: Path):
        self.root = root
        self.tracked = set(git(root, "ls-files", "-z", "--cached").decode().split("\0"))
        self.pending = set(git(root, "ls-files", "-z", "--others", "--exclude-standard",
                               "--", *sorted(PENDING)).decode().split("\0")) & PENDING
        self.cache: dict[str, str] = {}

    def read(self, path: str) -> str:
        if path in self.cache:
            return self.cache[path]
        require(path in self.tracked or path in self.pending,
                f"target: not clone-visible: {path}")
        ignored = subprocess.run(["git", "check-ignore", "--no-index", "--quiet", "--", path],
                                 cwd=self.root, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        require(ignored.returncode == 1, f"target: ignored or unverifiable: {path}")
        current = self.root
        parts = Path(path).parts
        for index, part in enumerate(parts):
            current = current / part
            try:
                mode = current.lstat().st_mode
            except OSError as error:
                raise ContractError(f"target: missing: {path}") from error
            require(not stat.S_ISLNK(mode), f"target: symlink: {path}")
            require(stat.S_ISREG(mode) if index == len(parts) - 1 else stat.S_ISDIR(mode),
                    f"target: not regular: {path}")
        try:
            text = current.read_bytes().decode("utf-8")
        except (OSError, UnicodeError) as error:
            raise ContractError(f"target: unreadable UTF-8: {path}") from error
        self.cache[path] = text
        return text


def headings(text: str) -> dict[str, str]:
    """ATX heading slugs for the cited documentation, not a Markdown engine."""
    found: dict[str, str] = {}
    counts: Counter[str] = Counter()
    lines = text.splitlines()
    starts = [(index, re.sub(r"^#{1,6} +", "", line)) for index, line in enumerate(lines)
              if re.match(r"^#{1,6} +", line)]
    for position, (start, title) in enumerate(starts):
        slug = re.sub(r"[^\w\- ]", "", title.lower()).replace(" ", "-")
        suffix = counts[slug]
        counts[slug] += 1
        anchor = slug if not suffix else f"{slug}-{suffix}"
        end = starts[position + 1][0] if position + 1 < len(starts) else len(lines)
        found[anchor] = "\n".join(lines[start + 1:end]).strip()
    return found


def link_target(repository: Repository, source: str, destination: str) -> tuple[str, str, str]:
    """Validate local link paths and fragments, returning the selected source text."""
    parsed = urlsplit(destination)
    require(not parsed.scheme and not parsed.netloc and not parsed.query,
            "target: local evidence required")
    require(destination.count("#") <= 1 and not destination.endswith("#"),
            "target: malformed fragment")
    path = local_path(source, parsed.path) if parsed.path else source
    text = repository.read(path)
    fragment = parsed.fragment
    selected = text
    if fragment:
        if path.endswith(".md"):
            anchors = headings(text)
            require(fragment in anchors, f"target: missing heading: {path}#{fragment}")
            selected = anchors[fragment]
        else:
            match = LINE_ANCHOR.fullmatch(fragment)
            if match is None:
                raise ContractError(f"target: expected line anchor: {path}")
            start = int(match[1])
            end = int(match[2] or match[1])
            lines = text.splitlines()
            require(1 <= start <= end <= len(lines), f"target: line range: {path}#{fragment}")
            selected = "\n".join(lines[start - 1:end])
    return path, fragment, selected


def link_list(value: str) -> list[tuple[str, str]]:
    pieces = value.split("; ")
    matches = [LINK.fullmatch(piece) for piece in pieces]
    if any(match is None for match in matches):
        raise ContractError("table: expected link list")
    return [(match[1], match[2]) for match in matches if match is not None]


def test_locator(repository: Repository, name: str, destination: str) -> None:
    """Check a named declaration in a line range; execution/relevance stay separate."""
    require(re.fullmatch(r"[a-z][a-z0-9_]+", name) is not None, "evidence: invalid test name")
    path, fragment, selected = link_target(repository, DOCUMENT, destination)
    require(bool(LINE_ANCHOR.fullmatch(fragment)), "evidence: test needs line anchor")
    text = repository.read(path)
    if path.endswith(".rs"):
        declaration = rf"(?m)^\s*fn {re.escape(name)}\(\)"
        require(re.search(declaration, selected) is not None, "evidence: test not in range")
        # Intentionally lexical and narrow: current cited tests use this exact form.
        active = rf"(?m)^\s*#\[test\]\s*\n\s*fn {re.escape(name)}\(\)"
        require(len(re.findall(active, text)) == 1, "evidence: unique test declaration required")
    elif path.endswith(".py"):
        require(re.search(rf"(?m)^\s*def {re.escape(name)}\(self\)(?: -> None)?:", selected) is not None,
                "evidence: test not in range")
        try:
            tree = ast.parse(text)
        except SyntaxError as error:
            raise ContractError("evidence: invalid Python test source") from error
        tests = [node for node in ast.walk(tree) if isinstance(node, ast.FunctionDef)
                 and node.name == name and name.startswith("test_") and not node.decorator_list]
        require(len(tests) == 1, "evidence: unique test declaration required")
    else:
        raise ContractError("evidence: unsupported test source")


@dataclass(frozen=True)
class Requirement:
    identifier: str
    status: str
    actor: str
    owner: str
    statement: str
    limitation: str
    grounding: str
    evidence: str


def parse_document(text: str) -> tuple[str, str, list[Requirement]]:
    """Read the sole requirement table; refuse unsupported hiding/expansion syntax."""
    require("\r" not in text and "\t" not in text and text.endswith("\n"),
            "grammar: expected LF text with final newline")
    require(not any(token in text for token in ("<", ">", "&", "\\", "{{", "}}", "```", "~~~")),
            "grammar: unsupported markup")
    plain = LINK.sub("", text)
    require("[" not in plain and "]" not in plain, "grammar: unsupported link spelling")
    revisions = re.findall(r"(?m)^Document revision: ([1-9][0-9]*)$", text)
    hashes = re.findall(r"(?m)^Grounding SHA: ([0-9a-f]{40})$", text)
    require(len(revisions) == len(hashes) == 1, "metadata: revision and grounding SHA required once")
    lines = text.splitlines()
    require(lines.count("## Requirements") == 1 and lines.count(HEADER) == 1,
            "table: one Requirements table required")
    start = lines.index(HEADER)
    require(lines[start - 2:start] == ["## Requirements", ""] and
            lines[start + 1:start + 2] == [SEPARATOR], "table: header or separator")
    rows: list[Requirement] = []
    end = start + 2
    while end < len(lines) and lines[end].startswith("|"):
        cells = lines[end].split("|")
        require(len(cells) == 10 and cells[0] == cells[-1] == "", "table: eight cells required")
        values = [cell.strip() for cell in cells[1:-1]]
        require(all(re.search(r"[A-Za-z0-9]", cell) for cell in values), "table: empty field")
        row = Requirement(*values)
        require(re.fullmatch(r"PC-[0-9]{3}", row.identifier) is not None, "table: invalid ID")
        require(row.status in STATUSES, f"table: unknown status: {row.identifier}")
        require(re.fullmatch(r"[A-Za-z][A-Za-z -]*", row.actor) is not None and
                re.fullmatch(r"[A-Za-z][A-Za-z -]*", row.owner) is not None,
                f"table: actor and owner required: {row.identifier}")
        require(row.statement.startswith("MUST ") and len(OBLIGATION.findall(row.statement)) == 1,
                f"table: one obligation required: {row.identifier}")
        unformatted_row = re.sub(r"[`*_]", "", lines[end])
        require(len(OBLIGATION.findall(unformatted_row)) == 1,
                "table: obligation outside Requirement cell")
        require(IDENTIFIER.findall(lines[end]) == [row.identifier], "table: orphan ID in row")
        rows.append(row)
        end += 1
    require(bool(rows), "table: empty requirements")
    ids = [row.identifier for row in rows]
    require(len(ids) == len(set(ids)), "table: duplicate ID")
    require(ids == [f"PC-{index:03}" for index in range(1, len(ids) + 1)],
            "table: omitted or unordered ID")
    outside = "\n".join(lines[:start] + lines[end:])
    # Formatting cannot hide an outside-table keyword by splitting its letters.
    unformatted = re.sub(r"[`*_]", "", outside)
    require(not OBLIGATION.search(unformatted), "table: obligation outside Requirement cell")
    outside_ids = IDENTIFIER.findall(outside)
    require(all(identifier in ids for identifier in outside_ids), "table: orphan ID")
    require(set(row.status for row in rows) == set(STATUSES), "table: all statuses required")
    owner_section = text.split("## Authority and owners\n", 1)
    require(len(owner_section) == 2, "table: owner delegation required")
    owner_text = owner_section[1].split("\n## ", 1)[0]
    owners = re.findall(r"(?m)^\| ([A-Za-z][A-Za-z -]*) \| .+ \|$", owner_text)
    owners = [owner for owner in owners if owner != "Owner role"]
    require(bool(owners) and len(owners) == len(set(owners)), "table: unique owner delegation required")
    require(all(row.owner in owners for row in rows), "table: undelegated owner")
    require("## Change record" in lines, "metadata: change record required")
    record = headings(text).get("change-record", "")
    require(re.search(rf"(?m)^- Revision {revisions[0]}, [0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}}: .+", record)
            is not None, "metadata: current revision change record required")
    return revisions[0], hashes[0], rows


def validate(repository: Repository) -> str:
    """Return a deterministic report after structural traceability validation."""
    text = repository.read(DOCUMENT)
    revision, grounding, rows = parse_document(text)
    for match in LINK.finditer(text):
        parsed = urlsplit(match[2])
        if parsed.scheme:
            require(parsed.scheme == "https" and bool(parsed.netloc), "target: unsupported external URL")
        else:
            link_target(repository, DOCUMENT, match[2])
    future_headings = headings(text)
    assignments: set[str] = set()
    for row in rows:
        for _, destination in link_list(row.grounding):
            link_target(repository, DOCUMENT, destination)
        if row.evidence.startswith("test "):
            require(row.status != "FUTURE", "evidence: future outcome assignment required")
            for name, destination in link_list(row.evidence[5:]):
                test_locator(repository, name, destination)
        elif row.evidence.startswith("future "):
            links = link_list(row.evidence[7:])
            require(len(links) == 1 and ASSIGNMENT.fullmatch(links[0][0]) is not None,
                    "evidence: one named future assignment required")
            name, destination = links[0]
            require(destination.startswith("#"), "evidence: future outcome must be in this document")
            _, fragment, outcome = link_target(repository, DOCUMENT, destination)
            require(fragment in future_headings and outcome.startswith(f"{name}: "),
                    "evidence: missing future description")
            require(len(outcome[len(name) + 2:].split()) >= 12,
                    "evidence: missing future description")
            assignments.add(name)
        else:
            raise ContractError(f"evidence: test or future assignment required: {row.identifier}")
    outcomes = re.findall(r"(?m)^(M[0-9]{2}-PR[0-9]{2}): ", text)
    require(len(outcomes) == len(set(outcomes)) and set(outcomes) == assignments,
            "evidence: duplicate or orphan future outcome")
    for source in POINTERS:
        pointer_text = repository.read(source)
        destinations = [match[2] for match in LINK.finditer(pointer_text)]
        expected = posixpath.relpath(DOCUMENT, posixpath.dirname(source) or ".")
        require(expected in destinations, f"pointer: missing product contract in {source}")
        link_target(repository, source, expected)
        if source == "README.md":
            validate_readme_facade(pointer_text)
    counts = Counter(row.status for row in rows)
    status_lines = "\n".join(f"{status}: {counts[status]}" for status in STATUSES)
    return (f"product contract: OK\nDocument revision: {revision}\nGrounding SHA: {grounding}\n"
            f"Requirements: {len(rows)}\n{status_lines}\nFuture outcomes: {len(assignments)}\n"
            f"Integration pointers: {len(POINTERS)}\n"
            "Scope: traceability only; semantics and host compliance are not proven.\n")


def validate_readme_facade(text: str) -> None:
    """Reject known obsolete current-claim bullets, not historical API mentions.

    These are narrow lexical sentinels over public guidance, not a semantic proof
    or a ban on spelling removed names in migration/history accounts.
    """
    for bullet in re.findall(r"(?m)^- .+(?:\n[ \t]+.+)*", text):
        normalized = " ".join(re.sub(r"[`*]", "", bullet).split()).casefold()
        require(not normalized.startswith("- two stable loader signatures are placeholders."),
                "facade: callable removed loaders in README.md")
        require(not normalized.startswith("- assertion events are warning-only today. "
                                          "although assertlevel::error is public for surface stability"),
                "facade: removed assertion severity advertised as public in README.md")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    try:
        report = validate(Repository(Path(__file__).resolve().parents[2]))
    except (ContractError, OSError, UnicodeError, ValueError) as error:
        print(f"product contract: FAIL: {error}", file=sys.stderr)
        return 1
    print(report, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
