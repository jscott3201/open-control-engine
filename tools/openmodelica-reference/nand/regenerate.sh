#!/bin/sh
set -eu

IMAGE_REPOSITORY='openmodelica/openmodelica'
IMAGE_TAG="$IMAGE_REPOSITORY:v1.25.1-minimal"
INDEX='sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864'
MANIFEST='sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4'
CONFIG='sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666'
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
test "$HOST_UID" -ne 0
test -d "$BUILDINGS/.git"
test -d "$MODELICA/.git"
test ! -e "$OUTPUT"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)

run_timed() {
  seconds=$1
  shift
  perl -e 'my $seconds = shift @ARGV; alarm $seconds; exec @ARGV; exit 127' "$seconds" "$@"
}

monotonic_seconds() {
  perl -MTime::HiRes=clock_gettime,CLOCK_MONOTONIC -e 'print int(clock_gettime(CLOCK_MONOTONIC))'
}

REGENERATION_COMPLETE=0
STAGING=
RUN_A_CONTAINER=
RUN_B_CONTAINER=
CONTROL_CONTAINER=
cleanup() {
  if [ -n "$RUN_A_CONTAINER$RUN_B_CONTAINER$CONTROL_CONTAINER" ]; then
    run_timed 5 docker rm -f "$RUN_A_CONTAINER" "$RUN_B_CONTAINER" "$CONTROL_CONTAINER" >/dev/null 2>&1 || true
  fi
  if [ -n "$STAGING" ]; then
    rm -rf "$STAGING"
  fi
  if [ "$REGENERATION_COMPLETE" -ne 1 ] && [ -e "$OUTPUT" ]; then
    rm -rf "$OUTPUT"
  fi
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

if ! mkdir "$OUTPUT"; then
  REGENERATION_COMPLETE=1
  exit 1
fi
STAGING=$(mktemp -d "${TMPDIR:-/tmp}/oce-omc-nand.XXXXXX")
CONTAINER_PREFIX=$(basename "$STAGING")
RUN_A_CONTAINER="$CONTAINER_PREFIX-run-a"
RUN_B_CONTAINER="$CONTAINER_PREFIX-run-b"
CONTROL_CONTAINER="$CONTAINER_PREFIX-semantic-control"
mkdir "$STAGING/sources" "$STAGING/sources/buildings" "$STAGING/sources/modelica" "$STAGING/repositories"

OCI_DIR="$REPO_ROOT/crates/oce-conformance/tests/fixtures/open_modelica/logical_nand"
test "$(shasum -a 256 "$OCI_DIR/image-index.json" | cut -d' ' -f1)" = "${INDEX#sha256:}"
test "$(shasum -a 256 "$OCI_DIR/image-manifest.json" | cut -d' ' -f1)" = "${MANIFEST#sha256:}"
grep -q '"architecture": "arm64"' "$OCI_DIR/image-index.json"
grep -q '"os": "linux"' "$OCI_DIR/image-index.json"
grep -q "$MANIFEST" "$OCI_DIR/image-index.json"
grep -q "$CONFIG" "$OCI_DIR/image-manifest.json"

test "$(uname -m)" = arm64
test "$(run_timed 10 docker info --format '{{.Architecture}}')" = aarch64
test "$(run_timed 10 docker image inspect "$IMAGE_TAG" --format '{{.Descriptor.digest}}')" = "$INDEX"
test "$(run_timed 10 docker image inspect --platform linux/arm64 "$IMAGE_TAG" --format '{{.Architecture}}')" = arm64
test "$(run_timed 10 docker image inspect --platform linux/arm64 "$IMAGE_TAG" --format '{{.Os}}')" = linux
test "$(run_timed 10 docker image inspect --platform linux/arm64 "$IMAGE_TAG" --format '{{.Descriptor.digest}}')" = "$MANIFEST"
run_timed 120 docker run --rm --pull=never --platform linux/arm64 --network none --read-only \
  --cap-drop ALL --security-opt no-new-privileges \
  --user "$HOST_UID:$HOST_GID" --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 \
  --tmpfs "/tmp:rw,noexec,nosuid,size=256m,uid=$HOST_UID,gid=$HOST_GID" \
  --tmpfs "/out:rw,exec,nosuid,nodev,size=256m,uid=$HOST_UID,gid=$HOST_GID" \
  --ulimit fsize=67108864:67108864 \
  -e HOME=/tmp/home -e TMPDIR=/tmp -e MODELICAPATH= \
  "$IMAGE" sh -c 'test "$(uname -m)" = aarch64'

verify_source() {
  checkout=$1
  repository=$2
  commit=$3
  tree=$4
  destination=$5
  name=$6
  test "$(git -C "$checkout" remote get-url origin)" = "$repository"
  test "$(git -C "$checkout" rev-parse HEAD)" = "$commit"
  test "$(git -C "$checkout" rev-parse 'HEAD^{tree}')" = "$tree"
  archive_repository="$STAGING/repositories/$name.git"
  git clone --quiet --bare --shared "$checkout" "$archive_repository"
  if [ "$name" = modelica ]; then
    # The pinned MSL declares export-subst for package.mo. Disable that archive rewrite in this
    # isolated repository so the mounted file remains the exact committed, digest-pinned byte stream.
    printf '%s\n' 'Modelica/package.mo -export-subst' > "$archive_repository/info/attributes"
  fi
  git -C "$archive_repository" archive --worktree-attributes --format=tar "$commit" | tar -xf - -C "$destination"
}

verify_source "$BUILDINGS" 'https://github.com/lbl-srg/modelica-buildings.git' "$BUILDINGS_COMMIT" "$BUILDINGS_TREE" "$STAGING/sources/buildings" buildings
verify_source "$MODELICA" 'https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git' "$MODELICA_COMMIT" "$MODELICA_TREE" "$STAGING/sources/modelica" modelica

check_hash() {
  expected=$1
  file=$2
  test "$(shasum -a 256 "$file" | cut -d' ' -f1)" = "$expected"
}
check_hash f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59 "$STAGING/sources/buildings/Buildings/package.mo"
check_hash 6e420d89f0636059c9431d0966b8c6756e385bf6b02596e2582bac1ea6bf1ca1 "$STAGING/sources/buildings/Buildings/Controls/OBC/CDL/Logical/Nand.mo"
check_hash 5169e635aefc83a0f65f689af3e9af7385f57e7dc156cebf0dc8108d74ea0fde "$STAGING/sources/buildings/Buildings/Controls/OBC/CDL/Logical/And.mo"
check_hash c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191 "$STAGING/sources/modelica/Modelica/package.mo"
check_hash 565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3 "$STAGING/sources/modelica/Modelica/Blocks/Sources.mo"
check_hash 7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb "$STAGING/sources/modelica/ModelicaServices/package.mo"
check_hash 9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f "$STAGING/sources/modelica/Complex.mo"

run_model() {
  token=$1
  model=$2
  container=$3
  directory=$4
  raw=$5
  mkdir "$directory"
  log="$directory/run.log"
  {
    printf 'host_architecture=arm64\n'
    printf 'docker_server_architecture=aarch64\n'
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
    printf 'source_materialization=git_archive_exact_committed_bytes\n'
    printf 'buildings_package_sha256=f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59\n'
    printf 'nand_source_sha256=6e420d89f0636059c9431d0966b8c6756e385bf6b02596e2582bac1ea6bf1ca1\n'
    printf 'and_source_sha256=5169e635aefc83a0f65f689af3e9af7385f57e7dc156cebf0dc8108d74ea0fde\n'
    printf 'modelica_package_sha256=c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191\n'
    printf 'boolean_table_source_sha256=565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3\n'
    printf 'modelica_services_sha256=7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb\n'
    printf 'complex_sha256=9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f\n'
    printf '%s\n' 'docker_command=docker run --pull=never --platform linux/arm64 --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro'
  } > "$log"
  runner_log="$directory/runner.log"
  started=$(monotonic_seconds)
  remaining=120
  run_timed "$remaining" docker run -d --name "$container" \
    --pull=never --platform linux/arm64 --network none --read-only \
    --cap-drop ALL --security-opt no-new-privileges \
    --user "$HOST_UID:$HOST_GID" --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 \
    --tmpfs "/tmp:rw,noexec,nosuid,size=256m,uid=$HOST_UID,gid=$HOST_GID" \
    --tmpfs "/out:rw,exec,nosuid,nodev,size=256m,uid=$HOST_UID,gid=$HOST_GID" \
    --ulimit fsize=67108864:67108864 \
    -e HOME=/tmp/home -e TMPDIR=/tmp -e MODELICAPATH= -e MODEL="$model" -e OUTPUT_DIRECTORY_TOKEN="$token" \
    --mount "type=bind,src=$STAGING/sources,dst=/sources,readonly" \
    --mount "type=bind,src=$SCRIPT_DIR,dst=/reference,readonly" \
    "$IMAGE" /reference/runner.sh >/dev/null

  while :; do
    elapsed=$(($(monotonic_seconds) - started))
    remaining=$((120 - elapsed))
    if [ "$remaining" -le 0 ]; then
      run_timed 5 docker kill "$container" >/dev/null 2>&1 || true
      return 1
    fi
    poll_timeout=$remaining
    if [ "$poll_timeout" -gt 5 ]; then
      poll_timeout=5
    fi
    if run_timed "$poll_timeout" docker exec "$container" test -f /out/.oce-complete; then
      elapsed=$(($(monotonic_seconds) - started))
      remaining=$((120 - elapsed))
      test "$remaining" -gt 0
      run_timed "$remaining" docker logs "$container" > "$runner_log" 2>&1
      if ! grep -Fqx "selected_model=$model" "$runner_log" || ! grep -Fqx 'runner_complete=1' "$runner_log"; then
        sleep 1
        continue
      fi
      cat "$runner_log" >> "$log"
      rm "$runner_log"
      elapsed=$(($(monotonic_seconds) - started))
      remaining=$((120 - elapsed))
      test "$remaining" -gt 0
      run_timed "$remaining" docker exec "$container" cat "/out/$raw" > "$directory/$raw"
      elapsed=$(($(monotonic_seconds) - started))
      remaining=$((120 - elapsed))
      test "$remaining" -gt 0
      run_timed "$remaining" docker kill "$container" >/dev/null
      elapsed=$(($(monotonic_seconds) - started))
      remaining=$((120 - elapsed))
      test "$remaining" -gt 0
      run_timed "$remaining" docker rm "$container" >/dev/null
      test -f "$directory/$raw"
      return 0
    fi
    elapsed=$(($(monotonic_seconds) - started))
    remaining=$((120 - elapsed))
    if [ "$remaining" -le 0 ]; then
      run_timed 5 docker kill "$container" >/dev/null 2>&1 || true
      return 1
    fi
    poll_timeout=$remaining
    if [ "$poll_timeout" -gt 5 ]; then
      poll_timeout=5
    fi
    if [ "$(run_timed "$poll_timeout" docker inspect "$container" --format '{{.State.Running}}')" != true ]; then
      elapsed=$(($(monotonic_seconds) - started))
      remaining=$((120 - elapsed))
      if [ "$remaining" -gt 0 ]; then
        run_timed "$remaining" docker logs "$container" >> "$log" 2>&1 || true
      fi
      return 1
    fi
    sleep 1
  done
}

run_model fresh-run-a Nand "$RUN_A_CONTAINER" "$OUTPUT/run-a" NandPilot_res.csv
run_model fresh-run-b Nand "$RUN_B_CONTAINER" "$OUTPUT/run-b" NandPilot_res.csv
run_model fresh-semantic-control And "$CONTROL_CONTAINER" "$OUTPUT/semantic-control" NandPilot_res.csv
cmp "$OUTPUT/run-a/NandPilot_res.csv" "$OUTPUT/run-b/NandPilot_res.csv"
if cmp -s "$OUTPUT/run-a/NandPilot_res.csv" "$OUTPUT/semantic-control/NandPilot_res.csv"; then
  printf '%s\n' 'semantic control did not mutate raw output' >&2
  exit 1
fi

cargo run --manifest-path "$REPO_ROOT/tools/openmodelica-reference/Cargo.toml" --offline --locked -- \
  canonicalize "$OUTPUT/run-a/NandPilot_res.csv" "$OUTPUT/nand.canonical.csv" openmodelica_logical_nand
cargo run --manifest-path "$REPO_ROOT/tools/openmodelica-reference/Cargo.toml" --offline --locked -- \
  canonicalize "$OUTPUT/semantic-control/NandPilot_res.csv" "$OUTPUT/and.canonical.csv" openmodelica_logical_and
printf 'nand_raw_sha256='; shasum -a 256 "$OUTPUT/run-a/NandPilot_res.csv" | cut -d' ' -f1
printf 'and_raw_sha256='; shasum -a 256 "$OUTPUT/semantic-control/NandPilot_res.csv" | cut -d' ' -f1
REGENERATION_COMPLETE=1
