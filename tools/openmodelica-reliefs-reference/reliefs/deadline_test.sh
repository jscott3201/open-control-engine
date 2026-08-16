#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/deadline.sh"
STATE=$(mktemp -d "${TMPDIR:-/tmp}/oce-reliefs-deadline.XXXXXX")
trap 'rm -rf "$STATE"' EXIT HUP INT TERM
set_clock() { printf '%s\n' "$1" > "$STATE/clock"; }
clear_calls() { : > "$STATE/calls"; }
calls() { IFS= read -r value < "$STATE/calls" || true; printf '%s\n' "${value-}"; }
set_clock 0
clear_calls
EXEC_ADVANCE=0
EXEC_STATUS=1
INSPECT_ADVANCE=0
monotonic_seconds() { IFS= read -r value < "$STATE/clock"; printf '%s\n' "$value"; }
run_timed() {
  timeout=$1; operation=$2; subcommand=${3-}; printf '%s' "${operation}:${subcommand}:${timeout}," >> "$STATE/calls"
  IFS= read -r clock < "$STATE/clock"
  case "$operation" in
    poll) set_clock 45; return 1 ;;
    logs) test "$timeout" -eq 75; set_clock 46 ;;
    overrun) set_clock 121 ;;
    docker)
      case "$subcommand" in
        exec) set_clock $((clock + EXEC_ADVANCE)); return "$EXEC_STATUS" ;;
        inspect) set_clock $((clock + INSPECT_ADVANCE)); printf '%s\n' true ;;
        *) return 2 ;;
      esac
      ;;
    sleep) set_clock $((clock + subcommand)) ;;
    *) return 2 ;;
  esac
}
deadline_start 120
if deadline_call poll; then exit 1; else test "$?" -eq 1; fi
test "$DEADLINE_REMAINING" -eq 75
deadline_call logs
test "$(calls)" = 'poll::120,logs::75,'
set_clock 0; clear_calls; deadline_start 120
if deadline_call overrun; then exit 1; else test "$?" -eq 124; fi
test "$(calls)" = 'overrun::120,'

# The entrypoint poll cycle refreshes between exec and inspect and before its pause.
set_clock 110; clear_calls; EXEC_ADVANCE=2; EXEC_STATUS=1; INSPECT_ADVANCE=3; deadline_start 120; DEADLINE_STARTED=0
if deadline_poll_cycle abcdef012345; then exit 1; else test "$?" -eq 1; fi
test "$(monotonic_seconds)" -eq 116
test "$(calls)" = 'docker:exec:5,docker:inspect:5,sleep:1:5,'

# An exec that consumes the deadline prevents stale reuse by inspect or sleep.
set_clock 115; clear_calls; EXEC_ADVANCE=5; EXEC_STATUS=1; INSPECT_ADVANCE=0; deadline_start 120; DEADLINE_STARTED=0
if deadline_poll_cycle abcdef012345; then exit 1; else test "$?" -eq 124; fi
test "$(monotonic_seconds)" -eq 120
test "$(calls)" = 'docker:exec:5,'
printf '%s\n' 'deadline accounting test passed'
