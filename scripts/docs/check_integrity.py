#!/usr/bin/env python3
"""Prove that a full docs staging run leaves every docs/ byte unchanged."""

from __future__ import annotations

import argparse
import difflib
import hashlib
import subprocess
import sys
from pathlib import Path


def repository_root() -> Path:
    """Return the repository root derived from this script's tracked path."""

    return Path(__file__).resolve().parents[2]


def corpus_manifest(root: Path) -> tuple[list[str], str]:
    """Hash every regular filesystem entry under docs/ in path order."""

    docs = root / "docs"
    files = sorted((path for path in docs.rglob("*") if path.is_file()), key=lambda path: path.as_posix())
    if not files:
        raise ValueError("docs/ contains no files")

    lines: list[str] = []
    for path in files:
        if path.is_symlink():
            raise ValueError(f"docs/ source must not be a symlink: {path.relative_to(root)}")
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        lines.append(f"{digest}  {path.relative_to(root).as_posix()}")

    manifest_digest = hashlib.sha256(("\n".join(lines) + "\n").encode("utf-8")).hexdigest()
    return lines, manifest_digest


def main() -> int:
    """CLI entry point."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="owned disposable book root")
    parser.add_argument("--revision", help="full Git revision for repository-only links")
    arguments = parser.parse_args()

    root = repository_root()
    try:
        before, before_digest = corpus_manifest(root)
    except (OSError, ValueError) as error:
        print(f"docs byte integrity: FAIL before staging: {error}", file=sys.stderr)
        return 1

    print(f"docs before staging: {before_digest} ({len(before)} files)", flush=True)
    command = [sys.executable, str(root / "scripts" / "docs" / "stage.py"), "--output", str(arguments.output)]
    if arguments.revision:
        command.extend(["--revision", arguments.revision])
    staged = subprocess.run(command, cwd=root, check=False)
    if staged.returncode != 0:
        print(f"docs byte integrity: FAIL: staging exited {staged.returncode}", file=sys.stderr)
        return staged.returncode

    try:
        after, after_digest = corpus_manifest(root)
    except (OSError, ValueError) as error:
        print(f"docs byte integrity: FAIL after staging: {error}", file=sys.stderr)
        return 1

    print(f"docs after staging:  {after_digest} ({len(after)} files)")
    if before != after:
        print("docs byte integrity: FAIL: staging changed docs/", file=sys.stderr)
        for line in difflib.unified_diff(before, after, fromfile="before", tofile="after", lineterm=""):
            print(line, file=sys.stderr)
        return 1

    print("docs byte integrity: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
