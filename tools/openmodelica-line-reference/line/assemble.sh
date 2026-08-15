#!/bin/sh
set -eu

if [ "$#" -ne 3 ]; then
  printf '%s\n' 'usage: assemble.sh ARM64_EVIDENCE AMD64_EVIDENCE FRESH_OUTPUT_DIRECTORY' >&2
  exit 2
fi
ARM=$1
AMD=$2
DESTINATION=$3
test ! -e "$DESTINATION"
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
PUBLISH_HELPER="$SCRIPT_DIR/output_publish.py"
cargo run --manifest-path "$REPO_ROOT/tools/openmodelica-line-reference/Cargo.toml" --offline --locked --quiet -- \
  verify-architecture-canonical "$ARM"
cargo run --manifest-path "$REPO_ROOT/tools/openmodelica-line-reference/Cargo.toml" --offline --locked --quiet -- \
  verify-architecture-canonical "$AMD"
python3 "$SCRIPT_DIR/verify_evidence.py" architecture "$ARM" "$REPO_ROOT" arm64
python3 "$SCRIPT_DIR/verify_evidence.py" architecture "$AMD" "$REPO_ROOT" amd64

OUTPUT_RECORD=$(python3 "$PUBLISH_HELPER" claim "$DESTINATION")
OUTPUT=$(printf '%s' "$OUTPUT_RECORD" | cut -f1)
OUTPUT_DEVICE=$(printf '%s' "$OUTPUT_RECORD" | cut -f2)
OUTPUT_INODE=$(printf '%s' "$OUTPUT_RECORD" | cut -f3)
OUTPUT_PARENT_DEVICE=$(printf '%s' "$OUTPUT_RECORD" | cut -f4)
OUTPUT_PARENT_INODE=$(printf '%s' "$OUTPUT_RECORD" | cut -f5)
OUTPUT_DESTINATION=$(printf '%s' "$OUTPUT_RECORD" | cut -f6)
ASSEMBLY_COMPLETE=0
cleanup() {
  if [ "$ASSEMBLY_COMPLETE" -ne 1 ] && [ -n "${OUTPUT-}" ]; then
    python3 "$PUBLISH_HELPER" cleanup "$OUTPUT" "$OUTPUT_DEVICE" "$OUTPUT_INODE" >/dev/null 2>&1 || \
      printf 'OpenModelica Line assembly cleanup failed for %s\n' "$OUTPUT" >&2
  fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir "$OUTPUT/arm64" "$OUTPUT/amd64"
FILES='architecture.json line.canonical.csv line-run-a.raw.csv line-run-b.raw.csv run-a.log run-b.log flag-control.canonical.csv flag-control.raw.csv flag-control.log projection-mutation.log image-index.json image-manifest.json'
for file in $FILES; do cp "$ARM/$file" "$OUTPUT/arm64/$file"; cp "$AMD/$file" "$OUTPUT/amd64/$file"; done
cmp "$OUTPUT/arm64/image-index.json" "$OUTPUT/amd64/image-index.json"
cp "$OUTPUT/arm64/image-index.json" "$OUTPUT/image-index.json"
cmp "$OUTPUT/arm64/line.canonical.csv" "$OUTPUT/amd64/line.canonical.csv"
CANONICAL_SHA=$(shasum -a 256 "$OUTPUT/arm64/line.canonical.csv" | cut -d' ' -f1)
cat > "$OUTPUT/cross-architecture.log" <<EOF
comparison=canonical bytes
arm64_sha256=$CANONICAL_SHA
amd64_sha256=$CANONICAL_SHA
result=PASS
EOF
python3 "$SCRIPT_DIR/generate_manifest.py" "$OUTPUT" "$REPO_ROOT"
python3 "$SCRIPT_DIR/verify_evidence.py" final "$OUTPUT" "$REPO_ROOT"
trap '' HUP INT TERM
python3 "$PUBLISH_HELPER" publish "$OUTPUT" "$OUTPUT_DEVICE" "$OUTPUT_INODE" "$OUTPUT_PARENT_DEVICE" "$OUTPUT_PARENT_INODE" "$OUTPUT_DESTINATION"
OUTPUT=
ASSEMBLY_COMPLETE=1
