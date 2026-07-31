#!/usr/bin/env bash
# Shared truthiness policy for repository-controlled shell opt-ins.
#
# This pins the ASCII domain only. Rust's `str::trim` also trims Unicode whitespace,
# while this shell implementation deliberately forces the locale-independent C rules.

oce_enabled_for() {
  local LC_ALL=C value folded
  value="${1:-}"
  while [[ "${value}" == [[:space:]]* ]]; do
    value="${value#?}"
  done
  while [[ "${value}" == *[[:space:]] ]]; do
    value="${value%?}"
  done
  folded="$(tr '[:upper:]' '[:lower:]' <<<"${value}")"

  case "${folded}" in
    "" | 0 | false) return 1 ;;
    *) return 0 ;;
  esac
}
