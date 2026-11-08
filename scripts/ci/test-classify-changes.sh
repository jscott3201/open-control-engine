#!/usr/bin/env bash
# Fixtures for classify-changes.sh, plus a guard that every documentation file a test reads is
# excluded from the docs-only class. Each case feeds a changed-path list and compares all four
# outputs, so a rule that widens (a core path read as docs-only or leverage-only) fails here.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

CLASSIFY="scripts/ci/classify-changes.sh"
failures=0

expect() {
  local name="$1" want="$2" paths="$3" got
  got="$(printf '%s' "$paths" | bash "$CLASSIFY" 2>/dev/null | tr '\n' ' ')"
  if [ "$got" != "$want" ]; then
    echo "FAIL: classify case '$name'"
    echo "  want: $want"
    echo "  got:  $got"
    failures=$((failures + 1))
  fi
}

all='core=true leverage=true gate_scripts=true docs_only=false '
core='core=true leverage=true gate_scripts=false docs_only=false '
leverage='core=false leverage=true gate_scripts=false docs_only=false '
docs='core=false leverage=false gate_scripts=false docs_only=true '

expect leverage-readme "$leverage" 'crates/leverage/README.md'
expect leverage-crate-source "$leverage" $'crates/leverage/oce-host/src/lib.rs\ncrates/leverage/oce-host/Cargo.toml'
expect leverage-plus-docs "$leverage" $'crates/leverage/oce-host/src/lib.rs\ndocs/ci-and-the-gate.md'
expect docs-page "$docs" 'docs/ci-and-the-gate.md'
expect docs-site-and-tools "$docs" $'site/index.html\nscripts/docs/check_links.py\n.github/ISSUE_TEMPLATE/bug.md'
expect root-markdown "$docs" $'AGENTS.md\nTESTING.md\nCHANGELOG.md\nCONTRIBUTING.md\nSECURITY.md'
expect agent-facts-and-pr-template "$docs" $'.agents/project-facts.md\n.github/pull_request_template.md'
# Documentation that a test reads is engine input, not docs.
expect readme-quickstart "$core" 'README.md'
expect composite-contract-doc "$core" 'docs/cxf-composite-subset.md'
# Engine paths: every one runs the determinism lane, and with it the leverage lane.
expect core-four "$core" 'crates/oce-blocks/src/lib.rs'
expect cxf "$core" 'crates/oce-cxf/tests/fixtures/profile/a.json'
expect lockfile "$core" 'Cargo.lock'
expect root-manifest "$core" 'Cargo.toml'
expect toolchain "$core" 'rust-toolchain.toml'
expect cargo-config "$core" '.cargo/config.toml'
expect deny-config "$core" 'deny.toml'
expect golden-generator "$core" 'tools/golden-gen/goldens/CDL/a.json'
expect vendored-source "$core" 'third_party/modelica-buildings-cdl/README.md'
expect git-hooks "$core" '.githooks/pre-commit'
expect git-attributes "$core" '.gitattributes'
expect nested-markdown "$core" 'crates/oce-api/README.md'
expect agent-notes-other-than-facts "$core" '.agents/revendor-upstream.sh'
expect unknown-root-file "$core" 'LICENSE-MIT'
expect leverage-plus-core "$core" $'crates/leverage/README.md\ncrates/oce-api/src/lib.rs'
expect docs-plus-core "$core" $'docs/architecture.md\ncrates/oce-graph/src/lib.rs'
expect quoted-path "$core" '"docs/caf\303\251.md"'
expect leverage-prefix-lookalike "$core" 'crates/leverage-tools/src/lib.rs'
expect docs-prefix-lookalike "$core" 'docsx/a.md'
# Gate scripts imply the whole determinism lane.
expect gate-script "$all" '.agents/gate.sh'
expect ci-script "$all" '.github/scripts/check-file-size.sh'
expect workflow "$all" '.github/workflows/pr-gate.yml'
expect frozen-workflow "$all" '.github/workflows/ci.yml'
expect repo-script "$all" 'scripts/ci/classify-changes.sh'
expect package-policy "$all" 'scripts/package_policy/validate.py'
expect nextest-config "$all" '.config/nextest.toml'
expect gate-script-with-docs "$all" $'.agents/gate.sh\ndocs/a.md'
# Fail closed: nothing to classify runs everything.
expect empty-input "$all" ''
expect blank-lines "$all" $'\n\n'
expect no-trailing-newline "$docs" 'docs/a.md'

got="$(bash "$CLASSIFY" --all < /dev/null | tr '\n' ' ')"
[ "$got" = "$all" ] || { echo "FAIL: --all did not select every lane: $got"; failures=$((failures + 1)); }
if bash "$CLASSIFY" unexpected < /dev/null > /dev/null 2>&1; then
  echo "FAIL: an unknown argument was accepted"
  failures=$((failures + 1))
fi

# Every documentation path a crate or tool reads must be in TEST_READ_DOCS. The scan finds the two
# path constructions used today — `../../docs/...` (include_str!/concat!) and `.join("<file>")`
# from the repository root — in tracked Rust sources. A new construction is invisible to it, so
# the classifier's comment names the list as the place to add one.
listed="$(sed -n '/^TEST_READ_DOCS=(/,/^)/p' "$CLASSIFY" | sed '1d;$d' | sed 's/#.*//' | tr -s ' ' '\n' | sed '/^$/d' | sort -u)"
[ -n "$listed" ] || { echo "FAIL: TEST_READ_DOCS is empty or unparsable"; failures=$((failures + 1)); }
read_docs="$(git ls-files 'crates/*.rs' 'tools/*.rs' \
  | xargs grep -hoE '(\.\./)+(docs|site)/[A-Za-z0-9_./-]+|join\("(docs/[A-Za-z0-9_./-]+|site/[A-Za-z0-9_./-]+|[A-Za-z0-9_-]+\.md)"\)' \
  | sed -E 's#^(\.\./)+##; s#^join\("##; s#"\)$##' | sort -u || true)"
while IFS= read -r doc; do
  [ -n "$doc" ] || continue
  if ! grep -qxF -- "$doc" <<< "$listed"; then
    echo "FAIL: a test reads $doc, but classify-changes.sh would treat it as docs-only; add it to TEST_READ_DOCS"
    failures=$((failures + 1))
  fi
  if [ "$(printf '%s\n' "$doc" | bash "$CLASSIFY" | sed -n 's/^core=//p')" != true ]; then
    echo "FAIL: $doc is read by a test but does not run the determinism lane"
    failures=$((failures + 1))
  fi
done <<< "$read_docs"
for doc in README.md docs/cxf-composite-subset.md; do
  grep -qxF -- "$doc" <<< "$read_docs" || {
    echo "FAIL: the read-docs scan no longer finds $doc; the scan is broken"
    failures=$((failures + 1))
  }
done

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures classify-changes fixture(s) failed"
  exit 1
fi
echo "OK: classify-changes fixtures passed."
