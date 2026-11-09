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
#                 no core `src/` file other than a `math.rs` names `libm::`, and a core `tests/` or
#                 `benches/` file that names `libm::` is a declared oracle against `std` (it allows
#                 the lint crate-wide, as `libm_std_parity.rs` does) rather than half-and-half.
#   4. mutation — a copy of the workspace with `x.sin()` in oce-blocks' `reals_transcendental.rs`
#                 and `f.sqrt()` in oce-expr's `eval.rs` must fail `cargo clippy` with the
#                 disallowed-method error at both lines.
#
# The lint binds each core crate's `src/` tree, unit tests included. Files under `tests/` and
# `benches/` are separate crates outside it, deliberately: that is where the `std` methods serve as
# the oracle (`crates/oce-blocks/tests/libm_std_parity.rs`).
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
# Outside `src/`, a file may name `libm::` only as a declared oracle that also calls the `std`
# methods it compares against, which is what the crate-level allow states.
for file in $(grep -rlE '(^|[^[:alnum:]_])libm::' "${CORE_CRATES[@]/#/crates/}" --include='*.rs' |
  grep -vE '^crates/[^/]+/src/' || true); do
  grep -qx '#!\[allow(clippy::disallowed_methods)\]' "$file" ||
    fail "scope: $file calls libm directly but is not a declared std oracle (#![allow(clippy::disallowed_methods)]); a test outside src/ calls the std method"
done

# ── 4. mutation ──────────────────────────────────────────────────────────────
# The copy holds what the two library builds read: the workspace manifests and lockfile, the clippy
# and toolchain configuration, the crates, the firmware members (the lockfile lists them), and the
# reference catalog oce-blocks includes. One
# mutation per crate that does float math; one clippy run checks both.
COPY="$WORK/workspace"
mkdir -p "$COPY/tools"
cp Cargo.toml Cargo.lock clippy.toml rust-toolchain.toml "$COPY/"
cp -R crates "$COPY/crates"
cp -R firmware "$COPY/firmware"
cp -R tools/reference-catalog "$COPY/tools/reference-catalog"
# file|call it replaces|std method call it becomes|the disallowed path clippy must name
MUTATIONS=(
  'crates/oce-blocks/src/reals_transcendental.rs|math::sin(read_real(inputs, 0))|read_real(inputs, 0).sin()|f64::sin'
  'crates/oce-expr/src/eval.rs|Ok(real(math::sqrt(f)))|Ok(real(f.sqrt()))|f64::sqrt'
)
expected=()
for mutation in "${MUTATIONS[@]}"; do
  IFS='|' read -r file before after method <<< "$mutation"
  line="$(grep -nF "$before" "$COPY/$file" | head -1 | cut -d: -f1)"
  if [ -z "$line" ]; then
    fail "mutation: $file no longer contains '$before'; update this control"
    continue
  fi
  python3 - "$COPY/$file" "$line" "$before" "$after" <<'PY'
import sys
path, line, before, after = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
lines = open(path).read().split("\n")
lines[line - 1] = lines[line - 1].replace(before, after, 1)
open(path, "w").write("\n".join(lines))
PY
  grep -Fq "$after" "$COPY/$file" || fail "mutation: the edit to $file did not apply"
  expected+=("$file:$line|$method")
done
set +e
output="$(cd "$COPY" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}" \
  cargo clippy -p oce-blocks -p oce-expr --lib --locked --quiet -- -D warnings 2>&1)"
status=$?
set -e
if [ "$status" -eq 0 ]; then
  fail "mutation: clippy passed with std float methods in oce-blocks and oce-expr"
fi
for entry in ${expected[@]+"${expected[@]}"}; do
  location="${entry%%|*}" method="${entry##*|}"
  if grep -Fq "use of a disallowed method \`$method\`" <<< "$output" &&
    grep -Fq "$location:" <<< "$output"; then
    echo "mutation control: clippy refused $method at $location, as required"
  else
    fail "mutation: clippy did not report the disallowed $method at $location:
$output"
  fi
done

if [ "$failures" -ne 0 ]; then
  echo "$failures math-discipline check(s) failed."
  exit 1
fi
echo "OK: ${#CORE_METHODS[@]} core and ${#STD_METHODS[@]} std-only f64 methods probed, the clippy list is total, the lint is scoped to the core four, and the oce-blocks and oce-expr mutations fail clippy."
