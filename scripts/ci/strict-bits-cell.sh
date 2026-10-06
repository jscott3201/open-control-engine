#!/usr/bin/env bash
# One Linux strict-bit cell of the per-PR gate (pr-gate.yml): two independent capture processes, a
# byte-for-byte repeat comparison, then the exact signal wiring, corpus firewall and mutation
# controls — the same commands the GitHub-era `strict-bit-matrix` job ran per native runner.
#
#   bash scripts/ci/strict-bits-cell.sh <x86_64|aarch64> <debug|release>
#
# x86_64 runs natively on the runner. aarch64 is cross-compiled and executed under QEMU user-mode
# emulation (see .github/workflows/pr-gate.yml for the toolchain setup): it exercises the aarch64
# codegen and the byte-exact cross-cell comparison on every PR, but it is emulated evidence, not a
# native aarch64 capture, and is never admitted as native qualification.
#
# Captures are written to target/strict-bits/linux-<arch>-<codegen>-<first|repeat>.json, the file
# names `matrix::native_linux_matrix_is_complete_and_exact` reads.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

arch="${1:-}"
codegen="${2:-}"

case "$codegen" in
  debug) cargo_profile=dev nextest_profile=ci ;;
  release) cargo_profile=release nextest_profile=ci-release ;;
  *)
    echo "usage: bash scripts/ci/strict-bits-cell.sh <x86_64|aarch64> <debug|release>" >&2
    exit 2
    ;;
esac

target_args=()
case "$arch" in
  x86_64)
    if [ "$(uname -m)" != x86_64 ]; then
      echo "x86_64 strict-bit cell must run on an x86_64 host, not $(uname -m)" >&2
      exit 1
    fi
    ;;
  aarch64)
    target_args=(--target aarch64-unknown-linux-gnu --config-file "$PWD/scripts/ci/nextest-emulated.toml")
    ;;
  *)
    echo "usage: bash scripts/ci/strict-bits-cell.sh <x86_64|aarch64> <debug|release>" >&2
    exit 2
    ;;
esac

mkdir -p target/strict-bits
for repeat in first repeat; do
  OCE_STRICT_BITS_OUT="target/strict-bits/linux-$arch-$codegen-$repeat.json" \
    cargo nextest run -p oce-conformance --test strict_bits --locked ${target_args[@]+"${target_args[@]}"} \
    --profile "$nextest_profile" --cargo-profile "$cargo_profile" --no-tests=fail \
    -E 'test(=capture_raw_bits_without_reblessing_the_oracle)'
done
cmp "target/strict-bits/linux-$arch-$codegen-first.json" \
  "target/strict-bits/linux-$arch-$codegen-repeat.json"

cargo nextest run -p oce-conformance --test strict_bits --locked ${target_args[@]+"${target_args[@]}"} \
  --test per_block_reals_transcendental --test per_block_reals_sources_transcendental \
  --test per_block_psychrometrics --test per_block_utilities \
  --profile "$nextest_profile" --cargo-profile "$cargo_profile" --no-tests=fail
