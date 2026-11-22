#!/bin/sh
# Simulate upstream's CoolingCoilValve on the 78 synthetic frames of valve_oracle.txt in the
# pinned OpenModelica container and write its valve signal at the frame instants (issue #787),
# twice: as the oracle holds the frames (CoolingCoilValveFrames.mo -> yCooVal_frames.csv), and
# with the supply air at 290 K from the re-enable at 382 s (CoolingCoilValveReenableProbe.mo ->
# yCooVal_reenable_probe.csv), where OpenModelica resets the integrator and the engine does not
# (issue #800).
#
#   docker pull "$IMAGE"                              # once, by digest, with network
#   sh run_openmodelica_frames.sh <buildings checkout> <msl checkout>
#
# The two checkouts are the ones run_openmodelica.sh takes, at the same commits, checked by
# their git tree hash. The script first writes both wrappers from valve_oracle.txt's input
# columns (frames_wrapper.py) and stops if a committed wrapper differs: each run must see the
# frames the tests drive the engine with. Each run is its own container, with no network, as the
# host user, with 4 CPUs and 4 GB, both libraries read-only, and simulate_frames.mos with @MODEL@
# replaced by the wrapper's name. The model is upstream's CoolingCoilValve as it is at the pin, fed
# by one CombiTimeTable with constant segments (each frame's inputs held until the next frame,
# as the engine holds them), under cvode at tolerance 1e-6. The script writes the two signals and
# provenance_frames.json beside itself; no test runs it, and the tests read those files. Docker
# images and libraries are left as they are.
set -eu

IMAGE='openmodelica/openmodelica@sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864'
BUILDINGS_COMMIT='a131864e4c4df22ebcd52bb8da439de0087ac365'
BUILDINGS_TREE='a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09'
MODELICA_COMMIT='7a4bf7de77a3986e8eb1e88cbb515d646f78f834'
MODELICA_TREE='43d7d8fc1a991358e9e5e91976e27cdc4280173f'
MODEL='Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve'
COMMAND='docker run --rm --pull=never --network none --memory 4g --cpus 4 --user <host-uid>:<host-gid> -e HOME=/work -v <buildings>:/libs/buildings:ro -v <msl>:/libs/msl:ro -v <this directory>:/scripts:ro -v <run directory>:/work -w /work <image> omc /work/run.mos (run.mos: simulate_frames.mos with @MODEL@ replaced)'

if [ "$#" -ne 2 ]; then
  echo 'usage: sh run_openmodelica_frames.sh BUILDINGS_CHECKOUT MSL_CHECKOUT' >&2
  exit 2
fi
BUILDINGS=$(CDPATH= cd -- "$1" && pwd)
MODELICA=$(CDPATH= cd -- "$2" && pwd)
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
test "$(git -C "$BUILDINGS" rev-parse HEAD)" = "$BUILDINGS_COMMIT"
test "$(git -C "$BUILDINGS" rev-parse 'HEAD^{tree}')" = "$BUILDINGS_TREE"
test "$(git -C "$MODELICA" rev-parse HEAD)" = "$MODELICA_COMMIT"
test "$(git -C "$MODELICA" rev-parse 'HEAD^{tree}')" = "$MODELICA_TREE"
test -z "$(git -C "$BUILDINGS" status --porcelain)"
test -z "$(git -C "$MODELICA" status --porcelain)"
docker image inspect "$IMAGE" --format '{{json .RepoDigests}}' | grep -Fq "${IMAGE#*@}"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
python3 -I "$HERE/frames_wrapper.py" "$HERE/../valve_oracle.txt" > "$WORK/frames.mo"
cmp "$WORK/frames.mo" "$HERE/CoolingCoilValveFrames.mo"
python3 -I "$HERE/frames_wrapper.py" "$HERE/../valve_oracle.txt" probe > "$WORK/probe.mo"
cmp "$WORK/probe.mo" "$HERE/CoolingCoilValveReenableProbe.mo"

run() {
  model=$1; signal=$2; shift 2
  mkdir "$WORK/$model"
  sed "s/@MODEL@/$model/g" "$HERE/simulate_frames.mos" > "$WORK/$model/run.mos"
  docker run --rm --pull=never --network none --memory 4g --cpus 4 \
    --user "$(id -u):$(id -g)" -e HOME=/work \
    -v "$BUILDINGS":/libs/buildings:ro -v "$MODELICA":/libs/msl:ro -v "$HERE":/scripts:ro \
    -v "$WORK/$model":/work -w /work "$IMAGE" omc /work/run.mos > "$WORK/$model/omc.log" 2>&1
  grep -q 'The simulation finished successfully' "$WORK/$model/omc.log" \
    || { cat "$WORK/$model/omc.log" >&2; exit 1; }
  python3 -I "$HERE/extract_frames.py" "$WORK/$model/${model}_res.csv" \
    "$HERE/../valve_oracle.txt" "$HERE/$signal" "$@"
}
run CoolingCoilValveFrames yCooVal_frames.csv
run CoolingCoilValveReenableProbe yCooVal_reenable_probe.csv probe

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
OMC_VERSION=$(docker run --rm --pull=never --network none "$IMAGE" omc --version)
ARCHITECTURE=$(docker image inspect "$IMAGE" --format '{{.Architecture}}')
cat > "$HERE/provenance_frames.json" <<JSON
{
  "image": "$IMAGE",
  "omc": "$OMC_VERSION",
  "architecture": "$ARCHITECTURE",
  "buildings_commit": "$BUILDINGS_COMMIT",
  "buildings_tree": "$BUILDINGS_TREE",
  "msl_commit": "$MODELICA_COMMIT",
  "msl_tree": "$MODELICA_TREE",
  "model": "$MODEL",
  "model_source_sha256": "$(sha "$BUILDINGS/Buildings/Utilities/Plotters/Examples/BaseClasses/CoolingCoilValve.mo")",
  "oracle_sha256": "$(sha "$HERE/../valve_oracle.txt")",
  "simulate_template_sha256": "$(sha "$HERE/simulate_frames.mos")",
  "wrapper": "CoolingCoilValveFrames.mo",
  "wrapper_sha256": "$(sha "$HERE/CoolingCoilValveFrames.mo")",
  "signal": "yCooVal_frames.csv",
  "signal_sha256": "$(sha "$HERE/yCooVal_frames.csv")",
  "probe_wrapper": "CoolingCoilValveReenableProbe.mo",
  "probe_wrapper_sha256": "$(sha "$HERE/CoolingCoilValveReenableProbe.mo")",
  "probe_signal": "yCooVal_reenable_probe.csv",
  "probe_signal_sha256": "$(sha "$HERE/yCooVal_reenable_probe.csv")",
  "settings": "cvode, tolerance 1e-6, 0 to 393 s, output every 1 s, inputs held (constant segments)",
  "projection": "at each frame instant, the last row within 1e-6 s of it",
  "command": "$COMMAND",
  "rows": $(($(wc -l < "$HERE/yCooVal_frames.csv") - 1))
}
JSON
echo "wrote yCooVal_frames.csv, yCooVal_reenable_probe.csv and provenance_frames.json"
