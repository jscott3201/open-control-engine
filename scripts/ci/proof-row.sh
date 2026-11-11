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
#   determinism  the change touches one of the core four (oce-model, oce-expr, oce-blocks,
#                oce-graph) or oce-cxf: `bash .agents/gate.sh determinism`, the light proof plus
#                the determinism subset and the strict-bit step under release codegen
#
# The rows are ordered, and each mode runs everything the modes before it run, so a heavier run
# satisfies a lighter row. The lane boundaries are scripts/ci/classify-changes.sh's, read from its
# output and never restated here: documentation a test reads is an engine change, as is every gate
# script, workflow and tool config.
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

# The crates whose change needs release-codegen evidence before the merge: the core four, whose
# arithmetic and scheduling every determinism golden rests on, and the CXF importer.
RELEASE_CODEGEN_CRATES=(oce-model oce-expr oce-blocks oce-graph oce-cxf)

touches_release_codegen_crate() {
  local path="$1" crate
  case "$path" in
    '"'*) return 0 ;;
  esac
  for crate in "${RELEASE_CODEGEN_CRATES[@]}"; do
    case "$path" in
      "crates/$crate/"*) return 0 ;;
    esac
  done
  return 1
}

paths="$(cat)"
count=0
release_codegen=false
while IFS= read -r path || [ -n "$path" ]; do
  [ -n "$path" ] || continue
  count=$((count + 1))
  if touches_release_codegen_crate "$path"; then
    release_codegen=true
  fi
done <<< "$paths"

if [ "$count" -eq 0 ]; then
  echo "proof-row: no changed paths; naming the heaviest row" >&2
  echo 'row=determinism'
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

if [ "$docs_only" = true ] && [ "$core" = false ]; then
  echo 'row=docs'
elif [ "$core" = false ]; then
  echo 'row=leverage'
elif [ "$release_codegen" = true ]; then
  echo 'row=determinism'
else
  echo 'row=light'
fi
