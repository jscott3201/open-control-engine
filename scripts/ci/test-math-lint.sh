#!/usr/bin/env bash
# The math-discipline controls (issue #399). The core four (oce-model, oce-expr, oce-blocks,
# oce-graph) build `no_std + alloc`, so every `f64` operation `core` lacks goes through the crate's
# `math` module (the pinned `libm`), and `clippy.toml` lists the `std`-only float methods that the
# four crate roots deny. A lint that cannot fail is not a gate; this script makes each part of the
# rule fail on purpose or check itself against the toolchain:
#
#   1. probe    — a `no_std` build for thumbv7em-none-eabihf, one method per crate, sorts every
#                 `f64` method into `core` or `std`-only; the result must equal the tables below,
#                 which TESTING.md "Conventions" and the `math` module docs restate. A toolchain
#                 bump that moves a method into `core` fails here until the tables are updated.
#   2. list     — every `std`-only method is in `clippy.toml` for both `f64` and `f32`.
#   3. scope    — each core crate root denies `clippy::disallowed_methods`, the workspace allows it,
#                 and no core `src/` file other than a `math.rs` names `libm::`.
#   4. mutation — a copy of the workspace with `x.sin()` in `reals_transcendental.rs` must fail
#                 `cargo clippy -p oce-blocks` with the disallowed-method error at that line.
#
# Needs the pinned toolchain with clippy and the thumbv7em-none-eabihf target
# (`rustup target add thumbv7em-none-eabihf`); like gate.sh it installs nothing, and a missing
# target fails the probe rather than skipping it.

set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

CORE_CRATES=(oce-model oce-expr oce-blocks oce-graph)

# Method calls as the probe writes them; the name is the text before `(`.
CORE_METHODS=(
  'abs()' 'signum()' 'copysign(1.0)' 'min(1.0)' 'max(1.0)' 'clamp(0.0, 1.0)' 'recip()'
  'to_degrees()' 'to_radians()' 'total_cmp(&1.0)' 'midpoint(1.0)'
)
STD_METHODS=(
  'floor()' 'ceil()' 'round()' 'round_ties_even()' 'trunc()' 'fract()' 'mul_add(1.0, 1.0)'
  'div_euclid(2.0)' 'rem_euclid(2.0)' 'powi(2)' 'powf(2.0)' 'sqrt()' 'cbrt()' 'hypot(1.0)'
  'exp()' 'exp2()' 'exp_m1()' 'ln()' 'ln_1p()' 'log(2.0)' 'log2()' 'log10()'
  'sin()' 'cos()' 'tan()' 'sin_cos()' 'asin()' 'acos()' 'atan()' 'atan2(1.0)'
  'sinh()' 'cosh()' 'tanh()' 'asinh()' 'acosh()' 'atanh()' 'abs_sub(1.0)'
)

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
failures=0
fail() {
  echo "FAIL: $*"
  failures=$((failures + 1))
}

# ── 1. probe ─────────────────────────────────────────────────────────────────
# rustc runs from the repository root so rust-toolchain.toml selects the pinned toolchain.
mkdir -p "$WORK/probe/out"
probe() {
  local call="$1" name="${1%%(*}" src
  src="$WORK/probe/$name.rs"
  printf '#![no_std]\n#![allow(deprecated)]\npub fn probe(x: f64) { let _ = x.%s; }\n' \
    "$call" > "$src"
  rustc --edition 2024 --crate-type lib --target thumbv7em-none-eabihf --emit metadata \
    --out-dir "$WORK/probe/out" "$src" > "$WORK/probe/$name.log" 2>&1
}
if ! probe 'abs()'; then
  cat "$WORK/probe/abs.log"
  echo "FAIL: the no_std probe cannot build for thumbv7em-none-eabihf (is the target installed?)"
  exit 1
fi
for call in "${CORE_METHODS[@]}"; do
  probe "$call" || fail "probe: f64::${call%%(*} is listed as core but does not build without std"
done
for call in "${STD_METHODS[@]}"; do
  name="${call%%(*}"
  if probe "$call"; then
    fail "probe: f64::$name now builds without std: move it to the core table here, in TESTING.md and in the math module docs"
  elif ! grep -q 'E0599' "$WORK/probe/$name.log"; then
    fail "probe: f64::$name failed without std for a reason other than a missing method: $(head -3 "$WORK/probe/$name.log")"
  fi
done

# ── 2. list ──────────────────────────────────────────────────────────────────
for call in "${STD_METHODS[@]}"; do
  for ty in f64 f32; do
    grep -Fq "path = \"$ty::${call%%(*}\"" clippy.toml ||
      fail "list: clippy.toml disallowed-methods lacks $ty::${call%%(*}"
  done
done

# ── 3. scope ─────────────────────────────────────────────────────────────────
for crate in "${CORE_CRATES[@]}"; do
  grep -qx '#!\[deny(clippy::disallowed_methods)\]' "crates/$crate/src/lib.rs" ||
    fail "scope: crates/$crate/src/lib.rs does not deny clippy::disallowed_methods"
done
grep -qx 'disallowed_methods = "allow"' Cargo.toml ||
  fail 'scope: [workspace.lints.clippy] does not allow disallowed_methods outside the core four'
stray="$(grep -rnE '(^|[^[:alnum:]_])libm::' "${CORE_CRATES[@]/#/crates/}" --include='*.rs' |
  grep -vE '^crates/[^/]+/src/math\.rs:' | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' |
  grep -E '^crates/[^/]+/src/' || true)"
[ -z "$stray" ] || fail "scope: core src/ calls libm outside a math module:
$stray"

# ── 4. mutation ──────────────────────────────────────────────────────────────
# The copy holds what the oce-blocks library build reads: the workspace manifests and lockfile,
# the clippy and toolchain configuration, the crates, and the reference catalog it includes.
COPY="$WORK/workspace"
mkdir -p "$COPY/tools"
cp Cargo.toml Cargo.lock clippy.toml rust-toolchain.toml "$COPY/"
cp -R crates "$COPY/crates"
cp -R tools/reference-catalog "$COPY/tools/reference-catalog"
target="$COPY/crates/oce-blocks/src/reals_transcendental.rs"
before='emit_real(0, math::sin(read_real(inputs, 0)), emit);'
after='emit_real(0, read_real(inputs, 0).sin(), emit);'
if ! grep -Fq "$before" "$target"; then
  fail "mutation: the Sin block no longer calls '$before'; update this control"
else
  line="$(grep -nF "$before" "$target" | head -1 | cut -d: -f1)"
  sed -i.orig "${line}s/math::sin(read_real(inputs, 0))/read_real(inputs, 0).sin()/" "$target"
  grep -Fq "$after" "$target" || fail "mutation: the sed edit did not apply"
  set +e
  output="$(cd "$COPY" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}" \
    cargo clippy -p oce-blocks --lib --locked --quiet -- -D warnings 2>&1)"
  status=$?
  set -e
  if [ "$status" -eq 0 ]; then
    fail "mutation: clippy passed with x.sin() in reals_transcendental.rs"
  elif ! grep -Fq 'use of a disallowed method `f64::sin`' <<< "$output" ||
    ! grep -Fq "src/reals_transcendental.rs:$line:" <<< "$output"; then
    fail "mutation: clippy failed without the disallowed-method error at line $line:
$output"
  else
    echo "mutation control: clippy refused x.sin() at reals_transcendental.rs:$line, as required"
  fi
fi

if [ "$failures" -ne 0 ]; then
  echo "$failures math-discipline check(s) failed."
  exit 1
fi
echo "OK: ${#CORE_METHODS[@]} core and ${#STD_METHODS[@]} std-only f64 methods probed, the clippy list is total, the lint is scoped to the core four, and the x.sin() mutation fails clippy."
