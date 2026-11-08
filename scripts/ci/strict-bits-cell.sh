#!/usr/bin/env bash
# One Linux strict-bit cell of the per-PR gate (pr-gate.yml): two independent capture processes, a
# byte-for-byte repeat comparison, then the exact signal wiring, corpus firewall and mutation
# controls.
#
#   bash scripts/ci/strict-bits-cell.sh <x86_64|aarch64> <debug|release>
#
# Every cell runs natively: x86_64 on an x86_64 host, aarch64 on an aarch64 host
# (`ubuntu-24.04-arm` in CI). The script refuses a host whose `uname -m` is not the named
# architecture, so a mislabeled runner cannot produce a capture under the wrong cell name. Nothing
# is cross-compiled or emulated.
#
# Captures are written to target/strict-bits/linux-<arch>-<codegen>-<first|repeat>.json, the file
# names `matrix::native_linux_matrix_is_complete_and_exact` reads in the strict-bit-compare fan-in.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

usage() {
  echo "usage: bash scripts/ci/strict-bits-cell.sh <x86_64|aarch64> <debug|release>" >&2
  exit 2
}

arch="${1:-}"
codegen="${2:-}"

case "$codegen" in
  debug) cargo_profile=dev nextest_profile=ci ;;
  release) cargo_profile=release nextest_profile=ci-release ;;
  *) usage ;;
esac

case "$arch" in
  x86_64 | aarch64) ;;
  *) usage ;;
esac

if [ "$(uname -m)" != "$arch" ]; then
  echo "$arch strict-bit cell must run natively on an $arch host, not $(uname -m)" >&2
  exit 1
fi

mkdir -p target/strict-bits
for repeat in first repeat; do
  OCE_STRICT_BITS_OUT="target/strict-bits/linux-$arch-$codegen-$repeat.json" \
    cargo nextest run -p oce-conformance --test strict_bits --locked \
    --profile "$nextest_profile" --cargo-profile "$cargo_profile" --no-tests=fail \
    -E 'test(=capture_raw_bits_without_reblessing_the_oracle)'
done
cmp "target/strict-bits/linux-$arch-$codegen-first.json" \
  "target/strict-bits/linux-$arch-$codegen-repeat.json"

cargo nextest run -p oce-conformance --test strict_bits --locked \
  --test per_block_reals_transcendental --test per_block_reals_sources_transcendental \
  --test per_block_psychrometrics --test per_block_utilities \
  --profile "$nextest_profile" --cargo-profile "$cargo_profile" --no-tests=fail
