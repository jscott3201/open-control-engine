#!/bin/sh
# Run the five event-trigger pilots in the pinned OpenModelica container and write their evidence
# under crates/oce-conformance/tests/fixtures/open_modelica/<family>_<class>/ (issue #670).
#
#   docker pull "$IMAGE"                              # once, by digest, with network
#   sh regenerate.sh <buildings checkout> <msl checkout>
#
# <buildings checkout> is lbl-srg/modelica-buildings and <msl checkout>
# OpenModelica/OpenModelica-ModelicaStandardLibrary at the commits every other OpenModelica case
# of the repository uses; both are checked by their git tree hash and must be clean. A shallow
# fetch of each commit is enough:
#
#   git init -q b && git -C b fetch -q --depth 1 \
#     https://github.com/lbl-srg/modelica-buildings.git a131864e4c4df22ebcd52bb8da439de0087ac365 \
#     && git -C b checkout -q FETCH_HEAD
#
# Each pilot runs in its own container: no network, the host user, 4 CPUs and 4 GB, both
# libraries and the pilots read-only, simulate.mos with @PILOT@ replaced by the pilot's name.
# For each pilot the script writes, beside the class's other files: the result as OpenModelica
# wrote it (<name>.raw.csv), the canonical tables canonicalize.py projects from it, omc's output
# (run.log) and provenance.json; the two extra pilots of a class (OnCounterChangePilot,
# IntegratorWithResetRampPilot) prefix their log and record with their scenario's name. With a
# third argument `pulses` it runs only the three pulse-event pilots of issue #800
# (PulseConsumersPilot, SampleTriggerPilot, SampleTriggerOffGridPilot) into pulse_events/, with
# simulate_pulses.mos. No test runs this script; the tests read those files.
# Docker images and libraries are left as they are.
set -eu

IMAGE='openmodelica/openmodelica@sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864'
BUILDINGS_COMMIT='a131864e4c4df22ebcd52bb8da439de0087ac365'
BUILDINGS_TREE='a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09'
MODELICA_COMMIT='7a4bf7de77a3986e8eb1e88cbb515d646f78f834'
MODELICA_TREE='43d7d8fc1a991358e9e5e91976e27cdc4280173f'
SETTINGS='cvode, tolerance 1e-6, 0 to 10 s, output every 1 s'
PROJECTION='at each frame instant 0 to 9 s, the last row within 1e-6 s of it'
COMMAND='docker run --rm --pull=never --network none --memory 4g --cpus 4 --user <host-uid>:<host-gid> -e HOME=/work -v <buildings>:/libs/buildings:ro -v <msl>:/libs/msl:ro -v tools/openmodelica-event-reference/events:/scripts:ro -v <run directory>:/work -w /work <image> omc /work/run.mos'

if [ "$#" -ne 2 ] && { [ "$#" -ne 3 ] || [ "$3" != pulses ]; }; then
  echo 'usage: sh regenerate.sh BUILDINGS_CHECKOUT MSL_CHECKOUT [pulses]' >&2
  exit 2
fi
ONLY="${3:-}"
BUILDINGS=$(CDPATH= cd -- "$1" && pwd)
MODELICA=$(CDPATH= cd -- "$2" && pwd)
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO=$(CDPATH= cd -- "$HERE/../../.." && pwd)
FIXTURES="$REPO/crates/oce-conformance/tests/fixtures/open_modelica"
test "$(git -C "$BUILDINGS" rev-parse HEAD)" = "$BUILDINGS_COMMIT"
test "$(git -C "$BUILDINGS" rev-parse 'HEAD^{tree}')" = "$BUILDINGS_TREE"
test "$(git -C "$MODELICA" rev-parse HEAD)" = "$MODELICA_COMMIT"
test "$(git -C "$MODELICA" rev-parse 'HEAD^{tree}')" = "$MODELICA_TREE"
test -z "$(git -C "$BUILDINGS" status --porcelain)"
test -z "$(git -C "$MODELICA" status --porcelain)"
docker image inspect "$IMAGE" --format '{{json .RepoDigests}}' | grep -Fq "${IMAGE#*@}"
OMC_VERSION=$(docker run --rm --pull=never --network none "$IMAGE" omc --version)
ARCHITECTURE=$(docker image inspect "$IMAGE" --format '{{.Architecture}}')

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

# run_pilot PILOT DIRECTORY RAW CLASS SOURCE PREFIX STEM...: run PILOT, write its result as
# <RAW>.raw.csv, its log as <PREFIX>run.log and its record as <PREFIX>provenance.json under
# DIRECTORY, beside the canonical tables <STEM>.canonical.csv that canonicalize.py writes.
run_pilot() {
  pilot=$1; slug=$2; raw=$3; class=$4; source=$5; prefix=$6
  shift 6
  out="$FIXTURES/$slug"
  mkdir -p "$out"
  work=$(mktemp -d)
  sed "s/@PILOT@/$pilot/g" "$HERE/simulate.mos" > "$work/run.mos"
  docker run --rm --pull=never --network none --memory 4g --cpus 4 \
    --user "$(id -u):$(id -g)" -e HOME=/work \
    -v "$BUILDINGS":/libs/buildings:ro -v "$MODELICA":/libs/msl:ro -v "$HERE":/scripts:ro \
    -v "$work":/work -w /work "$IMAGE" omc /work/run.mos > "$work/omc.log" 2>&1
  # omc reports its compile and simulation times; they vary run to run, so the committed log
  # drops them and a regenerated log is byte-identical.
  grep -v '^    time[A-Za-z]* = ' "$work/omc.log" > "$out/${prefix}run.log"
  grep -q 'The simulation finished successfully' "$out/${prefix}run.log" \
    || { cat "$out/${prefix}run.log" >&2; exit 1; }
  cp "$work/${pilot}_res.csv" "$out/$raw.raw.csv"
  rm -rf "$work"
  python3 -I "$HERE/canonicalize.py" "$pilot" "$out/$raw.raw.csv" "$out"
  tables=''
  also=''
  if [ -n "${ALSO:-}" ]; then
    also="
  \"also_source\": \"$ALSO\",
  \"also_source_sha256\": \"$(sha "$BUILDINGS/$ALSO")\","
  fi
  for stem in "$@"; do
    tables="$tables
  \"${stem}_sha256\": \"$(sha "$out/$stem.canonical.csv")\","
  done
  cat > "$out/${prefix}provenance.json" <<JSON
{
  "class": "$class",
  "pilot": "tools/openmodelica-event-reference/events/$pilot.mo",
  "pilot_sha256": "$(sha "$HERE/$pilot.mo")",
  "simulate_template_sha256": "$(sha "$HERE/simulate.mos")",
  "image": "$IMAGE",
  "omc": "$OMC_VERSION",
  "architecture": "$ARCHITECTURE",
  "buildings_commit": "$BUILDINGS_COMMIT",
  "buildings_tree": "$BUILDINGS_TREE",
  "msl_commit": "$MODELICA_COMMIT",
  "msl_tree": "$MODELICA_TREE",
  "class_source": "$source",
  "class_source_sha256": "$(sha "$BUILDINGS/$source")",$also
  "settings": "$SETTINGS",
  "projection": "$PROJECTION",
  "command": "$COMMAND",
  "raw": "$raw.raw.csv",$tables
  "raw_sha256": "$(sha "$out/$raw.raw.csv")"
}
JSON
  echo "wrote $slug/${prefix}provenance.json"
}

# run_pulse_pilot PILOT START STEM SOURCE...: run one pulse-event pilot (issue #800) from START s
# to START + 10 s with simulate_pulses.mos, write its result, log and record under
# integers_on_counter's sibling directory pulse_events/, and the canonical table STEM.
run_pulse_pilot() {
  pilot=$1; start=$2; stem=$3
  shift 3
  out="$FIXTURES/pulse_events"
  mkdir -p "$out"
  work=$(mktemp -d)
  stop=$(python3 -I -c "import sys; print(float(sys.argv[1]) + 10)" "$start")
  sed -e "s/@PILOT@/$pilot/g" -e "s/@START@/$start/g" -e "s/@STOP@/$stop/g" \
    "$HERE/simulate_pulses.mos" > "$work/run.mos"
  docker run --rm --pull=never --network none --memory 4g --cpus 4 \
    --user "$(id -u):$(id -g)" -e HOME=/work \
    -v "$BUILDINGS":/libs/buildings:ro -v "$MODELICA":/libs/msl:ro -v "$HERE":/scripts:ro \
    -v "$work":/work -w /work "$IMAGE" omc /work/run.mos > "$work/omc.log" 2>&1
  grep -v '^    time[A-Za-z]* = ' "$work/omc.log" > "$out/$stem.run.log"
  grep -q 'The simulation finished successfully' "$out/$stem.run.log" \
    || { cat "$out/$stem.run.log" >&2; exit 1; }
  cp "$work/${pilot}_res.csv" "$out/$stem.raw.csv"
  rm -rf "$work"
  python3 -I "$HERE/canonicalize.py" "$pilot" "$out/$stem.raw.csv" "$out"
  sources=''
  for source in "$@"; do
    sources="$sources
    \"$source\": \"$(sha "$BUILDINGS/$source")\","
  done
  sources="${sources%,}"
  cat > "$out/$stem.provenance.json" <<JSON
{
  "pilot": "tools/openmodelica-event-reference/events/$pilot.mo",
  "pilot_sha256": "$(sha "$HERE/$pilot.mo")",
  "simulate_template_sha256": "$(sha "$HERE/simulate_pulses.mos")",
  "image": "$IMAGE",
  "omc": "$OMC_VERSION",
  "architecture": "$ARCHITECTURE",
  "buildings_commit": "$BUILDINGS_COMMIT",
  "buildings_tree": "$BUILDINGS_TREE",
  "msl_commit": "$MODELICA_COMMIT",
  "msl_tree": "$MODELICA_TREE",
  "class_sources": {$sources
  },
  "settings": "cvode, tolerance 1e-6, $start to $stop s, output every 1 s",
  "projection": "at each frame instant $start + k s (k = 0 to 9), the last row within 1e-6 s of it",
  "command": "$COMMAND",
  "raw": "$stem.raw.csv",
  "canonical_sha256": "$(sha "$out/$stem.canonical.csv")",
  "raw_sha256": "$(sha "$out/$stem.raw.csv")"
}
JSON
  echo "wrote pulse_events/$stem.provenance.json"
}

CDL='Buildings/Controls/OBC/CDL'
L="$CDL/Logical"
CONSUMERS="$L/Change.mo $CDL/Discrete/TriggeredSampler.mo $CDL/Discrete/TriggeredMax.mo \
  $CDL/Discrete/TriggeredMovingMean.mo $CDL/Reals/IntegratorWithReset.mo $CDL/Reals/PIDWithReset.mo \
  $L/Latch.mo $L/Toggle.mo $L/TimerAccumulating.mo $CDL/Integers/OnCounter.mo $L/Not.mo $L/Or.mo \
  $L/Pre.mo"
SAMPLES="$L/Sources/SampleTrigger.mo $CDL/Integers/OnCounter.mo $CDL/Discrete/TriggeredSampler.mo \
  $L/Not.mo $L/Latch.mo $CDL/Discrete/TriggeredMovingMean.mo $L/Pre.mo $CDL/Discrete/TriggeredMax.mo \
  $CDL/Reals/IntegratorWithReset.mo $CDL/Reals/PIDWithReset.mo $L/TimerAccumulating.mo $L/Toggle.mo \
  $L/And.mo"
if [ "$ONLY" = pulses ]; then
  # shellcheck disable=SC2086
  run_pulse_pilot PulseConsumersPilot 0.0 consumers $CONSUMERS
  # shellcheck disable=SC2086
  run_pulse_pilot SampleTriggerPilot 0.0 sample_triggers $SAMPLES
  # shellcheck disable=SC2086
  run_pulse_pilot SampleTriggerOffGridPilot 0.25 sample_trigger_off_grid $SAMPLES
  exit 0
fi
TWO='initially_true rising_after_start'
# shellcheck disable=SC2086
run_pilot OnCounterPilot integers_on_counter on_counter CDL.Integers.OnCounter "$CDL/Integers/OnCounter.mo" '' $TWO
# The pulse pilot's result depends on Change too: its record binds that source as well.
ALSO="$CDL/Logical/Change.mo"
run_pilot OnCounterChangePilot integers_on_counter change_pulses CDL.Integers.OnCounter "$CDL/Integers/OnCounter.mo" change_pulses. change_pulses
ALSO=''
# shellcheck disable=SC2086
run_pilot TriggeredSamplerPilot discrete_triggered_sampler triggered_sampler CDL.Discrete.TriggeredSampler "$CDL/Discrete/TriggeredSampler.mo" '' $TWO
# shellcheck disable=SC2086
run_pilot TriggeredMaxPilot discrete_triggered_max triggered_max CDL.Discrete.TriggeredMax "$CDL/Discrete/TriggeredMax.mo" '' $TWO
# shellcheck disable=SC2086
run_pilot IntegratorWithResetPilot reals_integrator_with_reset integrator_with_reset CDL.Reals.IntegratorWithReset "$CDL/Reals/IntegratorWithReset.mo" '' $TWO
run_pilot IntegratorWithResetRampPilot reals_integrator_with_reset ramp CDL.Reals.IntegratorWithReset "$CDL/Reals/IntegratorWithReset.mo" ramp. ramp
# shellcheck disable=SC2086
run_pilot PIDWithResetPilot reals_pid_with_reset pid_with_reset CDL.Reals.PIDWithReset "$CDL/Reals/PIDWithReset.mo" '' $TWO
