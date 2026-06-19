#!/usr/bin/env bash
# Static smoke checks for CI gate topology. This catches false-green workflow edits before GitHub
# Actions gets a chance to silently skip the heavy test suite or dependency gates.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

WORKFLOW_DIR="${OCE_WORKFLOW_DIR:-.github/workflows}"
DENY_TOML="${OCE_DENY_TOML:-deny.toml}"

require_file() {
  path="$1"
  if [ ! -s "$path" ]; then
    echo "FAIL: required gate file is missing or empty: $path"
    exit 1
  fi
}

require_pattern() {
  path="$1"
  pattern="$2"
  label="$3"
  if ! grep -Eq "$pattern" "$path"; then
    echo "FAIL: $path is missing required gate pattern: $label"
    exit 1
  fi
}

ci="$WORKFLOW_DIR/ci.yml"
release="$WORKFLOW_DIR/release-gate.yml"
advisories="$WORKFLOW_DIR/advisories.yml"

require_file "$ci"
require_file "$release"
require_file "$advisories"
require_file "$DENY_TOML"

# Light per-PR dependency drift gate.
require_pattern "$ci" 'cargo-machete' 'install cargo-machete'
require_pattern "$ci" 'cargo machete' 'run cargo machete'
require_pattern "$ci" 'test-check-default-no-db\.sh' 'run default-no-db fixture tests'
require_pattern "$ci" 'check-workflow-gates\.sh' 'run workflow gate smoke'

# Heavy gate runs on release PRs, manual dispatch, and scheduled development-tip checks.
require_pattern "$release" 'schedule:' 'scheduled heavy gate'
require_pattern "$release" 'cron:' 'cron entry'
require_pattern "$release" 'workflow_dispatch:' 'manual heavy gate'
require_pattern "$release" '(CHECKOUT_REF:.*development|ref:.*development)' 'scheduled checkout targets development tip'
require_pattern "$release" 'ref:.*(CHECKOUT_REF|development)' 'checkout uses the scheduled development ref'
require_pattern "$release" 'cargo nextest run --workspace --locked --profile ci --no-tests=fail' \
  'debug nextest with hard-fail-on-zero-tests'
require_pattern "$release" 'cargo nextest run --workspace --locked --profile ci --cargo-profile release --no-tests=fail' \
  'release-codegen nextest with hard-fail-on-zero-tests'
require_pattern "$release" 'cargo test --workspace --doc --locked' 'doctest gate'
require_pattern "$release" 'OCE_REQUIRE_SURFACE_CHECK:[[:space:]]*"1"' 'armed public-api surface gate'
require_pattern "$release" 'cargo-machete' 'release gate installs cargo-machete'
require_pattern "$release" 'cargo machete' 'release gate runs cargo machete'

# Daily advisory/yanked gate and deny.toml discipline.
require_pattern "$advisories" 'schedule:' 'scheduled advisory gate'
require_pattern "$advisories" 'workflow_dispatch:' 'manual advisory gate'
require_pattern "$advisories" 'cargo deny check advisories' 'advisory-only cargo-deny command'
require_pattern "$DENY_TOML" '^[[:space:]]*yanked[[:space:]]*=[[:space:]]*"deny"' 'yanked = "deny"'
require_pattern "$DENY_TOML" '^[[:space:]]*ignore[[:space:]]*=[[:space:]]*\[\][[:space:]]*$' 'empty advisory ignore list'

echo "OK: workflow gate smoke passed."
