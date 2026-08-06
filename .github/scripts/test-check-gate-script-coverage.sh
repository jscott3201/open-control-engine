#!/usr/bin/env bash
# Gate-behavior fixtures for check-gate-script-coverage.sh. A gate that cannot fail is not a gate,
# so the negative fixture below plants the exact divergence the checker exists to catch — a CI
# script the gate script does not invoke — and requires a non-zero exit.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT="$(pwd)/.github/scripts/check-gate-script-coverage.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# A throwaway repo so the fixtures never touch the real tree.
build_repo() {
  root="$1"
  wired="$2" # whether gate.sh invokes the planted script
  rm -rf "$root"
  mkdir -p "$root/.github/scripts" "$root/.agents"
  cd "$root"
  git init -q .
  printf '#!/usr/bin/env bash\necho already-wired\n' > .github/scripts/check-wired.sh
  printf '#!/usr/bin/env bash\necho planted\n' > .github/scripts/check-planted.sh
  {
    printf '#!/usr/bin/env bash\n'
    printf 'bash .github/scripts/check-wired.sh\n'
    [ "$wired" = wired ] && printf 'bash .github/scripts/check-planted.sh\n'
  } > .agents/gate.sh
  chmod +x .github/scripts/*.sh
  cd "$OLDPWD"
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

# NEGATIVE: one script left unwired ⇒ non-zero, and it must NAME the offender. A checker that fails
# without saying which script is missing sends the reader back to a manual diff.
build_repo "$tmp/negative" unwired
if (cd "$tmp/negative" && bash "$SCRIPT" > "$tmp/neg.out" 2>&1); then
  echo "FAIL: the checker passed a repo with an unwired CI script — it cannot fail, so it is not a gate" >&2
  cat "$tmp/neg.out" >&2
  exit 1
fi
grep -q "check-planted.sh" "$tmp/neg.out" || {
  echo "FAIL: the checker refused but did not name the unwired script" >&2
  cat "$tmp/neg.out" >&2
  exit 1
}

echo "OK: gate-script-coverage checker accepts a wired repo and names the offender in an unwired one."
