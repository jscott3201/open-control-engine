#!/usr/bin/env bash
# The five gate modes of .agents/gate.sh nest (issues #545 and #556): docs, leverage, light, determinism,
# full, each running every step of the one before it and at least one more. The local proof rests
# on that: a row is satisfied by any heavier mode, and scripts/ci/proof-row.sh only ever names the
# lightest sufficient one.
#
# The real script is executed in each mode, in an empty directory, with every expensive child
# replaced by a stub that succeeds or fails at once (cargo, bash, python3, env, git, rustup, and
# the mdBook binary the docs steps run by path). Only `step` labels are read, so this proves which
# steps a mode REACHES, not that they pass. Four mutated copies prove the check can fail: a
# `light` that runs the release-codegen steps, a `docs` that runs the compiled lanes, and a `full`
# whose workspace suite is unreachable (the whole block, and the release-codegen step alone).

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

GATE=".agents/gate.sh"
MODES=(docs leverage light determinism full)
failures=0

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/stubs.sh" <<'STUBS'
cargo() { return 0; }
bash() { return 0; }
python3() { return 0; }
env() { return 0; }
git() { return 0; }
rustup() { return 0; }
STUBS

# labels <gate script> <mode>: the step labels that mode reaches, one per line, sorted.
labels() {
  local script="$1" mode="$2" root="$tmp/run"
  rm -rf "$root"
  mkdir -p "$root/.agents" "$root/target/mdbook-bin"
  cp "$script" "$root/.agents/gate.sh"
  printf '#!/bin/sh\nexit 0\n' > "$root/target/mdbook-bin/mdbook"
  chmod +x "$root/target/mdbook-bin/mdbook"
  (cd "$root" && BASH_ENV="$tmp/stubs.sh" command bash .agents/gate.sh "$mode" 2>&1 || true) \
    | sed -e $'s/\033\\[[0-9;]*m//g' -n -e 's/^── //p' | sort -u
}

has() { grep -qxF -- "$2" <<< "$1"; }

# chain_defects <gate script>: one line per way the modes fail to nest.
chain_defects() {
  local script="$1" mode previous="" previous_labels="" current missing
  local docs leverage light determinism full
  for mode in "${MODES[@]}"; do
    current="$(labels "$script" "$mode")"
    [ -n "$current" ] || echo "mode $mode reached no step"
    printf -v "$mode" '%s' "$current"
    if [ -n "$previous" ]; then
      missing="$(comm -23 <(printf '%s\n' "$previous_labels") <(printf '%s\n' "$current"))"
      [ -z "$missing" ] || echo "mode $mode does not run a step of $previous: $(tr '\n' ';' <<< "$missing")"
      [ "$current" != "$previous_labels" ] || echo "mode $mode runs nothing that $previous does not"
    fi
    previous="$mode"
    previous_labels="$current"
  done
  # Each boundary, by the step that defines it.
  has "$docs" 'generated internal links and anchors' || echo 'docs does not run the documentation checks'
  has "$docs" 'workflow gate fixtures' || echo 'docs does not run the gate fixtures'
  ! has "$docs" 'leverage lane' || echo 'docs runs the leverage lane'
  ! has "$docs" 'README Quickstart byte-pin' || echo 'docs runs the README lane'
  has "$leverage" 'leverage lane' || echo 'leverage does not run the leverage lane'
  has "$leverage" 'README Quickstart byte-pin' || echo 'leverage does not run the README lane'
  ! has "$leverage" 'determinism subset' || echo 'leverage runs the engine lane'
  ! has "$leverage" 'clippy (default — no DB)' || echo 'leverage runs the engine lane'
  has "$light" 'determinism subset' || echo 'light does not run the determinism subset'
  has "$light" 'clippy (default — no DB)' || echo 'light does not run clippy'
  ! has "$light" 'determinism subset (release codegen)' || echo 'light runs the release-codegen determinism subset'
  ! has "$light" 'strict-bit corpus and controls (release codegen)' || echo 'light runs the release-codegen strict-bit step'
  has "$determinism" 'determinism subset (release codegen)' || echo 'determinism does not run the release-codegen determinism subset'
  has "$determinism" 'strict-bit corpus and controls (release codegen)' || echo 'determinism does not run the release-codegen strict-bit step'
  ! has "$determinism" 'nextest — workspace' || echo 'determinism runs the workspace suite'
  ! has "$determinism" 'nextest — workspace (release codegen)' || echo 'determinism runs the release-codegen workspace suite'
  has "$full" 'nextest — workspace' || echo 'full does not run the workspace suite'
  has "$full" 'nextest — workspace (release codegen)' || echo 'full does not run the release-codegen workspace suite'
  has "$full" 'doctests' || echo 'full does not run the doctests'
  has "$full" 'blob round trip, the recording replayed on wasm32-wasip1 under wasmtime' || echo 'full does not run the blob round trip'
}

defects="$(chain_defects "$GATE")"
if [ -n "$defects" ]; then
  while IFS= read -r defect; do
    echo "FAIL: $GATE: $defect"
    failures=$((failures + 1))
  done <<< "$defects"
fi

# mutant <name> <expected defect> <sed expression>: the mutated script must show that defect.
mutant() {
  local name="$1" expected="$2" expression="$3" found
  sed "$expression" "$GATE" > "$tmp/mutant.sh"
  if cmp -s "$GATE" "$tmp/mutant.sh"; then
    echo "FAIL: mutant '$name' did not change the gate script; its pattern is stale"
    failures=$((failures + 1))
    return
  fi
  found="$(chain_defects "$tmp/mutant.sh")"
  if ! grep -qF -- "$expected" <<< "$found"; then
    echo "FAIL: mutant '$name' was not caught (expected: $expected)"
    printf '%s\n' "$found" | sed 's/^/  /'
    failures=$((failures + 1))
  fi
}

mutant light-runs-release-codegen 'light runs the release-codegen determinism subset' \
  's/^release_codegen() { .*$/release_codegen() { engine_lane; }/'
mutant docs-runs-compiled-lanes 'docs runs the leverage lane' \
  's/^compiled_lanes() { .*$/compiled_lanes() { true; }/'
mutant full-workspace-suite-unreachable 'full does not run the release-codegen workspace suite' \
  's/^if \[ "\$MODE" = full \]; then$/if [ "$MODE" = full ] \&\& false; then/'

mutant full-release-suite-short-circuited 'full does not run the release-codegen workspace suite' \
  "s/^  step 'nextest — workspace (release codegen)' /  false \&\& step 'nextest — workspace (release codegen)' /"

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures gate-mode fixture(s) failed"
  exit 1
fi
echo "OK: gate modes nest (docs, leverage, light, determinism, full), and four mutants are caught."
