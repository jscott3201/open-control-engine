#!/usr/bin/env bash
# Gate-behavior fixtures for check-golden-gen-anti-tautology.sh.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT=".github/scripts/check-golden-gen-anti-tautology.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

run_fixture() {
  name="$1"
  expected="$2"
  writer="$3"
  metadata_file="$tmp/$name.json"
  "$writer" "$metadata_file"

  set +e
  output="$(
    OCE_GOLDEN_GEN_METADATA_FILE="$metadata_file" \
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

positive_metadata() {
  file="$1"
  cat > "$file" <<'EOF'
{"packages":[{"name":"golden-gen"},{"name":"ryu"},{"name":"serde_json"}]}
EOF
}

oce_dependency_metadata() {
  file="$1"
  cat > "$file" <<'EOF'
{"packages":[{"name":"golden-gen"},{"name":"oce-blocks"},{"name":"serde_json"}]}
EOF
}

missing_root_metadata() {
  file="$1"
  cat > "$file" <<'EOF'
{"packages":[{"name":"ryu"}]}
EOF
}

empty_metadata() {
  file="$1"
  : > "$file"
}

run_fixture positive pass positive_metadata
run_fixture oce-dependency fail oce_dependency_metadata
run_fixture missing-root fail missing_root_metadata
run_fixture empty-metadata fail empty_metadata

echo "OK: golden-gen anti-tautology firewall fixtures passed."
