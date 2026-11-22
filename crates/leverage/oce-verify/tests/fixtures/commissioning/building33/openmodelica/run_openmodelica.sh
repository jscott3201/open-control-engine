#!/bin/sh
# Simulate upstream's Building 33 verification model in the pinned OpenModelica container and
# write its valve signal over the first day (issue #755).
#
#   docker pull "$IMAGE"                              # once, by digest, with network
#   sh run_openmodelica.sh <buildings checkout> <msl checkout>
#
# <buildings checkout> is lbl-srg/modelica-buildings at the commit the vendored tree is pinned
# to, <msl checkout> OpenModelica/OpenModelica-ModelicaStandardLibrary at the commit the
# repository's other OpenModelica evidence uses (tools/openmodelica-line-reference); both are
# checked by their git tree hash. A shallow fetch of the one commit is enough:
#
#   git init -q b && git -C b fetch -q --depth 1 \
#     https://github.com/lbl-srg/modelica-buildings.git a131864e4c4df22ebcd52bb8da439de0087ac365 \
#     && git -C b checkout -q FETCH_HEAD
#
# The container runs with no network, as the host user, with both libraries read-only. The model
# is upstream's ControlsVerification_CoolingCoilValve as it is at the pin, not the authored
# transcription: it reads the six trends through CombiTimeTable (linear segments), converts
# units with From_degF and a 0.01 gain, makes the fan status with GreaterThreshold(0.5) and runs
# CoolingCoilValve under cvode at tolerance 1e-6. The script writes yCooVal_day.csv and
# provenance.json beside itself; no test runs it, and the test reads those two files. Docker
# images and libraries are left as they are: remove the image yourself if you pulled it only for
# this.
set -eu

IMAGE='openmodelica/openmodelica@sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864'
BUILDINGS_COMMIT='a131864e4c4df22ebcd52bb8da439de0087ac365'
BUILDINGS_TREE='a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09'
MODELICA_COMMIT='7a4bf7de77a3986e8eb1e88cbb515d646f78f834'
MODELICA_TREE='43d7d8fc1a991358e9e5e91976e27cdc4280173f'
MODEL='Buildings.Utilities.Plotters.Examples.ControlsVerification_CoolingCoilValve'

if [ "$#" -ne 2 ]; then
  echo 'usage: sh run_openmodelica.sh BUILDINGS_CHECKOUT MSL_CHECKOUT' >&2
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
docker run --rm --pull=never --network none --user "$(id -u):$(id -g)" -e HOME=/work \
  -v "$BUILDINGS":/libs/buildings:ro -v "$MODELICA":/libs/msl:ro -v "$HERE":/scripts:ro \
  -v "$WORK":/work -w /work "$IMAGE" omc /scripts/simulate.mos > "$WORK/omc.log" 2>&1
grep -q 'The simulation finished successfully' "$WORK/omc.log" || { cat "$WORK/omc.log" >&2; exit 1; }
python3 -I "$HERE/extract_day.py" "$WORK/${MODEL}_res.csv" "$HERE/yCooVal_day.csv"

OMC_VERSION=$(docker run --rm --pull=never --network none "$IMAGE" omc --version)
ARCHITECTURE=$(docker image inspect "$IMAGE" --format '{{.Architecture}}')
cat > "$HERE/provenance.json" <<JSON
{
  "image": "$IMAGE",
  "omc": "$OMC_VERSION",
  "architecture": "$ARCHITECTURE",
  "buildings_commit": "$BUILDINGS_COMMIT",
  "buildings_tree": "$BUILDINGS_TREE",
  "msl_commit": "$MODELICA_COMMIT",
  "msl_tree": "$MODELICA_TREE",
  "model": "$MODEL",
  "settings": "cvode, tolerance 1e-6, 0 to 86400 s, output every 5 s",
  "rows": $(($(wc -l < "$HERE/yCooVal_day.csv") - 1))
}
JSON
echo "wrote yCooVal_day.csv and provenance.json"
