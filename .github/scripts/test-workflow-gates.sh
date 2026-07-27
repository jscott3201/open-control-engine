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
  gate_script="$5"
  mkdir -p "$dir"
  # A stand-in for .agents/gate.sh. Two invocations are deliberately split across lines with a
  # trailing backslash — the same way the real script writes them — so this fixture exercises the
  # checker's continuation-joining. If that flattening ever regresses, THIS positive case goes red
  # rather than every gate.sh assertion silently matching nothing.
  cat > "$gate_script" <<'EOF'
#!/usr/bin/env bash
step 'clippy' cargo clippy --workspace --all-targets --locked -- -D warnings
step 'determinism subset' \
  cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
step 'determinism subset (release codegen)' \
  cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci \
  --cargo-profile release --no-tests=fail
step 'nextest — workspace' \
  cargo nextest run --workspace --locked --profile ci --no-tests=fail
step 'nextest — workspace (release codegen)' \
  cargo nextest run --workspace --locked --profile ci \
  --cargo-profile release --no-tests=fail
step 'doctests' cargo test --workspace --doc --locked
EOF
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
  gate_script="$tmp/$name/gate.sh"
  mkdir -p "$tmp/$name"
  write_positive "$dir" "$deny" "$root_cargo" "$crates_dir" "$gate_script"
  # The 5th argument is new; mutators that predate it simply ignore it.
  "$mutate" "$dir" "$deny" "$root_cargo" "$crates_dir" "$gate_script"

  set +e
  output="$(
    OCE_WORKFLOW_DIR="$dir" \
    OCE_DENY_TOML="$deny" \
    OCE_ROOT_CARGO_TOML="$root_cargo" \
    OCE_CRATES_DIR="$crates_dir" \
    OCE_GATE_SCRIPT="$gate_script" \
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

remove_crates_dir() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -rf "$crates_dir"
}

# --- gate.sh drift fixtures ---------------------------------------------------------------
# The local gate script is the one place a contributor's "it passed locally" comes from, so each
# way it can silently fall behind CI gets its own negative case.

gate_drops_release_determinism() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  grep -v -- '--cargo-profile release --no-tests=fail' "$gate_script" > "$gate_script.tmp"
  mv "$gate_script.tmp" "$gate_script"
}

gate_drops_doctests() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  grep -v -- 'cargo test --workspace --doc --locked' "$gate_script" > "$gate_script.tmp"
  mv "$gate_script.tmp" "$gate_script"
}

gate_weakens_clippy() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  # Drops `-D warnings`, so warnings stop failing the local run while CI still rejects them.
  sed 's/ -- -D warnings//' "$gate_script" > "$gate_script.tmp"
  mv "$gate_script.tmp" "$gate_script"
}

gate_lints_all_features() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  # Lints a build CI never produces, defeating the database-free default promise.
  sed 's/cargo clippy --workspace/cargo clippy --all-features --workspace/' "$gate_script" \
    > "$gate_script.tmp"
  mv "$gate_script.tmp" "$gate_script"
}

gate_script_missing() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  rm -f "$gate_script"
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
run_case gate-drops-release-determinism fail gate_drops_release_determinism \
  "gate.sh runs the release determinism subset in CI form"
run_case gate-drops-doctests fail gate_drops_doctests \
  "gate.sh full runs doctests, which nextest cannot"
run_case gate-weakens-clippy fail gate_weakens_clippy \
  "gate.sh runs clippy with -D warnings over all targets"
run_case gate-lints-all-features fail gate_lints_all_features \
  "lints --all-features"
run_case gate-script-missing fail gate_script_missing \
  "required gate file is missing or empty"

echo "OK: workflow gate fixtures passed."
