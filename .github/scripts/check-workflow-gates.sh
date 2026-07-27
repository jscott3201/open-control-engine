#!/usr/bin/env bash
# Static smoke checks for CI gate topology. This catches false-green workflow edits before GitHub
# Actions gets a chance to silently skip the heavy test suite or dependency gates.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

WORKFLOW_DIR="${OCE_WORKFLOW_DIR:-.github/workflows}"
DENY_TOML="${OCE_DENY_TOML:-deny.toml}"
ROOT_CARGO_TOML="${OCE_ROOT_CARGO_TOML:-Cargo.toml}"
CRATES_DIR="${OCE_CRATES_DIR:-crates}"

require_file() {
  path="$1"
  if [ ! -s "$path" ]; then
    echo "FAIL: required gate file is missing or empty: $path"
    exit 1
  fi
}

require_pattern() {
  path="$1"
  pattern="$2"
  label="$3"
  if ! grep -Eq "$pattern" "$path"; then
    echo "FAIL: $path is missing required gate pattern: $label"
    exit 1
  fi
}

require_lints_workspace() {
  path="$1"
  if ! awk '
    /^\[lints\]$/ { in_lints = 1; next }
    /^\[/ { in_lints = 0 }
    in_lints && /^[[:space:]]*workspace[[:space:]]*=[[:space:]]*true[[:space:]]*$/ { found = 1 }
    END { exit found ? 0 : 1 }
  ' "$path"; then
    echo "FAIL: $path is missing [lints] workspace = true"
    exit 1
  fi
}

require_unsafe_forbid_layers() {
  local manifest_count=0
  local lib_count=0
  local manifest=""
  local lib=""
  local first_line=""

  require_file "$ROOT_CARGO_TOML"
  if [ ! -d "$CRATES_DIR" ]; then
    echo "FAIL: crates directory is missing: $CRATES_DIR"
    exit 1
  fi

  require_pattern "$ROOT_CARGO_TOML" '^[[:space:]]*unsafe_code[[:space:]]*=[[:space:]]*"forbid"' \
    'workspace.lints.rust unsafe_code = "forbid"'

  for manifest in "$CRATES_DIR"/*/Cargo.toml; do
    [ -e "$manifest" ] || continue
    manifest_count=$((manifest_count + 1))
    require_lints_workspace "$manifest"
  done

  if [ "$manifest_count" -eq 0 ]; then
    echo "FAIL: no crate Cargo.toml files found under $CRATES_DIR"
    exit 1
  fi

  for lib in "$CRATES_DIR"/*/src/lib.rs; do
    [ -e "$lib" ] || continue
    lib_count=$((lib_count + 1))
    first_line="$(sed -n '1p' "$lib")"
    if [ "$first_line" != '#![forbid(unsafe_code)]' ]; then
      echo "FAIL: $lib line 1 must be #![forbid(unsafe_code)]"
      exit 1
    fi
  done

  if [ "$lib_count" -eq 0 ]; then
    echo "FAIL: no crate src/lib.rs files found under $CRATES_DIR"
    exit 1
  fi
}

ci="$WORKFLOW_DIR/ci.yml"
release="$WORKFLOW_DIR/release-gate.yml"
advisories="$WORKFLOW_DIR/advisories.yml"

require_file "$ci"
require_file "$release"
require_file "$advisories"
require_file "$DENY_TOML"
require_unsafe_forbid_layers

# Light per-PR dependency drift gate.
require_pattern "$ci" 'cargo-machete' 'install cargo-machete'
require_pattern "$ci" 'cargo machete' 'run cargo machete'
require_pattern "$ci" 'check-default-no-db\.sh' 'run default-no-db smoke'
require_pattern "$ci" 'test-check-default-no-db\.sh' 'run default-no-db fixture tests'
require_pattern "$ci" 'test-check-golden-gen-anti-tautology\.sh' \
  'run golden-gen firewall fixture tests'
require_pattern "$ci" 'check-golden-gen-anti-tautology\.sh' 'run golden-gen anti-tautology firewall'
require_pattern "$ci" 'test-check-stale-crate-status\.sh' 'run stale crate-status fixture tests'
require_pattern "$ci" 'check-stale-crate-status\.sh' 'run stale crate-status smoke'
require_pattern "$ci" 'check-workflow-gates\.sh' 'run workflow gate smoke'
require_pattern "$ci" 'determinism-matrix' 'targeted cross-arch determinism matrix job'
require_pattern "$ci" 'ubuntu-24\.04-arm' 'arm64 determinism matrix runner'
require_pattern "$ci" 'cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail' \
  'debug determinism subset with hard-fail-on-zero-tests'
require_pattern "$ci" 'cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --cargo-profile release --no-tests=fail' \
  'release determinism subset with hard-fail-on-zero-tests'

# CI's own lint/build/doc commands, in exact form. Without these the parity story runs one way
# only: `gate.sh` was pinned while `ci.yml` was free to weaken underneath it. A review flipped
# ci.yml's clippy to `-A warnings`, left gate.sh untouched, and this script still said OK.
require_pattern "$ci" 'cargo fmt --all --check' 'fmt in exact form'
require_pattern "$ci" 'cargo clippy --workspace --all-targets --locked -- -D warnings' \
  'clippy denying warnings over all targets, default features'
require_pattern "$ci" 'cargo build --workspace --locked' 'locked workspace build'
require_pattern "$ci" 'RUSTDOCFLAGS: "-D warnings"' 'rustdoc denies warnings'
require_pattern "$ci" 'cargo doc --no-deps --workspace --lib --document-private-items --locked' \
  'rustdoc over libs including private items'
require_pattern "$ci" 'cargo doc --no-deps --workspace --bins --document-private-items --locked' \
  'rustdoc over bins including private items'
require_pattern "$ci" 'check-file-size\.sh' 'file-size cap'
require_pattern "$ci" 'check-no-secrets\.sh' 'no-secret scan'
# Every job is draft-gated, so a draft PR runs nothing. Contributors are told to open non-draft;
# pin the condition that makes that instruction matter.
require_pattern "$ci" 'pull_request\.draft == false' 'jobs gated on non-draft pull requests'

# Heavy gate runs on release PRs, manual dispatch, and scheduled development-tip checks.
require_pattern "$release" 'schedule:' 'scheduled heavy gate'
require_pattern "$release" 'cron:' 'cron entry'
require_pattern "$release" 'workflow_dispatch:' 'manual heavy gate'
require_pattern "$release" '(CHECKOUT_REF:.*development|ref:.*development)' 'scheduled checkout targets development tip'
require_pattern "$release" 'ref:.*(CHECKOUT_REF|development)' 'checkout uses the scheduled development ref'
require_pattern "$release" 'cargo nextest run --workspace --locked --profile ci --no-tests=fail' \
  'debug nextest with hard-fail-on-zero-tests'
require_pattern "$release" 'cargo nextest run --workspace --locked --profile ci --cargo-profile release --no-tests=fail' \
  'release-codegen nextest with hard-fail-on-zero-tests'
require_pattern "$release" 'cargo test --workspace --doc --locked' 'doctest gate'
require_pattern "$release" 'OCE_REQUIRE_SURFACE_CHECK:[[:space:]]*"1"' 'armed public-api surface gate'
require_pattern "$release" 'cargo public-api surface gate \(oce-store\)' \
  'dedicated oce-store public-api surface gate step'
require_pattern "$release" 'cargo nextest run -p oce-store' \
  'oce-store public-api surface gate package selector'
require_pattern "$release" 'cargo-machete' 'release gate installs cargo-machete'
require_pattern "$release" 'cargo machete' 'release gate runs cargo machete'
require_pattern "$release" 'check-default-no-db\.sh' 'release gate runs default-no-db smoke'
require_pattern "$release" 'test-check-default-no-db\.sh' 'release gate runs default-no-db fixture tests'
require_pattern "$release" 'test-check-golden-gen-anti-tautology\.sh' \
  'release gate runs golden-gen firewall fixture tests'
require_pattern "$release" 'check-golden-gen-anti-tautology\.sh' \
  'release gate runs golden-gen anti-tautology firewall'
require_pattern "$release" 'test-check-stale-crate-status\.sh' \
  'release gate runs stale crate-status fixture tests'
require_pattern "$release" 'check-stale-crate-status\.sh' 'release gate runs stale crate-status smoke'

# Daily advisory/yanked gate and deny.toml discipline.
require_pattern "$advisories" 'schedule:' 'scheduled advisory gate'
require_pattern "$advisories" 'workflow_dispatch:' 'manual advisory gate'
require_pattern "$advisories" 'cargo deny check advisories' 'advisory-only cargo-deny command'
require_pattern "$DENY_TOML" '^[[:space:]]*yanked[[:space:]]*=[[:space:]]*"deny"' 'yanked = "deny"'
require_pattern "$DENY_TOML" '^[[:space:]]*ignore[[:space:]]*=[[:space:]]*\[\][[:space:]]*$' 'empty advisory ignore list'
# Non-empty sentinel only: this proves representative sqlx/sled bans remain, not that the curated
# family list is complete. A list gutted to only these two entries would still pass by design.
require_pattern "$DENY_TOML" '^[[:space:]]*\{[[:space:]]*name[[:space:]]*=[[:space:]]*"sqlx"[[:space:]]*\}' \
  'cargo-deny bans include representative SQL/ORM crate sqlx'
require_pattern "$DENY_TOML" '^[[:space:]]*\{[[:space:]]*name[[:space:]]*=[[:space:]]*"sled"[[:space:]]*\}' \
  'cargo-deny bans include representative embedded-KV crate sled'

# The local gate script must not drift from CI. `.agents/gate.sh` claims to be the single source
# of truth for gate commands, and nothing enforced that: every assertion above pins ci.yml and
# release-gate.yml only, so the script could quietly diverge back into the nine incompatible
# copies it was written to replace. A contributor trusting a green local run is exactly who that
# drift hurts.
#
# The pin is EXECUTION-DERIVED, not text-derived. An earlier version of this block grepped the
# flattened file, and a review defeated it in one move: put the required command in a comment,
# strip `--locked` from the real one, and the check passed. Moving every pinned command into an
# unexecuted heredoc passed too, while the weakened script printed GATE PASSED having run no
# tests at all. Grepping a script proves a string is present, never that it runs.
#
# `gate.sh list` routes every command through the same `step`/`step_env` that executes it and
# prints the argv it would run, one `CMD ` line each, for light and full together. A comment
# produces no line; neither does a heredoc. The shell also expands the line-continuations for
# us, so no flattening is needed here at all.
GATE_SCRIPT="${OCE_GATE_SCRIPT:-.agents/gate.sh}"
require_file "$GATE_SCRIPT"

gate_cmds="$(mktemp)"
gate_stderr="$(mktemp)"
gate_shim="$(mktemp -d)"
gate_exec_log="$(mktemp)"
trap 'rm -f "$gate_cmds" "$gate_stderr" "$gate_exec_log"; rm -rf "$gate_shim"' EXIT
if ! bash "$GATE_SCRIPT" list > "$gate_cmds" 2>"$gate_stderr"; then
  echo "FAIL: $GATE_SCRIPT does not support 'list' mode, so its commands cannot be pinned"
  # Surface the interpreter's own diagnosis. Swallowing it reported a syntactically broken
  # gate as "no list mode", which fails closed but sends the reader hunting the wrong bug.
  if [ -s "$gate_stderr" ]; then
    echo "       the script reported:"
    sed 's/^/       | /' "$gate_stderr" >&2
  fi
  exit 1
fi
# A listing that emits nothing — or emits prose instead of CMD lines — would make every
# assertion below vacuously unsatisfiable rather than silently satisfied, but say so plainly.
if ! grep -q '^CMD ' "$gate_cmds"; then
  echo "FAIL: $GATE_SCRIPT list produced no CMD lines; the gate command set cannot be verified"
  exit 1
fi
if grep -qv '^CMD ' "$gate_cmds"; then
  echo "FAIL: $GATE_SCRIPT list emitted non-CMD output; listing must be machine-readable only"
  exit 1
fi

# The listing is produced by a `list` branch inside `step`/`step_env`, so on its own it is the
# script's own account of what it would do — self-attestation, not evidence. A review proved the
# gap both ways: early-returning from the NORMAL branch left the listing intact while `gate.sh
# light` executed nothing and printed GATE PASSED, and a command added to the normal branch alone
# ran without ever appearing in the listing.
#
# So run the gate for real against a PATH shim that records argv and executes nothing, and require
# the recorded set to equal the listed set. `cargo` and `bash` are the only external programs the
# steps invoke; `step_env` reaches `cargo` through `env`, so the shim reads RUSTDOCFLAGS back out
# of its own environment to reproduce the listed form. The gate script's own interpreter is given
# as an absolute path so shimming `bash` cannot displace it.
#
# WHAT THIS DOES NOT PROVE, stated plainly because three successive versions of this block
# overclaimed and were broken by the next review:
#
#   * A script that DETECTS the trace can behave one way while observed and another when run
#     normally. `OCE_EXEC_LOG` is visible to it. No tracing scheme fixes this — being observed is
#     detectable in principle — so this check is a guard against DRIFT (a command edited, dropped,
#     or added without CI changing), not a defence against a gate script written to deceive it.
#     A gate that deliberately lies is sabotage, and the review that would catch it is a human
#     reading the diff, not a script.
#   * Equality of the executed SET says nothing about order, count, or the arguments a step
#     computes at runtime.
#
# Both traces below — all-success and all-failure — are about drift. Claiming more than that is
# how this block got broken twice.
cat > "$gate_shim/cargo" <<'SHIM'
#!/bin/sh
if [ -n "${RUSTDOCFLAGS:-}" ]; then
  printf 'CMD RUSTDOCFLAGS=%s cargo %s\n' "$RUSTDOCFLAGS" "$*" >> "$OCE_EXEC_LOG"
else
  printf 'CMD cargo %s\n' "$*" >> "$OCE_EXEC_LOG"
fi
exit 0
SHIM
cat > "$gate_shim/bash" <<'SHIM'
#!/bin/sh
printf 'CMD bash %s\n' "$*" >> "$OCE_EXEC_LOG"
exit 0
SHIM
chmod +x "$gate_shim/cargo" "$gate_shim/bash"

if ! OCE_EXEC_LOG="$gate_exec_log" PATH="$gate_shim:$PATH" \
     /bin/bash "$GATE_SCRIPT" full >/dev/null 2>&1; then
  echo "FAIL: $GATE_SCRIPT full did not complete against a no-op shim; its commands cannot be traced"
  exit 1
fi

listed_only="$(comm -23 <(sort -u "$gate_cmds") <(sort -u "$gate_exec_log"))"
if [ -n "$listed_only" ]; then
  echo "FAIL: $GATE_SCRIPT lists commands it does not execute — the listing is not evidence:"
  printf '%s\n' "$listed_only" | sed 's/^/       /'
  exit 1
fi
executed_only="$(comm -13 <(sort -u "$gate_cmds") <(sort -u "$gate_exec_log"))"
if [ -n "$executed_only" ]; then
  echo "FAIL: $GATE_SCRIPT executes commands it does not list — the pin cannot see them:"
  printf '%s\n' "$executed_only" | sed 's/^/       /'
  exit 1
fi

# Second trace, every command FAILING. The trace above makes everything succeed, so it can only
# ever prove the happy path: a review added `return 1` to `step`'s failure branch and this check
# stayed green, while the real gate would abort after the first red step instead of accumulating.
# That contract is load-bearing — a partial report teaches one problem per round trip instead of
# all of them — so it gets its own trace rather than being assumed.
gate_fail_shim="$(mktemp -d)"
gate_fail_log="$(mktemp)"
gate_fail_out="$(mktemp)"
trap 'rm -f "$gate_cmds" "$gate_stderr" "$gate_exec_log" "$gate_fail_log" "$gate_fail_out"; rm -rf "$gate_shim" "$gate_fail_shim"' EXIT
sed 's/^exit 0$/exit 1/' "$gate_shim/cargo" > "$gate_fail_shim/cargo"
sed 's/^exit 0$/exit 1/' "$gate_shim/bash" > "$gate_fail_shim/bash"
chmod +x "$gate_fail_shim/cargo" "$gate_fail_shim/bash"

if OCE_EXEC_LOG="$gate_fail_log" PATH="$gate_fail_shim:$PATH" \
   /bin/bash "$GATE_SCRIPT" full > "$gate_fail_out" 2>&1; then
  echo "FAIL: $GATE_SCRIPT full reported success while every one of its commands failed"
  exit 1
fi
missed_after_failure="$(comm -23 <(sort -u "$gate_cmds") <(sort -u "$gate_fail_log"))"
if [ -n "$missed_after_failure" ]; then
  echo "FAIL: $GATE_SCRIPT stops after a failing step instead of accumulating; never reached:"
  printf '%s\n' "$missed_after_failure" | sed 's/^/       /'
  exit 1
fi
if ! grep -q 'GATE FAILED' "$gate_fail_out"; then
  echo "FAIL: $GATE_SCRIPT ran with every command failing but printed no GATE FAILED report"
  exit 1
fi

require_gate_cmd() {
  cmd="$1"
  if ! grep -Fxq "CMD $cmd" "$gate_cmds"; then
    echo "FAIL: $GATE_SCRIPT does not run: $cmd"
    exit 1
  fi
}

# Checked before the exact-form pins: any edit to the clippy line breaks its pin too, and
# whichever fires first decides the message. "You used --all-features" is a diagnosis;
# "missing required pattern" is a puzzle.
if grep -Eq '^CMD cargo clippy .*--all-features' "$gate_cmds"; then
  echo "FAIL: $GATE_SCRIPT lints --all-features; CI lints the default feature set (no-DB promise)"
  exit 1
fi

# Every command GATE.SH runs, in CI's exact form. This half of the parity check inspects the gate
# script only; the CI side is pinned separately above. Saying "every command ci.yml runs" here was
# wrong and hid a real gap for a round — pinning only the test commands left the rest free to
# drift, and a review removed the no-secret step, dropped build's --locked, flipped rustdoc to
# -A warnings, deleted cargo machete, and cut cargo-deny's `sources`, all green.
require_gate_cmd 'cargo fmt --all --check'
require_gate_cmd 'bash .github/scripts/check-file-size.sh'
require_gate_cmd 'bash .github/scripts/check-no-secrets.sh'
require_gate_cmd 'bash .github/scripts/check-default-no-db.sh'
require_gate_cmd 'bash .github/scripts/check-golden-gen-anti-tautology.sh'
require_gate_cmd 'bash .github/scripts/test-check-default-no-db.sh'
require_gate_cmd 'bash .github/scripts/test-check-golden-gen-anti-tautology.sh'
require_gate_cmd 'bash .github/scripts/test-check-stale-crate-status.sh'
require_gate_cmd 'bash .github/scripts/check-stale-crate-status.sh'
# The gate's own fixture suite and workflow smoke. Unpinned in the first version of this block,
# which meant the local gate could silently stop testing its own gates.
require_gate_cmd 'bash .github/scripts/test-workflow-gates.sh'
require_gate_cmd 'bash .github/scripts/check-workflow-gates.sh'
require_gate_cmd 'cargo machete'
require_gate_cmd 'cargo clippy --workspace --all-targets --locked -- -D warnings'
require_gate_cmd 'cargo build --workspace --locked'
require_gate_cmd 'RUSTDOCFLAGS=-D warnings cargo doc --no-deps --workspace --lib --document-private-items --locked'
require_gate_cmd 'RUSTDOCFLAGS=-D warnings cargo doc --no-deps --workspace --bins --document-private-items --locked'
require_gate_cmd 'cargo deny check bans licenses sources'
require_gate_cmd 'cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail'
require_gate_cmd 'cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --cargo-profile release --no-tests=fail'

# Full mode adds the release gate's own suite.
require_gate_cmd 'cargo nextest run --workspace --locked --profile ci --no-tests=fail'
require_gate_cmd 'cargo nextest run --workspace --locked --profile ci --cargo-profile release --no-tests=fail'
require_gate_cmd 'cargo test --workspace --doc --locked'

echo "OK: workflow gate smoke passed."
