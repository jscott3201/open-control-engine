#!/usr/bin/env bash
# Shared truthiness policy for repository-controlled shell opt-ins.
#
# This pins the ASCII domain only. Rust's `str::trim` also trims Unicode whitespace,
# while this shell implementation deliberately forces the locale-independent C rules.

oce_enabled_for() {
  local value folded
  value="${1:-}"
  value="$(LC_ALL=C sed 's/^[[:space:]]*//; s/[[:space:]]*$//' <<<"${value}")"
  folded="$(LC_ALL=C tr '[:upper:]' '[:lower:]' <<<"${value}")"
  folded="${folded%$'\n'}"

  case "${folded}" in
    "" | 0 | false) return 1 ;;
    *) return 0 ;;
  esac
}
