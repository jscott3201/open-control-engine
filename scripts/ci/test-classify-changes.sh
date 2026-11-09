#!/usr/bin/env bash
# Fixtures for classify-changes.sh, plus two guards: every documentation file a test reads is
# excluded from the docs-only class, and every test or script that reads the root README.md is run
# by a job that is always on or filtered on the README lane (issue #503). Each case feeds a
# changed-path list and compares all five outputs, so a rule that widens (a core path read as
# docs-only, leverage-only or README-only) fails here.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

CLASSIFY="scripts/ci/classify-changes.sh"
WORKFLOW=".github/workflows/pr-gate.yml"
failures=0

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

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

all='core=true leverage=true readme=true gate_scripts=true docs_only=false '
core='core=true leverage=true readme=true gate_scripts=false docs_only=false '
leverage='core=false leverage=true readme=false gate_scripts=false docs_only=false '
readme='core=false leverage=false readme=true gate_scripts=false docs_only=false '
readme_leverage='core=false leverage=true readme=true gate_scripts=false docs_only=false '
docs='core=false leverage=false readme=false gate_scripts=false docs_only=true '

expect leverage-readme "$leverage" 'crates/leverage/README.md'
expect leverage-crate-source "$leverage" $'crates/leverage/oce-host/src/lib.rs\ncrates/leverage/oce-host/Cargo.toml'
expect leverage-plus-docs "$leverage" $'crates/leverage/oce-host/src/lib.rs\ndocs/ci-and-the-gate.md'
expect docs-page "$docs" 'docs/ci-and-the-gate.md'
expect docs-site-and-tools "$docs" $'site/index.html\nscripts/docs/check_links.py\n.github/ISSUE_TEMPLATE/bug.md'
expect root-markdown "$docs" $'AGENTS.md\nTESTING.md\nCHANGELOG.md\nCONTRIBUTING.md\nSECURITY.md'
expect agent-facts-and-pr-template "$docs" $'.agents/project-facts.md\n.github/pull_request_template.md'
# The root README has its own lane (issue #503): alone, or beside documentation, it runs the README
# checks and neither the determinism lane nor the leverage lane. It is not docs-only.
expect readme-alone "$readme" 'README.md'
expect readme-plus-docs "$readme" $'README.md\ndocs/ci-and-the-gate.md\nCHANGELOG.md\n.agents/project-facts.md'
# Beside a leverage path it adds the leverage lane, still without the determinism lane: the shape
# of a landing pull request that updates the README's status and roadmap.
expect readme-plus-leverage "$readme_leverage" $'README.md\ncrates/leverage/oce-cli/src/lib.rs'
expect readme-plus-leverage-and-docs "$readme_leverage" \
  $'crates/leverage/oce-author/src/lib.rs\nCHANGELOG.md\nREADME.md\ndocs/design/importer.md'
# Beside an engine path, a gate script or documentation an engine test reads, the engine path
# decides: the whole determinism lane, and with it both other lanes.
expect readme-plus-engine "$core" $'README.md\ncrates/oce-api/src/lib.rs'
expect readme-plus-quickstart-example "$core" $'README.md\ncrates/oce-api/examples/quickstart.rs'
expect readme-plus-test-read-doc "$core" $'README.md\ndocs/cxf-composite-subset.md'
expect readme-plus-gate-script "$all" $'README.md\n.agents/gate.sh'
expect readme-plus-unknown-root-file "$core" $'README.md\nLICENSE-MIT'
# Only the root file is the README lane's. Every other README keeps its class.
expect docs-readme "$docs" 'docs/README.md'
expect readme-suffix-lookalike "$core" 'README.md.orig'
expect readme-quoted-by-git "$core" '"README.md"'
expect readme-in-a-directory "$core" 'crates/README.md'
# Documentation that a determinism-lane test reads is engine input, not docs.
expect composite-contract-doc "$core" 'docs/cxf-composite-subset.md'
# Engine paths: every one runs the determinism lane, and with it the leverage and README lanes.
expect core-four "$core" 'crates/oce-blocks/src/lib.rs'
# Firmware images (issue #402) link the core: engine lane, with or without docs beside them.
expect firmware-image "$core" $'firmware/link/src/lib.rs\nfirmware/link/size-budget.toml'
expect firmware-plus-docs "$core" $'firmware/link/boards/virt.x\ndocs/ci-and-the-gate.md'
expect cargo-config "$core" '.cargo/config.toml'
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
expect readme-no-trailing-newline "$readme" 'README.md'

got="$(bash "$CLASSIFY" --all < /dev/null | tr '\n' ' ')"
[ "$got" = "$all" ] || { echo "FAIL: --all did not select every lane: $got"; failures=$((failures + 1)); }
if bash "$CLASSIFY" unexpected < /dev/null > /dev/null 2>&1; then
  echo "FAIL: an unknown argument was accepted"
  failures=$((failures + 1))
fi

lane_alone() {
  printf '%s\n' "$1" | bash "$CLASSIFY" | sed -n "s/^$2=//p"
}

# Every documentation path a crate or tool reads must be in TEST_READ_DOCS (the determinism lane) or
# be the root README (its own lane, checked below). The scan finds the two path constructions used
# today — `../../docs/...` (include_str!/concat!) and `.join("<file>")` from the repository root —
# in tracked Rust sources. A new construction is invisible to it, so the classifier's comment names
# the list as the place to add one.
listed="$(sed -n '/^TEST_READ_DOCS=(/,/^)/p' "$CLASSIFY" | sed '1d;$d' | sed 's/#.*//' | tr -s ' ' '\n' | sed '/^$/d' | sort -u)"
[ -n "$listed" ] || { echo "FAIL: TEST_READ_DOCS is empty or unparsable"; failures=$((failures + 1)); }
read_docs="$(git ls-files 'crates/*.rs' 'tools/*.rs' \
  | xargs grep -hoE '(\.\./)+(docs|site)/[A-Za-z0-9_./-]+|join\("(docs/[A-Za-z0-9_./-]+|site/[A-Za-z0-9_./-]+|[A-Za-z0-9_-]+\.md)"\)' \
  | sed -E 's#^(\.\./)+##; s#^join\("##; s#"\)$##' | sort -u || true)"
while IFS= read -r doc; do
  [ -n "$doc" ] || continue
  if [ "$doc" = README.md ]; then
    if [ "$(lane_alone "$doc" readme)" != true ]; then
      echo "FAIL: the root README.md is read by a test but does not run the README lane"
      failures=$((failures + 1))
    fi
    continue
  fi
  if ! grep -qxF -- "$doc" <<< "$listed"; then
    echo "FAIL: a test reads $doc, but classify-changes.sh would treat it as docs-only; add it to TEST_READ_DOCS"
    failures=$((failures + 1))
  fi
  if [ "$(lane_alone "$doc" core)" != true ]; then
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

# ── Readers of the root README (issue #503) ──────────────────────────────────
# A change to the root README.md alone skips the determinism lane, so every check that reads that
# file has to run somewhere such a change still reaches: an always-on job, or a job filtered on the
# `readme` lane. This table is that enumeration: a source, the pr-gate.yml job that runs it, and the
# command in that job (a fixed string) that does. The guard below fails when a row's job is filtered
# on any other lane or no longer holds the command, and when a tracked source names the root
# README.md and is in neither this table nor OTHER_READMES.
README_CHECKS=(
  # Byte-pins the README's first Rust block to the compiled example.
  'crates/oce-api/tests/readme_quickstart.rs|facade-contract|cargo nextest run -p oce-api --test readme_quickstart --locked --profile ci --no-tests=fail'
  # Reads nothing itself: it executes the example the README's Quickstart is pinned to, which is
  # the other half of the README lane (a byte-pin proves the two copies agree, not that either runs).
  '.github/scripts/check-quickstart-runs.sh|facade-contract|bash .github/scripts/check-quickstart-runs.sh'
  # Copies the README's Quickstart into the staged book; check_integrity.py runs it.
  'scripts/docs/stage.py|docs|python3 scripts/docs/check_integrity.py'
  # Compares the README's verification section with the evidence pages.
  'scripts/docs/test_check_links.py|docs|python3 scripts/docs/test_check_links.py'
  # Holds the README to the product contract (pointer, retired facade names in the Rust example).
  'scripts/product_contract/check.py|package-publication-contract|python3 scripts/product_contract/check.py'
  # Mutates a fixture copy of the README to prove the check above can fail.
  'scripts/product_contract/test_check.py|package-publication-contract|python3 scripts/product_contract/test_check.py'
)

# Sources the scan finds because they name a `README.md` that is not the root file, or name the root
# path without opening it. An entry the scan no longer finds fails, so the list cannot go stale.
OTHER_READMES=(
  .agents/gate.sh                                     # comments; it runs the two README checks
  .agents/revendor-upstream.sh                        # the vendored tree's README
  crates/oce-cxf/tests/composite_contract_corpus.rs   # read_fixture: the composite corpus index
  crates/oce-cxf/tests/third_party_manifest/mod.rs    # the vendored tree's README
  scripts/ci/classify-changes.sh                      # classifies the path; opens nothing
  scripts/ci/test-classify-changes.sh                 # this file
  scripts/ci/test_ci_ok.py                            # feeds the path to the classifier
)

# Tracked sources that name the root README.md: any `README.md` not directly inside a named
# directory (`docs/README.md`, `crates/leverage/README.md` and the like are other files; `../` and
# a variable such as `$root/` or `{root}/` are not directories, so those forms still count).
# Comments count too: a false hit costs one OTHER_READMES line, a missed reader costs a gap.
# What this cannot see, stated so a green run is not read as more than it is: an inline `run:` step
# in a workflow, a source outside these trees, and a check that reaches the README by walking the
# tree instead of naming it (the no-secret scan, which is always-on).
readme_sources() {
  {
    git ls-files -- 'crates/*.rs' 'tools/*.rs' 'firmware/*.rs' 'scripts/*' '.github/scripts/*' \
      '.agents/*.sh' '.githooks/*' 'tools/*.py' 'tools/*.sh'
    [ "$#" -eq 0 ] || printf '%s\n' "$@"
  } | while IFS= read -r source; do
    [ -f "$source" ] || continue
    # No `grep -q` here: it exits at the first match, the `sed` feeding it dies on the closed pipe,
    # and under `pipefail` a large file that does name the README would then read as a miss.
    if sed -E 's#(^|[^$A-Za-z0-9_.-])[A-Za-z0-9_.-]*[A-Za-z0-9_-]/README\.md#\1#g' "$source" \
      | grep 'README\.md' > /dev/null; then
      printf '%s\n' "$source"
    fi
  done | sort -u
}

# The body of one pr-gate.yml job, comments dropped.
job_body() {
  awk -v header="  $2:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found && !/^[[:space:]]*#/ { print }
  ' "$1"
}

# Print one line per defect in the README coverage of workflow $1. Further arguments are extra
# sources to scan beside the tracked ones (the controls below pass an unlisted reader).
readme_coverage_defects() {
  local workflow="$1" row source job command body condition lane found recorded
  shift
  local -a known=()
  for row in "${README_CHECKS[@]}"; do
    IFS='|' read -r source job command <<< "$row"
    known+=("$source")
    [ -f "$source" ] || { echo "$source is in README_CHECKS but does not exist"; continue; }
    body="$(job_body "$workflow" "$job")"
    [ -n "$body" ] || { echo "$source: pr-gate.yml has no $job job"; continue; }
    condition="$(sed -n 's/^    if:[[:space:]]*//p' <<< "$body")"
    case "$condition" in
      '') lane=always ;;
      "needs.changes.outputs.readme == 'true'") lane=readme ;;
      *) lane="$condition" ;;
    esac
    if [ "$lane" != always ] && [ "$lane" != readme ]; then
      echo "$source is a root README check, but job $job is filtered on: $lane"
    fi
    if ! grep -qF -- "$command" <<< "$body"; then
      echo "$source is a root README check, but job $job does not run: $command"
    fi
  done
  found="$(readme_sources "$@")"
  recorded="$(printf '%s\n' "${known[@]}" "${OTHER_READMES[@]}")"
  while IFS= read -r source; do
    [ -n "$source" ] || continue
    if ! grep -qxF -- "$source" <<< "$recorded"; then
      echo "$source names the root README.md, but no README-lane or always-on job is recorded for it; add it to README_CHECKS (or to OTHER_READMES with the reason it is another file)"
    fi
  done <<< "$found"
  for source in "${OTHER_READMES[@]}"; do
    grep -qxF -- "$source" <<< "$found" || echo "$source is in OTHER_READMES, but the scan no longer finds a README.md in it"
  done
  for source in crates/oce-api/tests/readme_quickstart.rs scripts/docs/stage.py scripts/product_contract/check.py; do
    grep -qxF -- "$source" <<< "$found" || echo "the README scan no longer finds $source; the scan is broken"
  done
}

defects="$(readme_coverage_defects "$WORKFLOW")"
if [ -n "$defects" ]; then
  while IFS= read -r defect; do
    echo "FAIL: $defect"
    failures=$((failures + 1))
  done <<< "$defects"
fi
# The staging script is run through check_integrity.py, not named in the workflow.
grep -qF '"stage.py"' scripts/docs/check_integrity.py || {
  echo "FAIL: scripts/docs/check_integrity.py no longer runs stage.py, which reads the root README.md"
  failures=$((failures + 1))
}

# Controls: the guard above must be able to fail. Each mutates a copy of the real workflow (or adds
# an unlisted reader) and requires the named defect; a mutation that changes nothing fails too.
expect_defect() {
  local name="$1" want="$2" workflow="$3" got
  shift 3
  if cmp -s "$workflow" "$WORKFLOW" && [ "$#" -eq 0 ]; then
    echo "FAIL: README coverage control '$name' did not change the workflow"
    failures=$((failures + 1))
    return
  fi
  got="$(readme_coverage_defects "$workflow" "$@")"
  if ! grep -qF -- "$want" <<< "$got"; then
    echo "FAIL: README coverage control '$name' did not report: $want"
    printf '%s\n' "$got" | sed 's/^/  got: /'
    failures=$((failures + 1))
  fi
}

# The README job back on the determinism lane: a README-only change would skip the byte-pin.
sed "/^  facade-contract:\$/,/^    if:/ s/outputs.readme == 'true'/outputs.core == 'true'/" "$WORKFLOW" \
  > "$tmp/readme-job-on-core.yml"
expect_defect readme-job-on-core \
  "crates/oce-api/tests/readme_quickstart.rs is a root README check, but job facade-contract is filtered on: needs.changes.outputs.core == 'true'" \
  "$tmp/readme-job-on-core.yml"
# The byte-pin step removed from the README job.
grep -v 'cargo nextest run -p oce-api --test readme_quickstart ' "$WORKFLOW" > "$tmp/byte-pin-dropped.yml"
expect_defect byte-pin-dropped \
  'crates/oce-api/tests/readme_quickstart.rs is a root README check, but job facade-contract does not run: cargo nextest run -p oce-api --test readme_quickstart' \
  "$tmp/byte-pin-dropped.yml"
# The Quickstart execution removed from the README job.
grep -v 'run: bash .github/scripts/check-quickstart-runs.sh' "$WORKFLOW" > "$tmp/execution-dropped.yml"
expect_defect execution-dropped \
  '.github/scripts/check-quickstart-runs.sh is a root README check, but job facade-contract does not run: bash .github/scripts/check-quickstart-runs.sh' \
  "$tmp/execution-dropped.yml"
# An always-on reader's job given a lane filter: a README-only change would skip the docs staging.
awk '{ print } /^  docs:$/ { print "    if: needs.changes.outputs.leverage == '"'"'true'"'"'" }' "$WORKFLOW" \
  > "$tmp/docs-on-leverage.yml"
expect_defect docs-on-leverage \
  "scripts/docs/stage.py is a root README check, but job docs is filtered on: needs.changes.outputs.leverage == 'true'" \
  "$tmp/docs-on-leverage.yml"
# The product contract check removed from its always-on job.
grep -v 'run: python3 scripts/product_contract/check.py' "$WORKFLOW" > "$tmp/product-contract-dropped.yml"
expect_defect product-contract-dropped \
  'scripts/product_contract/check.py is a root README check, but job package-publication-contract does not run: python3 scripts/product_contract/check.py' \
  "$tmp/product-contract-dropped.yml"
# A job that holds a reader deleted from the workflow.
awk '
  /^  facade-contract:$/ { skip = 1; next }
  skip && /^  [^ ]/ { skip = 0 }
  !skip { print }
' "$WORKFLOW" > "$tmp/readme-job-removed.yml"
expect_defect readme-job-removed \
  'crates/oce-api/tests/readme_quickstart.rs: pr-gate.yml has no facade-contract job' \
  "$tmp/readme-job-removed.yml"
# A new reader nobody recorded, in each construction the scan knows: a path from the repository
# root, a parent-relative include, a shell variable prefix and a format-string prefix.
mkdir -p "$tmp/readers"
printf '%s\n' 'let readme = fs::read_to_string(root.join("README.md")).unwrap();' > "$tmp/readers/joined.rs"
printf '%s\n' 'const README: &str = include_str!("../../../README.md");' > "$tmp/readers/included.rs"
printf '%s\n' 'grep -q Quickstart "$ROOT/README.md"' > "$tmp/readers/variable.sh"
printf '%s\n' 'text = open(f"{root}/README.md").read()' > "$tmp/readers/formatted.py"
for reader in joined.rs included.rs variable.sh formatted.py; do
  expect_defect "unlisted-reader-$reader" \
    "$tmp/readers/$reader names the root README.md, but no README-lane or always-on job is recorded for it" \
    "$WORKFLOW" "$tmp/readers/$reader"
done
# Another directory's README is not the root one, and must not be reported.
printf '%s\n' 'let index = read("crates/leverage/README.md"); // and docs/README.md' > "$tmp/readers/other.rs"
if grep -qF "$tmp/readers/other.rs" <<< "$(readme_coverage_defects "$WORKFLOW" "$tmp/readers/other.rs")"; then
  echo "FAIL: a README.md inside a named directory was reported as the root README"
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures classify-changes fixture(s) failed"
  exit 1
fi
echo "OK: classify-changes fixtures passed."
