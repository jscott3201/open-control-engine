#!/usr/bin/env bash
# Fixtures for proof-row.sh (issue #545): each case feeds a changed-path list and compares the row
# it names. The controls are the directions that would let an author paste a lighter proof than
# the change needs: a pin advance or a gate-script change read as anything but `full`, a path the
# release determinism subset builds read as the engine row without release codegen, an engine
# path read as leverage or docs, and an uncomputable diff read as anything but the heaviest row.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

ROW="scripts/ci/proof-row.sh"
failures=0

expect() {
  local name="$1" want="$2" paths="$3" got
  got="$(printf '%s' "$paths" | bash "$ROW" 2>/dev/null || echo "exit=$?")"
  if [ "$got" != "row=$want" ]; then
    echo "FAIL: proof-row case '$name'"
    echo "  want: row=$want"
    echo "  got:  $got"
    failures=$((failures + 1))
  fi
}

# Row 1: documentation no lane-filtered job reads.
expect docs-page docs 'docs/ci-and-the-gate.md'
expect root-markdown docs $'AGENTS.md\nTESTING.md\nCHANGELOG.md\nCONTRIBUTING.md'
expect agent-facts-and-pr-template docs $'.agents/project-facts.md\n.github/pull_request_template.md'

# Row 2: crates/leverage/**, documentation and the root README.md, in any combination that
# includes a non-docs path.
expect leverage-crate leverage 'crates/leverage/oce-cli/src/lib.rs'
expect leverage-plus-docs leverage $'crates/leverage/oce-cli/src/lib.rs\ndocs/ci-and-the-gate.md'
expect readme-alone leverage 'README.md'
expect readme-plus-docs leverage $'README.md\nCHANGELOG.md'
expect readme-plus-leverage leverage $'README.md\ncrates/leverage/oce-author/src/lib.rs\nCHANGELOG.md'

# Row 3: the engine lane without release codegen.
expect workflow light '.github/workflows/merge-gate.yml'
expect release-workflow light '.github/workflows/release-gate.yml'
expect ci-script light 'scripts/ci/proof-row.sh'
expect checker light '.github/scripts/check-workflow-gates.sh'
expect tool-config light '.config/nextest.toml'
expect firmware-image light 'firmware/link/src/lib.rs'
expect other-engine-crates light $'crates/oce-validate/src/lib.rs\ncrates/oce-store/src/lib.rs\ncrates/oce-conformance/tests/strict_bits/main.rs'
expect golden-generator light 'tools/golden-gen/src/main.rs'
expect test-read-doc light 'docs/design/vectors.md'
expect unknown-root-file light 'LICENSE-MIT'
expect deny-config light 'deny.toml'
expect engine-plus-leverage-and-docs light $'crates/oce-diag/src/lib.rs\ncrates/leverage/oce-cli/src/lib.rs\nREADME.md'

# Row 4: what the release-codegen determinism subset builds or runs, alone or beside anything
# lighter. The core four, oce-cxf and oce-api, tests and fixtures included.
expect oce-model determinism 'crates/oce-model/src/lib.rs'
expect oce-expr determinism 'crates/oce-expr/src/eval.rs'
expect oce-blocks determinism 'crates/oce-blocks/src/reals.rs'
expect oce-graph determinism 'crates/oce-graph/src/blob.rs'
expect oce-cxf determinism 'crates/oce-cxf/src/lib.rs'
expect facade determinism 'crates/oce-api/src/lib.rs'
expect facade-tests determinism 'crates/oce-api/tests/blob_goldens.rs'
expect core-crate-test-fixture determinism 'crates/oce-graph/tests/fixtures/blob_frame_digests.tsv'
expect core-crate-manifest determinism 'crates/oce-blocks/Cargo.toml'
expect core-beside-docs determinism $'docs/architecture.md\ncrates/oce-expr/src/lib.rs'
expect core-beside-leverage determinism $'crates/leverage/oce-cli/src/lib.rs\ncrates/oce-cxf/src/lib.rs\nREADME.md'
expect core-listed-last determinism $'crates/oce-diag/src/lib.rs\nCHANGELOG.md\ncrates/oce-model/src/lib.rs'
# The files that decide how all of them are compiled.
expect lockfile determinism 'Cargo.lock'
expect root-manifest determinism 'Cargo.toml'
expect toolchain-pin determinism 'rust-toolchain.toml'
expect cargo-config determinism '.cargo/config.toml'
expect lockfile-beside-leverage determinism $'crates/leverage/oce-host/Cargo.toml\nCargo.lock'

# Row 5: what only the workspace suite or `full` mode itself checks before a merge. A pin
# advance, by every file one moves, and the gate script.
expect vendored-source full 'third_party/modelica-buildings-cdl/README.md'
expect vendored-class full 'third_party/modelica-buildings-cdl/Buildings/Controls/OBC/CDL/Reals/Add.mo'
expect hash-manifest full 'tools/reference-catalog/modelica-buildings-cdl.hash-manifest.json'
expect g36-provenance-record full 'tools/reference-catalog/Buildings.Controls.OBC.ASHRAE.G36.prov.json'
expect manifest-pin-constants full 'crates/oce-cxf/tests/third_party_manifest/mod.rs'
expect reference-commit-constant full 'crates/oce-blocks/src/registry/catalog_guard_support.rs'
expect gate-script full '.agents/gate.sh'
expect gate-script-beside-docs full $'docs/ci-and-the-gate.md\n.agents/gate.sh'
expect vendored-beside-core full $'crates/oce-cxf/src/lib.rs\nthird_party/modelica-buildings-cdl/cxf/a.json'
expect vendored-listed-last full $'CHANGELOG.md\ncrates/leverage/oce-vocab/data/generated.json\nthird_party/modelica-buildings-cdl/README.md'

# Look-alikes keep their own row: a crate whose name only starts like a listed crate, a leverage
# crate, a path that merely contains a listed name, and a manifest or lockfile that is not the
# root one.
expect core-name-prefix light 'crates/oce-graphviz/src/lib.rs'
expect facade-name-prefix light 'crates/oce-api-extras/src/lib.rs'
expect core-name-in-leverage leverage 'crates/leverage/oce-cxf/src/lib.rs'
expect core-name-in-docs docs 'docs/crates/oce-blocks/notes.md'
expect nested-manifest light 'crates/oce-validate/Cargo.toml'
expect golden-gen-lockfile light 'tools/golden-gen/Cargo.lock'
expect leverage-manifest leverage 'crates/leverage/oce-cli/Cargo.toml'
expect other-agents-file docs '.agents/project-facts.md'
expect vendored-name-in-docs docs 'docs/third_party/notes.md'
expect other-tools-directory light 'tools/reference-catalogue/a.json'
expect other-registry-file determinism 'crates/oce-blocks/src/registry/mod.rs'

# Fail-closed: a diff that could not be computed, and a path git had to quote.
expect empty-input full ''
expect blank-lines-only full $'\n\n'
expect quoted-path full '"crates/oce-bl\303\266cks/src/lib.rs"'
expect quoted-docs-path full '"docs/caf\303\251.md"'

# No trailing newline on the last path.
expect unterminated-last-line determinism $'CHANGELOG.md\ncrates/oce-cxf/src/lib.rs'

# Arguments are refused: the paths come on stdin only.
if bash "$ROW" --all < /dev/null > /dev/null 2>&1; then
  echo "FAIL: proof-row accepted an argument"
  failures=$((failures + 1))
fi

# The lists are the table's, exactly, and every entry exists: a renamed crate or a moved pin
# file must not leave its row silently empty.
expect_list() {
  local name="$1" want="$2" got
  got="$(sed -n "s/^$name=(\\(.*\\))\$/\\1/p" "$ROW")"
  if [ "$got" != "$want" ]; then
    echo "FAIL: proof-row's $name is '$got', not '$want'"
    failures=$((failures + 1))
  fi
}
expect_list RELEASE_CODEGEN_CRATES 'oce-model oce-expr oce-blocks oce-graph oce-cxf oce-api'
expect_list RELEASE_CODEGEN_FILES 'Cargo.lock Cargo.toml rust-toolchain.toml .cargo/config.toml'
expect_list FULL_PREFIXES 'third_party/ tools/reference-catalog/ crates/oce-cxf/tests/third_party_manifest/'
expect_list FULL_FILES 'crates/oce-blocks/src/registry/catalog_guard_support.rs .agents/gate.sh'
for crate in oce-model oce-expr oce-blocks oce-graph oce-cxf oce-api; do
  if [ ! -f "crates/$crate/Cargo.toml" ]; then
    echo "FAIL: proof-row names crates/$crate, which is not a crate in this workspace"
    failures=$((failures + 1))
  fi
done
for path in Cargo.lock Cargo.toml rust-toolchain.toml .cargo/config.toml \
  third_party tools/reference-catalog crates/oce-cxf/tests/third_party_manifest \
  crates/oce-blocks/src/registry/catalog_guard_support.rs .agents/gate.sh; do
  if [ ! -e "$path" ]; then
    echo "FAIL: proof-row names $path, which does not exist"
    failures=$((failures + 1))
  fi
done
# The two source files are listed because they hold the pinned upstream commits; if a pin moves
# to another file, this fails until the list follows it.
for path in crates/oce-cxf/tests/third_party_manifest/mod.rs \
  crates/oce-blocks/src/registry/catalog_guard_support.rs; do
  if ! grep -Eq 'const [A-Z_]*COMMIT: &str = "[0-9a-f]{40}"' "$path"; then
    echo "FAIL: $path no longer holds a pinned upstream commit; update proof-row's full row"
    failures=$((failures + 1))
  fi
done

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures proof-row fixture(s) failed"
  exit 1
fi
echo "OK: proof-row fixtures passed."
