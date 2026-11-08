#!/usr/bin/env bash
# The `no_std` blob execution entry's dependency closure (issue #401). A bare-metal or WASM target
# links `oce-graph` built `--no-default-features --features alloc`; its normal dependency closure
# must contain no `oce-cxf`, `oce-api` or `serde_json` and no crate named `std`, and no crate in it
# may have its `std` feature enabled. The issue states the acceptance as
#
#   cargo tree -p oce-graph --no-default-features --features alloc -e normal
#
# listing none of those; this script runs that tree with each package's enabled features printed
# (`-f '{p}|{f}'`) and proc-macro subtrees left out (`no-proc-macro`: they run on the build host,
# never on the target).
#
# A check that cannot fail is not a gate, so two controls run first, through the same matcher:
# the default (`std`) build of `oce-graph` must fail on its `std` features, and `oce-api`'s
# closure must fail on `oce-cxf` and `serde_json`. Plain cargo, no network; `--locked` as in CI.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

FORBIDDEN=(oce-cxf oce-api serde_json std)

# Print one violation per line for a `{p}|{f}` tree read from stdin; print nothing when clean.
violations() {
  local line package features name feature
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    package="${line%%|*}"
    features="${line#*|}"
    name="${package%% *}"
    for forbidden in "${FORBIDDEN[@]}"; do
      if [ "$name" = "$forbidden" ]; then
        echo "crate $name is in the closure"
      fi
    done
    IFS=',' read -r -a enabled <<<"${features% (\*)}"
    for feature in ${enabled[@]+"${enabled[@]}"}; do
      if [ "$feature" = "std" ]; then
        echo "crate $name has its std feature enabled"
      fi
    done
  done
}

tree() {
  cargo tree "$@" -e normal,no-proc-macro --prefix none --locked -f '{p}|{f}'
}

# Controls: the matcher must fire on a closure that does link std and the host-side crates.
control_std="$(tree -p oce-graph | violations | sort -u)"
if ! printf '%s\n' "$control_std" | grep -q -F 'crate oce-graph has its std feature enabled'; then
  echo "FAIL: control: the default oce-graph build did not trip the std-feature check" >&2
  exit 1
fi
control_crates="$(tree -p oce-api | violations | sort -u)"
for crate in oce-cxf serde_json; do
  if ! printf '%s\n' "$control_crates" | grep -q -F "crate $crate is in the closure"; then
    echo "FAIL: control: oce-api's closure did not trip the $crate check" >&2
    exit 1
  fi
done

found="$(tree -p oce-graph --no-default-features --features alloc | violations | sort -u)"
if [ -n "$found" ]; then
  echo "FAIL: the no_std oce-graph closure links what a target must not:" >&2
  printf '  %s\n' "$found" >&2
  exit 1
fi
echo "OK: oce-graph (no_std + alloc) links no oce-cxf, oce-api, serde_json or std; controls fired."
