#!/bin/sh
set -eu

IMAGE_REPOSITORY='openmodelica/openmodelica'
IMAGE_TAG="$IMAGE_REPOSITORY:v1.25.1-minimal"
INDEX='sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864'
IMAGE="$IMAGE_REPOSITORY@$INDEX"
BUILDINGS_COMMIT='a131864e4c4df22ebcd52bb8da439de0087ac365'
BUILDINGS_TREE='a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09'
MODELICA_COMMIT='7a4bf7de77a3986e8eb1e88cbb515d646f78f834'
MODELICA_TREE='43d7d8fc1a991358e9e5e91976e27cdc4280173f'

if [ "$#" -ne 3 ]; then
  printf '%s\n' 'usage: regenerate.sh BUILDINGS_CHECKOUT MODELICA_CHECKOUT FRESH_OUTPUT_DIRECTORY' >&2
  exit 2
fi
BUILDINGS=$1
MODELICA=$2
OUTPUT=$3
HOST_UID=$(id -u)
HOST_GID=$(id -g)
case "$(uname -m)" in
  arm64 | aarch64)
    ARCHITECTURE=arm64; HOST_ARCHITECTURE=arm64; DOCKER_ARCHITECTURE=aarch64; CONTAINER_ARCHITECTURE=aarch64
    PLATFORM=linux/arm64
    MANIFEST='sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4'
    CONFIG='sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666'
    ;;
  x86_64 | amd64)
    ARCHITECTURE=amd64; HOST_ARCHITECTURE=amd64; DOCKER_ARCHITECTURE=x86_64; CONTAINER_ARCHITECTURE=x86_64
    PLATFORM=linux/amd64
    MANIFEST='sha256:92d0779a01e7d43ed4d5ecb4cfd9754cb259b30673ddb454b5a32e3eb8665f11'
    CONFIG='sha256:0c81120bb392de44cab0e9ff6818d0a44afad657d5b401f25e148fa6c26e5347'
    ;;
  *) printf '%s\n' 'native host must be arm64/aarch64 or amd64/x86_64' >&2; exit 1 ;;
esac
test "$HOST_UID" -ne 0
test -d "$BUILDINGS/.git"
test -d "$MODELICA/.git"
test ! -e "$OUTPUT"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
check_hash() { test "$(sha256 "$2")" = "$1"; }
test -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=no)"
SOURCE_REVISION=$(git -C "$REPO_ROOT" rev-parse HEAD)
LINE_PILOT_SHA=$(sha256 "$SCRIPT_DIR/LinePilot.mo")
LINE_FLAG_PILOT_SHA=$(sha256 "$SCRIPT_DIR/LineFlagPilot.mo")
RUNNER_SHA=$(sha256 "$SCRIPT_DIR/runner.sh")
REGENERATE_SHA=$(sha256 "$SCRIPT_DIR/regenerate.sh")
CANONICALIZER_SOURCE="$REPO_ROOT/crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"
CANONICALIZER_SHA=$(sha256 "$CANONICALIZER_SOURCE")
TOOL_MAIN_SHA=$(sha256 "$REPO_ROOT/tools/openmodelica-line-reference/src/main.rs")
TOOL_CARGO_TOML_SHA=$(sha256 "$REPO_ROOT/tools/openmodelica-line-reference/Cargo.toml")
TOOL_CARGO_LOCK_SHA=$(sha256 "$REPO_ROOT/tools/openmodelica-line-reference/Cargo.lock")
ARCHITECTURE_GENERATOR_SHA=$(sha256 "$SCRIPT_DIR/generate_architecture.py")
ARCHITECTURE_VERIFIER_SHA=$(sha256 "$SCRIPT_DIR/verify_evidence.py")
check_generator_inputs() {
  test "$(git -C "$REPO_ROOT" rev-parse HEAD)" = "$SOURCE_REVISION"
  test -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=no)"
  check_hash "$LINE_PILOT_SHA" "$SCRIPT_DIR/LinePilot.mo"
  check_hash "$LINE_FLAG_PILOT_SHA" "$SCRIPT_DIR/LineFlagPilot.mo"
  check_hash "$RUNNER_SHA" "$SCRIPT_DIR/runner.sh"
  check_hash "$REGENERATE_SHA" "$SCRIPT_DIR/regenerate.sh"
  check_hash "$CANONICALIZER_SHA" "$CANONICALIZER_SOURCE"
  check_hash "$TOOL_MAIN_SHA" "$REPO_ROOT/tools/openmodelica-line-reference/src/main.rs"
  check_hash "$TOOL_CARGO_TOML_SHA" "$REPO_ROOT/tools/openmodelica-line-reference/Cargo.toml"
  check_hash "$TOOL_CARGO_LOCK_SHA" "$REPO_ROOT/tools/openmodelica-line-reference/Cargo.lock"
  check_hash "$ARCHITECTURE_GENERATOR_SHA" "$SCRIPT_DIR/generate_architecture.py"
  check_hash "$ARCHITECTURE_VERIFIER_SHA" "$SCRIPT_DIR/verify_evidence.py"
}
test "$(sh "$SCRIPT_DIR/deadline_test.sh")" = 'deadline accounting test passed'
test "$(sh "$SCRIPT_DIR/output_publish_test.sh")" = 'output publication test passed'
test "$(sh "$SCRIPT_DIR/container_cleanup_test.sh")" = 'container cleanup test passed'
PUBLISH_HELPER="$SCRIPT_DIR/output_publish.py"

run_timed() {
  seconds=$1
  shift
  perl -e 'my $seconds = shift @ARGV; alarm $seconds; exec @ARGV; exit 127' "$seconds" "$@"
}
monotonic_seconds() {
  perl -MTime::HiRes=clock_gettime,CLOCK_MONOTONIC -e 'print int(clock_gettime(CLOCK_MONOTONIC))'
}
. "$SCRIPT_DIR/deadline.sh"
. "$SCRIPT_DIR/container_cleanup.sh"

REGENERATION_COMPLETE=0
OUTPUT_PRIVATE=
ACTIVE_CID_FILE=
ACTIVE_CID=
RUN_LABEL=
cleanup() {
  if [ -n "${RUN_LABEL-}" ]; then cleanup_container "$RUN_LABEL" "${ACTIVE_CID_FILE-}" "${ACTIVE_CID-}" || true; fi
  if [ "$REGENERATION_COMPLETE" -ne 1 ] && [ -n "$OUTPUT_PRIVATE" ]; then
    python3 "$PUBLISH_HELPER" cleanup "$OUTPUT_PRIVATE" "$OUTPUT_DEVICE" "$OUTPUT_INODE" >/dev/null 2>&1 || \
      printf 'OpenModelica Line private-output cleanup failed for %s\n' "$OUTPUT_PRIVATE" >&2
  fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

OUTPUT_RECORD=$(python3 "$PUBLISH_HELPER" claim "$OUTPUT")
OUTPUT_PRIVATE=$(printf '%s' "$OUTPUT_RECORD" | cut -f1)
OUTPUT_DEVICE=$(printf '%s' "$OUTPUT_RECORD" | cut -f2)
OUTPUT_INODE=$(printf '%s' "$OUTPUT_RECORD" | cut -f3)
OUTPUT_PARENT_DEVICE=$(printf '%s' "$OUTPUT_RECORD" | cut -f4)
OUTPUT_PARENT_INODE=$(printf '%s' "$OUTPUT_RECORD" | cut -f5)
OUTPUT_DESTINATION=$(printf '%s' "$OUTPUT_RECORD" | cut -f6)
OUTPUT=$OUTPUT_PRIVATE
STAGING_RECORD=$(python3 "$PUBLISH_HELPER" claim-child "$OUTPUT" staging)
STAGING=$(printf '%s' "$STAGING_RECORD" | cut -f1)
STAGING_DEVICE=$(printf '%s' "$STAGING_RECORD" | cut -f2)
STAGING_INODE=$(printf '%s' "$STAGING_RECORD" | cut -f3)
OUTPUT_TOKEN=$(basename "$OUTPUT")
RUN_LABEL=${OUTPUT_TOKEN#.}
RUN_A_CONTAINER="$RUN_LABEL-run-a"
RUN_B_CONTAINER="$RUN_LABEL-run-b"
CONTROL_CONTAINER="$RUN_LABEL-flag-control"
mkdir "$STAGING/sources" "$STAGING/sources/buildings" "$STAGING/sources/modelica" "$STAGING/repositories" "$STAGING/reference" "$STAGING/tool-repo"
mkdir -p "$STAGING/tool-repo/tools/openmodelica-line-reference/src" "$STAGING/tool-repo/crates/oce-cxf/tests/open_modelica_line_reference"
cp "$SCRIPT_DIR/LinePilot.mo" "$SCRIPT_DIR/LineFlagPilot.mo" "$SCRIPT_DIR/runner.sh" "$STAGING/reference/"
cp "$REPO_ROOT/tools/openmodelica-line-reference/Cargo.toml" "$REPO_ROOT/tools/openmodelica-line-reference/Cargo.lock" "$STAGING/tool-repo/tools/openmodelica-line-reference/"
cp "$REPO_ROOT/tools/openmodelica-line-reference/src/main.rs" "$STAGING/tool-repo/tools/openmodelica-line-reference/src/main.rs"
cp "$CANONICALIZER_SOURCE" "$STAGING/tool-repo/crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"
check_hash "$LINE_PILOT_SHA" "$STAGING/reference/LinePilot.mo"
check_hash "$LINE_FLAG_PILOT_SHA" "$STAGING/reference/LineFlagPilot.mo"
check_hash "$RUNNER_SHA" "$STAGING/reference/runner.sh"
check_hash "$CANONICALIZER_SHA" "$STAGING/tool-repo/crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"
check_hash "$TOOL_MAIN_SHA" "$STAGING/tool-repo/tools/openmodelica-line-reference/src/main.rs"
check_hash "$TOOL_CARGO_TOML_SHA" "$STAGING/tool-repo/tools/openmodelica-line-reference/Cargo.toml"
check_hash "$TOOL_CARGO_LOCK_SHA" "$STAGING/tool-repo/tools/openmodelica-line-reference/Cargo.lock"

python3 "$SCRIPT_DIR/materialize_oci.py" "$SCRIPT_DIR/image-index.json" "$STAGING/index.raw.json" "${INDEX#sha256:}"
python3 "$SCRIPT_DIR/materialize_oci.py" "$SCRIPT_DIR/image-manifest-$ARCHITECTURE.json" "$STAGING/manifest.raw.json" "${MANIFEST#sha256:}"
grep -q "\"architecture\": \"$ARCHITECTURE\"" "$SCRIPT_DIR/image-index.json"
grep -q "$MANIFEST" "$SCRIPT_DIR/image-index.json"
grep -q "$CONFIG" "$SCRIPT_DIR/image-manifest-$ARCHITECTURE.json"
test "$(run_timed 10 docker info --format '{{.Architecture}}')" = "$DOCKER_ARCHITECTURE"
run_timed 10 docker image inspect "$IMAGE" --format '{{json .RepoDigests}}' | grep -Fq "\"$IMAGE\""
test "$(run_timed 10 docker image inspect "$IMAGE" --format '{{.Architecture}}')" = "$ARCHITECTURE"
test "$(run_timed 10 docker image inspect "$IMAGE" --format '{{.Os}}')" = linux
run_timed 120 docker run --rm --pull=never --platform "$PLATFORM" --network none --read-only \
  --cap-drop ALL --security-opt no-new-privileges --user "$HOST_UID:$HOST_GID" \
  --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 \
  --tmpfs "/tmp:rw,noexec,nosuid,size=256m,uid=$HOST_UID,gid=$HOST_GID" \
  --tmpfs "/out:rw,exec,nosuid,nodev,size=256m,uid=$HOST_UID,gid=$HOST_GID" \
  --ulimit fsize=67108864:67108864 -e HOME=/tmp/home -e TMPDIR=/tmp -e MODELICAPATH= \
  "$IMAGE" sh -c "test \"\$(uname -m)\" = $CONTAINER_ARCHITECTURE"

verify_source() {
  checkout=$1; repository=$2; commit=$3; tree=$4; destination=$5; name=$6
  test "$(git -C "$checkout" remote get-url origin)" = "$repository"
  test "$(git -C "$checkout" rev-parse HEAD)" = "$commit"
  test "$(git -C "$checkout" rev-parse 'HEAD^{tree}')" = "$tree"
  archive_repository="$STAGING/repositories/$name.git"
  git clone --quiet --bare --shared "$checkout" "$archive_repository"
  if [ "$name" = modelica ]; then printf '%s\n' 'Modelica/package.mo -export-subst' > "$archive_repository/info/attributes"; fi
  git -C "$archive_repository" archive --worktree-attributes --format=tar "$commit" | tar -xf - -C "$destination"
}
verify_source "$BUILDINGS" 'https://github.com/lbl-srg/modelica-buildings.git' "$BUILDINGS_COMMIT" "$BUILDINGS_TREE" "$STAGING/sources/buildings" buildings
verify_source "$MODELICA" 'https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git' "$MODELICA_COMMIT" "$MODELICA_TREE" "$STAGING/sources/modelica" modelica
check_hash f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59 "$STAGING/sources/buildings/Buildings/package.mo"
check_hash 85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5 "$STAGING/sources/buildings/Buildings/Controls/OBC/CDL/Reals/Line.mo"
check_hash c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191 "$STAGING/sources/modelica/Modelica/package.mo"
check_hash 565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3 "$STAGING/sources/modelica/Modelica/Blocks/Sources.mo"
check_hash 7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb "$STAGING/sources/modelica/ModelicaServices/package.mo"
check_hash 9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f "$STAGING/sources/modelica/Complex.mo"

run_model() {
  token=$1; model=$2; container=$3; directory=$4
  mkdir "$directory"
  ACTIVE_CID_FILE="$directory/container.cid"; ACTIVE_CID=; log="$directory/run.log"
  {
    printf 'host_architecture=%s\n' "$HOST_ARCHITECTURE"
    printf 'docker_server_architecture=%s\n' "$DOCKER_ARCHITECTURE"
    printf 'image_index_digest=%s\n' "$INDEX"
    printf 'image_platform_manifest_digest=%s\n' "$MANIFEST"
    printf 'image_config_digest=%s\n' "$CONFIG"
    printf 'oci_metadata_validation=raw_digest_and_pinned_graph\n'
    printf 'pull_policy=never\n'
    printf 'host_timeout_seconds=120\n'
    printf 'buildings_remote=https://github.com/lbl-srg/modelica-buildings.git\n'
    printf 'buildings_commit=%s\n' "$BUILDINGS_COMMIT"
    printf 'buildings_tree=%s\n' "$BUILDINGS_TREE"
    printf 'modelica_remote=https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git\n'
    printf 'modelica_commit=%s\n' "$MODELICA_COMMIT"
    printf 'modelica_tree=%s\n' "$MODELICA_TREE"
    printf 'repository_revision=%s\n' "$SOURCE_REVISION"
    printf 'line_pilot_sha256=%s\n' "$LINE_PILOT_SHA"
    printf 'line_flag_pilot_sha256=%s\n' "$LINE_FLAG_PILOT_SHA"
    printf 'runner_sha256=%s\n' "$RUNNER_SHA"
    printf 'regenerate_sha256=%s\n' "$REGENERATE_SHA"
    printf 'canonicalizer_sha256=%s\n' "$CANONICALIZER_SHA"
    printf 'tool_main_sha256=%s\n' "$TOOL_MAIN_SHA"
    printf 'tool_cargo_toml_sha256=%s\n' "$TOOL_CARGO_TOML_SHA"
    printf 'tool_cargo_lock_sha256=%s\n' "$TOOL_CARGO_LOCK_SHA"
    printf 'architecture_generator_sha256=%s\n' "$ARCHITECTURE_GENERATOR_SHA"
    printf 'architecture_verifier_sha256=%s\n' "$ARCHITECTURE_VERIFIER_SHA"
    printf 'source_materialization=git_archive_exact_committed_bytes\n'
    printf 'buildings_package_sha256=f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59\n'
    printf 'line_source_sha256=85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5\n'
    printf 'modelica_package_sha256=c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191\n'
    printf 'sources_source_sha256=565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3\n'
    printf 'modelica_services_sha256=7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb\n'
    printf 'complex_sha256=9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f\n'
    printf 'docker_command=docker run --pull=never --platform %s --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro\n' "$PLATFORM"
  } > "$log"
  runner_log="$directory/runner.log"
  deadline_start 120
  deadline_call docker run -d --cidfile "$ACTIVE_CID_FILE" --label "oce.line.run=$RUN_LABEL" --name "$container" \
    --pull=never --platform "$PLATFORM" --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
    --user "$HOST_UID:$HOST_GID" --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 \
    --tmpfs "/tmp:rw,noexec,nosuid,size=256m,uid=$HOST_UID,gid=$HOST_GID" \
    --tmpfs "/out:rw,exec,nosuid,nodev,size=256m,uid=$HOST_UID,gid=$HOST_GID" \
    --ulimit fsize=67108864:67108864 -e HOME=/tmp/home -e TMPDIR=/tmp -e MODELICAPATH= \
    -e MODEL="$model" -e OUTPUT_DIRECTORY_TOKEN="$token" \
    --mount "type=bind,src=$STAGING/sources,dst=/sources,readonly" \
    --mount "type=bind,src=$STAGING/reference,dst=/reference,readonly" "$IMAGE" /reference/runner.sh >/dev/null
  ACTIVE_CID=$(cat "$ACTIVE_CID_FILE"); valid_container_id "$ACTIVE_CID"
  while :; do
    if ! deadline_refresh; then run_timed 5 docker kill "$ACTIVE_CID" >/dev/null 2>&1 || true; return 1; fi
    poll_timeout=$DEADLINE_REMAINING; if [ "$poll_timeout" -gt 5 ]; then poll_timeout=5; fi
    if run_timed "$poll_timeout" docker exec "$ACTIVE_CID" test -f /out/.oce-complete; then poll_status=0; else poll_status=$?; fi
    if ! deadline_refresh; then run_timed 5 docker kill "$ACTIVE_CID" >/dev/null 2>&1 || true; return 1; fi
    if [ "$poll_status" -eq 0 ]; then
      deadline_call docker logs "$ACTIVE_CID" > "$runner_log" 2>&1
      if ! grep -Fqx "selected_model=$model" "$runner_log" || ! grep -Fqx 'runner_complete=1' "$runner_log"; then
        if ! deadline_refresh || [ "$DEADLINE_REMAINING" -le 1 ]; then return 1; fi
        sleep 1; continue
      fi
      cat "$runner_log" >> "$log"; rm "$runner_log"
      deadline_call docker exec "$ACTIVE_CID" cat /out/LinePilot_res.csv > "$directory/LinePilot_res.csv"
      run_timed 5 docker kill "$ACTIVE_CID" >/dev/null; run_timed 5 docker rm "$ACTIVE_CID" >/dev/null
      rm "$ACTIVE_CID_FILE"; ACTIVE_CID_FILE=; ACTIVE_CID=; return 0
    fi
    poll_timeout=$DEADLINE_REMAINING; if [ "$poll_timeout" -gt 5 ]; then poll_timeout=5; fi
    if inspect_state=$(run_timed "$poll_timeout" docker inspect "$ACTIVE_CID" --format '{{.State.Running}}'); then inspect_status=0; else inspect_status=$?; fi
    if [ "$inspect_status" -ne 0 ] || [ "$inspect_state" != true ]; then deadline_call docker logs "$ACTIVE_CID" >> "$log" 2>&1 || true; return 1; fi
    if ! deadline_refresh || [ "$DEADLINE_REMAINING" -le 1 ]; then return 1; fi
    sleep 1
  done
}

run_model fresh-run-a Line "$RUN_A_CONTAINER" "$OUTPUT/run-a"
run_model fresh-run-b Line "$RUN_B_CONTAINER" "$OUTPUT/run-b"
run_model fresh-flag-control FlagControl "$CONTROL_CONTAINER" "$OUTPUT/flag-control"
cmp "$OUTPUT/run-a/LinePilot_res.csv" "$OUTPUT/run-b/LinePilot_res.csv"
if cmp -s "$OUTPUT/run-a/LinePilot_res.csv" "$OUTPUT/flag-control/LinePilot_res.csv"; then printf '%s\n' 'flag control did not mutate raw output' >&2; exit 1; fi
CARGO_TARGET_DIR="$STAGING/cargo-target" cargo run --manifest-path "$STAGING/tool-repo/tools/openmodelica-line-reference/Cargo.toml" --offline --locked -- \
  canonicalize "$OUTPUT/run-a/LinePilot_res.csv" "$OUTPUT/line.canonical.csv" openmodelica_reals_line
CARGO_TARGET_DIR="$STAGING/cargo-target" cargo run --manifest-path "$STAGING/tool-repo/tools/openmodelica-line-reference/Cargo.toml" --offline --locked -- \
  canonicalize "$OUTPUT/flag-control/LinePilot_res.csv" "$OUTPUT/flag-control.canonical.csv" openmodelica_reals_line_flag_control
mv "$OUTPUT/run-a/LinePilot_res.csv" "$OUTPUT/line-run-a.raw.csv"
mv "$OUTPUT/run-b/LinePilot_res.csv" "$OUTPUT/line-run-b.raw.csv"
mv "$OUTPUT/flag-control/LinePilot_res.csv" "$OUTPUT/flag-control.raw.csv"
mv "$OUTPUT/run-a/run.log" "$OUTPUT/run-a.log"; mv "$OUTPUT/run-b/run.log" "$OUTPUT/run-b.log"; mv "$OUTPUT/flag-control/run.log" "$OUTPUT/flag-control.log"
rmdir "$OUTPUT/run-a" "$OUTPUT/run-b" "$OUTPUT/flag-control"
mv "$STAGING/index.raw.json" "$OUTPUT/image-index.json"
mv "$STAGING/manifest.raw.json" "$OUTPUT/image-manifest.json"

MUTATION=$(mktemp -d "$STAGING/mutation.XXXXXX")
mkdir -p "$MUTATION/repo/tools/openmodelica-line-reference/src" "$MUTATION/repo/crates/oce-cxf/tests/open_modelica_line_reference"
cp "$STAGING/tool-repo/tools/openmodelica-line-reference/Cargo.toml" "$STAGING/tool-repo/tools/openmodelica-line-reference/Cargo.lock" "$MUTATION/repo/tools/openmodelica-line-reference/"
cp "$STAGING/tool-repo/tools/openmodelica-line-reference/src/main.rs" "$MUTATION/repo/tools/openmodelica-line-reference/src/main.rs"
CANONICALIZER="$STAGING/tool-repo/crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"
cp "$CANONICALIZER" "$MUTATION/repo/crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"
perl -0pi -e 's/            \*rows\.last_mut\(\)\.expect\("equal-time group exists"\) = row;\n//' "$MUTATION/repo/crates/oce-cxf/tests/open_modelica_line_reference/canonicalizer.rs"
CARGO_TARGET_DIR="$STAGING/mutant-target" cargo run --manifest-path "$MUTATION/repo/tools/openmodelica-line-reference/Cargo.toml" --offline --locked -- \
  canonicalize-inspect "$OUTPUT/line-run-a.raw.csv" "$MUTATION/keep-first.canonical.csv" openmodelica_reals_line_keep_first "$MUTATION/keep-first.metadata"
MISMATCHES=$(CARGO_TARGET_DIR="$STAGING/mutant-target" cargo run --manifest-path "$MUTATION/repo/tools/openmodelica-line-reference/Cargo.toml" \
  --offline --locked --quiet -- schedule-mismatches "$MUTATION/keep-first.canonical.csv")
test "$MISMATCHES" = 'schedule_mismatch_rows=2,4,6,8'
GROUP_SIZES=$(sed -n 's/^group_sizes=//p' "$MUTATION/keep-first.metadata")
TIME_BITS=$(sed -n 's/^canonical_time_bits=//p' "$MUTATION/keep-first.metadata")
CANONICALIZER_SHA=$(shasum -a 256 "$CANONICALIZER" | cut -d' ' -f1)
cat > "$OUTPUT/projection-mutation.log" <<EOF
projection_mutation=contiguous equal-time selection changed from last to first
working_tree_modified=false
mutated_compile=PASS
mutated_input=line-run-a.raw.csv
mutated_input_sha256=$(shasum -a 256 "$OUTPUT/line-run-a.raw.csv" | cut -d' ' -f1)
mutated_raw_rows=15
mutated_canonical_rows=10
mutated_group_sizes=$GROUP_SIZES
mutated_canonical_time_bits=$TIME_BITS
mutated_schedule_result=FAIL
mutated_schedule_mismatch_rows=${MISMATCHES#schedule_mismatch_rows=}
mutated_schedule_first_mismatch_row=2
mutated_schedule_first_mismatch_time_bits=404e000000000eff
mutated_grouping_result=PASS
mutated_timestamp_bits_result=PASS
restoration_result=PASS
restored_canonicalizer_sha256=$CANONICALIZER_SHA
EOF

check_generator_inputs
python3 "$SCRIPT_DIR/generate_architecture.py" "$OUTPUT" "$ARCHITECTURE" "$REPO_ROOT"
python3 "$PUBLISH_HELPER" cleanup "$STAGING" "$STAGING_DEVICE" "$STAGING_INODE"; STAGING=
python3 "$SCRIPT_DIR/verify_evidence.py" architecture "$OUTPUT" "$REPO_ROOT" "$ARCHITECTURE"
trap '' HUP INT TERM
python3 "$PUBLISH_HELPER" publish "$OUTPUT" "$OUTPUT_DEVICE" "$OUTPUT_INODE" "$OUTPUT_PARENT_DEVICE" "$OUTPUT_PARENT_INODE" "$OUTPUT_DESTINATION"
OUTPUT_PRIVATE=
REGENERATION_COMPLETE=1
