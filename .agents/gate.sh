#!/usr/bin/env bash
#
# The gate. This file is the single runnable source of truth for what must pass
# before a change lands; every other document points here rather than restating
# commands. Nine divergent copies of this list existed before it was written,
# two of them materially weaker than CI, so prose copies are the failure mode
# this file exists to remove.
#
#   bash .agents/gate.sh          # light    — mirrors the per-PR gate (pr-gate.yml), and runs
#                                 #            the determinism subset under release codegen as
#                                 #            well, which CI runs only after the merge
#   bash .agents/gate.sh full     # full     — adds the suite (release-gate.yml)
#   bash .agents/gate.sh leverage # leverage — mirrors a PR confined to crates/leverage/: the
#                                 #            always-on jobs plus the leverage lane, no engine lane
#   bash .agents/gate.sh nostd    # nostd    — the bare-metal cell alone (pr-gate.yml's
#                                 #            `no-std-build` job): the no_std core, the firmware
#                                 #            image's build and size budget; light and full run
#                                 #            it too
#   bash .agents/gate.sh qemu     # qemu     — the emulation check (qemu.yml, nightly): the firmware
#                                 #            images under QEMU against the host reference; needs
#                                 #            qemu-system-arm and qemu-system-riscv32 on PATH
#   bash .agents/gate.sh roundtrip # roundtrip — the blob round trip (pr-gate.yml's determinism
#                                 #            cells, determinism-compare and blob-round-trip-wasm):
#                                 #            record on the host, then replay the recording on the
#                                 #            host and on wasm32-wasip1 under wasmtime; needs the
#                                 #            wasm32-wasip1 target and the pinned wasmtime
#                                 #            (scripts/ci/install-wasmtime.sh); full runs it too
#
# Commands are kept in CI's exact form, flags included. `--locked`, the `ci`
# nextest profile, `--no-tests=fail`, and `RUSTDOCFLAGS` are each load-bearing:
# dropping any one of them turns a real gate into a green light that proves
# less than it appears to. Change a command here only by changing CI first.
#
# CI executes this script as well as mirroring it: `gate (full)` in release-gate.yml, on
# every merge into `development` and daily. No per-PR job executes it (pr-gate.yml's
# `gate (light)` was retired by issue #502), so CI first runs an edit to this script, a CI
# or repository script, a workflow or tool config on its merge commit. Before the merge the
# author's own `bash .agents/gate.sh full` on the final head is the only run there is.
#
# Agents: this script is executable but not writable inside a Codex lane
# sandbox. That is deliberate. A failing gate is fixed at its cause, never by
# editing this file.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

MODE="${1:-light}"
case "$MODE" in
  light | full | leverage | nostd | qemu | roundtrip) ;;
  *)
    echo "usage: bash .agents/gate.sh [light|full|leverage|nostd|qemu|roundtrip]" >&2
    exit 2
    ;;
esac

failures=0
declare -a failed_steps=()

# Run one gate step, echoing the command first. A step never aborts the run —
# a partial gate report is less useful than a complete one, and an agent that
# stops at the first red learns one problem per round trip instead of all of
# them.
step() {
  local label="$1"
  shift
  printf '\n\033[1m── %s\033[0m\n$ %s\n' "$label" "$*"
  if "$@"; then
    printf '\033[32mok\033[0m — %s\n' "$label"
  else
    printf '\033[31mFAIL\033[0m — %s\n' "$label"
    failures=$((failures + 1))
    failed_steps+=("$label")
  fi
}

# Same, for steps needing an environment variable set only for that command.
# Write the blob artifact exactly as a determinism cell does, then check its size.
blob_matrix_artifact() {
  rm -f target/blobs-gate.bin
  OCE_BLOB_OUT=target/blobs-gate.bin cargo nextest run -p oce-api --locked --profile ci \
    --no-tests=fail -E 'binary(blob_goldens)' &&
    test -s target/blobs-gate.bin &&
    test "$(wc -c < target/blobs-gate.bin | tr -d ' ')" = 123329
}

step_env() {
  local label="$1" var="$2" val="$3"
  shift 3
  printf '\n\033[1m── %s\033[0m\n$ %s=%q %s\n' "$label" "$var" "$val" "$*"
  if env "$var=$val" "$@"; then
    printf '\033[32mok\033[0m — %s\n' "$label"
  else
    printf '\033[31mFAIL\033[0m — %s\n' "$label"
    failures=$((failures + 1))
    failed_steps+=("$label")
  fi
}

printf '\033[1mopen-control gate — mode: %s\033[0m\n' "$MODE"

# ── The bare-metal cell (pr-gate.yml's `no-std-build` job) ──────────────────
# The core four (oce-model, oce-expr, oce-blocks, oce-graph) build without `std` (issues #397,
# #402), and the firmware image links them for a bare-metal target. This script installs nothing:
# check-no-std.sh and check-firmware-link.sh fail, naming the `rustup target add <target>` command,
# when a target is missing. A missing target is a failure, never a skip.
bare_metal_cell() {
  # Build, clippy and the `cargo tree` std-feature scan on thumbv7em-none-eabihf,
  # riscv32imac-unknown-none-elf and wasm32-unknown-unknown, then the controls that prove both
  # halves can fail.
  step 'no_std + alloc core (build, clippy, graph scan)' bash scripts/ci/check-no-std.sh
  step 'no_std check controls' bash scripts/ci/test-check-no-std.sh
  # The math rule (issue #399): the core probe, the clippy.toml list and scope, and the
  # std-float-method mutations in oce-blocks and oce-expr.
  step 'math-discipline controls' bash scripts/ci/test-math-lint.sh
  # Without `std` the evaluation-fuel meter is program-wide rather than thread-local, so the suite
  # runs on one test thread.
  step 'oce-expr unit suite without std' \
    cargo test -p oce-expr --no-default-features --features alloc --lib --locked -- --test-threads=1
  # The blob execution entry (issue #401): its no_std closure holds no oce-cxf, oce-api, serde_json
  # or std, and its suites run against the no_std + alloc build of oce-graph (zero frame
  # allocations, the facade's run digests, raw state words).
  step 'blob entry closure (no_std)' bash scripts/ci/check-no-std-entry-closure.sh
  step 'blob entry suites against the no_std build' \
    cargo test -p oce-graph --no-default-features --features alloc --test blob_execution \
      --test blob_state_words --locked -- --nocapture
  # The firmware image (issue #402): graph scan, clippy, the `firmware`-profile build for both
  # bare-metal targets, the size budget, the size checker's own controls, and the image's run on
  # the host.
  step 'firmware image (build and size budget)' bash scripts/ci/check-firmware-link.sh
  step 'firmware size-check controls' python3 -I scripts/ci/test-firmware-size.py
  step 'firmware run on the host' cargo test -p oce-firmware-link --locked
}

# ── The blob round trip (issue #403) ─────────────────────────────────────────
# Three legs from one recording, as CI runs them: the suite records the 46 fixtures' blobs and
# expected outputs, warnings and raw state words on this host (comparing the facade with the no_std
# entry as it goes), then replays that recording here and on wasm32-wasip1 under wasmtime. CI's
# second native architecture cannot run on one machine. A missing target or wasmtime is a failure
# naming its install command, never a skip.
ROUND_TRIP_DIR=target/round-trip
round_trip_record() {
  rm -rf "$ROUND_TRIP_DIR"
  OCE_ROUND_TRIP_OUT="$ROUND_TRIP_DIR" cargo nextest run -p oce-conformance --test blob_round_trip \
    --locked --profile ci --no-tests=fail &&
    test "$(ls "$ROUND_TRIP_DIR" | wc -l | tr -d ' ')" = 92
}
round_trip_replay_host() {
  test -d "$ROUND_TRIP_DIR" &&
    OCE_ROUND_TRIP_EXPECT="$ROUND_TRIP_DIR" cargo nextest run -p oce-conformance \
      --test blob_round_trip --locked --profile ci --no-tests=fail
}
round_trip_replay_wasm() {
  local tools="$PWD/target/wasmtime-bin"
  if ! rustup target list --installed | grep -qx wasm32-wasip1; then
    echo 'the wasm32-wasip1 target is not installed: rustup target add wasm32-wasip1' >&2
    return 1
  fi
  if ! PATH="$tools:$PATH" command -v wasmtime >/dev/null 2>&1; then
    echo 'wasmtime is not installed: bash scripts/ci/install-wasmtime.sh (the pinned release, into target/wasmtime-bin)' >&2
    return 1
  fi
  test -d "$ROUND_TRIP_DIR" &&
    PATH="$tools:$PATH" wasmtime --version &&
    PATH="$tools:$PATH" OCE_ROUND_TRIP_EXPECT="$ROUND_TRIP_DIR" cargo nextest run \
      -p oce-conformance --test blob_round_trip --target wasm32-wasip1 --locked --profile ci \
      --no-tests=fail
}
blob_round_trip() {
  step 'blob round trip, recorded on the host' round_trip_record
  step 'blob round trip, the recording replayed on the host' round_trip_replay_host
  step 'blob round trip, the recording replayed on wasm32-wasip1 under wasmtime' round_trip_replay_wasm
}

# ── Focused modes: the bare-metal cell, the emulation check or the round trip, alone ──
if [ "$MODE" = nostd ] || [ "$MODE" = qemu ] || [ "$MODE" = roundtrip ]; then
  if [ "$MODE" = nostd ]; then
    bare_metal_cell
  elif [ "$MODE" = roundtrip ]; then
    blob_round_trip
  else
    # qemu.yml's one command: build both images, run them under QEMU (netduinoplus2 for
    # thumbv7em-none-eabihf, riscv32 virt for riscv32imac-unknown-none-elf) with semihosting, and
    # diff each one's lines against the host reference runner's.
    step 'firmware images under QEMU against the host reference' bash scripts/ci/emulate-firmware.sh
  fi
  echo
  echo '────────────────────────────────────────────────────────────────────────────'
  echo "NOT COVERED BY THIS RUN — mode $MODE runs one cell, not the gate:"
  echo '  · everything else in the per-PR gate. Run `bash .agents/gate.sh` for it.'
  if [ "$MODE" = nostd ]; then
    echo '  · executing the images on their targets: `bash .agents/gate.sh qemu` (nightly in CI).'
  fi
  if [ "$MODE" = roundtrip ]; then
    echo '  · the aarch64 leg and the comparison of the cells'"'"' recordings: CI only (determinism-compare).'
  fi
  echo '────────────────────────────────────────────────────────────────────────────'
  if [ "$failures" -gt 0 ]; then
    printf '\n\033[31mGATE FAILED\033[0m — %d step(s):\n' "$failures"
    printf '  · %s\n' "${failed_steps[@]}"
    echo
    echo 'Fix the cause. Never weaken a gate to reach green.'
    exit 1
  fi
  printf '\n\033[32mGATE PASSED\033[0m (%s) — all steps green.\n' "$MODE"
  exit 0
fi

# The engine (determinism) lane: everything that compiles the engine. `leverage` mode
# skips it, as pr-gate.yml does for a PR confined to crates/leverage/ and the docs.
engine_lane() { [ "$MODE" != leverage ]; }

# ── Formatting, size, secrets ────────────────────────────────────────────────
step 'fmt' cargo fmt --all --check
step 'file-size cap (700 LOC)' bash .github/scripts/check-file-size.sh
step 'no-secret scan' bash .github/scripts/check-no-secrets.sh

# ── Repository invariant gates ───────────────────────────────────────────────
# The default build linking no database and no async runtime is this repo's
# central promise; the golden-gen firewall stops a generator from blessing its
# own output as the oracle, and the package contract closes release selection.
if engine_lane; then
  step 'default build links no database' bash .github/scripts/check-default-no-db.sh
  step 'golden-gen anti-tautology firewall' bash .github/scripts/check-golden-gen-anti-tautology.sh
  step 'supported facade compiler contract' python3 scripts/facade_contract/check.py
fi
# These three read the documentation as well as the manifests, so they run in every mode.
step 'package, feature, and publication contract' python3 scripts/package_policy/validate.py
step 'authority claim consistency' python3 scripts/authority_claims/check.py --check
step 'product contract traceability' python3 scripts/product_contract/check.py

# ── Gate behavior fixtures ───────────────────────────────────────────────────
# The gates are themselves tested. A gate that cannot fail is not a gate, and
# these fixtures are what keep that from happening silently.
step 'no-db gate fixtures' bash .github/scripts/test-check-default-no-db.sh
step 'golden-gen firewall fixtures' bash .github/scripts/test-check-golden-gen-anti-tautology.sh
step 'package-contract hostile controls' python3 scripts/package_policy/test_validate.py
step 'authority claim hostile controls' python3 scripts/authority_claims/test_check.py
step 'product contract hostile controls' python3 scripts/product_contract/test_check.py
if engine_lane; then
  step 'facade compiler diagnostic controls' python3 scripts/facade_contract/test_check.py
fi
step 'stale crate-status fixtures' bash .github/scripts/test-check-stale-crate-status.sh
step 'stale crate-status smoke' bash .github/scripts/check-stale-crate-status.sh
step 'gate-script coverage' bash .github/scripts/check-gate-script-coverage.sh
step 'gate-script coverage fixtures' bash .github/scripts/test-check-gate-script-coverage.sh
step 'workflow gate fixtures' bash .github/scripts/test-workflow-gates.sh
step 'real workflow smoke' bash .github/scripts/check-workflow-gates.sh
step 'lane classifier fixtures' bash scripts/ci/test-classify-changes.sh
step 'CI OK result-check controls' python3 scripts/ci/test_ci_ok.py
step 'leverage dependency-rule fixtures' bash scripts/ci/test-leverage-members.sh

# ── Leverage lane ────────────────────────────────────────────────────────────
# crates/leverage/*: layout and the oce-api-only dependency rule, then fmt, clippy,
# rustdoc, nextest and cargo-machete per member — the `leverage-lane` job's one
# command (oce-replay is the first member; with none it prints "no leverage crates yet"). pr-gate.yml runs it
# for every engine change too, so it runs in every mode.
step 'leverage lane' bash scripts/ci/leverage-members.sh lane

# ── Engine lane: everything from here to `fi # engine_lane` ──────────────────
# Skipped in `leverage` mode, as pr-gate.yml skips the determinism lane for a PR
# confined to crates/leverage/ and the documentation.
if engine_lane; then

# ── Build, lint, docs ────────────────────────────────────────────────────────
# Clippy lints the DEFAULT feature set, matching ci.yml. Do not add
# `--all-features`: it enables a configuration CI never lints and defeats the
# database-free default this repo gates on.
step 'unused dependency gate' cargo machete
step 'clippy (default — no DB)' cargo clippy --workspace --all-targets --locked -- -D warnings
step 'build (default — no DB)' cargo build --workspace --locked
bare_metal_cell

step_env 'rustdoc — libs' RUSTDOCFLAGS '-D warnings' \
  cargo doc --no-deps --workspace --lib --document-private-items --locked
step_env 'rustdoc — bins' RUSTDOCFLAGS '-D warnings' \
  cargo doc --no-deps --workspace --bins --document-private-items --locked

# CI runs this only when a manifest changed. Running it unconditionally is
# stricter than CI, never weaker, and it is fast. `advisories` is deliberately
# excluded — it needs network and a writable advisory-db, neither of which a
# sandboxed lane has; it runs in advisories.yml instead.
step 'cargo-deny (bans licenses sources)' cargo deny check bans licenses sources

# ── Determinism subset ───────────────────────────────────────────────────────
# These three crates form the state-determinism subset. The scoped conformance strict-bit
# tests below are additional; the rest of the workspace waits for the release gate — why `full` exists
# and why a green PR is not evidence that the suite passes.
#
# The release-codegen run of the subset is this script's alone before a merge (issue #502):
# pr-gate.yml runs the subset in debug codegen on both architectures, and the release-codegen
# cells run in release-gate.yml on the merge commit. Run on the final head, this step is the
# pre-merge release-codegen determinism evidence for a change, on one host.
step 'determinism subset' \
  cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
step 'determinism subset (release codegen)' \
  cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci-release \
    --cargo-profile release --no-tests=fail

# The strict-bit binary shares the four suites' fixtures and runs inventory/provenance, facade
# mutation and synthetic matrix controls. These local observations do NOT qualify Linux or macOS;
# CI independently captures two processes in each native Linux architecture/codegen cell.
step 'strict-bit corpus and controls' \
  cargo nextest run -p oce-conformance --test strict_bits --locked \
    --test per_block_reals_transcendental --test per_block_reals_sources_transcendental \
    --test per_block_psychrometrics --test per_block_utilities --profile ci --no-tests=fail
step 'strict-bit corpus and controls (release codegen)' \
  cargo nextest run -p oce-conformance --test strict_bits --locked \
    --test per_block_reals_transcendental --test per_block_reals_sources_transcendental \
    --test per_block_psychrometrics --test per_block_utilities --profile ci-release \
    --cargo-profile release --no-tests=fail

# ── Fixture input hygiene ────────────────────────────────────────────────────
# NOT engine coverage — it exercises no shipping code path. It checks 46 G36 catalog fixtures and
# one resolver-contract document for block ports listed in upstream CDL declaration order.
#
# It gates because the fixtures are the INPUTS to every conformance test in the workspace: a
# transposed port fails nothing, it makes the whole suite validate the wrong sequence. The
# resolver assigns positions from document array order and only KINDS are checked downstream, so
# a same-kind transposition (PID's u_s/u_m, Latch's u/clr) is invisible to everything else.
#
# ~1.4s: the fixture corpus is verified clean, so this stays silent until someone edits a fixture
# — which is exactly when nobody remembers to run it by hand.
# `--no-tests=fail` so a renamed or deleted test hard-fails instead of going green on zero tests.
step 'fixture port order (input hygiene)' \
  cargo nextest run -p oce-cxf --locked --profile ci \
  -E 'binary(fixture_port_order)' --no-tests=fail

# Same register as the port-order audit, one level deeper: every G36 fixture is
# structurally verified against the vendored modelica-json oracle (hierarchy flattened,
# conditionals resolved against the fixture's own parameters, instances + undirected
# edges compared), or excluded with a recorded reason. A structurally wrong fixture
# fails nothing downstream — it makes the whole suite validate the wrong sequence.
# ~0.8s measured; the binary also carries the third_party hash-manifest gate and the
# G36 golden-provenance digest gate, so this stays silent until a fixture, the oracle,
# a vendored byte, or a provenance record changes — exactly when nobody would re-run it
# by hand. That binary also fails the run when OCE_BLESS arms regeneration, after two
# of its tests have already had an opportunity to rewrite their own goldens.
step 'structural oracle (input hygiene)' \
  cargo nextest run -p oce-cxf --locked --profile ci \
  -E 'binary(fixture_structural_oracle)' --no-tests=fail

# The published Quickstart, EXECUTED. A byte-pin proves the README and the example agree; it does
# not prove either one runs, and this repo once shipped a Quickstart that compiled, was byte-pinned,
# and had never been executed by anything.
#
# This step existed in ci.yml as its own `gate (light)` step and NOT here, which made it the one
# command out of eleven that gated a pull request while being invisible to anyone running this
# script. `bash .agents/gate.sh` could pass locally and CI still go red on a check the contributor
# had no way to run. Adding it closes that gap in the direction the rule requires: ci.yml already
# demanded it, so this is the script catching up to CI rather than the script growing a
# local-only requirement.
step 'quickstart executes' bash .github/scripts/check-quickstart-runs.sh

fi # engine_lane

# ── Full suite (release-gate.yml) ────────────────────────────────────────────
if [ "$MODE" = full ]; then
  step 'nextest — workspace' \
    cargo nextest run --workspace --locked --profile ci --no-tests=fail
  step 'nextest — workspace (release codegen)' \
    cargo nextest run --workspace --locked --profile ci-release \
    --cargo-profile release --no-tests=fail
  # nextest cannot run doctests; this step is not optional.
  step 'doctests' cargo test --workspace --doc --locked
  # The determinism cells write the 46 compiled-schedule blobs to OCE_BLOB_OUT as a path relative
  # to the workspace root, and the determinism-compare fan-in requires each file to be 123329 bytes
  # (the goldens' total). Write it the same way here, so a test that resolves the path wrongly fails
  # locally instead of first in CI (it did once, on the PR that added the blobs).
  step 'compiled-schedule blob matrix artifact' blob_matrix_artifact
  blob_round_trip
fi

# ── Report ───────────────────────────────────────────────────────────────────
cat <<'COVERAGE'

────────────────────────────────────────────────────────────────────────────
NOT COVERED BY THIS SCRIPT — a green run here does not prove these pass:
  · cross-arch determinism matrix. Per PR (pr-gate.yml): two native cells in
    debug codegen, x86_64 on ubuntu-latest and aarch64 on ubuntu-24.04-arm,
    with the cross-architecture snapshot, compiled-schedule blob and
    round-trip comparisons in a fan-in job. After each merge into
    development (release-gate.yml): the same matrix in four cells, debug and
    release codegen, adding the debug-against-release comparisons. One
    machine cannot reproduce either. CI is the only place they run.
    What `light` and `full` DO run is the determinism subset under release
    codegen on this host. No per-PR CI job runs it, so that step, run on the
    final head, is the pre-merge release-codegen evidence for a change.
  · the blob round trip's aarch64 leg and the comparison of the cells'
    recordings (determinism-compare). `full` and `roundtrip` record on this
    host and replay here and on wasm32-wasip1; the other architecture is CI's.
  · strict-bit raw-output matrix over the 21 inventoried signals: four native
    cells on every PR (both architectures, debug and release codegen), two
    capture processes each, then the eight-capture comparison. Local
    synthetic controls and macOS observations are NOT native Linux evidence.
  · the macOS arm64 platform cell (per PR) and the Windows platform cell
    (release-gate.yml, after each merge): they need those OSes; CI only.
  · executing the firmware images on their targets. This run builds them and
    checks their size; qemu.yml runs them under QEMU nightly, and
    `bash .agents/gate.sh qemu` does locally when QEMU is installed.
  · cargo deny check advisories. Needs network and a writable advisory-db;
    it runs in advisories.yml (daily) and in release-gate.yml's cargo-deny
    job, which sweeps advisories/bans/licenses/sources on every release PR.
  · that these commands still match pr-gate.yml. Nothing verifies that
    mechanically. Change a command here only by changing CI first.
    CI does RUN this script, but only after a merge: `gate (full)` in
    release-gate.yml, on every merge into development and daily. No per-PR
    job executes it, so for a PR that changes this script, a CI or
    repository script, a workflow or tool config, this run is the only one
    before the merge. Read the CI run as coverage, not parity: a green
    `gate (full)` says these commands ran, and says nothing at all about
    whether the two files still agree.
COVERAGE

if [ "$MODE" = leverage ]; then
  cat <<'LEVERAGE'
  · the engine lane: no-db, golden-gen firewall, facade contract and
    Quickstart, clippy, build, rustdoc, cargo-machete and cargo-deny over the
    workspace, the determinism and strict-bit subsets, and the input-hygiene
    audits. A `leverage` run mirrors a PR confined to crates/leverage/ and
    the docs. If anything else changed, run `bash .agents/gate.sh`.
LEVERAGE
fi

if [ "$MODE" != full ]; then
  cat <<'LIGHT'
  · the workspace test suite and doctests. This was not a `full` run: it
    mirrors the per-PR gate — and the per-PR gate does not run them.
    Run `bash .agents/gate.sh full` before claiming tests pass.
  · the blob round trip's recorded artifact and its wasm32-wasip1 replay,
    which the per-PR gate does run: `bash .agents/gate.sh roundtrip`.
LIGHT
fi

echo '────────────────────────────────────────────────────────────────────────────'

if [ "$failures" -gt 0 ]; then
  printf '\n\033[31mGATE FAILED\033[0m — %d step(s):\n' "$failures"
  printf '  · %s\n' "${failed_steps[@]}"
  echo
  echo 'Fix the cause. Never weaken a gate to reach green.'
  exit 1
fi

printf '\n\033[32mGATE PASSED\033[0m (%s) — all steps green.\n' "$MODE"
