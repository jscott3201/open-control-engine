#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/container_cleanup.sh"
ROOT="$SCRIPT_DIR/.container-cleanup-test-$$"; nonce=0
while ! mkdir -m 700 "$ROOT-$nonce" 2>/dev/null; do nonce=$((nonce + 1)); test "$nonce" -lt 1024; done
ROOT="$ROOT-$nonce"; trap 'rm -rf "$ROOT"' EXIT HUP INT TERM
ID_A='aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
ID_B='bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'
CALL_LOG="$ROOT/calls"; : > "$CALL_LOG"; PS_RESULT=; PS_STATUS=0; RM_STATUS=0
run_timed() {
  shift; test "$1" = docker; shift
  if [ "$1" = ps ]; then
    printf 'ps:%s;' "$*" >> "$CALL_LOG"; test "$PS_STATUS" -eq 0 || return "$PS_STATUS"; printf '%s' "$PS_RESULT"
  else
    printf 'rm:%s;' "$*" >> "$CALL_LOG"; return "$RM_STATUS"
  fi
}
printf '%s\n' "$ID_A" > "$ROOT/cid"
cleanup_container alpha "$ROOT/cid" ''
test "$(cat "$CALL_LOG")" = "rm:rm -f $ID_A;"
: > "$CALL_LOG"; : > "$ROOT/cid"; PS_RESULT=$ID_B
cleanup_container beta "$ROOT/cid" ''
test "$(cat "$CALL_LOG")" = "ps:ps -aq --no-trunc --filter label=oce.reliefs.run=beta;rm:rm -f $ID_B;"
: > "$CALL_LOG"; PS_RESULT="$ID_A
$ID_B"
if cleanup_container gamma "$ROOT/cid" '' 2> "$ROOT/multiple"; then exit 1; fi
grep -Fq 'multiple containers' "$ROOT/multiple"
: > "$CALL_LOG"; PS_RESULT=; PS_STATUS=1; printf '%s\n' output-cleaned > "$ROOT/output-marker"
if cleanup_container delta "$ROOT/cid" '' 2> "$ROOT/query"; then exit 1; fi
test "$(cat "$ROOT/output-marker")" = output-cleaned
grep -Fq 'cleanup query failed' "$ROOT/query"
printf '%s\n' 'container cleanup test passed'
