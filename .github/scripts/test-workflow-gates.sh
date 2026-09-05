#!/usr/bin/env bash
# Gate-behavior fixtures for check-workflow-gates.sh. The negative fixtures prove missing nextest,
# seeded advisory ignores, and garbled workflow input fail closed.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT=".github/scripts/check-workflow-gates.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

write_positive() {
  dir="$1"
  deny="$2"
  root_cargo="$3"
  crates_dir="$4"
  mkdir -p "$dir"
  cat > "$dir/ci.yml" <<'EOF'
jobs:
  default-no-db:
    steps:
      - run: bash .github/scripts/check-default-no-db.sh
  unused-deps:
    steps:
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-machete
      - run: cargo machete
  gate-fixtures:
    steps:
      - run: bash .github/scripts/test-check-default-no-db.sh
      - run: bash .github/scripts/test-check-golden-gen-anti-tautology.sh
      - run: bash .github/scripts/test-check-stale-crate-status.sh
      - run: bash .github/scripts/check-stale-crate-status.sh
      - run: bash .github/scripts/check-workflow-gates.sh
  golden-gen-firewall:
    steps:
      - run: bash .github/scripts/check-golden-gen-anti-tautology.sh
  package-publication-contract:
    steps:
      - run: python3 scripts/package_policy/test_validate.py
      - run: python3 scripts/package_policy/validate.py
      - name: authority claim consistency
        run: python3 scripts/authority_claims/check.py --check
      - name: authority claim hostile controls
        run: python3 scripts/authority_claims/test_check.py
  determinism-matrix:
    strategy:
      matrix:
        runner: [ubuntu-latest, ubuntu-24.04-arm]
    runs-on: ${{ matrix.runner }}
    steps:
      - uses: taiki-e/install-action@v2
        with:
          tool: nextest@0.9.143
      - name: Clear cached reports and state vectors
        run: >-
          rm -f target/nextest/{ci,ci-release}/junit.xml
          target/{portable,target}-state-{debug,release}.bin
      - run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
        env:
          OCE_PORTABLE_STATE_OUT: target/portable-state-debug.bin
          OCE_TARGET_STATE_OUT: target/target-state-debug.bin
      - run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci-release --cargo-profile release --no-tests=fail
        env:
          OCE_PORTABLE_STATE_OUT: target/portable-state-release.bin
          OCE_TARGET_STATE_OUT: target/target-state-release.bin
      - run: >-
          cmp target/portable-state-debug.bin target/portable-state-release.bin &&
          cmp target/target-state-debug.bin target/target-state-release.bin
      - run: >-
          for profile in ci ci-release;
          do test -s "target/nextest/$profile/junit.xml";
          done
      - uses: actions/upload-artifact@v7.0.1
        if: ${{ !cancelled() }}
        with:
          path: |
            target/nextest/ci/junit.xml
            target/nextest/ci-release/junit.xml
          if-no-files-found: error
          retention-days: 14
      - uses: actions/upload-artifact@v7.0.1
        with:
          name: portable-state-${{ matrix.runner }}
          path: |
            target/portable-state-debug.bin
            target/target-state-debug.bin
          if-no-files-found: error
          retention-days: 14
  portable-state-cross-arch:
    needs: determinism-matrix
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.97.1
      - uses: taiki-e/install-action@v2
        with:
          tool: nextest@0.9.143
      - uses: actions/download-artifact@v7.0.0
        with:
          name: portable-state-ubuntu-latest
          path: target/state-x86
      - uses: actions/download-artifact@v7.0.0
        with:
          name: portable-state-ubuntu-24.04-arm
          path: target/state-arm
      - run: >-
          cmp target/state-x86/portable-state-debug.bin target/state-arm/portable-state-debug.bin &&
          ! cmp -s target/state-x86/target-state-debug.bin target/state-arm/target-state-debug.bin
      - env:
          OCE_FOREIGN_TARGET_STATE_IN: target/state-arm/target-state-debug.bin
        run: >-
          cargo nextest run -p oce-api --lib --locked --profile ci --no-tests=fail
          -E 'test(=tests::state_portability_tests::foreign_matrix_target_snapshot_refuses_restore_when_supplied)'
EOF
  cat > "$dir/release-gate.yml" <<'EOF'
on:
  schedule:
    - cron: "17 8 * * *"
  workflow_dispatch:
jobs:
  test-suite:
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event_name == 'schedule' && 'development' || github.ref }}
      - uses: taiki-e/install-action@v2
        with:
          tool: nextest@0.9.143
      - name: Clear cached nextest reports
        run: rm -f target/nextest/{ci,ci-release,public-api-oce-api,public-api-oce-store}/junit.xml
      - run: cargo nextest run --workspace --locked --profile ci --no-tests=fail
      - run: cargo nextest run --workspace --locked --profile ci-release --cargo-profile release --no-tests=fail
      - run: cargo test --workspace --doc --locked
      - env:
          OCE_REQUIRE_SURFACE_CHECK: "1"
        run: cargo nextest run -p oce-api -E 'test(public_api_surface_matches_blessed_baseline)' --profile public-api-oce-api --locked --no-tests=fail
      - name: cargo public-api surface gate (oce-store)
        env:
          OCE_REQUIRE_SURFACE_CHECK: "1"
        run: cargo nextest run -p oce-store -E 'test(public_api_surface_matches_blessed_baseline)' --profile public-api-oce-store --locked --no-tests=fail
      - run: >-
          for profile in ci ci-release public-api-oce-api public-api-oce-store;
          do test -s "target/nextest/$profile/junit.xml";
          done
      - uses: actions/upload-artifact@v7.0.1
        if: ${{ !cancelled() }}
        with:
          path: |
            target/nextest/ci/junit.xml
            target/nextest/ci-release/junit.xml
            target/nextest/public-api-oce-api/junit.xml
            target/nextest/public-api-oce-store/junit.xml
          if-no-files-found: error
          retention-days: 14
  default-no-db:
    steps:
      - run: bash .github/scripts/check-default-no-db.sh
  unused-deps:
    steps:
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-machete
      - run: cargo machete
  gate-fixtures:
    steps:
      - run: bash .github/scripts/test-check-default-no-db.sh
      - run: bash .github/scripts/test-check-golden-gen-anti-tautology.sh
      - run: bash .github/scripts/test-check-stale-crate-status.sh
      - run: bash .github/scripts/check-stale-crate-status.sh
  golden-gen-firewall:
    steps:
      - run: bash .github/scripts/check-golden-gen-anti-tautology.sh
EOF
  cat > "$dir/advisories.yml" <<'EOF'
on:
  schedule:
    - cron: "43 8 * * *"
  workflow_dispatch:
jobs:
  advisories:
    steps:
      - run: cargo deny check advisories
EOF
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = []

[bans]
deny = [
  { name = "sqlx" },
  { name = "sled" },
]
EOF
  cat > "$root_cargo" <<'EOF'
[workspace.lints.rust]
unsafe_code = "forbid"
EOF
  mkdir -p "$crates_dir/oce-api/src" "$crates_dir/oce-store/src"
  cat > "$crates_dir/oce-api/Cargo.toml" <<'EOF'
[package]
name = "oce-api"

[lints]
workspace = true
EOF
  cat > "$crates_dir/oce-store/Cargo.toml" <<'EOF'
[package]
name = "oce-store"

[lints]
workspace = true
EOF
  cat > "$crates_dir/oce-api/src/lib.rs" <<'EOF'
#![forbid(unsafe_code)]
//! Fixture crate.
EOF
  cat > "$crates_dir/oce-store/src/lib.rs" <<'EOF'
#![forbid(unsafe_code)]
//! Fixture crate.
EOF
}

run_case() {
  name="$1"
  expected="$2"
  mutate="$3"
  expected_message="${4:-}"
  dir="$tmp/$name/workflows"
  deny="$tmp/$name/deny.toml"
  root_cargo="$tmp/$name/Cargo.toml"
  crates_dir="$tmp/$name/crates"
  mkdir -p "$tmp/$name"
  write_positive "$dir" "$deny" "$root_cargo" "$crates_dir"
  "$mutate" "$dir" "$deny" "$root_cargo" "$crates_dir"

  set +e
  output="$(
    OCE_WORKFLOW_DIR="$dir" \
    OCE_DENY_TOML="$deny" \
    OCE_ROOT_CARGO_TOML="$root_cargo" \
    OCE_CRATES_DIR="$crates_dir" \
    bash "$SCRIPT" 2>&1
  )"
  status=$?
  set -e

  case "$expected" in
    pass)
      if [ "$status" -ne 0 ]; then
        echo "FAIL: workflow fixture '$name' should pass but exited $status"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^OK:'; then
        echo "FAIL: workflow fixture '$name' passed without the OK line"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    fail)
      if [ "$status" -eq 0 ]; then
        echo "FAIL: workflow fixture '$name' should fail but exited 0"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^FAIL:'; then
        echo "FAIL: workflow fixture '$name' failed without a FAIL line"
        printf '%s\n' "$output"
        exit 1
      fi
      if [ -n "$expected_message" ] && ! printf '%s\n' "$output" | grep -Fq -- "$expected_message"; then
        echo "FAIL: workflow fixture '$name' failed without expected message substring: $expected_message"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    *)
      echo "BUG: unknown expectation '$expected'"
      exit 1
      ;;
  esac
}

noop() {
  _dir="$1"
  _deny="$2"
}

remove_release_nextest() {
  dir="$1"
  _deny="$2"
  grep -v -- '--cargo-profile release' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

seed_advisory_ignore() {
  _dir="$1"
  deny="$2"
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = ["RUSTSEC-2099-0001"]

[bans]
deny = [
  { name = "sqlx" },
  { name = "sled" },
]
EOF
}

empty_bans_deny() {
  _dir="$1"
  deny="$2"
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = []

[bans]
deny = []
EOF
}

garble_release_workflow() {
  dir="$1"
  _deny="$2"
  printf '%s\n' 'not: [a usable gate]' > "$dir/release-gate.yml"
}

remove_stale_status_gate() {
  dir="$1"
  _deny="$2"
  grep -v 'check-stale-crate-status' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_store_surface_gate() {
  dir="$1"
  _deny="$2"
  grep -v -- '-p oce-store' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

remove_golden_gen_firewall() {
  dir="$1"
  _deny="$2"
  grep -v 'check-golden-gen-anti-tautology' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_package_publication_contract() {
  dir="$1"
  _deny="$2"
  grep -v 'scripts/package_policy' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

comment_authority_claim_check() {
  dir="$1"
  sed 's@run: python3 scripts/authority_claims/check.py --check@run: true # python3 scripts/authority_claims/check.py --check@' \
    "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_authority_claim_controls() {
  dir="$1"
  grep -v 'scripts/authority_claims/test_check.py' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_root_unsafe_forbid() {
  _dir="$1"
  _deny="$2"
  root_cargo="$3"
  cat > "$root_cargo" <<'EOF'
[workspace.lints.rust]
unsafe_code = "allow"
EOF
}

remove_crate_lints_workspace() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  cat > "$crates_dir/oce-api/Cargo.toml" <<'EOF'
[package]
name = "oce-api"
EOF
}

remove_crate_lib_forbid() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  cat > "$crates_dir/oce-api/src/lib.rs" <<'EOF'
//! Fixture crate.
EOF
}

remove_no_db_gate() {
  dir="$1"
  _deny="$2"
  grep -v 'check-default-no-db' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_determinism_matrix() {
  dir="$1"
  _deny="$2"
  grep -v 'oce-api -p oce-blocks -p oce-expr' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

comment_foreign_restore_command() {
  dir="$1"
  _deny="$2"
  grep -v 'foreign_matrix_target_snapshot_refuses_restore_when_supplied' \
    "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

allow_absent_foreign_restore_test() {
  dir="$1"
  _deny="$2"
  sed '/cargo nextest run -p oce-api --lib/ s/ --no-tests=fail//' \
    "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_foreign_restore_nextest_install() {
  dir="$1"
  _deny="$2"
  awk '
    /portable-state-cross-arch:/ { in_job = 1 }
    in_job && /tool: nextest@0.9.143/ { next }
    { print }
  ' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_nextest_junit_artifacts() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/ci.yml" "$dir/release-gate.yml"; do
    grep -v -E 'upload-artifact|^[[:space:]]*target/nextest/(ci|ci-release|public-api-oce-api|public-api-oce-store)/junit\.xml' \
      "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

remove_one_nextest_junit_artifact() {
  dir="$1"
  _deny="$2"
  grep -v '^[[:space:]]*target/nextest/ci-release/junit\.xml' \
    "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_nextest_report_cleanup() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/ci.yml" "$dir/release-gate.yml"; do
    grep -v 'target/nextest/{' "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

remove_nextest_report_requirements() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/ci.yml" "$dir/release-gate.yml"; do
    grep -v -E 'for profile in|test -s.*\$profile|^[[:space:]]*done[[:space:]]*$' \
      "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

downgrade_nextest_pin() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/ci.yml" "$dir/release-gate.yml"; do
    sed 's/nextest@0\.9\.143/nextest@0.9.142/g' "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

empty_crate_dir_only() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -rf "$crates_dir"
  mkdir -p "$crates_dir/oce-empty"
}

remove_crate_lib_files() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -f "$crates_dir"/*/src/lib.rs
}

remove_crates_dir() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -rf "$crates_dir"
}

run_case positive pass noop
run_case missing-release-nextest fail remove_release_nextest \
  "release-codegen nextest with hard-fail-on-zero-tests"
run_case seeded-advisory-ignore fail seed_advisory_ignore \
  "empty advisory ignore list"
run_case garbled-workflow fail garble_release_workflow \
  "scheduled heavy gate"
run_case missing-stale-status-gate fail remove_stale_status_gate \
  "run stale crate-status fixture tests"
run_case empty-bans-deny fail empty_bans_deny \
  "cargo-deny bans include representative SQL/ORM crate sqlx"
run_case missing-store-surface-gate fail remove_store_surface_gate \
  "oce-store public-api surface gate package selector"
run_case missing-golden-gen-firewall fail remove_golden_gen_firewall \
  "run golden-gen firewall fixture tests"
run_case missing-package-publication-contract fail remove_package_publication_contract \
  "run package publication hostile controls"
run_case no-op-authority-claim-check fail comment_authority_claim_check \
  "run authority claim consistency check"
run_case missing-authority-claim-controls fail remove_authority_claim_controls \
  "run authority claim hostile controls"
run_case missing-root-unsafe-forbid fail remove_root_unsafe_forbid \
  "workspace.lints.rust unsafe_code = \"forbid\""
run_case missing-crate-lints-workspace fail remove_crate_lints_workspace \
  "missing [lints] workspace = true"
run_case missing-crate-lib-forbid fail remove_crate_lib_forbid \
  "line 1 must be #![forbid(unsafe_code)]"
run_case missing-no-db-gate fail remove_no_db_gate \
  "run default-no-db smoke"
run_case missing-determinism-matrix fail remove_determinism_matrix \
  "debug determinism subset with hard-fail-on-zero-tests"
run_case commented-foreign-restore fail comment_foreign_restore_command \
  "restore and refuse the foreign target-bound snapshot"
run_case vacuous-foreign-restore fail allow_absent_foreign_restore_test \
  "foreign target-bound restore hard-fails when its test is absent"
run_case missing-foreign-nextest-install fail remove_foreign_restore_nextest_install \
  "pinned cargo-nextest install for foreign restore"
run_case missing-nextest-junit-artifacts fail remove_nextest_junit_artifacts \
  "upload nextest JUnit report artifacts"
run_case partial-nextest-junit-artifacts fail remove_one_nextest_junit_artifact \
  "collect nextest release JUnit report"
run_case missing-nextest-report-cleanup fail remove_nextest_report_cleanup \
  "clear cached nextest JUnit reports"
run_case missing-nextest-report-requirements fail remove_nextest_report_requirements \
  "require the complete nextest determinism report set"
run_case stale-nextest-pin fail downgrade_nextest_pin \
  "pinned cargo-nextest 0.9.143 install"
run_case empty-crate-dir-only fail empty_crate_dir_only \
  "no crate Cargo.toml files found under"
run_case missing-crate-lib-files fail remove_crate_lib_files \
  "no crate src/lib.rs files found under"
run_case missing-crates-dir fail remove_crates_dir \
  "crates directory is missing"

echo "OK: workflow gate fixtures passed."
