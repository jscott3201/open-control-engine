#!/usr/bin/env bash
# DEFAULT-NO-DB gate (FRAME D-OWNER-1; roadmap 09 §2 M0 exit criterion 2).
#
# The DEFAULT build must link NO database, NO async runtime, and NO data-parallelism runtime anywhere
# in the workspace: `cargo tree -e normal` on the default feature set must list zero `selene-db-*`
# crates, zero `tokio`, zero `async-std`, and zero `rayon`. This proves a downstream project can
# embed the engine for load -> flatten -> validate -> schedule -> tick -> simulate with no DB at all.
#
# We keep `oce-api` (the public facade) as the canonical embeddable assertion AND loop the same
# forbidden-crate check over every workspace member derived from `cargo metadata --no-deps`. That
# catches Group C drift (`oce-conformance`, `oce-extension`, `oce-docs`) when new crates are added.
# We match real crate names (`selene-db`, `selene_*`, `tokio`, `async-std`, `rayon`); `selene-db` is
# listed as a representative graph database an app might adopt app-side — it must never enter this
# library's tree. `rayon` is forbidden by D6: the deterministic Kahn scheduler is single-threaded,
# and rayon's work-stealing parallelism would make schedules/traces non-bit-stable — the determinism
# contract (CDL §7.16) bans it from the engine, not just from the hot path.
#
# This shell regex intentionally stays narrow: it is the fast every-PR default-tree canary for the
# core embeddability invariant, not an exhaustive DB-family catalogue. The broader curated family
# list lives in cargo-deny `[bans].deny` (manifest-change CI + release gate). That list is a
# best-effort canary, not a proof; the load-bearing guarantee is architectural: the engine links no
# DB by construction and the `oce-api` facade default build links no async runtime.
#
# Runs from repo root; macOS bash 3.x compatible.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Real database / async-runtime / parallelism crate names that must never enter the library tree.
FORBIDDEN='(^|[[:space:]])(selene-db[-a-z]*|selene_[a-z]*|tokio|async-std|rayon[-a-z]*)([[:space:]]|$| v)'

workspace_members() {
  if [ -n "${OCE_NO_DB_MEMBERS_FILE:-}" ]; then
    if [ ! -s "$OCE_NO_DB_MEMBERS_FILE" ]; then
      echo "FAIL: workspace member fixture is missing or empty: $OCE_NO_DB_MEMBERS_FILE"
      return 1
    fi
    cat "$OCE_NO_DB_MEMBERS_FILE"
    return
  fi

  if ! command -v jq >/dev/null 2>&1; then
    echo "FAIL: jq is required to derive workspace members from cargo metadata"
    return 1
  fi

  if ! metadata="$(cargo metadata --no-deps --format-version 1 2>&1)"; then
    echo "FAIL: cargo metadata failed (gate fails closed, not open):"
    printf '%s\n' "$metadata"
    return 1
  fi

  printf '%s\n' "$metadata" | jq -r '.packages[].name' | sort -u
}

tree_for() {
  local package="$1"
  if [ -n "${OCE_NO_DB_TREE_DIR:-}" ]; then
    local fixture_file="$OCE_NO_DB_TREE_DIR/$package.tree"
    if [ ! -s "$fixture_file" ]; then
      echo "FAIL: tree fixture for '$package' is missing or empty: $fixture_file"
      return 1
    fi
    cat "$fixture_file"
    return
  fi

  # Fail CLOSED, not open: capture cargo's exit status instead of swallowing it with `|| true`.
  # A vacuous (empty) tree must never PASS — that posture (e.g. a momentarily unreachable git
  # source) is exactly where a regression could otherwise slip through. `--locked` keeps resolution
  # deterministic.
  if ! cargo tree -e normal --locked -p "$package" 2>&1; then
    echo "FAIL: cargo tree failed for '$package' (gate fails closed, not open)"
    return 1
  fi
}

check_tree() {
  local package="$1"
  local tree="$2"

  # Sanity: the tree must be non-empty AND contain the package root node before we trust a clean
  # FORBIDDEN result. Otherwise an empty/garbled tree would yield a vacuous green pass.
  if ! printf '%s\n' "$tree" | grep -Eq "(^|[[:space:]])$package v"; then
    echo "FAIL: cargo tree output for '$package' did not contain the expected '$package v' root node:"
    printf '%s\n' "$tree"
    exit 1
  fi

  if printf '%s\n' "$tree" | grep -Eiq "$FORBIDDEN"; then
    echo "FAIL: '$package' default build links a database / async runtime / parallelism runtime:"
    printf '%s\n' "$tree" | grep -Ei "$FORBIDDEN" || true
    echo
    echo "The build must be DB-free and async-runtime-free (FRAME D-OWNER-1): the library ships no"
    echo "first-party database. Durable backends are app-side adapters behind the oce-store port."
    echo "It must also be rayon-free (D6): the scheduler is single-threaded for bit-stable determinism."
    exit 1
  fi
}

checked=0
facade_checked=0
members_file="$(mktemp)"
tree_file="$(mktemp)"
trap 'rm -f "$members_file" "$tree_file"' EXIT

if ! workspace_members > "$members_file" 2>&1; then
  cat "$members_file"
  exit 1
fi

while IFS= read -r package; do
  [ -n "$package" ] || continue
  checked=$((checked + 1))
  [ "$package" = "oce-api" ] && facade_checked=1
  if ! tree_for "$package" > "$tree_file" 2>&1; then
    cat "$tree_file"
    exit 1
  fi
  tree="$(cat "$tree_file")"
  check_tree "$package" "$tree"
done < "$members_file"

if [ "$checked" -eq 0 ]; then
  echo "FAIL: workspace member list was empty (gate fails closed, not open)"
  exit 1
fi

if [ "$facade_checked" -ne 1 ]; then
  echo "FAIL: workspace member list did not include the canonical facade crate 'oce-api'"
  exit 1
fi

echo "OK: default build links no database (selene-db etc.), no tokio, no async-std, no rayon for oce-api and all workspace members."
