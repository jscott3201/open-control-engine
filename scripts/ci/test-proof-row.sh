#!/usr/bin/env bash
# Fixtures for proof-row.sh (issue #545): each case feeds a changed-path list and compares the row
# it names. The controls are the directions that would let an author paste a lighter proof than
# the change needs: a core-four or oce-cxf path read as the engine row without release codegen,
# an engine path read as leverage or docs, and an uncomputable diff read as anything but the
# heaviest row.

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
expect facade light 'crates/oce-api/src/lib.rs'
expect facade-tests light 'crates/oce-api/tests/blob_goldens.rs'
expect gate-script light '.agents/gate.sh'
expect workflow light '.github/workflows/merge-gate.yml'
expect ci-script light 'scripts/ci/proof-row.sh'
expect tool-config light '.config/nextest.toml'
expect lockfile light 'Cargo.lock'
expect root-manifest light 'Cargo.toml'
expect firmware-image light 'firmware/link/src/lib.rs'
expect other-engine-crates light $'crates/oce-validate/src/lib.rs\ncrates/oce-store/src/lib.rs\ncrates/oce-conformance/tests/strict_bits/main.rs'
expect vendored-source light 'third_party/modelica-buildings-cdl/README.md'
expect golden-generator light 'tools/golden-gen/src/main.rs'
expect test-read-doc light 'docs/design/vectors.md'
expect unknown-root-file light 'LICENSE-MIT'
expect engine-plus-leverage-and-docs light $'crates/oce-api/src/lib.rs\ncrates/leverage/oce-cli/src/lib.rs\nREADME.md'

# Row 4: the core four or oce-cxf, alone or beside anything else.
expect oce-model determinism 'crates/oce-model/src/lib.rs'
expect oce-expr determinism 'crates/oce-expr/src/eval.rs'
expect oce-blocks determinism 'crates/oce-blocks/src/reals.rs'
expect oce-graph determinism 'crates/oce-graph/src/blob.rs'
expect oce-cxf determinism 'crates/oce-cxf/src/lib.rs'
expect core-crate-test-fixture determinism 'crates/oce-graph/tests/fixtures/blob_frame_digests.tsv'
expect core-crate-manifest determinism 'crates/oce-blocks/Cargo.toml'
expect core-beside-docs determinism $'docs/architecture.md\ncrates/oce-expr/src/lib.rs'
expect core-beside-leverage determinism $'crates/leverage/oce-cli/src/lib.rs\ncrates/oce-cxf/src/lib.rs\nREADME.md'
expect core-listed-last determinism $'crates/oce-api/src/lib.rs\nCHANGELOG.md\ncrates/oce-model/src/lib.rs'

# Look-alikes keep their own row: a crate whose name only starts like a core crate, a leverage
# crate, and a path that merely contains a core crate's name.
expect core-name-prefix light 'crates/oce-graphviz/src/lib.rs'
expect core-name-in-leverage leverage 'crates/leverage/oce-cxf/src/lib.rs'
expect core-name-in-docs docs 'docs/crates/oce-blocks/notes.md'

# Fail-closed: a diff that could not be computed, and a path git had to quote.
expect empty-input determinism ''
expect blank-lines-only determinism $'\n\n'
expect quoted-path determinism '"crates/oce-bl\303\266cks/src/lib.rs"'
expect quoted-docs-path determinism '"docs/caf\303\251.md"'

# No trailing newline on the last path.
expect unterminated-last-line determinism $'CHANGELOG.md\ncrates/oce-cxf/src/lib.rs'

# Arguments are refused: the paths come on stdin only.
if bash "$ROW" --all < /dev/null > /dev/null 2>&1; then
  echo "FAIL: proof-row accepted an argument"
  failures=$((failures + 1))
fi

# The crate list is the table's, exactly: the core four and oce-cxf.
want_crates='oce-model oce-expr oce-blocks oce-graph oce-cxf'
got_crates="$(sed -n 's/^RELEASE_CODEGEN_CRATES=(\(.*\))$/\1/p' "$ROW")"
if [ "$got_crates" != "$want_crates" ]; then
  echo "FAIL: proof-row's release-codegen crate list is '$got_crates', not '$want_crates'"
  failures=$((failures + 1))
fi
for crate in $want_crates; do
  if [ ! -f "crates/$crate/Cargo.toml" ]; then
    echo "FAIL: proof-row names crates/$crate, which is not a crate in this workspace"
    failures=$((failures + 1))
  fi
done

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures proof-row fixture(s) failed"
  exit 1
fi
echo "OK: proof-row fixtures passed."
