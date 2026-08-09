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
# matrix semantics to become its own untested gate. This check does neither. It asks only whether
# each path under `.github/scripts/` appears in `.agents/gate.sh`, so no flag can defeat it, and it
# models no workflow semantics at all.
#
# WHAT IT DOES NOT CATCH, stated so nobody reads more into a green run than it earns. An inline
# `run:` step in `ci.yml` that invokes no script at all is invisible here, as is a script CI runs
# from outside `.github/scripts/`. Appearing in `gate.sh` is not the same as running there: a name
# inside a disabled branch or a dead function would still satisfy this. It catches the divergence
# that actually happened — a script sitting in the CI script directory that the gate script does not
# invoke — and nothing wider.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

GATE=".agents/gate.sh"
SCRIPT_DIR=".github/scripts"

# Scripts deliberately not invoked by the gate, each with the reason it cannot be. An entry here is
# a bypass unless something else covers the script, so keep it empty if at all possible and say
# what covers it if not.
EXCLUDED=()

# Comments are stripped before matching, so a name that survives only in prose does not read as an
# invocation.
gate_code="$(sed 's/#.*//' "$GATE")"

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
  # Match the PATH, not the bare basename. Every check here ships a `test-<name>` fixture beside it,
  # so `check-foo.sh` is a substring of `test-check-foo.sh`: matching the basename let the fixture's
  # line vouch for the real script, and deleting the real invocation kept the guard green. Four of
  # the thirteen scripts sat in that blind spot, this one included. The `$SCRIPT_DIR/` prefix
  # separates the pair, because the fixture is invoked as `.github/scripts/test-check-foo.sh` and
  # never contains `.github/scripts/check-foo.sh`.
  printf '%s\n' "$gate_code" | grep -q -F -- "$SCRIPT_DIR/$name" || missing+=("$name")
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
