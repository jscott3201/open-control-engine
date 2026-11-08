#!/usr/bin/env bash
# The firmware image (issue #402): firmware/link's `blob-frames` image, which runs a checked-in
# compiled-schedule blob through the `no_std` engine entry, linked for each bare-metal target and
# held to its size budget. Adapted from rusty-bacnet's scripts/ci/check-firmware-link.sh (same
# owner).
#
#   check-firmware-link.sh                       the default targets below
#   check-firmware-link.sh <triple>...           those targets
#   check-firmware-link.sh --suggest [<triple>...]
#                                                print a budget table per image instead (measured
#                                                + 12 %, rounded up to 16 octets)
#
# Building the image proves the core four link `no_std + alloc` into a bare-metal binary with no
# symbol only a hosted target provides; a library build (check-no-std.sh) never links. Per target:
# the image crate's own graph through check-no-std.sh's scan (no std feature, no host-only crate,
# with the image's runtime, allocator and semihosting in it), clippy -D warnings on the image's
# code (which a host build compiles out), the build with the root Cargo.toml's `firmware` profile
# (size-optimized, LTO, panic = "abort"), then scripts/ci/firmware_size.py, which prints the loaded
# sections, .text/.rodata/.data/.bss and the flash and RAM totals against
# firmware/link/size-budget.toml, with the core::fmt, 64-bit division and memmove routines linked
# in, and fails over budget. CI never runs the image: .github/workflows/qemu.yml does, nightly.
#
# The targets need only `rustup target add`: they link with the rust-lld that rustc ships.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

package=oce-firmware-link
default_targets=(thumbv7em-none-eabihf riscv32imac-unknown-none-elf)
images=(blob-frames)
budget=firmware/link/size-budget.toml

suggest=false
if [ "${1:-}" = --suggest ]; then suggest=true; shift; fi
if [ $# -gt 0 ]; then targets=("$@"); else targets=("${default_targets[@]}"); fi
for triple in "${targets[@]}"; do
  case $triple in -* | '') echo "usage: $0 [--suggest] [<target triple>...]" >&2; exit 2 ;; esac
done

if command -v rustup >/dev/null; then
  installed=$(rustup target list --installed)
  for triple in "${targets[@]}"; do
    grep -qxF "$triple" <<<"$installed" ||
      { echo "error: target $triple is not installed; rustup target add $triple" >&2; exit 1; }
  done
fi

target_dir=$(cargo metadata --format-version 1 --no-deps --locked |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')

status=0
for triple in "${targets[@]}"; do
  if "$suggest"; then
    cargo build -q -p "$package" --locked --target "$triple" --profile firmware
  else
    echo "==> $package ($triple)"
    cargo tree --locked -p "$package" --target "$triple" -e normal,no-proc-macro --prefix none \
      --format '{p} [{f}]' | bash scripts/ci/check-no-std.sh scan "$package ($triple)" || status=1
    cargo clippy -p "$package" --locked --target "$triple" -- -D warnings || status=1
    cargo build -p "$package" --locked --target "$triple" --profile firmware || status=1
  fi
  for image in "${images[@]}"; do
    elf=$target_dir/$triple/firmware/$image
    if "$suggest"; then
      python3 -I scripts/ci/firmware_size.py --suggest "$elf" "$triple.$image"
      echo
    else
      echo "--> $image ($triple)"
      python3 -I scripts/ci/firmware_size.py "$elf" "$budget" "$triple.$image" || status=1
    fi
  done
done
[ "$status" -eq 0 ] && ! "$suggest" && echo "OK: $package links for ${targets[*]} within $budget."
exit "$status"
