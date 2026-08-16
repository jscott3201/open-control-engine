#!/bin/sh
# One deadline covers container start, polling, log capture, and output extraction.

deadline_start() {
  DEADLINE_LIMIT=$1
  DEADLINE_STARTED=$(monotonic_seconds)
  DEADLINE_REMAINING=$DEADLINE_LIMIT
}

deadline_refresh() {
  deadline_now=$(monotonic_seconds)
  DEADLINE_REMAINING=$((DEADLINE_LIMIT - (deadline_now - DEADLINE_STARTED)))
  test "$DEADLINE_REMAINING" -gt 0
}

deadline_call() {
  if ! deadline_refresh; then return 124; fi
  deadline_timeout=$DEADLINE_REMAINING
  if run_timed "$deadline_timeout" "$@"; then deadline_status=0; else deadline_status=$?; fi
  if ! deadline_refresh; then return 124; fi
  return "$deadline_status"
}
