#!/usr/bin/env bash
# Fixtures for the default destination of install-wasmtime.sh (issue #545). Nothing is downloaded:
# every case asks only for `--print-destination`, or passes an argument the script must refuse
# before it reaches the network. The order under test is OCE_WASMTIME_DIR, then XDG_CACHE_HOME,
# then HOME, each read only when the one before it is unset or empty; a host with no HOME must
# still be served by OCE_WASMTIME_DIR alone, and must fail with a message when nothing is set.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

INSTALL="scripts/ci/install-wasmtime.sh"
failures=0

# expect <name> <wanted stdout> <wanted status> [VAR=value ...]: run with exactly those variables.
expect() {
  local name="$1" want="$2" want_status="$3" got status
  shift 3
  set +e
  got="$(env -u HOME -u XDG_CACHE_HOME -u OCE_WASMTIME_DIR "$@" bash "$INSTALL" --print-destination 2>/dev/null)"
  status=$?
  set -e
  if [ "$got" != "$want" ] || [ "$status" != "$want_status" ]; then
    echo "FAIL: install-wasmtime case '$name'"
    echo "  want: '$want' (exit $want_status)"
    echo "  got:  '$got' (exit $status)"
    failures=$((failures + 1))
  fi
}

expect explicit-directory-without-home /srv/wasmtime 0 OCE_WASMTIME_DIR=/srv/wasmtime
expect explicit-directory-wins /srv/wasmtime 0 \
  OCE_WASMTIME_DIR=/srv/wasmtime XDG_CACHE_HOME=/srv/cache HOME=/srv/user
expect cache-home-without-home /srv/cache/open-control-engine/wasmtime-bin 0 XDG_CACHE_HOME=/srv/cache
expect cache-home-before-home /srv/cache/open-control-engine/wasmtime-bin 0 \
  XDG_CACHE_HOME=/srv/cache HOME=/srv/user
expect home-alone /srv/user/.cache/open-control-engine/wasmtime-bin 0 HOME=/srv/user
expect empty-values-fall-through /srv/user/.cache/open-control-engine/wasmtime-bin 0 \
  OCE_WASMTIME_DIR= XDG_CACHE_HOME= HOME=/srv/user
expect nothing-set '' 1
expect nothing-but-empty-values '' 1 OCE_WASMTIME_DIR= XDG_CACHE_HOME= HOME=

# With nothing set the refusal names the variable that helps.
message="$(env -u HOME -u XDG_CACHE_HOME -u OCE_WASMTIME_DIR bash "$INSTALL" --print-destination 2>&1 || true)"
case "$message" in
  *OCE_WASMTIME_DIR*) ;;
  *)
    echo "FAIL: the no-directory refusal does not name OCE_WASMTIME_DIR: $message"
    failures=$((failures + 1))
    ;;
esac

# The default is never under `target/`, which `cargo clean` removes.
case "$(HOME=/srv/user env -u XDG_CACHE_HOME -u OCE_WASMTIME_DIR bash "$INSTALL" --print-destination)" in
  target/* | */target/*)
    echo "FAIL: the default wasmtime directory is under target/"
    failures=$((failures + 1))
    ;;
esac

# An unknown flag is refused with a usage error, not taken as a destination directory.
set +e
bash "$INSTALL" --destination /srv/wasmtime > /dev/null 2>&1
status=$?
set -e
if [ "$status" != 2 ]; then
  echo "FAIL: install-wasmtime took an unknown flag (exit $status, want 2)"
  failures=$((failures + 1))
fi

if [ "$failures" -ne 0 ]; then
  echo "FAIL: $failures install-wasmtime fixture(s) failed"
  exit 1
fi
echo "OK: install-wasmtime destination fixtures passed."
