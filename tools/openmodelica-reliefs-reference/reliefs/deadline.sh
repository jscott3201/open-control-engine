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

deadline_call_capped() {
  deadline_cap=$1
  shift
  if ! deadline_refresh; then return 124; fi
  deadline_timeout=$DEADLINE_REMAINING
  if [ "$deadline_timeout" -gt "$deadline_cap" ]; then deadline_timeout=$deadline_cap; fi
  if run_timed "$deadline_timeout" "$@"; then deadline_status=0; else deadline_status=$?; fi
  if ! deadline_refresh; then return 124; fi
  return "$deadline_status"
}

deadline_pause() {
  deadline_delay=$1
  if ! deadline_refresh || [ "$DEADLINE_REMAINING" -le "$deadline_delay" ]; then return 124; fi
  if run_timed "$DEADLINE_REMAINING" sleep "$deadline_delay"; then deadline_status=0; else deadline_status=$?; fi
  if ! deadline_refresh; then return 124; fi
  return "$deadline_status"
}

# Return 0 when output is complete, 1 while the container is running, 124 at
# the shared deadline, or 125 when Docker reports that the container stopped.
deadline_poll_cycle() {
  deadline_container=$1
  if deadline_call_capped 5 docker exec "$deadline_container" test -f /out/.oce-complete; then
    return 0
  else
    deadline_poll_status=$?
  fi
  if [ "$deadline_poll_status" -eq 124 ]; then return 124; fi
  if deadline_state=$(deadline_call_capped 5 docker inspect "$deadline_container" --format '{{.State.Running}}'); then
    deadline_inspect_status=0
  else
    deadline_inspect_status=$?
  fi
  if [ "$deadline_inspect_status" -eq 124 ]; then return 124; fi
  if [ "$deadline_inspect_status" -ne 0 ] || [ "$deadline_state" != true ]; then return 125; fi
  if ! deadline_pause 1; then return 124; fi
  return 1
}
