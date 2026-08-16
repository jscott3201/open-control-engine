#!/bin/sh
set -eu

IMAGE_REPOSITORY='openmodelica/openmodelica'
INDEX='sha256:79ddca5f56265f2b5811140589eccd809f7522ec5c553ae631ef606eeb8f9864'
IMAGE="$IMAGE_REPOSITORY@$INDEX"
BUILDINGS_COMMIT='a131864e4c4df22ebcd52bb8da439de0087ac365'
BUILDINGS_TREE='a2f4b04c59bdaac9c3fb64a7cda8c532a5fcae09'
MODELICA_COMMIT='7a4bf7de77a3986e8eb1e88cbb515d646f78f834'
MODELICA_TREE='43d7d8fc1a991358e9e5e91976e27cdc4280173f'
MODELICA_PACKAGE_SHA='c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191'

if [ "$#" -ne 3 ]; then
  printf '%s\n' 'usage: regenerate.sh BUILDINGS_CHECKOUT MODELICA_CHECKOUT FRESH_OUTPUT_DIRECTORY' >&2
  exit 2
fi
BUILDINGS=$1; MODELICA=$2; OUTPUT=$3; HOST_UID=$(id -u); HOST_GID=$(id -g)
case "$(uname -m)" in
  arm64 | aarch64)
    ARCHITECTURE=arm64; HOST_ARCHITECTURE=arm64; DOCKER_ARCHITECTURE=aarch64; CONTAINER_ARCHITECTURE=aarch64; PLATFORM=linux/arm64
    MANIFEST='sha256:39e8b9658bb24910b4ffd7b88604a6d3a0bb9bcd08b7dfa7cd4e9910537729e4'; CONFIG='sha256:9ad21ddf90a42a39a4c3d784c84fdb6a097721d1a1b8ca2188739ac50de52666'
    ;;
  x86_64 | amd64)
    ARCHITECTURE=amd64; HOST_ARCHITECTURE=amd64; DOCKER_ARCHITECTURE=x86_64; CONTAINER_ARCHITECTURE=x86_64; PLATFORM=linux/amd64
    MANIFEST='sha256:92d0779a01e7d43ed4d5ecb4cfd9754cb259b30673ddb454b5a32e3eb8665f11'; CONFIG='sha256:0c81120bb392de44cab0e9ff6818d0a44afad657d5b401f25e148fa6c26e5347'
    ;;
  *) printf '%s\n' 'native host must be arm64/aarch64 or amd64/x86_64' >&2; exit 1 ;;
esac
test "$HOST_UID" -ne 0; test -d "$BUILDINGS/.git"; test -d "$MODELICA/.git"; test ! -e "$OUTPUT"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)
sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
check_hash() { test "$(sha256 "$2")" = "$1"; }
test -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=no)"
SOURCE_REVISION=$(git -C "$REPO_ROOT" rev-parse HEAD)
RUSTC_VERBOSE=$(rustc --version --verbose); CARGO_VERBOSE=$(cargo --version --verbose); PYTHON_VERSION=$(python3 --version)
RUSTC_RELEASE=$(printf '%s\n' "$RUSTC_VERBOSE" | sed -n 's/^release: //p')
RUSTC_COMMIT_HASH=$(printf '%s\n' "$RUSTC_VERBOSE" | sed -n 's/^commit-hash: //p')
RUSTC_COMMIT_DATE=$(printf '%s\n' "$RUSTC_VERBOSE" | sed -n 's/^commit-date: //p')
RUSTC_HOST=$(printf '%s\n' "$RUSTC_VERBOSE" | sed -n 's/^host: //p')
RUSTC_LLVM_VERSION=$(printf '%s\n' "$RUSTC_VERBOSE" | sed -n 's/^LLVM version: //p')
CARGO_RELEASE=$(printf '%s\n' "$CARGO_VERBOSE" | sed -n 's/^release: //p')
CARGO_COMMIT_HASH=$(printf '%s\n' "$CARGO_VERBOSE" | sed -n 's/^commit-hash: //p')
CARGO_COMMIT_DATE=$(printf '%s\n' "$CARGO_VERBOSE" | sed -n 's/^commit-date: //p')
CARGO_HOST=$(printf '%s\n' "$CARGO_VERBOSE" | sed -n 's/^host: //p')
test "$RUSTC_RELEASE" = 1.97.1; test "$RUSTC_COMMIT_HASH" = 8bab26f4f68e0e26f0bb7960be334d5b520ea452
test "$RUSTC_COMMIT_DATE" = 2026-07-14; test "$RUSTC_LLVM_VERSION" = 22.1.6
test "$CARGO_RELEASE" = 1.97.1; test "$CARGO_COMMIT_HASH" = c980f4866141969fab6254a680546a277789d6f0
test "$CARGO_COMMIT_DATE" = 2026-06-30; test "$PYTHON_VERSION" = 'Python 3.13.7'
case "$ARCHITECTURE" in
  arm64) test "$RUSTC_HOST" = aarch64-unknown-linux-gnu; test "$CARGO_HOST" = aarch64-unknown-linux-gnu ;;
  amd64) test "$RUSTC_HOST" = x86_64-unknown-linux-gnu; test "$CARGO_HOST" = x86_64-unknown-linux-gnu ;;
esac

input_hash() { sha256 "$REPO_ROOT/$2"; }
RELIEFS_PILOT_SHA=$(input_hash reliefs_pilot_sha256 tools/openmodelica-reliefs-reference/reliefs/ReliefsPilot.mo)
PARAMETER_PILOT_SHA=$(input_hash parameter_pilot_sha256 tools/openmodelica-reliefs-reference/reliefs/ReliefsParameterPilot.mo)
CLAMP_PILOT_SHA=$(input_hash clamp_pilot_sha256 tools/openmodelica-reliefs-reference/reliefs/ReliefsClampPilot.mo)
RUNNER_SHA=$(sha256 "$SCRIPT_DIR/runner.sh"); REGENERATE_SHA=$(sha256 "$SCRIPT_DIR/regenerate.sh")
CANONICALIZER_SOURCE="$REPO_ROOT/crates/oce-cxf/tests/open_modelica_reliefs_reference/canonicalizer.rs"
CANONICALIZER_SHA=$(sha256 "$CANONICALIZER_SOURCE")
TOOL_MAIN_SHA=$(sha256 "$REPO_ROOT/tools/openmodelica-reliefs-reference/src/main.rs")
TOOL_CARGO_TOML_SHA=$(sha256 "$REPO_ROOT/tools/openmodelica-reliefs-reference/Cargo.toml")
TOOL_CARGO_LOCK_SHA=$(sha256 "$REPO_ROOT/tools/openmodelica-reliefs-reference/Cargo.lock")
ARCHITECTURE_GENERATOR_SHA=$(sha256 "$SCRIPT_DIR/generate_architecture.py")
ARCHITECTURE_VERIFIER_SHA=$(sha256 "$SCRIPT_DIR/verify_evidence.py")
PROJECTION_VERIFIER_SHA=$(sha256 "$SCRIPT_DIR/projection_evidence.py")
SAFE_FILE_HELPER_SHA=$(sha256 "$SCRIPT_DIR/safe_files.py")
EVIDENCE_WORKFLOW_SHA=$(sha256 "$REPO_ROOT/.github/workflows/openmodelica-reliefs-evidence.yml")
OCI_MATERIALIZER_SHA=$(sha256 "$SCRIPT_DIR/materialize_oci.py")
DEADLINE_SHA=$(sha256 "$SCRIPT_DIR/deadline.sh"); DEADLINE_TEST_SHA=$(sha256 "$SCRIPT_DIR/deadline_test.sh")
CONTAINER_CLEANUP_SHA=$(sha256 "$SCRIPT_DIR/container_cleanup.sh"); CONTAINER_CLEANUP_TEST_SHA=$(sha256 "$SCRIPT_DIR/container_cleanup_test.sh")
OUTPUT_PUBLISH_SHA=$(sha256 "$SCRIPT_DIR/output_publish.py"); OUTPUT_PUBLISH_TEST_SHA=$(sha256 "$SCRIPT_DIR/output_publish_test.sh")
OCI_INDEX_SOURCE_SHA=$(sha256 "$SCRIPT_DIR/image-index.json")
ARM64_MANIFEST_SOURCE_SHA=$(sha256 "$SCRIPT_DIR/image-manifest-arm64.json")
AMD64_MANIFEST_SOURCE_SHA=$(sha256 "$SCRIPT_DIR/image-manifest-amd64.json")
check_generator_inputs() {
  test "$(git -C "$REPO_ROOT" rev-parse HEAD)" = "$SOURCE_REVISION"
  test -z "$(git -C "$REPO_ROOT" status --porcelain --untracked-files=no)"
  check_hash "$RELIEFS_PILOT_SHA" "$SCRIPT_DIR/ReliefsPilot.mo"; check_hash "$PARAMETER_PILOT_SHA" "$SCRIPT_DIR/ReliefsParameterPilot.mo"; check_hash "$CLAMP_PILOT_SHA" "$SCRIPT_DIR/ReliefsClampPilot.mo"
  check_hash "$RUNNER_SHA" "$SCRIPT_DIR/runner.sh"; check_hash "$REGENERATE_SHA" "$SCRIPT_DIR/regenerate.sh"; check_hash "$CANONICALIZER_SHA" "$CANONICALIZER_SOURCE"
  check_hash "$TOOL_MAIN_SHA" "$REPO_ROOT/tools/openmodelica-reliefs-reference/src/main.rs"; check_hash "$TOOL_CARGO_TOML_SHA" "$REPO_ROOT/tools/openmodelica-reliefs-reference/Cargo.toml"; check_hash "$TOOL_CARGO_LOCK_SHA" "$REPO_ROOT/tools/openmodelica-reliefs-reference/Cargo.lock"
  check_hash "$ARCHITECTURE_GENERATOR_SHA" "$SCRIPT_DIR/generate_architecture.py"; check_hash "$ARCHITECTURE_VERIFIER_SHA" "$SCRIPT_DIR/verify_evidence.py"; check_hash "$PROJECTION_VERIFIER_SHA" "$SCRIPT_DIR/projection_evidence.py"; check_hash "$SAFE_FILE_HELPER_SHA" "$SCRIPT_DIR/safe_files.py"
  check_hash "$EVIDENCE_WORKFLOW_SHA" "$REPO_ROOT/.github/workflows/openmodelica-reliefs-evidence.yml"; check_hash "$OCI_MATERIALIZER_SHA" "$SCRIPT_DIR/materialize_oci.py"
  check_hash "$DEADLINE_SHA" "$SCRIPT_DIR/deadline.sh"; check_hash "$DEADLINE_TEST_SHA" "$SCRIPT_DIR/deadline_test.sh"; check_hash "$CONTAINER_CLEANUP_SHA" "$SCRIPT_DIR/container_cleanup.sh"; check_hash "$CONTAINER_CLEANUP_TEST_SHA" "$SCRIPT_DIR/container_cleanup_test.sh"
  check_hash "$OUTPUT_PUBLISH_SHA" "$SCRIPT_DIR/output_publish.py"; check_hash "$OUTPUT_PUBLISH_TEST_SHA" "$SCRIPT_DIR/output_publish_test.sh"; check_hash "$OCI_INDEX_SOURCE_SHA" "$SCRIPT_DIR/image-index.json"; check_hash "$ARM64_MANIFEST_SOURCE_SHA" "$SCRIPT_DIR/image-manifest-arm64.json"; check_hash "$AMD64_MANIFEST_SOURCE_SHA" "$SCRIPT_DIR/image-manifest-amd64.json"
}
test "$(sh "$SCRIPT_DIR/deadline_test.sh")" = 'deadline accounting test passed'
test "$(sh "$SCRIPT_DIR/output_publish_test.sh")" = 'output publication test passed'
test "$(sh "$SCRIPT_DIR/container_cleanup_test.sh")" = 'container cleanup test passed'
PUBLISH_HELPER="$SCRIPT_DIR/output_publish.py"
run_timed() { seconds=$1; shift; perl -e 'my $seconds = shift @ARGV; alarm $seconds; exec @ARGV; exit 127' "$seconds" "$@"; }
monotonic_seconds() { perl -MTime::HiRes=clock_gettime,CLOCK_MONOTONIC -e 'print int(clock_gettime(CLOCK_MONOTONIC))'; }
. "$SCRIPT_DIR/deadline.sh"; . "$SCRIPT_DIR/container_cleanup.sh"

REGENERATION_COMPLETE=0; OUTPUT_PRIVATE=; ACTIVE_CID_FILE=; ACTIVE_CID=; RUN_LABEL=
cleanup() {
  if [ -n "${RUN_LABEL-}" ]; then cleanup_container "$RUN_LABEL" "${ACTIVE_CID_FILE-}" "${ACTIVE_CID-}" || true; fi
  if [ "$REGENERATION_COMPLETE" -ne 1 ] && [ -n "$OUTPUT_PRIVATE" ]; then
    python3 "$PUBLISH_HELPER" cleanup "$OUTPUT_PRIVATE" "$OUTPUT_DEVICE" "$OUTPUT_INODE" >/dev/null 2>&1 || printf 'OpenModelica Reliefs private-output cleanup failed for %s\n' "$OUTPUT_PRIVATE" >&2
  fi
}
trap cleanup EXIT; trap 'exit 129' HUP; trap 'exit 130' INT; trap 'exit 143' TERM
OUTPUT_RECORD=$(python3 "$PUBLISH_HELPER" claim "$OUTPUT")
OUTPUT_PRIVATE=$(printf '%s' "$OUTPUT_RECORD" | cut -f1); OUTPUT_DEVICE=$(printf '%s' "$OUTPUT_RECORD" | cut -f2); OUTPUT_INODE=$(printf '%s' "$OUTPUT_RECORD" | cut -f3)
OUTPUT_PARENT_DEVICE=$(printf '%s' "$OUTPUT_RECORD" | cut -f4); OUTPUT_PARENT_INODE=$(printf '%s' "$OUTPUT_RECORD" | cut -f5); OUTPUT_DESTINATION=$(printf '%s' "$OUTPUT_RECORD" | cut -f6); OUTPUT=$OUTPUT_PRIVATE
STAGING_RECORD=$(python3 "$PUBLISH_HELPER" claim-child "$OUTPUT" staging); STAGING=$(printf '%s' "$STAGING_RECORD" | cut -f1); STAGING_DEVICE=$(printf '%s' "$STAGING_RECORD" | cut -f2); STAGING_INODE=$(printf '%s' "$STAGING_RECORD" | cut -f3)
OUTPUT_TOKEN=$(basename "$OUTPUT"); RUN_LABEL=${OUTPUT_TOKEN#.}; RUN_A_CONTAINER="$RUN_LABEL-run-a"; RUN_B_CONTAINER="$RUN_LABEL-run-b"; PARAMETER_CONTAINER="$RUN_LABEL-parameter"; CLAMP_CONTAINER="$RUN_LABEL-clamp"
mkdir "$STAGING/sources" "$STAGING/sources/buildings" "$STAGING/sources/modelica" "$STAGING/repositories" "$STAGING/reference" "$STAGING/tool-repo"
mkdir -p "$STAGING/tool-repo/tools/openmodelica-reliefs-reference/src" "$STAGING/tool-repo/crates/oce-cxf/tests/open_modelica_reliefs_reference"
cp "$SCRIPT_DIR/ReliefsPilot.mo" "$SCRIPT_DIR/ReliefsParameterPilot.mo" "$SCRIPT_DIR/ReliefsClampPilot.mo" "$SCRIPT_DIR/runner.sh" "$STAGING/reference/"
cp "$REPO_ROOT/tools/openmodelica-reliefs-reference/Cargo.toml" "$REPO_ROOT/tools/openmodelica-reliefs-reference/Cargo.lock" "$STAGING/tool-repo/tools/openmodelica-reliefs-reference/"
cp "$REPO_ROOT/tools/openmodelica-reliefs-reference/src/main.rs" "$STAGING/tool-repo/tools/openmodelica-reliefs-reference/src/main.rs"
cp "$CANONICALIZER_SOURCE" "$STAGING/tool-repo/crates/oce-cxf/tests/open_modelica_reliefs_reference/canonicalizer.rs"

python3 "$SCRIPT_DIR/materialize_oci.py" "$SCRIPT_DIR/image-index.json" "$STAGING/index.raw.json" "${INDEX#sha256:}"
python3 "$SCRIPT_DIR/materialize_oci.py" "$SCRIPT_DIR/image-manifest-$ARCHITECTURE.json" "$STAGING/manifest.raw.json" "${MANIFEST#sha256:}"
grep -q "\"architecture\": \"$ARCHITECTURE\"" "$SCRIPT_DIR/image-index.json"; grep -q "$MANIFEST" "$SCRIPT_DIR/image-index.json"; grep -q "$CONFIG" "$SCRIPT_DIR/image-manifest-$ARCHITECTURE.json"
test "$(run_timed 10 docker info --format '{{.Architecture}}')" = "$DOCKER_ARCHITECTURE"
run_timed 10 docker image inspect "$IMAGE" --format '{{json .RepoDigests}}' | grep -Fq "\"$IMAGE\""
test "$(run_timed 10 docker image inspect "$IMAGE" --format '{{.Architecture}}')" = "$ARCHITECTURE"; test "$(run_timed 10 docker image inspect "$IMAGE" --format '{{.Os}}')" = linux
run_timed 120 docker run --rm --pull=never --platform "$PLATFORM" --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user "$HOST_UID:$HOST_GID" --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs "/tmp:rw,noexec,nosuid,size=256m,uid=$HOST_UID,gid=$HOST_GID" --tmpfs "/out:rw,exec,nosuid,nodev,size=256m,uid=$HOST_UID,gid=$HOST_GID" --ulimit fsize=67108864:67108864 -e HOME=/tmp/home -e TMPDIR=/tmp -e MODELICAPATH= "$IMAGE" sh -c "test \"\$(uname -m)\" = $CONTAINER_ARCHITECTURE"

verify_source() {
  checkout=$1; repository=$2; commit=$3; tree=$4; destination=$5; name=$6
  test "$(git -C "$checkout" remote get-url origin)" = "$repository"; test "$(git -C "$checkout" rev-parse HEAD)" = "$commit"; test "$(git -C "$checkout" rev-parse 'HEAD^{tree}')" = "$tree"
  archive_repository="$STAGING/repositories/$name.git"; git clone --quiet --bare --shared "$checkout" "$archive_repository"
  if [ "$name" = modelica ]; then printf '%s\n' 'Modelica/package.mo -export-subst' > "$archive_repository/info/attributes"; fi
  git -C "$archive_repository" archive --worktree-attributes --format=tar "$commit" | tar -xf - -C "$destination"
}
verify_source "$BUILDINGS" https://github.com/lbl-srg/modelica-buildings.git "$BUILDINGS_COMMIT" "$BUILDINGS_TREE" "$STAGING/sources/buildings" buildings
verify_source "$MODELICA" https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git "$MODELICA_COMMIT" "$MODELICA_TREE" "$STAGING/sources/modelica" modelica
SOURCE_LIST='Buildings/package.mo f830afa369f22734a96440fac58444f4b8db1133fd3b1e337a29d1e6e060ab59
Buildings/Controls/package.mo 17f0bba8aa51f7051fa43d5cac6dcef1f33ca8f811fd6a6474bd3ed1263f61cd
Buildings/Controls/OBC/package.mo a86253df85e5531235ccb81ece569eedac973d8c4eae52be912877e7bd0d321c
Buildings/Controls/OBC/ASHRAE/package.mo 88b99ba4667c09e5a23c5ac21c88fe18e39af67c22cc2efc6dbab26db09e8e6b
Buildings/Controls/OBC/ASHRAE/G36/package.mo ae1fe5bfca73fd59ad4253aaea5e8c927ce1e1824cdce9790db3a24a20853881
Buildings/Controls/OBC/ASHRAE/G36/AHUs/package.mo 266b09bcb8a3266467c6728ee7a5d9872cdf3dad405af91bac14a697320176a2
Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/package.mo de4908f31fb15838b54dc41473b82059201ace000c2615ded47a1071dd718560
Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/package.mo 290c0e49356bc000364b644cac4baf353fc4d4a4ed5c77cb5e1145cdf3ab56e7
Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/package.mo 41ee31e3ed5ec6fd88a46447b73a5d5c55cd3cce06a899c25df3aadcba5b3b3b
Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/package.mo adebe030dcdd18a8777558b18e56084ed19546c375f678d083987a4480952216
Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/package.mo 0e2f3d3129ed06fc93655e75fda3597bba6f17f924117bb4b47c5dca7f3c3508
Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo 177fd5f2802bfd29072bc221756dd8846cd05b552f8fdf368a2c87a56593cb41
Buildings/Controls/OBC/CDL/package.mo 3ceda191a859e2513c4d3df322bec753ed8df406968cf4354c5488f4dcd79256
Buildings/Controls/OBC/CDL/Interfaces/package.mo a4b3a6831deb68e8209435e2b0f0067d227e3bff2be76845bd2f3690f13c82e4
Buildings/Controls/OBC/CDL/Interfaces/RealInput.mo 0f4afeda8d50035b722a79e6d6b48c86034facd3adcfc7f95e2b15cbd1ddc87a
Buildings/Controls/OBC/CDL/Interfaces/RealOutput.mo ba27a80bc46bf8b9550655b54a93679f5322b33786cc220daa59f7d39243d98f
Buildings/Controls/OBC/CDL/Reals/package.mo 3b9a58569701c9f7d44347d6304aeb60cea28902332fb16acc15e0fd61e19a8a
Buildings/Controls/OBC/CDL/Reals/Line.mo 85db4574432b236834a6fcec63b7713108eb67f90881494021cc25a7608ee7c5
Buildings/Controls/OBC/CDL/Reals/Min.mo e5dcf1e50d752365d05e44bc54eb743c116b87de240c1253c2126fcdbcbcbb04
Buildings/Controls/OBC/CDL/Reals/Max.mo 499e5162b21fa776c61065a46c4ba5d646ed887b227adaae93214d97750efca1
Buildings/Controls/OBC/CDL/Reals/Sources/package.mo 373e79eb61b6ace1527a93253ad0a3dfeb520dab0a1b6644dd4b0dd7419c9b20
Buildings/Controls/OBC/CDL/Reals/Sources/Constant.mo f3a131c5c6eb372ea48dec67ed5eb075eef1a485901143a338c4361511eed05e'
printf '%s\n' "$SOURCE_LIST" | while read -r path digest; do
  test "$(git -C "$BUILDINGS" show "$BUILDINGS_COMMIT:$path" | shasum -a 256 | cut -d' ' -f1)" = "$digest"; check_hash "$digest" "$STAGING/sources/buildings/$path"
done
MODELICA_LIST='Complex.mo 9bc7d4b185ddb7b01d966e2d6cc1c8eb06613cb95aedb9b71383a38c9b4e1f0f
Modelica/package.mo c3a060fc29842aaf3b7a565b93dbe80fe29d6a769848e3b077f5101117a65191
Modelica/Blocks/Sources.mo 565331012685bd195bc84712b6af3e3e911d5f59669360ab1a46990f90046aa3
ModelicaServices/package.mo 7eaa5e818964c81e587693a4228f98698426d3ed04bee57a9e44119164de1bbb'
printf '%s\n' "$MODELICA_LIST" | while read -r path digest; do
  test "$(git -C "$MODELICA" show "$MODELICA_COMMIT:$path" | shasum -a 256 | cut -d' ' -f1)" = "$digest"; check_hash "$digest" "$STAGING/sources/modelica/$path"
done
SOURCE_FILES_JSON=$(SOURCE_LIST="$SOURCE_LIST" MODELICA_LIST="$MODELICA_LIST" python3 -c 'import json,os; rows=[]
for source,body in (("buildings",os.environ["SOURCE_LIST"]),("modelica",os.environ["MODELICA_LIST"])):
  for line in body.splitlines():
    path,digest=line.split(); rows.append({"source":source,"path":path,"committed_sha256":digest,"materialized_sha256":digest})
print(json.dumps(rows,separators=(",",":")))')

run_model() {
  token=$1; model=$2; container=$3; directory=$4
  mkdir "$directory"; ACTIVE_CID_FILE="$directory/container.cid"; ACTIVE_CID=; log="$directory/run.log"
  {
    printf 'host_architecture=%s\n' "$HOST_ARCHITECTURE"; printf 'docker_server_architecture=%s\n' "$DOCKER_ARCHITECTURE"; printf 'image_index_digest=%s\n' "$INDEX"; printf 'image_platform_manifest_digest=%s\n' "$MANIFEST"; printf 'image_config_digest=%s\n' "$CONFIG"
    printf 'oci_metadata_validation=raw_digest_and_pinned_graph\n'; printf 'pull_policy=never\n'; printf 'host_timeout_seconds=120\n'; printf 'buildings_remote=https://github.com/lbl-srg/modelica-buildings.git\n'; printf 'buildings_commit=%s\n' "$BUILDINGS_COMMIT"; printf 'buildings_tree=%s\n' "$BUILDINGS_TREE"; printf 'modelica_remote=https://github.com/OpenModelica/OpenModelica-ModelicaStandardLibrary.git\n'; printf 'modelica_commit=%s\n' "$MODELICA_COMMIT"; printf 'modelica_tree=%s\n' "$MODELICA_TREE"
    printf 'repository_revision=%s\n' "$SOURCE_REVISION"; printf 'generator_provenance_scope=native_generation_and_publication\n'; printf 'rustc_release=%s\n' "$RUSTC_RELEASE"; printf 'rustc_commit_hash=%s\n' "$RUSTC_COMMIT_HASH"; printf 'rustc_commit_date=%s\n' "$RUSTC_COMMIT_DATE"; printf 'rustc_host=%s\n' "$RUSTC_HOST"; printf 'rustc_llvm_version=%s\n' "$RUSTC_LLVM_VERSION"; printf 'cargo_release=%s\n' "$CARGO_RELEASE"; printf 'cargo_commit_hash=%s\n' "$CARGO_COMMIT_HASH"; printf 'cargo_commit_date=%s\n' "$CARGO_COMMIT_DATE"; printf 'cargo_host=%s\n' "$CARGO_HOST"; printf 'python_version=%s\n' "$PYTHON_VERSION"
    printf 'reliefs_pilot_sha256=%s\n' "$RELIEFS_PILOT_SHA"; printf 'parameter_pilot_sha256=%s\n' "$PARAMETER_PILOT_SHA"; printf 'clamp_pilot_sha256=%s\n' "$CLAMP_PILOT_SHA"; printf 'runner_sha256=%s\n' "$RUNNER_SHA"; printf 'regenerate_sha256=%s\n' "$REGENERATE_SHA"; printf 'canonicalizer_sha256=%s\n' "$CANONICALIZER_SHA"; printf 'tool_main_sha256=%s\n' "$TOOL_MAIN_SHA"; printf 'tool_cargo_toml_sha256=%s\n' "$TOOL_CARGO_TOML_SHA"; printf 'tool_cargo_lock_sha256=%s\n' "$TOOL_CARGO_LOCK_SHA"; printf 'architecture_generator_sha256=%s\n' "$ARCHITECTURE_GENERATOR_SHA"; printf 'architecture_verifier_sha256=%s\n' "$ARCHITECTURE_VERIFIER_SHA"; printf 'projection_verifier_sha256=%s\n' "$PROJECTION_VERIFIER_SHA"; printf 'safe_file_helper_sha256=%s\n' "$SAFE_FILE_HELPER_SHA"; printf 'evidence_workflow_sha256=%s\n' "$EVIDENCE_WORKFLOW_SHA"; printf 'oci_materializer_sha256=%s\n' "$OCI_MATERIALIZER_SHA"; printf 'deadline_sha256=%s\n' "$DEADLINE_SHA"; printf 'deadline_test_sha256=%s\n' "$DEADLINE_TEST_SHA"; printf 'container_cleanup_sha256=%s\n' "$CONTAINER_CLEANUP_SHA"; printf 'container_cleanup_test_sha256=%s\n' "$CONTAINER_CLEANUP_TEST_SHA"; printf 'output_publish_sha256=%s\n' "$OUTPUT_PUBLISH_SHA"; printf 'output_publish_test_sha256=%s\n' "$OUTPUT_PUBLISH_TEST_SHA"; printf 'oci_index_source_sha256=%s\n' "$OCI_INDEX_SOURCE_SHA"; printf 'arm64_manifest_source_sha256=%s\n' "$ARM64_MANIFEST_SOURCE_SHA"; printf 'amd64_manifest_source_sha256=%s\n' "$AMD64_MANIFEST_SOURCE_SHA"
    printf 'source_materialization=git_archive_with_pinned_modelica_export_subst\n'; printf 'buildings_materialization=git_archive_without_local_attribute_override\n'; printf 'modelica_transform_path=Modelica/package.mo\n'; printf 'modelica_transform_rule=Modelica/package.mo -export-subst\n'; printf 'modelica_package_committed_sha256=%s\n' "$MODELICA_PACKAGE_SHA"; printf 'modelica_package_materialized_sha256=%s\n' "$MODELICA_PACKAGE_SHA"; printf 'source_files_json=%s\n' "$SOURCE_FILES_JSON"
    printf 'docker_command=docker run --pull=never --platform %s --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user <host-uid>:<host-gid> --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /out:rw,exec,nosuid,nodev,size=256m --ulimit fsize=67108864:67108864 --mount sources:ro --mount reference:ro\n' "$PLATFORM"
  } > "$log"
  runner_log="$directory/runner.log"; deadline_start 120
  deadline_call docker run -d --cidfile "$ACTIVE_CID_FILE" --label "oce.reliefs.run=$RUN_LABEL" --name "$container" --pull=never --platform "$PLATFORM" --network none --read-only --cap-drop ALL --security-opt no-new-privileges --user "$HOST_UID:$HOST_GID" --cpus 4 --memory 2g --memory-swap 2g --pids-limit 256 --tmpfs "/tmp:rw,noexec,nosuid,size=256m,uid=$HOST_UID,gid=$HOST_GID" --tmpfs "/out:rw,exec,nosuid,nodev,size=256m,uid=$HOST_UID,gid=$HOST_GID" --ulimit fsize=67108864:67108864 -e HOME=/tmp/home -e TMPDIR=/tmp -e MODELICAPATH= -e MODEL="$model" -e OUTPUT_DIRECTORY_TOKEN="$token" --mount "type=bind,src=$STAGING/sources,dst=/sources,readonly" --mount "type=bind,src=$STAGING/reference,dst=/reference,readonly" "$IMAGE" sh /reference/runner.sh >/dev/null
  ACTIVE_CID=$(cat "$ACTIVE_CID_FILE"); valid_container_id "$ACTIVE_CID"
  while :; do
    if ! deadline_refresh; then run_timed 5 docker kill "$ACTIVE_CID" >/dev/null 2>&1 || true; return 1; fi
    poll_timeout=$DEADLINE_REMAINING; if [ "$poll_timeout" -gt 5 ]; then poll_timeout=5; fi
    if run_timed "$poll_timeout" docker exec "$ACTIVE_CID" test -f /out/.oce-complete; then poll_status=0; else poll_status=$?; fi
    if [ "$poll_status" -eq 0 ]; then
      deadline_call docker logs "$ACTIVE_CID" > "$runner_log" 2>&1
      if ! grep -Fqx "selected_model=$model" "$runner_log" || ! grep -Fqx runner_complete=1 "$runner_log"; then sleep 1; continue; fi
      cat "$runner_log" >> "$log"; rm "$runner_log"; deadline_call docker exec "$ACTIVE_CID" cat /out/ReliefsPilot_res.csv > "$directory/ReliefsPilot_res.csv"
      run_timed 5 docker kill "$ACTIVE_CID" >/dev/null; run_timed 5 docker rm "$ACTIVE_CID" >/dev/null; rm "$ACTIVE_CID_FILE"; ACTIVE_CID_FILE=; ACTIVE_CID=; return 0
    fi
    if inspect_state=$(run_timed "$poll_timeout" docker inspect "$ACTIVE_CID" --format '{{.State.Running}}'); then inspect_status=0; else inspect_status=$?; fi
    if [ "$inspect_status" -ne 0 ] || [ "$inspect_state" != true ]; then deadline_call docker logs "$ACTIVE_CID" >> "$log" 2>&1 || true; return 1; fi
    sleep 1
  done
}

run_model fresh-run-a Reliefs "$RUN_A_CONTAINER" "$OUTPUT/run-a"
run_model fresh-run-b Reliefs "$RUN_B_CONTAINER" "$OUTPUT/run-b"
run_model fresh-parameter-control ParameterControl "$PARAMETER_CONTAINER" "$OUTPUT/parameter-control"
run_model fresh-final-clamp FinalClamp "$CLAMP_CONTAINER" "$OUTPUT/final-clamp"
cmp "$OUTPUT/run-a/ReliefsPilot_res.csv" "$OUTPUT/run-b/ReliefsPilot_res.csv"
if cmp -s "$OUTPUT/run-a/ReliefsPilot_res.csv" "$OUTPUT/parameter-control/ReliefsPilot_res.csv" || cmp -s "$OUTPUT/run-a/ReliefsPilot_res.csv" "$OUTPUT/final-clamp/ReliefsPilot_res.csv"; then printf '%s\n' 'Reliefs control did not mutate raw output' >&2; exit 1; fi
CARGO_TARGET_DIR="$STAGING/cargo-target" cargo run --manifest-path "$STAGING/tool-repo/tools/openmodelica-reliefs-reference/Cargo.toml" --offline --locked -- canonicalize "$OUTPUT/run-a/ReliefsPilot_res.csv" "$OUTPUT/reliefs.canonical.csv" openmodelica_g36_reliefs
CARGO_TARGET_DIR="$STAGING/cargo-target" cargo run --manifest-path "$STAGING/tool-repo/tools/openmodelica-reliefs-reference/Cargo.toml" --offline --locked -- canonicalize "$OUTPUT/parameter-control/ReliefsPilot_res.csv" "$OUTPUT/parameter-control.canonical.csv" openmodelica_g36_reliefs_parameter_control
CARGO_TARGET_DIR="$STAGING/cargo-target" cargo run --manifest-path "$STAGING/tool-repo/tools/openmodelica-reliefs-reference/Cargo.toml" --offline --locked -- canonicalize "$OUTPUT/final-clamp/ReliefsPilot_res.csv" "$OUTPUT/final-clamp.canonical.csv" openmodelica_g36_reliefs_final_clamp
mv "$OUTPUT/run-a/ReliefsPilot_res.csv" "$OUTPUT/reliefs-run-a.raw.csv"; mv "$OUTPUT/run-b/ReliefsPilot_res.csv" "$OUTPUT/reliefs-run-b.raw.csv"; mv "$OUTPUT/parameter-control/ReliefsPilot_res.csv" "$OUTPUT/parameter-control.raw.csv"; mv "$OUTPUT/final-clamp/ReliefsPilot_res.csv" "$OUTPUT/final-clamp.raw.csv"
mv "$OUTPUT/run-a/run.log" "$OUTPUT/run-a.log"; mv "$OUTPUT/run-b/run.log" "$OUTPUT/run-b.log"; mv "$OUTPUT/parameter-control/run.log" "$OUTPUT/parameter-control.log"; mv "$OUTPUT/final-clamp/run.log" "$OUTPUT/final-clamp.log"; rmdir "$OUTPUT/run-a" "$OUTPUT/run-b" "$OUTPUT/parameter-control" "$OUTPUT/final-clamp"
mv "$STAGING/index.raw.json" "$OUTPUT/image-index.json"; mv "$STAGING/manifest.raw.json" "$OUTPUT/image-manifest.json"
CARGO_TARGET_DIR="$STAGING/cargo-target" cargo run --manifest-path "$STAGING/tool-repo/tools/openmodelica-reliefs-reference/Cargo.toml" --offline --locked -- canonicalize-first-inspect "$OUTPUT/reliefs-run-a.raw.csv" "$OUTPUT/projection-keep-first.canonical.csv" openmodelica_g36_reliefs "$OUTPUT/projection-keep-first.metadata"
if cmp -s "$OUTPUT/reliefs.canonical.csv" "$OUTPUT/projection-keep-first.canonical.csv"; then printf '%s\n' 'explicit keep-first projection did not change canonical output' >&2; exit 1; fi
MUTATION_SHA=$(sha256 "$OUTPUT/projection-keep-first.canonical.csv"); METADATA_SHA=$(sha256 "$OUTPUT/projection-keep-first.metadata")
GROUP_SIZES=$(sed -n 's/^group_sizes=//p' "$OUTPUT/projection-keep-first.metadata"); RAW_TIME_BITS=$(sed -n 's/^raw_time_bits=//p' "$OUTPUT/projection-keep-first.metadata"); SELECTED_SOURCES=$(sed -n 's/^selected_source_rows=//p' "$OUTPUT/projection-keep-first.metadata"); SELECTED_TIMES=$(sed -n 's/^selected_time_bits=//p' "$OUTPUT/projection-keep-first.metadata")
cat > "$OUTPUT/projection-mutation.log" <<EOF
projection_mutation=contiguous equal-time selection changed from last to first
execution_path=canonicalize-first-inspect
mutated_input=reliefs-run-a.raw.csv
mutated_input_sha256=$(sha256 "$OUTPUT/reliefs-run-a.raw.csv")
mutated_output=projection-keep-first.canonical.csv
mutated_output_sha256=$MUTATION_SHA
mutated_metadata=projection-keep-first.metadata
mutated_metadata_sha256=$METADATA_SHA
mutated_raw_rows=21
mutated_grouped_rows=14
mutated_canonical_rows=7
mutated_group_sizes=$GROUP_SIZES
mutated_raw_time_bits=$RAW_TIME_BITS
mutated_selected_source_rows=$SELECTED_SOURCES
mutated_selected_time_bits=$SELECTED_TIMES
expected_selected_source_rows=0,3,6,9,12,15,18
expected_selected_time_bits=0000000000000000,404e000000000eff,405e000000000781,40668000000003c1,406e0000000003c1,4072c000000003c1,40768000000003c1
mutated_input_tuples_result=PASS
mutated_selected_source_rows_result=FAIL
mutated_selected_time_bits_result=FAIL
mutated_output_differs_from_keep_last=PASS
mutated_grouping_result=PASS
explicit_keep_first_execution=PASS
executed_canonicalizer_sha256=$CANONICALIZER_SHA
EOF
check_generator_inputs
python3 "$SCRIPT_DIR/generate_architecture.py" "$OUTPUT" "$ARCHITECTURE" "$REPO_ROOT"
python3 "$PUBLISH_HELPER" cleanup "$STAGING" "$STAGING_DEVICE" "$STAGING_INODE"; STAGING=
python3 "$SCRIPT_DIR/verify_evidence.py" architecture "$OUTPUT" "$REPO_ROOT" "$ARCHITECTURE"
check_generator_inputs; trap '' HUP INT TERM
python3 "$PUBLISH_HELPER" publish "$OUTPUT" "$OUTPUT_DEVICE" "$OUTPUT_INODE" "$OUTPUT_PARENT_DEVICE" "$OUTPUT_PARENT_INODE" "$OUTPUT_DESTINATION"
OUTPUT_PRIVATE=; REGENERATION_COMPLETE=1
