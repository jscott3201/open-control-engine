#!/usr/bin/env bash
# Regenerate the vendored modelica-json translations in a pinned container and compare them with
# the checked-in tree (issue #661).
#
#   bash scripts/vendored_cxf/regenerate.sh <empty work directory>
#
# It clones lbl-srg/modelica-buildings (a partial, sparse clone of the cone it needs) and
# lbl-srg/modelica-json at the two pins third_party/modelica-buildings-cdl/README.md records,
# installs modelica-json's dependencies and runs it inside the Node image below, pinned by
# digest, for every class the vendored cxf/ tree holds a translation of: the upstream classes the
# structural-oracle class manifest names, and the Building 33 sequence. It then compares the
# regenerated tree with third_party/modelica-buildings-cdl/cxf byte for byte and exits non-zero
# on any difference.
#
# Needs git, python3, Docker and network access to GitHub and the npm registry. Nothing from the
# clones runs on the host: npm and modelica-json run in the container, which mounts the Buildings
# clone read-only. No CI job runs this script; the translations are checked in.
#
# `npm install` resolves modelica-json's dependencies at install time (upstream ships no
# lockfile), so a later run can differ for reasons outside both pins; the README says what such a
# difference does and does not mean.

set -euo pipefail

BUILDINGS_COMMIT="a131864e4c4df22ebcd52bb8da439de0087ac365"
MODELICA_JSON_COMMIT="85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb"
# node:26.5.0-bookworm-slim, the Node version the README records for the cxf/ translations.
NODE_IMAGE="node@sha256:2d49d876e96237d76de412761cf05dbfe5aee325cc4406a4d41d5824c5bb8beb"
# Translated classes outside the structural-oracle class manifest.
EXTRA_CLASSES=(Buildings.Utilities.Plotters.Examples.BaseClasses.CoolingCoilValve)

if [ "$#" -ne 1 ]; then
  echo "usage: bash scripts/vendored_cxf/regenerate.sh <empty work directory>" >&2
  exit 2
fi
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$1"
work="$(cd "$1" && pwd)"
if [ -n "$(ls -A "$work")" ]; then
  echo "work directory is not empty: $work" >&2
  exit 2
fi

python3 -I - "$repo_root/crates/oce-cxf/tests/fixtures/g36/structural_oracle_manifest.json" \
  "${EXTRA_CLASSES[@]}" >"$work/classes.txt" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as source:
    manifest = json.load(source)
classes = {entry["upstream_class"] for entry in manifest.values() if entry["upstream_class"]}
classes.update(sys.argv[2:])
for name in sorted(classes):
    print(name)
PY

git clone --quiet --filter=blob:none --no-checkout \
  https://github.com/lbl-srg/modelica-buildings "$work/modelica-buildings"
git -C "$work/modelica-buildings" sparse-checkout set --no-cone \
  /Buildings/package.mo /Buildings/Controls/package.mo /Buildings/Controls/OBC/ \
  /Buildings/Utilities/package.mo /Buildings/Utilities/Plotters/
git -C "$work/modelica-buildings" checkout --quiet "$BUILDINGS_COMMIT"
git clone --quiet --filter=blob:none --no-checkout \
  https://github.com/lbl-srg/modelica-json "$work/modelica-json"
git -C "$work/modelica-json" checkout --quiet "$MODELICA_JSON_COMMIT"
mkdir "$work/out"

docker run --rm \
  -v "$work/modelica-json:/modelica-json" \
  -v "$work/modelica-buildings:/modelica-buildings:ro" \
  -v "$work/out:/out" \
  -v "$work/classes.txt:/classes.txt:ro" \
  -w /modelica-json -e MODELICAPATH=/modelica-buildings \
  "$NODE_IMAGE" sh -c '
    set -e
    node --version
    npm install --no-audit --no-fund --loglevel=error
    while read -r class; do
      file="/modelica-buildings/$(printf %s "$class" | tr . /).mo"
      node app.js -f "$file" -o cxf -m cdl -d /out -p -l error
    done < /classes.txt
  '

diff -r "$repo_root/third_party/modelica-buildings-cdl/cxf" "$work/out/cxf"
echo "regenerated $(find "$work/out/cxf" -type f | wc -l | tr -d ' ') translations; byte-identical"
