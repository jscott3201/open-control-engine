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
  gate:
    name: gate (light)
    steps:
      - run: bash .agents/gate.sh
  determinism-matrix:
    strategy:
      matrix:
        runner: [ubuntu-latest, ubuntu-24.04-arm]
    runs-on: ${{ matrix.runner }}
    steps:
      - run: cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
      - run: cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --cargo-profile release --no-tests=fail
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
      - run: cargo nextest run --workspace --locked --profile ci --no-tests=fail
      - run: cargo nextest run --workspace --locked --profile ci --cargo-profile release --no-tests=fail
      - run: cargo test --workspace --doc --locked
      - env:
          OCE_REQUIRE_SURFACE_CHECK: "1"
        run: cargo nextest run -p oce-api -E 'test(public_api_surface_matches_blessed_baseline)' --profile public-api --locked --no-tests=fail
      - name: cargo public-api surface gate (oce-store)
        env:
          OCE_REQUIRE_SURFACE_CHECK: "1"
        run: cargo nextest run -p oce-store -E 'test(public_api_surface_matches_blessed_baseline)' --profile public-api --locked --no-tests=fail
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
  gate:
    name: gate (full)
    steps:
      - run: bash .agents/gate.sh full
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
  grep -v 'oce-blocks -p oce-expr' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
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

remove_ci_gate_job() {
  dir="$1"
  _deny="$2"
  grep -v 'agents/gate\.sh' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_release_gate_job() {
  dir="$1"
  _deny="$2"
  grep -v 'agents/gate\.sh' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

rename_ci_gate_job() {
  dir="$1"
  _deny="$2"
  sed 's/gate (light)/gate (lite)/' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

# The pin that guards the pin. Swapping ci.yml's `light` invocation for `full` leaves the string
# `bash .agents/gate.sh` present, so a pattern without the `$` anchor would still match and the
# per-PR mode — the default a contributor actually runs — would be executed nowhere. A withdrawn
# earlier attempt at gate/CI parity shipped exactly this hole: both of its traces ran `full`, so
# `light` was never covered and the gap was invisible.
downgrade_ci_gate_to_full() {
  dir="$1"
  _deny="$2"
  sed 's|run: bash .agents/gate.sh$|run: bash .agents/gate.sh full|' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
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
run_case empty-crate-dir-only fail empty_crate_dir_only \
  "no crate Cargo.toml files found under"
run_case missing-crate-lib-files fail remove_crate_lib_files \
  "no crate src/lib.rs files found under"
run_case missing-crates-dir fail remove_crates_dir \
  "crates directory is missing"
run_case missing-ci-gate-job fail remove_ci_gate_job \
  "ci executes the gate script in light mode"
run_case missing-release-gate-job fail remove_release_gate_job \
  "release gate executes the gate script in full mode"
run_case renamed-ci-gate-job fail rename_ci_gate_job \
  "gate job keeps the name branch protection requires"
run_case ci-gate-downgraded-to-full fail downgrade_ci_gate_to_full \
  "ci executes the gate script in light mode"

echo "OK: workflow gate fixtures passed."
