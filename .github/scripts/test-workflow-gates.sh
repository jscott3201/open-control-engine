#!/usr/bin/env bash
# Gate-behavior fixtures for check-workflow-gates.sh. The negative fixtures prove missing nextest,
# seeded advisory ignores, and garbled workflow input fail closed.

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

SCRIPT=".github/scripts/check-workflow-gates.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# A faithful stand-in for .agents/gate.sh: real `step`/`step_env` functions, real `list` mode,
# and every command the checker pins. Two invocations are deliberately split with a trailing
# backslash, as the real script writes them — `list` must report the shell-joined argv, so a
# fixture that never split a line would not prove that.
write_gate_fixture() {
  cat > "$1" <<'GATEEOF'
#!/usr/bin/env bash
set -euo pipefail
MODE="${1:-light}"
failures=0
# Mirrors the real gate's contract: a failing step never aborts the run, it accumulates.
step() {
  local label="$1"; shift
  if [ "$MODE" = list ]; then printf 'CMD %s\n' "$*"; return 0; fi
  if "$@"; then :; else failures=$((failures + 1)); fi
}
step_env() {
  local label="$1" var="$2" val="$3"; shift 3
  if [ "$MODE" = list ]; then printf 'CMD %s=%s %s\n' "$var" "$val" "$*"; return 0; fi
  if env "$var=$val" "$@"; then :; else failures=$((failures + 1)); fi
}
step 'fmt' cargo fmt --all --check
step 'file-size' bash .github/scripts/check-file-size.sh
step 'no-secret' bash .github/scripts/check-no-secrets.sh
step 'no-db' bash .github/scripts/check-default-no-db.sh
step 'golden-gen' bash .github/scripts/check-golden-gen-anti-tautology.sh
step 'no-db fixtures' bash .github/scripts/test-check-default-no-db.sh
step 'golden-gen fixtures' bash .github/scripts/test-check-golden-gen-anti-tautology.sh
step 'crate-status fixtures' bash .github/scripts/test-check-stale-crate-status.sh
step 'crate-status smoke' bash .github/scripts/check-stale-crate-status.sh
step 'workflow fixtures' bash .github/scripts/test-workflow-gates.sh
step 'workflow smoke' bash .github/scripts/check-workflow-gates.sh
step 'machete' cargo machete
step 'clippy' cargo clippy --workspace --all-targets --locked -- -D warnings
step 'build' cargo build --workspace --locked
step_env 'rustdoc libs' RUSTDOCFLAGS '-D warnings' \
  cargo doc --no-deps --workspace --lib --document-private-items --locked
step_env 'rustdoc bins' RUSTDOCFLAGS '-D warnings' \
  cargo doc --no-deps --workspace --bins --document-private-items --locked
step 'deny' cargo deny check bans licenses sources
step 'determinism' \
  cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
step 'determinism release' \
  cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci \
  --cargo-profile release --no-tests=fail
step 'workspace' cargo nextest run --workspace --locked --profile ci --no-tests=fail
step 'workspace release' \
  cargo nextest run --workspace --locked --profile ci --cargo-profile release --no-tests=fail
step 'doctests' cargo test --workspace --doc --locked
if [ "$MODE" = list ]; then exit 0; fi
if [ "$failures" -gt 0 ]; then
  echo "GATE FAILED — $failures step(s)"
  exit 1
fi
echo "GATE PASSED"
GATEEOF
}

write_positive() {
  dir="$1"
  deny="$2"
  root_cargo="$3"
  crates_dir="$4"
  gate_script="$5"
  mkdir -p "$dir"
  write_gate_fixture "$gate_script"
  cat > "$dir/ci.yml" <<'EOF'
on:
  pull_request:
    branches: [development]
jobs:
  fmt:
    if: github.event.pull_request.draft == false
    steps:
      - run: cargo fmt --all --check
  file-size-cap:
    steps:
      - run: bash .github/scripts/check-file-size.sh
  no-secret-scan:
    steps:
      - run: bash .github/scripts/check-no-secrets.sh
  clippy:
    steps:
      - run: cargo clippy --workspace --all-targets --locked -- -D warnings
  build:
    steps:
      - run: cargo build --workspace --locked
  rustdoc:
    env:
      RUSTDOCFLAGS: "-D warnings"
    steps:
      - run: cargo doc --no-deps --workspace --lib --document-private-items --locked
      - run: cargo doc --no-deps --workspace --bins --document-private-items --locked
  default-no-db:
    steps:
      - run: bash .github/scripts/check-default-no-db.sh
  unused-deps:
    steps:
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-machete
      - run: cargo machete
  gate-fixtures:
    steps:
      - run: bash .github/scripts/test-check-default-no-db.sh
      - run: bash .github/scripts/test-check-golden-gen-anti-tautology.sh
      - run: bash .github/scripts/test-check-stale-crate-status.sh
      - run: bash .github/scripts/check-stale-crate-status.sh
      - run: bash .github/scripts/check-workflow-gates.sh
  golden-gen-firewall:
    steps:
      - run: bash .github/scripts/check-golden-gen-anti-tautology.sh
  determinism-matrix:
    strategy:
      matrix:
        runner: [ubuntu-latest, ubuntu-24.04-arm]
    runs-on: ${{ matrix.runner }}
    steps:
      - run: cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail
      - run: cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --cargo-profile release --no-tests=fail
EOF
  cat > "$dir/release-gate.yml" <<'EOF'
on:
  schedule:
    - cron: "17 8 * * *"
  workflow_dispatch:
jobs:
  test-suite:
    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ github.event_name == 'schedule' && 'development' || github.ref }}
      - run: cargo nextest run --workspace --locked --profile ci --no-tests=fail
      - run: cargo nextest run --workspace --locked --profile ci --cargo-profile release --no-tests=fail
      - run: cargo test --workspace --doc --locked
      - env:
          OCE_REQUIRE_SURFACE_CHECK: "1"
        run: cargo nextest run -p oce-api -E 'test(public_api_surface_matches_blessed_baseline)' --profile public-api --locked --no-tests=fail
      - name: cargo public-api surface gate (oce-store)
        env:
          OCE_REQUIRE_SURFACE_CHECK: "1"
        run: cargo nextest run -p oce-store -E 'test(public_api_surface_matches_blessed_baseline)' --profile public-api --locked --no-tests=fail
  default-no-db:
    steps:
      - run: bash .github/scripts/check-default-no-db.sh
  unused-deps:
    steps:
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-machete
      - run: cargo machete
  gate-fixtures:
    steps:
      - run: bash .github/scripts/test-check-default-no-db.sh
      - run: bash .github/scripts/test-check-golden-gen-anti-tautology.sh
      - run: bash .github/scripts/test-check-stale-crate-status.sh
      - run: bash .github/scripts/check-stale-crate-status.sh
  golden-gen-firewall:
    steps:
      - run: bash .github/scripts/check-golden-gen-anti-tautology.sh
EOF
  cat > "$dir/advisories.yml" <<'EOF'
on:
  schedule:
    - cron: "43 8 * * *"
  workflow_dispatch:
jobs:
  advisories:
    steps:
      - run: cargo deny check advisories
EOF
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = []

[bans]
deny = [
  { name = "sqlx" },
  { name = "sled" },
]
EOF
  cat > "$root_cargo" <<'EOF'
[workspace.lints.rust]
unsafe_code = "forbid"
EOF
  mkdir -p "$crates_dir/oce-api/src" "$crates_dir/oce-store/src"
  cat > "$crates_dir/oce-api/Cargo.toml" <<'EOF'
[package]
name = "oce-api"

[lints]
workspace = true
EOF
  cat > "$crates_dir/oce-store/Cargo.toml" <<'EOF'
[package]
name = "oce-store"

[lints]
workspace = true
EOF
  cat > "$crates_dir/oce-api/src/lib.rs" <<'EOF'
#![forbid(unsafe_code)]
//! Fixture crate.
EOF
  cat > "$crates_dir/oce-store/src/lib.rs" <<'EOF'
#![forbid(unsafe_code)]
//! Fixture crate.
EOF
}

run_case() {
  name="$1"
  expected="$2"
  mutate="$3"
  expected_message="${4:-}"
  dir="$tmp/$name/workflows"
  deny="$tmp/$name/deny.toml"
  root_cargo="$tmp/$name/Cargo.toml"
  crates_dir="$tmp/$name/crates"
  gate_script="$tmp/$name/gate.sh"
  mkdir -p "$tmp/$name"
  write_positive "$dir" "$deny" "$root_cargo" "$crates_dir" "$gate_script"
  # The 5th argument is new; mutators that predate it simply ignore it.
  "$mutate" "$dir" "$deny" "$root_cargo" "$crates_dir" "$gate_script"

  set +e
  output="$(
    OCE_WORKFLOW_DIR="$dir" \
    OCE_DENY_TOML="$deny" \
    OCE_ROOT_CARGO_TOML="$root_cargo" \
    OCE_CRATES_DIR="$crates_dir" \
    OCE_GATE_SCRIPT="$gate_script" \
    bash "$SCRIPT" 2>&1
  )"
  status=$?
  set -e

  case "$expected" in
    pass)
      if [ "$status" -ne 0 ]; then
        echo "FAIL: workflow fixture '$name' should pass but exited $status"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^OK:'; then
        echo "FAIL: workflow fixture '$name' passed without the OK line"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    fail)
      if [ "$status" -eq 0 ]; then
        echo "FAIL: workflow fixture '$name' should fail but exited 0"
        printf '%s\n' "$output"
        exit 1
      fi
      if ! printf '%s\n' "$output" | grep -q '^FAIL:'; then
        echo "FAIL: workflow fixture '$name' failed without a FAIL line"
        printf '%s\n' "$output"
        exit 1
      fi
      if [ -n "$expected_message" ] && ! printf '%s\n' "$output" | grep -Fq -- "$expected_message"; then
        echo "FAIL: workflow fixture '$name' failed without expected message substring: $expected_message"
        printf '%s\n' "$output"
        exit 1
      fi
      ;;
    *)
      echo "BUG: unknown expectation '$expected'"
      exit 1
      ;;
  esac
}

noop() {
  _dir="$1"
  _deny="$2"
}

remove_release_nextest() {
  dir="$1"
  _deny="$2"
  grep -v -- '--cargo-profile release' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

seed_advisory_ignore() {
  _dir="$1"
  deny="$2"
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = ["RUSTSEC-2099-0001"]

[bans]
deny = [
  { name = "sqlx" },
  { name = "sled" },
]
EOF
}

empty_bans_deny() {
  _dir="$1"
  deny="$2"
  cat > "$deny" <<'EOF'
[advisories]
yanked = "deny"
ignore = []

[bans]
deny = []
EOF
}

garble_release_workflow() {
  dir="$1"
  _deny="$2"
  printf '%s\n' 'not: [a usable gate]' > "$dir/release-gate.yml"
}

remove_stale_status_gate() {
  dir="$1"
  _deny="$2"
  grep -v 'check-stale-crate-status' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_store_surface_gate() {
  dir="$1"
  _deny="$2"
  grep -v -- '-p oce-store' "$dir/release-gate.yml" > "$dir/release-gate.yml.tmp"
  mv "$dir/release-gate.yml.tmp" "$dir/release-gate.yml"
}

remove_golden_gen_firewall() {
  dir="$1"
  _deny="$2"
  grep -v 'check-golden-gen-anti-tautology' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_root_unsafe_forbid() {
  _dir="$1"
  _deny="$2"
  root_cargo="$3"
  cat > "$root_cargo" <<'EOF'
[workspace.lints.rust]
unsafe_code = "allow"
EOF
}

remove_crate_lints_workspace() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  cat > "$crates_dir/oce-api/Cargo.toml" <<'EOF'
[package]
name = "oce-api"
EOF
}

remove_crate_lib_forbid() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  cat > "$crates_dir/oce-api/src/lib.rs" <<'EOF'
//! Fixture crate.
EOF
}

remove_no_db_gate() {
  dir="$1"
  _deny="$2"
  grep -v 'check-default-no-db' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

remove_determinism_matrix() {
  dir="$1"
  _deny="$2"
  grep -v 'oce-blocks -p oce-expr' "$dir/ci.yml" > "$dir/ci.yml.tmp"
  mv "$dir/ci.yml.tmp" "$dir/ci.yml"
}

empty_crate_dir_only() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -rf "$crates_dir"
  mkdir -p "$crates_dir/oce-empty"
}

remove_crate_lib_files() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -f "$crates_dir"/*/src/lib.rs
}

remove_crates_dir() {
  _dir="$1"
  _deny="$2"
  _root_cargo="$3"
  crates_dir="$4"
  rm -rf "$crates_dir"
}

# --- gate.sh drift fixtures ---------------------------------------------------------------
# The local gate script is the one place a contributor's "it passed locally" comes from, so each
# way it can silently fall behind CI gets its own negative case.

gate_no_list_mode() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  # A gate script the checker cannot interrogate must fail closed, not be waved through.
  printf '%s\n' '#!/usr/bin/env bash' 'echo "no list mode here"; exit 2' > "$gate_script"
}

gate_list_emits_prose() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  # Listing that mixes prose into the CMD stream: a consumer could match a pinned string
  # against banner text rather than an executed command, which is the text-grep hole again.
  {
    printf '%s\n' '#!/usr/bin/env bash'
    printf '%s\n' 'echo "open-control gate — mode: list"'
    printf '%s\n' 'echo "CMD cargo fmt --all --check"'
  } > "$gate_script"
}

gate_lists_without_executing() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  # The listing stays complete; the normal path runs nothing. This is the attack a review used
  # against the self-attesting design, and the shim cross-check is what rejects it.
  python3 - "$gate_script" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); s = p.read_text()
old = '  if "$@"; then :; else failures=$((failures + 1)); fi\n'
new = '  return 0\n'
assert old in s, "fixture shape changed; this case would be vacuous"
p.write_text(s.replace(old, new, 1))
PY
}

gate_executes_without_listing() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  # A command on the normal path only, invisible to the listing and therefore to every pin.
  printf '%s\n' '[ "$MODE" = list ] || cargo clippy --workspace --all-features --locked' \
    >> "$gate_script"
}

gate_aborts_on_first_failure() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  # Breaks failure accumulation. Invisible to an all-success trace, which is why the
  # all-failure trace exists.
  python3 - "$gate_script" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1]); s = p.read_text()
old = '  if "$@"; then :; else failures=$((failures + 1)); fi\n'
new = '  "$@" || exit 1\n'
assert old in s, "fixture shape changed; this case would be vacuous"
p.write_text(s.replace(old, new, 1))
PY
}

gate_lints_all_features() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  # Lints a build CI never produces, defeating the database-free default promise.
  sed 's/cargo clippy --workspace/cargo clippy --all-features --workspace/' "$gate_script" \
    > "$gate_script.tmp"
  mv "$gate_script.tmp" "$gate_script"
}

gate_script_missing() {
  _dir="$1"; _deny="$2"; _root_cargo="$3"; _crates_dir="$4"; gate_script="$5"
  rm -f "$gate_script"
}

run_case positive pass noop
run_case missing-release-nextest fail remove_release_nextest \
  "release-codegen nextest with hard-fail-on-zero-tests"
run_case seeded-advisory-ignore fail seed_advisory_ignore \
  "empty advisory ignore list"
run_case garbled-workflow fail garble_release_workflow \
  "scheduled heavy gate"
run_case missing-stale-status-gate fail remove_stale_status_gate \
  "run stale crate-status fixture tests"
run_case empty-bans-deny fail empty_bans_deny \
  "cargo-deny bans include representative SQL/ORM crate sqlx"
run_case missing-store-surface-gate fail remove_store_surface_gate \
  "oce-store public-api surface gate package selector"
run_case missing-golden-gen-firewall fail remove_golden_gen_firewall \
  "run golden-gen firewall fixture tests"
run_case missing-root-unsafe-forbid fail remove_root_unsafe_forbid \
  "workspace.lints.rust unsafe_code = \"forbid\""
run_case missing-crate-lints-workspace fail remove_crate_lints_workspace \
  "missing [lints] workspace = true"
run_case missing-crate-lib-forbid fail remove_crate_lib_forbid \
  "line 1 must be #![forbid(unsafe_code)]"
run_case missing-no-db-gate fail remove_no_db_gate \
  "run default-no-db smoke"
run_case missing-determinism-matrix fail remove_determinism_matrix \
  "debug determinism subset with hard-fail-on-zero-tests"
run_case empty-crate-dir-only fail empty_crate_dir_only \
  "no crate Cargo.toml files found under"
run_case missing-crate-lib-files fail remove_crate_lib_files \
  "no crate src/lib.rs files found under"
run_case missing-crates-dir fail remove_crates_dir \
  "crates directory is missing"
run_case gate-lints-all-features fail gate_lints_all_features \
  "lints --all-features"
run_case gate-script-missing fail gate_script_missing \
  "required gate file is missing or empty"
run_case gate-no-list-mode fail gate_no_list_mode \
  "does not support 'list' mode"
run_case gate-list-emits-prose fail gate_list_emits_prose \
  "emitted non-CMD output"
# The two mismatch guards and the failure-accumulation guard each get a case. Without these,
# deleting the comparison blocks from the checker left this whole suite green — the branch's
# central protection could be removed without a single test noticing.
run_case gate-lists-without-executing fail gate_lists_without_executing \
  "lists commands it does not execute"
run_case gate-executes-without-listing fail gate_executes_without_listing \
  "executes commands it does not list"
run_case gate-aborts-on-first-failure fail gate_aborts_on_first_failure \
  "stops after a failing step instead of accumulating"

# ── One negative fixture per pinned command ─────────────────────────────────────────────────
#
# The cases above pin how each assertion BEHAVES. They do not pin that it EXISTS: delete an
# assertion from check-workflow-gates.sh and its fixture simply stops being exercised, silently.
# A review demonstrated exactly that — removing the debug-subset, workspace-debug and
# workspace-release assertions left this suite green.
#
# So the list below is the independent statement of what must be pinned. For each command it
# strips that one command from an otherwise-complete gate fixture and requires the checker to
# fail NAMING it. Delete an assertion from the checker and its case here stops failing, turning
# this suite red. The duplication between this list and the checker is the mechanism, not an
# oversight: two independent statements that must agree.
PINNED_COMMANDS=(
  'cargo fmt --all --check'
  'bash .github/scripts/check-file-size.sh'
  'bash .github/scripts/check-no-secrets.sh'
  'bash .github/scripts/check-default-no-db.sh'
  'bash .github/scripts/check-golden-gen-anti-tautology.sh'
  'bash .github/scripts/test-check-default-no-db.sh'
  'bash .github/scripts/test-check-golden-gen-anti-tautology.sh'
  'bash .github/scripts/test-check-stale-crate-status.sh'
  'bash .github/scripts/check-stale-crate-status.sh'
  'bash .github/scripts/test-workflow-gates.sh'
  'bash .github/scripts/check-workflow-gates.sh'
  'cargo machete'
  'cargo clippy --workspace --all-targets --locked -- -D warnings'
  'cargo build --workspace --locked'
  'RUSTDOCFLAGS=-D warnings cargo doc --no-deps --workspace --lib --document-private-items --locked'
  'RUSTDOCFLAGS=-D warnings cargo doc --no-deps --workspace --bins --document-private-items --locked'
  'cargo deny check bans licenses sources'
  'cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --no-tests=fail'
  'cargo nextest run -p oce-blocks -p oce-expr --locked --profile ci --cargo-profile release --no-tests=fail'
  'cargo nextest run --workspace --locked --profile ci --no-tests=fail'
  'cargo nextest run --workspace --locked --profile ci --cargo-profile release --no-tests=fail'
  'cargo test --workspace --doc --locked'
)

pinned_case=0
for pinned_cmd in "${PINNED_COMMANDS[@]}"; do
  pinned_case=$((pinned_case + 1))
  case_dir="$tmp/pinned-$pinned_case"
  mkdir -p "$case_dir"
  gate_fixture="$case_dir/gate.sh"
  write_gate_fixture "$gate_fixture"

  # Drop exactly this command from the fixture's listing by neutralising the `step` line that
  # carries it. Matching on the listed argv rather than on source text keeps this honest for the
  # line-continued invocations, whose source never contains the joined form.
  python3 - "$gate_fixture" "$pinned_cmd" <<'PY'
import shlex, subprocess, sys, pathlib

path, target = sys.argv[1], sys.argv[2]

def listing():
    return subprocess.run(["bash", path, "list"], capture_output=True, text=True).stdout

assert f"CMD {target}" in listing(), (
    f"fixture does not list {target!r}; this case would pass vacuously")

# Join continuations, then reconstruct each step's argv the same way `step`/`step_env` render
# it. Reconstructing via shlex rather than matching source substrings is what makes this exact:
# `step_env 'label' RUSTDOCFLAGS '-D warnings' cargo doc ...` shares no contiguous text with the
# `RUSTDOCFLAGS=-D warnings cargo doc ...` line it prints.
joined, buf = [], ""
for line in pathlib.Path(path).read_text().splitlines():
    if line.rstrip().endswith("\\"):
        buf += line.rstrip()[:-1] + " "
        continue
    joined.append(buf + line)
    buf = ""

def rendered(line):
    if not line.lstrip().startswith(("step ", "step_env ")):
        return None
    try:
        t = shlex.split(line)
    except ValueError:
        return None
    if t[0] == "step":
        return " ".join(t[2:])
    if t[0] == "step_env":
        return f"{t[2]}={t[3]} " + " ".join(t[4:])
    return None

out = [line for line in joined if rendered(line) != target]
assert len(out) == len(joined) - 1, (
    f"expected to drop exactly one step for {target!r}, dropped {len(joined) - len(out)}")
pathlib.Path(path).write_text("\n".join(out) + "\n")
assert f"CMD {target}" not in listing(), f"failed to remove {target!r} from the listing"
PY

  set +e
  pinned_output="$(OCE_GATE_SCRIPT="$gate_fixture" bash "$SCRIPT" 2>&1)"
  pinned_status=$?
  set -e
  if [ "$pinned_status" -eq 0 ]; then
    echo "FAIL: removing '$pinned_cmd' from the gate left check-workflow-gates.sh green;"
    echo "      that command is not actually pinned."
    exit 1
  fi
  if ! printf '%s\n' "$pinned_output" | grep -Fq -- "does not run: $pinned_cmd"; then
    echo "FAIL: removing '$pinned_cmd' failed for the wrong reason:"
    printf '%s\n' "$pinned_output" | tail -3
    exit 1
  fi
done

# ── One negative fixture per pinned CI command ──────────────────────────────────────────────
#
# Same reasoning as the loop above, for the other half of the parity check. The gate-side pins
# were fixture-covered while the CI-side pins were not, so an assertion about ci.yml could be
# deleted without any test noticing — the identical gap, one file over.
PINNED_CI_LINES=(
  'cargo fmt --all --check'
  'bash .github/scripts/check-file-size.sh'
  'bash .github/scripts/check-no-secrets.sh'
  'cargo clippy --workspace --all-targets --locked -- -D warnings'
  'cargo build --workspace --locked'
  'RUSTDOCFLAGS: "-D warnings"'
  'cargo doc --no-deps --workspace --lib --document-private-items --locked'
  'cargo doc --no-deps --workspace --bins --document-private-items --locked'
  'github.event.pull_request.draft == false'
)

ci_case=0
for ci_line in "${PINNED_CI_LINES[@]}"; do
  ci_case=$((ci_case + 1))
  case_root="$tmp/pinned-ci-$ci_case"
  case_dir="$case_root/workflows"
  mkdir -p "$case_root"
  write_positive "$case_dir" "$case_root/deny.toml" "$case_root/Cargo.toml" \
    "$case_root/crates" "$case_root/gate.sh"

  if ! grep -Fq -- "$ci_line" "$case_dir/ci.yml"; then
    echo "FAIL: the ci.yml fixture does not contain '$ci_line'; that case would pass vacuously"
    exit 1
  fi
  grep -Fv -- "$ci_line" "$case_dir/ci.yml" > "$case_dir/ci.yml.tmp"
  mv "$case_dir/ci.yml.tmp" "$case_dir/ci.yml"

  set +e
  ci_output="$(
    OCE_WORKFLOW_DIR="$case_dir" OCE_DENY_TOML="$case_root/deny.toml" \
    OCE_ROOT_CARGO_TOML="$case_root/Cargo.toml" OCE_CRATES_DIR="$case_root/crates" \
    OCE_GATE_SCRIPT="$case_root/gate.sh" bash "$SCRIPT" 2>&1
  )"
  ci_status=$?
  set -e
  if [ "$ci_status" -eq 0 ]; then
    echo "FAIL: removing '$ci_line' from ci.yml left check-workflow-gates.sh green;"
    echo "      that CI command is not actually pinned."
    exit 1
  fi
done

echo "OK: workflow gate fixtures passed (${pinned_case} gate commands, ${ci_case} CI commands individually verified)."
