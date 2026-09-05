#!/usr/bin/env python3
"""Fast authority-index check; native and delegated owner verification is separate."""

import argparse
import subprocess
import sys

sys.dont_write_bytecode = True

from model import OUTPUT, require
from observe import ROOT, Repository, observe
from render import render


def run(root, write=False):
    repo = Repository(root)
    index, observed = observe(repo)
    expected = render(index, observed)
    output = repo.path(OUTPUT, output=write)
    if write:
        output.write_bytes(expected)
    else:
        require(output.read_bytes() == expected, "projection: stale bytes; edit owner first, then explicitly --write")
    return expected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    args = parser.parse_args()
    try:
        run(ROOT, write=args.write)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"authority claims: FAIL: {error}", file=sys.stderr)
        return 1
    print("authority claims: PASS (index/projection/corpus only; delegated/native owner verification is separate)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
