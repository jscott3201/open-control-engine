#!/usr/bin/env bash
# Gate-behavior fixtures for check-workflow-gates.sh. The negative fixtures prove missing nextest,
# seeded advisory ignores, garbled workflow input, actions referenced by a movable tag, a per-PR
# gate that re-runs on pushes to development, a cell matrix that is no longer native on both
# architectures (or loses a cell, a fan-in or a comparison), and a lane filter that loses its CI OK
# result check, its `changes` outputs or its lane map, or lets a job skip on the wrong lane, a
# `continue-on-error` or a step-level `if:` that is not a static matrix selector, and a macOS or
# Windows platform cell that loses its runner, its lane, its comparison or its strict-bit exclusion,
# and a blob round trip that loses a leg, its recording, its comparison or its pinned wasmtime, and
# a README lane (issue #503) whose job loses its lane-map entry, its lane, its classifier output or
# its byte-pin step, fail closed. Since issue #502 the determinism matrix is split across the two workflows, and the
# fixtures follow it: pr-gate.yml must hold exactly the two debug cells and the cross-architecture
# comparisons, and release-gate.yml all four cells, the cross-codegen comparisons, the Windows
# cell, the push-to-development trigger, a pinned checkout ref and a concurrency group that never
# cancels a push run in progress.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT=".github/scripts/check-workflow-gates.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

write_positive() {
  dir="$1"
  deny="$2"
  root_cargo="$3"
  crates_dir="$4"
  mkdir -p "$dir"
  cat > "$dir/pr-gate.yml" <<'EOF'
on:
  pull_request:
    branches: [development]
  workflow_dispatch:
jobs:
  changes:
    outputs:
      core: ${{ steps.classify.outputs.core }}
      leverage: ${{ steps.classify.outputs.leverage }}
      readme: ${{ steps.classify.outputs.readme }}
      gate_scripts: ${{ steps.classify.outputs.gate_scripts }}
      docs_only: ${{ steps.classify.outputs.docs_only }}
    steps:
      - uses: actions/checkout@89abcdef0123456789abcdef0123456789abcdef # v4
        with:
          fetch-depth: 2
      - id: classify
        run: |
          if [ "$EVENT_NAME" = pull_request ]; then
            git diff --no-renames --name-only HEAD^1 HEAD > "$RUNNER_TEMP/changed-paths"
            bash scripts/ci/classify-changes.sh < "$RUNNER_TEMP/changed-paths" | tee -a "$GITHUB_OUTPUT"
          else
            bash scripts/ci/classify-changes.sh --all | tee -a "$GITHUB_OUTPUT"
          fi
  fmt:
    steps:
      - run: cargo fmt --all --check
  file-size-cap:
    steps:
      - run: bash .github/scripts/check-file-size.sh
  no-secret-scan:
    steps:
      - run: bash .github/scripts/check-no-secrets.sh
  golden-gen-hygiene:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: cargo run --release --locked
  build:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: cargo build --workspace --locked
  clippy:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: cargo clippy --workspace --all-targets --locked -- -D warnings
  rustdoc:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: cargo doc --no-deps --workspace --lib --document-private-items --locked
  deny:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: cargo deny check bans licenses sources
  facade-contract:
    needs: changes
    if: needs.changes.outputs.readme == 'true'
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - run: python3 scripts/facade_contract/check.py
      - run: python3 scripts/facade_contract/test_check.py
      - run: cargo nextest run -p oce-api --test readme_quickstart --locked --profile ci --no-tests=fail
      - run: test -s target/nextest/ci/junit.xml
      - run: bash .github/scripts/check-quickstart-runs.sh
  leverage-lane:
    needs: changes
    if: needs.changes.outputs.leverage == 'true'
    steps:
      - run: bash scripts/ci/leverage-members.sh lane
  default-no-db:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: bash .github/scripts/check-default-no-db.sh
  unused-deps:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: cargo-machete
      - run: cargo machete
  gate-fixtures:
    steps:
      - run: bash .github/scripts/test-check-default-no-db.sh
      - run: bash .github/scripts/test-check-golden-gen-anti-tautology.sh
      - run: bash .github/scripts/test-check-stale-crate-status.sh
      - run: bash .github/scripts/check-stale-crate-status.sh
      - run: bash .github/scripts/check-workflow-gates.sh
      - run: bash scripts/ci/test-classify-changes.sh
      - run: python3 scripts/ci/test_ci_ok.py
      - run: bash scripts/ci/test-leverage-members.sh
  golden-gen-firewall:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: bash .github/scripts/check-golden-gen-anti-tautology.sh
  package-publication-contract:
    steps:
      - run: python3 scripts/package_policy/test_validate.py
      - run: python3 scripts/package_policy/validate.py
      - name: authority claim consistency
        run: python3 scripts/authority_claims/check.py --check
      - name: authority claim hostile controls
        run: python3 scripts/authority_claims/test_check.py
  strict-bit-matrix:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        runner: [ubuntu-latest, ubuntu-24.04-arm]
        codegen: [debug, release]
        include:
          - runner: ubuntu-latest
            arch: x86_64
          - runner: ubuntu-24.04-arm
            arch: aarch64
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - env:
          CELL_ARCH: ${{ matrix.arch }}
          CELL_CODEGEN: ${{ matrix.codegen }}
        run: bash scripts/ci/strict-bits-cell.sh "$CELL_ARCH" "$CELL_CODEGEN"
      - uses: actions/upload-artifact@0123456789abcdef0123456789abcdef01234567 # v7
        with:
          name: strict-bits-${{ matrix.arch }}-${{ matrix.codegen }}
          path: target/strict-bits/
          if-no-files-found: error
  strict-bit-compare:
    needs: [changes, strict-bit-matrix]
    if: needs.changes.outputs.core == 'true'
    steps:
      - env:
          OCE_STRICT_MATRIX_DIR: target/strict-bits
          OCE_STRICT_RUN_ID: ${{ github.run_id }}
        run: >-
          cargo nextest run --archive-file target/strict-bits/strict-bits-tests.tar.zst
          --workspace-remap . --profile ci --no-tests=fail --run-ignored only
          -E 'test(=matrix::native_linux_matrix_is_complete_and_exact)'
      - run: test -s target/strict-bits/receipt-candidate.json
      - if: ${{ !cancelled() }}
        uses: actions/upload-artifact@0123456789abcdef0123456789abcdef01234567 # v7
        with:
          name: strict-bit-matrix
  determinism-matrix:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        runner: [ubuntu-latest, ubuntu-24.04-arm]
        codegen: [debug]
        include:
          - runner: ubuntu-latest
            arch: x86_64
          - runner: ubuntu-24.04-arm
            arch: aarch64
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - env:
          CELL_ARCH: ${{ matrix.arch }}
        run: test "$(uname -m)" = "$CELL_ARCH"
      - name: Clear cached reports and state vectors
        run: >-
          rm -rf target/nextest/{ci,ci-release}/junit.xml
          target/{portable,target}-state-{debug,release}.bin
      - run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
        env:
          OCE_PORTABLE_STATE_OUT: target/portable-state-debug.bin
          OCE_TARGET_STATE_OUT: target/target-state-debug.bin
      - env:
          OCE_ROUND_TRIP_OUT: target/determinism/round-trip
        run: cargo nextest run -p oce-conformance --test blob_round_trip --locked --profile ci --no-tests=fail
      - run: |
          test -s target/nextest/ci/junit.xml
          test -s target/portable-state-debug.bin
          test "$(ls target/determinism/round-trip | wc -l | tr -d ' ')" = 92
      - run: |
          cargo nextest archive -p oce-conformance --test blob_round_trip --locked --archive-file target/determinism/blob-round-trip.tar.zst
      - uses: actions/upload-artifact@0123456789abcdef0123456789abcdef01234567 # v7
        with:
          name: determinism-${{ matrix.arch }}-${{ matrix.codegen }}
          path: target/determinism/
          if-no-files-found: error
  determinism-compare:
    needs: [changes, determinism-matrix]
    if: needs.changes.outputs.core == 'true'
    strategy:
      matrix:
        include:
          - runner: ubuntu-latest
            arch: x86_64
            foreign: aarch64
          - runner: ubuntu-24.04-arm
            arch: aarch64
            foreign: x86_64
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - run: >-
          cmp determinism-x86_64-debug/portable-state-debug.bin determinism-aarch64-debug/portable-state-debug.bin &&
          ! cmp -s determinism-x86_64-debug/target-state-debug.bin determinism-aarch64-debug/target-state-debug.bin
      - run: |
          for file in determinism-{x86_64,aarch64}-debug/blobs-debug.bin; do
            size="$(wc -c < "$file" | tr -d ' ')"
            if [ "$size" != 123329 ]; then
              exit 1
            fi
          done
          cmp determinism-x86_64-debug/blobs-debug.bin determinism-aarch64-debug/blobs-debug.bin
      - run: |
          test "$CELL_ARCH" != "$FOREIGN_ARCH"
          export OCE_FOREIGN_TARGET_STATE_IN="$PWD/target/determinism/determinism-$FOREIGN_ARCH-debug/target-state-debug.bin"
          cargo nextest run --archive-file "target/determinism/determinism-$CELL_ARCH-debug/oce-api-lib.tar.zst" \
            --workspace-remap . --profile ci --no-tests=fail \
            -E 'test(=tests::state_portability_tests::foreign_matrix_target_snapshot_refuses_restore_when_supplied)'
      - run: |
          diff -rq determinism-x86_64-debug/round-trip determinism-aarch64-debug/round-trip
      - run: |
          export OCE_ROUND_TRIP_EXPECT="$PWD/target/determinism/determinism-x86_64-debug/round-trip"
          cargo nextest run --archive-file "target/determinism/determinism-$CELL_ARCH-debug/blob-round-trip.tar.zst" \
            --workspace-remap . --profile ci --no-tests=fail
  blob-round-trip-wasm:
    needs: [changes, determinism-matrix]
    if: needs.changes.outputs.core == 'true'
    runs-on: ubuntu-latest
    steps:
      - run: rustup target add wasm32-wasip1
      - uses: actions/cache@89abcdef0123456789abcdef0123456789abcdef # v6
        with:
          path: target/wasmtime-bin
          key: wasmtime-${{ runner.os }}-${{ runner.arch }}-${{ hashFiles('scripts/ci/install-wasmtime.sh') }}
      - run: bash scripts/ci/install-wasmtime.sh target/wasmtime-bin
      - uses: actions/download-artifact@0123456789abcdef0123456789abcdef01234567 # v7
        with:
          name: determinism-x86_64-debug
          path: target/round-trip-recorded
      - env:
          OCE_ROUND_TRIP_EXPECT: target/round-trip-recorded/round-trip
        run: |
          wasmtime --version
          cargo nextest run -p oce-conformance --test blob_round_trip --target wasm32-wasip1 --locked --profile ci --no-tests=fail
      - run: test -s target/nextest/ci/junit.xml
  portable-snapshot-macos:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    runs-on: macos-15
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - run: test "$(uname -s)-$(uname -m)" = Darwin-arm64
      - env:
          OCE_PORTABLE_STATE_OUT: target/portable-state-debug.bin
          OCE_TARGET_STATE_OUT: target/target-state-debug.bin
        run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
      - run: |
          test -s target/nextest/ci/junit.xml
          test -s target/portable-state-debug.bin
          test -s target/target-state-debug.bin
      - uses: actions/upload-artifact@0123456789abcdef0123456789abcdef01234567 # v7
        with:
          name: platform-snapshot-macos-arm64-debug
          path: target/platform-snapshot/
          if-no-files-found: error
  portable-snapshot-compare:
    needs: [changes, determinism-matrix, portable-snapshot-macos]
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: |
          cmp linux-x86_64/portable-state-debug.bin macos-arm64/portable-state-debug.bin
          for linux in linux-x86_64 linux-aarch64; do
            if cmp -s "$linux/target-state-debug.bin" macos-arm64/target-state-debug.bin; then
              exit 1
            fi
          done
  crate-tests:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - run: rm -f target/nextest/ci/junit.xml
      - run: >-
          cargo nextest run -p oce-graph -p oce-validate -p oce-model -p oce-cxf -p oce-store --locked --profile ci --no-tests=fail
      - run: test -s target/nextest/ci/junit.xml
  no-std-build:
    needs: changes
    if: needs.changes.outputs.core == 'true'
    steps:
      - run: rustup target add thumbv7em-none-eabihf riscv32imac-unknown-none-elf wasm32-unknown-unknown
      - run: bash scripts/ci/check-no-std.sh
      - run: bash scripts/ci/test-check-no-std.sh
      - run: >-
          cargo test -p oce-expr --no-default-features --features alloc --lib --locked -- --test-threads=1
      - run: bash scripts/ci/check-firmware-link.sh
      - run: python3 -I scripts/ci/test-firmware-size.py
      - run: cargo test -p oce-firmware-link --locked
  docs:
    steps:
      - uses: actions/cache@89abcdef0123456789abcdef0123456789abcdef # v6
        with:
          path: target/mdbook-bin
          key: mdbook-${{ runner.os }}-${{ runner.arch }}-${{ hashFiles('site/mdbook-checksums.txt', 'scripts/docs/install-mdbook.sh') }}
      - run: bash scripts/docs/install-mdbook.sh target/mdbook-bin
      - run: python3 scripts/docs/test_check_links.py
      - run: python3 scripts/docs/test_authority_literals.py --mdbook target/mdbook-bin/mdbook
      - run: >-
          python3 scripts/docs/check_integrity.py
          --output target/docs-book
          --revision "$DOCS_REPOSITORY_REVISION"
      - run: target/mdbook-bin/mdbook build target/docs-book
      - run: >-
          python3 scripts/docs/check_links.py
          target/docs-book/book
          --site-prefix /open-control-engine/
  ci-ok:
    name: CI OK
    if: always()
    needs:
      - changes
      - fmt
      - file-size-cap
      - no-secret-scan
      - golden-gen-hygiene
      - build
      - clippy
      - rustdoc
      - deny
      - facade-contract
      - leverage-lane
      - default-no-db
      - unused-deps
      - gate-fixtures
      - golden-gen-firewall
      - package-publication-contract
      - strict-bit-matrix
      - strict-bit-compare
      - determinism-matrix
      - determinism-compare
      - blob-round-trip-wasm
      - portable-snapshot-macos
      - portable-snapshot-compare
      - crate-tests
      - no-std-build
      - docs
    steps:
      - env:
          NEEDS_JSON: ${{ toJSON(needs) }}
          JOB_LANES: >-
            golden-gen-hygiene=core build=core clippy=core rustdoc=core deny=core
            facade-contract=readme leverage-lane=leverage
            default-no-db=core unused-deps=core golden-gen-firewall=core
            strict-bit-matrix=core strict-bit-compare=core
            determinism-matrix=core determinism-compare=core crate-tests=core
            portable-snapshot-macos=core portable-snapshot-compare=core
            no-std-build=core
            blob-round-trip-wasm=core
        run: python3 scripts/ci/ci_ok.py
EOF
  cat > "$dir/docs-pages.yml" <<'EOF'
on:
  pull_request:
    branches: [development]
jobs:
  build:
    steps:
      - uses: actions/cache@89abcdef0123456789abcdef0123456789abcdef # v6
        with:
          path: target/mdbook-bin
          key: mdbook-${{ runner.os }}-${{ runner.arch }}-${{ hashFiles('site/mdbook-checksums.txt', 'scripts/docs/install-mdbook.sh') }}
      - run: bash scripts/docs/install-mdbook.sh target/mdbook-bin
EOF
  cat > "$dir/release-gate.yml" <<'EOF'
on:
  pull_request:
    branches: [main]
  push:
    branches: [main, development]
  schedule:
    - cron: "17 8 * * *"
  workflow_dispatch:
concurrency:
  group: ${{ github.workflow }}-${{ github.event_name }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}
env:
  CHECKOUT_REF: ${{ (github.event_name == 'schedule' || github.event_name == 'workflow_dispatch') && 'development' || github.event_name == 'push' && github.sha || github.ref }}
jobs:
  test-suite:
    steps:
      - uses: actions/checkout@89abcdef0123456789abcdef0123456789abcdef # v4
        with:
          ref: ${{ env.CHECKOUT_REF }}
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - name: Clear cached nextest reports
        run: rm -f target/nextest/{ci,ci-release}/junit.xml
      - run: cargo nextest run --workspace --locked --profile ci --no-tests=fail
      - run: cargo nextest run --workspace --locked --profile ci-release --cargo-profile release --no-tests=fail
      - run: cargo test --workspace --doc --locked
      - run: >-
          for profile in ci ci-release;
          do test -s "target/nextest/$profile/junit.xml";
          done
  default-no-db:
    steps:
      - run: bash .github/scripts/check-default-no-db.sh
  unused-deps:
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: cargo-machete
      - run: cargo machete
  gate-fixtures:
    steps:
      - run: bash .github/scripts/test-check-default-no-db.sh
      - run: bash .github/scripts/test-check-golden-gen-anti-tautology.sh
      - run: bash .github/scripts/test-check-stale-crate-status.sh
      - run: bash .github/scripts/check-stale-crate-status.sh
  golden-gen-firewall:
    steps:
      - run: bash .github/scripts/check-golden-gen-anti-tautology.sh
  gate:
    steps:
      - run: rustup target add thumbv7em-none-eabihf riscv32imac-unknown-none-elf wasm32-unknown-unknown
      - run: |
          rustup target add wasm32-wasip1
          bash scripts/ci/install-wasmtime.sh target/wasmtime-bin
      - run: bash .agents/gate.sh full
  determinism-matrix:
    runs-on: ${{ matrix.runner }}
    strategy:
      fail-fast: false
      matrix:
        runner: [ubuntu-latest, ubuntu-24.04-arm]
        codegen: [debug, release]
        include:
          - runner: ubuntu-latest
            arch: x86_64
          - runner: ubuntu-24.04-arm
            arch: aarch64
    steps:
      - uses: actions/checkout@89abcdef0123456789abcdef0123456789abcdef # v4
        with:
          ref: ${{ env.CHECKOUT_REF }}
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - env:
          CELL_ARCH: ${{ matrix.arch }}
        run: test "$(uname -m)" = "$CELL_ARCH"
      - name: Clear cached reports and state vectors
        run: >-
          rm -rf target/nextest/{ci,ci-release}/junit.xml
          target/{portable,target}-state-{debug,release}.bin
      - if: matrix.codegen == 'debug'
        run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
        env:
          OCE_PORTABLE_STATE_OUT: target/portable-state-debug.bin
          OCE_TARGET_STATE_OUT: target/target-state-debug.bin
      - if: matrix.codegen == 'release'
        run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci-release --cargo-profile release --no-tests=fail
        env:
          OCE_PORTABLE_STATE_OUT: target/portable-state-release.bin
          OCE_TARGET_STATE_OUT: target/target-state-release.bin
      - if: matrix.codegen == 'debug'
        env:
          OCE_ROUND_TRIP_OUT: target/determinism/round-trip
        run: cargo nextest run -p oce-conformance --test blob_round_trip --locked --profile ci --no-tests=fail
      - if: matrix.codegen == 'release'
        env:
          OCE_ROUND_TRIP_OUT: target/determinism/round-trip
        run: cargo nextest run -p oce-conformance --test blob_round_trip --locked --profile ci-release --cargo-profile release --no-tests=fail
      - run: |
          test -s "target/nextest/$profile/junit.xml"
          test -s "target/portable-state-$CELL_CODEGEN.bin"
          test "$(ls target/determinism/round-trip | wc -l | tr -d ' ')" = 92
      - if: matrix.codegen == 'debug'
        run: |
          cargo nextest archive -p oce-conformance --test blob_round_trip --locked --archive-file target/determinism/blob-round-trip.tar.zst
      - uses: actions/upload-artifact@0123456789abcdef0123456789abcdef01234567 # v7
        with:
          name: determinism-${{ matrix.arch }}-${{ matrix.codegen }}
          path: target/determinism/
          if-no-files-found: error
  determinism-compare:
    needs: determinism-matrix
    strategy:
      matrix:
        include:
          - runner: ubuntu-latest
            arch: x86_64
            foreign: aarch64
          - runner: ubuntu-24.04-arm
            arch: aarch64
            foreign: x86_64
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - run: >-
          cmp determinism-x86_64-debug/portable-state-debug.bin determinism-x86_64-release/portable-state-release.bin &&
          cmp determinism-x86_64-debug/target-state-debug.bin determinism-x86_64-release/target-state-release.bin &&
          cmp determinism-aarch64-debug/portable-state-debug.bin determinism-aarch64-release/portable-state-release.bin &&
          cmp determinism-aarch64-debug/target-state-debug.bin determinism-aarch64-release/target-state-release.bin
      - run: >-
          cmp determinism-x86_64-debug/portable-state-debug.bin determinism-aarch64-debug/portable-state-debug.bin &&
          ! cmp -s determinism-x86_64-debug/target-state-debug.bin determinism-aarch64-debug/target-state-debug.bin
      - run: |
          for file in determinism-{x86_64,aarch64}-{debug,release}/blobs-*.bin; do
            size="$(wc -c < "$file" | tr -d ' ')"
            if [ "$size" != 123329 ]; then
              exit 1
            fi
          done
          test "$(ls determinism-*/blobs-*.bin | wc -l | tr -d ' ')" = 4
          cmp determinism-x86_64-debug/blobs-debug.bin determinism-x86_64-release/blobs-release.bin
          cmp determinism-aarch64-debug/blobs-debug.bin determinism-aarch64-release/blobs-release.bin
          cmp determinism-x86_64-debug/blobs-debug.bin determinism-aarch64-debug/blobs-debug.bin
      - run: |
          test "$CELL_ARCH" != "$FOREIGN_ARCH"
          export OCE_FOREIGN_TARGET_STATE_IN="$PWD/target/determinism/determinism-$FOREIGN_ARCH-debug/target-state-debug.bin"
          cargo nextest run --archive-file "target/determinism/determinism-$CELL_ARCH-debug/oce-api-lib.tar.zst" \
            --workspace-remap . --profile ci --no-tests=fail \
            -E 'test(=tests::state_portability_tests::foreign_matrix_target_snapshot_refuses_restore_when_supplied)'
      - run: |
          diff -rq determinism-x86_64-debug/round-trip determinism-x86_64-release/round-trip
          diff -rq determinism-x86_64-debug/round-trip determinism-aarch64-debug/round-trip
          diff -rq determinism-x86_64-debug/round-trip determinism-aarch64-release/round-trip
      - run: |
          export OCE_ROUND_TRIP_EXPECT="$PWD/target/determinism/determinism-x86_64-debug/round-trip"
          cargo nextest run --archive-file "target/determinism/determinism-$CELL_ARCH-debug/blob-round-trip.tar.zst" \
            --workspace-remap . --profile ci --no-tests=fail
  std-build-windows:
    runs-on: windows-latest
    steps:
      - uses: taiki-e/install-action@0123456789abcdef0123456789abcdef01234567 # v2
        with:
          tool: nextest@0.9.143
      - run: cargo build --workspace --locked
      - run: >-
          cargo nextest run --workspace --locked --profile ci --no-tests=fail
          -E 'not (binary_id(oce-conformance::strict_bits) | binary_id(oce-conformance::per_block_reals_transcendental) | binary_id(oce-conformance::per_block_reals_sources_transcendental) | binary_id(oce-conformance::per_block_psychrometrics) | binary_id(oce-conformance::per_block_utilities))'
      - run: test -s target/nextest/ci/junit.xml
  ci-ok:
    name: CI OK
    if: always()
    needs: [test-suite, default-no-db, unused-deps, gate-fixtures, golden-gen-firewall, gate, determinism-matrix, determinism-compare, std-build-windows]
    steps:
      - env:
          NEEDS_JSON: ${{ toJSON(needs) }}
        run: python3 check.py
EOF
  cat > "$dir/qemu.yml" <<'EOF'
on:
  schedule:
    - cron: "17 4 * * *"
  workflow_dispatch:
jobs:
  emulate:
    steps:
      - uses: actions/checkout@89abcdef0123456789abcdef0123456789abcdef # v4
      - run: bash scripts/ci/emulate-firmware.sh
EOF
  cat > "$dir/advisories.yml" <<'EOF'
on:
  schedule:
    - cron: "43 8 * * *"
  workflow_dispatch:
jobs:
  advisories:
    steps:
      - run: cargo deny check advisories
EOF
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = []

[bans]
deny = [
  { name = "sqlx" },
  { name = "sled" },
]
EOF
  cat > "$root_cargo" <<'EOF'
[workspace.lints.rust]
unsafe_code = "forbid"
EOF
  mkdir -p "$crates_dir/oce-api/src" "$crates_dir/oce-store/src"
  cat > "$crates_dir/oce-api/Cargo.toml" <<'EOF'
[package]
name = "oce-api"

[lints]
workspace = true
EOF
  cat > "$crates_dir/oce-store/Cargo.toml" <<'EOF'
[package]
name = "oce-store"

[lints]
workspace = true
EOF
  cat > "$crates_dir/oce-api/src/lib.rs" <<'EOF'
#![forbid(unsafe_code)]
//! Fixture crate.
EOF
  cat > "$crates_dir/oce-store/src/lib.rs" <<'EOF'
#![forbid(unsafe_code)]
//! Fixture crate.
EOF
}

run_case() {
  name="$1"
  expected="$2"
  mutate="$3"
  expected_message="${4:-}"
  dir="$tmp/$name/workflows"
  deny="$tmp/$name/deny.toml"
  root_cargo="$tmp/$name/Cargo.toml"
  crates_dir="$tmp/$name/crates"
  mkdir -p "$tmp/$name"
  write_positive "$dir" "$deny" "$root_cargo" "$crates_dir"
  "$mutate" "$dir" "$deny" "$root_cargo" "$crates_dir"

  set +e
  output="$(
    OCE_WORKFLOW_DIR="$dir" \
    OCE_DENY_TOML="$deny" \
    OCE_ROOT_CARGO_TOML="$root_cargo" \
    OCE_CRATES_DIR="$crates_dir" \
    OCE_FIRMWARE_DIR="$tmp/$name/firmware" \
    bash "$SCRIPT" 2>&1
  )"
  status=$?
  set -e

  case "$expected" in
    pass)
      if [ "$status" -ne 0 ]; then
        echo "FAIL: workflow fixture '$name' should pass but exited $status"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^OK:'; then
        echo "FAIL: workflow fixture '$name' passed without the OK line"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    fail)
      if [ "$status" -eq 0 ]; then
        echo "FAIL: workflow fixture '$name' should fail but exited 0"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^FAIL:'; then
        echo "FAIL: workflow fixture '$name' failed without a FAIL line"
        printf '%s\n' "$output"
        exit 1
      fi
      if [ -n "$expected_message" ] && ! printf '%s\n' "$output" | grep -Fq -- "$expected_message"; then
        echo "FAIL: workflow fixture '$name' failed without expected message substring: $expected_message"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    *)
      echo "BUG: unknown expectation '$expected'"
      exit 1
      ;;
  esac
}

noop() {
  _dir="$1"
  _deny="$2"
}

# `sed` over the fixture's release-gate.yml (edit_pr_gate, further down, is its pr-gate.yml twin).
edit_release_gate() {
  dir="$1"
  shift
  sed "$@" "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

# Only the full suite's release-codegen run goes; the determinism cells keep theirs, which must not
# stand in for it.
remove_release_nextest() {
  edit_release_gate "$1" '/^  test-suite:$/,/^  [^ ]/ {/--cargo-profile release/d;}'
}

seed_advisory_ignore() {
  _dir="$1"
  deny="$2"
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = ["RUSTSEC-2099-0001"]

[bans]
deny = [
  { name = "sqlx" },
  { name = "sled" },
]
EOF
}

empty_bans_deny() {
  _dir="$1"
  deny="$2"
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = []

[bans]
deny = []
EOF
}

garble_release_workflow() {
  dir="$1"
  _deny="$2"
  printf '%s\n' 'not: [a usable gate]' > "$dir/release-gate.yml"
}

remove_stale_status_gate() {
  dir="$1"
  _deny="$2"
  grep -v 'check-stale-crate-status' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

reinstall_gate_nightly() {
  dir="$1"
  _deny="$2"
  awk '
    { print }
    /tool: nextest@0\.9\.143/ {
      print "      - run: rustup toolchain install nightly-2026-05-01 --profile minimal"
    }
  ' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

remove_golden_gen_firewall() {
  dir="$1"
  _deny="$2"
  grep -v 'check-golden-gen-anti-tautology' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_package_publication_contract() {
  dir="$1"
  _deny="$2"
  grep -v 'scripts/package_policy' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

comment_authority_claim_check() {
  dir="$1"
  sed 's@run: python3 scripts/authority_claims/check.py --check@run: true # python3 scripts/authority_claims/check.py --check@' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_authority_claim_controls() {
  dir="$1"
  grep -v 'scripts/authority_claims/test_check.py' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_root_unsafe_forbid() {
  _dir="$1"
  _deny="$2"
  root_cargo="$3"
  cat > "$root_cargo" <<'EOF'
[workspace.lints.rust]
unsafe_code = "allow"
EOF
}

remove_crate_lints_workspace() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  cat > "$crates_dir/oce-api/Cargo.toml" <<'EOF'
[package]
name = "oce-api"
EOF
}

remove_crate_lib_forbid() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  cat > "$crates_dir/oce-api/src/lib.rs" <<'EOF'
//! Fixture crate.
EOF
}

remove_no_db_gate() {
  dir="$1"
  _deny="$2"
  grep -v 'check-default-no-db' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_determinism_matrix() {
  dir="$1"
  _deny="$2"
  grep -v 'oce-api -p oce-blocks -p oce-expr' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

comment_foreign_restore_command() {
  dir="$1"
  _deny="$2"
  grep -v 'foreign_matrix_target_snapshot_refuses_restore_when_supplied' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

allow_absent_foreign_restore_test() {
  dir="$1"
  _deny="$2"
  sed '/--workspace-remap \. --profile ci --no-tests=fail \\$/ s/ --no-tests=fail//' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_foreign_restore_nextest_install() {
  dir="$1"
  _deny="$2"
  awk '
    /^  determinism-compare:/ { in_job = 1 }
    /^  crate-tests:/ { in_job = 0 }
    in_job && /tool: nextest@0.9.143/ { next }
    { print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_aarch64_leg() {
  dir="$1"
  _deny="$2"
  sed 's/^        runner: \[ubuntu-latest, ubuntu-24\.04-arm\]$/        runner: [ubuntu-latest]/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

# aarch64 cells relabeled onto an x86_64 runner: the matrix still names both architectures, but
# no native arm64 host would ever run.
swap_aarch64_runner() {
  dir="$1"
  sed 's/^          - runner: ubuntu-24\.04-arm$/          - runner: ubuntu-latest/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

# The per-PR strict-bit matrix keeps its release cells (issue #502 moved only the determinism ones).
drop_strict_bit_release_codegen() {
  edit_pr_gate "$1" 's/^        codegen: \[debug, release\]$/        codegen: [debug]/'
}

# The split of the determinism matrix (issue #502), in both directions: pr-gate.yml must not grow
# its release-codegen cells back, and release-gate.yml must not lose them.
restore_per_pr_release_codegen() {
  edit_pr_gate "$1" 's/^        codegen: \[debug\]$/        codegen: [debug, release]/'
}

drop_release_gate_release_codegen() {
  edit_release_gate "$1" 's/^        codegen: \[debug, release\]$/        codegen: [debug]/'
}

drop_release_gate_aarch64_runner() {
  edit_release_gate "$1" 's/^        runner: \[ubuntu-latest, ubuntu-24\.04-arm\]$/        runner: [ubuntu-latest]/'
}

release_gate_fail_fast() {
  edit_release_gate "$1" 's/^      fail-fast: false$/      fail-fast: true/'
}

drop_release_gate_determinism_matrix() {
  edit_release_gate "$1" \
    -e '/^  determinism-matrix:$/,/^  determinism-compare:$/ { /^  determinism-compare:$/!d; }' \
    -e 's/^    needs: determinism-matrix$/    needs: test-suite/' \
    -e 's/, determinism-matrix, determinism-compare, /, determinism-compare, /'
}

drop_release_gate_release_subset() {
  edit_release_gate "$1" '/^  determinism-matrix:$/,/^  [^ ]/ {/-p oce-expr --locked --profile ci-release --cargo-profile release/d;}'
}

drop_release_gate_release_round_trip() {
  edit_release_gate "$1" '/^  determinism-matrix:$/,/^  [^ ]/ {/--test blob_round_trip --locked --profile ci-release/d;}'
}

drop_release_gate_cell_report_requirement() {
  edit_release_gate "$1" '/^  determinism-matrix:$/,/^  [^ ]/ {/test -s "target\/nextest\/\$profile\/junit.xml"/d;}'
}

detach_release_gate_fan_in() {
  edit_release_gate "$1" 's/^    needs: determinism-matrix$/    needs: test-suite/'
}

release_gate_fan_in_out_of_ci_ok() {
  edit_release_gate "$1" 's/, determinism-compare, std-build-windows\]$/, std-build-windows]/'
}

drop_cross_codegen_state_comparison() {
  edit_release_gate "$1" '/cmp determinism-aarch64-debug\/target-state-debug.bin determinism-aarch64-release\/target-state-release.bin$/d'
}

drop_cross_codegen_blob_comparison() {
  edit_release_gate "$1" '/cmp determinism-aarch64-debug\/blobs-debug.bin determinism-aarch64-release\/blobs-release.bin$/d'
}

drop_cross_codegen_round_trip_comparison() {
  edit_release_gate "$1" '/diff -rq determinism-x86_64-debug\/round-trip determinism-x86_64-release\/round-trip$/d'
}

release_gate_fan_in_over_two_cells() {
  edit_release_gate "$1" "s/^          test \"\$(ls determinism-\\*\/blobs-\\*.bin | wc -l | tr -d ' ')\" = 4$/          test \"\$(ls determinism-*\/blobs-*.bin | wc -l | tr -d ' ')\" = 2/"
}

drop_release_gate_cross_architecture_comparison() {
  edit_release_gate "$1" '/cmp -s determinism-x86_64-debug\/target-state-debug.bin/d'
}

drop_blob_architecture_comparison() {
  edit_pr_gate "$1" '/^          cmp determinism-x86_64-debug\/blobs-debug.bin determinism-aarch64-debug\/blobs-debug.bin$/d'
}

drop_blob_size_requirement() {
  edit_pr_gate "$1" '/if \[ "\$size" != 123329 \]; then/d'
}

# The run after each merge (issue #502): the trigger, the commit a push run tests, the pinned ref
# of every checkout, and a concurrency group that queues pushes without cancelling one in progress.
release_gate_without_development_push() {
  edit_release_gate "$1" 's/^    branches: \[main, development\]$/    branches: [main]/'
}

release_gate_without_push_trigger() {
  edit_release_gate "$1" '/^  push:$/,/^    branches:/d'
}

release_push_checks_out_the_moving_ref() {
  edit_release_gate "$1" "s/ || github.event_name == 'push' \&\& github.sha || github.ref }}\$/ || github.ref }}/"
}

release_checkout_without_the_pinned_ref() {
  edit_release_gate "$1" '/^  determinism-matrix:$/,/^  [^ ]/ {/^          ref: \${{ env.CHECKOUT_REF }}$/d;}'
}

release_push_cancels_a_run_in_progress() {
  edit_release_gate "$1" 's/^  cancel-in-progress: .*$/  cancel-in-progress: true/'
}

release_runs_share_no_concurrency_group() {
  edit_release_gate "$1" 's/^  group: .*$/  group: ${{ github.workflow }}-${{ github.run_id }}/'
}

enable_fail_fast() {
  dir="$1"
  sed 's/^      fail-fast: false$/      fail-fast: true/' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

# One emulation step, in a gating workflow or only in a comment.
reinstall_qemu() {
  dir="$1"
  awk '
    { print }
    /^  determinism-matrix:$/ { getline; print; print "    # aarch64 under qemu-user" }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

reinstall_release_cross_target() {
  dir="$1"
  awk '
    { print }
    /^      - run: cargo test --workspace --doc --locked$/ {
      print "      - run: cargo nextest run --workspace --target aarch64-unknown-linux-gnu"
    }
  ' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

detach_fan_in() {
  dir="$1"
  sed 's/^    needs: \[changes, determinism-matrix\]$/    needs: [changes]/' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

omit_strict_fan_in_need() {
  dir="$1"
  grep -v '^      - strict-bit-compare$' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

stop_requiring_target_bound_difference() {
  dir="$1"
  grep -v 'cmp -s determinism-x86_64-debug/target-state-debug.bin' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

restore_own_architecture_snapshot() {
  dir="$1"
  grep -v 'test "$CELL_ARCH" != "$FOREIGN_ARCH"' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

skip_ignored_matrix_comparison() {
  dir="$1"
  sed 's/ --run-ignored only$//' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

drop_receipt_candidate_requirement() {
  dir="$1"
  grep -v 'receipt-candidate.json' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_release_state_vector() {
  edit_release_gate "$1" '/OCE_PORTABLE_STATE_OUT: target\/portable-state-release.bin/d'
}

conditional_ci_ok() {
  dir="$1"
  _deny="$2"
  sed 's/^    if: always()$/    if: success()/' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

blind_release_ci_ok() {
  dir="$1"
  _deny="$2"
  grep -v 'toJSON(needs)' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

omit_ci_ok_need() {
  dir="$1"
  _deny="$2"
  grep -v '^      - golden-gen-firewall$' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

omit_release_ci_ok_need() {
  edit_release_gate "$1" 's/^    needs: \[test-suite, default-no-db, /    needs: [test-suite, /'
}

restore_development_push_trigger() {
  dir="$1"
  _deny="$2"
  awk '
    { print }
    /^    branches: \[development\]$/ && !done { print "  push:"; print "    branches: [development]"; done = 1 }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

unfiltered_push_trigger() {
  dir="$1"
  _deny="$2"
  awk '
    /^  workflow_dispatch:$/ { print "  push:" }
    { print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

flow_list_push_trigger() {
  dir="$1"
  _deny="$2"
  awk '
    /^on:$/ { print "on: [pull_request, push]"; skip = 1; next }
    skip && /^  / { next }
    { skip = 0; print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

# Replace the fixture's whole top-level `on:` block (up to `jobs:`) with the given text.
replace_trigger_block() {
  dir="$1"
  block="$2"
  TRIGGER_BLOCK="$block" awk '
    /^on:/ { print ENVIRON["TRIGGER_BLOCK"]; skip = 1; next }
    skip && /^jobs:$/ { skip = 0 }
    !skip { print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

four_space_push_trigger() {
  replace_trigger_block "$1" 'on:
    pull_request:
        branches: [development]
    "push":
        branches: [development]
    workflow_dispatch:'
}

four_space_pull_request_trigger() {
  replace_trigger_block "$1" 'on:
    pull_request:
        branches: [development]
    workflow_dispatch:'
}

block_list_push_trigger() {
  replace_trigger_block "$1" 'on:
  - pull_request
  - push'
}

flow_map_push_trigger() {
  replace_trigger_block "$1" "on: {pull_request: {branches: [development]}, 'push': {branches: [development]}}"
}

flow_map_pull_request_trigger() {
  replace_trigger_block "$1" 'on: {pull_request: {branches: [development]}, workflow_dispatch: null}'
}

remove_pull_request_trigger() {
  dir="$1"
  _deny="$2"
  awk '
    /^  pull_request:$/ { skip = 1; next }
    skip && /^    / { next }
    { skip = 0; print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

conditional_gating_job() {
  dir="$1"
  _deny="$2"
  awk '
    { print }
    /^  gate-fixtures:$/ { print "    if: github.event_name == '"'"'schedule'"'"'" }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

conditional_release_job() {
  dir="$1"
  awk '
    { print }
    /^  unused-deps:$/ { print "    if: github.event_name == '"'"'schedule'"'"'" }
  ' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

# Lane filter (issue #393): the controls the issue's acceptance names, and the ways a lane could
# quietly widen what a PR may skip.
edit_pr_gate() {
  dir="$1"
  shift
  sed "$@" "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

# gate_scripts stays a classifier output but is no longer a lane a job may be filtered on.
lane_filter_on_gate_scripts_output() {
  edit_pr_gate "$1" \
    -e '/^  facade-contract:$/,/^    if:/ s/outputs.readme/outputs.gate_scripts/' \
    -e 's/facade-contract=readme/facade-contract=gate_scripts/'
}

# The README lane (issue #503). `facade-contract` is the job filtered on `readme`; each control
# below is one way a README-only change could stop running the README checks, or stop being able
# to pass CI OK.
readme_job_out_of_lane_map() {
  edit_pr_gate "$1" 's/facade-contract=readme leverage-lane=leverage$/leverage-lane=leverage/'
}

readme_job_on_core_lane() {
  edit_pr_gate "$1" \
    -e '/^  facade-contract:$/,/^    if:/ s/outputs.readme/outputs.core/' \
    -e 's/facade-contract=readme/facade-contract=core/'
}

readme_job_made_always_on() {
  edit_pr_gate "$1" \
    -e '/^  facade-contract:$/,/^    if:/ { /^    needs: changes$/d; /^    if:/d; }' \
    -e 's/facade-contract=readme leverage-lane=leverage$/leverage-lane=leverage/'
}

determinism_on_readme_lane() {
  edit_pr_gate "$1" \
    -e '/^  determinism-matrix:$/,/^    if:/ s/outputs.core/outputs.readme/' \
    -e 's/determinism-matrix=core/determinism-matrix=readme/'
}

drop_changes_readme_output() {
  edit_pr_gate "$1" '/readme: \${{ steps.classify.outputs.readme }}/d'
}

drop_readme_byte_pin() {
  edit_pr_gate "$1" '/cargo nextest run -p oce-api --test readme_quickstart /d'
}

readme_byte_pin_passes_on_zero_tests() {
  edit_pr_gate "$1" \
    's/--test readme_quickstart --locked --profile ci --no-tests=fail$/--test readme_quickstart --locked --profile ci/'
}

readme_job_without_nextest() {
  dir="$1"
  awk '
    /^  facade-contract:$/ { in_job = 1 }
    in_job && /^      - uses: taiki-e\/install-action@/ { skip = 3 }
    in_job && /^  [^ ]/ && !/^  facade-contract:$/ { in_job = 0 }
    skip > 0 { skip--; next }
    { print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

readme_job_without_report_check() {
  dir="$1"
  awk '
    /^  facade-contract:$/ { in_job = 1 }
    in_job && /^  [^ ]/ && !/^  facade-contract:$/ { in_job = 0 }
    in_job && /^      - run: test -s target\/nextest\/ci\/junit\.xml$/ { next }
    { print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

drop_quickstart_execution() {
  edit_pr_gate "$1" '/run: bash .github\/scripts\/check-quickstart-runs.sh/d'
}

delete_ci_ok_result_check() {
  edit_pr_gate "$1" '/run: python3 scripts\/ci\/ci_ok.py/d'
}

restore_success_only_result_check() {
  edit_pr_gate "$1" 's/run: python3 scripts\/ci\/ci_ok.py/run: python3 check.py/'
}

drop_changes_core_output() {
  edit_pr_gate "$1" '/core: \${{ steps.classify.outputs.core }}/d'
}

drop_changes_classifier() {
  edit_pr_gate "$1" '/classify-changes.sh < /d'
}

shallow_changes_checkout() {
  edit_pr_gate "$1" '/fetch-depth: 2/d'
}

rename_blind_diff() {
  edit_pr_gate "$1" 's/git diff --no-renames --name-only/git diff --name-only/'
}

unlisted_lane_job() {
  edit_pr_gate "$1" 's/ crate-tests=core$//'
}

lane_map_names_wrong_lane() {
  edit_pr_gate "$1" 's/deny=core$/deny=leverage/'
}

determinism_on_leverage_lane() {
  edit_pr_gate "$1" \
    -e '/^  determinism-matrix:$/,/^    if:/ s/outputs.core/outputs.leverage/' \
    -e 's/determinism-matrix=core/determinism-matrix=leverage/'
}

docs_made_optional() {
  edit_pr_gate "$1" \
    -e 's/^  docs:$/  docs:\n    needs: changes\n    if: needs.changes.outputs.core == '"'"'true'"'"'/' \
    -e 's/crate-tests=core$/crate-tests=core docs=core/'
}

lane_filter_without_changes_need() {
  edit_pr_gate "$1" '/^  clippy:$/,/^    if:/ { /^    needs: changes$/d; }'
}

lane_filter_on_docs_only_output() {
  edit_pr_gate "$1" \
    -e '/^  leverage-lane:$/,/^    if:/ s/outputs.leverage/outputs.docs_only/' \
    -e 's/leverage-lane=leverage/leverage-lane=docs_only/'
}

negated_lane_filter() {
  edit_pr_gate "$1" "/^  build:\$/,/^    if:/ s/== 'true'/!= 'false'/"
}

removed_lane_job() {
  dir="$1"
  awk '
    /^  leverage-lane:$/ { skip = 1; next }
    skip && /^  [^ ]/ { skip = 0 }
    skip { next }
    /^      - leverage-lane$/ { next }
    { sub(/ leverage-lane=leverage/, ""); print }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

leverage_crate_without_lints() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  mkdir -p "$crates_dir/leverage/oce-host/src"
  printf '[package]\nname = "oce-host"\n' > "$crates_dir/leverage/oce-host/Cargo.toml"
  printf '#![forbid(unsafe_code)]\n' > "$crates_dir/leverage/oce-host/src/lib.rs"
}

leverage_crate_without_forbid() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  mkdir -p "$crates_dir/leverage/oce-host/src"
  printf '[package]\nname = "oce-host"\n\n[lints]\nworkspace = true\n' > "$crates_dir/leverage/oce-host/Cargo.toml"
  printf '//! Fixture crate.\n' > "$crates_dir/leverage/oce-host/src/lib.rs"
}

compliant_leverage_crate() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  mkdir -p "$crates_dir/leverage/oce-host/src"
  printf '[package]\nname = "oce-host"\n\n[lints]\nworkspace = true\n' > "$crates_dir/leverage/oce-host/Cargo.toml"
  printf '#![forbid(unsafe_code)]\n' > "$crates_dir/leverage/oce-host/src/lib.rs"
}

hardcode_strict_cell_architecture() {
  dir="$1"
  _deny="$2"
  sed 's/strict-bits-cell.sh "$CELL_ARCH" "$CELL_CODEGEN"/strict-bits-cell.sh x86_64 "$CELL_CODEGEN"/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_nextest_report_cleanup() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/pr-gate.yml" "$dir/release-gate.yml"; do
    grep -v 'target/nextest/{' "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

remove_nextest_report_requirements() {
  edit_pr_gate "$1" '/^  determinism-matrix:$/,/^  [^ ]/ {/test -s target\/nextest\/ci\/junit.xml$/d;}'
}

remove_release_nextest_report_requirements() {
  edit_release_gate "$1" '/^  test-suite:$/,/^  [^ ]/ {/for profile in/d; /test -s.*\$profile/d; /^[[:space:]]*done[[:space:]]*$/d;}'
}

downgrade_nextest_pin() {
  dir="$1"
  _deny="$2"
  for workflow in "$dir/pr-gate.yml" "$dir/release-gate.yml"; do
    sed 's/nextest@0\.9\.143/nextest@0.9.142/g' "$workflow" > "$workflow.tmp"
    mv "$workflow.tmp" "$workflow"
  done
}

unpin_action() {
  dir="$1"
  _deny="$2"
  sed 's#taiki-e/install-action@[0-9a-f]\{40\} \# v2#taiki-e/install-action@v2#' \
    "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

remove_crate_suite() {
  dir="$1"
  _deny="$2"
  sed 's/ -p oce-cxf -p oce-store --locked/ -p oce-cxf --locked/' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

allow_zero_crate_suite_tests() {
  dir="$1"
  _deny="$2"
  sed 's/ -p oce-store --locked --profile ci --no-tests=fail$/ -p oce-store --locked --profile ci/' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_docs_link_check() {
  dir="$1"
  _deny="$2"
  grep -v 'scripts/docs/check_links.py' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

remove_docs_checker_contracts() {
  dir="$1"
  _deny="$2"
  grep -v 'scripts/docs/test_check_links.py' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

mdbook_cache_restore_keys() {
  dir="$1"
  _deny="$2"
  sed 's|^\(          key: mdbook-.*\)$|\1\n          restore-keys: mdbook-|' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

mdbook_cache_after_install() {
  dir="$1"
  _deny="$2"
  awk '
    /^      - uses: actions\/cache@/ && !done { hold = 1 }
    hold { buf = buf $0 "\n"; if ($0 ~ /key: mdbook-/) { hold = 0; done = 1 } ; next }
    { print }
    /install-mdbook\.sh target\/mdbook-bin/ && buf != "" { printf "%s", buf; buf = "" }
  ' "$dir/docs-pages.yml" > "$dir/docs-pages.yml.tmp"
  mv "$dir/docs-pages.yml.tmp" "$dir/docs-pages.yml"
}

remove_mdbook_cache() {
  dir="$1"
  _deny="$2"
  awk '/^  [^ ]/ { job = $0 } job == "  docs:" && /^      - uses: actions\/cache@/ { skip = 4 }
    skip > 0 { skip--; next } { print }' \
    "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

empty_crate_dir_only() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -rf "$crates_dir"
  mkdir -p "$crates_dir/oce-empty"
}

remove_crate_lib_files() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -f "$crates_dir"/*/src/lib.rs
}

# Platform cells (issue #394) and the #418-review hardening: no continue-on-error anywhere, and no
# step-level if: beyond a static matrix selector or !cancelled().
job_continue_on_error() {
  edit_pr_gate "$1" 's/^  portable-snapshot-macos:$/  portable-snapshot-macos:\n    continue-on-error: true/'
}

step_continue_on_error() {
  edit_pr_gate "$1" '/^  build:$/,/^  [^ ]/ s/^      - run: cargo build --workspace --locked$/&\n        continue-on-error: true/'
}

release_job_continue_on_error() {
  edit_release_gate "$1" 's/^  std-build-windows:$/  std-build-windows:\n    continue-on-error: true/'
}

release_step_continue_on_error() {
  edit_release_gate "$1" '/^  std-build-windows:$/,/^  [^ ]/ s/^      - run: cargo build --workspace --locked$/&\n        continue-on-error: true/'
}

release_step_condition_on_event() {
  edit_release_gate "$1" \
    "s/^      - run: cargo test --workspace --doc --locked$/      - if: github.event_name != 'push'\n        run: cargo test --workspace --doc --locked/"
}

step_condition_on_lane_output() {
  edit_pr_gate "$1" \
    "s/^      - run: bash scripts\/ci\/test-classify-changes.sh$/      - if: needs.changes.outputs.core == 'true'\n        run: bash scripts\/ci\/test-classify-changes.sh/"
}

step_condition_false() {
  edit_pr_gate "$1" \
    '/^  build:$/,/^  [^ ]/ s/^      - run: cargo build --workspace --locked$/      - if: false\n        run: cargo build --workspace --locked/'
}

step_condition_on_event() {
  edit_pr_gate "$1" \
    "s/^      - run: cargo clippy --workspace/      - if: \${{ github.event_name != 'pull_request' }}\n        run: cargo clippy --workspace/"
}

step_condition_always() {
  edit_pr_gate "$1" \
    's/^      - run: test -s target\/strict-bits\/receipt-candidate.json$/      - if: always()\n        run: test -s target\/strict-bits\/receipt-candidate.json/'
}

matrix_selector_widened_by_or() {
  edit_release_gate "$1" \
    "s/^      - if: matrix.codegen == 'debug'$/      - if: matrix.codegen == 'debug' || github.event_name == 'schedule'/"
}

macos_on_intel_runner() {
  edit_pr_gate "$1" 's/^    runs-on: macos-15$/    runs-on: macos-13/'
}

macos_on_leverage_lane() {
  edit_pr_gate "$1" \
    -e '/^  portable-snapshot-macos:$/,/^    if:/ s/outputs.core/outputs.leverage/' \
    -e 's/portable-snapshot-macos=core/portable-snapshot-macos=leverage/'
}

macos_out_of_ci_ok() {
  edit_pr_gate "$1" '/^      - portable-snapshot-macos$/d'
}

compare_without_macos_cell() {
  edit_pr_gate "$1" 's/needs: \[changes, determinism-matrix, portable-snapshot-macos\]/needs: [changes, determinism-matrix]/'
}

macos_target_bound_difference_unchecked() {
  edit_pr_gate "$1" '/if cmp -s "\$linux\/target-state-debug.bin" macos-arm64\/target-state-debug.bin; then/d'
}

macos_portable_comparison_dropped() {
  edit_pr_gate "$1" '/^          cmp linux-x86_64\/portable-state-debug.bin macos-arm64\/portable-state-debug.bin$/d'
}

# The Windows cell lives in release-gate.yml (issue #502).
windows_strict_exclusion_dropped() {
  edit_release_gate "$1" "/^          -E 'not (binary_id(oce-conformance::strict_bits)/d"
}

windows_excludes_whole_conformance_crate() {
  edit_release_gate "$1" "s/^          -E 'not (binary_id(oce-conformance::strict_bits).*/          -E 'not package(oce-conformance)'/"
}

windows_on_linux_runner() {
  edit_release_gate "$1" 's/^    runs-on: windows-latest$/    runs-on: ubuntu-latest/'
}

windows_build_dropped() {
  edit_release_gate "$1" '/^  std-build-windows:$/,/^  [^ ]/ {/^      - run: cargo build --workspace --locked$/d;}'
}

windows_report_requirement_dropped() {
  edit_release_gate "$1" '/^  std-build-windows:$/,/^  [^ ]/ {/^      - run: test -s target\/nextest\/ci\/junit.xml$/d;}'
}

windows_out_of_release_ci_ok() {
  edit_release_gate "$1" 's/, determinism-compare, std-build-windows\]$/, determinism-compare]/'
}

windows_cell_removed() {
  edit_release_gate "$1" \
    -e '/^  std-build-windows:$/,/^  ci-ok:$/ { /^  ci-ok:$/!d; }' \
    -e 's/, determinism-compare, std-build-windows\]$/, determinism-compare]/'
}

# The Windows cell's suite line must not vouch for the full suite's.
release_full_suite_debug_run_dropped() {
  edit_release_gate "$1" '/^  test-suite:$/,/^  [^ ]/ {/^      - run: cargo nextest run --workspace --locked --profile ci --no-tests=fail$/d;}'
}

# The bare-metal cell (issues #397, #402).
no_std_check_script_dropped() {
  edit_pr_gate "$1" '/^      - run: bash scripts\/ci\/check-no-std.sh$/d'
}

no_std_controls_dropped() {
  edit_pr_gate "$1" '/^      - run: bash scripts\/ci\/test-check-no-std.sh$/d'
}

no_std_riscv_target_dropped() {
  edit_pr_gate "$1" \
    '/^  no-std-build:$/,/^  [^ ]/ s/ riscv32imac-unknown-none-elf wasm32-unknown-unknown$/ wasm32-unknown-unknown/'
}

no_std_targets_after_the_check() {
  dir="$1"
  awk '
    /^  no-std-build:$/ { in_job = 1 }
    in_job && /^  [^ ]/ && !/^  no-std-build:$/ { in_job = 0 }
    in_job && /rustup target add/ { held = $0; next }
    { print }
    in_job && /run: bash scripts\/ci\/check-no-std.sh$/ && held != "" { print held; held = "" }
  ' "$dir/pr-gate.yml" > "$dir/pr-gate.yml.tmp"
  mv "$dir/pr-gate.yml.tmp" "$dir/pr-gate.yml"
}

firmware_link_check_dropped() {
  edit_pr_gate "$1" '/^      - run: bash scripts\/ci\/check-firmware-link.sh$/d'
}

firmware_size_controls_dropped() {
  edit_pr_gate "$1" '/^      - run: python3 -I scripts\/ci\/test-firmware-size.py$/d'
}

firmware_host_run_dropped() {
  edit_pr_gate "$1" '/^      - run: cargo test -p oce-firmware-link --locked$/d'
}

no_std_build_out_of_lane_map() {
  edit_pr_gate "$1" '/^            no-std-build=core$/d'
}

release_gate_without_no_std_targets() {
  dir="$1"
  grep -v 'rustup target add thumbv7em-none-eabihf riscv32imac-unknown-none-elf wasm32-unknown-unknown' "$dir/release-gate.yml" \
    > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

release_gate_installs_targets_after_the_script() {
  dir="$1"
  awk '
    /rustup target add thumbv7em-none-eabihf riscv32imac-unknown-none-elf wasm32-unknown-unknown/ { held = $0; next }
    { print }
    /run: bash .agents\/gate.sh full/ && held != "" { print held; held = "" }
  ' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

no_std_build_out_of_ci_ok() {
  edit_pr_gate "$1" '/^      - no-std-build$/d'
}

# The blob round trip (issue #403): three legs from one recording, each of which can fail closed.
round_trip_recording_dropped() {
  edit_pr_gate "$1" '/^  determinism-matrix:$/,/^  [^ ]/ {/--test blob_round_trip --locked --profile ci --no-tests=fail$/d;}'
}

round_trip_recordings_not_compared() {
  edit_pr_gate "$1" '/diff -rq determinism-x86_64-debug\/round-trip determinism-aarch64-debug\/round-trip$/d'
}

arm64_replay_dropped() {
  edit_pr_gate "$1" '/export OCE_ROUND_TRIP_EXPECT=/d'
}

vacuous_arm64_replay() {
  edit_pr_gate "$1" 's/^            --workspace-remap \. --profile ci --no-tests=fail$/            --workspace-remap . --profile ci/'
}

wasm_leg_on_the_browser_target() {
  edit_pr_gate "$1" 's/--test blob_round_trip --target wasm32-wasip1 /--test blob_round_trip --target wasm32-unknown-unknown /'
}

wasm_leg_without_wasmtime() {
  edit_pr_gate "$1" '/^      - run: bash scripts\/ci\/install-wasmtime.sh target\/wasmtime-bin$/d'
}

wasm_leg_without_the_recording() {
  edit_pr_gate "$1" '/^  blob-round-trip-wasm:$/,/^  [^ ]/ {/name: determinism-x86_64-debug$/d;}'
}

wasm_leg_detached_from_the_matrix() {
  edit_pr_gate "$1" '/^  blob-round-trip-wasm:$/,/^  [^ ]/ s/^    needs: \[changes, determinism-matrix\]$/    needs: changes/'
}

wasm_leg_out_of_lane_map() {
  edit_pr_gate "$1" '/^            blob-round-trip-wasm=core$/d'
}

wasm_leg_out_of_ci_ok() {
  edit_pr_gate "$1" '/^      - blob-round-trip-wasm$/d'
}

release_gate_without_wasm_runtime() {
  dir="$1"
  grep -v 'rustup target add wasm32-wasip1' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

# The emulation check (issue #402) stays nightly, non-gating and pinned.
qemu_workflow_missing() {
  rm -f "$1/qemu.yml"
}

qemu_on_pull_request() {
  awk '{ print } /^  workflow_dispatch:$/ { print "  pull_request:" }' "$1/qemu.yml" > "$1/qemu.yml.tmp"
  mv "$1/qemu.yml.tmp" "$1/qemu.yml"
}

qemu_on_push_one_line() {
  # The block triggers stay, so only the one-line `on:` refusal can catch it.
  sed '1s/^on:$/on: [push]/' "$1/qemu.yml" > "$1/qemu.yml.tmp"
  mv "$1/qemu.yml.tmp" "$1/qemu.yml"
}

qemu_without_schedule() {
  sed '/^  schedule:$/d; /^    - cron:/d' "$1/qemu.yml" > "$1/qemu.yml.tmp"
  mv "$1/qemu.yml.tmp" "$1/qemu.yml"
}

qemu_unpinned_action() {
  sed 's/actions\/checkout@[0-9a-f]* # v4/actions\/checkout@v4/' "$1/qemu.yml" > "$1/qemu.yml.tmp"
  mv "$1/qemu.yml.tmp" "$1/qemu.yml"
}

qemu_without_the_script() {
  sed '/emulate-firmware.sh/d' "$1/qemu.yml" > "$1/qemu.yml.tmp"
  mv "$1/qemu.yml.tmp" "$1/qemu.yml"
}

# A firmware member (firmware/<name>) and the three ways its unsafe-code layer can lapse.
firmware_member() {
  local root="$1" manifest="$2" lib="$3" bin="$4"
  mkdir -p "$root/link/src/bin"
  printf '%b' "$manifest" > "$root/link/Cargo.toml"
  printf '%b' "$lib" > "$root/link/src/lib.rs"
  printf '%b' "$bin" > "$root/link/src/bin/image.rs"
}

compliant_firmware_member() {
  firmware_member "$(dirname "$1")/firmware" '[package]\nname = "oce-firmware-link"\n\n[lints]\nworkspace = true\n' \
    '#![forbid(unsafe_code)]\n#![no_std]\n' '//! Image.\n\n#![forbid(unsafe_code)]\n'
}

firmware_member_without_lints() {
  firmware_member "$(dirname "$1")/firmware" '[package]\nname = "oce-firmware-link"\n' \
    '#![forbid(unsafe_code)]\n' '#![forbid(unsafe_code)]\n'
}

firmware_lib_without_forbid() {
  firmware_member "$(dirname "$1")/firmware" '[package]\nname = "oce-firmware-link"\n\n[lints]\nworkspace = true\n' \
    '#![no_std]\n#![forbid(unsafe_code)]\n' '#![forbid(unsafe_code)]\n'
}

firmware_bin_without_forbid() {
  firmware_member "$(dirname "$1")/firmware" '[package]\nname = "oce-firmware-link"\n\n[lints]\nworkspace = true\n' \
    '#![forbid(unsafe_code)]\n' '//! Image.\n#![allow(unsafe_code)]\n'
}

remove_crates_dir() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -rf "$crates_dir"
}

run_case positive pass noop
run_case missing-release-nextest fail remove_release_nextest \
  "release-codegen nextest with hard-fail-on-zero-tests"
run_case seeded-advisory-ignore fail seed_advisory_ignore \
  "empty advisory ignore list"
run_case garbled-workflow fail garble_release_workflow \
  "scheduled heavy gate"
run_case missing-stale-status-gate fail remove_stale_status_gate \
  "run stale crate-status fixture tests"
run_case empty-bans-deny fail empty_bans_deny \
  "cargo-deny bans include representative SQL/ORM crate sqlx"
run_case reinstalled-gate-nightly fail reinstall_gate_nightly \
  "release gate installs no nightly toolchain"
run_case missing-golden-gen-firewall fail remove_golden_gen_firewall \
  "run golden-gen firewall fixture tests"
run_case missing-package-publication-contract fail remove_package_publication_contract \
  "run package publication hostile controls"
run_case no-op-authority-claim-check fail comment_authority_claim_check \
  "run authority claim consistency check"
run_case missing-authority-claim-controls fail remove_authority_claim_controls \
  "run authority claim hostile controls"
run_case missing-root-unsafe-forbid fail remove_root_unsafe_forbid \
  "workspace.lints.rust unsafe_code = \"forbid\""
run_case missing-crate-lints-workspace fail remove_crate_lints_workspace \
  "missing [lints] workspace = true"
run_case missing-crate-lib-forbid fail remove_crate_lib_forbid \
  "line 1 must be #![forbid(unsafe_code)]"
run_case missing-no-db-gate fail remove_no_db_gate \
  "run default-no-db smoke"
run_case missing-determinism-matrix fail remove_determinism_matrix \
  "debug determinism subset with hard-fail-on-zero-tests"
run_case commented-foreign-restore fail comment_foreign_restore_command \
  "restore and refuse the foreign target-bound snapshot"
run_case vacuous-foreign-restore fail allow_absent_foreign_restore_test \
  "foreign target-bound restore hard-fails when its test is absent"
run_case missing-foreign-nextest-install fail remove_foreign_restore_nextest_install \
  "pinned cargo-nextest install for foreign restore"
run_case missing-aarch64-runner fail remove_aarch64_leg \
  "determinism-matrix runs native x86_64 and aarch64 runners"
run_case aarch64-on-x86-runner fail swap_aarch64_runner \
  "does not map x86_64 to ubuntu-latest and aarch64 to ubuntu-24.04-arm"
run_case strict-bit-release-codegen-dropped fail drop_strict_bit_release_codegen \
  "strict-bit-matrix runs debug and release codegen"
run_case per-pr-release-codegen-cells-restored fail restore_per_pr_release_codegen \
  "pr-gate.yml job determinism-matrix is missing required gate pattern: determinism-matrix runs debug codegen only"
run_case missing-release-codegen-cells fail drop_release_gate_release_codegen \
  "release-gate.yml job determinism-matrix is missing required gate pattern: determinism-matrix runs debug and release codegen"
run_case release-gate-missing-aarch64-runner fail drop_release_gate_aarch64_runner \
  "release-gate.yml job determinism-matrix is missing required gate pattern: determinism-matrix runs native x86_64 and aarch64 runners"
run_case release-gate-fail-fast-cells fail release_gate_fail_fast \
  "release-gate.yml job determinism-matrix is missing required gate pattern: determinism-matrix reports every failing cell (fail-fast: false)"
run_case release-gate-without-determinism-matrix fail drop_release_gate_determinism_matrix \
  "release-gate.yml job determinism-matrix is missing required gate pattern: determinism-matrix runs on the matrix runner"
run_case release-gate-missing-release-subset fail drop_release_gate_release_subset \
  "release-gate.yml job determinism-matrix is missing required gate pattern: release determinism subset with hard-fail-on-zero-tests"
run_case release-gate-missing-release-round-trip fail drop_release_gate_release_round_trip \
  "release-gate.yml job determinism-matrix is missing required gate pattern: record the release blob round trip in every determinism cell"
run_case release-gate-cell-report-unchecked fail drop_release_gate_cell_report_requirement \
  "release-gate.yml job determinism-matrix is missing required gate pattern: refuse an absent or empty nextest determinism report"
run_case release-gate-detached-determinism-fan-in fail detach_release_gate_fan_in \
  "release-gate.yml job determinism-compare does not need determinism-matrix"
run_case release-gate-fan-in-out-of-ci-ok fail release_gate_fan_in_out_of_ci_ok \
  "release-gate.yml CI OK needs does not match the workflow's jobs"
run_case cross-codegen-state-comparison-dropped fail drop_cross_codegen_state_comparison \
  "release-gate.yml job determinism-compare is missing required gate pattern: compare aarch64 target-bound state bytes across codegen profiles"
run_case cross-codegen-blob-comparison-dropped fail drop_cross_codegen_blob_comparison \
  "release-gate.yml job determinism-compare is missing required gate pattern: compare aarch64 compiled-schedule blob bytes across codegen profiles"
run_case cross-codegen-round-trip-comparison-dropped fail drop_cross_codegen_round_trip_comparison \
  "release-gate.yml job determinism-compare is missing required gate pattern: compare the determinism-x86_64-debug and determinism-x86_64-release round-trip recordings"
run_case release-gate-fan-in-over-two-cells fail release_gate_fan_in_over_two_cells \
  "release-gate.yml job determinism-compare is missing required gate pattern: the fan-in holds the compiled-schedule blobs of all four cells"
run_case release-gate-cross-architecture-comparison-dropped fail drop_release_gate_cross_architecture_comparison \
  "release-gate.yml job determinism-compare is missing required gate pattern: require target-bound state bytes to differ across architectures"
run_case blob-architecture-comparison-dropped fail drop_blob_architecture_comparison \
  "pr-gate.yml job determinism-compare is missing required gate pattern: compare compiled-schedule blob bytes across architectures"
run_case blob-size-unchecked fail drop_blob_size_requirement \
  "refuse a compiled-schedule blob artifact that is not the 123329 bytes of the goldens"
run_case release-gate-without-development-push fail release_gate_without_development_push \
  "release-gate.yml push trigger does not name branches [main, development] (got: [main])"
run_case release-gate-without-push-trigger fail release_gate_without_push_trigger \
  "release-gate.yml push trigger does not name branches [main, development] (got: none)"
run_case release-push-checks-out-the-moving-ref fail release_push_checks_out_the_moving_ref \
  "a push run checks out the pushed commit in every job"
run_case release-checkout-without-the-pinned-ref fail release_checkout_without_the_pinned_ref \
  "release-gate.yml has a checkout that does not pin ref:"
run_case release-push-cancels-a-run-in-progress fail release_push_cancels_a_run_in_progress \
  "only a superseded release-PR run is cancelled; a push run in progress always finishes"
run_case release-runs-share-no-concurrency-group fail release_runs_share_no_concurrency_group \
  "one concurrency group per event and ref, so pushes to development queue behind each other"
run_case fail-fast-cells fail enable_fail_fast \
  "determinism-matrix reports every failing cell (fail-fast: false)"
run_case reinstalled-qemu fail reinstall_qemu \
  "pr-gate.yml mentions emulation or a cross-compiled aarch64 target"
run_case release-cross-target fail reinstall_release_cross_target \
  "release-gate.yml mentions emulation or a cross-compiled aarch64 target"
run_case detached-determinism-fan-in fail detach_fan_in \
  "job determinism-compare does not need determinism-matrix"
run_case omitted-strict-fan-in-need fail omit_strict_fan_in_need \
  "pr-gate.yml CI OK needs does not match the workflow's jobs"
run_case target-bound-difference-unchecked fail stop_requiring_target_bound_difference \
  "require target-bound state bytes to differ across architectures"
run_case own-architecture-restore fail restore_own_architecture_snapshot \
  "the restoring architecture differs from the snapshot architecture"
run_case unrun-matrix-comparison fail skip_ignored_matrix_comparison \
  "the eight-capture comparison hard-fails when its test is absent"
run_case missing-receipt-candidate fail drop_receipt_candidate_requirement \
  "a green comparison writes the candidate receipt-log record"
run_case missing-release-state-vector fail remove_release_state_vector \
  "release-gate.yml job determinism-matrix is missing required gate pattern: emit the release portable state vector in every determinism cell"
run_case conditional-ci-ok fail conditional_ci_ok \
  "CI OK aggregate reports on every run"
run_case blind-release-ci-ok fail blind_release_ci_ok \
  "release gate CI OK aggregate inspects every needed result"
run_case omitted-ci-ok-need fail omit_ci_ok_need \
  "pr-gate.yml CI OK needs does not match the workflow's jobs"
run_case omitted-release-ci-ok-need fail omit_release_ci_ok_need \
  "release-gate.yml CI OK needs does not match the workflow's jobs"
run_case conditional-gating-job fail conditional_gating_job \
  "pr-gate.yml job gate-fixtures has a job-level if: that is not a lane filter"
run_case conditional-release-job fail conditional_release_job \
  "release-gate.yml gating job has a job-level if: that CI OK would read as failure: unused-deps"
run_case deleted-ci-ok-result-check fail delete_ci_ok_result_check \
  "CI OK runs the lane-aware result check"
run_case success-only-result-check fail restore_success_only_result_check \
  "CI OK runs the lane-aware result check"
run_case missing-changes-core-output fail drop_changes_core_output \
  "changes job exports the core lane output"
run_case missing-changes-classifier fail drop_changes_classifier \
  "changes job classifies the changed paths into its outputs"
run_case shallow-changes-checkout fail shallow_changes_checkout \
  "changes job fetches the merge commit and its base parent"
run_case rename-blind-changes-diff fail rename_blind_diff \
  "renames as delete plus add"
run_case unlisted-lane-job fail unlisted_lane_job \
  "CI OK JOB_LANES does not match the lane-filtered jobs' if: lines"
run_case lane-map-wrong-lane fail lane_map_names_wrong_lane \
  "CI OK JOB_LANES does not match the lane-filtered jobs' if: lines"
run_case determinism-on-leverage-lane fail determinism_on_leverage_lane \
  "job determinism-matrix runs on lane leverage, not core"
run_case docs-made-optional fail docs_made_optional \
  "job docs runs on lane core, not always"
run_case lane-filter-without-changes-need fail lane_filter_without_changes_need \
  "job clippy does not need changes"
run_case lane-filter-on-docs-only-output fail lane_filter_on_docs_only_output \
  "job leverage-lane has a job-level if: that is not a lane filter"
run_case negated-lane-filter fail negated_lane_filter \
  "job build has a job-level if: that is not a lane filter"
run_case lane-filter-on-gate-scripts-output fail lane_filter_on_gate_scripts_output \
  "job facade-contract has a job-level if: that is not a lane filter: needs.changes.outputs.gate_scripts == 'true'"
run_case removed-leverage-lane fail removed_lane_job \
  "is missing the leverage-lane job"
run_case readme-job-out-of-lane-map fail readme_job_out_of_lane_map \
  "CI OK JOB_LANES does not match the lane-filtered jobs' if: lines"
run_case readme-job-on-core-lane fail readme_job_on_core_lane \
  "job facade-contract runs on lane core, not readme"
run_case readme-job-made-always-on fail readme_job_made_always_on \
  "job facade-contract runs on lane always, not readme"
run_case determinism-on-readme-lane fail determinism_on_readme_lane \
  "job determinism-matrix runs on lane readme, not core"
run_case missing-changes-readme-output fail drop_changes_readme_output \
  "changes job exports the readme lane output"
run_case readme-byte-pin-dropped fail drop_readme_byte_pin \
  "pr-gate.yml job facade-contract is missing required gate pattern: run the README Quickstart byte-pin on the README lane"
run_case readme-byte-pin-passes-on-zero-tests fail readme_byte_pin_passes_on_zero_tests \
  "pr-gate.yml job facade-contract is missing required gate pattern: run the README Quickstart byte-pin on the README lane"
run_case readme-job-without-nextest fail readme_job_without_nextest \
  "pr-gate.yml job facade-contract is missing required gate pattern: pinned cargo-nextest install for the README Quickstart byte-pin"
run_case readme-job-without-report-check fail readme_job_without_report_check \
  "pr-gate.yml job facade-contract is missing required gate pattern: the README lane refuses an absent or empty nextest report"
run_case quickstart-execution-dropped fail drop_quickstart_execution \
  "pr-gate.yml job facade-contract is missing required gate pattern: execute the Quickstart"
run_case leverage-crate-without-lints fail leverage_crate_without_lints \
  "leverage/oce-host/Cargo.toml is missing [lints] workspace = true"
run_case leverage-crate-without-forbid fail leverage_crate_without_forbid \
  "leverage/oce-host/src/lib.rs line 1 must be #![forbid(unsafe_code)]"
run_case compliant-leverage-crate pass compliant_leverage_crate
run_case restored-development-push-trigger fail restore_development_push_trigger \
  "pr-gate.yml has a push trigger; the per-PR gate must not re-run on merges into development"
run_case unfiltered-push-trigger fail unfiltered_push_trigger \
  "pr-gate.yml has a push trigger"
run_case flow-list-push-trigger fail flow_list_push_trigger \
  "pr-gate.yml has a push trigger"
run_case missing-pull-request-trigger fail remove_pull_request_trigger \
  "pr-gate.yml is missing its pull_request trigger"
run_case four-space-push-trigger fail four_space_push_trigger \
  "pr-gate.yml has a push trigger"
run_case four-space-pull-request-trigger pass four_space_pull_request_trigger
run_case block-list-push-trigger fail block_list_push_trigger \
  "pr-gate.yml has a push trigger"
run_case flow-map-push-trigger fail flow_map_push_trigger \
  "pr-gate.yml has a push trigger"
run_case flow-map-pull-request-trigger pass flow_map_pull_request_trigger
run_case hardcoded-strict-cell fail hardcode_strict_cell_architecture \
  "native strict-bit cell for the matrix architecture and codegen"
run_case missing-nextest-report-cleanup fail remove_nextest_report_cleanup \
  "clear cached nextest JUnit reports"
run_case missing-nextest-report-requirements fail remove_nextest_report_requirements \
  "pr-gate.yml job determinism-matrix is missing required gate pattern: refuse an absent or empty nextest determinism report"
run_case missing-release-nextest-report-requirements fail remove_release_nextest_report_requirements \
  "release gate requires the complete nextest report set"
run_case release-full-suite-debug-run-dropped fail release_full_suite_debug_run_dropped \
  "release-gate.yml job test-suite is missing required gate pattern: debug nextest with hard-fail-on-zero-tests"
run_case stale-nextest-pin fail downgrade_nextest_pin \
  "pinned cargo-nextest 0.9.143 install"
run_case tag-referenced-action fail unpin_action \
  "uses an action not pinned to a full commit SHA"
run_case missing-store-crate-suite fail remove_crate_suite \
  "run the oce-graph, oce-validate, oce-model, oce-cxf and oce-store suites per PR"
run_case crate-suite-allows-zero-tests fail allow_zero_crate_suite_tests \
  "run the oce-graph, oce-validate, oce-model, oce-cxf and oce-store suites per PR and hard-fail on zero tests"
run_case missing-docs-link-check fail remove_docs_link_check \
  "check generated internal links and anchors"
run_case missing-docs-checker-contracts fail remove_docs_checker_contracts \
  "run the documentation checker contracts"
run_case mdbook-cache-restore-keys fail mdbook_cache_restore_keys \
  "sets restore-keys on the mdBook cache"
run_case mdbook-cache-after-install fail mdbook_cache_after_install \
  "must restore the mdBook cache before the install step"
run_case missing-mdbook-cache fail remove_mdbook_cache \
  "restores the verified mdBook binary through a SHA-pinned actions/cache"
run_case empty-crate-dir-only fail empty_crate_dir_only \
  "no crate Cargo.toml files found under"
run_case missing-crate-lib-files fail remove_crate_lib_files \
  "no crate src/lib.rs files found under"
run_case missing-crates-dir fail remove_crates_dir \
  "crates directory is missing"

run_case job-continue-on-error fail job_continue_on_error \
  "contains a refused gate pattern: continue-on-error"
run_case step-continue-on-error fail step_continue_on_error \
  "contains a refused gate pattern: continue-on-error"
run_case release-job-continue-on-error fail release_job_continue_on_error \
  "release-gate.yml contains a refused gate pattern: continue-on-error"
run_case release-step-continue-on-error fail release_step_continue_on_error \
  "release-gate.yml contains a refused gate pattern: continue-on-error"
run_case release-step-condition-on-event fail release_step_condition_on_event \
  "test-suite: github.event_name != 'push'"
run_case step-condition-on-lane-output fail step_condition_on_lane_output \
  "gate-fixtures: needs.changes.outputs.core == 'true'"
run_case step-condition-false fail step_condition_false \
  "has a step-level if: other than a matrix selector or !cancelled()"
run_case step-condition-on-event fail step_condition_on_event \
  "clippy: \${{ github.event_name != 'pull_request' }}"
run_case step-condition-always fail step_condition_always \
  "strict-bit-compare: always()"
run_case matrix-selector-widened-by-or fail matrix_selector_widened_by_or \
  "determinism-matrix: matrix.codegen == 'debug' || github.event_name == 'schedule'"
run_case macos-on-intel-runner fail macos_on_intel_runner \
  "the macOS portable-snapshot cell runs on macos-15"
run_case macos-on-leverage-lane fail macos_on_leverage_lane \
  "job portable-snapshot-macos runs on lane leverage, not core"
run_case macos-out-of-ci-ok fail macos_out_of_ci_ok \
  "pr-gate.yml CI OK needs does not match the workflow's jobs"
run_case compare-without-macos-cell fail compare_without_macos_cell \
  "job portable-snapshot-compare does not need portable-snapshot-macos"
run_case macos-target-bound-difference-unchecked fail macos_target_bound_difference_unchecked \
  "require the macOS target-bound state bytes to differ from Linux"
run_case macos-portable-comparison-dropped fail macos_portable_comparison_dropped \
  "compare the macOS portable state bytes with Linux x86_64"
run_case windows-strict-exclusion-dropped fail windows_strict_exclusion_dropped \
  "the Windows cell excludes exactly the strict-bit binaries"
run_case windows-excludes-whole-conformance-crate fail windows_excludes_whole_conformance_crate \
  "the Windows cell excludes exactly the strict-bit binaries"
run_case windows-on-linux-runner fail windows_on_linux_runner \
  "the Windows cell runs on windows-latest"
run_case windows-build-dropped fail windows_build_dropped \
  "the Windows cell builds the workspace"
run_case windows-report-requirement-dropped fail windows_report_requirement_dropped \
  "the Windows cell refuses an absent or empty nextest report"
run_case windows-out-of-release-ci-ok fail windows_out_of_release_ci_ok \
  "release-gate.yml CI OK needs does not match the workflow's jobs"
run_case windows-cell-removed fail windows_cell_removed \
  "release-gate.yml job std-build-windows is missing required gate pattern: the Windows cell runs on windows-latest"
run_case no-std-check-script-dropped fail no_std_check_script_dropped \
  "build, lint and graph-scan the core four no_std + alloc on every target"
run_case no-std-controls-dropped fail no_std_controls_dropped \
  "prove the no_std check fails on std"
run_case no-std-riscv-target-dropped fail no_std_riscv_target_dropped \
  "install the no_std targets"
run_case no-std-targets-after-the-check fail no_std_targets_after_the_check \
  "the bare-metal cell installs its targets before building for them"
run_case firmware-link-check-dropped fail firmware_link_check_dropped \
  "build the firmware image for each bare-metal target within its size budget"
run_case firmware-size-controls-dropped fail firmware_size_controls_dropped \
  "run the firmware size-check controls"
run_case firmware-host-run-dropped fail firmware_host_run_dropped \
  "run the firmware image on the host against every golden"
run_case qemu-workflow-missing fail qemu_workflow_missing \
  "required gate file is missing or empty"
run_case qemu-on-pull-request fail qemu_on_pull_request \
  "the emulation run is nightly and non-gating"
run_case qemu-on-push-one-line fail qemu_on_push_one_line \
  "the emulation run is nightly and non-gating"
run_case qemu-without-schedule fail qemu_without_schedule \
  "scheduled emulation run"
run_case qemu-unpinned-action fail qemu_unpinned_action \
  "uses an action not pinned to a full commit SHA"
run_case qemu-without-the-script fail qemu_without_the_script \
  "the emulation run executes emulate-firmware.sh"
run_case compliant-firmware-member pass compliant_firmware_member
run_case firmware-member-without-lints fail firmware_member_without_lints \
  "is missing [lints] workspace = true"
run_case firmware-lib-without-forbid fail firmware_lib_without_forbid \
  "line 1 must be #![forbid(unsafe_code)]"
run_case firmware-bin-without-forbid fail firmware_bin_without_forbid \
  "must carry #![forbid(unsafe_code)]"
run_case no-std-build-out-of-lane-map fail no_std_build_out_of_lane_map \
  "CI OK JOB_LANES does not match the lane-filtered jobs' if: lines"
run_case no-std-build-out-of-ci-ok fail no_std_build_out_of_ci_ok \
  "pr-gate.yml CI OK needs does not match the workflow's jobs"
run_case release-gate-without-no-std-targets fail release_gate_without_no_std_targets \
  "the executed full gate script has the no_std targets it builds for"
run_case release-gate-installs-targets-after-the-script fail release_gate_installs_targets_after_the_script \
  "the executed full gate script has the no_std targets it builds for"
run_case round-trip-recording-dropped fail round_trip_recording_dropped \
  "record the debug blob round trip in every determinism cell"
run_case round-trip-recordings-not-compared fail round_trip_recordings_not_compared \
  "compare the determinism-x86_64-debug and determinism-aarch64-debug round-trip recordings"
run_case arm64-replay-dropped fail arm64_replay_dropped \
  "replay the x86_64 debug round-trip recording on each architecture"
run_case vacuous-arm64-replay fail vacuous_arm64_replay \
  "the round-trip replay hard-fails when its suite is absent"
run_case wasm-leg-on-the-browser-target fail wasm_leg_on_the_browser_target \
  "the wasm32 leg runs the round trip on wasm32-wasip1 with hard-fail-on-zero-tests"
run_case wasm-leg-without-wasmtime fail wasm_leg_without_wasmtime \
  "the wasm32 leg installs the checksum-pinned wasmtime"
run_case wasm-leg-without-the-recording fail wasm_leg_without_the_recording \
  "the wasm32 leg downloads the x86_64 debug recording"
run_case wasm-leg-detached-from-the-matrix fail wasm_leg_detached_from_the_matrix \
  "job blob-round-trip-wasm does not need determinism-matrix"
run_case wasm-leg-out-of-lane-map fail wasm_leg_out_of_lane_map \
  "CI OK JOB_LANES does not match the lane-filtered jobs' if: lines"
run_case wasm-leg-out-of-ci-ok fail wasm_leg_out_of_ci_ok \
  "pr-gate.yml CI OK needs does not match the workflow's jobs"
run_case release-gate-without-wasm-runtime fail release_gate_without_wasm_runtime \
  "the executed full gate script has the wasm32-wasip1 target its round trip runs on"

echo "OK: workflow gate fixtures passed."
