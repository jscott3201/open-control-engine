#!/usr/bin/env bash
# Gate-behavior fixtures for check-default-no-db.sh. These run without Cargo network/build work by
# injecting workspace-member and cargo-tree fixtures through the script's test-only env vars.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT=".github/scripts/check-default-no-db.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

run_fixture() {
  name="$1"
  expected="$2"
  members="$tmp/$name.members"
  trees="$tmp/$name.trees"
  shift 2

  mkdir -p "$trees"
  "$@" "$members" "$trees"

  set +e
  output="$(
    OCE_NO_DB_MEMBERS_FILE="$members" \
    OCE_NO_DB_TREE_DIR="$trees" \
    bash "$SCRIPT" 2>&1
  )"
  status=$?
  set -e

  case "$expected" in
    pass)
      if [ "$status" -ne 0 ]; then
        echo "FAIL: fixture '$name' should pass but exited $status"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^OK:'; then
        echo "FAIL: fixture '$name' passed without the OK line"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    fail)
      if [ "$status" -eq 0 ]; then
        echo "FAIL: fixture '$name' should fail but exited 0"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^FAIL:'; then
        echo "FAIL: fixture '$name' failed without a FAIL line"
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

positive_fixture() {
  members="$1"
  trees="$2"
  printf '%s\n' oce-api oce-docs > "$members"
  cat > "$trees/oce-api.tree" <<'EOF'
oce-api v0.0.0 (/repo/crates/oce-api)
├── oce-model v0.0.0 (/repo/crates/oce-model)
└── thiserror v2.0.0
EOF
  cat > "$trees/oce-docs.tree" <<'EOF'
oce-docs v0.0.0 (/repo/crates/oce-docs)
└── oce-store v0.0.0 (/repo/crates/oce-store)
EOF
}

forbidden_non_facade_fixture() {
  members="$1"
  trees="$2"
  printf '%s\n' oce-api oce-conformance > "$members"
  cat > "$trees/oce-api.tree" <<'EOF'
oce-api v0.0.0 (/repo/crates/oce-api)
└── oce-model v0.0.0 (/repo/crates/oce-model)
EOF
  cat > "$trees/oce-conformance.tree" <<'EOF'
oce-conformance v0.0.0 (/repo/crates/oce-conformance)
└── tokio v1.0.0
EOF
}

empty_members_fixture() {
  members="$1"
  _trees="$2"
  : > "$members"
}

missing_tree_fixture() {
  members="$1"
  _trees="$2"
  printf '%s\n' oce-api > "$members"
}

garbled_tree_fixture() {
  members="$1"
  trees="$2"
  printf '%s\n' oce-api > "$members"
  printf '%s\n' 'not a cargo tree' > "$trees/oce-api.tree"
}

run_fixture positive pass positive_fixture
run_fixture forbidden-non-facade fail forbidden_non_facade_fixture
run_fixture empty-members fail empty_members_fixture
run_fixture missing-tree fail missing_tree_fixture
run_fixture garbled-tree fail garbled_tree_fixture

echo "OK: check-default-no-db gate fixtures passed."
