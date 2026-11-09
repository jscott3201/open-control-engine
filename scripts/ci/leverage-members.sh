#!/usr/bin/env bash
# The leverage lane (issue #393): crates under crates/leverage/<name>, above the facade.
#
#   bash scripts/ci/leverage-members.sh list    # leverage member package names, one per line
#   bash scripts/ci/leverage-members.sh check   # workspace layout + the dependency rule
#   bash scripts/ci/leverage-members.sh lane    # check, then fmt, clippy, rustdoc, nextest, machete
#   bash scripts/ci/leverage-members.sh check-tree <member> [--dev] [allowed...] < cargo-tree-output
#
# `lane` is what the `leverage-lane` job in .github/workflows/pr-gate.yml runs and what
# `bash .agents/gate.sh leverage` runs locally: per member `cargo fmt --check`, clippy -D warnings,
# rustdoc -D warnings and nextest (`--no-tests=fail`, as everywhere in this repo), then
# cargo-machete over crates/leverage. With no leverage crates it prints "no leverage crates yet"
# after the layout check and exits 0. Every command runs even after one fails, so one run reports
# every problem.
#
# THE DEPENDENCY RULE. A leverage crate depends on `oce-api` only, never on a core crate directly.
# Concretely: among packages named `oce-*`, the member's DIRECT dependencies of every kind (normal,
# build and dev — a dev-dependency on an engine crate would let its tests reach past the facade)
# may name only `oce-api` and other leverage members, with one exception: `oce-bless`, the repo's
# single golden-bless switch, is allowed as a DEV-dependency only (authoring note decision 19,
# first used by oce-replay, issue #445); as a normal or build dependency it still fails. What
# `oce-api` itself depends on is the facade's business and is not inspected. The check reads
# `cargo tree --depth 1` twice, normal and build edges and then dev edges (`--dev`); it reports
# real package names, so a dependency renamed in Cargo.toml cannot hide.
#
# THE LAYOUT RULE. Every directory crates/<name>/, crates/leverage/<name>/ or firmware/<name>/
# that holds a Cargo.toml must be a workspace member. The root Cargo.toml selects members by the
# globs `crates/oce-*`, `crates/leverage/*` and `firmware/*` (the bare-metal images, issue #402,
# which build with the engine lane); a crate outside them would otherwise be built and tested by
# nothing.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

usage() {
  echo "usage: bash scripts/ci/leverage-members.sh list|check|lane|check-tree <member> [--dev] [allowed...]" >&2
  exit 2
}

# Workspace member manifests, repository-relative, one per line.
member_manifests() {
  cargo metadata --format-version 1 --no-deps --locked | python3 -c '
import json, os, sys
meta = json.load(sys.stdin)
root = meta["workspace_root"]
members = set(meta["workspace_members"])
for package in meta["packages"]:
    if package["id"] in members:
        print(os.path.relpath(package["manifest_path"], root) + "\t" + package["name"])
'
}

list_members() {
  member_manifests | awk -F '\t' '$1 ~ /^crates\/leverage\/[^\/]+\/Cargo\.toml$/ { print $2 }' | sort
}

# Reads `cargo tree --depth 1 --prefix none --format {p}` output for one member on stdin. The first
# package line is the member itself; every later one is a direct dependency. With `--dev` the
# listing is the member's dev edges, where `oce-bless` is also allowed.
check_tree() {
  [ "$#" -ge 1 ] || usage
  local member="$1" dev=""
  shift
  if [ "${1:-}" = --dev ]; then
    dev=1
    shift
  fi
  local allowed=" oce-api $* " line name first="" bad=()
  [ -z "$dev" ] || allowed="$allowed oce-bless "
  while IFS= read -r line; do
    case "$line" in '' | '['*) continue ;; esac
    name="${line%% *}"
    if [ -z "$first" ]; then
      first="$name"
      if [ "$first" != "$member" ]; then
        echo "FAIL: cargo tree output starts at $first, not at the leverage member $member"
        return 1
      fi
      continue
    fi
    case "$name" in
      oce-*) [[ "$allowed" == *" $name "* ]] || bad+=("$name") ;;
    esac
  done
  if [ -z "$first" ]; then
    echo "FAIL: no cargo tree output for the leverage member $member"
    return 1
  fi
  if [ "${#bad[@]}" -ne 0 ]; then
    echo "FAIL: leverage crate $member depends directly on ${bad[*]}${dev:+ (dev)}; leverage crates depend on oce-api only (and oce-bless as a dev-dependency)"
    return 1
  fi
}

check() {
  local manifests on_disk missing members member status=0
  manifests="$(member_manifests | cut -f1 | sort)"
  on_disk="$(git ls-files --cached --others --exclude-standard -- 'crates/*/Cargo.toml' 'crates/leverage/*/Cargo.toml' \
    'firmware/*/Cargo.toml' | grep -E '^(crates/([^/]+|leverage/[^/]+)|firmware/[^/]+)/Cargo\.toml$' | sort -u || true)"
  missing="$(comm -13 <(printf '%s\n' "$manifests") <(printf '%s\n' "$on_disk") | sed '/^$/d')"
  if [ -n "$missing" ]; then
    echo "FAIL: crate manifests that are not workspace members (name core crates oce-*, put the rest under crates/leverage/, firmware images under firmware/):"
    printf '  %s\n' $missing
    status=1
  fi
  members="$(list_members)"
  for member in $members; do
    # shellcheck disable=SC2086 # one word per member name
    if ! cargo tree -p "$member" --depth 1 --prefix none -e normal,build --format '{p}' --locked \
      | check_tree "$member" $members; then
      status=1
    fi
    # shellcheck disable=SC2086 # one word per member name
    if ! cargo tree -p "$member" --depth 1 --prefix none -e dev --format '{p}' --locked \
      | check_tree "$member" --dev $members; then
      status=1
    fi
  done
  [ "$status" -eq 0 ] && echo "OK: leverage layout and dependency rule hold ($(printf '%s\n' $members | grep -c . || true) member(s))."
  return "$status"
}

lane() {
  local members member status=0
  check || status=1
  members="$(list_members)"
  if [ -z "$members" ]; then
    echo "no leverage crates yet"
    return "$status"
  fi
  for member in $members; do
    echo "── leverage member: $member"
    cargo fmt -p "$member" --check || status=1
    cargo clippy -p "$member" --all-targets --locked -- -D warnings || status=1
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p "$member" --document-private-items --locked || status=1
    cargo nextest run -p "$member" --locked --profile ci --no-tests=fail || status=1
  done
  cargo machete crates/leverage || status=1
  return "$status"
}

case "${1:-}" in
  list) list_members ;;
  check) check ;;
  lane) lane ;;
  check-tree) shift; check_tree "$@" ;;
  *) usage ;;
esac
