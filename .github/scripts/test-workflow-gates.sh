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
  mkdir -p "$dir"
  cat > "$dir/ci.yml" <<'EOF'
jobs:
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
  unused-deps:
    steps:
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-machete
      - run: cargo machete
  gate-fixtures:
    steps:
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
}

run_case() {
  name="$1"
  expected="$2"
  mutate="$3"
  dir="$tmp/$name/workflows"
  deny="$tmp/$name/deny.toml"
  mkdir -p "$tmp/$name"
  write_positive "$dir" "$deny"
  "$mutate" "$dir" "$deny"

  set +e
  output="$(
    OCE_WORKFLOW_DIR="$dir" \
    OCE_DENY_TOML="$deny" \
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

run_case positive pass noop
run_case missing-release-nextest fail remove_release_nextest
run_case seeded-advisory-ignore fail seed_advisory_ignore
run_case garbled-workflow fail garble_release_workflow
run_case missing-stale-status-gate fail remove_stale_status_gate
run_case empty-bans-deny fail empty_bans_deny
run_case missing-store-surface-gate fail remove_store_surface_gate
run_case missing-golden-gen-firewall fail remove_golden_gen_firewall

echo "OK: workflow gate fixtures passed."
