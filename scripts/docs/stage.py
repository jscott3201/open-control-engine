#!/usr/bin/env python3
"""Stage the documentation corpus without modifying source-file contents.

The tracked files under ``docs/`` are copied into an owned, disposable build
tree. Repository-relative links are rewritten only in those staged copies so
source and test paths resolve to the exact repository revision being built.
"""

from __future__ import annotations

import argparse
import posixpath
import re
import shutil
import subprocess
import sys
from pathlib import Path, PurePosixPath
from urllib.parse import quote, urlsplit


STAGE_MARKER = ".open-control-docs-stage"
STAGE_MARKER_CONTENT = "owned disposable docs staging tree\n"
REPOSITORY_URL = "https://github.com/jscott3201/open-control-engine"
LINK = re.compile(
    r"(?P<prefix>!?\[[^\]\n]*\]\()"
    r"(?P<destination>[^)\s]+)"
    r"(?P<suffix>(?:\s+[^)]*)?\))"
)


def repository_root() -> Path:
    """Return the repository root derived from this script's tracked path."""

    return Path(__file__).resolve().parents[2]


def git_output(root: Path, *arguments: str) -> bytes:
    """Run a read-only Git query and return its stdout."""

    result = subprocess.run(
        ["git", *arguments],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout


def resolve_revision(root: Path, requested: str | None) -> str:
    """Resolve and validate the exact revision used for repository links."""

    revision = requested or git_output(root, "rev-parse", "HEAD").decode().strip()
    if re.fullmatch(r"[0-9a-f]{40}", revision) is None:
        raise ValueError(f"repository revision must be a full lowercase SHA, got {revision!r}")
    return revision


def prepare_stage(root: Path, requested: Path) -> Path:
    """Create an owned staging directory without risking source deletion."""

    if requested.is_symlink():
        raise ValueError(f"staging destination must not be a symlink: {requested}")

    stage = requested.resolve()
    docs = (root / "docs").resolve()
    target_root = (root / "target").resolve()
    if stage == root or (root in stage.parents and target_root not in stage.parents):
        raise ValueError(f"refusing unsafe staging destination: {stage}")
    if stage == docs or docs in stage.parents:
        raise ValueError(f"staging destination must stay outside docs/: {stage}")

    if stage.exists():
        marker = stage / STAGE_MARKER
        if not marker.is_file() or marker.read_text(encoding="utf-8") != STAGE_MARKER_CONTENT:
            raise ValueError(f"refusing to replace unowned staging directory: {stage}")
        shutil.rmtree(stage)

    stage.mkdir(parents=True)
    (stage / STAGE_MARKER).write_text(STAGE_MARKER_CONTENT, encoding="utf-8")
    return stage


def tracked_docs(root: Path) -> list[PurePosixPath]:
    """Return every tracked docs path in deterministic order."""

    raw_paths = git_output(root, "ls-files", "-z", "--", "docs").split(b"\0")
    paths = [PurePosixPath(raw.decode("utf-8")) for raw in raw_paths if raw]
    paths.sort(key=str)
    if not paths:
        raise ValueError("the tracked docs corpus is empty")
    for path in paths:
        if path.parts[0] != "docs" or ".." in path.parts:
            raise ValueError(f"unsafe tracked docs path: {path}")
        source = root.joinpath(*path.parts)
        if source.is_symlink() or not source.is_file():
            raise ValueError(f"tracked docs source must be a regular file: {path}")
    return paths


def repository_link(root: Path, source: PurePosixPath, target: str, revision: str) -> str:
    """Rewrite a staged ``../`` link to an exact GitHub blob URL."""

    parsed = urlsplit(target)
    if parsed.scheme or parsed.netloc or not parsed.path.startswith("../"):
        return target

    normalized = posixpath.normpath(posixpath.join(str(source.parent), parsed.path))
    if normalized == ".." or normalized.startswith("../"):
        raise ValueError(f"link escapes the repository from {source}: {target}")

    repository_path = PurePosixPath(normalized)
    local_target = root.joinpath(*repository_path.parts)
    if not local_target.exists():
        raise ValueError(f"repository link from {source} does not exist: {target}")

    rewritten = f"{REPOSITORY_URL}/blob/{revision}/{quote(str(repository_path), safe='/')}"
    if parsed.query:
        rewritten += f"?{parsed.query}"
    if parsed.fragment:
        rewritten += f"#{parsed.fragment}"
    return rewritten


def rewrite_staged_links(root: Path, staged_path: Path, source: PurePosixPath, revision: str) -> None:
    """Rewrite repository-only links in one staged Markdown copy."""

    original = staged_path.read_bytes().decode("utf-8")

    def replace(match: re.Match[str]) -> str:
        destination = repository_link(root, source, match.group("destination"), revision)
        return f"{match.group('prefix')}{destination}{match.group('suffix')}"

    rewritten = LINK.sub(replace, original)
    staged_path.write_bytes(rewritten.encode("utf-8"))


def extract_quickstart(root: Path) -> str:
    """Extract the first Rust fence from the guarded repository README."""

    readme = (root / "README.md").read_text(encoding="utf-8")
    opening = "```rust\n"
    start = readme.find(opening)
    if start < 0:
        raise ValueError("README.md has no ```rust Quickstart fence")
    start += len(opening)
    end = readme.find("```", start)
    if end < 0:
        raise ValueError("README.md Quickstart fence is unterminated")
    return readme[start:end]


def write_generated_chapters(stage: Path, quickstart: str) -> None:
    """Write adapter-owned landing, Quickstart, and navigation chapters."""

    source = stage / "src"
    source.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(repository_root() / "site" / "index.md", source / "index.md")

    quickstart_page = (
        "# Embed the engine\n\n"
        "This is the repository Quickstart, extracted mechanically from the first Rust block in "
        "`README.md`. That block is byte-compared with the compiled "
        "`crates/oce-api/examples/quickstart.rs` example.\n\n"
        "> Before connecting real equipment, read [Host responsibilities]"
        "(docs/host-responsibilities.md). The host owns sample quality, missing-data, timing, "
        "fault, and safe-state policy.\n\n"
        "```rust\n"
        f"{quickstart}"
        "```\n\n"
        "Continue with [Architecture](docs/architecture.md) and "
        "[CDL coverage](docs/cdl-coverage.md).\n"
    )
    (source / "quickstart.md").write_text(quickstart_page, encoding="utf-8")

    summary = """# Summary

[Open Control Engine](index.md)
[Embed the engine](quickstart.md)

# Reference

- [Documentation map](docs/README.md)
- [Architecture](docs/architecture.md)
- [Execution profile](docs/execution-profile.md)
- [Verification and evidence](docs/verification-evidence.md)
- [CDL coverage](docs/cdl-coverage.md)
- [CXF round trip](docs/cxf-round-trip.md)
- [CXF composite subset](docs/cxf-composite-subset.md)
- [Host responsibilities](docs/host-responsibilities.md)
- [CI and the gate](docs/ci-and-the-gate.md)
- [Benchmarks](docs/benchmarks.md)
"""
    (source / "SUMMARY.md").write_text(summary, encoding="utf-8")


def stage_book(output: Path, requested_revision: str | None) -> tuple[Path, int, str]:
    """Create the complete disposable mdBook input tree."""

    root = repository_root()
    revision = resolve_revision(root, requested_revision)
    stage = prepare_stage(root, output)
    source_root = stage / "src"
    source_root.mkdir()

    paths = tracked_docs(root)
    for source_path in paths:
        source = root.joinpath(*source_path.parts)
        destination = source_root.joinpath(*source_path.parts)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        if destination.suffix == ".md":
            rewrite_staged_links(root, destination, source_path, revision)

    shutil.copyfile(root / "site" / "book.toml", stage / "book.toml")
    shutil.copyfile(root / "site" / "custom.css", stage / "custom.css")
    write_generated_chapters(stage, extract_quickstart(root))
    (stage / "source-revision.txt").write_text(f"{revision}\n", encoding="utf-8")

    if any(path.name == ".DS_Store" for path in stage.rglob("*")):
        raise ValueError("ignored .DS_Store debris reached the staging tree")
    return stage, len(paths), revision


def main() -> int:
    """CLI entry point."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="owned disposable book root")
    parser.add_argument("--revision", help="full Git revision for repository-only links")
    arguments = parser.parse_args()

    try:
        stage, count, revision = stage_book(arguments.output, arguments.revision)
    except (OSError, subprocess.CalledProcessError, UnicodeError, ValueError) as error:
        print(f"docs staging: FAIL: {error}", file=sys.stderr)
        return 1

    print(f"docs staging: PASS ({count} tracked corpus files, revision {revision}, output {stage})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
