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
  searchable="$(grep -Ev '^[[:space:]]*#' "$path")"
  if ! grep -Eq "$pattern" <<< "$searchable"; then
    echo "FAIL: $path is missing required gate pattern: $label"
    exit 1
  fi
}

require_job_pattern() {
  path="$1"
  job="$2"
  pattern="$3"
  label="$4"
  job_body="$(awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found { print }
  ' "$path")"
  if ! grep -Eq "$pattern" <<< "$job_body"; then
    echo "FAIL: $path job $job is missing required gate pattern: $label"
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
require_pattern "$ci" 'nextest@0\.9\.143' 'pinned cargo-nextest 0.9.143 install'
require_pattern "$ci" 'cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail' \
  'debug determinism subset with hard-fail-on-zero-tests'
require_pattern "$ci" 'cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci-release --cargo-profile release --no-tests=fail' \
  'release determinism subset with hard-fail-on-zero-tests'
require_pattern "$ci" 'OCE_PORTABLE_STATE_OUT: target/portable-state-debug\.bin' \
  'emit the debug portable state vector in every determinism cell'
require_pattern "$ci" 'OCE_PORTABLE_STATE_OUT: target/portable-state-release\.bin' \
  'emit the release portable state vector in every determinism cell'
require_pattern "$ci" 'OCE_TARGET_STATE_OUT: target/target-state-debug\.bin' \
  'emit the debug target-bound state vector in every determinism cell'
require_pattern "$ci" 'OCE_TARGET_STATE_OUT: target/target-state-release\.bin' \
  'emit the release target-bound state vector in every determinism cell'
require_pattern "$ci" 'cmp target/portable-state-debug\.bin target/portable-state-release\.bin' \
  'compare portable state bytes across codegen profiles'
require_pattern "$ci" 'cmp target/target-state-debug\.bin target/target-state-release\.bin' \
  'compare target-bound state bytes across codegen profiles'
require_pattern "$ci" 'portable-state-cross-arch:' 'cross-architecture portable-state comparison job'
require_job_pattern "$ci" 'portable-state-cross-arch' 'tool:[[:space:]]*nextest@0\.9\.143' \
  'pinned cargo-nextest install for foreign restore'
require_pattern "$ci" 'portable-state-ubuntu-latest' 'download the x86_64 portable state vector'
require_pattern "$ci" 'portable-state-ubuntu-24\.04-arm' 'download the arm64 portable state vector'
require_pattern "$ci" 'cmp target/state-x86/portable-state-debug\.bin target/state-arm/portable-state-debug\.bin' \
  'compare portable state bytes across architectures'
require_pattern "$ci" '![[:space:]]*cmp -s target/state-x86/target-state-debug\.bin target/state-arm/target-state-debug\.bin' \
  'require target-bound state bytes to differ across architectures'
require_pattern "$ci" 'OCE_FOREIGN_TARGET_STATE_IN: target/state-arm/target-state-debug\.bin' \
  'supply the arm64 target-bound snapshot to the x86_64 restore test'
require_pattern "$ci" "test\(=tests::state_portability_tests::foreign_matrix_target_snapshot_refuses_restore_when_supplied\).*'" \
  'restore and refuse the foreign target-bound snapshot'
require_pattern "$ci" 'cargo nextest run -p oce-api --lib --locked --profile ci --no-tests=fail' \
  'foreign target-bound restore hard-fails when its test is absent'
require_pattern "$ci" 'target/nextest/\{ci,ci-release\}/junit\.xml' \
  'clear cached nextest JUnit reports'
require_pattern "$ci" 'target/\{portable,target\}-state-\{debug,release\}\.bin' \
  'clear cached state vectors before the determinism matrix'
require_pattern "$ci" 'actions/upload-artifact@v7\.0\.1' 'upload nextest JUnit report artifacts'
require_pattern "$ci" '![[:space:]]*cancelled\(\)' 'upload nextest reports after test failures'
require_pattern "$ci" 'target/nextest/ci/junit\.xml' 'collect nextest debug JUnit report'
require_pattern "$ci" 'target/nextest/ci-release/junit\.xml' 'collect nextest release JUnit report'
require_pattern "$ci" 'for profile in ci ci-release;' \
  'require the complete nextest determinism report set'
require_pattern "$ci" 'test -s "target/nextest/\$profile/junit\.xml"' \
  'refuse an absent or empty nextest determinism report'
require_pattern "$ci" 'if-no-files-found:[[:space:]]*error' 'fail when nextest reports are absent'
require_pattern "$ci" 'retention-days:[[:space:]]*14' 'retain nextest reports for 14 days'

# Heavy gate runs on release PRs, manual dispatch, and scheduled development-tip checks.
require_pattern "$release" 'schedule:' 'scheduled heavy gate'
require_pattern "$release" 'cron:' 'cron entry'
require_pattern "$release" 'workflow_dispatch:' 'manual heavy gate'
require_pattern "$release" '(CHECKOUT_REF:.*development|ref:.*development)' 'scheduled checkout targets development tip'
require_pattern "$release" 'ref:.*(CHECKOUT_REF|development)' 'checkout uses the scheduled development ref'
require_pattern "$release" 'nextest@0\.9\.143' 'release gate pinned cargo-nextest 0.9.143 install'
require_pattern "$release" 'cargo nextest run --workspace --locked --profile ci --no-tests=fail' \
  'debug nextest with hard-fail-on-zero-tests'
require_pattern "$release" 'cargo nextest run --workspace --locked --profile ci-release --cargo-profile release --no-tests=fail' \
  'release-codegen nextest with hard-fail-on-zero-tests'
require_pattern "$release" 'target/nextest/\{ci,ci-release,public-api-oce-api,public-api-oce-store\}/junit\.xml' \
  'release gate clears cached nextest JUnit reports'
require_pattern "$release" 'actions/upload-artifact@v7\.0\.1' \
  'release gate uploads nextest JUnit reports'
require_pattern "$release" '![[:space:]]*cancelled\(\)' \
  'release gate uploads nextest reports after test failures'
require_pattern "$release" 'target/nextest/ci/junit\.xml' \
  'release gate collects debug nextest JUnit report'
require_pattern "$release" 'target/nextest/ci-release/junit\.xml' \
  'release gate collects release-codegen nextest JUnit report'
require_pattern "$release" 'target/nextest/public-api-oce-api/junit\.xml' \
  'release gate collects oce-api surface nextest JUnit report'
require_pattern "$release" 'target/nextest/public-api-oce-store/junit\.xml' \
  'release gate collects oce-store surface nextest JUnit report'
require_pattern "$release" 'for profile in ci ci-release public-api-oce-api public-api-oce-store;' \
  'release gate requires the complete nextest report set'
require_pattern "$release" 'test -s "target/nextest/\$profile/junit\.xml"' \
  'release gate refuses an absent or empty nextest report'
require_pattern "$release" 'if-no-files-found:[[:space:]]*error' \
  'release gate fails when nextest reports are absent'
require_pattern "$release" 'retention-days:[[:space:]]*14' \
  'release gate retains nextest reports for 14 days'
require_pattern "$release" 'cargo test --workspace --doc --locked' 'doctest gate'
require_pattern "$release" 'OCE_REQUIRE_SURFACE_CHECK:[[:space:]]*"1"' 'armed public-api surface gate'
require_pattern "$release" 'cargo public-api surface gate \(oce-store\)' \
  'dedicated oce-store public-api surface gate step'
require_pattern "$release" 'cargo nextest run -p oce-store' \
  'oce-store public-api surface gate package selector'
require_pattern "$release" 'profile public-api-oce-api' 'oce-api surface report profile'
require_pattern "$release" 'profile public-api-oce-store' 'oce-store surface report profile'
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

echo "OK: workflow gate smoke passed."
