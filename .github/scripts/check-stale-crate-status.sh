#!/usr/bin/env bash
# Crate-root status headers must describe the current milestone honestly. A bolded status token
# starting with M0 is stale after M1; Deferred-OK crates should say what milestone owns them.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

list_file="${OCE_STALE_STATUS_FILE_LIST:-}"
tmp_list=""

if [ -n "$list_file" ]; then
  if [ ! -s "$list_file" ]; then
    echo "FAIL: stale-status input list is missing or empty: $list_file"
    exit 1
  fi
else
  tmp_list="$(mktemp)"
  trap 'rm -f "$tmp_list"' EXIT
  git ls-files 'crates/*/src/lib.rs' > "$tmp_list"
  list_file="$tmp_list"
  if [ ! -s "$list_file" ]; then
    echo "FAIL: no crate-root lib.rs files discovered"
    exit 1
  fi
fi

violations=0
checked=0

while IFS= read -r path; do
  [ -n "$path" ] || continue
  checked=$((checked + 1))
  if [ ! -s "$path" ]; then
    echo "FAIL: crate-root status target is missing or empty: $path"
    violations=$((violations + 1))
    continue
  fi
  if grep -nE '^//![[:space:]]*Status:.*\*\*M0' "$path" >/dev/null; then
    echo "FAIL: stale crate-root status header starts with bolded M0 token: $path"
    grep -nE '^//![[:space:]]*Status:.*\*\*M0' "$path" || true
    violations=$((violations + 1))
  fi
done < "$list_file"

if [ "$checked" -eq 0 ]; then
  echo "FAIL: stale-status input list contained no crate-root files"
  exit 1
fi

if [ "$violations" -gt 0 ]; then
  echo
  echo "Replace stale bolded M0 crate-root status wording with the as-built or Deferred-OK milestone."
  exit 1
fi

echo "OK: crate-root status headers contain no stale bolded M0 status wording."
