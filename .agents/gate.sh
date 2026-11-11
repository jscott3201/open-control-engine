#!/usr/bin/env bash
#
# The gate. This file is the single runnable source of truth for what must pass
# before a change lands; every other document points here rather than restating
# commands. Nine divergent copies of this list existed before it was written,
# two of them materially weaker than CI, so prose copies are the failure mode
# this file exists to remove.
#
# FOUR PROOF MODES, one per row of the lane-matched local proof (issue #545; the table and the
# rules around it are in docs/ci-and-the-gate.md, "The local proof"). Each runs everything the
# one above it runs, so a heavier run satisfies a lighter row. An author runs the row the change
# needs, first-hand, on the final head, and pastes the tail into the pull request;
# scripts/ci/proof-row.sh names the row from the changed paths, and the closing report of every
# run prints the row this checkout needs beside the row the run covered.
#
#   bash .agents/gate.sh docs        # docs        — a docs-only PR: the always-on jobs of
#                                    #               pr-gate.yml (fmt, file size, secrets, the
#                                    #               package/authority/product contract, the gate
#                                    #               fixtures and the documentation checks). No
#                                    #               cargo build of any kind.
#   bash .agents/gate.sh leverage    # leverage    — a PR confined to crates/leverage/, the
#                                    #               documentation and the root README.md: `docs`
#                                    #               plus the leverage lane and the README lane
#   bash .agents/gate.sh             # light       — any other change (the engine lane): `leverage`
#                                    #               plus everything that compiles the engine, in
#                                    #               debug codegen only
#   bash .agents/gate.sh full        # full        — a pin advance (third_party/**, the reference
#                                    #               catalog's pin records, the two pin-guard
#                                    #               files proof-row.sh lists) or a change to this
#                                    #               script, .config/nextest.toml or
#                                    #               scripts/ci/install-wasmtime.sh:
#                                    #               `determinism` (below) plus the
#                                    #               workspace suite in debug and release codegen,
#                                    #               the doctests, the blob matrix artifact and
#                                    #               the blob round trip. It is also the mode CI
#                                    #               executes (release-gate.yml's `gate (full)`,
#                                    #               nightly); no other change owes it.
#
# ONE REPRODUCTION MODE, between `light` and `full`, that no row asks for (issue #556):
#
#   bash .agents/gate.sh determinism # determinism — `light` plus the determinism subset and the
#                                    #               strict-bit step under release codegen. From
#                                    #               issue #545 a change to the core four, oce-cxf,
#                                    #               oce-api or the build files owed it, because
#                                    #               no per-PR job ran the subset under release
#                                    #               codegen; pr-gate.yml runs all four cells
#                                    #               again, so it is now the way to reproduce a
#                                    #               red release cell on one host.
#
# THREE FOCUSED MODES, each one cell and not a proof row:
#
#   bash .agents/gate.sh nostd       # nostd       — the bare-metal cell alone (pr-gate.yml's
#                                    #               `no-std-build` job): the no_std core, the
#                                    #               firmware image's build and size budget; light,
#                                    #               determinism and full run it too
#   bash .agents/gate.sh qemu        # qemu        — the emulation check (qemu.yml, nightly): the
#                                    #               firmware images under QEMU against the host
#                                    #               reference; needs qemu-system-arm and
#                                    #               qemu-system-riscv32 on PATH
#   bash .agents/gate.sh roundtrip   # roundtrip   — the blob round trip (pr-gate.yml's determinism
#                                    #               cells, determinism-compare and
#                                    #               blob-round-trip-wasm): record on the host, then
#                                    #               replay the recording on the host and on
#                                    #               wasm32-wasip1 under wasmtime; needs the
#                                    #               wasm32-wasip1 target, and installs or
#                                    #               re-verifies the pinned wasmtime
#                                    #               (scripts/ci/install-wasmtime.sh); full runs it
#                                    #               too
#
# No mode runs every crate's own tests except `full`. Whatever the row, also run the suites of
# the crates the change touches: `cargo nextest run -p <crate> --locked --profile ci
# --no-tests=fail` (and `cargo test -p <crate> --doc --locked` for its doctests).
#
# Commands are kept in CI's exact form, flags included. `--locked`, the `ci`
# nextest profile, `--no-tests=fail`, and `RUSTDOCFLAGS` are each load-bearing:
# dropping any one of them turns a real gate into a green light that proves
# less than it appears to. Change a command here only by changing CI first.
#
# CI executes this script as well as mirroring it: `gate (full)` in release-gate.yml, nightly on
# the `development` tip, on dispatch, on release PRs and on pushes to `main` (issue #545; it ran
# on every merge into `development` before). No per-PR job executes it (pr-gate.yml's
# `gate (light)` was retired by issue #502), so CI first runs an edit to this script, a CI or
# repository script, a workflow or tool config on the night after it merges. Before the merge the
# author's own lane-matched run on the final head is the only run there is.
#
# Agents: this script is executable but not writable inside a Codex lane
# sandbox. That is deliberate. A failing gate is fixed at its cause, never by
# editing this file.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

MODE="${1:-light}"
case "$MODE" in
  docs | leverage | light | determinism | full | nostd | qemu | roundtrip) ;;
  *)
    echo "usage: bash .agents/gate.sh [docs|leverage|light|determinism|full|nostd|qemu|roundtrip]" >&2
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
# wasmtime comes from the installer, never from whatever `wasmtime` PATH holds: the installer
# keeps a binary already at its default destination only if its SHA-256 is the pinned one, and
# otherwise downloads the pinned release and verifies it (issue #545). That destination is a user
# cache directory outside `target/`, so `cargo clean` does not delete it. The rustup target is
# still the author's to add.
round_trip_replay_wasm() {
  local tools
  tools="$(bash scripts/ci/install-wasmtime.sh --print-destination)" || return 1
  bash scripts/ci/install-wasmtime.sh || return 1
  if ! rustup target list --installed | grep -qx wasm32-wasip1; then
    echo 'the wasm32-wasip1 target is not installed: rustup target add wasm32-wasip1' >&2
    return 1
  fi
  if [ ! -x "$tools/wasmtime" ]; then
    echo "the pinned wasmtime is not at $tools: bash scripts/ci/install-wasmtime.sh" >&2
    return 1
  fi
  test -d "$ROUND_TRIP_DIR" &&
    "$tools/wasmtime" --version &&
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
  echo '  · everything else in the per-PR gate, and every row of the local proof: a focused'
  echo '    mode is not one. Run the mode `bash scripts/ci/proof-row.sh` names for the change.'
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

# The engine (determinism) lane: everything that compiles the engine. `docs` and `leverage`
# skip it, as pr-gate.yml does for a PR confined to crates/leverage/, the docs and the root
# README.md.
engine_lane() { [ "$MODE" = light ] || [ "$MODE" = determinism ] || [ "$MODE" = full ]; }
# The leverage lane and the README lane: the two jobs that compile without the engine lane.
# `docs` skips them, as pr-gate.yml does for a docs-only PR, which compiles nothing.
compiled_lanes() { [ "$MODE" != docs ]; }
# The release-codegen steps (issue #545): the determinism subset and the strict-bit step under
# `--cargo-profile release`. pr-gate.yml runs both in native cells on every determinism-lane PR
# (the strict-bit matrix always did; the determinism matrix again since issue #556), so no row
# of the local proof asks for them and `light` leaves them out. `determinism` runs them, to
# reproduce a red release cell locally, and `full` does because CI executes it.
release_codegen() { [ "$MODE" = determinism ] || [ "$MODE" = full ]; }

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
step 'local proof-row fixtures' bash scripts/ci/test-proof-row.sh
step 'gate-mode nesting fixtures' bash scripts/ci/test-gate-modes.sh
step 'wasmtime installer destination fixtures' bash scripts/ci/test-install-wasmtime.sh

# ── Documentation checks ─────────────────────────────────────────────────────
# pr-gate.yml's always-on `docs` job, step for step (issue #545: they were CI-only until the
# docs-only proof row needed them, and a docs PR's own check was the one thing its author could
# not run from here). The installer is the job's first step: it downloads the checksum-pinned
# mdBook once, and afterwards only re-verifies the binary's digest. The staged book is removed
# when the link check has read it.
documentation_staging() {
  local revision
  revision="$(git rev-parse HEAD)" || return 1
  python3 scripts/docs/check_integrity.py --output target/docs-book --revision "$revision"
}
step 'mdBook, checksum-pinned' bash scripts/docs/install-mdbook.sh target/mdbook-bin
step 'mdBook installer verify-or-reinstall contract' bash scripts/docs/test-install-mdbook.sh
step 'documentation checker contracts' python3 scripts/docs/test_check_links.py
step 'authority literals through staging and mdBook' \
  python3 scripts/docs/test_authority_literals.py --mdbook target/mdbook-bin/mdbook
step 'documentation staging without source writes' documentation_staging
step 'mdBook build' target/mdbook-bin/mdbook build target/docs-book
step 'generated internal links and anchors' \
  python3 scripts/docs/check_links.py target/docs-book/book --site-prefix /open-control-engine/
rm -rf target/docs-book

if compiled_lanes; then

# ── Leverage lane ────────────────────────────────────────────────────────────
# crates/leverage/*: layout and the oce-api-only dependency rule, then fmt, clippy,
# rustdoc, nextest and cargo-machete per member — the `leverage-lane` job's one
# command (oce-replay is the first member; with none it prints "no leverage crates yet"). pr-gate.yml runs it
# for every engine change too, so it runs in every mode but `docs`.
step 'leverage lane' bash scripts/ci/leverage-members.sh lane

# ── README lane ──────────────────────────────────────────────────────────────
# The two checks that read the root README.md and are in no always-on job: pr-gate.yml's
# `facade-contract` job, filtered on the `readme` lane (issue #503), which runs when the root
# README.md changes and for every engine change. They run in every mode but `docs`. A landing PR
# confined to crates/leverage/ usually updates the README's status and roadmap too; CI then
# runs these two beside the leverage lane, so a `leverage` run that left them out could pass
# here and go red there. That job's other two steps, the facade compiler contract and its
# controls, stay in the engine lane below: they read the facade's source, which no leverage
# or README change touches.
#
# The byte-pin proves the README's Quickstart and crates/oce-api/examples/quickstart.rs are
# the same bytes (the determinism subset below runs the same test inside the oce-api suite;
# it is named here because `leverage` mode skips that subset). `--no-tests=fail` so a renamed
# or deleted test hard-fails instead of going green on zero tests.
step 'README Quickstart byte-pin' \
  cargo nextest run -p oce-api --test readme_quickstart --locked --profile ci --no-tests=fail

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

fi # compiled_lanes

# ── Engine lane: everything from here to `fi # engine_lane` ──────────────────
# Skipped in `docs` and `leverage` mode, as pr-gate.yml skips the determinism lane for a PR
# confined to crates/leverage/, the documentation and the root README.md.
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
step 'determinism subset' \
  cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail

# The strict-bit binary shares the four suites' fixtures and runs inventory/provenance, facade
# mutation and synthetic matrix controls. These local observations do NOT qualify Linux or macOS;
# CI independently captures two processes in each native Linux architecture/codegen cell.
step 'strict-bit corpus and controls' \
  cargo nextest run -p oce-conformance --test strict_bits --locked \
    --test per_block_reals_transcendental --test per_block_reals_sources_transcendental \
    --test per_block_psychrometrics --test per_block_utilities --profile ci --no-tests=fail

# ── Release codegen (`determinism` and `full`) ───────────────────────────────
# The subset and the strict-bit binaries again, under release codegen, on this one host.
# pr-gate.yml runs both in four native cells per PR (issue #556) and compares debug against
# release across them, which one machine cannot; these two steps reproduce a release-only
# failure locally.
if release_codegen; then
  step 'determinism subset (release codegen)' \
    cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci-release \
      --cargo-profile release --no-tests=fail
  step 'strict-bit corpus and controls (release codegen)' \
    cargo nextest run -p oce-conformance --test strict_bits --locked \
      --test per_block_reals_transcendental --test per_block_reals_sources_transcendental \
      --test per_block_psychrometrics --test per_block_utilities --profile ci-release \
      --cargo-profile release --no-tests=fail
fi

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

fi # engine_lane

# ── Full suite (release-gate.yml) ────────────────────────────────────────────
# The release-codegen workspace run is CI's only one (issue #545): release-gate.yml's `test-suite`
# job runs the debug suite and the doctests, and `gate (full)` executes this block and then
# requires the JUnit report of the nextest profile `ci-release-workspace`. That profile is this
# one command's: the release determinism subset and strict-bit step above write `ci-release`'s
# report, so only a profile of its own lets the job tell that this run happened. Use it nowhere
# else. check-workflow-gates.sh pins the five lines after the `if` below exactly, comments
# aside, so a step cannot be dropped, wrapped in a condition or moved out of the block unseen;
# keep each command on one line.
if [ "$MODE" = full ]; then
  step 'nextest — workspace' \
    cargo nextest run --workspace --locked --profile ci --no-tests=fail
  step 'nextest — workspace (release codegen)' \
    cargo nextest run --workspace --locked --profile ci-release-workspace --cargo-profile release --no-tests=fail
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
# Which row of the lane-matched local proof this run is, and which row this checkout's changes
# need (issue #545). The second half is advice from scripts/ci/proof-row.sh over the paths that
# differ from the merge base with origin/development, uncommitted and untracked files included.
# It never changes the exit status: a run answers for its steps, and a stale or absent
# origin/development (a CI checkout has none) must not fail a gate. The tail an author pastes
# carries both lines, so a proof lighter than the change needs is visible to whoever reads it.
row_rank() {
  case "$1" in
    docs) echo 1 ;;
    leverage) echo 2 ;;
    light) echo 3 ;;
    determinism) echo 4 ;;
    full) echo 5 ;;
    *) echo 0 ;;
  esac
}
row_command() {
  case "$1" in
    light) echo 'bash .agents/gate.sh' ;;
    *) echo "bash .agents/gate.sh $1" ;;
  esac
}
proof_row_report() {
  local base paths count needed
  echo
  echo '────────────────────────────────────────────────────────────────────────────'
  echo 'LOCAL PROOF — the row of the lane-matched table this run is'
  echo '(docs/ci-and-the-gate.md, "The local proof"):'
  case "$MODE" in
    docs)
      echo '  · this run: `docs`, the proof for a docs-only change.'
      ;;
    leverage)
      echo '  · this run: `leverage`, the proof for a change confined to crates/leverage/,'
      echo '    the documentation and the root README.md. It also covers the `docs` row.'
      ;;
    light)
      echo '  · this run: `light`, the engine-lane proof, in debug codegen, for any change the'
      echo '    `full` row does not name. It also covers the `docs` and `leverage` rows.'
      ;;
    determinism)
      echo '  · this run: `determinism`, the engine-lane proof plus the determinism subset and'
      echo '    the strict-bit step under release codegen. No row asks for it since pr-gate.yml'
      echo '    runs the release cells per PR; it covers every row but `full`.'
      ;;
    full)
      echo '  · this run: `full`, the proof for a pin advance or a change to this script, and'
      echo '    what CI executes nightly. It covers every row.'
      ;;
  esac
  if ! base="$(git merge-base HEAD origin/development 2>/dev/null)"; then
    echo '  · the row this checkout needs: UNKNOWN. There is no origin/development to compare'
    echo '    with here; list the changed paths yourself and run'
    echo '    `bash scripts/ci/proof-row.sh` over them.'
    return 0
  fi
  paths="$({
    git diff --no-renames --name-only "$base"
    git ls-files --others --exclude-standard
  } 2>/dev/null | sort -u || true)"
  count="$(printf '%s' "$paths" | grep -c . || true)"
  if [ "$count" -eq 0 ]; then
    echo "  · the row this checkout needs: none. Nothing differs from the merge base with"
    echo "    origin/development (${base:0:9})."
    return 0
  fi
  if ! needed="$(printf '%s\n' "$paths" | bash scripts/ci/proof-row.sh 2>/dev/null | sed -n 's/^row=//p')" ||
    [ "$(row_rank "$needed")" -eq 0 ]; then
    echo '  · the row this checkout needs: UNKNOWN. scripts/ci/proof-row.sh did not name one.'
    return 0
  fi
  echo "  · the row this checkout needs: \`$needed\` ($(row_command "$needed")), from the $count"
  echo "    path(s) that differ from the merge base with origin/development (${base:0:9}),"
  echo '    uncommitted and untracked files included. Fetch first if that base is stale.'
  if [ "$(row_rank "$MODE")" -ge "$(row_rank "$needed")" ]; then
    echo '  · this mode covers that row.'
  else
    printf '  · \033[31mTHIS MODE DOES NOT COVER THAT ROW.\033[0m Run `%s` and paste that tail.\n' \
      "$(row_command "$needed")"
  fi
  echo '  · a row does not run every crate'"'"'s tests. Run the suites of the crates the change'
  echo '    touches as well: cargo nextest run -p <crate> --locked --profile ci --no-tests=fail'
}
proof_row_report

cat <<'COVERAGE'

────────────────────────────────────────────────────────────────────────────
NOT COVERED BY THIS SCRIPT — a green run here does not prove these pass:
  · cross-arch determinism matrix. Per PR (pr-gate.yml), and again nightly
    (release-gate.yml): four native cells, x86_64 on ubuntu-latest and
    aarch64 on ubuntu-24.04-arm, each in debug and release codegen, with
    the cross-architecture and the debug-against-release snapshot,
    compiled-schedule blob and round-trip comparisons in a fan-in job. One
    machine cannot reproduce it. CI is the only place it runs. What
    `determinism` and `full` DO run is the determinism subset under release
    codegen on this host: one cell of the four, and no comparison.
  · the blob round trip's aarch64 leg and the comparison of the cells'
    recordings (determinism-compare). `full` and `roundtrip` record on this
    host and replay here and on wasm32-wasip1; the other architecture is CI's.
  · strict-bit raw-output matrix over the 21 inventoried signals: four native
    cells on every PR (both architectures, debug and release codegen), two
    capture processes each, then the eight-capture comparison. Local
    synthetic controls and macOS observations are NOT native Linux evidence.
  · the macOS arm64 platform cell (per PR) and the Windows platform cell
    (merge-gate.yml, after each merge): they need those OSes; CI only.
  · executing the firmware images on their targets. `light`, `determinism`
    and `full` build them and check their size; qemu.yml runs them under
    QEMU nightly, and `bash .agents/gate.sh qemu` does locally when QEMU is
    installed.
  · cargo deny check advisories. Needs network and a writable advisory-db;
    it runs in advisories.yml (daily) and in release-gate.yml's cargo-deny
    job, which sweeps advisories/bans/licenses/sources nightly and on every
    release PR.
  · the golden-gen hygiene job and the five per-PR crate suites (oce-graph,
    oce-validate, oce-model, oce-cxf, oce-store). pr-gate.yml runs both;
    this script runs the five suites only inside `full`'s workspace suite,
    and the separate golden-gen workspace never. .agents/project-facts.md
    gives the commands.
  · that these commands still match pr-gate.yml. Nothing verifies that
    mechanically. Change a command here only by changing CI first.
    CI does RUN this script, but only after a merge: `gate (full)` in
    release-gate.yml, nightly on the development tip and on every release
    PR. No per-PR job executes it, so for a PR that changes this script, a
    CI or repository script, a workflow or tool config, this run is the
    only one before the merge. Read the CI run as coverage, not parity: a
    green `gate (full)` says these commands ran, and says nothing at all
    about whether the two files still agree.
COVERAGE

if [ "$MODE" = docs ]; then
  cat <<'DOCS'
  · anything that compiles: the leverage lane, the README lane (the
    Quickstart byte-pin and its execution) and the engine lane. A `docs` run
    mirrors a docs-only PR, which compiles nothing in CI either. If a path
    outside the documentation changed, or documentation a test reads
    (scripts/ci/classify-changes.sh lists it), run the mode
    `bash scripts/ci/proof-row.sh` names.
DOCS
fi

if [ "$MODE" = leverage ]; then
  cat <<'LEVERAGE'
  · the engine lane: no-db, golden-gen firewall, the facade compiler
    contract, clippy, build, rustdoc, cargo-machete and cargo-deny over the
    workspace, the determinism and strict-bit subsets, and the input-hygiene
    audits. A `leverage` run mirrors a PR confined to crates/leverage/, the
    docs and the root README.md (whose Quickstart byte-pin and execution it
    did run). If anything else changed, run `bash .agents/gate.sh`.
LEVERAGE
fi

if ! release_codegen; then
  cat <<'DEBUG_ONLY'
  · release codegen, of anything. This run built and tested in debug codegen
    only. The determinism subset and the strict-bit binaries run under
    release codegen per PR in pr-gate.yml's native cells;
    `bash .agents/gate.sh determinism` reproduces them on this host.
DEBUG_ONLY
fi

if [ "$MODE" != full ]; then
  cat <<'NOT_FULL'
  · the workspace test suite and doctests. This was not a `full` run: it
    mirrors the per-PR gate — and the per-PR gate does not run them. They
    run nightly (release-gate.yml). Before claiming a crate's tests pass,
    run that crate's suite: cargo nextest run -p <crate> --locked
    --profile ci --no-tests=fail, and cargo test -p <crate> --doc --locked.
  · the blob round trip's recorded artifact and its wasm32-wasip1 replay,
    which the per-PR gate does run: `bash .agents/gate.sh roundtrip`.
NOT_FULL
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
