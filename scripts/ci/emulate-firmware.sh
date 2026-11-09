#!/usr/bin/env bash
# The emulation check (issue #402): runs the firmware image under QEMU on each bare-metal target and
# diffs its output against the host reference runner's, line for line. `.github/workflows/qemu.yml`
# runs it nightly and on dispatch; `bash .agents/gate.sh qemu` runs it locally. Never per PR: the
# per-PR bare-metal cell builds the same images and checks their size (check-firmware-link.sh).
# The QEMU invocation is rusty-bacnet's scripts/emu/run-qemu.sh's (same owner).
#
#   emulate-firmware.sh [<timeout seconds>]       default 120 per image
#
# 1. The host reference: `cargo run -p oce-firmware-link --bin blob-frames` on the host writes the
#    run's lines (the fixture, a run digest after every frame, the final digest against
#    crates/oce-graph/tests/fixtures/blob_frame_digests.tsv, the verdict) and must exit 0.
# 2. Per target: the image built with the `firmware` profile, as the per-PR cell builds it, then
#    run on the target's QEMU machine with semihosting, so its lines reach stdout and its exit code
#    becomes QEMU's: 0 PASS, 1 MISMATCH, 3 FAULT (the engine refused), 101 PANIC. QEMU itself also
#    exits 1 when it cannot start (an unknown machine, an unloadable -kernel). An image still
#    running at the timeout is killed and fails as 124: a hang, or a CPU fault, which lands in the
#    runtime's spinning default handler.
# 3. The image's lines, less its `heap` line (the arena's use, which a host has no equivalent of),
#    must equal the host reference's byte for byte.
#
# Machines: `netduinoplus2` (STM32F405, Cortex-M4F) for thumbv7em-none-eabihf; the riscv32 `virt`
# machine with `-bios none` for riscv32imac-unknown-none-elf (rusty-bacnet's `sifive_e` has 16 KiB
# of RAM, too little for the image's allocator arena). The image's memory layouts match them
# (firmware/link/boards/). Needs qemu-system-arm and qemu-system-riscv32 on PATH (Debian/Ubuntu:
# qemu-system-arm and qemu-system-misc; macOS: `brew install qemu`) and both targets installed; it
# installs nothing. Outputs land in target/emulation/.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2

timeout=${1:-120}
case $timeout in '' | *[!0-9]* | 0*)
  echo "error: the timeout is a positive whole number of seconds, not '$timeout'" >&2
  exit 2
  ;;
esac

package=oce-firmware-link
image=blob-frames
# <target>|<qemu binary>|<machine arguments>
jobs=(
  "thumbv7em-none-eabihf|qemu-system-arm|-machine netduinoplus2"
  "riscv32imac-unknown-none-elf|qemu-system-riscv32|-machine virt -bios none"
)
out=target/emulation
mkdir -p "$out"

# Run "$@" with stdin closed, polling every tenth of a second, and kill it when $timeout seconds
# have passed: its exit status, or 124 when it ran out of time. Needs nothing beyond bash and sleep,
# unlike timeout(1), which macOS lacks.
bounded() {
  "$@" </dev/null &
  local pid=$! ticks=$((timeout * 10))
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$ticks" -le 0 ]; then
      kill -KILL "$pid" 2>/dev/null
      wait "$pid" 2>/dev/null
      return 124
    fi
    sleep 0.1
    ticks=$((ticks - 1))
  done
  wait "$pid"
}

for job in "${jobs[@]}"; do
  IFS='|' read -r _ qemu _ <<<"$job"
  command -v "$qemu" >/dev/null || { echo "error: $qemu is not on PATH" >&2; exit 2; }
  echo "$qemu: $("$qemu" --version | head -n 1)"
done

target_dir=$(cargo metadata --format-version 1 --no-deps --locked |
  python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])') || exit 2

echo
echo "==> host reference ($package --bin $image on the host)"
if ! cargo run -q -p "$package" --bin "$image" --locked >"$out/host.txt"; then
  echo "FAIL: the host reference runner did not pass:" >&2
  tail -n 3 "$out/host.txt" >&2
  exit 1
fi
tail -n 2 "$out/host.txt"

results=()
failed=0
for job in "${jobs[@]}"; do
  IFS='|' read -r triple qemu machine <<<"$job"
  echo
  echo "==> $image on $qemu $machine ($triple)"
  if ! cargo build -q -p "$package" --locked --target "$triple" --profile firmware; then
    results+=("$(printf '%-30s %s' "$triple" 'FAIL: the image did not build')")
    failed=1
    continue
  fi
  log="$out/$triple.txt"
  # No display, monitor or serial port: the image talks only through semihosting.
  # shellcheck disable=SC2086 # the machine arguments are words
  bounded "$qemu" $machine -nodefaults -display none -monitor none -serial none \
    -semihosting-config enable=on,target=native \
    -kernel "$target_dir/$triple/firmware/$image" >"$log" 2>&1
  status=$?
  tail -n 3 "$log"
  if [ "$status" -ne 0 ]; then
    case $status in
      1) result="FAIL: exit 1: MISMATCH, or QEMU failed to start; see the log ($log)" ;;
      3) result='FAIL: FAULT, the engine refused (exit 3)' ;;
      101) result='FAIL: PANIC (exit 101)' ;;
      124) result="FAIL: still running after ${timeout}s (a hang, or a CPU fault)" ;;
      *) result="FAIL: exit status $status" ;;
    esac
    failed=1
  elif ! diff -u "$out/host.txt" <(grep -v '^heap ' "$log") >"$out/$triple.diff"; then
    result="FAIL: exit 0, but its lines differ from the host reference ($out/$triple.diff)"
    head -n 20 "$out/$triple.diff"
    failed=1
  else
    result="pass: $(grep -c '^frame ' "$log") frames, every line equal to the host reference"
  fi
  results+=("$(printf '%-30s %s' "$triple" "$result")")
done

echo
echo "Emulation results:"
printf '  %s\n' "${results[@]}"
exit $failed
