#!/bin/sh
set -eu
export PYTHONDONTWRITEBYTECODE=1

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
FIXTURE="$REPO_ROOT/crates/oce-conformance/tests/fixtures/open_modelica/g36_reliefs"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/oce-reliefs-candidate-assembly.XXXXXX")
trap 'rm -rf "$WORK"' EXIT HUP INT TERM

sh "$SCRIPT_DIR/assemble.sh" "$FIXTURE/arm64" "$FIXTURE/amd64" "$WORK/assembled"
python3 "$SCRIPT_DIR/verify_evidence.py" candidate-final "$WORK/assembled" "$REPO_ROOT"
test -f "$WORK/assembled/generation-contract.json"
printf '%s\n' 'Reliefs Docker-free candidate assembly regression passed'
