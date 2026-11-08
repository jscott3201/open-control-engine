#!/usr/bin/env bash
# The `no_std + alloc` core on targets without `std` (issue #402): build and clippy for the core
# four, then a check of each one's dependency graph. Adapted from rusty-bacnet's
# scripts/ci/check-no-std.sh (same owner), which has the same two halves.
#
#   check-no-std.sh                 the default targets below
#   check-no-std.sh <triple>...     those targets; cargo builds them side by side, in one run
#   check-no-std.sh scan <root> <label>
#                                   the graph check alone, on a `cargo tree` of crate <root> read
#                                   from stdin (what test-check-no-std.sh drives with synthetic
#                                   graphs, and check-firmware-link.sh with the image's graph)
#
# A host build can't prove a crate is no_std: the host always has std, so a stray `std::` path or a
# dependency that turns on its std feature still compiles. thumbv7em-none-eabihf (Cortex-M4F) and
# riscv32imac-unknown-none-elf (RV32IMAC) have no std at all, so the build fails there.
# wasm32-unknown-unknown has a std, but one whose I/O and threads are stubs; the core builds for
# it because a browser or WASM host links it. All four crates build with
# `--no-default-features --features alloc` (their `default` is `std`), lib only: tests and their
# dev-dependencies need std. Clippy lints the same configuration with `-D warnings`, which is
# where the core's `clippy.toml` math and hash-collection rules bind (issues #398, #399).
#
# The graph check reads `cargo tree -e normal,no-proc-macro` for each crate on each target (proc
# macros run on the build host, never on the target) with each package's enabled features. No
# package may have a `std` feature on, and none may be one of `host_only` below. That names the
# package when std leaks in (through a workspace entry with default features, say), and it catches
# a leak that still compiles, as one on wasm32-unknown-unknown does, where std exists.
#
# Each target needs only `rustup target add`. This script installs nothing; it fails, naming the
# command, when a target is missing.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

crates=(oce-model oce-expr oce-blocks oce-graph)
features=alloc
default_targets=(thumbv7em-none-eabihf riscv32imac-unknown-none-elf wasm32-unknown-unknown)
# Packages a no_std build's graph must never hold: the workspace's std-only crates (the facade, the
# CXF loader, validation, diagnostics, semantics, the store ports, the conformance suite and the
# bless helper), the std-only JSON stack, the OS-call crate, and std itself.
host_only=(oce-api oce-cxf oce-validate oce-diag oce-semantics oce-store oce-conformance oce-bless
  serde_json rustix std)

# Fails if the `{p} [{f}]` graph of crate <root> on stdin turns on a std feature or holds a
# host-only crate, naming each offending package under <label>. A graph without <root>'s own line
# (empty, truncated, or another crate's) fails too: a scan of nothing must not pass.
scan() {
  local root=$1 label=$2 host_re bad tree
  tree=$(cat)
  if ! grep -q "^$root v" <<<"$tree"; then
    echo "error: the $label graph has no line for $root itself; cargo tree printed nothing usable" >&2
    return 1
  fi
  host_re=$(IFS='|'; echo "${host_only[*]}")
  bad=$(grep -E "\[([^]]*,)?std(,[^]]*)?\]|^($host_re) v" <<<"$tree" || true)
  if [ -n "$bad" ]; then
    echo "error: the $label graph has std in it:" >&2
    sort -u <<<"$bad" | sed 's/^/  /' >&2
    return 1
  fi
}

# One line per package of <cargo tree args...>'s graph: `name vX.Y.Z [feature,...]`.
graph() {
  cargo tree --locked "$@" -e normal,no-proc-macro --prefix none --format '{p} [{f}]'
}

if [ "${1:-}" = scan ]; then
  [ $# -eq 3 ] || { echo "usage: $0 scan <root crate> <label> < cargo-tree-output" >&2; exit 2; }
  scan "$2" "$3"
  exit
fi

if [ $# -gt 0 ]; then targets=("$@"); else targets=("${default_targets[@]}"); fi
for triple in "${targets[@]}"; do
  case $triple in -* | '') echo "usage: $0 [<target triple>...] | scan <root> <label>" >&2; exit 2 ;; esac
done

if command -v rustup >/dev/null; then
  installed=$(rustup target list --installed)
  for triple in "${targets[@]}"; do
    grep -qxF "$triple" <<<"$installed" ||
      { echo "error: target $triple is not installed; rustup target add $triple" >&2; exit 1; }
  done
fi

args=(--no-default-features --features "$features" --locked)
for crate in "${crates[@]}"; do args+=(-p "$crate"); done
for triple in "${targets[@]}"; do args+=(--target "$triple"); done

status=0
echo "==> build: ${crates[*]} (features: $features; ${targets[*]})"
cargo build "${args[@]}" || status=1
echo "==> clippy: ${crates[*]} (features: $features; ${targets[*]})"
cargo clippy "${args[@]}" -- -D warnings || status=1
for crate in "${crates[@]}"; do
  for triple in "${targets[@]}"; do
    echo "==> graph: $crate (features: $features; $triple)"
    graph -p "$crate" --no-default-features --features "$features" --target "$triple" |
      scan "$crate" "$crate (features: $features; $triple)" || status=1
  done
done
[ "$status" -eq 0 ] && echo "OK: ${crates[*]} build and lint no_std + $features on ${targets[*]}; no std in any graph."
exit "$status"
