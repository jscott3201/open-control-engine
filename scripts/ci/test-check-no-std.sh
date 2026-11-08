#!/usr/bin/env bash
# Controls for check-no-std.sh (issue #402). A check that cannot fail is not a gate, so each half
# of the script is made to fail on purpose:
#
#   1. graph scan — synthetic `cargo tree` graphs through `check-no-std.sh scan`: a clean graph
#      passes; a `std` feature anywhere in a package's feature list fails, and a host-only crate
#      fails; a feature or crate that merely starts with those names (`stdio`, `serde_json_path`)
#      does not; an empty graph, or one without the root crate's own line, fails.
#   2. extern crate std — a copy of the workspace with `extern crate std;` in oce-graph's lib.rs
#      must fail the script on thumbv7em-none-eabihf, at the build, because that target has no std
#      (the acceptance control the issue names).
#   3. std that compiles — a copy whose oce-model turns `serde/std` on from its `alloc` feature must
#      fail on wasm32-unknown-unknown, where std exists and the build and clippy both pass: only
#      the graph scan can catch it, and it must name serde.
#
# The copies hold what the builds read: the workspace manifests and lockfile, the toolchain and
# clippy configuration, the crates, the firmware members (Cargo.lock lists them) and the reference
# catalog oce-blocks includes. They build into this checkout's target directory, as
# test-math-lint.sh's copy does. Needs the thumbv7em-none-eabihf and wasm32-unknown-unknown targets.
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

CHECK="scripts/ci/check-no-std.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
failures=0
fail() {
  echo "FAIL: $*"
  failures=$((failures + 1))
}

# ── 1. graph scan ────────────────────────────────────────────────────────────
# <name> <pass|fail> <graph>
scan_case() {
  local name=$1 want=$2 graph=$3 got
  if printf '%s\n' "$graph" | bash "$CHECK" scan oce-graph "$name" >/dev/null 2>&1; then
    got=pass
  else
    got=fail
  fi
  [ "$got" = "$want" ] || fail "scan case '$name': want $want, got $got"
}
clean=$'oce-graph v0.3.0 (crates/oce-graph) [alloc]\noce-model v0.3.0 (crates/oce-model) [alloc]\nlibm v0.2.16 []'
scan_case clean pass "$clean"
scan_case std-alone fail "$clean"$'\nthiserror v2.0.18 [std]'
scan_case std-first fail "$clean"$'\nserde v1.0.228 [std,alloc,derive]'
scan_case std-middle fail "$clean"$'\nserde v1.0.228 [alloc,std,derive]'
scan_case std-last fail "$clean"$'\nserde_core v1.0.228 [alloc,result,std]'
scan_case std-on-a-member fail $'oce-graph v0.3.0 (crates/oce-graph) [alloc,default,std]'
scan_case host-only-json fail "$clean"$'\nserde_json v1.0.140 [alloc]'
scan_case host-only-facade fail "$clean"$'\noce-api v0.3.0 (crates/oce-api) []'
scan_case std-prefixed-feature pass "$clean"$'\nsemihosting v0.1.25 [stdio]'
scan_case std-suffixed-feature pass "$clean"$'\nportable-atomic v1.15.0 [require-cas,nostd]'
scan_case host-only-prefixed-crate pass "$clean"$'\nserde_json_path v0.7.0 []'
scan_case empty-graph fail ''
scan_case another-crates-graph fail $'oce-model v0.3.0 (crates/oce-model) [alloc]\nlibm v0.2.16 []'
if bash "$CHECK" scan oce-graph </dev/null >/dev/null 2>&1; then
  fail "scan without a label must be a usage error"
fi

# ── 2 and 3. workspace copies ────────────────────────────────────────────────
copy() {
  local dest=$1
  mkdir -p "$dest/tools" "$dest/scripts/ci"
  cp Cargo.toml Cargo.lock clippy.toml rust-toolchain.toml "$dest/"
  cp -R crates "$dest/crates"
  cp -R firmware "$dest/firmware"
  cp -R tools/reference-catalog "$dest/tools/reference-catalog"
  cp "$CHECK" "$dest/scripts/ci/"
  # check-no-std.sh finds its root with git; the copy is its own repository.
  git -C "$dest" init -q
}

# Run the copy's script on <target>; print its output and return its status.
run_copy() {
  local dest=$1 target=$2
  (cd "$dest" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}" bash scripts/ci/check-no-std.sh "$target" 2>&1)
}

STD_CRATE="$WORK/extern-crate-std"
copy "$STD_CRATE"
lib="$STD_CRATE/crates/oce-graph/src/lib.rs"
grep -qx 'extern crate alloc;' "$lib" || fail "extern crate std: oce-graph's lib.rs no longer has 'extern crate alloc;'; update this control"
python3 - "$lib" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
open(path, "w").write(text.replace("\nextern crate alloc;\n", "\nextern crate alloc;\nextern crate std;\n", 1))
PY
grep -qx 'extern crate std;' "$lib" || fail "extern crate std: the edit to oce-graph's lib.rs did not apply"
set +e
output="$(run_copy "$STD_CRATE" thumbv7em-none-eabihf)"
status=$?
set -e
if [ "$status" -eq 0 ]; then
  fail "extern crate std: check-no-std.sh passed with 'extern crate std;' in oce-graph"
elif grep -Fq "can't find crate for \`std\`" <<<"$output"; then
  echo "control: 'extern crate std;' in oce-graph fails check-no-std.sh on thumbv7em-none-eabihf, as required"
else
  fail "extern crate std: the script failed, but not because std is missing:
$output"
fi

STD_FEATURE="$WORK/serde-std"
copy "$STD_FEATURE"
manifest="$STD_FEATURE/crates/oce-model/Cargo.toml"
grep -qx 'alloc = \[\]' "$manifest" || fail "std that compiles: oce-model's manifest no longer has 'alloc = []'; update this control"
python3 - "$manifest" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
open(path, "w").write(text.replace("\nalloc = []\n", '\nalloc = ["serde/std"]\n', 1))
PY
grep -qx 'alloc = \["serde/std"\]' "$manifest" || fail "std that compiles: the edit to oce-model's manifest did not apply"
set +e
output="$(run_copy "$STD_FEATURE" wasm32-unknown-unknown)"
status=$?
set -e
if [ "$status" -eq 0 ]; then
  fail "std that compiles: check-no-std.sh passed with serde's std feature on in oce-model"
elif grep -Eq '^error(\[E[0-9]+\])?: ' <<<"$output" && ! grep -q 'graph has std in it' <<<"$output"; then
  fail "std that compiles: the build failed, so this control no longer isolates the graph scan:
$output"
elif grep -Fq 'error: the oce-model (features: alloc; wasm32-unknown-unknown) graph has std in it' <<<"$output" &&
  grep -Eq '^  serde v[^ ]+ \[([^]]*,)?std(,[^]]*)?\]' <<<"$output"; then
  echo "control: serde/std turned on through oce-model fails the graph scan on wasm32-unknown-unknown, naming serde, as required"
else
  fail "std that compiles: the script did not report serde's std feature in oce-model's graph:
$output"
fi

if [ "$failures" -ne 0 ]; then
  echo "$failures no_std check control(s) failed."
  exit 1
fi
echo "OK: the graph scan's fixtures hold, 'extern crate std;' fails the build, and a std feature that compiles fails the graph scan."
