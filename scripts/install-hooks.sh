#!/usr/bin/env bash
# One-time local setup: point git at the tracked .githooks/ directory.
# Run once per clone:  bash scripts/install-hooks.sh
#
# .githooks/ is version-controlled (unlike .git/hooks/), so the team shares the same gates.
# Mirrors the CI split:
#   pre-commit -> cargo fmt --check + file-size + no-secret                     (fast)
#   pre-push   -> cargo clippy -D warnings + default-no-db gate                 (fast)
#
# Escape hatches: `git commit/push --no-verify` (once) or
# `export OCE_SKIP_HOOKS=1` (whole shell session). Empty, `0`, and `false` do not skip.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

git config core.hooksPath .githooks
chmod +x .githooks/pre-commit .githooks/pre-push 2>/dev/null || true
chmod +x .github/scripts/*.sh 2>/dev/null || true

echo "core.hooksPath -> .githooks"
echo "  pre-commit: cargo fmt --check + file-size cap + no-secret scan"
echo "  pre-push:   cargo clippy -D warnings + default-no-db gate"
echo "Skip once: --no-verify   |   skip session: export OCE_SKIP_HOOKS=1"
echo "Skip truthiness: empty, 0, and false do not skip; every other value skips"
