#!/usr/bin/env bash
# Every script CI runs must also be runnable from `.agents/gate.sh`.
#
# WHY THIS EXISTS. `ci.yml`'s `gate (light)` job is `bash .agents/gate.sh` plus any steps of its
# own. For a while it had one such step — `check-quickstart-runs.sh` — that the gate script did not
# invoke, so a required check gated every pull request while no local run of the script performed
# it. `bash .agents/gate.sh` could pass on a contributor's machine and CI still go red on a command
# they had no way to run. It surfaced only as a CI failure on a PR that had not caused it.
#
# WHY THIS SHAPE. A general gate.sh/ci.yml parity check was attempted and withdrawn; `ci.yml`
# records why. Every design either compared argv strings — which `RUSTFLAGS=--cap-lints=allow`
# leaves byte-identical while neutering clippy — or reimplemented enough of GitHub's `if:`/`needs:`/
# matrix semantics to become its own untested gate. This check does neither. It compares a set of
# FILENAMES, so no flag can defeat it, and it models no workflow semantics at all.
#
# WHAT IT DOES NOT CATCH, stated so nobody reads more into a green run than it earns: an inline
# `run:` step in `ci.yml` that invokes no script at all is invisible here. This catches the
# divergence that actually happened — a script CI runs and the gate script does not — and nothing
# wider.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

GATE=".agents/gate.sh"
SCRIPT_DIR=".github/scripts"

# Scripts deliberately not invoked by the gate, each with the reason it cannot be. An entry here is
# a bypass unless something else covers the script, so keep it empty if at all possible and say
# what covers it if not.
EXCLUDED=()

missing=()
total=0
for path in "$SCRIPT_DIR"/*.sh; do
  name="$(basename "$path")"
  total=$((total + 1))
  skip=""
  for excluded in ${EXCLUDED[@]+"${EXCLUDED[@]}"}; do
    [ "$name" = "$excluded" ] && skip=1
  done
  [ -n "$skip" ] && continue
  grep -q -- "$name" "$GATE" || missing+=("$name")
done

if [ "${#missing[@]}" -ne 0 ]; then
  echo "FAIL: $GATE does not invoke ${#missing[@]} of $total CI script(s):" >&2
  for name in "${missing[@]}"; do
    echo "  · $SCRIPT_DIR/$name" >&2
  done
  echo >&2
  echo "Either wire it into $GATE, or add it to EXCLUDED there with the reason it cannot be." >&2
  exit 1
fi

echo "OK: all $total CI scripts are invoked by $GATE (${#EXCLUDED[@]} excluded)."
