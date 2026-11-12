#!/usr/bin/env bash
# Static smoke checks for CI gate topology. This catches false-green workflow edits before GitHub
# Actions gets a chance to silently skip the heavy test suite or dependency gates.
#
# The live per-PR gate is .github/workflows/pr-gate.yml. .github/workflows/ci.yml is a dormant,
# byte-frozen copy kept as the history of the retained strict-bit capture archives, disabled in the
# repository's Actions settings, and is not checked here.
#
# Three gating workflows, each held to its own shape (issue #545 split the third from the second):
#   pr-gate.yml       per PR into `development`: the lane-filtered light gate.
#   merge-gate.yml    after each merge into `development`: exactly the Windows cell, the one job no
#                     pull request runs (issue #556 returned the determinism cells to pr-gate.yml).
#   release-gate.yml  nightly, on dispatch, on release PRs and on pushes to `main`: the heavy set,
#                     whose exact job list is pinned below so that none can be dropped.
# The one job the last two share, std-build-windows, must be byte-identical in both files; the
# determinism cells and their fan-in must run the same steps in the first and the third.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

WORKFLOW_DIR="${OCE_WORKFLOW_DIR:-.github/workflows}"
DENY_TOML="${OCE_DENY_TOML:-deny.toml}"
ROOT_CARGO_TOML="${OCE_ROOT_CARGO_TOML:-Cargo.toml}"
CRATES_DIR="${OCE_CRATES_DIR:-crates}"
FIRMWARE_DIR="${OCE_FIRMWARE_DIR:-firmware}"
GATE_SCRIPT="${OCE_GATE_SCRIPT:-.agents/gate.sh}"
NEXTEST_CONFIG="${OCE_NEXTEST_CONFIG:-.config/nextest.toml}"

require_file() {
  path="$1"
  if [ ! -s "$path" ]; then
    echo "FAIL: required gate file is missing or empty: $path"
    exit 1
  fi
}

require_pattern() {
  path="$1"
  pattern="$2"
  label="$3"
  searchable="$(grep -Ev '^[[:space:]]*#' "$path")"
  if ! grep -Eq -- "$pattern" <<< "$searchable"; then
    echo "FAIL: $path is missing required gate pattern: $label"
    exit 1
  fi
}

refuse_pattern() {
  path="$1"
  pattern="$2"
  label="$3"
  searchable="$(grep -Ev '^[[:space:]]*#' "$path")"
  if grep -Eq -- "$pattern" <<< "$searchable"; then
    echo "FAIL: $path contains a refused gate pattern: $label"
    exit 1
  fi
}

require_job_pattern() {
  path="$1"
  job="$2"
  pattern="$3"
  label="$4"
  # Comment lines never count: a required command that survives only in a comment is not run.
  job_body="$(awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found && !/^[[:space:]]*#/ { print }
  ' "$path")"
  if ! grep -Eq -- "$pattern" <<< "$job_body"; then
    echo "FAIL: $path job $job is missing required gate pattern: $label"
    exit 1
  fi
}

# Like require_job_literal, but also requires `first` on an earlier line of the job than `second`
# (a setup step that must run before the command that needs it).
require_job_order() {
  path="$1"
  job="$2"
  first="$3"
  second="$4"
  label="$5"
  job_body="$(awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found && !/^[[:space:]]*#/ { print }
  ' "$path")"
  first_line="$(grep -nF -- "$first" <<< "$job_body" | head -n 1 | cut -d: -f1 || true)"
  second_line="$(grep -nF -- "$second" <<< "$job_body" | head -n 1 | cut -d: -f1 || true)"
  if [ -z "$first_line" ] || [ -z "$second_line" ] || [ "$first_line" -ge "$second_line" ]; then
    echo "FAIL: $path job $job is missing required gate pattern: $label"
    exit 1
  fi
}

# Like require_job_pattern, but the needle is a fixed string (for long nextest filter expressions).
require_job_literal() {
  path="$1"
  job="$2"
  literal="$3"
  label="$4"
  job_body="$(awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found && !/^[[:space:]]*#/ { print }
  ' "$path")"
  if ! grep -Fq -- "$literal" <<< "$job_body"; then
    echo "FAIL: $path job $job is missing required gate pattern: $label"
    exit 1
  fi
}

# Like require_job_literal, but the needle must be a WHOLE line of the job: leading whitespace, a
# leading `- ` and a leading `run: ` aside, nothing may come before it or after it. A presence
# check that is not anchored at the end is satisfied by `<command> || true`, which is the command
# with its failure thrown away (issue #555).
require_job_line() {
  path="$1"
  job="$2"
  literal="$3"
  label="$4"
  job_body="$(awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found && !/^[[:space:]]*#/ {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      sub(/^- /, "", line)
      sub(/^run: /, "", line)
      print line
    }
  ' "$path")"
  if ! grep -Fxq -- "$literal" <<< "$job_body"; then
    echo "FAIL: $path job $job is missing required gate pattern: $label"
    exit 1
  fi
}

# Step-level `if:` (#418 review). A step condition can skip a gating command while its job still
# reports success, and CI OK sees only job results, so the lane filter is the one condition that may
# skip work. Two step-condition shapes remain, both of which run a step at least as often as its
# cell's identity says it must: a static matrix selector (`matrix.<key> == '<value>'`, optionally
# joined by `&&`), whose cell a later step proves did the selected work, and `${{ !cancelled() }}`,
# which only lets an evidence upload run after an earlier failure. Every other step `if:` fails.
# That a selector picks at least one cell of its job's matrix is require_live_matrix_selectors'
# rule (issue #566).
require_static_step_conditions() {
  path="$1"
  local bad
  bad="$(awk '
    /^jobs:[[:space:]]*$/ { in_jobs = 1; next }
    in_jobs && /^[^[:space:]#]/ { in_jobs = 0 }
    !in_jobs || /^[[:space:]]*#/ { next }
    /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { job = $0; sub(/^  /, "", job); sub(/:.*/, "", job); next }
    /^      [[:space:]]*(- )?if:/ {
      value = $0
      sub(/^[[:space:]]*(- )?if:[[:space:]]*/, "", value)
      sub(/[[:space:]]+$/, "", value)
      selector = "matrix\\.[a-z_]+ == \047[A-Za-z0-9_.-]+\047"
      if (value == "${{ !cancelled() }}") next
      if (value ~ ("^" selector "( && " selector ")*$")) next
      print job ": " value
    }
  ' "$path")"
  if [ -n "$bad" ]; then
    echo "FAIL: $path has a step-level if: other than a matrix selector or !cancelled():"
    printf '%s\n' "$bad" | sed 's/^/  /'
    exit 1
  fi
}

# No emulation anywhere in a gating workflow, comments included: the matrices run on native runners
# and QEMU is reserved for future bare-metal targets outside these files.
refuse_emulation() {
  path="$1"
  if grep -Eiq 'qemu|nextest-emulated|--target[[:space:]]+aarch64-unknown-linux-gnu' "$path"; then
    echo "FAIL: $path mentions emulation or a cross-compiled aarch64 target; cells must run natively"
    exit 1
  fi
}

# A native cell matrix: x86_64 on ubuntu-latest and aarch64 on ubuntu-24.04-arm, in debug and
# release codegen, without fail-fast, so one run reports every failing cell. Every matrix is all
# four cells since issue #556: the per-PR determinism matrix was debug only from issue #502 until
# then, and a `codegen: [debug]` there now fails like a dropped cell anywhere else.
require_native_cell_matrix() {
  path="$1"
  job="$2"
  local codegen_pattern codegen_label
  case "$3" in
    'debug, release')
      codegen_pattern='\[debug,[[:space:]]*release\]'
      codegen_label="$job runs debug and release codegen"
      ;;
    *)
      echo "BUG: require_native_cell_matrix: unknown codegen list '$3'"
      exit 1
      ;;
  esac
  require_job_pattern "$path" "$job" '^    runs-on:[[:space:]]*\$\{\{[[:space:]]*matrix\.runner[[:space:]]*\}\}[[:space:]]*$' \
    "$job runs on the matrix runner"
  require_job_pattern "$path" "$job" '^      fail-fast:[[:space:]]*false[[:space:]]*$' \
    "$job reports every failing cell (fail-fast: false)"
  require_job_pattern "$path" "$job" '^        runner:[[:space:]]*\[ubuntu-latest,[[:space:]]*ubuntu-24\.04-arm\][[:space:]]*$' \
    "$job runs native x86_64 and aarch64 runners"
  require_job_pattern "$path" "$job" "^        codegen:[[:space:]]*${codegen_pattern}[[:space:]]*\$" \
    "$codegen_label"
  local pairs
  pairs="$(awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found && /^          - runner:/ { runner = $3; next }
    found && /^            arch:/ { print runner "=" $2 }
  ' "$path" | sort | tr '\n' ' ')"
  if [ "$pairs" != "ubuntu-24.04-arm=aarch64 ubuntu-latest=x86_64 " ]; then
    echo "FAIL: $path job $job does not map x86_64 to ubuntu-latest and aarch64 to ubuntu-24.04-arm (got: $pairs)"
    exit 1
  fi
}

# The determinism cells, in pr-gate.yml (per PR, issue #556) and release-gate.yml (nightly). Every cell refuses a runner
# of the wrong architecture, records the blob round trip (issue #403), runs the debug subset with
# hard-fail-on-zero-tests, emits both state vectors, archives its round-trip suite for the fan-in
# and uploads under its own name.
require_determinism_cells() {
  path="$1"
  require_job_pattern "$path" 'determinism-matrix' 'test "\$\(uname -m\)" = "\$CELL_ARCH"$' \
    'determinism cells refuse a runner of the wrong architecture'
  require_job_pattern "$path" 'determinism-matrix' \
    'cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail$' \
    'debug determinism subset with hard-fail-on-zero-tests'
  require_job_pattern "$path" 'determinism-matrix' 'OCE_PORTABLE_STATE_OUT: target/portable-state-debug\.bin$' \
    'emit the debug portable state vector in every determinism cell'
  require_job_pattern "$path" 'determinism-matrix' 'OCE_TARGET_STATE_OUT: target/target-state-debug\.bin$' \
    'emit the debug target-bound state vector in every determinism cell'
  require_job_pattern "$path" 'determinism-matrix' 'tool:[[:space:]]*nextest@0\.9\.143' \
    'pinned cargo-nextest install for the determinism cells'
  require_job_pattern "$path" 'determinism-matrix' 'name: determinism-\$\{\{ matrix\.arch \}\}-\$\{\{ matrix\.codegen \}\}' \
    'upload each determinism cell under its own name'
  require_job_pattern "$path" 'determinism-matrix' 'if-no-files-found: error' \
    'a determinism cell with no vectors fails its upload'
  require_job_pattern "$path" 'determinism-matrix' 'target/nextest/\{ci,ci-release\}/junit\.xml' \
    'clear cached nextest JUnit reports'
  require_job_pattern "$path" 'determinism-matrix' 'target/\{portable,target\}-state-\{debug,release\}\.bin' \
    'clear cached state vectors before the determinism matrix'
  require_job_pattern "$path" 'determinism-matrix' \
    '^[[:space:]]*(- )?run: cargo nextest run -p oce-conformance --test blob_round_trip --locked --profile ci --no-tests=fail$' \
    'record the debug blob round trip in every determinism cell'
  require_job_pattern "$path" 'determinism-matrix' 'OCE_ROUND_TRIP_OUT: target/determinism/round-trip$' \
    'write the blob round-trip recording into the uploaded cell directory'
  require_job_line "$path" 'determinism-matrix' \
    'test "$(ls target/determinism/round-trip | wc -l | tr -d '"' '"')" = 92' \
    'refuse a determinism cell whose round-trip recording is not 46 blobs and 46 traces'
  require_job_line "$path" 'determinism-matrix' \
    'cargo nextest archive -p oce-conformance --test blob_round_trip --locked --archive-file target/determinism/blob-round-trip.tar.zst' \
    'archive each architecture'"'"'s debug blob round trip for the fan-in'
}

# The release-codegen determinism cells (pr-gate.yml and release-gate.yml): the subset and the blob round trip
# under `--cargo-profile release`, their state vectors, and the report and vector checks that follow
# the cell's codegen so a matrix edit cannot leave a release cell a green no-op.
require_release_codegen_determinism_cells() {
  path="$1"
  require_job_pattern "$path" 'determinism-matrix' \
    'cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci-release --cargo-profile release --no-tests=fail$' \
    'release determinism subset with hard-fail-on-zero-tests'
  require_job_pattern "$path" 'determinism-matrix' 'OCE_PORTABLE_STATE_OUT: target/portable-state-release\.bin$' \
    'emit the release portable state vector in every determinism cell'
  require_job_pattern "$path" 'determinism-matrix' 'OCE_TARGET_STATE_OUT: target/target-state-release\.bin$' \
    'emit the release target-bound state vector in every determinism cell'
  require_job_pattern "$path" 'determinism-matrix' \
    '^[[:space:]]*(- )?run: cargo nextest run -p oce-conformance --test blob_round_trip --locked --profile ci-release --cargo-profile release --no-tests=fail$' \
    'record the release blob round trip in every determinism cell'
  require_job_pattern "$path" 'determinism-matrix' 'test -s "target/nextest/\$profile/junit\.xml"$' \
    'refuse an absent or empty nextest determinism report'
  require_job_pattern "$path" 'determinism-matrix' 'test -s "target/portable-state-\$CELL_CODEGEN\.bin"$' \
    'refuse a determinism cell that emitted no portable state vector'
}

# The determinism fan-in's cross-architecture half, in either workflow: portable bytes equal,
# target-bound bytes different, compiled-schedule blobs equal and the goldens' 123329 bytes, the
# round-trip recordings equal, the x86_64 debug recording replayed through each architecture's
# archived suite, and the other architecture's target-bound snapshot refused.
require_determinism_fan_in() {
  path="$1"
  require_job_needs "$path" 'determinism-compare' 'determinism-matrix'
  require_job_pattern "$path" 'determinism-compare' \
    'cmp determinism-x86_64-debug/portable-state-debug\.bin determinism-aarch64-debug/portable-state-debug\.bin( &&)?$' \
    'compare portable state bytes across architectures'
  require_job_pattern "$path" 'determinism-compare' \
    '![[:space:]]*cmp -s determinism-x86_64-debug/target-state-debug\.bin determinism-aarch64-debug/target-state-debug\.bin( &&)?$' \
    'require target-bound state bytes to differ across architectures'
  require_job_pattern "$path" 'determinism-compare' \
    '^[[:space:]]*cmp determinism-x86_64-debug/blobs-debug\.bin determinism-aarch64-debug/blobs-debug\.bin$' \
    'compare compiled-schedule blob bytes across architectures'
  require_job_line "$path" 'determinism-compare' 'if [ "$size" != 123329 ]; then' \
    'refuse a compiled-schedule blob artifact that is not the 123329 bytes of the goldens'
  require_job_pattern "$path" 'determinism-compare' \
    'export OCE_FOREIGN_TARGET_STATE_IN=.*determinism-\$FOREIGN_ARCH-debug/target-state-debug\.bin' \
    'supply the other architecture target-bound snapshot to the restore test'
  require_job_pattern "$path" 'determinism-compare' 'test "\$CELL_ARCH" != "\$FOREIGN_ARCH"$' \
    'the restoring architecture differs from the snapshot architecture'
  require_job_line "$path" 'determinism-compare' \
    "-E 'test(=tests::state_portability_tests::foreign_matrix_target_snapshot_refuses_restore_when_supplied)'" \
    'restore and refuse the foreign target-bound snapshot'
  require_job_pattern "$path" 'determinism-compare' '--profile ci --no-tests=fail \\$' \
    'foreign target-bound restore hard-fails when its test is absent'
  require_job_pattern "$path" 'determinism-compare' 'tool:[[:space:]]*nextest@0\.9\.143' \
    'pinned cargo-nextest install for foreign restore'
  require_job_pattern "$path" 'determinism-compare' 'arch: x86_64' 'x86_64 restores the aarch64 snapshot'
  require_job_pattern "$path" 'determinism-compare' 'arch: aarch64' 'aarch64 restores the x86_64 snapshot'
  require_job_line "$path" 'determinism-compare' \
    'diff -rq determinism-x86_64-debug/round-trip determinism-aarch64-debug/round-trip' \
    'compare the determinism-x86_64-debug and determinism-aarch64-debug round-trip recordings'
  require_job_line "$path" 'determinism-compare' \
    'export OCE_ROUND_TRIP_EXPECT="$PWD/target/determinism/determinism-x86_64-debug/round-trip"' \
    'replay the x86_64 debug round-trip recording on each architecture'
  require_job_line "$path" 'determinism-compare' \
    'cargo nextest run --archive-file "target/determinism/determinism-$CELL_ARCH-debug/blob-round-trip.tar.zst" \' \
    'replay the recording through this architecture'"'"'s own archived suite'
  require_job_pattern "$path" 'determinism-compare' '^[[:space:]]*--workspace-remap \. --profile ci --no-tests=fail$' \
    'the round-trip replay hard-fails when its suite is absent'
}

# The determinism fan-in's cross-codegen half (pr-gate.yml and release-gate.yml): debug against
# release within each architecture, for the portable and target-bound state bytes, the
# compiled-schedule blobs and the round-trip recordings, over all four cells' artifacts.
require_cross_codegen_fan_in() {
  path="$1"
  require_job_pattern "$path" 'determinism-compare' \
    'cmp determinism-x86_64-debug/portable-state-debug\.bin determinism-x86_64-release/portable-state-release\.bin( &&)?$' \
    'compare x86_64 portable state bytes across codegen profiles'
  require_job_pattern "$path" 'determinism-compare' \
    'cmp determinism-x86_64-debug/target-state-debug\.bin determinism-x86_64-release/target-state-release\.bin( &&)?$' \
    'compare x86_64 target-bound state bytes across codegen profiles'
  require_job_pattern "$path" 'determinism-compare' \
    'cmp determinism-aarch64-debug/portable-state-debug\.bin determinism-aarch64-release/portable-state-release\.bin( &&)?$' \
    'compare aarch64 portable state bytes across codegen profiles'
  require_job_pattern "$path" 'determinism-compare' \
    'cmp determinism-aarch64-debug/target-state-debug\.bin determinism-aarch64-release/target-state-release\.bin( &&)?$' \
    'compare aarch64 target-bound state bytes across codegen profiles'
  for arch in x86_64 aarch64; do
    require_job_pattern "$path" 'determinism-compare' \
      "^[[:space:]]*cmp determinism-$arch-debug/blobs-debug\\.bin determinism-$arch-release/blobs-release\\.bin\$" \
      "compare $arch compiled-schedule blob bytes across codegen profiles"
  done
  require_job_line "$path" 'determinism-compare' \
    'test "$(ls determinism-*/blobs-*.bin | wc -l | tr -d '"' '"')" = 4' \
    'the fan-in holds the compiled-schedule blobs of all four cells'
  for pair in 'x86_64-debug determinism-x86_64-release' 'x86_64-debug determinism-aarch64-release'; do
    require_job_line "$path" 'determinism-compare' \
      "diff -rq determinism-${pair% *}/round-trip ${pair#* }/round-trip" \
      "compare the determinism-${pair% *} and ${pair#* } round-trip recordings"
  done
}

# The Windows platform cell (issue #394; out of pr-gate.yml since issue #502, in merge-gate.yml and
# release-gate.yml): the workspace build
# and the workspace suite without exactly the strict-bit binaries, on a Windows runner.
require_windows_cell() {
  path="$1"
  require_job_pattern "$path" 'std-build-windows' '^    runs-on:[[:space:]]*windows-latest[[:space:]]*$' \
    'the Windows cell runs on windows-latest'
  require_job_pattern "$path" 'std-build-windows' '^[[:space:]]*(- )?run: cargo build --workspace --locked$' \
    'the Windows cell builds the workspace'
  require_job_pattern "$path" 'std-build-windows' \
    '^[[:space:]]*cargo nextest run --workspace --locked --profile ci --no-tests=fail$' \
    'the Windows cell runs the workspace suite with hard-fail-on-zero-tests'
  require_job_line "$path" 'std-build-windows' \
    "-E 'not (binary_id(oce-conformance::strict_bits) | binary_id(oce-conformance::per_block_reals_transcendental) | binary_id(oce-conformance::per_block_reals_sources_transcendental) | binary_id(oce-conformance::per_block_psychrometrics) | binary_id(oce-conformance::per_block_utilities))'" \
    'the Windows cell excludes exactly the strict-bit binaries'
  require_job_pattern "$path" 'std-build-windows' '^[[:space:]]*(- )?run: test -s target/nextest/ci/junit\.xml$' \
    'the Windows cell refuses an absent or empty nextest report'
}

# The top-level events of a workflow's block-map `on:`, one per line, sorted.
workflow_events() {
  awk '
    /^on:[[:space:]]*$/ { in_on = 1; next }
    in_on && /^[^[:space:]#]/ { in_on = 0 }
    !in_on || /^[[:space:]]*#/ { next }
    /^  [A-Za-z_]+:/ { event = $0; sub(/^  /, "", event); sub(/:.*/, "", event); print event }
  ' "$1" | sort
}

# The events a workflow runs on, exactly (issue #545). The split of the release gate is a split by
# event: a `push` trigger gone from release-gate.yml would leave the merged release untested, a
# `pull_request` one gone would leave the release PR with no `CI OK` at all, a `schedule` one gone
# would end the nightly run, and a `pull_request` or `schedule` event added to merge-gate.yml would
# let its short run report where the heavy set must.
require_workflow_events() {
  path="$1"
  local expected actual
  expected="$(printf '%s\n' $2 | sort)"
  actual="$(workflow_events "$path")"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL: $path does not run on exactly the events: $2 (got: $(printf '%s' "$actual" | tr '\n' ' '))"
    exit 1
  fi
}

# The `branches:` filter of one event in a workflow's block-map `on:`, exactly. Which workflow runs
# on a push to which branch is the whole of the split (issue #545): merge-gate.yml takes
# `development`, every merged PR, and release-gate.yml takes `main`, the merged release; a release
# PR is a `pull_request` into `main`. A `development` push back in release-gate.yml would put the
# two-hour heavy set behind every merge again, and one missing from merge-gate.yml would leave a
# merge with no release-codegen determinism evidence until the night.
require_event_branches() {
  path="$1"
  event="$2"
  expected="$3"
  reason="$4"
  local branches
  branches="$(awk -v header="  $event:" '
    /^on:[[:space:]]*$/ { in_on = 1; next }
    in_on && /^[^[:space:]#]/ { in_on = 0 }
    !in_on || /^[[:space:]]*#/ { next }
    /^  [A-Za-z_]+:/ { line = $0; sub(/[[:space:]]+$/, "", line); in_event = (line == header); next }
    in_event && /^    branches:/ { sub(/^    branches:[[:space:]]*/, ""); sub(/[[:space:]]+$/, ""); print }
  ' "$path")"
  if [ "$branches" != "$expected" ]; then
    echo "FAIL: $path $event trigger does not name branches $expected (got: ${branches:-none}); $reason"
    exit 1
  fi
  # `branches:` is the event's only filter. A `paths:`, `paths-ignore:`, `branches-ignore:` or
  # `tags:` key beside it would let the event skip commits the branch list says it runs on.
  local filters
  filters="$(awk -v header="  $event:" '
    /^on:[[:space:]]*$/ { in_on = 1; next }
    in_on && /^[^[:space:]#]/ { in_on = 0 }
    !in_on || /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
    /^  [A-Za-z_]+:/ { line = $0; sub(/[[:space:]]+$/, "", line); in_event = (line == header); next }
    in_event && /^    [^[:space:]]/ { key = $0; sub(/^    /, "", key); sub(/:.*/, "", key); print key }
  ' "$path" | sort | tr '\n' ' ')"
  if [ "$filters" != 'branches ' ]; then
    echo "FAIL: $path $event trigger carries a filter other than branches: (got: $filters); $reason"
    exit 1
  fi
}

# The lines under `schedule:` in a workflow's block-map `on:`, exactly one cron entry with the
# given expression. The nightly run is where the heavy set executes (issue #545), and a cron that
# is present but never fires (`0 0 31 2 *`) would end it as surely as a deleted trigger, so the
# expression is pinned to the string, not to its shape.
require_schedule() {
  path="$1"
  expected="$2"
  local entries
  entries="$(awk '
    /^on:[[:space:]]*$/ { in_on = 1; next }
    in_on && /^[^[:space:]#]/ { in_on = 0 }
    !in_on || /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
    /^  [A-Za-z_]+:/ { in_schedule = ($0 ~ /^  schedule:[[:space:]]*$/); next }
    in_schedule { line = $0; sub(/^[[:space:]]+/, "", line); sub(/[[:space:]]+(#.*)?$/, "", line); print line }
  ' "$path")"
  if [ "$entries" != "- cron: \"$expected\"" ]; then
    echo "FAIL: $path schedule is not exactly the one cron entry \"$expected\" (got: ${entries:-none})"
    exit 1
  fi
}

# The `full` block of the gate script (issue #545). release-gate.yml reaches the workspace suite
# under release codegen only by executing the script, so the script's shape is part of the gate:
# after `if [ "$MODE" = full ]; then`, comment lines aside, come exactly the debug workspace step,
# the release-codegen workspace step under its own nextest profile, and the doctests, each a plain
# `step` at the block's own indent. A step that is dropped, prefixed with a condition
# (`[ -n "$X" ] || step ...`), wrapped in an inner `if`, or moved elsewhere changes those five
# lines. What this cannot see: an enclosing condition above the block, a redefined `step`, or an
# early exit before it; the executed job's report requirement and test-gate-modes.sh, which runs
# the script in `full` mode with stubbed children and reads which steps it reaches, cover those.
require_gate_full_block() {
  path="$1"
  local expected actual
  expected="$(cat <<'BLOCK'
  step 'nextest — workspace' \
    cargo nextest run --workspace --locked --profile ci --no-tests=fail
  step 'nextest — workspace (release codegen)' \
    cargo nextest run --workspace --locked --profile ci-release-workspace --cargo-profile release --no-tests=fail
  step 'doctests' cargo test --workspace --doc --locked
BLOCK
)"
  actual="$(awk '
    /^[[:space:]]*#/ { next }
    $0 == "if [ \"$MODE\" = full ]; then" { blocks++; taken = 0; inside = (blocks == 1); next }
    inside && taken < 5 { print; taken++ }
  ' "$path")"
  if [ "$(grep -c '^if \[ "\$MODE" = full \]; then$' "$path")" != 1 ] || [ "$expected" != "$actual" ]; then
    echo "FAIL: $path full block does not open with exactly the workspace suite, the release-codegen workspace suite under profile ci-release-workspace, and the doctests"
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") | sed 's/^/  /' || true
    exit 1
  fi
}

# How many non-comment lines of a file name a fixed string.
count_code_lines() {
  grep -Ev '^[[:space:]]*#' "$1" | grep -cF -- "$2" || true
}

# Every checkout in merge-gate.yml and release-gate.yml pins `ref: ${{ env.CHECKOUT_REF }}`: a job
# that took the default ref would test a different commit from the rest of its run on a scheduled
# run, and a fan-in would compare cells built from two trees.
require_pinned_checkout_ref() {
  path="$1"
  local unpinned
  unpinned="$(awk '
    function flush() { if (in_checkout && !pinned) print job; in_checkout = 0; pinned = 0 }
    /^[[:space:]]*#/ { next }
    /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { flush(); job = $0; sub(/^  /, "", job); sub(/:.*/, "", job); next }
    /^      - / { flush() }
    /^      - uses:[[:space:]]*actions\/checkout@/ { in_checkout = 1; next }
    in_checkout && /^          ref: \$\{\{ env\.CHECKOUT_REF \}\}[[:space:]]*$/ { pinned = 1 }
    END { flush() }
  ' "$path")"
  if [ -n "$unpinned" ]; then
    echo "FAIL: $path has a checkout that does not pin ref: \${{ env.CHECKOUT_REF }}:"
    printf '%s\n' "$unpinned" | sed 's/^/  /'
    exit 1
  fi
}

# Top-level job ids of a workflow, one per line, sorted.
workflow_jobs() {
  awk '
    /^jobs:[[:space:]]*$/ { in_jobs = 1; next }
    in_jobs && /^[^[:space:]#]/ { in_jobs = 0 }
    in_jobs && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ {
      job = $0
      sub(/^  /, "", job)
      sub(/:.*/, "", job)
      print job
    }
  ' "$1" | sort
}

# The `needs:` of one job (flow `[a, b]` or block `- a` form), one per line, sorted.
job_needs() {
  awk -v header="  $2:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    !found { next }
    in_list && /^      - / {
      value = $0
      sub(/^      - /, "", value)
      sub(/[[:space:]]*(#.*)?$/, "", value)
      print value
      next
    }
    in_list { in_list = 0 }
    /^    needs:[[:space:]]*$/ { in_list = 1; next }
    /^    needs:[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*(#.*)?$/ {
      value = $0
      sub(/^    needs:[[:space:]]*/, "", value)
      sub(/[[:space:]]*(#.*)?$/, "", value)
      print value
    }
    /^    needs:[[:space:]]*\[/ {
      value = $0
      sub(/^[^[]*\[/, "", value)
      sub(/\].*/, "", value)
      gsub(/[[:space:]]/, "", value)
      count = split(value, items, ",")
      for (i = 1; i <= count; i++) if (items[i] != "") print items[i]
    }
  ' "$1" | sort
}

# One job must need another, so a fan-in cannot start before (or without) its cells.
require_job_needs() {
  path="$1"
  job="$2"
  need="$3"
  if ! job_needs "$path" "$job" | grep -qx -- "$need"; then
    echo "FAIL: $path job $job does not need $need"
    exit 1
  fi
}

# Every job-level `if:` other than the aggregate job's (`ci-ok` unless named), as
# `job<TAB>condition` lines.
job_conditions() {
  awk -v aggregate="${2:-ci-ok}" '
    /^jobs:[[:space:]]*$/ { in_jobs = 1; next }
    in_jobs && /^[^[:space:]#]/ { in_jobs = 0 }
    in_jobs && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { job = $0; sub(/^  /, "", job); sub(/:.*/, "", job) }
    in_jobs && job != aggregate && /^    if:/ {
      value = $0
      sub(/^    if:[[:space:]]*/, "", value)
      sub(/[[:space:]]+$/, "", value)
      print job "\t" value
    }
  ' "$1"
}

# The aggregate job (`ci-ok` unless named) must need every other job: one missing from `needs:`
# would not be gated at all.
require_ci_ok_covers_every_job() {
  path="$1"
  local aggregate="${2:-ci-ok}" label="${3:-CI OK}" expected actual
  expected="$(workflow_jobs "$path" | grep -vx -- "$aggregate" || true)"
  actual="$(job_needs "$path" "$aggregate")"
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    echo "FAIL: $path $label needs does not match the workflow's jobs"
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") | sed 's/^/  /' || true
    exit 1
  fi
}

# No job other than the aggregate may carry a job-level `if:` (merge-gate.yml, release-gate.yml):
# the aggregate accepts only `success`, so a conditionally skipped gate would read as a failure.
# This is also what keeps the split by event out of the job graph: a heavy job cannot be made to
# run "only on schedule" inside one file, it has to live in the workflow that runs on that event.
require_no_job_conditions() {
  path="$1"
  local aggregate="${2:-ci-ok}" label="${3:-CI OK}" gated
  gated="$(job_conditions "$path" "$aggregate" | cut -f1)"
  if [ -n "$gated" ]; then
    echo "FAIL: $path gating job has a job-level if: that $label would read as failure: $gated"
    exit 1
  fi
}

# The exact job ids of a workflow (issue #545). release-gate.yml is the only place the heavy set
# runs, nightly and on the release PR, so a job deleted from it together with its `needs:` entry
# would pass every other check here and simply stop running. merge-gate.yml is held to the same
# rule in the other direction: a heavy job added to it would put the wait back behind every merge.
require_job_set() {
  path="$1"
  local expected actual
  expected="$(printf '%s\n' $2 | sort)"
  actual="$(workflow_jobs "$path")"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL: $path does not hold exactly its pinned job set"
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") | sed 's/^/  /' || true
    exit 1
  fi
}

# One job's whole definition, comments and blank lines included, with the trailing blank lines and
# the comment block that introduces the next job dropped.
job_definition() {
  awk -v header="  $2:" '
    $0 == header { found = 1 }
    found && $0 != header && /^  [^ #]/ { exit }
    found { lines[++count] = $0 }
    END {
      while (count > 0 && lines[count] ~ /^(  #.*)?[[:space:]]*$/) count--
      for (i = 1; i <= count; i++) print lines[i]
    }
  ' "$1"
}

# A job that two workflows both run must be the same job in both (issue #545): the Windows cell
# runs after each merge from merge-gate.yml and nightly and on release PRs from release-gate.yml.
# Byte equality, comments inside the job included, is the whole rule: the cell cannot be weakened
# in one file while the other keeps it.
require_identical_job() {
  first="$1"
  second="$2"
  job="$3"
  local one two
  one="$(job_definition "$first" "$job")"
  two="$(job_definition "$second" "$job")"
  if [ -z "$one" ] || [ "$one" != "$two" ]; then
    echo "FAIL: job $job is not defined identically in $first and $second"
    diff <(printf '%s\n' "$one") <(printf '%s\n' "$two") | head -n 20 | sed 's/^/  /' || true
    exit 1
  fi
}

# One job, the lines of it that do work: comment lines dropped, and the lines that legitimately
# differ between the per-PR and the release definition of a determinism job. Each exception
# belongs to ONE side (issue #555): `per-pr` drops the lane filter (`needs:` and the job-level
# `if:`), which only pr-gate.yml may carry, and `release` drops the pinned checkout ref, which only
# release-gate.yml may carry. A lane filter in the release job, or a `ref:` of any kind in the
# per-PR job, is therefore left in and fails the comparison. The rust-cache `save-if:` differs on
# both sides and is dropped from both; each side's value is pinned by a rule of its own below.
job_commands() {
  job_definition "$1" "$2" | awk -v side="$3" '
    /^[[:space:]]*#/ { next }
    side == "per-pr" && (/^    needs:/ || /^    if: needs\.changes\.outputs\./) { next }
    side == "release" && /^    needs: determinism-matrix[[:space:]]*$/ { next }
    side == "release" && /^          ref: \$\{\{ env\.CHECKOUT_REF \}\}[[:space:]]*$/ { next }
    /^          save-if:/ { next }
    { print }
  '
}

# The determinism cells and their fan-in run per PR from pr-gate.yml and nightly from
# release-gate.yml (issue #556). Every step, command, environment variable, matrix entry and
# step condition must be the same in both: a cell, a codegen mode or a comparison cannot be
# dropped or weakened per PR while the nightly definition keeps it, or the reverse. Comments may
# differ, and so may the lines job_commands sets aside, each on the one side that may carry it.
require_same_job_commands() {
  first="$1"
  second="$2"
  job="$3"
  local one two
  one="$(job_commands "$first" "$job" per-pr)"
  two="$(job_commands "$second" "$job" release)"
  if [ -z "$one" ] || [ "$one" != "$two" ]; then
    echo "FAIL: job $job does not run the same steps in $first and $second"
    diff <(printf '%s\n' "$one") <(printf '%s\n' "$two") | head -n 20 | sed 's/^/  /' || true
    exit 1
  fi
}

# The top-level `env:` of a workflow, comment lines dropped, one `NAME: value` per line.
workflow_env() {
  awk '
    /^env:[[:space:]]*$/ { inside = 1; next }
    inside && /^[^[:space:]#]/ { inside = 0 }
    inside && !/^[[:space:]]*(#.*)?$/ { line = $0; sub(/[[:space:]]+$/, "", line); print line }
  ' "$1"
}

# Two workflows that run the same jobs must give them the same workflow-level environment
# (issue #555): the job comparison above reads one job at a time, so `RUSTFLAGS`, a
# `CARGO_PROFILE_*` override or `CARGO_INCREMENTAL` set for a whole file changes what every step
# in it builds without changing a line the comparison sees. `CHECKOUT_REF` is the one variable
# that may differ; it is dropped from the files named in `$3` (each pins its own value) and left
# in for the others, so pr-gate.yml, which has no pinned ref, fails if it grows one.
require_same_workflow_env() {
  first="$1"
  second="$2"
  local one two
  one="$(workflow_env "$first")"
  two="$(workflow_env "$second")"
  case " $3 " in *" first "*) one="$(grep -v '^  CHECKOUT_REF:' <<< "$one" || true)" ;; esac
  case " $3 " in *" second "*) two="$(grep -v '^  CHECKOUT_REF:' <<< "$two" || true)" ;; esac
  if [ "$one" != "$two" ]; then
    echo "FAIL: $first and $second do not set the same workflow-level env"
    diff <(printf '%s\n' "$one") <(printf '%s\n' "$two") | head -n 20 | sed 's/^/  /' || true
    exit 1
  fi
}

# A tripwire for masked failures in the gating workflows (issue #555). It is NOT a proof that no
# step discards a failure: shell has more ways to do that than a line scan can list, and the
# known ones it does not catch are named at the end. It exists because the same edit made to both
# copies of a shared job keeps the two copies equal, and a whole-line presence rule only guards
# the lines it names. On every non-comment line it refuses:
#   * inside a `${{ ... }}` expression, a quoted literal that contains `|`, `&`, `;` or `+`, which
#     is how an operator or a `set` option could be spelled out of sight of the rules below (no
#     expression here has such a literal);
# and then, with the expressions removed (their own `||` and `&&` are the only ones today):
#   * `||`: `|| true`, `|| :`, `|| /bin/true`, `|| echo`, `|| exit 0`, `|| { true; }`,
#     `|| (true)`, `|| "true"`. No gating step has a use for a fallback command;
#   * a line that ends in `|\` or starts with `|`, which is the same operator split over a
#     continuation;
#   * a trailing `; true`, `; :` or `; exit 0`, and a trailing `&`;
#   * `set` with a `+` option among its arguments (`set +e`, `set +eu`, `set +o errexit`,
#     `set -o pipefail +e`, `set -x +e`), and `shopt -u`: nothing here turns a shell option off.
# And every `shell` key, in block or flow style and with or without quotes, must be the line
# `shell: bash`. GitHub runs that as `bash --noprofile --norc -eo pipefail {0}`; a custom template
# such as `bash {0}` drops `-e` and `pipefail` for every step under it.
# A bare `exit 0`, `cmd && true` or `cmd && :` before the last command of a step, `cmd & wait` and
# `(cmd &)` pass this line scan; refuse_masked_step_status reads each step's script whole and
# refuses them (issue #566). Anything inside a script the workflow calls passes both.
refuse_failure_masks() {
  path="$1"
  local lines code bad
  lines="$(grep -Ev '^[[:space:]]*#' "$path")"
  bad="$(grep -Eo '\$\{\{[^}]*\}\}' <<< "$lines" | grep -Eo "'[^']*'|\"[^\"]*\"" | grep -E '[|&;+]' || true)"
  if [ -n "$bad" ]; then
    echo "FAIL: $path contains a refused gate pattern: a masked failure (an expression literal that spells a shell operator or option)"
    printf '%s\n' "$bad" | head -n 5 | sed 's/^/  /'
    exit 1
  fi
  code="$(sed -E 's/\$\{\{[^}]*\}\}//g' <<< "$lines")"
  bad="$(grep -En '\|\||\|[[:space:]]*\\$|^[[:space:]]*\||;[[:space:]]*(true|:|exit[[:space:]]+0+)[[:space:]]*$|(^|[^&])&[[:space:]]*$|(^|[^[:alnum:]_-])set([[:space:]]+[^;&|#]*)?[[:space:]]\+|(^|[^[:alnum:]_-])shopt[[:space:]]+([^;&|#]*[[:space:]])?-[[:alnum:]]*u' <<< "$code" || true)"
  if [ -n "$bad" ]; then
    echo "FAIL: $path contains a refused gate pattern: a masked failure (||, a split |, a trailing ; true, ; exit 0 or &, set with a + option, or shopt -u), which lets a failing command read as green"
    printf '%s\n' "$bad" | head -n 5 | sed 's/^/  /'
    exit 1
  fi
  bad="$(grep -E "(^|[[:space:]{,\"'-])[\"']?shell[\"']?[[:space:]]*:" <<< "$code" | grep -Ev '^[[:space:]]*(- )?shell: bash[[:space:]]*$' || true)"
  if [ -n "$bad" ]; then
    echo "FAIL: $path sets a shell other than exactly \`shell: bash\`, which can drop errexit and pipefail"
    printf '%s\n' "$bad" | head -n 5 | sed 's/^/  /'
    exit 1
  fi
}

# The body of one TOML table, exactly: its non-comment, non-blank lines up to the next table
# header, in order, compared with the lines given.
require_toml_table() {
  path="$1"
  table="$2"
  expected="$3"
  label="$4"
  local actual
  actual="$(awk -v header="[$table]" '
    $0 == header { inside = 1; next }
    inside && /^\[/ { inside = 0 }
    inside && !/^[[:space:]]*(#.*)?$/ { line = $0; sub(/[[:space:]]+$/, "", line); print line }
  ' "$path")"
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $path [$table] is not exactly its pinned body: $label"
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") | sed 's/^/  /' || true
    exit 1
  fi
}

# A whole file's non-comment, non-blank lines, trailing space trimmed, exactly (issue #566).
require_file_lines() {
  path="$1"
  expected="$2"
  label="$3"
  local actual
  actual="$(grep -Ev '^[[:space:]]*(#.*)?$' "$path" | sed 's/[[:space:]]*$//')"
  if [ "$actual" != "$expected" ]; then
    echo "FAIL: $path is not exactly its pinned lines: $label"
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") | sed 's/^/  /' || true
    exit 1
  fi
}

# Every step selector must pick at least one cell of its job's matrix (issue #566).
# require_static_step_conditions admits `matrix.<key> == '<value>'` joined by `&&`; a selector
# naming a key or value the matrix does not have, a combination no cell has, or any selector on a
# job with no matrix, skips its step in every run while the job stays green. The job's cells are
# expanded as GitHub expands them, for the shapes the gating workflows use: keys with a list of
# values (in flow or block style) give their cross product, and each `include` entry is merged
# into every original combination it does not contradict on an original key, or becomes a
# combination of its own when it fits none (an include-only matrix is its entries). A matrix this
# cannot read (an `exclude`, a value from an expression, a flow-style matrix) fails closed.
require_live_matrix_selectors() {
  path="$1"
  local bad
  bad="$(awk '
    function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
    function unquote(s) {
      s = trim(s)
      if (s ~ /^".*"$/ || s ~ /^\047.*\047$/) s = substr(s, 2, length(s) - 2)
      return s
    }
    function indent(s) { match(s, /^ */); return RLENGTH }
    # The value of key k in combination c ("\034k=v\034k=v\034"), or "\001" when c lacks it.
    function get(c, k,   i, rest) {
      i = index(c, "\034" k "=")
      if (!i) return "\001"
      rest = substr(c, i + length(k) + 2)
      return substr(rest, 1, index(rest, "\034") - 1)
    }
    function set(c, k, v,   i, rest) {
      i = index(c, "\034" k "=")
      if (!i) return c k "=" v "\034"
      rest = substr(c, i + length(k) + 2)
      return substr(c, 1, i) k "=" v substr(rest, index(rest, "\034"))
    }
    function add_base(j, k, v) {
      if (!((j, k) in base_seen)) { base_seen[j, k] = 1; base_keys[j] = base_keys[j] "\034" k }
      base_vals[j, k] = base_vals[j, k] "\034" v
    }
    function expand(j,   nk, keys, i, kv, nv, vals, combos, nc, next_c, nn, a, b, e, ne, entry, bk, isbase,
                         pairs, np, p, ok, matched, cell_n) {
      nc = 1; combos[1] = "\034"
      nk = split(substr(base_keys[j], 2), keys, "\034")
      if (base_keys[j] == "") nk = 0
      for (i = 1; i <= nk; i++) {
        nv = split(substr(base_vals[j, keys[i]], 2), vals, "\034")
        nn = 0
        for (a = 1; a <= nc; a++)
          for (b = 1; b <= nv; b++) next_c[++nn] = set(combos[a], keys[i], vals[b])
        nc = nn
        for (a = 1; a <= nc; a++) combos[a] = next_c[a]
      }
      if (nk == 0) nc = 0
      cell_n = nc
      for (a = 1; a <= nc; a++) cell[j, a] = combos[a]
      for (e = 1; e <= includes[j]; e++) {
        np = split(substr(include[j, e], 2), pairs, "\034")
        matched = 0
        for (a = 1; a <= nc; a++) {
          ok = 1
          for (p = 1; p <= np; p++) {
            if (pairs[p] == "") continue
            kv = index(pairs[p], "=")
            bk = substr(pairs[p], 1, kv - 1)
            isbase = ((j, bk) in base_seen)
            if (isbase && get(combos[a], bk) != substr(pairs[p], kv + 1)) ok = 0
          }
          if (!ok) continue
          matched = 1
          for (p = 1; p <= np; p++) {
            if (pairs[p] == "") continue
            kv = index(pairs[p], "=")
            cell[j, a] = set(cell[j, a], substr(pairs[p], 1, kv - 1), substr(pairs[p], kv + 1))
          }
        }
        if (!matched) cell[j, ++cell_n] = include[j, e]
      }
      cells[j] = cell_n
    }
    FNR == NR {
      if ($0 ~ /^[[:space:]]*#/) next
      if ($0 ~ /^jobs:[[:space:]]*$/) { in_jobs = 1; next }
      if (!in_jobs) next
      if ($0 ~ /^  [A-Za-z0-9_-]+:[[:space:]]*$/) {
        job = trim($0); sub(/:$/, "", job); mode = ""; in_matrix = 0; next
      }
      if ($0 ~ /^      matrix:/) {
        rest = $0; sub(/^      matrix:[[:space:]]*/, "", rest)
        has_matrix[job] = 1
        if (trim(rest) != "") unreadable[job] = 1
        in_matrix = 1; mode = ""; next
      }
      if (!in_matrix || $0 ~ /^[[:space:]]*$/) next
      if (indent($0) < 8) { in_matrix = 0; next }
      line = $0
      if (indent(line) == 8) {
        key = trim(line); sub(/:.*/, "", key)
        value = line; sub(/^[^:]*:/, "", value); value = trim(value)
        if (key == "include") { mode = "include"; next }
        if (key == "exclude" || value ~ /\$\{\{/) { unreadable[job] = 1; mode = ""; next }
        if (value == "") { mode = "list"; list_key = key; next }
        mode = ""
        if (value ~ /^\[.*\]$/) {
          n = split(substr(value, 2, length(value) - 2), items, ",")
          for (i = 1; i <= n; i++) add_base(job, key, unquote(items[i]))
        } else add_base(job, key, unquote(value))
        next
      }
      if (mode == "list" && line ~ /^          - /) {
        item = line; sub(/^          - /, "", item); add_base(job, list_key, unquote(item)); next
      }
      if (mode == "include" && line ~ /^          - /) {
        includes[job]++; include[job, includes[job]] = "\034"
        line = "            " substr(line, 13)
      }
      if (mode == "include" && indent(line) == 12 && line ~ /:/) {
        key = trim(line); sub(/:.*/, "", key)
        value = line; sub(/^[^:]*:/, "", value)
        if (value ~ /\$\{\{/) unreadable[job] = 1
        include[job, includes[job]] = set(include[job, includes[job]], key, unquote(value))
        next
      }
      unreadable[job] = 1
      next
    }
    FNR == 1 { for (j in has_matrix) expand(j); in_jobs = 0 }
    /^[[:space:]]*#/ { next }
    /^jobs:[[:space:]]*$/ { in_jobs = 1; next }
    !in_jobs { next }
    /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { job = trim($0); sub(/:$/, "", job); next }
    /^      [[:space:]]*(- )?if:[[:space:]]*matrix\./ {
      value = $0
      sub(/^[[:space:]]*(- )?if:[[:space:]]*/, "", value)
      value = trim(value)
      if (!(job in has_matrix)) { print job ": " value " (the job has no matrix)"; next }
      if (job in unreadable) { print job ": " value " (the job matrix is not one this check can read)"; next }
      n = split(value, clauses, / && /)
      live = 0
      for (c = 1; c <= cells[job] && !live; c++) {
        all = 1
        for (i = 1; i <= n; i++) {
          k = clauses[i]; sub(/^matrix\./, "", k); sub(/ == .*/, "", k)
          v = clauses[i]; sub(/^[^\047]*\047/, "", v); sub(/\047.*$/, "", v)
          if (get(cell[job, c], k) != v) all = 0
        }
        if (all) live = 1
      }
      if (!live) print job ": " value " (no cell of the job matrix has it)"
    }
  ' "$path" "$path")"
  if [ -n "$bad" ]; then
    echo "FAIL: $path has a step selector that picks no cell of its job's matrix, so the step never runs:"
    printf '%s\n' "$bad" | sed 's/^/  /'
    exit 1
  fi
}

# What a step builds and how its shell runs stay out of the jobs' hands (issue #566). The
# workflow-level `env:` is pinned below and compared across files; below it, nothing may set a
# variable that changes what cargo or nextest builds and runs, or how bash starts, for a job, a
# step or every later step: no `RUST*`, `CARGO_*`, `NEXTEST_*`, `BASH_ENV`, `ENV`, `SHELLOPTS`,
# `BASHOPTS` or `PATH` key in a job or step `env:` (the rustdoc steps' `RUSTDOCFLAGS: "-D warnings"`
# is the one exception, value included), no inline assignment of those names in a command, no
# write to `$GITHUB_ENV` at all, and no write to `$GITHUB_PATH` but the wasmtime directory the
# blob round trip's wasm32 leg adds. A flow-style `env:` below `jobs:` is refused as unreadable.
# `strict-bit-matrix`, which only pr-gate.yml runs, is covered by this rule and by no comparison.
require_build_env_untouched() {
  path="$1"
  local bad
  bad="$(awk '
    function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
    function indent(s) { match(s, /^ */); return RLENGTH }
    /^[[:space:]]*#/ { next }
    /^jobs:[[:space:]]*$/ { in_jobs = 1; next }
    !in_jobs { next }
    {
      code = $0
      gsub(/\$\{\{[^}]*\}\}/, "", code)
      if (env_indent >= 0 && code !~ /^[[:space:]]*$/) {
        if (indent($0) <= env_indent) env_indent = -1
        else {
          if (key_indent < 0) key_indent = indent($0)
          if (indent($0) == key_indent) {
            name = trim($0); sub(/:.*/, "", name)
            value = $0; sub(/^[^:]*:/, "", value); value = trim(value)
            if (name ~ /^(RUST[A-Z0-9_]*|CARGO_[A-Z0-9_]+|NEXTEST_[A-Z0-9_]+|BASH_ENV|ENV|SHELLOPTS|BASHOPTS|PATH)$/ &&
                !(name == "RUSTDOCFLAGS" && value == "\"-D warnings\""))
              print NR ": " trim($0) " (a job or step env key that changes what is built or how bash runs)"
          }
          next
        }
      }
      if ($0 ~ /^ +(- )?env:/) {
        rest = $0; sub(/^ +(- )?env:/, "", rest)
        if (trim(rest) != "") print NR ": " trim($0) " (a flow-style env this check cannot read)"
        else { env_indent = indent($0); if ($0 ~ /^ +- /) env_indent += 2; key_indent = -1 }
        next
      }
      if (code ~ /GITHUB_ENV/) print NR ": " trim($0) " (a write to GITHUB_ENV)"
      command = trim(code); sub(/^(- )?run:[[:space:]]*/, "", command)
      if (code ~ /GITHUB_PATH/ && command != "echo \"$PWD/target/wasmtime-bin\" >> \"$GITHUB_PATH\"")
        print NR ": " trim($0) " (a write to GITHUB_PATH other than the wasmtime directory)"
      if (code ~ /(^|[^A-Za-z0-9_])(RUST[A-Z0-9_]*|CARGO_[A-Z0-9_]+|NEXTEST_[A-Z0-9_]+|BASH_ENV|SHELLOPTS|BASHOPTS)=/)
        print NR ": " trim($0) " (an inline assignment that changes what is built or how bash runs)"
    }
    BEGIN { env_indent = -1 }
  ' "$path")"
  if [ -n "$bad" ]; then
    echo "FAIL: $path sets build or shell environment below the workflow level:"
    printf '%s\n' "$bad" | head -n 5 | sed 's/^/  /'
    exit 1
  fi
}

# What a step's `run:` script returns, read whole (issue #566). GitHub runs it as
# `bash --noprofile --norc -eo pipefail`, and errexit does not fire for a command in a `&&` list
# unless the list is the script's last command, so `cmd && true`, `cmd && :` or `cmd && echo ok`
# followed by anything else ends the step green when `cmd` fails. For each step's script (a literal
# block line by line, a folded block or a plain value as one line, continuations joined, heredoc
# bodies and quoted text set aside, `if`/`elif`/`while`/`until` conditions set aside, and split
# on `;`) this refuses:
#   * `exit` with no status or a zero status, anywhere: `exit 0` ahead of the real command ends
#     the step green;
#   * a `&&` in any command but the script's last (closing `fi`, `done`, `esac` and braces aside);
#   * a background `&`, once `&&` and the redirections `>&`, `&>` and `<&` are removed: `cmd & wait`
#     and `(cmd &)` both return 0 whatever `cmd` returned.
# Like refuse_failure_masks it is a tripwire, not a proof. Known to pass: a `&&` that is the last
# command of a loop body (an earlier iteration's failure is lost), a status masked inside a script
# the step calls, and `!` in front of a command.
refuse_masked_step_status() {
  path="$1"
  local bad
  bad="$(awk '
    function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
    function indent(s) { match(s, /^ */); return RLENGTH }
    # Quoted text removed, quotes kept: a quoted `&&`, `;` or `exit 0` is not a command.
    function unquote_text(s,   out, i, ch, q) {
      out = ""; q = ""
      for (i = 1; i <= length(s); i++) {
        ch = substr(s, i, 1)
        if (q == "") {
          if (ch == "\047" || ch == "\"") { q = ch; out = out ch }
          else out = out ch
        } else if (ch == q && !(q == "\"" && substr(s, i - 1, 1) == "\\")) { q = ""; out = out ch }
      }
      return out
    }
    function analyse(   i, j, n, lines, logical, nl, delim, cmds, nc, parts, np, c, cmd, s) {
      nl = 0; delim = ""; logical = ""
      for (i = 1; i <= nraw; i++) {
        line = raw[i]
        gsub(/\$\{\{[^}]*\}\}/, "", line)
        if (delim != "") { if (trim(line) == delim) delim = ""; continue }
        if (line ~ /^[[:space:]]*(#.*)?$/) continue
        if (match(line, /<<-?[[:space:]]*[\047"]?[A-Za-z_][A-Za-z0-9_]*[\047"]?/)) {
          delim = substr(line, RSTART, RLENGTH); sub(/^<<-?[[:space:]]*/, "", delim); gsub(/[\047"]/, "", delim)
        }
        line = unquote_text(line)
        logical = logical (logical == "" ? "" : " ") trim(line)
        if (logical ~ /(\\|&&|\|\||\|)$/) { sub(/\\$/, "", logical); continue }
        lines[++nl] = logical; logical = ""
      }
      if (logical != "") lines[++nl] = logical
      nc = 0
      for (i = 1; i <= nl; i++) {
        s = lines[i]
        gsub(/\[\[[^]]*\]\]/, "", s)
        if (s ~ /^(if|elif|while|until)[[:space:]]/) {
          if (match(s, /;[[:space:]]*(then|do)([[:space:]]|$)/)) s = substr(s, RSTART + RLENGTH)
          else s = ""
        }
        gsub(/;;/, ";", s)
        np = split(s, parts, ";")
        for (j = 1; j <= np; j++) {
          cmd = trim(parts[j])
          while (cmd ~ /^(then|do|else|\{|\()([[:space:]]|$)/) { sub(/^(then|do|else|\{|\()[[:space:]]*/, "", cmd) }
          if (cmd ~ /^(fi|done|esac|\}|\))?$/) continue
          cmds[++nc] = cmd
        }
      }
      for (c = 1; c <= nc; c++) {
        cmd = cmds[c]
        if (cmd ~ /(^|[[:space:](])exit([[:space:]]+0+)?[[:space:]]*\)?$/)
          print job ": " cmd " (exit with no status or a zero status)"
        if (c < nc && index(cmd, "&&"))
          print job ": " cmd " (a && list that is not the step last command, so errexit ignores its failure)"
        s = cmd
        gsub(/&&/, "", s); gsub(/[0-9]*>&[0-9-]*/, "", s); gsub(/&>>?/, "", s); gsub(/<&[0-9-]*/, "", s)
        if (index(s, "&")) print job ": " cmd " (a background &)"
      }
    }
    function flush() { if (nraw) analyse(); nraw = 0; in_block = 0 }
    /^jobs:[[:space:]]*$/ { in_jobs = 1; next }
    !in_jobs { next }
    in_block {
      if ($0 ~ /^[[:space:]]*$/ || indent($0) > key_indent) {
        line = $0
        if (block_indent < 0 && line !~ /^[[:space:]]*$/) block_indent = indent(line)
        line = substr(line, block_indent + 1)
        if (style == ">") { if (trim(line) != "") folded = folded " " trim(line) }
        else raw[++nraw] = line
        next
      }
      if (style == ">") raw[++nraw] = folded
      in_block = 0
      flush()
    }
    /^[[:space:]]*#/ { next }
    /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { flush(); job = trim($0); sub(/:$/, "", job); next }
    /^      - / { flush() }
    /^(      - |        )run:/ {
      key_indent = 8
      value = $0; sub(/^ *(- )?run:[[:space:]]*/, "", value); value = trim(value)
      if (value ~ /^[|>][-+]?$/) {
        style = substr(value, 1, 1); in_block = 1; block_indent = -1; folded = ""; nraw = 0
      } else {
        if (value ~ /^".*"$/ || value ~ /^\047.*\047$/) value = substr(value, 2, length(value) - 2)
        nraw = 1; raw[1] = value; flush()
      }
      next
    }
    END { if (in_block && style == ">") raw[++nraw] = folded; flush() }
  ' "$path")"
  if [ -n "$bad" ]; then
    echo "FAIL: $path contains a refused gate pattern: a masked step status (an exit with no or a zero status, a && list before the last command, or a background &), which lets a failing command read as green"
    printf '%s\n' "$bad" | head -n 5 | sed 's/^/  /'
    exit 1
  fi
}

# The per-PR lane filter (issue #393). A job-level `if:` is allowed only in the exact form
# `needs.changes.outputs.<lane> == 'true'` for lane core, leverage or readme, on a job that needs
# `changes`. `gate_scripts` is not a filter lane (issue #502 retired `gate (light)`, the one job it
# gated): it stays a classifier output, informational like `docs_only`, and a job filtered on it
# fails here. CI OK's JOB_LANES must list exactly the filtered jobs with their lanes, because it
# is what lets ci_ok.py accept their `skipped` result, and CI OK must run that result check. The
# `changes` job must export all five outputs from the classifier. Named jobs are pinned to their
# lane, so a determinism job cannot drift onto a lighter lane or an always-on job become optional.
# The `readme` lane (issue #503) is the root README's: `facade-contract` is pinned to it, so the
# Quickstart byte-pin and the executed Quickstart run when the README changes without the
# determinism lane, and cannot drift back onto `core` (where a README-only change would skip them)
# or lose their JOB_LANES entry (where ci_ok.py would read their skip as a failure).
require_lane_filter() {
  path="$1"
  local conditions expected actual job condition lane want got
  conditions="$(job_conditions "$path")"
  expected=""
  while IFS=$'\t' read -r job condition; do
    [ -n "$job" ] || continue
    if [[ "$condition" =~ ^needs\.changes\.outputs\.(core|leverage|readme)\ ==\ \'true\'$ ]]; then
      lane="${BASH_REMATCH[1]}"
    else
      echo "FAIL: $path job $job has a job-level if: that is not a lane filter: $condition"
      exit 1
    fi
    require_job_needs "$path" "$job" changes
    expected+="$job=$lane"$'\n'
  done <<< "$conditions"
  expected="$(printf '%s' "$expected" | sed '/^$/d' | sort)"
  actual="$(awk '
    $0 == "  ci-ok:" { found = 1; next }
    found && /^  [^ ]/ { exit }
    found && /^          JOB_LANES:[[:space:]]*>-[[:space:]]*$/ { in_list = 1; next }
    found && in_list && /^            [^[:space:]]/ {
      line = $0; sub(/^[[:space:]]+/, "", line); sub(/[[:space:]]+$/, "", line)
      count = split(line, items, /[[:space:]]+/)
      for (i = 1; i <= count; i++) print items[i]
      next
    }
    found && in_list { in_list = 0 }
  ' "$path" | sort)"
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    echo "FAIL: $path CI OK JOB_LANES does not match the lane-filtered jobs' if: lines"
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") | sed 's/^/  /' || true
    exit 1
  fi
  for want in \
    determinism-matrix=core determinism-compare=core strict-bit-matrix=core strict-bit-compare=core \
    crate-tests=core golden-gen-hygiene=core golden-gen-firewall=core build=core clippy=core \
    rustdoc=core deny=core unused-deps=core default-no-db=core no-std-build=core \
    blob-round-trip-wasm=core portable-snapshot-macos=core portable-snapshot-compare=core \
    leverage-lane=leverage facade-contract=readme \
    changes=always fmt=always file-size-cap=always no-secret-scan=always docs=always \
    gate-fixtures=always package-publication-contract=always; do
    job="${want%%=*}"
    lane="${want#*=}"
    if ! workflow_jobs "$path" | grep -qx -- "$job"; then
      echo "FAIL: $path is missing the $job job"
      exit 1
    fi
    got="$(grep -E "^$job=" <<< "$expected" | cut -d= -f2 || true)"
    [ -n "$got" ] || got=always
    if [ "$got" != "$lane" ]; then
      echo "FAIL: $path job $job runs on lane $got, not $lane"
      exit 1
    fi
  done
  require_job_pattern "$path" 'ci-ok' '^[[:space:]]*(- )?run: python3 scripts/ci/ci_ok\.py$' \
    'CI OK runs the lane-aware result check'
  for lane in core leverage readme gate_scripts docs_only; do
    require_job_pattern "$path" 'changes' \
      "^      $lane: \\\$\\{\\{ steps\\.classify\\.outputs\\.$lane \\}\\}\$" \
      "changes job exports the $lane lane output"
  done
  require_job_pattern "$path" 'changes' '^[[:space:]]*fetch-depth: 2$' \
    'changes job fetches the merge commit and its base parent'
  require_job_pattern "$path" 'changes' \
    'git diff --no-renames --name-only HEAD\^1 HEAD' \
    'changes job lists the merge against its base, renames as delete plus add'
  require_job_pattern "$path" 'changes' \
    'bash scripts/ci/classify-changes\.sh < "\$RUNNER_TEMP/changed-paths" \| tee -a "\$GITHUB_OUTPUT"' \
    'changes job classifies the changed paths into its outputs'
  require_job_pattern "$path" 'changes' \
    'bash scripts/ci/classify-changes\.sh --all \| tee -a "\$GITHUB_OUTPUT"' \
    'changes job runs every lane without a PR merge checkout'
}

# The per-PR gate runs once per change: on the pull request. A `push` trigger would re-run the
# whole gate on the merge commit in `development` — the same tree the PR already proved — so any
# `push` event in the workflow's top-level `on:` fails, with or without a branch filter. The parse
# accepts every trigger spelling YAML allows: a scalar (`on: push`), a flow list
# (`on: [push, pull_request]`), a flow map (`on: {push: ..., pull_request: ...}`, also across
# lines), and a block map or block list at any indent, with plain or quoted keys. The
# `pull_request` trigger must remain.
require_pr_trigger_only() {
  local path="$1"
  local triggers
  triggers="$(awk '
    function unquote(word) {
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", word)
      gsub(/^["\047]|["\047]$/, "", word)
      return word
    }
    # Top-level entries of a flow collection: keys of a map, items of a list.
    function flow_events(text,    i, c, depth, kind, token, in_value) {
      depth = 0; kind = ""; token = ""; in_value = 0
      for (i = 1; i <= length(text); i++) {
        c = substr(text, i, 1)
        if (c == "{" || c == "[") {
          depth++
          if (depth == 1) kind = c
          continue
        }
        if (c == "}" || c == "]") {
          depth--
          if (depth == 0) {
            if (!in_value && token != "") print unquote(token)
            return
          }
          continue
        }
        if (depth != 1) continue
        if (c == ",") {
          if (!in_value && token != "") print unquote(token)
          token = ""; in_value = 0
          continue
        }
        if (c == ":" && kind == "{" && !in_value) {
          print unquote(token)
          token = ""; in_value = 1
          continue
        }
        if (!in_value) token = token c
      }
    }
    function bracket_balance(text,    i, c, depth) {
      depth = 0
      for (i = 1; i <= length(text); i++) {
        c = substr(text, i, 1)
        if (c == "{" || c == "[") depth++
        else if (c == "}" || c == "]") depth--
      }
      return depth
    }
    function strip_comment(line) {
      sub(/(^|[[:space:]])#.*$/, "", line)
      return line
    }
    flow != "" {
      flow = flow " " strip_comment($0)
      if (bracket_balance(flow) <= 0) { flow_events(flow); flow = ""; done = 1 }
      next
    }
    done { next }
    /^("on"|\047on\047|on)[[:space:]]*:/ {
      inline = strip_comment($0)
      sub(/^[^:]*:[[:space:]]*/, "", inline)
      sub(/[[:space:]]+$/, "", inline)
      if (inline ~ /^[[{]/) {
        if (bracket_balance(inline) <= 0) { flow_events(inline); done = 1 }
        else flow = inline
      } else if (inline != "") {
        print unquote(inline); done = 1
      } else {
        in_on = 1
      }
      next
    }
    !in_on { next }
    /^[[:space:]]*(#.*)?$/ { next }
    {
      match($0, /^[[:space:]]*/)
      indent = RLENGTH
      if (indent == 0) { in_on = 0; done = 1; next }
      if (child == 0) child = indent
      if (indent != child) next
      entry = substr($0, indent + 1)
      if (entry ~ /^-[[:space:]]/) {
        sub(/^-[[:space:]]+/, "", entry)
        print unquote(strip_comment(entry))
      } else {
        sub(/[[:space:]]*:.*$/, "", entry)
        print unquote(entry)
      }
    }
  ' "$path")"
  if grep -qx 'push' <<< "$triggers"; then
    echo "FAIL: $path has a push trigger; the per-PR gate must not re-run on merges into development"
    exit 1
  fi
  if ! grep -qx 'pull_request' <<< "$triggers"; then
    echo "FAIL: $path is missing its pull_request trigger"
    exit 1
  fi
}

# Every `uses:` of a remote action must name a full 40-hex commit SHA, never a movable tag or
# branch. Local (`./`) and `docker://` references are exempt.
require_pinned_actions() {
  path="$1"
  local unpinned
  unpinned="$(grep -Ev '^[[:space:]]*#' "$path" \
    | grep -E '^[[:space:]]*(- )?uses:' \
    | grep -Ev 'uses:[[:space:]]*(\./|docker://)' \
    | grep -Ev 'uses:[[:space:]]*[^[:space:]@]+@[0-9a-f]{40}([[:space:]]|$)' || true)"
  if [ -n "$unpinned" ]; then
    echo "FAIL: $path uses an action not pinned to a full commit SHA:"
    printf '%s\n' "$unpinned" | sed 's/^/  /'
    exit 1
  fi
}

require_lints_workspace() {
  path="$1"
  if ! awk '
    /^\[lints\]$/ { in_lints = 1; next }
    /^\[/ { in_lints = 0 }
    in_lints && /^[[:space:]]*workspace[[:space:]]*=[[:space:]]*true[[:space:]]*$/ { found = 1 }
    END { exit found ? 0 : 1 }
  ' "$path"; then
    echo "FAIL: $path is missing [lints] workspace = true"
    exit 1
  fi
}

require_unsafe_forbid_layers() {
  local manifest_count=0
  local lib_count=0
  local manifest=""
  local lib=""
  local first_line=""

  require_file "$ROOT_CARGO_TOML"
  if [ ! -d "$CRATES_DIR" ]; then
    echo "FAIL: crates directory is missing: $CRATES_DIR"
    exit 1
  fi

  require_pattern "$ROOT_CARGO_TOML" '^[[:space:]]*unsafe_code[[:space:]]*=[[:space:]]*"forbid"' \
    'workspace.lints.rust unsafe_code = "forbid"'

  for manifest in "$CRATES_DIR"/*/Cargo.toml "$CRATES_DIR"/leverage/*/Cargo.toml; do
    [ -e "$manifest" ] || continue
    manifest_count=$((manifest_count + 1))
    require_lints_workspace "$manifest"
  done

  if [ "$manifest_count" -eq 0 ]; then
    echo "FAIL: no crate Cargo.toml files found under $CRATES_DIR"
    exit 1
  fi

  for lib in "$CRATES_DIR"/*/src/lib.rs "$CRATES_DIR"/leverage/*/src/lib.rs; do
    [ -e "$lib" ] || continue
    lib_count=$((lib_count + 1))
    first_line="$(sed -n '1p' "$lib")"
    if [ "$first_line" != '#![forbid(unsafe_code)]' ]; then
      echo "FAIL: $lib line 1 must be #![forbid(unsafe_code)]"
      exit 1
    fi
  done

  if [ "$lib_count" -eq 0 ]; then
    echo "FAIL: no crate src/lib.rs files found under $CRATES_DIR"
    exit 1
  fi

  # Firmware members (issue #402) hold no `unsafe` either: their manifests take the workspace lints,
  # their lib roots start with the forbid, and every bin root carries it, since an image's entry,
  # allocator and panic handler live in its bin.
  for manifest in "$FIRMWARE_DIR"/*/Cargo.toml; do
    [ -e "$manifest" ] || continue
    require_lints_workspace "$manifest"
  done
  for lib in "$FIRMWARE_DIR"/*/src/lib.rs; do
    [ -e "$lib" ] || continue
    first_line="$(sed -n '1p' "$lib")"
    if [ "$first_line" != '#![forbid(unsafe_code)]' ]; then
      echo "FAIL: $lib line 1 must be #![forbid(unsafe_code)]"
      exit 1
    fi
  done
  for bin in "$FIRMWARE_DIR"/*/src/main.rs "$FIRMWARE_DIR"/*/src/bin/*.rs; do
    [ -e "$bin" ] || continue
    if ! grep -qx '#!\[forbid(unsafe_code)\]' "$bin"; then
      echo "FAIL: $bin must carry #![forbid(unsafe_code)]"
      exit 1
    fi
  done
}

# The mdBook binary is restored from actions/cache before the installer runs, keyed on the pinned
# digests and the installer, with no restore-keys: an exact hit or nothing, so a stale binary can
# never be restored for a bumped pin. The installer re-verifies whatever it finds (issue #416).
require_mdbook_cache() {
  path="$1"
  job="$2"
  require_job_pattern "$path" "$job" \
    "^[[:space:]]*(- )?uses:[[:space:]]*actions/cache@[0-9a-f]{40}[[:space:]]+#" \
    "$job restores the verified mdBook binary through a SHA-pinned actions/cache"
  require_job_pattern "$path" "$job" '^[[:space:]]*path:[[:space:]]*target/mdbook-bin[[:space:]]*$' \
    "$job caches target/mdbook-bin"
  require_job_pattern "$path" "$job" \
    "^[[:space:]]*key:[[:space:]]*mdbook-.*hashFiles\\('site/mdbook-checksums\\.txt', 'scripts/docs/install-mdbook\\.sh'\\)" \
    "$job keys the mdBook cache on the pinned checksums and the installer"
  local body cache_line install_line
  body="$(awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found { print }
  ' "$path" | grep -Ev '^[[:space:]]*#')"
  if grep -Eq 'restore-keys' <<< "$body"; then
    echo "FAIL: $path job $job sets restore-keys on the mdBook cache; a prefix restore could serve a stale binary"
    exit 1
  fi
  cache_line="$(grep -En 'uses:[[:space:]]*actions/cache@' <<< "$body" | head -n1 | cut -d: -f1)"
  install_line="$(grep -En 'run: bash scripts/docs/install-mdbook\.sh target/mdbook-bin$' <<< "$body" | head -n1 | cut -d: -f1)"
  if [ -z "$cache_line" ] || [ -z "$install_line" ] || [ "$cache_line" -ge "$install_line" ]; then
    echo "FAIL: $path job $job must restore the mdBook cache before the install step"
    exit 1
  fi
}

# The tool workspaces some oce-cxf tests build with `cargo --offline` (issue #610). Those tests (the
# OpenModelica evidence checks of `fixture_structural_oracle`) need the tools' crates, which no
# build of the root workspace downloads and no cache keyed on the root lockfile need hold: the
# verify job of tag v0.5.0, with no cache, failed 15 of them. So every job that runs the oce-cxf
# suite, or the whole workspace's, must run the prefetch script, as a step of its own, before it.
#
# A job runs those tests when one of its `run:` commands (inline, a literal block with its
# backslash continuations joined, or a folded block joined whole) is `cargo test`, `cargo t`,
# `cargo nextest run`/`r` or `cargo-nextest`, a `+toolchain` included, that selects oce-cxf
# (`-p`/`--package`, spaced, joined or with `=`), the workspace (`--workspace`, `--all`), or nothing
# at all from the repository root, where a bare command runs every member. Not counted: `--doc`
# (doctests build no tool), `--archive-file` (the archive's packages were chosen where it was
# built; no archive holds oce-cxf today), another selection, a bare command under a
# `working-directory:` (another workspace, golden-gen's), and a job whose `runs-on:` is a literal
# `windows-*` label (the tests are `#[cfg(unix)]`; an expression counts). The prefetch must be the
# whole command of a step with no `if:` and no `continue-on-error:`, on an earlier step.
OFFLINE_TOOL_PREFETCH='bash scripts/ci/prefetch-offline-tools.sh'

require_offline_tool_prefetch() {
  path="$1"
  local bad
  bad="$(awk -v prefetch="$OFFLINE_TOOL_PREFETCH" '
    function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
    function unquote(s,   q, first, last) {
      q = sprintf("%c", 39)
      first = substr(s, 1, 1); last = substr(s, length(s), 1)
      if (length(s) >= 2 && first == last && (first == "\"" || first == q)) s = substr(s, 2, length(s) - 2)
      return s
    }
    function indent(s) { match(s, /^ */); return RLENGTH }
    function runs_suite(c, wd) {
      if (c !~ /(^|[^[:alnum:]_.\/-])(cargo([[:space:]]+\+[^[:space:]]+)?[[:space:]]+(test|t|nextest[[:space:]]+(run|r))|cargo-nextest([[:space:]]+nextest)?[[:space:]]+(run|r))([[:space:]]|$)/) return 0
      if (c ~ /(^|[[:space:]])--doc([[:space:]]|$)/) return 0
      if (c ~ /--archive-file/) return 0
      if (c ~ /(^|[[:space:]])(-p|--package)([[:space:]]+|=)?oce-cxf([^[:alnum:]_-]|$)/) return 1
      if (c ~ /(^|[[:space:]])(--workspace|--all)([[:space:]=]|$)/) return 1
      if (c ~ /(^|[[:space:]])(-p|--package|--manifest-path)([[:space:]=]|[[:alnum:]])/) return 0
      return wd == "" || wd == "." || wd == "./"
    }
    function command(c) {
      c = trim(c)
      if (c == "") return
      if (c == prefetch && !step_masked) { prefetched = 1; return }
      if (first_test == "" && !windows && runs_suite(c, step_wd != "" ? step_wd : job_wd)) {
        first_test = c
        if (!prefetched) print job ": `" c "` runs before any unmasked `" prefetch "` step"
      }
    }
    function end_block(   n, k, part, joined) {
      if (!in_block) return
      in_block = 0
      if (block_folded) { command(block); block = ""; return }
      n = split(block, parts, "\n")
      joined = ""
      for (k = 1; k <= n; k++) {
        part = parts[k]
        if (part ~ /\\[[:space:]]*$/) { sub(/\\[[:space:]]*$/, " ", part); joined = joined part; continue }
        command(joined part)
        joined = ""
      }
      if (joined != "") command(joined)
      block = ""
    }
    function end_step(   k) {
      end_block()
      for (k = 1; k <= ncmd; k++) command(cmd[k])
      ncmd = 0; step_masked = 0; step_wd = ""; in_step = 0
    }
    function end_job() {
      end_step()
      job = ""; job_wd = ""; windows = 0; prefetched = 0; first_test = ""; in_steps = 0
    }
    /^jobs:[[:space:]]*$/ { in_jobs = 1; next }
    in_jobs && /^[^[:space:]#]/ { end_job(); in_jobs = 0 }
    !in_jobs { next }
    /^[[:space:]]*#/ { next }
    /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { end_job(); job = $0; sub(/^  /, "", job); sub(/:.*/, "", job); next }
    job == "" { next }
    in_block {
      if ($0 ~ /^[[:space:]]*$/ || indent($0) > block_indent) {
        block = block (block == "" ? "" : (block_folded ? " " : "\n")) trim($0)
        next
      }
      end_block()
    }
    /^    runs-on:/ {
      value = $0; sub(/^    runs-on:[[:space:]]*/, "", value)
      windows = (unquote(trim(value)) ~ /^windows-/)
      next
    }
    /^    steps:/ { in_steps = 1; next }
    /^    [^ ]/ { end_step(); in_steps = 0 }
    !in_steps && /^        working-directory:/ {
      value = $0; sub(/^[^:]*:[[:space:]]*/, "", value); job_wd = unquote(trim(value)); next
    }
    !in_steps { next }
    { line = $0 }
    /^      - / { end_step(); in_step = 1; sub(/^      - /, "        ", line) }
    in_step && line ~ /^        (if|continue-on-error):/ { step_masked = 1 }
    in_step && line ~ /^        working-directory:/ {
      value = line; sub(/^[^:]*:[[:space:]]*/, "", value); step_wd = unquote(trim(value))
    }
    in_step && line ~ /^        run:/ {
      value = line; sub(/^        run:[[:space:]]*/, "", value); value = trim(value)
      if (value ~ /^[|>][-+]?$/) { in_block = 1; block = ""; block_indent = 8; block_folded = (value ~ /^>/) }
      else cmd[++ncmd] = unquote(value)
    }
    END { end_job() }
  ' "$path")"
  if [ -n "$bad" ]; then
    echo "FAIL: $path runs the oce-cxf or workspace suite, whose OpenModelica checks build tool workspaces offline, without running \`$OFFLINE_TOOL_PREFETCH\` first (issue #610):"
    printf '%s\n' "$bad" | sed 's/^/  /'
    exit 1
  fi
}

# The gate script runs the same tests (its structural-oracle step and `full`'s workspace suite), so
# it must run the prefetch, as a whole step line, before the first of its oce-cxf or workspace test
# commands, backslash continuations joined. Comment lines never count.
require_gate_script_prefetch() {
  local step_line="step 'prefetch the offline tool workspaces' $OFFLINE_TOOL_PREFETCH"
  local order prefetch_at test_at
  order="$(awk -v wanted="$step_line" '
    function trim(s) { sub(/^[[:space:]]+/, "", s); sub(/[[:space:]]+$/, "", s); return s }
    /^[[:space:]]*#/ { next }
    {
      line = $0
      if (line ~ /\\[[:space:]]*$/) {
        sub(/\\[[:space:]]*$/, " ", line); held = held line; if (start == 0) start = NR; next
      }
      logical = trim(held line); first = (start == 0 ? NR : start); held = ""; start = 0
      if (logical == wanted && prefetch == 0) prefetch = first
      if (test == 0 && logical ~ /cargo[[:space:]]+(nextest[[:space:]]+run|test)([[:space:]]|$)/ &&
          logical !~ /--doc/ &&
          (logical ~ /(-p|--package)([[:space:]]+|=)?oce-cxf([^[:alnum:]_-]|$)/ ||
           logical ~ /--workspace([[:space:]=]|$)/)) test = first
    }
    END { print prefetch + 0, test + 0 }
  ' "$GATE_SCRIPT")"
  prefetch_at="${order% *}"
  test_at="${order#* }"
  if [ "$prefetch_at" -eq 0 ] || { [ "$test_at" -ne 0 ] && [ "$prefetch_at" -ge "$test_at" ]; }; then
    echo "FAIL: $GATE_SCRIPT must run \`$step_line\`, the whole line, before its first oce-cxf or workspace test step (issue #610)"
    exit 1
  fi
}

ci="$WORKFLOW_DIR/pr-gate.yml"
merge="$WORKFLOW_DIR/merge-gate.yml"
release="$WORKFLOW_DIR/release-gate.yml"
advisories="$WORKFLOW_DIR/advisories.yml"
publish="$WORKFLOW_DIR/release.yml"

require_file "$ci"
require_file "$merge"
require_file "$release"
require_file "$advisories"
require_file "$publish"
require_file "$DENY_TOML"
require_file "$GATE_SCRIPT"
require_unsafe_forbid_layers
require_pinned_actions "$ci"
require_pinned_actions "$merge"
require_pinned_actions "$release"
require_pinned_actions "$advisories"

# Light per-PR dependency drift gate.
require_pattern "$ci" 'cargo-machete' 'install cargo-machete'
require_pattern "$ci" 'cargo machete' 'run cargo machete'
require_pattern "$ci" 'check-default-no-db\.sh' 'run default-no-db smoke'
require_pattern "$ci" 'test-check-default-no-db\.sh' 'run default-no-db fixture tests'
require_pattern "$ci" 'test-check-golden-gen-anti-tautology\.sh' \
  'run golden-gen firewall fixture tests'
require_pattern "$ci" 'check-golden-gen-anti-tautology\.sh' 'run golden-gen anti-tautology firewall'
require_pattern "$ci" 'scripts/package_policy/test_validate\.py' \
  'run package publication hostile controls'
require_pattern "$ci" 'scripts/package_policy/validate\.py' \
  'run metadata-driven package publication contract'
require_pattern "$ci" '^[[:space:]]*run: python3 scripts/authority_claims/check\.py --check$' \
  'run authority claim consistency check'
require_pattern "$ci" '^[[:space:]]*run: python3 scripts/authority_claims/test_check\.py$' \
  'run authority claim hostile controls'
require_pattern "$ci" 'test-check-stale-crate-status\.sh' 'run stale crate-status fixture tests'
require_pattern "$ci" 'check-stale-crate-status\.sh' 'run stale crate-status smoke'
require_pattern "$ci" 'check-workflow-gates\.sh' 'run workflow gate smoke'
require_pattern "$ci" 'determinism-matrix' 'targeted cross-arch determinism matrix job'
refuse_emulation "$ci"
refuse_emulation "$merge"
refuse_emulation "$release"
# Per PR the determinism matrix is all four native cells, debug and release codegen on both
# architectures, and its fan-in compares across codegen as well as across architectures
# (issue #556: the release cells and the cross-codegen comparisons ran after the merge from
# issue #502 until the release build of the subset fell to about three minutes). The strict-bit
# matrix has been four cells throughout.
require_native_cell_matrix "$ci" 'determinism-matrix' 'debug, release'
require_native_cell_matrix "$ci" 'strict-bit-matrix' 'debug, release'
require_pattern "$ci" 'nextest@0\.9\.143' 'pinned cargo-nextest 0.9.143 install'
require_determinism_cells "$ci"
require_release_codegen_determinism_cells "$ci"
# A release cell never writes the rust-cache entry the debug cells, the crate suites and the
# strict-bit cells restore: rust-cache writes a key once, on the first job to finish, and a
# dispatch of this workflow on the default branch saves into the scope every PR reads.
require_job_pattern "$ci" 'determinism-matrix' "^          save-if: \\\$\\{\\{ matrix\\.codegen == 'debug' \\}\\}\$" \
  'only the debug determinism cells save the shared rust-cache entry'
# The blob round trip (issue #403): three legs, one recorded artifact. Every determinism cell
# records the suite natively, the fan-in requires the recordings equal and replays the x86_64
# debug recording through each architecture's archived suite, and the wasm32-wasip1 leg replays it
# under the checksum-pinned wasmtime.
require_determinism_fan_in "$ci"
require_cross_codegen_fan_in "$ci"
require_job_needs "$ci" 'blob-round-trip-wasm' 'determinism-matrix'
require_job_pattern "$ci" 'blob-round-trip-wasm' '^    runs-on:[[:space:]]*ubuntu-latest[[:space:]]*$' \
  'the wasm32 leg runs on the x86_64 runner'
require_job_pattern "$ci" 'blob-round-trip-wasm' '^[[:space:]]*(- )?run: rustup target add wasm32-wasip1$' \
  'the wasm32 leg installs the wasm32-wasip1 target'
require_job_pattern "$ci" 'blob-round-trip-wasm' \
  '^[[:space:]]*(- )?run: bash scripts/ci/install-wasmtime\.sh target/wasmtime-bin$' \
  'the wasm32 leg installs the checksum-pinned wasmtime'
require_job_pattern "$ci" 'blob-round-trip-wasm' \
  "^[[:space:]]*key:[[:space:]]*wasmtime-.*hashFiles\\('scripts/ci/install-wasmtime\\.sh'\\)" \
  'the wasm32 leg keys its wasmtime cache on the pinned installer'
require_job_pattern "$ci" 'blob-round-trip-wasm' '^[[:space:]]*name:[[:space:]]*determinism-x86_64-debug$' \
  'the wasm32 leg downloads the x86_64 debug recording'
require_job_pattern "$ci" 'blob-round-trip-wasm' \
  'OCE_ROUND_TRIP_EXPECT: target/round-trip-recorded/round-trip$' \
  'the wasm32 leg replays the downloaded recording'
require_job_pattern "$ci" 'blob-round-trip-wasm' '^[[:space:]]*wasmtime --version$' \
  'the wasm32 leg prints the wasmtime version it ran under'
require_job_pattern "$ci" 'blob-round-trip-wasm' \
  '^[[:space:]]*cargo nextest run -p oce-conformance --test blob_round_trip --target wasm32-wasip1 --locked --profile ci --no-tests=fail$' \
  'the wasm32 leg runs the round trip on wasm32-wasip1 with hard-fail-on-zero-tests'
require_job_pattern "$ci" 'blob-round-trip-wasm' '^[[:space:]]*(- )?run: test -s target/nextest/ci/junit\.xml$' \
  'the wasm32 leg refuses an absent or empty nextest report'
require_job_order "$ci" 'blob-round-trip-wasm' 'run: rustup target add wasm32-wasip1' \
  'cargo nextest run -p oce-conformance --test blob_round_trip --target wasm32-wasip1' \
  'the wasm32 leg installs its target before running on it'
require_job_order "$ci" 'blob-round-trip-wasm' 'run: bash scripts/ci/install-wasmtime.sh target/wasmtime-bin' \
  'cargo nextest run -p oce-conformance --test blob_round_trip --target wasm32-wasip1' \
  'the wasm32 leg installs wasmtime before running under it'
# Platform observation cell (issue #394): macOS arm64 portable snapshot. The Windows std build is
# merge-gate.yml's and release-gate.yml's (below).
require_job_pattern "$ci" 'portable-snapshot-macos' '^    runs-on:[[:space:]]*macos-15[[:space:]]*$' \
  'the macOS portable-snapshot cell runs on macos-15'
require_job_pattern "$ci" 'portable-snapshot-macos' 'test "\$\(uname -s\)-\$\(uname -m\)" = Darwin-arm64' \
  'the macOS cell refuses a runner that is not macOS arm64'
require_job_pattern "$ci" 'portable-snapshot-macos' \
  '^[[:space:]]*(- )?run: cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail$' \
  'the macOS cell runs the debug determinism subset with hard-fail-on-zero-tests'
require_job_pattern "$ci" 'portable-snapshot-macos' 'OCE_PORTABLE_STATE_OUT: target/portable-state-debug\.bin' \
  'the macOS cell emits its portable state vector'
require_job_pattern "$ci" 'portable-snapshot-macos' 'OCE_TARGET_STATE_OUT: target/target-state-debug\.bin' \
  'the macOS cell emits its target-bound state vector'
require_job_pattern "$ci" 'portable-snapshot-macos' 'test -s target/portable-state-debug\.bin' \
  'the macOS cell refuses an absent or empty portable state vector'
require_job_pattern "$ci" 'portable-snapshot-macos' 'test -s target/nextest/ci/junit\.xml' \
  'the macOS cell refuses an absent or empty nextest report'
require_job_pattern "$ci" 'portable-snapshot-macos' 'name: platform-snapshot-macos-arm64-debug' \
  'the macOS cell uploads its state vectors'
require_job_pattern "$ci" 'portable-snapshot-macos' 'if-no-files-found: error' \
  'a macOS cell with no vectors fails its upload'
require_job_needs "$ci" 'portable-snapshot-compare' 'determinism-matrix'
require_job_needs "$ci" 'portable-snapshot-compare' 'portable-snapshot-macos'
require_job_pattern "$ci" 'portable-snapshot-compare' \
  '^[[:space:]]*cmp linux-x86_64/portable-state-debug\.bin macos-arm64/portable-state-debug\.bin$' \
  'compare the macOS portable state bytes with Linux x86_64'
require_job_pattern "$ci" 'portable-snapshot-compare' '^[[:space:]]*for linux in linux-x86_64 linux-aarch64; do$' \
  'compare the macOS target-bound state bytes with both Linux architectures'
require_job_literal "$ci" 'portable-snapshot-compare' \
  'if cmp -s "$linux/target-state-debug.bin" macos-arm64/target-state-debug.bin; then' \
  'require the macOS target-bound state bytes to differ from Linux'
# Per-PR crate suites outside the determinism subset (issue #370).
require_job_pattern "$ci" 'crate-tests' 'tool:[[:space:]]*nextest@0\.9\.143' \
  'pinned cargo-nextest install for the crate suites'
require_job_pattern "$ci" 'crate-tests' \
  '^[[:space:]]*cargo nextest run -p oce-graph -p oce-validate -p oce-model -p oce-cxf -p oce-store --locked --profile ci --no-tests=fail$' \
  'run the oce-graph, oce-validate, oce-model, oce-cxf and oce-store suites per PR and hard-fail on zero tests'
require_job_pattern "$ci" 'crate-tests' '^[[:space:]]*(- )?run: test -s target/nextest/ci/junit\.xml$' \
  'refuse an absent or empty crate-suite report'
# The `no_std + alloc` build of the core four (issue #397): both targets, no default features, and
# the oce-expr suite that exercises the program-wide fuel meter.
# The bare-metal cell (issues #397, #402): the three targets, the core four's build, clippy and
# graph scan with its controls, the oce-expr suite without std, and the firmware image's build,
# size budget, size-checker controls and host run. The scripts own the commands; check-no-std.sh
# names the crates, features and targets.
no_std_targets='rustup target add thumbv7em-none-eabihf riscv32imac-unknown-none-elf wasm32-unknown-unknown'
require_job_literal "$ci" 'no-std-build' "$no_std_targets" 'install the no_std targets'
require_job_pattern "$ci" 'no-std-build' '^[[:space:]]*(- )?run: bash scripts/ci/check-no-std\.sh$' \
  'build, lint and graph-scan the core four no_std + alloc on every target'
require_job_pattern "$ci" 'no-std-build' '^[[:space:]]*(- )?run: bash scripts/ci/test-check-no-std\.sh$' \
  'prove the no_std check fails on std'
require_job_literal "$ci" 'no-std-build' \
  'cargo test -p oce-expr --no-default-features --features alloc --lib --locked -- --test-threads=1' \
  'run the oce-expr unit suite without std'
require_job_pattern "$ci" 'no-std-build' '^[[:space:]]*(- )?run: bash scripts/ci/check-firmware-link\.sh$' \
  'build the firmware image for each bare-metal target within its size budget'
require_job_pattern "$ci" 'no-std-build' \
  '^[[:space:]]*(- )?run: python3 -I scripts/ci/test-firmware-size\.py$' \
  'run the firmware size-check controls'
require_job_pattern "$ci" 'no-std-build' '^[[:space:]]*(- )?run: cargo test -p oce-firmware-link --locked$' \
  'run the firmware image on the host against every golden'
require_job_order "$ci" 'no-std-build' "$no_std_targets" 'run: bash scripts/ci/check-no-std.sh' \
  'the bare-metal cell installs its targets before building for them'
# Documentation checks under CI OK (issue #377); docs-pages.yml is path-filtered and not required.
require_job_pattern "$ci" 'docs' '^[[:space:]]*(- )?run: bash scripts/docs/install-mdbook\.sh target/mdbook-bin$' \
  'install the checksum-pinned mdBook'
require_job_pattern "$ci" 'docs' '^[[:space:]]*(- )?run: python3 scripts/docs/test_check_links\.py$' \
  'run the documentation checker contracts'
require_job_pattern "$ci" 'docs' \
  '^[[:space:]]*(- )?run: python3 scripts/docs/test_authority_literals\.py --mdbook target/mdbook-bin/mdbook$' \
  'run the authority literal suite through mdBook'
require_job_pattern "$ci" 'docs' '^[[:space:]]*python3 scripts/docs/check_integrity\.py$' \
  'stage the documentation'
require_job_pattern "$ci" 'docs' '^[[:space:]]*(- )?run: target/mdbook-bin/mdbook build target/docs-book$' \
  'build the mdBook'
require_job_pattern "$ci" 'docs' '^[[:space:]]*python3 scripts/docs/check_links\.py$' \
  'check generated internal links and anchors'
require_mdbook_cache "$ci" docs
docs_pages="$WORKFLOW_DIR/docs-pages.yml"
require_file "$docs_pages"
require_pinned_actions "$docs_pages"
require_mdbook_cache "$docs_pages" build
require_job_pattern "$ci" 'ci-ok' '^[[:space:]]*if:[[:space:]]*always\(\)' \
  'CI OK aggregate reports on every run'
require_job_pattern "$ci" 'ci-ok' 'toJSON\(needs\)' 'CI OK aggregate inspects every needed result'
require_ci_ok_covers_every_job "$ci"
require_lane_filter "$ci"
require_pr_trigger_only "$ci"
refuse_pattern "$ci" 'continue-on-error' \
  'continue-on-error, which lets a failing job or step read as green'
require_static_step_conditions "$ci"
# The lane machinery is itself tested on every PR, and the facade contract and Quickstart run in
# their own job, on the README lane. Nothing in pr-gate.yml executes .agents/gate.sh:
# release-gate.yml's `gate (full)` does, nightly and on release PRs (below).
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: bash scripts/ci/test-classify-changes\.sh$' \
  'run the lane classifier fixtures on every PR'
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: bash scripts/ci/test-proof-row\.sh$' \
  'run the local proof-row fixtures on every PR'
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: bash scripts/ci/test-gate-modes\.sh$' \
  'run the gate-mode nesting fixtures on every PR'
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: bash scripts/ci/test-install-wasmtime\.sh$' \
  'run the wasmtime installer destination fixtures on every PR'
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: python3 scripts/ci/test_ci_ok\.py$' \
  'run the CI OK result-check controls on every PR'
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: bash scripts/ci/test-leverage-members\.sh$' \
  'run the leverage dependency-rule fixtures on every PR'
# The no-secret scan's fixtures (issue #524): its private-key pattern matched nothing until they
# existed, and they are what fails if `-e` leaves its `git grep` again.
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: bash \.github/scripts/test-check-no-secrets\.sh$' \
  'run the no-secret scan fixtures on every PR'
require_job_pattern "$ci" 'leverage-lane' '^[[:space:]]*(- )?run: bash scripts/ci/leverage-members\.sh lane$' \
  'run the leverage lane'
require_job_pattern "$ci" 'facade-contract' '^[[:space:]]*(- )?run: python3 scripts/facade_contract/check\.py$' \
  'run the supported facade compiler contract'
require_job_pattern "$ci" 'facade-contract' '^[[:space:]]*(- )?run: python3 scripts/facade_contract/test_check\.py$' \
  'run the facade compiler diagnostic controls'
require_job_pattern "$ci" 'facade-contract' '^[[:space:]]*(- )?run: bash \.github/scripts/check-quickstart-runs\.sh$' \
  'execute the Quickstart'
# The README lane's own check (issue #503): the byte-pin of the README's Quickstart, named in the
# job so a README-only change runs it without the determinism cells that run the whole oce-api
# suite. `--no-tests=fail` and the report requirement keep a renamed or emptied test binary red.
require_job_pattern "$ci" 'facade-contract' 'tool:[[:space:]]*nextest@0\.9\.143' \
  'pinned cargo-nextest install for the README Quickstart byte-pin'
require_job_pattern "$ci" 'facade-contract' \
  '^[[:space:]]*(- )?run: cargo nextest run -p oce-api --test readme_quickstart --locked --profile ci --no-tests=fail$' \
  'run the README Quickstart byte-pin on the README lane'
require_job_pattern "$ci" 'facade-contract' '^[[:space:]]*(- )?run: test -s target/nextest/ci/junit\.xml$' \
  'the README lane refuses an absent or empty nextest report'
# Strict-bit cells and their fan-in.
require_job_pattern "$ci" 'strict-bit-matrix' \
  '^[[:space:]]*(- )?run: bash scripts/ci/strict-bits-cell\.sh "\$CELL_ARCH" "\$CELL_CODEGEN"$' \
  'native strict-bit cell for the matrix architecture and codegen'
require_job_pattern "$ci" 'strict-bit-matrix' 'CELL_ARCH: \$\{\{ matrix\.arch \}\}' \
  'strict-bit cell architecture comes from the matrix'
require_job_pattern "$ci" 'strict-bit-matrix' 'CELL_CODEGEN: \$\{\{ matrix\.codegen \}\}' \
  'strict-bit cell codegen comes from the matrix'
require_job_pattern "$ci" 'strict-bit-matrix' 'if-no-files-found: error' \
  'a strict-bit cell with no captures fails its upload'
require_job_needs "$ci" 'strict-bit-compare' 'strict-bit-matrix'
require_job_pattern "$ci" 'strict-bit-compare' "test\\(=matrix::native_linux_matrix_is_complete_and_exact\\)'" \
  'compare all eight strict-bit captures'
require_job_pattern "$ci" 'strict-bit-compare' '--no-tests=fail --run-ignored only' \
  'the eight-capture comparison hard-fails when its test is absent'
require_job_pattern "$ci" 'strict-bit-compare' 'OCE_STRICT_MATRIX_DIR: target/strict-bits' \
  'the eight-capture comparison reads the downloaded captures'
require_job_pattern "$ci" 'strict-bit-compare' 'OCE_STRICT_RUN_ID: \$\{\{ github\.run_id \}\}' \
  'the candidate receipt-log record names its run'
require_job_pattern "$ci" 'strict-bit-compare' 'test -s target/strict-bits/receipt-candidate\.json' \
  'a green comparison writes the candidate receipt-log record'
require_pattern "scripts/ci/strict-bits-cell.sh" 'uname -m' \
  'strict-bit cells refuse a host of the wrong architecture'

# ── merge-gate.yml: after each merge into `development` (issues #545 and #556) ───────────────
# Exactly the Windows cell, the one job no pull request runs, on every push to `development` and
# on dispatch, on the commit the event names, never cancelled in progress, under an aggregate
# that is not called `CI OK`. A determinism job back in this file fails the job set: those cells
# gate the pull request now, and a copy here would only repeat them on the tree the PR proved.
require_workflow_events "$merge" 'push workflow_dispatch'
require_event_branches "$merge" push '[development]' \
  'the merge gate must run on every merge into development, and only there'
require_pattern "$merge" '^  CHECKOUT_REF: \$\{\{ github\.sha \}\}$' \
  'a merge-gate run checks out the commit its event names in every job'
require_pinned_checkout_ref "$merge"
require_pattern "$merge" '^  group: \$\{\{ github\.workflow \}\}-\$\{\{ github\.event_name \}\}-\$\{\{ github\.ref \}\}$' \
  'one concurrency group per event and ref, so pushes to development queue behind each other'
require_pattern "$merge" '^  cancel-in-progress: false$' \
  'a merge-gate run in progress always finishes'
require_job_set "$merge" 'std-build-windows merge-gate-ok'
require_windows_cell "$merge"
require_job_pattern "$merge" 'merge-gate-ok' '^[[:space:]]*if:[[:space:]]*always\(\)' \
  'the merge-gate aggregate reports on every run'
require_job_pattern "$merge" 'merge-gate-ok' 'toJSON\(needs\)' \
  'the merge-gate aggregate inspects every needed result'
require_job_pattern "$merge" 'merge-gate-ok' '^    name: Merge gate OK$' \
  'the merge-gate aggregate is named Merge gate OK'
# `CI OK` is the required status on `main`, and a release PR's head is a commit this workflow has
# already reported on: a job of that name here could satisfy branch protection in place of the
# heavy set's.
refuse_pattern "$merge" '^[[:space:]]*name:[[:space:]]*["'"'"']?CI OK' \
  'a job named CI OK; that name is the required status of pr-gate.yml and release-gate.yml'
require_ci_ok_covers_every_job "$merge" merge-gate-ok 'Merge gate OK'
require_no_job_conditions "$merge" merge-gate-ok 'Merge gate OK'
refuse_pattern "$merge" 'continue-on-error' \
  'continue-on-error, which lets a failing job or step read as green'
require_static_step_conditions "$merge"

# ── release-gate.yml: the heavy set (issue #545) ──────────────────────────────────────────────
# Nightly on the development tip, on manual dispatch, on release PRs into `main` and on pushes to
# `main`. Not on a push to `development`: that is merge-gate.yml's.
require_workflow_events "$release" 'pull_request push schedule workflow_dispatch'
require_event_branches "$release" pull_request '[main]' \
  'the release PR into main must run the heavy set, and no other pull request may'
require_event_branches "$release" push '[main]' \
  'the heavy set runs on the merged release only; a merge into development runs merge-gate.yml'
require_schedule "$release" '17 8 * * *'
# A scheduled run checks out the development tip; a push run and a dispatched run check out the
# commit the event names, the same one in every job; a release PR checks out its merge ref. Every
# checkout takes that one pinned ref, and the concurrency group cancels only a superseded release-PR
# run.
require_pattern "$release" "^  CHECKOUT_REF: \\\$\\{\\{ github\\.event_name == 'schedule' && 'development' \\|\\| \\(github\\.event_name == 'push' \\|\\| github\\.event_name == 'workflow_dispatch'\\) && github\\.sha \\|\\| github\\.ref \\}\\}\$" \
  'a scheduled run checks out development, a push or dispatched run the commit its event names, a release PR its merge ref'
require_pinned_checkout_ref "$release"
require_pattern "$release" '^  group: \$\{\{ github\.workflow \}\}-\$\{\{ github\.event_name \}\}-\$\{\{ github\.ref \}\}$' \
  'one concurrency group per event and ref'
require_pattern "$release" "^  cancel-in-progress: \\\$\\{\\{ github\\.event_name == 'pull_request' \\}\\}\$" \
  'only a superseded release-PR run is cancelled; any other run in progress always finishes'
# The heavy set, by job id. Each job below is also held to the command that makes it that job.
require_job_set "$release" \
  'test-suite fmt file-size-cap no-secret-scan gate-fixtures unused-deps default-no-db golden-gen-firewall clippy build rustdoc deny gate determinism-matrix determinism-compare std-build-windows ci-ok'
require_pattern "$release" 'nextest@0\.9\.143' 'release gate pinned cargo-nextest 0.9.143 install'
# Scoped to the `test-suite` job: the determinism cells and the Windows cell in this file carry
# look-alike lines that must not stand in for the workspace suite's. The job runs the suite in
# debug codegen and the doctests; the release-codegen run is the `gate` job's (below).
require_job_pattern "$release" 'test-suite' \
  'cargo nextest run --workspace --locked --profile ci --no-tests=fail$' \
  'debug nextest with hard-fail-on-zero-tests'
require_job_pattern "$release" 'test-suite' 'rm -f target/nextest/ci/junit\.xml$' \
  'release gate clears the cached nextest JUnit report'
require_job_pattern "$release" 'test-suite' '^[[:space:]]*(- )?run: test -s target/nextest/ci/junit\.xml$' \
  'release gate refuses an absent or empty nextest report'
require_job_pattern "$release" 'test-suite' 'cargo test --workspace --doc --locked' 'doctest gate'
require_job_pattern "$release" 'fmt' '^[[:space:]]*(- )?run: cargo fmt --all --check$' 'release gate checks formatting'
require_job_pattern "$release" 'file-size-cap' '^[[:space:]]*(- )?run: bash \.github/scripts/check-file-size\.sh$' \
  'release gate runs the file-size cap'
require_job_pattern "$release" 'no-secret-scan' '^[[:space:]]*(- )?run: bash \.github/scripts/check-no-secrets\.sh$' \
  'release gate runs the no-secret scan'
require_job_pattern "$release" 'clippy' \
  '^[[:space:]]*(- )?run: cargo clippy --workspace --all-targets --locked -- -D warnings$' \
  'release gate lints the default feature set with warnings denied'
require_job_pattern "$release" 'build' '^[[:space:]]*(- )?run: cargo build --workspace --locked$' \
  'release gate builds the workspace'
require_job_pattern "$release" 'rustdoc' \
  '^[[:space:]]*(- )?run: cargo doc --no-deps --workspace --lib --document-private-items --locked$' \
  'release gate builds the library rustdoc'
require_job_pattern "$release" 'deny' '^[[:space:]]*(- )?run: cargo deny check bans licenses sources advisories$' \
  'release gate sweeps advisories with bans, licenses and sources'
refuse_pattern "$release" 'rustup[[:space:]]+toolchain[[:space:]]+install|nightly' \
  'release gate installs no nightly toolchain; the workspace builds on its pinned stable'
require_pattern "$release" 'cargo-machete' 'release gate installs cargo-machete'
require_pattern "$release" 'cargo machete' 'release gate runs cargo machete'
require_pattern "$release" 'check-default-no-db\.sh' 'release gate runs default-no-db smoke'
require_pattern "$release" 'test-check-default-no-db\.sh' 'release gate runs default-no-db fixture tests'
require_pattern "$release" 'test-check-golden-gen-anti-tautology\.sh' \
  'release gate runs golden-gen firewall fixture tests'
require_pattern "$release" 'check-golden-gen-anti-tautology\.sh' \
  'release gate runs golden-gen anti-tautology firewall'
require_pattern "$release" 'test-check-stale-crate-status\.sh' \
  'release gate runs stale crate-status fixture tests'
require_pattern "$release" 'check-stale-crate-status\.sh' 'release gate runs stale crate-status smoke'
require_job_pattern "$release" 'gate-fixtures' '^[[:space:]]*(- )?run: bash \.github/scripts/test-check-no-secrets\.sh$' \
  'release gate runs the no-secret scan fixtures'
require_job_pattern "$release" 'ci-ok' '^[[:space:]]*if:[[:space:]]*always\(\)' \
  'release gate CI OK aggregate reports on every run'
require_job_pattern "$release" 'ci-ok' 'toJSON\(needs\)' \
  'release gate CI OK aggregate inspects every needed result'
require_job_pattern "$release" 'ci-ok' '^    name: CI OK$' \
  'release gate aggregate is the CI OK status branch protection on main requires'
require_ci_ok_covers_every_job "$release"
require_no_job_conditions "$release"
refuse_pattern "$release" 'continue-on-error' \
  'continue-on-error, which lets a failing job or step read as green'
require_static_step_conditions "$release"
# The determinism matrix in all four native cells, with the cross-architecture and the cross-codegen
# halves of its fan-in, and the Windows cell (issue #502 moved the release-codegen cells, the
# debug-against-release comparisons and the Windows cell out of pr-gate.yml).
require_native_cell_matrix "$release" 'determinism-matrix' 'debug, release'
require_determinism_cells "$release"
require_release_codegen_determinism_cells "$release"
require_determinism_fan_in "$release"
require_cross_codegen_fan_in "$release"
require_windows_cell "$release"
# The Windows cell is one definition in the two workflows that run it (issue #545), and the
# determinism cells and their fan-in run the same steps per PR and nightly (issue #556).
require_identical_job "$merge" "$release" 'std-build-windows'
for shared in determinism-matrix determinism-compare; do
  require_same_job_commands "$ci" "$release" "$shared"
done
# The same workflow-level environment in every file that runs those jobs, and in the two that run
# the Windows cell. Only release-gate.yml and merge-gate.yml pin a checkout ref.
require_same_workflow_env "$ci" "$release" 'second'
require_same_workflow_env "$merge" "$release" 'first second'
# And that environment is pinned (issue #566), so a `RUSTFLAGS`, a `CARGO_PROFILE_*` override or a
# `NEXTEST_*` setting added to all three files at once cannot pass as three equal blocks. Below the
# workflow level require_build_env_untouched refuses them outright.
pinned_workflow_env="$(cat <<'EOF'
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: short
  CARGO_NET_GIT_FETCH_WITH_CLI: "true"
  CARGO_INCREMENTAL: "0"
  CARGO_PROFILE_DEV_DEBUG: "0"
  CARGO_PROFILE_TEST_DEBUG: "0"
EOF
)"
if [ "$(workflow_env "$ci")" != "$pinned_workflow_env" ]; then
  echo "FAIL: $ci workflow-level env is not exactly its pinned lines"
  diff <(printf '%s\n' "$pinned_workflow_env") <(workflow_env "$ci") | sed 's/^/  /' || true
  exit 1
fi
# The release gate's side of the `save-if` exception (the per-PR side is pinned with pr-gate.yml's
# cells above): only its aarch64 cells save, because `test-suite` writes the x86_64 entry.
require_job_pattern "$release" 'determinism-matrix' "^          save-if: \\\$\\{\\{ matrix\\.arch == 'aarch64' \\}\\}\$" \
  'only the aarch64 determinism cells of the release gate save the shared rust-cache entry'
for workflow in "$ci" "$merge" "$release"; do
  refuse_failure_masks "$workflow"
  refuse_masked_step_status "$workflow"
  require_live_matrix_selectors "$workflow"
  require_build_env_untouched "$workflow"
done
require_job_pattern "$release" 'gate' '^[[:space:]]*(- )?run: bash \.agents/gate\.sh full$' \
  'the release gate executes the gate script in full mode'
require_job_order "$release" 'gate' "$no_std_targets" 'run: bash .agents/gate.sh full' \
  'the executed full gate script has the no_std targets it builds for'
# `gate.sh full` runs the blob round trip's wasm32 leg and fails closed without its runtime. The
# installer takes no destination here: its default is the directory gate.sh looks in.
require_job_order "$release" 'gate' 'rustup target add wasm32-wasip1' 'run: bash .agents/gate.sh full' \
  'the executed full gate script has the wasm32-wasip1 target its round trip runs on'
require_job_pattern "$release" 'gate' '^[[:space:]]*bash scripts/ci/install-wasmtime\.sh$' \
  'the executed full gate script has the pinned wasmtime its round trip runs under'
require_job_order "$release" 'gate' 'bash scripts/ci/install-wasmtime.sh' \
  'run: bash .agents/gate.sh full' \
  'the executed full gate script has the pinned wasmtime its round trip runs under'
# The workspace suite under release codegen runs in this job only (issue #545: `test-suite` and
# `gate (full)` each built the release workspace), and only through the executed script, under a
# nextest profile that nothing else uses. The profile is what makes the report requirement mean
# something: `full` mode also runs the determinism subset and the strict-bit step under release
# codegen, which write the `ci-release` report, so a requirement on that file is met with the
# workspace run removed. The job clears the `ci-release-workspace` report, executes the script,
# then requires the report. A report says the command started and nextest wrote one; the script's
# exit status is what says the suite passed.
require_pattern "$NEXTEST_CONFIG" '^\[profile\.ci-release-workspace\]$' \
  'a nextest profile of its own for the release-codegen workspace run'
require_pattern "$NEXTEST_CONFIG" '^\[profile\.ci-release-workspace\.junit\]$' \
  'a JUnit report of its own for the release-codegen workspace run'
# The profile's body is pinned to the line (issue #555). A `default-filter`, a test-group override
# or any other key added to it changes which tests the one release-codegen workspace run executes
# while the profile, the command and the report requirement all stay in place: with
# `default-filter = "package(oce-diag)"` the nightly would run one crate and stay green.
require_toml_table "$NEXTEST_CONFIG" 'profile.ci-release-workspace' 'inherits = "ci"' \
  'the release-codegen workspace profile only inherits ci'
require_toml_table "$NEXTEST_CONFIG" 'profile.ci-release-workspace.junit' \
  'report-name = "open-control-ci-release-workspace"' \
  'the release-codegen workspace profile only names its report'
# And the whole file is pinned to the line (issue #566). `ci` and `ci-release` are the profiles
# every other gate command runs under, and `default` is what they inherit: a `default-filter`, a
# `retries` above 0, an override table, a setup script or a test group added to any of them
# narrows or softens every per-PR suite while each command and report stays in place. Comment lines
# and blank lines may change; a change to a key, or a comment after one, is a change to this pin.
pinned_nextest_config="$(cat <<'EOF'
nextest-version = { required = "0.9.143", recommended = "0.9.143" }
[profile.default]
fail-fast = true
retries = 0
flaky-result = "fail"
slow-timeout = { period = "60s", terminate-after = 2, on-timeout = "fail" }
leak-timeout = { period = "2s", result = "fail" }
global-timeout = "15m"
[profile.ci]
fail-fast = false
failure-output = "immediate-final"
success-output = "never"
status-level = "fail"
final-status-level = "slow"
[profile.ci.junit]
path = "junit.xml"
report-name = "open-control-ci"
store-success-output = false
store-failure-output = true
report-skipped = "ignored"
flaky-fail-status = "failure"
[profile.ci-release]
inherits = "ci"
[profile.ci-release.junit]
report-name = "open-control-ci-release"
[profile.ci-release-workspace]
inherits = "ci"
[profile.ci-release-workspace.junit]
report-name = "open-control-ci-release-workspace"
EOF
)"
require_file_lines "$NEXTEST_CONFIG" "$pinned_nextest_config" \
  'the nextest profiles every gate command runs under narrow and soften nothing'
require_job_order "$release" 'gate' 'run: rm -f target/nextest/ci-release-workspace/junit.xml' \
  'run: bash .agents/gate.sh full' \
  'the release-codegen workspace report is cleared before the gate script runs'
require_job_order "$release" 'gate' 'run: bash .agents/gate.sh full' \
  'run: test -s target/nextest/ci-release-workspace/junit.xml' \
  'the gate job requires the release-codegen workspace suite report after the script'
require_gate_full_block "$GATE_SCRIPT"
# Exactly one command anywhere may write that report: the script's workspace run. A second user in
# the script, or any in a workflow other than the gate job's clear and requirement, would let the
# report exist without that run.
if [ "$(count_code_lines "$GATE_SCRIPT" 'ci-release-workspace')" != 1 ]; then
  echo "FAIL: $GATE_SCRIPT names the nextest profile ci-release-workspace on other than exactly one line; only the release-codegen workspace run may use it"
  exit 1
fi
if [ "$(count_code_lines "$release" 'ci-release-workspace')" != 2 ]; then
  echo "FAIL: $release names ci-release-workspace outside the gate job's report clear and requirement"
  exit 1
fi
for other in "$ci" "$merge"; do
  if [ "$(count_code_lines "$other" 'ci-release-workspace')" != 0 ]; then
    echo "FAIL: $other names the nextest profile ci-release-workspace; only the release-codegen workspace run in $GATE_SCRIPT may use it"
    exit 1
  fi
done

# The emulation check (issue #402): nightly and on dispatch, never on a pull request or a push (it
# is not part of CI OK), its actions pinned, and it runs the emulation script.
qemu="$WORKFLOW_DIR/qemu.yml"
require_file "$qemu"
require_pinned_actions "$qemu"
require_pattern "$qemu" '^[[:space:]]*schedule:' 'scheduled emulation run'
require_pattern "$qemu" '^[[:space:]]*- cron:' 'emulation cron entry'
require_pattern "$qemu" '^[[:space:]]*workflow_dispatch:' 'manual emulation run'
require_pattern "$qemu" '^[[:space:]]*(- )?run: bash scripts/ci/emulate-firmware\.sh$' \
  'the emulation run executes emulate-firmware.sh'
refuse_pattern "$qemu" '^on:[[:space:]]*[^[:space:]#]|^[[:space:]]*(pull_request|pull_request_target|push|merge_group):' \
  'a pull_request or push trigger, or a one-line on:; the emulation run is nightly and non-gating'

# Daily advisory/yanked gate and deny.toml discipline.
require_pattern "$advisories" 'schedule:' 'scheduled advisory gate'
require_pattern "$advisories" 'workflow_dispatch:' 'manual advisory gate'
require_pattern "$advisories" 'cargo deny check advisories' 'advisory-only cargo-deny command'
require_pattern "$DENY_TOML" '^[[:space:]]*yanked[[:space:]]*=[[:space:]]*"deny"' 'yanked = "deny"'
require_pattern "$DENY_TOML" '^[[:space:]]*ignore[[:space:]]*=[[:space:]]*\[\][[:space:]]*$' 'empty advisory ignore list'
# Non-empty sentinel only: this proves representative sqlx/sled bans remain, not that the curated
# family list is complete. A list gutted to only these two entries would still pass by design.
require_pattern "$DENY_TOML" '^[[:space:]]*\{[[:space:]]*name[[:space:]]*=[[:space:]]*"sqlx"[[:space:]]*\}' \
  'cargo-deny bans include representative SQL/ORM crate sqlx'
require_pattern "$DENY_TOML" '^[[:space:]]*\{[[:space:]]*name[[:space:]]*=[[:space:]]*"sled"[[:space:]]*\}' \
  'cargo-deny bans include representative embedded-KV crate sled'

# The offline tool workspaces (issue #610), last, so each job-specific rule above reports first:
# every workflow but the frozen ci.yml, so a job added anywhere is held to it, and the gate script.
for workflow in "$WORKFLOW_DIR"/*.yml; do
  [ "$(basename "$workflow")" = ci.yml ] && continue
  require_offline_tool_prefetch "$workflow"
done
require_gate_script_prefetch

echo "OK: workflow gate smoke passed."
