#!/usr/bin/env bash
# Fixtures for the leverage dependency rule in leverage-members.sh. Each case pipes a fake
# `cargo tree --depth 1 --prefix none --format {p}` listing into `check-tree` and expects the rule
# to accept or refuse it; the last case runs the real `check` and `lane` against this workspace.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT="scripts/ci/leverage-members.sh"
failures=0

case_tree() {
  local name="$1" expected="$2" member="$3" tree="$4" message="${5:-}"
  shift 5 || shift "$#"
  local output status
  set +e
  output="$(printf '%s\n' "$tree" | bash "$SCRIPT" check-tree "$member" "$@" 2>&1)"
  status=$?
  set -e
  if [ "$expected" = pass ] && [ "$status" -ne 0 ]; then
    echo "FAIL: leverage fixture '$name' should pass but exited $status: $output"
    failures=$((failures + 1))
  elif [ "$expected" = fail ] && [ "$status" -eq 0 ]; then
    echo "FAIL: leverage fixture '$name' should fail but passed"
    failures=$((failures + 1))
  elif [ "$expected" = fail ] && ! grep -Fq -- "$message" <<< "$output"; then
    echo "FAIL: leverage fixture '$name' failed without '$message': $output"
    failures=$((failures + 1))
  fi
}

facade_only='oce-host v0.2.0 (crates/leverage/oce-host)
oce-api v0.2.0 (crates/oce-api)
serde v1.0.228
oce-api v0.2.0 (crates/oce-api) (*)'

case_tree facade-only pass oce-host "$facade_only" ''
case_tree no-dependencies pass oce-host 'oce-host v0.2.0 (crates/leverage/oce-host)' ''
case_tree other-leverage-member pass oce-py \
  $'oce-py v0.2.0 (crates/leverage/oce-py)\noce-host v0.2.0 (crates/leverage/oce-host)\noce-api v0.2.0 (crates/oce-api)' \
  '' oce-host oce-py
case_tree forbidden-core-edge fail oce-host \
  $'oce-host v0.2.0 (crates/leverage/oce-host)\noce-api v0.2.0 (crates/oce-api)\noce-blocks v0.2.0 (crates/oce-blocks)' \
  'leverage crate oce-host depends directly on oce-blocks'
case_tree forbidden-dev-edge fail oce-host \
  $'oce-host v0.2.0 (crates/leverage/oce-host)\noce-api v0.2.0 (crates/oce-api)\n\n[dev-dependencies]\noce-conformance v0.2.0 (crates/oce-conformance)' \
  'depends directly on oce-conformance'
case_tree every-forbidden-edge-named fail oce-host \
  $'oce-host v0.2.0 (crates/leverage/oce-host)\noce-model v0.2.0 (crates/oce-model)\noce-store v0.2.0 (crates/oce-store)' \
  'depends directly on oce-model oce-store'
case_tree unlisted-leverage-sibling fail oce-py \
  $'oce-py v0.2.0 (crates/leverage/oce-py)\noce-host v0.2.0 (crates/leverage/oce-host)' \
  'depends directly on oce-host'
case_tree allowed-name-prefix-lookalike fail oce-host \
  $'oce-host v0.2.0 (crates/leverage/oce-host)\noce-api-internal v0.2.0 (crates/oce-api-internal)' \
  'depends directly on oce-api-internal'
# oce-bless (the golden-bless switch) is allowed as a dev-dependency only (authoring note decision
# 19, issue #445): in the dev listing it passes; as a normal dependency, or under a lookalike name,
# it fails, and the dev listing still refuses an engine crate.
case_tree bless-as-dev-dependency pass oce-replay \
  $'oce-replay v0.4.0 (crates/leverage/oce-replay)\nallocation-counter v0.8.1\noce-bless v0.4.0 (crates/oce-bless)' \
  '' --dev oce-replay
case_tree bless-as-normal-dependency fail oce-replay \
  $'oce-replay v0.4.0 (crates/leverage/oce-replay)\noce-api v0.4.0 (crates/oce-api)\noce-bless v0.4.0 (crates/oce-bless)' \
  'leverage crate oce-replay depends directly on oce-bless;' oce-replay
case_tree engine-crate-as-dev-dependency fail oce-replay \
  $'oce-replay v0.4.0 (crates/leverage/oce-replay)\noce-bless v0.4.0 (crates/oce-bless)\noce-graph v0.4.0 (crates/oce-graph)' \
  'depends directly on oce-graph (dev)' --dev oce-replay
case_tree bless-lookalike-as-dev-dependency fail oce-replay \
  $'oce-replay v0.4.0 (crates/leverage/oce-replay)\noce-bless-extra v0.4.0 (crates/oce-bless-extra)' \
  'depends directly on oce-bless-extra (dev)' --dev oce-replay
case_tree tree-of-another-crate fail oce-host 'oce-blocks v0.2.0 (crates/oce-blocks)' \
  'not at the leverage member oce-host'
case_tree empty-tree fail oce-host '' 'no cargo tree output'

if bash "$SCRIPT" unexpected > /dev/null 2>&1; then
  echo "FAIL: an unknown subcommand was accepted"
  failures=$((failures + 1))
fi

# The real workspace: the layout and dependency rule hold, and the lane passes.
if ! bash "$SCRIPT" check > /dev/null; then
  echo "FAIL: leverage-members.sh check fails on this workspace"
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures leverage-members fixture(s) failed"
  exit 1
fi
echo "OK: leverage-members fixtures passed."
