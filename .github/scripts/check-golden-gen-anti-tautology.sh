#!/usr/bin/env bash
# Anti-tautology firewall for tools/golden-gen.
#
# golden-gen is allowed to depend on general-purpose formatting / JSON crates, but it must never
# depend on any `oce-*` crate. The sequence references it emits are independent oracles, not
# replays of the implementation under test.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

manifest="${OCE_GOLDEN_GEN_MANIFEST:-tools/golden-gen/Cargo.toml}"

metadata() {
  if [ -n "${OCE_GOLDEN_GEN_METADATA_FILE:-}" ]; then
    if [ ! -s "$OCE_GOLDEN_GEN_METADATA_FILE" ]; then
      echo "FAIL: golden-gen metadata fixture is missing or empty: $OCE_GOLDEN_GEN_METADATA_FILE"
      return 1
    fi
    cat "$OCE_GOLDEN_GEN_METADATA_FILE"
    return
  fi

  if ! cargo metadata --manifest-path "$manifest" --locked --format-version 1 2>&1; then
    echo "FAIL: cargo metadata failed for $manifest (firewall fails closed)"
    return 1
  fi
}

if ! payload="$(metadata)"; then
  printf '%s\n' "$payload"
  exit 1
fi

if ! printf '%s\n' "$payload" | grep -Eq '"name"[[:space:]]*:[[:space:]]*"golden-gen"'; then
  echo "FAIL: cargo metadata for $manifest did not include the golden-gen package"
  exit 1
fi

if printf '%s\n' "$payload" | grep -Eq '"name"[[:space:]]*:[[:space:]]*"oce-[^"]*"'; then
  echo "FAIL: tools/golden-gen dependency graph contains an oce-* crate:"
  printf '%s\n' "$payload" | grep -Eo '"name"[[:space:]]*:[[:space:]]*"oce-[^"]*"'
  exit 1
fi

echo "OK: tools/golden-gen dependency graph contains no oce-* crates."
