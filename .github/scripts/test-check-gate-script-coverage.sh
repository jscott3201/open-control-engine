#!/usr/bin/env bash
# Gate-behavior fixtures for check-gate-script-coverage.sh. A gate that cannot fail is not a gate,
# so each negative fixture below plants a way for a CI script to go uninvoked and requires a
# non-zero exit. Three ways, because the first version of the checker caught only the first: a
# script can be absent from gate.sh, shadowed by a `test-` sibling whose name contains it, or named
# there in a comment that runs nothing.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT="$(pwd)/.github/scripts/check-gate-script-coverage.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# A throwaway repo so the fixtures never touch the real tree.
#
# `mode` decides how check-planted.sh is treated:
#   wired     — gate.sh invokes it. The only mode that should pass.
#   unwired   — gate.sh does not mention it at all.
#   shadowed  — gate.sh invokes a `test-check-planted.sh` sibling but not the script itself.
#   commented — gate.sh names it only inside a comment.
build_repo() {
  root="$1"
  mode="$2"
  rm -rf "$root"
  mkdir -p "$root/.github/scripts" "$root/.agents"
  cd "$root"
  git init -q .
  printf '#!/usr/bin/env bash\necho already-wired\n' > .github/scripts/check-wired.sh
  printf '#!/usr/bin/env bash\necho planted\n' > .github/scripts/check-planted.sh
  [ "$mode" = shadowed ] &&
    printf '#!/usr/bin/env bash\necho fixture\n' > .github/scripts/test-check-planted.sh
  {
    printf '#!/usr/bin/env bash\n'
    printf 'bash .github/scripts/check-wired.sh\n'
    case "$mode" in
      wired) printf 'bash .github/scripts/check-planted.sh\n' ;;
      shadowed) printf 'bash .github/scripts/test-check-planted.sh\n' ;;
      commented) printf '# see .github/scripts/check-planted.sh for the rules it enforces\n' ;;
    esac
  } > .agents/gate.sh
  chmod +x .github/scripts/*.sh
  cd "$OLDPWD"
}

# Every mode that must be REFUSED, run through one body so a future mode cannot be added without an
# assertion. Each names the offender too: a checker that fails without saying which script is
# missing sends the reader back to a manual diff.
expect_refusal() {
  mode="$1"
  why="$2"
  build_repo "$tmp/$mode" "$mode"
  if (cd "$tmp/$mode" && bash "$SCRIPT" > "$tmp/$mode.out" 2>&1); then
    echo "FAIL: the checker passed a repo where $why" >&2
    cat "$tmp/$mode.out" >&2
    exit 1
  fi
  grep -q "check-planted.sh" "$tmp/$mode.out" || {
    echo "FAIL: the checker refused the '$mode' repo but did not name the unwired script" >&2
    cat "$tmp/$mode.out" >&2
    exit 1
  }
}

# POSITIVE: every script wired ⇒ exit 0.
build_repo "$tmp/positive" wired
if ! (cd "$tmp/positive" && bash "$SCRIPT" > "$tmp/pos.out" 2>&1); then
  echo "FAIL: the checker rejected a fully-wired repo" >&2
  cat "$tmp/pos.out" >&2
  exit 1
fi
grep -q "OK: all 2 CI scripts" "$tmp/pos.out" || {
  echo "FAIL: positive fixture did not report the expected count" >&2
  cat "$tmp/pos.out" >&2
  exit 1
}

expect_refusal unwired "a CI script goes uninvoked — it cannot fail, so it is not a gate"

# The shadowing case, which the first version of this checker could not see. It matched the bare
# basename, and `check-planted.sh` is a substring of `test-check-planted.sh`, so the fixture's line
# vouched for a script nothing ran. That is not hypothetical: every real check in this directory
# ships a `test-` sibling, so four of the thirteen were covered by nothing but their own fixtures.
expect_refusal shadowed "only a test- sibling of the script is invoked"

# Appearing in gate.sh is not being run by it.
expect_refusal commented "the script is named only in a comment"

echo "OK: gate-script-coverage checker accepts a wired repo and refuses an unwired, shadowed, or commented one."
