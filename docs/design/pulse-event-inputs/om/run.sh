#!/bin/sh
# Rerun the issue #800 semantics probes (scratch quality, NOT conformance fixtures): each .mo here
# transcribes the vendored CDL equations it names, or uses Modelica built-ins only, and loads no
# library. Writes <model>_res.csv into OUT; `python3 -I fires.py` then prints results.txt.
#
#   sh run.sh OUT
#
# The image is the OpenModelica 1.25.1 digest every other OpenModelica case of the repository
# pins; pull it once by digest. One container at a time, no network, 4 CPUs, 4 GB.
set -eu
IMAGE='openmodelica/openmodelica@sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864'
[ "$#" -eq 1 ] || { echo 'usage: sh run.sh OUT' >&2; exit 2; }
HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$1"
OUT=$(CDPATH= cd -- "$1" && pwd)
run() {  # run FILE MODEL STOP
  cat > "$OUT/run.mos" <<MOS
loadFile("/models/$1"); getErrorString();
simulate($2, startTime=0, stopTime=$3, numberOfIntervals=$3, tolerance=1e-6, method="cvode", outputFormat="csv"); getErrorString();
MOS
  docker run --rm --pull=never --network none --memory 4g --cpus 4 --user "$(id -u):$(id -g)" \
    -e HOME=/work -v "$HERE":/models:ro -v "$OUT":/work -w /work "$IMAGE" omc /work/run.mos \
    > "$OUT/$2.log" 2>&1
  grep -q 'The simulation finished successfully' "$OUT/$2.log"
}
run PulseSemantics.mo PulseSemantics.Pilot 10
run SampleSemantics.mo SampleSemantics.Pilot 8
run InitPulse.mo InitPulse 2
run PulseReview.mo PulseReview.Pilot 10
