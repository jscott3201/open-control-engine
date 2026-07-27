#!/usr/bin/env bash
#
# The gate. This file is the single runnable source of truth for what must pass
# before a change lands; every other document points here rather than restating
# commands. Nine divergent copies of this list existed before it was written,
# two of them materially weaker than CI, so prose copies are the failure mode
# this file exists to remove.
#
#   bash .agents/gate.sh          # light  — mirrors the per-PR gate (ci.yml)
#   bash .agents/gate.sh full     # full   — adds the suite (release-gate.yml)
#
# Commands are kept in CI's exact form, flags included. `--locked`, the `ci`
# nextest profile, `--no-tests=fail`, and `RUSTDOCFLAGS` are each load-bearing:
# dropping any one of them turns a real gate into a green light that proves
# less than it appears to. Change a command here only by changing CI first.
#
# Agents: this script is executable but not writable inside a Codex lane
# sandbox. That is deliberate. A failing gate is fixed at its cause, never by
# editing this file.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

MODE="${1:-light}"
case "$MODE" in
  light | full) ;;
  *)
    echo "usage: bash .agents/gate.sh [light|full]" >&2
    exit 2
    ;;
esac

failures=0
declare -a failed_steps=()

# Run one gate step, echoing the command first. A step never aborts the run —
# a partial gate report is less useful than a complete one, and an agent that
# stops at the first red learns one problem per round trip instead of all of
# them.
step() {
  local label="$1"
  shift
  printf '\n\033[1m── %s\033[0m\n$ %s\n' "$label" "$*"
  if "$@"; then
    printf '\033[32mok\033[0m — %s\n' "$label"
  else
    printf '\033[31mFAIL\033[0m — %s\n' "$label"
    failures=$((failures + 1))
    failed_steps+=("$label")
  fi
}

# Same, for steps needing an environment variable set only for that command.
step_env() {
  local label="$1" var="$2" val="$3"
  shift 3
  printf '\n\033[1m── %s\033[0m\n$ %s=%q %s\n' "$label" "$var" "$val" "$*"
  if env "$var=$val" "$@"; then
    printf '\033[32mok\033[0m — %s\n' "$label"
  else
    printf '\033[31mFAIL\033[0m — %s\n' "$label"
    failures=$((failures + 1))
    failed_steps+=("$label")
  fi
}

printf '\033[1mopen-control gate — mode: %s\033[0m\n' "$MODE"

# ── Formatting, size, secrets ────────────────────────────────────────────────
step 'fmt' cargo fmt --all --check
step 'file-size cap (700 LOC)' bash .github/scripts/check-file-size.sh
step 'no-secret scan' bash .github/scripts/check-no-secrets.sh

# ── Repository invariant gates ───────────────────────────────────────────────
# The default build linking no database and no async runtime is this repo's
# central promise; the golden-gen firewall stops a generator from blessing its
# own output as the oracle.
step 'default build links no database' bash .github/scripts/check-default-no-db.sh
step 'golden-gen anti-tautology firewall' bash .github/scripts/check-golden-gen-anti-tautology.sh

# ── Gate behavior fixtures ───────────────────────────────────────────────────
# The gates are themselves tested. A gate that cannot fail is not a gate, and
# these fixtures are what keep that from happening silently.
step 'no-db gate fixtures' bash .github/scripts/test-check-default-no-db.sh
step 'golden-gen firewall fixtures' bash .github/scripts/test-check-golden-gen-anti-tautology.sh
step 'stale crate-status fixtures' bash .github/scripts/test-check-stale-crate-status.sh
step 'stale crate-status smoke' bash .github/scripts/check-stale-crate-status.sh
step 'workflow gate fixtures' bash .github/scripts/test-workflow-gates.sh
step 'real workflow smoke' bash .github/scripts/check-workflow-gates.sh

# ── Build, lint, docs ────────────────────────────────────────────────────────
# Clippy lints the DEFAULT feature set, matching ci.yml. Do not add
# `--all-features`: it enables a configuration CI never lints and defeats the
# database-free default this repo gates on.
step 'unused dependency gate' cargo machete
step 'clippy (default — no DB)' cargo clippy --workspace --all-targets --locked -- -D warnings
step 'build (default — no DB)' cargo build --workspace --locked
step_env 'rustdoc — libs' RUSTDOCFLAGS '-D warnings' \
  cargo doc --no-deps --workspace --lib --document-private-items --locked
step_env 'rustdoc — bins' RUSTDOCFLAGS '-D warnings' \
  cargo doc --no-deps --workspace --bins --document-private-items --locked

# CI runs this only when a manifest changed. Running it unconditionally is
# stricter than CI, never weaker, and it is fast. `advisories` is deliberately
# excluded — it needs network and a writable advisory-db, neither of which a
# sandboxed lane has; it runs in advisories.yml instead.
step 'cargo-deny (bans licenses sources)' cargo deny check bans licenses sources

# ── Determinism subset ───────────────────────────────────────────────────────
# These two are the ONLY engine tests the per-PR gate runs. Everything else in
# the workspace is untested until the release gate — which is why `full` exists
# and why a green PR is not evidence that the suite passes.
step 'determinism subset' \
  cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
step 'determinism subset (release codegen)' \
  cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci \
  --cargo-profile release --no-tests=fail

# ── Full suite (release-gate.yml) ────────────────────────────────────────────
if [ "$MODE" = full ]; then
  step 'nextest — workspace' \
    cargo nextest run --workspace --locked --profile ci --no-tests=fail
  step 'nextest — workspace (release codegen)' \
    cargo nextest run --workspace --locked --profile ci \
    --cargo-profile release --no-tests=fail
  # nextest cannot run doctests; this step is not optional.
  step 'doctests' cargo test --workspace --doc --locked
fi

# ── Report ───────────────────────────────────────────────────────────────────
cat <<'COVERAGE'

────────────────────────────────────────────────────────────────────────────
NOT COVERED BY THIS SCRIPT — a green run here does not prove these pass:
  · cross-arch determinism matrix (ubuntu-latest AND ubuntu-24.04-arm).
    One machine cannot reproduce it. CI is the only place it runs.
  · cargo public-api surface gates for oce-api / oce-store. They need the
    gate-only nightly toolchain and run in release-gate.yml.
  · cargo deny check advisories. Needs network and a writable advisory-db;
    it runs in advisories.yml (daily) and in release-gate.yml's cargo-deny
    job, which sweeps advisories/bans/licenses/sources on every release PR.
COVERAGE

if [ "$MODE" = light ]; then
  cat <<'LIGHT'
  · the workspace test suite and doctests. This was a `light` run, which
    mirrors the per-PR gate — and the per-PR gate does not run them.
    Run `bash .agents/gate.sh full` before claiming tests pass.
LIGHT
fi

echo '────────────────────────────────────────────────────────────────────────────'

if [ "$failures" -gt 0 ]; then
  printf '\n\033[31mGATE FAILED\033[0m — %d step(s):\n' "$failures"
  printf '  · %s\n' "${failed_steps[@]}"
  echo
  echo 'Fix the cause. Never weaken a gate to reach green.'
  exit 1
fi

printf '\n\033[32mGATE PASSED\033[0m (%s) — all steps green.\n' "$MODE"
