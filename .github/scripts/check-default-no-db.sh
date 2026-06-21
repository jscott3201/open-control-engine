#!/usr/bin/env bash
# DEFAULT-NO-DB gate (FRAME D-OWNER-1; roadmap 09 §2 M0 exit criterion 2).
#
# The DEFAULT build must link NO database, NO async runtime, and NO data-parallelism runtime. This
# script enforces that posture with three independent canaries:
#
# 1. Transitive cargo-tree breadth check: `cargo tree -e normal` on the default feature set must
#    list zero `selene-db-*` crates, zero `tokio`, zero `async-std`, and zero `rayon` for `oce-api`
#    and every workspace member.
# 2. Direct shipped-crate edge check: workspace packages with `publish == null` must not declare a
#    normal/build dependency whose name matches the same forbidden-crate regex. Dev dependencies are
#    intentionally ignored so future verification adapters can be used only from tests.
# 3. Verification-adapter unreachability check: workspace packages with `publish == []` are treated
#    as verification-only. Shipped crates must not depend on them directly or reach them through any
#    non-dev dependency path. The pass is forward-armed and vacuously green while no such package
#    exists. To arm this pass, the future reference WAL adapter must be a `publish = false` workspace
#    member under `crates/` so root cargo metadata can see it; a tests/ fixture or tools/-style
#    standalone sub-workspace is invisible here and would leave this pass permanently vacuous.
#
# Tests may inject cargo metadata with OCE_NO_DB_METADATA_FILE; the fixture is used for both the
# `--no-deps` package scan and the full resolve graph scan. The gate fails closed if metadata or jq
# is unavailable, empty, malformed, has no shipped crates, or omits the canonical `oce-api` facade.
# Together these prove a downstream project can embed the engine for load -> flatten -> validate ->
# schedule -> tick -> simulate with no DB at all.
#
# We keep `oce-api` (the public facade) as the canonical embeddable assertion AND loop the same
# forbidden-crate check over every workspace member derived from `cargo metadata --no-deps`. That
# catches Group C drift (`oce-conformance`, `oce-extension`, `oce-docs`) when new crates are added.
# We match real crate names (`selene-db`, `selene_*`, `tokio*`, `async-std*`, `rayon*`); `selene-db`
# is listed as a representative graph database an app might adopt app-side — it must never enter
# this library's tree. `rayon` is forbidden by D6: the deterministic Kahn scheduler is
# single-threaded, and rayon's work-stealing parallelism would make schedules/traces non-bit-stable
# — the determinism contract (CDL §7.16) bans it from the engine, not just from the hot path.
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
FORBIDDEN='(^|[[:space:]])(selene-db[-a-z]*|selene_[a-z]*|tokio[-a-z]*|async-std[-a-z]*|rayon[-a-z]*)([[:space:]]|$| v)'

load_metadata() {
  local mode="$1"
  local output_file="$2"
  local metadata=""

  if ! command -v jq >/dev/null 2>&1; then
    echo "FAIL: jq is required to inspect cargo metadata"
    return 1
  fi

  if [ -n "${OCE_NO_DB_METADATA_FILE:-}" ]; then
    if [ ! -s "$OCE_NO_DB_METADATA_FILE" ]; then
      echo "FAIL: cargo metadata fixture is missing or empty: $OCE_NO_DB_METADATA_FILE"
      return 1
    fi
    cat "$OCE_NO_DB_METADATA_FILE" > "$output_file"
  elif [ "$mode" = "no_deps" ]; then
    if ! metadata="$(cargo metadata --no-deps --format-version 1 2>&1)"; then
      echo "FAIL: cargo metadata --no-deps failed (gate fails closed, not open):"
      printf '%s\n' "$metadata"
      return 1
    fi
    printf '%s\n' "$metadata" > "$output_file"
  elif [ "$mode" = "full" ]; then
    if ! metadata="$(cargo metadata --format-version 1 2>&1)"; then
      echo "FAIL: cargo metadata failed (gate fails closed, not open):"
      printf '%s\n' "$metadata"
      return 1
    fi
    printf '%s\n' "$metadata" > "$output_file"
  else
    echo "BUG: unknown cargo metadata mode '$mode'"
    return 1
  fi

  if [ ! -s "$output_file" ]; then
    echo "FAIL: cargo metadata output was empty (gate fails closed, not open)"
    return 1
  fi

  if ! jq -e '.packages | type == "array"' "$output_file" >/dev/null 2>&1; then
    echo "FAIL: cargo metadata was missing a package array or was malformed"
    return 1
  fi
}

validate_shipped_metadata() {
  local metadata_file="$1"
  local shipped_count=""

  if ! shipped_count="$(jq '[.packages[] | select(.publish == null)] | length' "$metadata_file")"; then
    echo "FAIL: cargo metadata shipped-crate query failed"
    return 1
  fi

  if [ "$shipped_count" -eq 0 ]; then
    echo "FAIL: cargo metadata contained no shipped crates (publish == null)"
    return 1
  fi

  if ! jq -e '.packages[] | select(.publish == null and .name == "oce-api")' \
    "$metadata_file" >/dev/null; then
    echo "FAIL: cargo metadata shipped set did not include the canonical facade crate 'oce-api'"
    return 1
  fi
}

workspace_members() {
  if [ -n "${OCE_NO_DB_MEMBERS_FILE:-}" ]; then
    if [ ! -s "$OCE_NO_DB_MEMBERS_FILE" ]; then
      echo "FAIL: workspace member fixture is missing or empty: $OCE_NO_DB_MEMBERS_FILE"
      return 1
    fi
    cat "$OCE_NO_DB_MEMBERS_FILE"
    return
  fi

  jq -r '.packages[].name' "$no_deps_metadata_file" | sort -u
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

check_direct_forbidden_edges() {
  local metadata_file="$1"
  local failures=""

  if ! failures="$(jq -r --arg forbidden "$FORBIDDEN" '
    def nondev: (.kind == null or .kind == "normal" or .kind == "build");
    .packages[]
    | select(.publish == null) as $package
    | $package.dependencies[]?
    | select(nondev)
    | select(.name | test($forbidden; "i"))
    | "\($package.name)\t\(.name)\t\((.kind // "normal"))"
  ' "$metadata_file")"; then
    echo "FAIL: cargo metadata direct-edge query failed"
    return 1
  fi

  if [ -n "$failures" ]; then
    printf '%s\n' "$failures" | while IFS="$(printf '\t')" read -r package dependency kind; do
      echo "FAIL: shipped crate '$package' has forbidden direct $kind dependency '$dependency'"
    done
    echo
    echo "Shipped crates may not declare direct normal/build database, async-runtime, or rayon edges."
    return 1
  fi
}

check_direct_verification_edges() {
  local metadata_file="$1"
  local failures=""

  if ! failures="$(jq -r '
    def nondev: (.kind == null or .kind == "normal" or .kind == "build");
    [.packages[] | select(.publish == []) | .name] as $verification_only
    | .packages[]
    | select(.publish == null) as $package
    | $package.dependencies[]?
    | select(nondev)
    | .name as $dependency_name
    | select(($verification_only | index($dependency_name)) != null)
    | "\($package.name)\t\(.name)\t\((.kind // "normal"))"
  ' "$metadata_file")"; then
    echo "FAIL: cargo metadata verification-only direct-edge query failed"
    return 1
  fi

  if [ -n "$failures" ]; then
    printf '%s\n' "$failures" | while IFS="$(printf '\t')" read -r package dependency kind; do
      echo "FAIL: shipped crate '$package' has forbidden direct $kind dependency on verification-only crate '$dependency'"
    done
    echo
    echo "publish=false workspace members are verification-only and must remain unreachable from shipped crates."
    return 1
  fi
}

verification_only_count() {
  local metadata_file="$1"
  jq '[.packages[] | select(.publish == [])] | length' "$metadata_file"
}

check_transitive_verification_edges() {
  local shipped_metadata_file="$1"
  local full_metadata_file="$2"
  local failures=""

  if ! jq -e '.resolve.nodes | type == "array"' "$full_metadata_file" >/dev/null 2>&1; then
    echo "FAIL: full cargo metadata was missing resolve.nodes; cannot prove adapter unreachability"
    return 1
  fi

  if ! failures="$(jq -r --slurpfile shipped_metadata "$shipped_metadata_file" '
    def nondev_dep:
      any(.dep_kinds[]?; (.kind == null or .kind == "normal" or .kind == "build"));
    def reaches($edges; $seen; $frontier):
      if ($frontier | length) == 0 then
        $seen
      else
        ([
          $edges[]
          | .from as $from
          | select(($frontier | index($from)) != null)
          | .to as $to
          | select(($seen | index($to)) == null)
          | $to
        ] | unique) as $next
        | reaches($edges; ($seen + $next | unique); $next)
      end;

    ($shipped_metadata[0].packages | map(select(.publish == null) | {id, name})) as $shipped
    | ($shipped_metadata[0].packages | map(select(.publish == []) | {id, name})) as $verification_only
    | [
        .resolve.nodes[]? as $node
        | $node.deps[]?
        | select(nondev_dep)
        | {from: $node.id, to: .pkg}
      ] as $edges
    | $shipped[] as $package
    | (reaches($edges; [$package.id]; [$package.id]) - [$package.id]) as $reachable
    | $verification_only[] as $verification
    | select(($reachable | index($verification.id)) != null)
    | "\($package.name)\t\($verification.name)"
  ' "$full_metadata_file")"; then
    echo "FAIL: full cargo metadata transitive reachability query failed"
    return 1
  fi

  if [ -n "$failures" ]; then
    printf '%s\n' "$failures" | while IFS="$(printf '\t')" read -r package verification_only; do
      echo "FAIL: shipped crate '$package' reaches verification-only crate '$verification_only' through non-dev dependencies"
    done
    echo
    echo "publish=false workspace members may be used by verification code only; shipped crates must not reach them."
    return 1
  fi
}

checked=0
facade_checked=0
members_file="$(mktemp)"
tree_file="$(mktemp)"
no_deps_metadata_file="$(mktemp)"
full_metadata_file="$(mktemp)"
trap 'rm -f "$members_file" "$tree_file" "$no_deps_metadata_file" "$full_metadata_file"' EXIT

if ! load_metadata no_deps "$no_deps_metadata_file"; then
  exit 1
fi

if ! validate_shipped_metadata "$no_deps_metadata_file"; then
  exit 1
fi

if ! check_direct_forbidden_edges "$no_deps_metadata_file"; then
  exit 1
fi

if ! check_direct_verification_edges "$no_deps_metadata_file"; then
  exit 1
fi

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

verification_count="$(verification_only_count "$no_deps_metadata_file")"
if [ "$verification_count" -gt 0 ]; then
  # Future adapter placement contract: the reference WAL adapter must be a `publish = false`
  # workspace member under crates/. Root cargo metadata cannot see tests/ fixtures or tools/-style
  # standalone sub-workspaces, which would make this forward-armed pass vacuous forever.
  if ! load_metadata full "$full_metadata_file"; then
    exit 1
  fi

  if ! check_transitive_verification_edges "$no_deps_metadata_file" "$full_metadata_file"; then
    exit 1
  fi
fi

echo "OK: default build links no database (selene-db etc.), no tokio, no async-std, no rayon; shipped crates have no forbidden direct edges; verification-only crates are unreachable from shipped crates."
