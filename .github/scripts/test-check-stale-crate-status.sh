#!/usr/bin/env bash
# Gate-behavior fixtures for check-stale-crate-status.sh.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT=".github/scripts/check-stale-crate-status.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

write_file() {
  path="$1"
  body="$2"
  mkdir -p "$(dirname "$path")"
  printf '%s\n' "$body" > "$path"
}

run_case() {
  name="$1"
  expected="$2"
  list_mode="$3"
  file_body="${4:-}"

  dir="$tmp/$name"
  mkdir -p "$dir"
  list="$dir/list.txt"

  case "$list_mode" in
    valid)
      file="$dir/crates/oce-demo/src/lib.rs"
      write_file "$file" "$file_body"
      printf '%s\n' "$file" > "$list"
      ;;
    late-status)
      file="$dir/crates/oce-late/src/lib.rs"
      mkdir -p "$(dirname "$file")"
      i=1
      while [ "$i" -lt 46 ]; do
        printf '%s\n' "//! filler line $i" >> "$file"
        i=$((i + 1))
      done
      printf '%s\n' "$file_body" >> "$file"
      printf '%s\n' "$file" > "$list"
      ;;
    empty-list)
      : > "$list"
      ;;
    missing-file)
      printf '%s\n' "$dir/crates/oce-missing/src/lib.rs" > "$list"
      ;;
    *)
      echo "BUG: unknown list mode '$list_mode'"
      exit 1
      ;;
  esac

  set +e
  output="$(
    OCE_STALE_STATUS_FILE_LIST="$list" \
    bash "$SCRIPT" 2>&1
  )"
  status=$?
  set -e

  case "$expected" in
    pass)
      if [ "$status" -ne 0 ]; then
        echo "FAIL: stale-status fixture '$name' should pass but exited $status"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^OK:'; then
        echo "FAIL: stale-status fixture '$name' passed without the OK line"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    fail)
      if [ "$status" -eq 0 ]; then
        echo "FAIL: stale-status fixture '$name' should fail but exited 0"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^FAIL:'; then
        echo "FAIL: stale-status fixture '$name' failed without a FAIL line"
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

run_case positive pass valid '//! Status: **M1 as-built.** Active implementation.'
run_case roadmap-prose pass valid '//! Roadmap: phased across M0-M2.
//! Status: **M1 as-built.** Active implementation.'
run_case m0-scaffold fail valid '//! Status: **M0 scaffold.** This text is stale.'
run_case m0-starter-catalog fail valid '//! Status: **M0 starter catalog.** This text is stale.'
run_case m0-dot fail valid '//! Status: **M0.** This text is stale.'
run_case late-m0-status fail late-status '//! Status: **M0.** This deep status line is stale.'
run_case empty-list fail empty-list
run_case missing-file fail missing-file

echo "OK: stale crate-status fixtures passed."
