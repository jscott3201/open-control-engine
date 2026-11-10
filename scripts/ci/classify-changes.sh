#!/usr/bin/env bash
# Classify a pull request's changed paths into the per-PR CI lanes (issue #393).
#
#   git diff --no-renames --name-only BASE HEAD | bash scripts/ci/classify-changes.sh
#   bash scripts/ci/classify-changes.sh --all        # manual dispatch: every lane runs
#
# Reads one repository-relative path per line on stdin and prints five `name=true|false` lines for
# `$GITHUB_OUTPUT`. Each output names a set of jobs that must run, not merely a kind of path:
#
#   core          the determinism lane: anything that is not a docs-only path or a leverage-crate
#                 path, including every gate script, workflow and tool config (a CI change must
#                 prove the whole lane still passes), Cargo.lock, every crate outside
#                 crates/leverage, and the bare-metal firmware images under firmware/ (they link
#                 the core four; never add firmware/ to the docs or leverage allowlists).
#   leverage      the leverage lane: any path under crates/leverage/, or `core` (an engine change
#                 can break a crate built on the facade).
#   readme        the README lane (issue #503): the root README.md changed, or `core`. It gates the
#                 one job that runs the checks reading the root README that are not always-on: the
#                 byte-pin of its Quickstart against the compiled example
#                 (crates/oce-api/tests/readme_quickstart.rs) and the executed Quickstart. The root
#                 README.md alone sets neither `core` nor `leverage`: it is not engine input, and
#                 every other reader of it runs in an always-on job (test-classify-changes.sh holds
#                 that enumeration). Only the root file: crates/*/README.md is still `core` and
#                 crates/leverage/**/README.md still `leverage`.
#   gate_scripts  the gate's own definition changed: the gate script itself, the CI scripts,
#                 scripts/ outside scripts/docs, the workflows and .config/. Implies core, so such
#                 a change runs the whole determinism lane. Informational since issue #502: no job
#                 is gated on it (`gate (light)` was; release-gate.yml's `gate (full)` executes
#                 .agents/gate.sh on every merge into `development` instead).
#   docs_only     every path is a documentation path that no lane-filtered job reads
#                 (informational; nothing is gated on it). The root README.md is not one.
#
# FAIL-CLOSED. A path that matches no allowlist counts as core. Empty input (a diff that could not
# be computed, or an empty PR) runs every lane. Renames must be listed as a deletion plus an
# addition (`--no-renames`), or moving engine source under docs/ would read as docs-only. A path
# git had to quote (unusual bytes) matches no allowlist and therefore counts as core.

set -euo pipefail

# Documentation paths that a determinism-lane test reads. Editing one can fail a test, so it is not
# docs-only: it counts as core. The fixture test (test-classify-changes.sh) scans the crates for
# path constructions that read documentation and fails if one is missing here.
TEST_READ_DOCS=(
  docs/cxf-composite-subset.md   # crates/oce-cxf/tests/composite_contract_doc.rs include_str!s it
  docs/design/vectors.md         # crates/leverage/oce-verify/tests/closed_objects.rs include_str!s it
)

# The root README has a lane of its own (issue #503). A test byte-pins its Quickstart, so it is not
# docs-only either, but the checks that read it need the facade and nothing else of the engine.
# The fixture test fails when a test or script reads this file and no always-on or README-lane job
# runs it, so a new reader cannot leave this lane too narrow unnoticed.
ROOT_README=README.md

is_test_read_doc() {
  local path="$1" entry
  for entry in "${TEST_READ_DOCS[@]}"; do
    [ "$path" = "$entry" ] && return 0
  done
  return 1
}

is_docs_path() {
  local path="$1"
  is_test_read_doc "$path" && return 1
  [ "$path" = "$ROOT_README" ] && return 1
  case "$path" in
    docs/* | site/* | scripts/docs/*) return 0 ;;
    .agents/project-facts.md | .github/pull_request_template.md | .github/ISSUE_TEMPLATE/*) return 0 ;;
    */*) return 1 ;;
    *.md) return 0 ;;
  esac
  return 1
}

is_leverage_path() {
  case "$1" in
    crates/leverage/*) return 0 ;;
  esac
  return 1
}

is_gate_script_path() {
  case "$1" in
    scripts/docs/*) return 1 ;;
    .agents/gate.sh | .github/scripts/* | scripts/* | .github/workflows/* | .config/*) return 0 ;;
  esac
  return 1
}

emit() {
  printf 'core=%s\nleverage=%s\nreadme=%s\ngate_scripts=%s\ndocs_only=%s\n' "$1" "$2" "$3" "$4" "$5"
}

if [ "${1:-}" = "--all" ]; then
  emit true true true true false
  exit 0
fi
if [ "$#" -ne 0 ]; then
  echo "usage: bash scripts/ci/classify-changes.sh [--all] < changed-paths" >&2
  exit 2
fi

core=false
leverage=false
readme=false
gate_scripts=false
docs_only=true
count=0

while IFS= read -r path || [ -n "$path" ]; do
  [ -n "$path" ] || continue
  count=$((count + 1))
  if is_gate_script_path "$path"; then
    gate_scripts=true
  fi
  if is_docs_path "$path"; then
    continue
  fi
  docs_only=false
  if [ "$path" = "$ROOT_README" ]; then
    readme=true
  elif is_leverage_path "$path"; then
    leverage=true
  else
    core=true
  fi
done

if [ "$count" -eq 0 ]; then
  echo "classify-changes: no changed paths; running every lane" >&2
  emit true true true true false
  exit 0
fi

[ "$gate_scripts" = true ] && core=true
if [ "$core" = true ]; then
  leverage=true
  readme=true
fi

emit "$core" "$leverage" "$readme" "$gate_scripts" "$docs_only"
