#!/usr/bin/env bash
# Capture the funnel differential fixture's reference outputs from the pinned pyfunnel.
#
#   bash capture/capture.sh <funnel checkout>      # from tests/fixtures/funnel, on x86_64 Linux
#
# <funnel checkout> is lbl-srg/funnel at commit a12fa96917ca73b78fcb6a17a1140b3a27a61e31 (tag
# v2.1.1). The script downloads the one wheel requirements.txt pins by SHA-256, installs it into
# a throwaway virtualenv with no other package, and runs capture.py, which rewrites expected/,
# printf/glibc.tsv and capture/provenance.json. It needs python3 with venv and network access to
# the package index, and nothing else. No CI job runs it: the fixtures are checked in and are
# regenerated only by an issue that says so (docs/design/commissioning.md, decision 13).
#
# x86_64 Linux is required, not preferred: the wheel's linux64 library is built without fused
# multiply-add, and the port matches that arithmetic. The same source built for arm64 contracts
# `a * b + c` and can differ in the last bit.

set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: bash capture/capture.sh <funnel checkout>" >&2
  exit 2
fi
if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "x86_64" ]; then
  echo "capture runs on x86_64 Linux, not $(uname -s) $(uname -m)" >&2
  exit 2
fi

here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
checkout="$(cd "$1" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

python3 -m venv "$work/venv"
"$work/venv/bin/python" -m pip download --quiet --disable-pip-version-check --no-deps \
  --only-binary :all: --require-hashes --dest "$work/wheel" -r "$here/requirements.txt"
wheel="$work/wheel/pyfunnel-2.1.1-py3-none-any.whl"
wheel_sha256="$(sha256sum "$wheel" | cut -d ' ' -f 1)"
"$work/venv/bin/python" -m pip install --quiet --disable-pip-version-check --no-deps --no-index \
  "$wheel"

# -I: isolated mode, so nothing in the fixture or the checkout can shadow a module.
"$work/venv/bin/python" -I "$here/capture.py" "$root" "$checkout" "$wheel_sha256"
