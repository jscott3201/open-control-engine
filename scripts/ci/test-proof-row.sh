#!/usr/bin/env bash
# Fixtures for proof-row.sh (issue #545): each case feeds a changed-path list and compares the row
# it names. The controls are the directions that would let an author paste a lighter proof than
# the change needs: a pin advance or a gate-script change read as anything but `full`, an engine
# path read as leverage or docs, and an uncomputable diff read as anything but the heaviest row.
# Since issue #556 there is also the opposite control: nothing is read as `determinism`.

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

# Still row 3 (issue #556): what the release-codegen determinism subset builds or runs. These
# owed a `determinism` row from issue #545 until pr-gate.yml ran the release cells again; the
# control is that the script never names that mode now, for any of them.
expect oce-model light 'crates/oce-model/src/lib.rs'
expect oce-expr light 'crates/oce-expr/src/eval.rs'
expect oce-blocks light 'crates/oce-blocks/src/reals.rs'
expect oce-graph light 'crates/oce-graph/src/blob.rs'
expect oce-cxf light 'crates/oce-cxf/src/lib.rs'
expect facade light 'crates/oce-api/src/lib.rs'
expect facade-tests light 'crates/oce-api/tests/blob_goldens.rs'
expect core-crate-test-fixture light 'crates/oce-graph/tests/fixtures/blob_frame_digests.tsv'
expect core-crate-manifest light 'crates/oce-blocks/Cargo.toml'
expect core-beside-docs light $'docs/architecture.md\ncrates/oce-expr/src/lib.rs'
expect core-beside-leverage light $'crates/leverage/oce-cli/src/lib.rs\ncrates/oce-cxf/src/lib.rs\nREADME.md'
expect core-listed-last light $'crates/oce-diag/src/lib.rs\nCHANGELOG.md\ncrates/oce-model/src/lib.rs'
# The files that decide how all of them are compiled.
expect lockfile light 'Cargo.lock'
expect root-manifest light 'Cargo.toml'
expect toolchain-pin light 'rust-toolchain.toml'
expect cargo-config light '.cargo/config.toml'
expect lockfile-beside-leverage light $'crates/leverage/oce-host/Cargo.toml\nCargo.lock'

# Row 4: what only the workspace suite or `full` mode itself checks before a merge. A pin
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
expect other-registry-file light 'crates/oce-blocks/src/registry/mod.rs'

# Fail-closed: a diff that could not be computed, and a path git had to quote.
expect empty-input full ''
expect blank-lines-only full $'\n\n'
expect quoted-path full '"crates/oce-bl\303\266cks/src/lib.rs"'
expect quoted-docs-path full '"docs/caf\303\251.md"'

# No trailing newline on the last path.
expect unterminated-last-line light $'CHANGELOG.md\ncrates/oce-cxf/src/lib.rs'

# No input names `determinism`: every row above was one of docs, leverage, light or full, and the
# script holds no other name.
if grep -q "row=determinism" "$ROW"; then
  echo "FAIL: proof-row can still name the determinism row"
  failures=$((failures + 1))
fi

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
expect_list FULL_PREFIXES 'third_party/ tools/reference-catalog/ crates/oce-cxf/tests/third_party_manifest/'
expect_list FULL_FILES 'crates/oce-blocks/src/registry/catalog_guard_support.rs .agents/gate.sh'
for path in third_party tools/reference-catalog crates/oce-cxf/tests/third_party_manifest \
  crates/oce-blocks/src/registry/catalog_guard_support.rs .agents/gate.sh; do
  if [ ! -e "$path" ]; then
    echo "FAIL: proof-row names $path, which does not exist"
    failures=$((failures + 1))
  fi
done
# The two source files in the row each hold a pinned upstream commit; if a pin leaves one of
# them, this fails until the row follows it.
for path in crates/oce-cxf/tests/third_party_manifest/mod.rs \
  crates/oce-blocks/src/registry/catalog_guard_support.rs; do
  if ! grep -Eq 'const [A-Z_]*COMMIT: &str = "[0-9a-f]{40}"' "$path"; then
    echo "FAIL: $path no longer holds a pinned upstream commit; update proof-row's full row"
    failures=$((failures + 1))
  fi
done

# Every tracked file that writes a pinned upstream commit, exactly. The documents say the row's
# two files are not the only ones and name the rest; a pin constant added to a new file, or
# removed from one, fails here until this list and that prose are brought back in step.
want_pin_files='crates/leverage/oce-vocab/tests/vocabulary/generator/mod.rs
crates/oce-blocks/src/registry/catalog_guard_support.rs
crates/oce-cxf/src/g36_catalog_guard_data.rs
crates/oce-cxf/tests/third_party_manifest/mod.rs
tools/golden-gen/src/logical.rs
tools/golden-gen/src/logical_pre.rs
tools/golden-gen/src/logical_proof.rs
tools/golden-gen/src/sequences/provenance.rs
tools/golden-gen/src/utilities_assert.rs'
got_pin_files="$(git grep -lE 'const [A-Z_]*COMMIT: &str = "[0-9a-f]{40}"' -- crates tools firmware | LC_ALL=C sort)"
if [ "$got_pin_files" != "$want_pin_files" ]; then
  echo "FAIL: the files that hold a pinned upstream commit changed; update this list and the prose that names them"
  diff <(printf '%s\n' "$want_pin_files") <(printf '%s\n' "$got_pin_files") | sed 's/^/  /' || true
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures proof-row fixture(s) failed"
  exit 1
fi
echo "OK: proof-row fixtures passed."
