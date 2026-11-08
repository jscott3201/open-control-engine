#!/usr/bin/env bash
# The cargo target runner for wasm32-wasip1 (`.cargo/config.toml`), so `cargo test` and
# `cargo nextest run --target wasm32-wasip1` execute test binaries under wasmtime (issue #403).
#
#   scripts/ci/wasmtime-runner.sh <module.wasm> [test-harness arguments...]
#
# The guest sees the workspace root at the same absolute path it has on the host, because the test
# binaries name their fixtures through `env!("CARGO_MANIFEST_DIR")`, which the compiler bakes in
# as the host path; nothing outside the workspace is visible. A round-trip artifact directory set
# as an absolute path outside the workspace is granted too. The two round-trip variables and
# RUST_BACKTRACE are passed through when the host sets them; no other host variable reaches the
# guest. `wasmtime` must be on PATH (scripts/ci/install-wasmtime.sh installs the pinned release).
set -euo pipefail

if [ "$#" -lt 1 ]; then
  echo "usage: wasmtime-runner.sh <module.wasm> [arguments...]" >&2
  exit 2
fi
if ! command -v wasmtime >/dev/null 2>&1; then
  echo "wasmtime-runner.sh: wasmtime is not on PATH; run scripts/ci/install-wasmtime.sh and add its directory to PATH" >&2
  exit 127
fi

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
module="$1"
shift

args=(run --dir "$root")
for var in OCE_ROUND_TRIP_OUT OCE_ROUND_TRIP_EXPECT; do
  value="${!var-}"
  [ -n "$value" ] || continue
  args+=(--env "$var=$value")
  case "$value" in
    "$root" | "$root"/*) ;;
    /*)
      if [ "$var" = OCE_ROUND_TRIP_OUT ]; then mkdir -p "$value"; fi
      args+=(--dir "$value")
      ;;
  esac
done
if [ -n "${RUST_BACKTRACE-}" ]; then
  args+=(--env "RUST_BACKTRACE=$RUST_BACKTRACE")
fi

exec wasmtime "${args[@]}" -- "$module" "$@"
