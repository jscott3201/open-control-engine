#!/usr/bin/env bash
# Static smoke checks for CI gate topology. This catches false-green workflow edits before GitHub
# Actions gets a chance to silently skip the heavy test suite or dependency gates.
#
# The live per-PR gate is .github/workflows/pr-gate.yml. .github/workflows/ci.yml is a dormant,
# byte-frozen copy kept as the history of the retained strict-bit capture archives, disabled in the
# repository's Actions settings, and is not checked here.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

WORKFLOW_DIR="${OCE_WORKFLOW_DIR:-.github/workflows}"
DENY_TOML="${OCE_DENY_TOML:-deny.toml}"
ROOT_CARGO_TOML="${OCE_ROOT_CARGO_TOML:-Cargo.toml}"
CRATES_DIR="${OCE_CRATES_DIR:-crates}"

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
  job_body="$(awk -v header="  $job:" '
    $0 == header { found = 1; next }
    found && /^  [^ ]/ { exit }
    found { print }
  ' "$path")"
  if ! grep -Eq -- "$pattern" <<< "$job_body"; then
    echo "FAIL: $path job $job is missing required gate pattern: $label"
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

# A native cell matrix: every (runner, codegen) pair, x86_64 on ubuntu-latest and aarch64 on
# ubuntu-24.04-arm, without fail-fast, so one run reports every failing cell.
require_native_cell_matrix() {
  path="$1"
  job="$2"
  require_job_pattern "$path" "$job" '^    runs-on:[[:space:]]*\$\{\{[[:space:]]*matrix\.runner[[:space:]]*\}\}[[:space:]]*$' \
    "$job runs on the matrix runner"
  require_job_pattern "$path" "$job" '^      fail-fast:[[:space:]]*false[[:space:]]*$' \
    "$job reports every failing cell (fail-fast: false)"
  require_job_pattern "$path" "$job" '^        runner:[[:space:]]*\[ubuntu-latest,[[:space:]]*ubuntu-24\.04-arm\][[:space:]]*$' \
    "$job runs native x86_64 and aarch64 runners"
  require_job_pattern "$path" "$job" '^        codegen:[[:space:]]*\[debug,[[:space:]]*release\][[:space:]]*$' \
    "$job runs debug and release codegen"
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

# Every job-level `if:` other than CI OK's, as `job<TAB>condition` lines.
job_conditions() {
  awk '
    /^jobs:[[:space:]]*$/ { in_jobs = 1; next }
    in_jobs && /^[^[:space:]#]/ { in_jobs = 0 }
    in_jobs && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { job = $0; sub(/^  /, "", job); sub(/:.*/, "", job) }
    in_jobs && job != "ci-ok" && /^    if:/ {
      value = $0
      sub(/^    if:[[:space:]]*/, "", value)
      sub(/[[:space:]]+$/, "", value)
      print job "\t" value
    }
  ' "$1"
}

# The aggregate job must need every other job: one missing from `needs:` would not be gated at all.
require_ci_ok_covers_every_job() {
  path="$1"
  local expected actual
  expected="$(workflow_jobs "$path" | grep -vx 'ci-ok' || true)"
  actual="$(job_needs "$path" ci-ok)"
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    echo "FAIL: $path CI OK needs does not match the workflow's jobs"
    diff <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") | sed 's/^/  /' || true
    exit 1
  fi
}

# No job other than CI OK may carry a job-level `if:` (release-gate.yml): its aggregate accepts
# only `success`, so a conditionally skipped gate would read as a failure.
require_no_job_conditions() {
  path="$1"
  local gated
  gated="$(job_conditions "$path" | cut -f1)"
  if [ -n "$gated" ]; then
    echo "FAIL: $path gating job has a job-level if: that CI OK would read as failure: $gated"
    exit 1
  fi
}

# The per-PR lane filter (issue #393). A job-level `if:` is allowed only in the exact form
# `needs.changes.outputs.<lane> == 'true'` for lane core, leverage or gate_scripts, on a job that
# needs `changes`. CI OK's JOB_LANES must list exactly those jobs with those lanes, because it is
# what lets ci_ok.py accept their `skipped` result, and CI OK must run that result check. The
# `changes` job must export all four outputs from the classifier. Named jobs are pinned to their
# lane, so a determinism job cannot drift onto a lighter lane or an always-on job become optional.
require_lane_filter() {
  path="$1"
  local conditions expected actual job condition lane want got
  conditions="$(job_conditions "$path")"
  expected=""
  while IFS=$'\t' read -r job condition; do
    [ -n "$job" ] || continue
    if [[ "$condition" =~ ^needs\.changes\.outputs\.(core|leverage|gate_scripts)\ ==\ \'true\'$ ]]; then
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
    rustdoc=core deny=core unused-deps=core default-no-db=core facade-contract=core \
    leverage-lane=leverage gate=gate_scripts \
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
  for lane in core leverage gate_scripts docs_only; do
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
}

ci="$WORKFLOW_DIR/pr-gate.yml"
release="$WORKFLOW_DIR/release-gate.yml"
advisories="$WORKFLOW_DIR/advisories.yml"

require_file "$ci"
require_file "$release"
require_file "$advisories"
require_file "$DENY_TOML"
require_unsafe_forbid_layers
require_pinned_actions "$ci"
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
refuse_emulation "$release"
require_native_cell_matrix "$ci" 'determinism-matrix'
require_native_cell_matrix "$ci" 'strict-bit-matrix'
require_pattern "$ci" 'nextest@0\.9\.143' 'pinned cargo-nextest 0.9.143 install'
require_job_pattern "$ci" 'determinism-matrix' 'test "\$\(uname -m\)" = "\$CELL_ARCH"' \
  'determinism cells refuse a runner of the wrong architecture'
require_job_pattern "$ci" 'determinism-matrix' \
  'cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail' \
  'debug determinism subset with hard-fail-on-zero-tests'
require_job_pattern "$ci" 'determinism-matrix' \
  'cargo nextest run -p oce-api -p oce-blocks -p oce-expr --locked --profile ci-release --cargo-profile release --no-tests=fail' \
  'release determinism subset with hard-fail-on-zero-tests'
require_pattern "$ci" 'OCE_PORTABLE_STATE_OUT: target/portable-state-debug\.bin' \
  'emit the debug portable state vector in every determinism cell'
require_pattern "$ci" 'OCE_PORTABLE_STATE_OUT: target/portable-state-release\.bin' \
  'emit the release portable state vector in every determinism cell'
require_pattern "$ci" 'OCE_TARGET_STATE_OUT: target/target-state-debug\.bin' \
  'emit the debug target-bound state vector in every determinism cell'
require_pattern "$ci" 'OCE_TARGET_STATE_OUT: target/target-state-release\.bin' \
  'emit the release target-bound state vector in every determinism cell'
require_job_pattern "$ci" 'determinism-matrix' 'tool:[[:space:]]*nextest@0\.9\.143' \
  'pinned cargo-nextest install for the determinism cells'
require_job_pattern "$ci" 'determinism-matrix' 'test -s "target/nextest/\$profile/junit\.xml"' \
  'refuse an absent or empty nextest determinism report'
require_job_pattern "$ci" 'determinism-matrix' 'test -s "target/portable-state-\$CELL_CODEGEN\.bin"' \
  'refuse a determinism cell that emitted no portable state vector'
require_job_pattern "$ci" 'determinism-matrix' 'name: determinism-\$\{\{ matrix\.arch \}\}-\$\{\{ matrix\.codegen \}\}' \
  'upload each determinism cell under its own name'
require_job_pattern "$ci" 'determinism-matrix' 'if-no-files-found: error' \
  'a determinism cell with no vectors fails its upload'
require_pattern "$ci" 'target/nextest/\{ci,ci-release\}/junit\.xml' \
  'clear cached nextest JUnit reports'
require_pattern "$ci" 'target/\{portable,target\}-state-\{debug,release\}\.bin' \
  'clear cached state vectors before the determinism matrix'
# Determinism fan-in: byte comparisons over the four cells and the foreign-snapshot refusal.
require_job_needs "$ci" 'determinism-compare' 'determinism-matrix'
require_job_pattern "$ci" 'determinism-compare' \
  'cmp determinism-x86_64-debug/portable-state-debug\.bin determinism-x86_64-release/portable-state-release\.bin' \
  'compare x86_64 portable state bytes across codegen profiles'
require_job_pattern "$ci" 'determinism-compare' \
  'cmp determinism-x86_64-debug/target-state-debug\.bin determinism-x86_64-release/target-state-release\.bin' \
  'compare x86_64 target-bound state bytes across codegen profiles'
require_job_pattern "$ci" 'determinism-compare' \
  'cmp determinism-aarch64-debug/portable-state-debug\.bin determinism-aarch64-release/portable-state-release\.bin' \
  'compare aarch64 portable state bytes across codegen profiles'
require_job_pattern "$ci" 'determinism-compare' \
  'cmp determinism-aarch64-debug/target-state-debug\.bin determinism-aarch64-release/target-state-release\.bin' \
  'compare aarch64 target-bound state bytes across codegen profiles'
require_job_pattern "$ci" 'determinism-compare' \
  'cmp determinism-x86_64-debug/portable-state-debug\.bin determinism-aarch64-debug/portable-state-debug\.bin' \
  'compare portable state bytes across architectures'
require_job_pattern "$ci" 'determinism-compare' \
  '![[:space:]]*cmp -s determinism-x86_64-debug/target-state-debug\.bin determinism-aarch64-debug/target-state-debug\.bin' \
  'require target-bound state bytes to differ across architectures'
require_job_pattern "$ci" 'determinism-compare' \
  'export OCE_FOREIGN_TARGET_STATE_IN=.*determinism-\$FOREIGN_ARCH-debug/target-state-debug\.bin' \
  'supply the other architecture target-bound snapshot to the restore test'
require_job_pattern "$ci" 'determinism-compare' 'test "\$CELL_ARCH" != "\$FOREIGN_ARCH"' \
  'the restoring architecture differs from the snapshot architecture'
require_job_pattern "$ci" 'determinism-compare' \
  "test\\(=tests::state_portability_tests::foreign_matrix_target_snapshot_refuses_restore_when_supplied\\)'" \
  'restore and refuse the foreign target-bound snapshot'
require_job_pattern "$ci" 'determinism-compare' '--profile ci --no-tests=fail' \
  'foreign target-bound restore hard-fails when its test is absent'
require_job_pattern "$ci" 'determinism-compare' 'tool:[[:space:]]*nextest@0\.9\.143' \
  'pinned cargo-nextest install for foreign restore'
require_job_pattern "$ci" 'determinism-compare' 'arch: x86_64' 'x86_64 restores the aarch64 snapshot'
require_job_pattern "$ci" 'determinism-compare' 'arch: aarch64' 'aarch64 restores the x86_64 snapshot'
# Per-PR crate suites outside the determinism subset (issue #370).
require_job_pattern "$ci" 'crate-tests' 'tool:[[:space:]]*nextest@0\.9\.143' \
  'pinned cargo-nextest install for the crate suites'
require_job_pattern "$ci" 'crate-tests' \
  '^[[:space:]]*cargo nextest run -p oce-graph -p oce-validate -p oce-model -p oce-cxf --locked --profile ci --no-tests=fail$' \
  'run the oce-graph, oce-validate, oce-model and oce-cxf suites per PR and hard-fail on zero tests'
require_job_pattern "$ci" 'crate-tests' '^[[:space:]]*(- )?run: test -s target/nextest/ci/junit\.xml$' \
  'refuse an absent or empty crate-suite report'
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
require_job_pattern "$ci" 'ci-ok' '^[[:space:]]*if:[[:space:]]*always\(\)' \
  'CI OK aggregate reports on every run'
require_job_pattern "$ci" 'ci-ok' 'toJSON\(needs\)' 'CI OK aggregate inspects every needed result'
require_ci_ok_covers_every_job "$ci"
require_lane_filter "$ci"
require_pr_trigger_only "$ci"
# The lane machinery is itself tested on every PR, and the moved gate (light) steps still run.
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: bash scripts/ci/test-classify-changes\.sh$' \
  'run the lane classifier fixtures on every PR'
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: python3 scripts/ci/test_ci_ok\.py$' \
  'run the CI OK result-check controls on every PR'
require_job_pattern "$ci" 'gate-fixtures' '^[[:space:]]*(- )?run: bash scripts/ci/test-leverage-members\.sh$' \
  'run the leverage dependency-rule fixtures on every PR'
require_job_pattern "$ci" 'leverage-lane' '^[[:space:]]*(- )?run: bash scripts/ci/leverage-members\.sh lane$' \
  'run the leverage lane'
require_job_pattern "$ci" 'facade-contract' '^[[:space:]]*(- )?run: python3 scripts/facade_contract/check\.py$' \
  'run the supported facade compiler contract'
require_job_pattern "$ci" 'facade-contract' '^[[:space:]]*(- )?run: python3 scripts/facade_contract/test_check\.py$' \
  'run the facade compiler diagnostic controls'
require_job_pattern "$ci" 'facade-contract' '^[[:space:]]*(- )?run: bash \.github/scripts/check-quickstart-runs\.sh$' \
  'execute the Quickstart'
require_job_pattern "$ci" 'gate' '^[[:space:]]*(- )?run: bash \.agents/gate\.sh$' \
  'gate (light) executes the gate script'
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

# Heavy gate runs on release PRs, manual dispatch, and scheduled development-tip checks.
require_pattern "$release" 'schedule:' 'scheduled heavy gate'
require_pattern "$release" 'cron:' 'cron entry'
require_pattern "$release" 'workflow_dispatch:' 'manual heavy gate'
require_pattern "$release" '(CHECKOUT_REF:.*development|ref:.*development)' 'scheduled checkout targets development tip'
require_pattern "$release" 'ref:.*(CHECKOUT_REF|development)' 'checkout uses the scheduled development ref'
require_pattern "$release" 'nextest@0\.9\.143' 'release gate pinned cargo-nextest 0.9.143 install'
require_pattern "$release" 'cargo nextest run --workspace --locked --profile ci --no-tests=fail' \
  'debug nextest with hard-fail-on-zero-tests'
require_pattern "$release" 'cargo nextest run --workspace --locked --profile ci-release --cargo-profile release --no-tests=fail' \
  'release-codegen nextest with hard-fail-on-zero-tests'
require_pattern "$release" 'target/nextest/\{ci,ci-release\}/junit\.xml' \
  'release gate clears cached nextest JUnit reports'
require_pattern "$release" 'for profile in ci ci-release;' \
  'release gate requires the complete nextest report set'
require_pattern "$release" 'test -s "target/nextest/\$profile/junit\.xml"' \
  'release gate refuses an absent or empty nextest report'
require_pattern "$release" 'cargo test --workspace --doc --locked' 'doctest gate'
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
require_job_pattern "$release" 'ci-ok' '^[[:space:]]*if:[[:space:]]*always\(\)' \
  'release gate CI OK aggregate reports on every run'
require_job_pattern "$release" 'ci-ok' 'toJSON\(needs\)' \
  'release gate CI OK aggregate inspects every needed result'
require_ci_ok_covers_every_job "$release"
require_no_job_conditions "$release"

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

echo "OK: workflow gate smoke passed."
