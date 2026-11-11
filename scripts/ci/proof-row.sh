#!/usr/bin/env bash
# Name the local proof a change needs before its pull request merges (issue #545).
#
#   git diff --no-renames --name-only BASE HEAD | bash scripts/ci/proof-row.sh
#
# Reads one repository-relative path per line on stdin and prints one line, `row=<name>`, where
# the name is the `.agents/gate.sh` mode whose tail the author pastes into the pull request:
#
#   docs         every path is documentation no lane-filtered job reads: `bash .agents/gate.sh docs`
#   leverage     only crates/leverage/**, documentation and the root README.md:
#                `bash .agents/gate.sh leverage`
#   light        any other change (the engine lane): `bash .agents/gate.sh`
#   full         the change is one only the workspace suite or `full` mode itself can check
#                before the merge: the vendored tree and the pin records its guard reads (a pin
#                advance; the G36 half of that guard runs only in the workspace suite), or
#                .agents/gate.sh (whose `full`-only steps no other mode executes).
#                `bash .agents/gate.sh full`.
#
# `determinism` is not a row (issue #556). From issue #545 it was owed by a change to what the
# release-codegen determinism subset builds, because no per-PR CI job ran that subset under
# release codegen; pr-gate.yml runs it in all four cells again, so those changes are engine-lane
# changes like any other. `bash .agents/gate.sh determinism` stays a mode, for reproducing a red
# release cell on one host, and this script never names it.
#
# The rows are ordered, and each mode runs everything the modes before it run, so a heavier run
# satisfies a lighter row. The lane boundaries are scripts/ci/classify-changes.sh's, read from its
# output and never restated here: documentation a test reads is an engine change, as is every gate
# script, workflow and tool config.
#
# Not a row, because no local run can stand in for it: a change to release-gate.yml, or to
# anything else only the nightly heavy set executes, is shown green by dispatching that workflow
# on the branch and waiting for it (docs/ci-and-the-gate.md, "The local proof").
#
# FAIL-CLOSED. Empty input (a diff that could not be computed) and a path git had to quote
# (unusual bytes; no prefix can be read from it) both name the heaviest row. A classifier that
# fails, or prints anything this script does not expect, fails this script.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

if [ "$#" -ne 0 ]; then
  echo "usage: bash scripts/ci/proof-row.sh < changed-paths" >&2
  exit 2
fi

# What only `full` checks before a merge. The first four are a pin advance: the vendored tree, the
# hash manifest and G36 provenance record beside the reference catalog, and the two source files
# that hold the pinned upstream commits. The last is the gate script, whose `full` block runs in no
# lighter mode.
FULL_PREFIXES=(third_party/ tools/reference-catalog/ crates/oce-cxf/tests/third_party_manifest/)
FULL_FILES=(crates/oce-blocks/src/registry/catalog_guard_support.rs .agents/gate.sh)

needs_full() {
  local path="$1" entry
  case "$path" in
    '"'*) return 0 ;;
  esac
  for entry in "${FULL_PREFIXES[@]}"; do
    case "$path" in
      "$entry"*) return 0 ;;
    esac
  done
  for entry in "${FULL_FILES[@]}"; do
    [ "$path" = "$entry" ] && return 0
  done
  return 1
}

paths="$(cat)"
count=0
full=false
while IFS= read -r path || [ -n "$path" ]; do
  [ -n "$path" ] || continue
  count=$((count + 1))
  if needs_full "$path"; then
    full=true
  fi
done <<< "$paths"

if [ "$count" -eq 0 ]; then
  echo "proof-row: no changed paths; naming the heaviest row" >&2
  echo 'row=full'
  exit 0
fi

lanes="$(printf '%s\n' "$paths" | bash scripts/ci/classify-changes.sh)"
lane() {
  local value
  value="$(printf '%s\n' "$lanes" | sed -n "s/^$1=//p")"
  case "$value" in
    true | false) printf '%s' "$value" ;;
    *)
      echo "proof-row: classify-changes.sh printed no true/false value for $1" >&2
      exit 1
      ;;
  esac
}
core="$(lane core)"
docs_only="$(lane docs_only)"

if [ "$full" = true ]; then
  echo 'row=full'
elif [ "$docs_only" = true ] && [ "$core" = false ]; then
  echo 'row=docs'
elif [ "$core" = false ]; then
  echo 'row=leverage'
else
  echo 'row=light'
fi
