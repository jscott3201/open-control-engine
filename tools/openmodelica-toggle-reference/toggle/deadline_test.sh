#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/deadline.sh"

CLOCK=0
CALLS=
monotonic_seconds() { printf '%s\n' "$CLOCK"; }
run_timed() {
  timeout=$1
  operation=$2
  CALLS="${CALLS}${operation}:${timeout},"
  case "$operation" in
    poll) CLOCK=45; return 1 ;;
    logs) test "$timeout" -eq 75; CLOCK=46 ;;
    overrun) CLOCK=121 ;;
    *) return 2 ;;
  esac
}

deadline_start 120
if deadline_call poll; then exit 1; else test "$?" -eq 1; fi
test "$DEADLINE_REMAINING" -eq 75
deadline_call logs
test "$CALLS" = 'poll:120,logs:75,'

CLOCK=0
CALLS=
deadline_start 120
if deadline_call overrun; then exit 1; else test "$?" -eq 124; fi
test "$CALLS" = 'overrun:120,'
printf '%s\n' 'deadline accounting test passed'
