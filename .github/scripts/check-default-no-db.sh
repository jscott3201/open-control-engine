#!/usr/bin/env bash
# DEFAULT-NO-DB gate (FRAME D-OWNER-1; roadmap 09 §2 M0 exit criterion 2).
#
# The DEFAULT build must link NO database, NO async runtime, and NO data-parallelism runtime:
# `cargo tree -e normal` on the default feature set must list zero `selene-db-*` crates, zero
# `tokio`, zero `async-std`, and zero `rayon`. This proves a downstream project can embed the
# engine for load -> flatten -> validate -> schedule -> tick -> simulate with no DB at all.
#
# We inspect the default-feature *normal* dependency tree of `oce-api` (the public facade): it must
# pull none of the named database / async-runtime / parallelism crates. We match real crate names
# (`selene-db`, `selene_*`, `tokio`, `async-std`, `rayon`); `selene-db` is listed as a
# representative graph database an app might adopt app-side — it must never enter this library's
# tree. `rayon` is forbidden by D6: the deterministic Kahn scheduler is single-threaded, and
# rayon's work-stealing parallelism would make schedules/traces non-bit-stable — the determinism
# contract (CDL §7.16) bans it from the engine, not just from the hot path.
#
# Runs from repo root; macOS bash 3.x compatible.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Real database / async-runtime / parallelism crate names that must never enter the library tree.
FORBIDDEN='(^|[[:space:]])(selene-db[-a-z]*|selene_[a-z]*|tokio|async-std|rayon[-a-z]*)([[:space:]]|$| v)'

# Fail CLOSED, not open: capture cargo's exit status instead of swallowing it with `|| true`.
# A vacuous (empty) tree must never PASS — that posture (e.g. a momentarily unreachable git
# source) is exactly where a regression could otherwise slip through. `--locked` keeps resolution
# deterministic.
if ! tree="$(cargo tree -e normal --locked -p oce-api 2>&1)"; then
  echo "FAIL: cargo tree failed (gate fails closed, not open):"
  printf '%s\n' "$tree"
  exit 1
fi

# Sanity: the tree must be non-empty AND contain the oce-api root node before we trust a clean
# FORBIDDEN result. Otherwise an empty/garbled tree would yield a vacuous green pass.
if ! printf '%s\n' "$tree" | grep -Eq '(^|[[:space:]])oce-api v'; then
  echo "FAIL: cargo tree output did not contain the expected 'oce-api v' root node:"
  printf '%s\n' "$tree"
  exit 1
fi

if printf '%s\n' "$tree" | grep -Eiq "$FORBIDDEN"; then
  echo "FAIL: the default build links a database / async runtime / parallelism runtime:"
  printf '%s\n' "$tree" | grep -Ei "$FORBIDDEN" || true
  echo
  echo "The build must be DB-free and async-runtime-free (FRAME D-OWNER-1): the library ships no"
  echo "first-party database. Durable backends are app-side adapters behind the oce-store port."
  echo "It must also be rayon-free (D6): the scheduler is single-threaded for bit-stable determinism."
  exit 1
fi

echo "OK: default build links no database (selene-db etc.), no tokio, no async-std, no rayon."
