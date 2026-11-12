#!/usr/bin/env bash
# Download the registry crates of every tool workspace a test builds offline, so those tests pass
# on a cold cargo cache (GitHub issue #610; the release.yml verify run of tag v0.5.0, 43129183575,
# failed 15 tests on it).
#
# WHY. Three oce-cxf integration test modules build a tool workspace under tools/ with
# `cargo run --offline --locked` (the OpenModelica evidence checks of `fixture_structural_oracle`),
# directly or through the tool's own assemble.sh and verify_evidence.py. Each tool is a cargo
# workspace of its own with its own Cargo.lock. The root workspace's build fetches nothing for it,
# and every cache this repository keys (rust-cache's key hashes the root Cargo.lock and the
# members' Cargo.toml files, and its save prunes the registry to the root workspace's crates) can
# lack the tool's crates, while release.yml uses no cache at all. Two of the tools pin
# `rustix 1.1.4` while the root moved to 1.1.6 (GitHub #605), so from that change on only a cache
# saved earlier still held it. Their lockfile digests are bound into recorded OpenModelica
# evidence, so the bump itself waits on issue #604; this prefetch is what keeps the tests
# independent of the cache until then, and after.
#
# WHERE. Every job that runs those tests on a runner other than Windows (they are #[cfg(unix)])
# runs this script before them, and .agents/gate.sh runs it before its first oce-cxf step.
# .github/scripts/check-workflow-gates.sh refuses a job, or a gate script, that runs the oce-cxf or
# the whole-workspace suite without it.
#
# THE LIST. Derived from the tests (`--offline` in crates/oce-cxf/tests/open_modelica_*). A new
# test that builds another tool workspace offline adds it here; without it the test passes on a
# warm cache and fails on a cold one. `--list` prints the list.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

TOOLS=(
  # open_modelica_line_reference: assembly_boundary_tests (cargo run), and line/assemble.sh and
  # line/verify_evidence.py through the verifier tests.
  tools/openmodelica-line-reference
  # open_modelica_reliefs_reference: reliefs/verify_evidence.py and reliefs/assemble.sh.
  tools/openmodelica-reliefs-reference
  # open_modelica_toggle_reference: its keep-first mutation control (cargo run). No registry crate
  # today, so its fetch downloads nothing; it is listed because the test builds it offline.
  tools/openmodelica-toggle-reference
)

case "${1:-}" in
  --list)
    printf '%s\n' "${TOOLS[@]}"
    exit 0
    ;;
  "") ;;
  *)
    echo "usage: bash scripts/ci/prefetch-offline-tools.sh [--list]" >&2
    exit 2
    ;;
esac

for tool in "${TOOLS[@]}"; do
  if [ ! -s "$tool/Cargo.toml" ] || [ ! -s "$tool/Cargo.lock" ]; then
    echo "FAIL: $tool has no Cargo.toml or Cargo.lock to prefetch" >&2
    exit 1
  fi
  echo "prefetch: $tool"
  cargo fetch --locked --manifest-path "$tool/Cargo.toml"
done
