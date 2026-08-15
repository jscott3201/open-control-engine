#!/bin/sh
# Exact-ID Docker cleanup for one labelled OpenModelica run.

valid_container_id() {
  test "${#1}" -eq 64 && case "$1" in *[!0-9a-f]*) return 1 ;; *) return 0 ;; esac
}

cleanup_container() {
  cleanup_label=$1
  cleanup_file=$2
  cleanup_known=${3-}
  cleanup_id=$cleanup_known
  if [ -z "$cleanup_id" ] && [ -n "$cleanup_file" ] && [ -f "$cleanup_file" ]; then
    cleanup_id=$(cat "$cleanup_file" 2>/dev/null || true)
  fi
  if ! valid_container_id "$cleanup_id"; then
    cleanup_id=
    if ! cleanup_matches=$(run_timed 5 docker ps -aq --no-trunc --filter "label=oce.line.run=$cleanup_label" 2>/dev/null); then
      printf 'OpenModelica Line cleanup query failed for run label %s\n' "$cleanup_label" >&2
      return 1
    fi
    cleanup_count=$(printf '%s\n' "$cleanup_matches" | sed '/^$/d' | wc -l | tr -d ' ')
    if [ "$cleanup_count" -gt 1 ]; then
      printf 'OpenModelica Line cleanup found multiple containers for run label %s; manual cleanup required\n' "$cleanup_label" >&2
      return 1
    fi
    if [ "$cleanup_count" -eq 1 ]; then cleanup_id=$cleanup_matches; fi
  fi
  if [ -z "$cleanup_id" ]; then return 0; fi
  if ! valid_container_id "$cleanup_id"; then
    printf 'OpenModelica Line cleanup rejected invalid container ID for run label %s\n' "$cleanup_label" >&2
    return 1
  fi
  if ! run_timed 5 docker rm -f "$cleanup_id" >/dev/null 2>&1; then
    printf 'OpenModelica Line cleanup failed for container %s (run label %s)\n' "$cleanup_id" "$cleanup_label" >&2
    return 1
  fi
}
