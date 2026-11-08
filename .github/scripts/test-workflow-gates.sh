#!/usr/bin/env bash
# Gate-behavior fixtures for check-workflow-gates.sh. The negative fixtures prove missing nextest,
# seeded advisory ignores, garbled workflow input, actions referenced by a movable tag, a per-PR
# gate that re-runs on pushes to development, and a cell matrix that is no longer native on both
# architectures (or loses a cell, a fan-in or a comparison) fail closed.

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
  cat > "$dir/pr-gate.yml" <<'EOF'
on:
  pull_request:
    branches: [development]
  workflow_dispatch:
jobs:
  default-no-db:
    steps:
      - run: bash .github/scripts/check-default-no-db.sh
  unused-deps:
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
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
  strict-bit-matrix:
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        runner: [ubuntu-latest, ubuntu-24.04-arm]
        codegen: [debug, release]
        include:
          - runner: ubuntu-latest
            arch: x86_64
          - runner: ubuntu-24.04-arm
            arch: aarch64
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - env:
          CELL_ARCH: ${{ matrix.arch }}
          CELL_CODEGEN: ${{ matrix.codegen }}
        run: bash scripts/ci/strict-bits-cell.sh "$CELL_ARCH" "$CELL_CODEGEN"
      - uses: actions/upload-artifact@0123456789abcdef0123456789abcdef01234567 # v7
        with:
          name: strict-bits-${{ matrix.arch }}-${{ matrix.codegen }}
          path: target/strict-bits/
          if-no-files-found: error
  strict-bit-compare:
    needs: strict-bit-matrix
    steps:
      - env:
          OCE_STRICT_MATRIX_DIR: target/strict-bits
          OCE_STRICT_RUN_ID: ${{ github.run_id }}
        run: >-
          cargo nextest run --archive-file target/strict-bits/strict-bits-tests.tar.zst
          --workspace-remap . --profile ci --no-tests=fail --run-ignored only
          -E 'test(=matrix::native_linux_matrix_is_complete_and_exact)'
      - run: test -s target/strict-bits/receipt-candidate.json
  determinism-matrix:
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        runner: [ubuntu-latest, ubuntu-24.04-arm]
        codegen: [debug, release]
        include:
          - runner: ubuntu-latest
            arch: x86_64
          - runner: ubuntu-24.04-arm
            arch: aarch64
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - env:
          CELL_ARCH: ${{ matrix.arch }}
        run: test "$(uname -m)" = "$CELL_ARCH"
      - name: Clear cached reports and state vectors
        run: >-
          rm -rf target/nextest/{ci,ci-release}/junit.xml
          target/{portable,target}-state-{debug,release}.bin
      - if: matrix.codegen == 'debug'
        run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
        env:
          OCE_PORTABLE_STATE_OUT: target/portable-state-debug.bin
          OCE_TARGET_STATE_OUT: target/target-state-debug.bin
      - if: matrix.codegen == 'release'
        run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci-release --cargo-profile release --no-tests=fail
        env:
          OCE_PORTABLE_STATE_OUT: target/portable-state-release.bin
          OCE_TARGET_STATE_OUT: target/target-state-release.bin
      - run: |
          test -s "target/nextest/$profile/junit.xml"
          test -s "target/portable-state-$CELL_CODEGEN.bin"
      - uses: actions/upload-artifact@0123456789abcdef0123456789abcdef01234567 # v7
        with:
          name: determinism-${{ matrix.arch }}-${{ matrix.codegen }}
          path: target/determinism/
          if-no-files-found: error
  determinism-compare:
    needs: determinism-matrix
    strategy:
      matrix:
        include:
          - runner: ubuntu-latest
            arch: x86_64
            foreign: aarch64
          - runner: ubuntu-24.04-arm
            arch: aarch64
            foreign: x86_64
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - run: >-
          cmp determinism-x86_64-debug/portable-state-debug.bin determinism-x86_64-release/portable-state-release.bin &&
          cmp determinism-x86_64-debug/target-state-debug.bin determinism-x86_64-release/target-state-release.bin &&
          cmp determinism-aarch64-debug/portable-state-debug.bin determinism-aarch64-release/portable-state-release.bin &&
          cmp determinism-aarch64-debug/target-state-debug.bin determinism-aarch64-release/target-state-release.bin
      - run: >-
          cmp determinism-x86_64-debug/portable-state-debug.bin determinism-aarch64-debug/portable-state-debug.bin &&
          ! cmp -s determinism-x86_64-debug/target-state-debug.bin determinism-aarch64-debug/target-state-debug.bin
      - run: |
          test "$CELL_ARCH" != "$FOREIGN_ARCH"
          export OCE_FOREIGN_TARGET_STATE_IN="$PWD/target/determinism/determinism-$FOREIGN_ARCH-debug/target-state-debug.bin"
          cargo nextest run --archive-file "target/determinism/determinism-$CELL_ARCH-debug/oce-api-lib.tar.zst" \
            --workspace-remap . --profile ci --no-tests=fail \
            -E 'test(=tests::state_portability_tests::foreign_matrix_target_snapshot_refuses_restore_when_supplied)'
  crate-tests:
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - run: rm -f target/nextest/ci/junit.xml
      - run: >-
          cargo nextest run -p oce-graph -p oce-validate -p oce-model -p oce-cxf -p oce-store --locked --profile ci --no-tests=fail
      - run: test -s target/nextest/ci/junit.xml
  docs:
    steps:
      - run: bash scripts/docs/install-mdbook.sh target/mdbook-bin
      - run: python3 scripts/docs/test_check_links.py
      - run: python3 scripts/docs/test_authority_literals.py --mdbook target/mdbook-bin/mdbook
      - run: >-
          python3 scripts/docs/check_integrity.py
          --output target/docs-book
          --revision "$DOCS_REPOSITORY_REVISION"
      - run: target/mdbook-bin/mdbook build target/docs-book
      - run: >-
          python3 scripts/docs/check_links.py
          target/docs-book/book
          --site-prefix /open-control-engine/
  ci-ok:
    name: CI OK
    if: always()
    needs:
      - default-no-db
      - unused-deps
      - gate-fixtures
      - golden-gen-firewall
      - package-publication-contract
      - strict-bit-matrix
      - strict-bit-compare
      - determinism-matrix
      - determinism-compare
      - crate-tests
      - docs
    steps:
      - env:
          NEEDS_JSON: ${{ toJSON(needs) }}
        run: python3 check.py
EOF
  cat > "$dir/release-gate.yml" <<'EOF'
on:
  schedule:
    - cron: "17 8 * * *"
  workflow_dispatch:
jobs:
  test-suite:
    steps:
      - uses: actions/checkout@89abcdef0123456789abcdef0123456789abcdef # v4
        with:
          ref: ${{ github.event_name == 'schedule' && 'development' || github.ref }}
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - name: Clear cached nextest reports
        run: rm -f target/nextest/{ci,ci-release}/junit.xml
      - run: cargo nextest run --workspace --locked --profile ci --no-tests=fail
      - run: cargo nextest run --workspace --locked --profile ci-release --cargo-profile release --no-tests=fail
      - run: cargo test --workspace --doc --locked
      - run: >-
          for profile in ci ci-release;
          do test -s "target/nextest/$profile/junit.xml";
          done
  default-no-db:
    steps:
      - run: bash .github/scripts/check-default-no-db.sh
  unused-deps:
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
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
  ci-ok:
    name: CI OK
    if: always()
    needs: [test-suite, default-no-db, unused-deps, gate-fixtures, golden-gen-firewall]
    steps:
      - env:
          NEEDS_JSON: ${{ toJSON(needs) }}
        run: python3 check.py
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
  grep -v 'check-stale-crate-status' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

reinstall_gate_nightly() {
  dir="$1"
  _deny="$2"
  awk '
    { print }
    /tool: nextest@0\.9\.143/ {
      print "      - run: rustup toolchain install nightly-2026-05-01 --profile minimal"
    }
  ' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

remove_golden_gen_firewall() {
  dir="$1"
  _deny="$2"
  grep -v 'check-golden-gen-anti-tautology' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_package_publication_contract() {
  dir="$1"
  _deny="$2"
  grep -v 'scripts/package_policy' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

comment_authority_claim_check() {
  dir="$1"
  sed 's@run: python3 scripts/authority_claims/check.py --check@run: true # python3 scripts/authority_claims/check.py --check@' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_authority_claim_controls() {
  dir="$1"
  grep -v 'scripts/authority_claims/test_check.py' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
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
  grep -v 'check-default-no-db' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_determinism_matrix() {
  dir="$1"
  _deny="$2"
  grep -v 'oce-api -p oce-blocks -p oce-expr' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

comment_foreign_restore_command() {
  dir="$1"
  _deny="$2"
  grep -v 'foreign_matrix_target_snapshot_refuses_restore_when_supplied' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

allow_absent_foreign_restore_test() {
  dir="$1"
  _deny="$2"
  sed '/--workspace-remap \. --profile ci --no-tests=fail \\$/ s/ --no-tests=fail//' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_foreign_restore_nextest_install() {
  dir="$1"
  _deny="$2"
  awk '
    /^  determinism-compare:/ { in_job = 1 }
    /^  crate-tests:/ { in_job = 0 }
    in_job && /tool: nextest@0.9.143/ { next }
    { print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_aarch64_leg() {
  dir="$1"
  _deny="$2"
  sed 's/^        runner: \[ubuntu-latest, ubuntu-24\.04-arm\]$/        runner: [ubuntu-latest]/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

# aarch64 cells relabeled onto an x86_64 runner: the matrix still names both architectures, but
# no native arm64 host would ever run.
swap_aarch64_runner() {
  dir="$1"
  sed 's/^          - runner: ubuntu-24\.04-arm$/          - runner: ubuntu-latest/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

drop_release_codegen() {
  dir="$1"
  sed 's/^        codegen: \[debug, release\]$/        codegen: [debug]/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

enable_fail_fast() {
  dir="$1"
  sed 's/^      fail-fast: false$/      fail-fast: true/' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

# One emulation step, in a gating workflow or only in a comment.
reinstall_qemu() {
  dir="$1"
  awk '
    { print }
    /^  determinism-matrix:$/ { getline; print; print "    # aarch64 under qemu-user" }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

reinstall_release_cross_target() {
  dir="$1"
  awk '
    { print }
    /^      - run: cargo test --workspace --doc --locked$/ {
      print "      - run: cargo nextest run --workspace --target aarch64-unknown-linux-gnu"
    }
  ' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

detach_fan_in() {
  dir="$1"
  grep -v '^    needs: determinism-matrix$' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

omit_strict_fan_in_need() {
  dir="$1"
  grep -v '^      - strict-bit-compare$' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

stop_requiring_target_bound_difference() {
  dir="$1"
  grep -v 'cmp -s determinism-x86_64-debug/target-state-debug.bin' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

restore_own_architecture_snapshot() {
  dir="$1"
  grep -v 'test "$CELL_ARCH" != "$FOREIGN_ARCH"' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

skip_ignored_matrix_comparison() {
  dir="$1"
  sed 's/ --run-ignored only$//' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

drop_receipt_candidate_requirement() {
  dir="$1"
  grep -v 'receipt-candidate.json' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_arm_state_vector() {
  dir="$1"
  _deny="$2"
  grep -v 'OCE_PORTABLE_STATE_OUT: target/portable-state-release.bin' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

conditional_ci_ok() {
  dir="$1"
  _deny="$2"
  sed 's/^    if: always()$/    if: success()/' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

blind_release_ci_ok() {
  dir="$1"
  _deny="$2"
  grep -v 'toJSON(needs)' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

omit_ci_ok_need() {
  dir="$1"
  _deny="$2"
  grep -v '^      - golden-gen-firewall$' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

omit_release_ci_ok_need() {
  dir="$1"
  _deny="$2"
  sed 's/^    needs: \[test-suite, default-no-db, /    needs: [test-suite, /' \
    "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

restore_development_push_trigger() {
  dir="$1"
  _deny="$2"
  awk '
    { print }
    /^    branches: \[development\]$/ && !done { print "  push:"; print "    branches: [development]"; done = 1 }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

unfiltered_push_trigger() {
  dir="$1"
  _deny="$2"
  awk '
    /^  workflow_dispatch:$/ { print "  push:" }
    { print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

flow_list_push_trigger() {
  dir="$1"
  _deny="$2"
  awk '
    /^on:$/ { print "on: [pull_request, push]"; skip = 1; next }
    skip && /^  / { next }
    { skip = 0; print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

# Replace the fixture's whole top-level `on:` block (up to `jobs:`) with the given text.
replace_trigger_block() {
  dir="$1"
  block="$2"
  TRIGGER_BLOCK="$block" awk '
    /^on:/ { print ENVIRON["TRIGGER_BLOCK"]; skip = 1; next }
    skip && /^jobs:$/ { skip = 0 }
    !skip { print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

four_space_push_trigger() {
  replace_trigger_block "$1" 'on:
    pull_request:
        branches: [development]
    "push":
        branches: [development]
    workflow_dispatch:'
}

four_space_pull_request_trigger() {
  replace_trigger_block "$1" 'on:
    pull_request:
        branches: [development]
    workflow_dispatch:'
}

block_list_push_trigger() {
  replace_trigger_block "$1" 'on:
  - pull_request
  - push'
}

flow_map_push_trigger() {
  replace_trigger_block "$1" "on: {pull_request: {branches: [development]}, 'push': {branches: [development]}}"
}

flow_map_pull_request_trigger() {
  replace_trigger_block "$1" 'on: {pull_request: {branches: [development]}, workflow_dispatch: null}'
}

remove_pull_request_trigger() {
  dir="$1"
  _deny="$2"
  awk '
    /^  pull_request:$/ { skip = 1; next }
    skip && /^    / { next }
    { skip = 0; print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

conditional_gating_job() {
  dir="$1"
  _deny="$2"
  awk '
    { print }
    /^  unused-deps:$/ { print "    if: github.event_name == '"'"'schedule'"'"'" }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

hardcode_strict_cell_architecture() {
  dir="$1"
  _deny="$2"
  sed 's/strict-bits-cell.sh "$CELL_ARCH" "$CELL_CODEGEN"/strict-bits-cell.sh x86_64 "$CELL_CODEGEN"/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_nextest_report_cleanup() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/pr-gate.yml" "$dir/release-gate.yml"; do
    grep -v 'target/nextest/{' "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

remove_nextest_report_requirements() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/pr-gate.yml" "$dir/release-gate.yml"; do
    grep -v -E 'for profile in|test -s.*\$profile|^[[:space:]]*done[[:space:]]*$' \
      "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

downgrade_nextest_pin() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/pr-gate.yml" "$dir/release-gate.yml"; do
    sed 's/nextest@0\.9\.143/nextest@0.9.142/g' "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

unpin_action() {
  dir="$1"
  _deny="$2"
  sed 's#taiki-e/install-action@[0-9a-f]\{40\} \# v2#taiki-e/install-action@v2#' \
    "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

remove_crate_suite() {
  dir="$1"
  _deny="$2"
  sed 's/ -p oce-cxf -p oce-store --locked/ -p oce-cxf --locked/' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

allow_zero_crate_suite_tests() {
  dir="$1"
  _deny="$2"
  sed 's/ -p oce-store --locked --profile ci --no-tests=fail$/ -p oce-store --locked --profile ci/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_docs_link_check() {
  dir="$1"
  _deny="$2"
  grep -v 'scripts/docs/check_links.py' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_docs_checker_contracts() {
  dir="$1"
  _deny="$2"
  grep -v 'scripts/docs/test_check_links.py' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
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
run_case reinstalled-gate-nightly fail reinstall_gate_nightly \
  "release gate installs no nightly toolchain"
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
run_case missing-aarch64-runner fail remove_aarch64_leg \
  "determinism-matrix runs native x86_64 and aarch64 runners"
run_case aarch64-on-x86-runner fail swap_aarch64_runner \
  "does not map x86_64 to ubuntu-latest and aarch64 to ubuntu-24.04-arm"
run_case missing-release-codegen-cells fail drop_release_codegen \
  "determinism-matrix runs debug and release codegen"
run_case fail-fast-cells fail enable_fail_fast \
  "determinism-matrix reports every failing cell (fail-fast: false)"
run_case reinstalled-qemu fail reinstall_qemu \
  "pr-gate.yml mentions emulation or a cross-compiled aarch64 target"
run_case release-cross-target fail reinstall_release_cross_target \
  "release-gate.yml mentions emulation or a cross-compiled aarch64 target"
run_case detached-determinism-fan-in fail detach_fan_in \
  "job determinism-compare does not need determinism-matrix"
run_case omitted-strict-fan-in-need fail omit_strict_fan_in_need \
  "pr-gate.yml CI OK needs does not match the workflow's jobs"
run_case target-bound-difference-unchecked fail stop_requiring_target_bound_difference \
  "require target-bound state bytes to differ across architectures"
run_case own-architecture-restore fail restore_own_architecture_snapshot \
  "the restoring architecture differs from the snapshot architecture"
run_case unrun-matrix-comparison fail skip_ignored_matrix_comparison \
  "the eight-capture comparison hard-fails when its test is absent"
run_case missing-receipt-candidate fail drop_receipt_candidate_requirement \
  "a green comparison writes the candidate receipt-log record"
run_case missing-release-state-vector fail remove_arm_state_vector \
  "emit the release portable state vector in every determinism cell"
run_case conditional-ci-ok fail conditional_ci_ok \
  "CI OK aggregate reports on every run"
run_case blind-release-ci-ok fail blind_release_ci_ok \
  "release gate CI OK aggregate inspects every needed result"
run_case omitted-ci-ok-need fail omit_ci_ok_need \
  "pr-gate.yml CI OK needs does not match the workflow's jobs"
run_case omitted-release-ci-ok-need fail omit_release_ci_ok_need \
  "release-gate.yml CI OK needs does not match the workflow's jobs"
run_case conditional-gating-job fail conditional_gating_job \
  "gating job has a job-level if: that CI OK would read as failure: unused-deps"
run_case restored-development-push-trigger fail restore_development_push_trigger \
  "pr-gate.yml has a push trigger; the per-PR gate must not re-run on merges into development"
run_case unfiltered-push-trigger fail unfiltered_push_trigger \
  "pr-gate.yml has a push trigger"
run_case flow-list-push-trigger fail flow_list_push_trigger \
  "pr-gate.yml has a push trigger"
run_case missing-pull-request-trigger fail remove_pull_request_trigger \
  "pr-gate.yml is missing its pull_request trigger"
run_case four-space-push-trigger fail four_space_push_trigger \
  "pr-gate.yml has a push trigger"
run_case four-space-pull-request-trigger pass four_space_pull_request_trigger
run_case block-list-push-trigger fail block_list_push_trigger \
  "pr-gate.yml has a push trigger"
run_case flow-map-push-trigger fail flow_map_push_trigger \
  "pr-gate.yml has a push trigger"
run_case flow-map-pull-request-trigger pass flow_map_pull_request_trigger
run_case hardcoded-strict-cell fail hardcode_strict_cell_architecture \
  "native strict-bit cell for the matrix architecture and codegen"
run_case missing-nextest-report-cleanup fail remove_nextest_report_cleanup \
  "clear cached nextest JUnit reports"
run_case missing-nextest-report-requirements fail remove_nextest_report_requirements \
  "refuse an absent or empty nextest determinism report"
run_case stale-nextest-pin fail downgrade_nextest_pin \
  "pinned cargo-nextest 0.9.143 install"
run_case tag-referenced-action fail unpin_action \
  "uses an action not pinned to a full commit SHA"
run_case missing-store-crate-suite fail remove_crate_suite \
  "run the oce-graph, oce-validate, oce-model, oce-cxf and oce-store suites per PR"
run_case crate-suite-allows-zero-tests fail allow_zero_crate_suite_tests \
  "run the oce-graph, oce-validate, oce-model, oce-cxf and oce-store suites per PR and hard-fail on zero tests"
run_case missing-docs-link-check fail remove_docs_link_check \
  "check generated internal links and anchors"
run_case missing-docs-checker-contracts fail remove_docs_checker_contracts \
  "run the documentation checker contracts"
run_case empty-crate-dir-only fail empty_crate_dir_only \
  "no crate Cargo.toml files found under"
run_case missing-crate-lib-files fail remove_crate_lib_files \
  "no crate src/lib.rs files found under"
run_case missing-crates-dir fail remove_crates_dir \
  "crates directory is missing"

echo "OK: workflow gate fixtures passed."
