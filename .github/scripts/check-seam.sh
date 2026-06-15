#!/usr/bin/env bash
# SEAM gate (FRAME D-OWNER-1 / R-SEAM-1, R-SEAM-2).
#
# The execution core, the store ports, and the Group-C crates spec'd selene-free must be
# selene-free: only `oce-store-selene` (the adapter crate) may COUPLE to selene-db. This gate
# fails if any Group-A crate, `oce-store` / `oce-store-mem`, or any Group-C selene-free crate
# (`oce-conformance` / `oce-extension` / `oce-docs`) actually couples to selene — i.e.
#   * a `selene*` dependency in its Cargo.toml, or
#   * a selene-db identifier in its Rust source (a `use`/`extern crate`/path reference to
#     `selene_core`/`selene_graph`/`selene_persist`/`selene_gql`/`selene_algorithms`, the
#     `selene-db` package name, or a bare `selene::`/`selene_` code reference).
#
# It deliberately does NOT flag the English word "selene" in doc comments (`//!`, `//`, `#`), so
# the very invariant these crates document ("this crate is selene-free / no selene-db") does not
# trip the gate. The structural coupling is what matters, not prose.
#
# Runs from repo root; macOS bash 3.x compatible.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Group A (execution core) + the selene-free store ports + the Group-C crates spec'd selene-free
# (FRAME §5: oce-conformance / oce-extension / oce-docs carry no selene-db). NONE of these may
# couple to selene — only oce-store-selene may, behind the `selene` feature. Guarding the Group-C
# crates makes their selene-free invariant enforced rather than merely currently-true.
GUARDED_CRATES="
crates/oce-model
crates/oce-expr
crates/oce-blocks
crates/oce-flatten
crates/oce-validate
crates/oce-graph
crates/oce-cxf
crates/oce-semantics
crates/oce-store
crates/oce-store-mem
crates/oce-conformance
crates/oce-extension
crates/oce-docs
"

violations=0

# A Cargo.toml line that pulls a selene crate: a dependency key beginning with `selene`, or any
# `package = "selene-db-..."` / git/path reference to selene-db. Comment lines (`#...`) are ignored.
DEP_PATTERN='^[[:space:]]*(selene[a-z_-]*[[:space:]]*=|.*package[[:space:]]*=[[:space:]]*"selene)'

# A Rust line that references a selene identifier in CODE (not a comment). We strip leading
# whitespace then exclude lines that start with `//`; then look for real selene-db identifiers.
CODE_PATTERN='(\bselene_(core|graph|persist|gql|algorithms)\b|\bselene-db\b|\bselene::|\bextern[[:space:]]+crate[[:space:]]+selene)'

check_file() {
  local f="$1"
  case "$f" in
    *Cargo.toml)
      if grep -nE "$DEP_PATTERN" -- "$f" 2>/dev/null | grep -vqE '^[0-9]+:[[:space:]]*#'; then
        echo "FAIL: selene dependency in selene-free crate manifest: $f"
        grep -nE "$DEP_PATTERN" -- "$f" | grep -vE '^[0-9]+:[[:space:]]*#' || true
        violations=$((violations + 1))
      fi
      ;;
    *.rs)
      # Drop pure-comment lines, strip any TRAILING `// ...` comment (so a selene word in a
      # trailing comment can't trip the gate — `//` inside a string literal is a rare non-issue at
      # this scale), then search the remainder for selene identifiers. The `N:` line-number prefix
      # from `grep -n` never contains `//`, so the strip is safe for reporting.
      local stripped
      stripped="$(grep -nvE '^[[:space:]]*//' -- "$f" 2>/dev/null | sed 's://.*::')"
      if printf '%s\n' "$stripped" | grep -qE "$CODE_PATTERN"; then
        echo "FAIL: selene-db identifier in selene-free crate source: $f"
        printf '%s\n' "$stripped" | grep -E "$CODE_PATTERN" || true
        violations=$((violations + 1))
      fi
      ;;
  esac
}

for crate in $GUARDED_CRATES; do
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    check_file "$f"
  done < <(git ls-files "$crate/**/*.rs" "$crate/Cargo.toml" 2>/dev/null || true)
done

if [ "$violations" -gt 0 ]; then
  echo
  echo "The execution core (Group A), the store ports (oce-store, oce-store-mem), and the Group-C"
  echo "selene-free crates (oce-conformance, oce-extension, oce-docs) must not couple to selene-db"
  echo "(FRAME D-OWNER-1). Only oce-store-selene may name selene. Move any selene-touching code"
  echo "into oce-store-selene behind the \`selene\` feature."
  exit 1
fi

echo "OK: seam gate clean — no selene-db coupling in Group A / oce-store / oce-store-mem."
