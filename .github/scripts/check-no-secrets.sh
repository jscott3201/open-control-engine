#!/usr/bin/env bash
# Baseline no-secret scan. Greps tracked files for common secret-shaped patterns.
# Catches the noisy floor; the `_*/` working dirs are gitignored (never tracked) so
# they are out of scope by construction.
#
# HOW A PATTERN IS PASSED (issue #524). Each pattern goes to `git grep` after `-e`, so a pattern
# that begins with `-` is a pattern and never an option. Before that the private-key pattern,
# which begins with `-----`, was read as one: `git grep` printed its usage and exited 129, the
# `if` around it took any non-zero status for "no match", and `>/dev/null 2>&1` hid the error, so
# a tracked private key passed this gate. `git grep` exits 0 on a match, 1 on no match and above
# 1 when it could not search (128 outside a repository, 129 on a usage error). Only 1 is clean:
# any other status fails the scan, and git's own error is left on stderr. `grep.fallbackToNoIndex`
# is forced off, because when a config file, `git -c` or `GIT_CONFIG_COUNT` turns it on, `git grep`
# outside a repository searches the directory instead of failing, and an empty one is "no match".
# Its fixtures, test-check-no-secrets.sh, plant a match for every pattern below, a near miss beside
# each, and put the defect back.
#
# Exit status: 0 clean, 1 a pattern matched, 2 a pattern could not be searched for. 2 wins over 1,
# because a scan that did not run says nothing about the files it did not read.

set -euo pipefail

violations=0
scan_errors=0

scan() {
  local pattern="$1"
  local label="$2"
  local matches
  local status=0
  matches="$(git -c grep.fallbackToNoIndex=false grep -nE -e "$pattern")" || status=$?
  case "$status" in
    0)
      echo "FAIL: matched $label pattern in tracked files:"
      printf '%s\n' "$matches"
      violations=$((violations + 1))
      ;;
    1) ;;
    *)
      echo "FAIL: git grep exited $status searching for the $label pattern; the tracked files were not scanned for it (git's error is above)." >&2
      scan_errors=$((scan_errors + 1))
      ;;
  esac
}

scan 'AKIA[0-9A-Z]{16}' 'AWS access key id'
# Every PEM private-key header (issue #607): PKCS#8's `PRIVATE KEY` and `ENCRYPTED PRIVATE KEY`
# (RFC 7468), the legacy `RSA`, `DSA` and `EC` forms, OpenSSH's, any other label of upper-case
# words before `PRIVATE KEY`, and the armored OpenPGP key, whose header ends `PRIVATE KEY BLOCK`.
scan '-----BEGIN ([A-Z0-9]+ )*PRIVATE KEY( BLOCK)?-----' 'private key block'
scan 'xox[abpr]-[A-Za-z0-9-]{10,}' 'Slack token'
scan 'gh[pousr]_[A-Za-z0-9]{36,}' 'GitHub token'
scan 'sk-[A-Za-z0-9_-]{20,}' 'API token (sk- prefix)'

# Project-specific leak classes (added after the M0 identity-leak incident): full UUIDs (Aionforge
# agent / namespace ids), developer home-directory paths, and Aionforge env-var names. Each pattern
# matches a SHAPE, never the literal sensitive value, so this scanner never re-leaks what it detects.
scan '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}' 'UUID (possible agent / namespace identity)'
scan '/(Users|home)/[A-Za-z0-9._-]+/' 'absolute developer home-directory path'
scan 'AIONFORGE_[A-Z][A-Z_]+' 'Aionforge environment variable name'

if [ "$violations" -gt 0 ]; then
  echo
  echo "Remove secrets from the working tree, rotate compromised credentials, and rewrite history."
fi

if [ "$scan_errors" -gt 0 ]; then
  echo >&2
  echo "FAIL: $scan_errors pattern(s) could not be searched for, so this scan does not show the tree is clean." >&2
  exit 2
fi

if [ "$violations" -gt 0 ]; then
  exit 1
fi

echo "OK: baseline no-secret scan clean."
