# Project facts

Things worth knowing before you change this repo that the code does not tell you.

## The gate

```bash
bash .agents/gate.sh docs         # a docs-only PR: the always-on checks, no cargo build
bash .agents/gate.sh leverage     # crates/leverage/**, docs, the root README.md
bash .agents/gate.sh              # any other change: the engine lane, debug codegen
bash .agents/gate.sh determinism  # the core four, oce-cxf, oce-api, Cargo.lock, the root Cargo.toml,
                                  # rust-toolchain.toml, .cargo/config.toml: adds the release-codegen subset
bash .agents/gate.sh full         # a pin advance or .agents/gate.sh itself: adds the workspace suite
                                  # and doctests; also what CI executes nightly
```

That script is the single source of truth for gate commands. Every other document
points at it. Run it in the form above and read the real output — a summary of a
gate is a claim about a gate, and this engine controls physical equipment.

**Run the row your PR's paths select, on the final head, and paste its tail** (issue #545, owner
decision of 2026-11-10; before it every PR owed `full`, 30 minutes cold). The five lines above
are the five rows, lightest first, and each runs everything the one before it runs.
`scripts/ci/proof-row.sh` maps changed paths to a row using the lane classifier and holds the
exact path lists, and every gate run ends by printing the row the checkout needs beside the row
it covered, so a tail that is too light says so. The `determinism` row is what the
release-codegen determinism subset builds and runs (the core four, `oce-cxf`, `oce-api`) and the
four files that set how all of it is compiled; the `full` row is a pin advance (`third_party/**`,
`tools/reference-catalog/**`, `crates/oce-cxf/tests/third_party_manifest/**` and
`crates/oce-blocks/src/registry/catalog_guard_support.rs`, the two places a pinned upstream
commit is written) and `.agents/gate.sh`, whose `full`-only steps no lighter mode executes. Rows
four and five beyond the issue's own table are coordinator decisions of 2026-11-10, to be
confirmed by the owner. Four rules go with the table:

- A rebase onto `development` with no conflicts needs `CI OK` reported on the new head, not a
  local re-run. A rebase with conflicts, or a fix round, re-runs the row.
- No row runs every crate's suite. Run the tests of each crate the PR changes:
  `cargo nextest run -p <crate> --locked --profile ci --no-tests=fail`, and
  `cargo test -p <crate> --doc --locked` for its doctests.
- `full` is owed only by the fifth row. It is also what CI executes nightly.
- A change to `release-gate.yml`, or to anything else only `full` or the nightly run executes,
  also dispatches the workflow on its branch (`gh workflow run release-gate.yml --ref <branch>`)
  and waits for a green run before merging. That run posts a `CI OK` of its own on the head,
  without the strict-bit matrix; read `CI OK` for the merge from the `pr-gate` run on the head
  SHA (`gh pr checks <N>` names the workflow of each check).

Change a command by changing [`.github/workflows/pr-gate.yml`](../.github/workflows/pr-gate.yml)
first, then the script.

## Hosting and CI

GitHub is the primary host: `origin` is `github.com/jscott3201/open-control-engine`, and branches,
PRs, reviews, issues and CI live there. CI is GitHub Actions in `.github/workflows/` on GitHub-hosted Linux
runners (plus the macOS platform cell in `pr-gate.yml` and the Windows one in `merge-gate.yml` and `release-gate.yml`): `pr-gate.yml` (per-PR, light), `merge-gate.yml` (every merge into `development`: the four determinism cells, their comparisons and the Windows cell), `release-gate.yml` (nightly against
`development`, release PRs into `main`, pushes to `main`, dispatch; heavy), `advisories.yml` (daily), `docs-pages.yml` (docs validation and the Pages
publish from `main`), `release.yml` (tag verify / manual crates.io publish) and the two manual
native-arm64 OpenModelica evidence workflows. `ci.yml` there is byte-frozen and disabled in the
repository's Actions settings — it is not the CI.

The per-PR gate runs once per change, on the PR: `pr-gate.yml` has no `push` trigger, and
`check-workflow-gates.sh` refuses one. A merge into `development` runs `merge-gate.yml` on the
merge commit instead (issues #502 and #545), beside the path-filtered `docs-pages.yml`. That run
holds what the per-PR gate does not and a merge cannot wait a night for: the determinism subset
under release codegen on both architectures with the debug-against-release comparisons, and the
Windows cell (the measured time is in
[CI and the gate](../docs/ci-and-the-gate.md#what-runs-after-the-merge)). Its aggregate is `Merge gate OK`, deliberately not `CI OK`,
and it is not a required status — the commit has landed — so after merging a change that runs the
determinism lane, watch it: a red run is fixed forward or reverted before the next
determinism-lane merge. Pushes queue: a run in progress finishes and only the newest waiting run
is kept, so an older waiting run shows as cancelled.

The heavy set runs nightly, not per merge (issue #545): `release-gate.yml` runs the workspace
suite in debug and release codegen, the doctests, the executed gate script (`gate (full)`),
cargo-deny with advisories, the light gates again and the same three merge-gate jobs, against the
`development` tip every night, on manual dispatch, on every `development` → `main` release PR and
on pushes to `main`. It took about two hours per merge when it ran on each one. Before a
determinism-lane merge, check the latest nightly with
`gh run list --workflow release-gate.yml --branch development --event schedule --limit 1`:
the merge needs it not red, or its failure already being fixed forward; otherwise the red nightly
is fixed forward or reverted first. `check-workflow-gates.sh` pins each
workflow's events, branches and exact job set, and requires the three shared jobs to be identical
in both files. See [CI and the gate](../docs/ci-and-the-gate.md#what-runs-when).

The per-PR gate has three lanes under the one `CI OK` (issues #393 and #503). Its first job classifies the
PR's changed paths with plain git and `scripts/ci/classify-changes.sh`, fail-closed: any path that
is not documentation or under `crates/leverage/` runs the determinism lane — every job that
compiles the engine — and so does any change to a gate script, workflow or tool config. No per-PR
job executes `.agents/gate.sh` any more (`gate (light)` did; `gate (full)` in `release-gate.yml`
runs it nightly). A `crates/leverage/**` change runs only the `leverage lane` job beside the
always-on checks (fmt, file-size, no-secret, docs, gate fixtures, the package/authority/product
contract); an engine change runs it too. A docs-only PR compiles nothing, and documentation a
test reads (`docs/cxf-composite-subset.md`, and `docs/design/vectors.md` since issue #519) is not
docs-only: it is an engine change.

The root `README.md` is its own lane (issue #503), so updating it in a landing PR no longer costs
the determinism lane. It runs one job, `facade contract and Quickstart`, which holds the two README
checks that are in no always-on job: the `readme_quickstart` byte-pin of the Quickstart against
`crates/oce-api/examples/quickstart.rs`, and the executed example. An engine change runs that job
too. Only the root file: `crates/*/README.md` is still an engine path. If you add a test or script
that reads the root README, `scripts/ci/test-classify-changes.sh` fails until its `README_CHECKS`
table names the always-on or README-lane job that runs it.

`CI OK` accepts a skip only when the job's lane output was `false`. Locally,
`bash .agents/gate.sh leverage` mirrors a PR confined to `crates/leverage/`, the docs and the root
README (it always runs the two README checks), and `bash .agents/gate.sh docs` a docs-only one; see
[lanes](../docs/ci-and-the-gate.md#lanes-what-a-pr-runs).

Two platform cells observe other operating systems (issue #394): macOS arm64, per PR with the
determinism lane, checks that the portable state snapshot equals Linux byte for byte; Windows, in
`merge-gate.yml` after each merge (issue #502) and nightly in `release-gate.yml`, builds the workspace and runs its suite without
the strict-bit binaries — observations, not qualification; see
[platform cells](../docs/ci-and-the-gate.md#platform-cells-macos-and-windows).

Pull requests merged on the previous host before the October 2026 move to GitHub share numbers
with GitHub PRs (#332 names two different changes). Bare `(#N)` citations predate the move; cite
every PR merged on GitHub as `(GitHub #N)` in `CHANGELOG.md` and the docs.

Native strict-bit evidence is re-established on every PR: the strict-bit matrix in `pr-gate.yml`
recaptures the pinned corpus in four native Linux cells and compares them from scratch, so no
source file is bound to a stored receipt (the 35-file source-digest binding and its
`recapture-pending` status were retired with GitHub #392). `.github/workflows/ci.yml` stays
byte-frozen and disabled as the history of the retained capture archives. See
[`docs/strict-bit-evidence.md`](../docs/strict-bit-evidence.md).

## A green PR does not mean the tests passed

CI is dev-light and release-heavy, and the split is easy to misread in the
dangerous direction.

The per-PR gate into `development` runs fmt, clippy, build, rustdoc, the file-size
cap, the no-secret scan, the database-free check, the golden-gen firewall and
[hygiene job](#golden-gen-hygiene-is-ci-only-for-now), the docs job, the
[crate-suite job](#the-crate-suite-job-is-ci-only-for-now), the closed
package/feature/publication contract and its hostile controls, the gate fixtures,
the [authority index/projection and its hostile controls](../docs/authority-claims.md),
`cargo machete` — and the state-determinism tests for **`oce-api`, `oce-blocks`, and `oce-expr`**, via the
determinism matrix: two native cells per PR, x86_64 on `ubuntu-latest` and aarch64 on
`ubuntu-24.04-arm`, in debug codegen.
Its fan-in job compares a populated portable engine-state snapshot byte-for-byte across
architectures and requires target-bound bytes to differ across architectures. Each architecture
then parses and refuses the other's target-bound bytes through the public restore path, natively.
The same fan-in requires the 46 compiled-schedule blobs each cell encodes (`OCE_BLOB_OUT`) to be
byte-identical in both cells (GitHub #400).
The release-codegen cells of that matrix do not run per PR (issue #502): `merge-gate.yml` runs
all four cells on every merge into `development`, and its fan-in adds the checks that portable and
target-bound bytes, the blobs and the round-trip recordings match across debug/release codegen.
Before the merge, `bash .agents/gate.sh determinism` runs the subset under release codegen on your
machine; that run is the release-codegen evidence a PR the `determinism` row names has, and the
only kind of PR that owes it.
The blob round trip (issue #403) rides on the same cells: each records
`crates/oce-conformance/tests/blob_round_trip.rs` (facade against the `no_std` entry, every output,
warning and raw state word after every frame of the 46 fixtures) into `OCE_ROUND_TRIP_OUT`, the
fan-in requires the recordings identical (the two debug cells per PR, all four in the merge
gate) and replays the x86_64 debug recording on aarch64
through the aarch64 debug cell's archived suite, and `blob round trip (wasm32-wasip1 under wasmtime)` replays it on
`wasm32-wasip1` under the wasmtime pinned in `scripts/ci/install-wasmtime.sh`. Locally,
`bash .agents/gate.sh roundtrip` (also in `full`) records and replays on the host and on
`wasm32-wasip1`; it needs `rustup target add wasm32-wasip1` and `bash scripts/ci/install-wasmtime.sh`
and fails, not skips, without the target; it runs the installer itself, which keeps a binary
already installed only if its SHA-256 is the pinned one, and never uses another `wasmtime` from
PATH. The installer puts wasmtime in a user cache directory
(`$OCE_WASMTIME_DIR`, else `${XDG_CACHE_HOME:-$HOME/.cache}/open-control-engine/wasmtime-bin`), not
under `target/`, so the `cargo clean` at a merge boundary no longer deletes it and every worktree
shares one copy (issue #545). A behaviour change that moves a run digest moves the recording
with it; there is nothing extra to bless.
The standalone `cargo-deny` CI job runs `cargo deny check bans licenses sources` on every PR, and
`.agents/gate.sh` runs it too.
`advisories` is excluded from the script deliberately: it needs network and a writable
advisory-db, neither of which a sandboxed lane has, so it runs in `advisories.yml`.

The scoped `oce-conformance` strict-bit subset and four affected per-block suites also run per PR
in four native Linux architecture/codegen cells (both architectures, debug and release; this
matrix did not move), with repeat captures and an eight-capture
cross-cell comparison ([bounded evidence](../docs/strict-bit-evidence.md)). The
`oce-graph`, `oce-validate`, `oce-model`, `oce-cxf` and `oce-store` suites run per
PR too, x86_64 debug only. The remainder — and the release-codegen runs of those five — runs in
the nightly release gate. A change confined to `oce-diag` or `oce-semantics` can show a fully
green PR having executed none of its own tests, and its local proof row does not run them either.
Before claiming your tests pass, run the suites of the crates you changed
(`cargo nextest run -p <crate> --locked --profile ci --no-tests=fail`) and read the tail.

Pin-advance PRs — any change under `third_party/**` or to the pin constants — still run
`bash .agents/gate.sh full` first-hand; see the vendored README's
`## Pin-advance policy` section. It is the `full` row of the local proof, and `proof-row.sh` names
it for every path a pin advance moves: the G36 half of the pin guard runs only in the workspace
suite, which CI now runs nightly, after the advance would have landed.

**Confirm the checks ran.** GitHub Actions runs the whole per-PR gate on every PR, drafts
included, and `CI OK` is the one status that summarizes it. A PR with no checks is easy to
mistake for a PR with no failing checks; confirm `CI OK` actually reported, not merely that
nothing is red.

## The core four build `no_std + alloc`

`oce-model`, `oce-expr`, `oce-blocks` and `oce-graph` have `default = ["std"]` and build
`#![no_std]` with `--no-default-features --features alloc` (issue #397). In those four, write
`core::`/`alloc::` paths and import `Vec`, `String`, `Box`, `format!` and `vec!` from `alloc`; call
the crate's `math` module (which re-exports `libm`) for any `f64` method that is not in `core`
(`floor`, `ceil`, `trunc`, `round`, `sqrt`, `powi`, `mul_add`, the transcendentals — the full table is
in `TESTING.md` "Conventions"), because clippy refuses the `std` method in each core crate's `src/` tree, unit tests included
(issue #399; `tests/` files are separate crates outside the lint, where `std` serves as the oracle); use no
hash collections; and give every new dependency `default-features = false`.
A crate that depends on one of the four names `features = ["std"]` on that edge, because their
workspace entries carry `default-features = false`. The bare-metal cell (the `no-std-build` job in
`pr-gate.yml`; `bash .agents/gate.sh nostd` runs it alone, and the light gate runs it too) builds,
lints and `cargo tree`-scans them for `thumbv7em-none-eabihf`, `riscv32imac-unknown-none-elf` and
`wasm32-unknown-unknown` (`scripts/ci/check-no-std.sh`; a dependency that turns `std` on fails the
scan even where it compiles); the scripts install nothing, so add the three targets once with
`rustup target add`. The same job checks the `no_std` `oce-graph` closure
(`scripts/ci/check-no-std-entry-closure.sh`), runs `oce-graph`'s `blob_execution` and
`blob_state_words` suites against the `no_std + alloc` build, and builds the firmware image (next
section). See [the architecture](../docs/architecture.md#no_std--alloc-the-core-four).

## The firmware image has a size budget, and runs only nightly

`firmware/link` (package `oce-firmware-link`, `publish = false`, a workspace member through the
`firmware/*` glob) links the core four into a bare-metal image that runs the
`multizone_vav_supply_fan` blob golden for 160 frames (issue #402). Per PR,
`scripts/ci/check-firmware-link.sh` builds it for `thumbv7em-none-eabihf` and
`riscv32imac-unknown-none-elf` with the root `firmware` profile and holds each ELF to
`firmware/link/size-budget.toml`; a change that grows the core can fail that budget. Raise a budget
only deliberately: `bash scripts/ci/check-firmware-link.sh --suggest` prints measured + 12 %, and
the PR says what grew. The image is executed only by `qemu.yml`, nightly and outside `CI OK`
(`bash .agents/gate.sh qemu` locally; needs `qemu-system-arm` and `qemu-system-riscv32`), which
diffs its per-frame run digests against the host reference runner
(`cargo run -p oce-firmware-link --bin blob-frames`). A change to a fixture's run digest in
`blob_frame_digests.tsv` moves the image's expected output with it: nothing else to bless. The
firmware lib and bin roots carry `#![forbid(unsafe_code)]` (`check-workflow-gates.sh` checks it).
A workspace copy (`test-math-lint.sh`, `test-check-no-std.sh`) must copy `firmware/` too, or
`--locked` fails on the lockfile's `oce-firmware-link` entry.

The core four use no hash collections at all: use `BTreeMap`, `BTreeSet`, `BinaryHeap` or a sorted
`Vec` (issue #398). Two things enforce this. The `clippy.toml` `disallowed-types` block refuses
`std`'s hash types; it is allowed workspace-wide and denied in each core crate root, so the `std`
crates keep their lookup maps. `crates/oce-api/tests/core_crates_hash_free.rs` also refuses
`hashbrown`, `ahash`, `indexmap` and similar crates, and checks that the lint wiring is still in
place. Outside the core, `clippy::iter_over_hash_type` is denied workspace-wide, so a `for` loop
over a `HashMap`/`HashSet` needs an `#[expect]` with a reason. The order-leak test is
`crates/oce-api/tests/hash_order_independence.rs`: it runs the G36 corpus on 20 independently
seeded threads and byte-compares every output.

## Compiled-schedule blob goldens move with the engine version

`crates/oce-api/tests/fixtures/blob/` holds one `OCEB` blob per G36 catalog fixture (46) and
`class_signatures.tsv`, the per-class signature hash of every registered class (GitHub #400).
`tests/blob_goldens.rs` re-encodes and compares them byte for byte. Four kinds of change move them,
deliberately:

- **A workspace version bump.** The header carries the encoder's engine version and the decoder
  accepts only its own, so every blob changes (header and CRC only) when `workspace.package.version`
  does. Regenerate them in the version-bump PR.
- **A class signature change.** A port, a parameter rule, a literal default, the state layout or a
  flag of any class changes `class_signatures.tsv`, and every blob that uses the class. A derived
  default's formula text does not.
- **A format change.** `FORMAT_VERSION` in `oce_graph::blob` stays 1 until 1.0.0 (issue #509): the
  layout changes in place, and the first such change after a release tag bumps the workspace
  version in the same PR, because the engine version in the header is what refuses a blob of the
  earlier layout. `tests/blob_old_layout.rs` keeps one release-0.4.0 blob unblessed to prove it.
- **A canonical content id change.** The header carries the loaded model's `oce:canon1:` digest
  (GitHub #470), so a change to the id's encoding or meaning (a new graph record, say) or to
  anything it covers moves every affected blob header and the content-id pin together. The tag stays
  `oce:canon1:` before 1.0.0: an encoding change re-stamps the ids in place, and the engine version
  tells the layouts apart.

Regenerate with `OCE_BLESS_BLOB=1 cargo test -p oce-api --test blob_goldens --locked`, run it again
without the switch, and say in the PR which files moved and why. The blob decoder lives in
`oce-graph` (`no_std`), so a decoder change also has to pass the `no-std-build` mirror.

## The content-id pin moves with the canonical ids

`crates/oce-api/tests/fixtures/content_id/pin.tsv` pins the canonical content id of each of the 47
corpus documents (GitHub #470), one `<content id> TAB <source path>` line each. It holds current ids
only: the retired-id column went with issue #509, and nothing in this repo maps one id scheme to
another. Its owner test, `crates/oce-api/tests/content_id_pin.rs`, regenerates the whole file
and compares it; regenerate with
`OCE_BLESS=1 cargo nextest run -p oce-api --test content_id_pin`, run it again without the
switch, and say in the PR which ids moved and why. Its `# Engine version:` line moves with a
workspace version bump.

## Formats stay at revision 1, and the workspace version separates layouts

Every format and schema the project defines stays at revision 1 until 1.0.0 (owner ruling,
2026-11-09, issue #509): the OCEB blob format, the state snapshot format and execution-state ABI,
the compatibility descriptor, every contract descriptor under `crates/oce-api/contracts/`, the
replay record and trace, `OCECANON` and its id tag `oce:canon1:`, and each stateful class's
`state_contract_revision`. A format changes in place and its goldens are re-blessed.

That makes the engine version the only field that tells two layouts apart. The blob header, the
state snapshot header and the descriptor's `oce-api-version` line carry it. A blob and a snapshot
are each accepted only by the engine version that wrote them (issue #530): both headers hold the
major, minor and patch of one constant, `oce_graph::blob::ENGINE_VERSION` (a pre-release suffix
is not recorded, so `0.5.0-rc.1` and `0.5.0` write and accept the same bytes), and a decoder
refuses any other with `EngineVersionMismatch`, on `BlobError` and on `EngineStateError`. The
snapshot decoder makes that check straight after its two revision fields and reads nothing
further from another version's bytes; the snapshot's fingerprint and trailer both cover the
field. `state_contract_revision` and the `oce:canon1:` tag stay at 1 like every other revision, so
the engine version separates their meanings too. So the first PR after a
release tag that changes a format's bytes or meaning does three things: it bumps
`workspace.package.version` on `development` to the next milestone's version, if it is not bumped
already; it re-blesses; and it includes a test that an artefact of the previous layout is refused
with a typed error (`tests/blob_old_layout.rs` and `tests/state_old_layout.rs` are the pattern).
A version bump is not a release: it cuts no tag
and leaves `CHANGELOG.md`'s unreleased section open. It moves `Cargo.toml`
(`workspace.package.version` and the `version` of each of the 11 workspace crates under
`[workspace.dependencies]`) and `Cargo.lock`, the 46 blob goldens (version bytes
and CRC), `compatibility.txt` and `compatibility_export.txt`, the content-id pin's
`# Engine version:` line (`content_id/pin.tsv`), the header table's engine-version row and the
worked hex in `docs/design/compiled-schedule-blob.md`, the sentence in the root `README.md` that
names the version `development` carries, and the version literals in
`blob_goldens.rs`, `compatibility.rs`, `compatibility_tests.rs`, `replay_codec.rs`,
`scripts/ci/test-check-no-std.sh` and `scripts/ci/test-leverage-members.sh`. It also moves the
three inline state pins in `crates/oce-api/src/tests/`, because a snapshot's header, fingerprint
and trailer carry the version: the empty-model bytes and fingerprint in `state_tests.rs` and the
portable-vector checksum in `state_format_golden_tests.rs`. Each failing assertion prints the new
value; the vectors beside the checksum are built there without the codec, so copy the values in
and run the tests again. The snapshot fixtures under `crates/oce-api/tests/fixtures/`
(`state_unstamped.hex`, `state_revision_two.hex`, `state_legacy.hex`) are artefacts of earlier
layouts and never move.

Every exported and emitted CXF document carries the workspace version too, in the
translation-software label on its root node (`S231:translationSoftwareVersion`, GitHub #527), so
a version bump also moves every pin of exported or emitted bytes:

- the nine export goldens under `crates/oce-cxf/tests/fixtures/export/`
  (`OCE_BLESS=1 cargo nextest run -p oce-cxf --test export_golden`);
- `crates/oce-cxf/tests/fixtures/expectations/g36_boundary_input_allowed_delta.golden`
  (`OCE_BLESS=1 cargo nextest run -p oce-cxf --test export_declared_input_attrs`) and
  `g36_export_content.txt` beside it
  (`OCE_BLESS=1 cargo nextest run -p oce-api --test export_content_oracle`), all 47 records each;
- the three inline export digests in `crates/oce-api/tests/export_cxf.rs` (each failing assertion
  prints the new value);
- the six emit goldens under `crates/leverage/oce-author/tests/fixtures/emit/`
  (`OCE_BLESS_AUTHOR=1 cargo nextest run -p oce-author`);
- the `emit_text_stdout` and `emit_wire_namespace` CLI transcripts
  (`OCE_BLESS_CLI=1 cargo nextest run -p oce-cli`).

Run each again without the switch. Only the version string moves in them. No content id and no
blob golden moves for this reason: nothing reads the label back. The two unblessed documents
under `crates/oce-cxf/tests/fixtures/previous_layout/` and
`crates/leverage/oce-author/tests/fixtures/previous_layout/` are release-0.4.0 artefacts bound to
the namespace the engine wrote then, kept to prove they are refused; they never move.

## The class-ports golden moves with port names, descriptions and parameters

`crates/oce-blocks/tests/fixtures/class_ports.golden.txt` pins the contract port names of every
registered class (GitHub #440): at the empty parameter table, for every width-driven class at widths
1, 2 and the maximum, and for the three `TimeTable` shapes, plus every class description and every
parameter's type, default and settable flag. A new class, a port, a parameter default or a
description moves it.
Regenerate with `OCE_BLESS_CLASS_PORTS=1 cargo nextest run -p oce-blocks --test class_ports`, run
it again without the switch, and say in the PR which lines moved and why. It is separate from the
revision-1 catalog, whose bytes (`crates/oce-api/contracts/catalog.json`) and content id do not
move with it.

## The `oce-author` goldens and schema move with the authoring surface

`crates/leverage/oce-author/schema/sequence.schema.json` is the published wire-form JSON Schema
(derived with `schemars` from the wire DTOs, the `class` enum from the catalog's non-reserved class
paths, issue #463; a sequence parameter's `type` enum from `oce_api::enumeration_types()`, issue
#492, so a new enumeration row in the facade table moves it). `tests/schema_pinned.rs`
regenerates it and compares byte for byte, and walks it for keywords outside the strict profile. `tests/fixtures/wire/*.json` are canonical wire bundles
(each must round-trip byte for byte) and `tests/fixtures/findings/*.tsv` the stage-1 finding list of
each. A DTO doc comment, a catalog class, a finding message or the canonical writer moves them.
Regenerate with `OCE_BLESS_AUTHOR=1 cargo nextest run -p oce-author`, run it again without the
switch, and say in the PR which files moved and why. Nothing else rewrites them.
`tests/fixtures/emit/*.cxf.json` are the emitter's byte goldens (issue #464): the worked example,
the every-element sequence and four of the 31 corpus fixtures the importer reads, each emitted under the
default namespace. They are named `.cxf.json`, not `.jsonld`, because `oce-cxf`'s
`parameter_grounding_corpus` test pins the count of every tracked `.jsonld` file in the workspace and
pins each one's import; a leverage golden must not move an engine census. The emitter's layout or
writer, a catalog change (port names, parameter types, enumeration members) or the fixed class base
moves them; the same bless switch regenerates them, and every one must still load warning-free.
`tests/fixtures/text/*.oce` are the canonical text of every wire bundle and of the same four
imported corpus fixtures, and `tests/fixtures/text/fmt/*.out.oce` the `fmt` output of the hand-written
`*.in.oce` inputs beside them (comment and blank-line preservation; issue #482); the printer's
spelling, a catalog change to an array template or a bundle moves them, and the same
`OCE_BLESS_AUTHOR` switch regenerates them (the `*.in.oce` inputs are hand-written, never blessed).
`tests/fixtures/v0/` holds the frozen prototype's three v0 golden pairs (`*.graph.json`,
`*.cxf.json`), copied verbatim from its repository at a pinned commit (`PROVENANCE.md` there):
never blessed and never edited, with one recorded exception: the `S231` context IRI of the three
CXF files was rebound when the engine stopped reading the earlier namespace (GitHub #527;
`PROVENANCE.md` has the source digest beside the new one). The prototype's `.cxf.jsonld` files are renamed `.cxf.json` for the
same census reason as the emit goldens; re-copy them only with the provenance record. The `*.import.txt`
beside them are this crate's goldens of what `import_graph_v0` makes of each graph (the report and
the canonical text) and regenerate with the same switch.
`tests/fixtures/note/text_example.oce` is the note's worked text example, copied by hand like the
wire example below.
`tests/fixtures/note/wire_example.json` is the JSON block of the authoring note's "Wire form"
section copied verbatim (a test that read `docs/design/` would put the note on the classifier's
test-read list); when that block changes, copy it again by hand.

## The `oce` CLI transcripts move with every message they print

`crates/leverage/oce-cli/tests/fixtures/runs/*.txt` pin each golden run of the `oce` binary
(issue #489): its exit status, stdout (the JSON report or the command's document: CXF, canonical
text, a catalog or vocabulary listing) and stderr (clap's message, or the `--render always`
rendering), one transcript per subcommand and outcome class, plus `--help` of every command. They
read the `oce-author` fixtures in place, so anything that moves an `oce-author` golden, a finding or
parse message, a catalog description, parameter or port name (`catalog_list`, `catalog_class*`), a
`clap` upgrade or a flag's doc comment (the `help_*` transcripts) moves them too. Since issue #500
`oce catalog CLASS` also prints the block vocabulary, so the `catalog_*` transcripts of a described
class move with `oce-vocab`'s two data files (a description, a unit, a note), with a catalog
parameter rule (the `range` lines are rendered from `param_rules`) and with the pinned Buildings
commit (the links); `vocab_text` and `vocab_json` move with `oce_vocab::SCALE_FACTORS`. Regenerate with
`OCE_BLESS_CLI=1 cargo nextest run -p oce-cli`, run it again without the switch, and say in the PR
which transcripts moved and why. `tests/report_shape.rs` holds every report transcript to the
README's contract (sorted keys, `ok` iff exit 0, `errors` iff exit 2, no absolute path), so a
blessed transcript that breaks the contract still fails.

## The `oce-vocab` data files move with catalog names and the vendored CXF-Core

`crates/leverage/oce-vocab/data/generated.json` is regenerated and compared byte for byte by the
crate's own test (issue #499): it is the vendored
`third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld` projected through the catalog's
names. A new, renamed or removed class, port or parameter moves it (and fails the generator by
name until upstream has text for it or `curated.json` does), and so does a pin advance that
changes an upstream description or unit. Regenerate with
`OCE_BLESS=1 cargo nextest run -p oce-vocab`, run it again without the switch, and say in the PR
which entries moved and why. `data/curated.json` is written by hand and never blessed; the reader
refuses it unless it is in the canonical spelling. A new width-driven or positional class needs a
row in `tests/vocabulary/widths.rs`, and a new `loop`, `port-order`, `convention` or `pitfall`
note needs its case in `tests/vocabulary/engine_agreement.rs`. A new `CatalogRule` kind does not
compile in `oce-vocab` until `src/range.rs` renders it or lists it as type, shape or structure,
and it needs its row in `tests/vocabulary/ranges.rs` (which also pins how many parameters have a
range, so a new bound on any class moves that count). Vendoring a new `CXF-Core.jsonld`
is a pin advance: it also moves the vendored README's digest row, the hash manifest and its
tree-SHA constant, and `oce-cxf`'s `fixture_corpus_import.txt` line for the file.

## The `oce-verify` schema is blessed; its canonical goldens are derived by hand

`crates/leverage/oce-verify/schema/vectors.schema.json` is the published JSON Schema of the vector
format `oce/vectors/v1` (issue #519), derived with `schemars` from the reader's shapes in
`src/dto.rs`. `tests/schema_pinned.rs` regenerates it and compares byte for byte, and walks it for
keywords outside the strict profile; `tests/schema_conformance.rs` holds each closed object's keys
to the keys the reader knows, so a key added to a shape needs its arm in `src/decode.rs` and its
row in that test's `OBJECTS` table. A shape or a doc comment in `src/dto.rs` moves the schema.
Regenerate with `OCE_BLESS_VERIFY=1 cargo nextest run -p oce-verify`, run it again without the
switch, and say in the PR what moved and why. That switch rewrites the schema and nothing else.

`tests/fixtures/canonical/*.json` (the canonical forms of the two in-repo library rules' vector
files and of `tests/fixtures/sets/every_field.json`) and the `spec_id` literals in
`tests/canonical_form.rs` are **never blessed**: they come from
`tests/fixtures/derive_canonical.py`, a second implementation of the canonical form and of
FNV-1a-128 that shares no code with the crate, run by hand (the fixtures README has the three
commands). The canonical writer or number spelling, or either `vectors.json` under
`crates/oce-api/tests/fixtures/library_rules/`, moves them. The format stays `v1` until 1.0.0, so
a change to it rewrites those two `vectors.json` files, their SHA-256 rows in that directory's
README and these goldens in the same PR; `crates/oce-api/tests/library_rule_state_continuation.rs`
keeps its own small reader of the shape, because an engine crate cannot depend on a leverage crate.
`tests/fixtures/note/worked_example.json` is the JSON block of the vectors note's "The vector
format" section, and `tests/closed_objects.rs` reads the note and holds the two equal byte for
byte, which is why `scripts/ci/classify-changes.sh` lists `docs/design/vectors.md` as a document a
test reads (an edit to the note runs the determinism lane); copy the block again when it changes. A new `VectorErrorKind`
does not build until `VECTOR_ERROR_CODES` lists its code in order, and needs its case in
`tests/error_codes.rs`.

## The frame driver lives in `oce-graph`, and its run digests move with behaviour

The HostTick v1 frame driver (time checks, per-slot input checks, the non-finite policy, the
accepted-frame sequence, staging and the single evaluation) is `oce_graph::frame` (GitHub #401).
`oce-api`'s `prepare_frame`/`execute_frame` resolve names and call it; `oce_graph::blob::Executable`
calls it by slot on a `no_std` target. Change a frame rule there, never in the facade:
`crates/oce-api/tests/frame_driver_ownership.rs` refuses a facade source that re-implements one.

`crates/oce-graph/tests/fixtures/blob_frame_digests.tsv` holds one run digest per G36 fixture over
the shared frame schedule (`crates/oce-graph/tests/support/frame_schedule.rs`). The facade's
dual-path test and `oce-graph`'s `blob_execution` test (run against the `no_std + alloc` build in
`no-std-build`) both check it, so it moves with any change to a fixture's outputs or state
trajectory, like the Tier-2 traces. Bless deliberately with
`OCE_BLESS_FRAME_DIGESTS=1 cargo nextest run -p oce-api --lib -E 'test(/blob_dual_path/)'`, rerun
both tests without the switch, and say in the PR which fixtures moved and why.

## golden-gen hygiene is CI-only for now

`tools/golden-gen` is its own cargo workspace, so the workspace fmt, clippy and test commands
never reach it. The `golden-gen-hygiene` job in `pr-gate.yml` covers it: fmt, clippy
`-D warnings`, its tests, and a release regeneration that must leave `tools/golden-gen/goldens`
byte-identical with no untracked files. `.agents/gate.sh` does not run these yet; wiring them in is
a separate change to the script. Until then, run them by hand after touching the generator or its
goldens:

```bash
cd tools/golden-gen
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo run --release --locked
git status --porcelain -- goldens    # must print nothing
```

## The crate-suite job is CI-only for now

One `pr-gate.yml` job that `CI OK` needs runs a command `.agents/gate.sh` does not, like the
golden-gen hygiene job: wiring it into the script is a separate change. `crate-tests` runs the
`oce-graph`, `oce-validate`, `oce-model`, `oce-cxf` and `oce-store` suites in x86_64 debug. It is
a separate job, not a determinism-matrix cell, so it runs beside the cells instead of lengthening
them. Run it by hand after touching those crates:

```bash
cargo nextest run -p oce-graph -p oce-validate -p oce-model -p oce-cxf -p oce-store \
  --locked --profile ci --no-tests=fail
```

The `docs` job is no longer CI-only (issue #545): every gate mode runs its steps, the mdBook
install included (`scripts/docs/install-mdbook.sh target/mdbook-bin`, which downloads the pinned
release once and afterwards only re-verifies it; `cargo clean` removes it, and the next run
downloads it again). That job runs the documentation checks `docs-pages.yml` also runs; that
workflow is path-filtered and not required, and still builds and publishes the site from `main`.

## Clippy lints the default feature set

`cargo clippy --workspace --all-targets --locked -- -D warnings`, matching CI.

Do not add `--all-features`. `oce-api` declares no features today, but the promise
this repo gates on is that the *default* build links no database and no async
runtime. Linting all features would check a configuration that never ships, and let a
default-build regression through once any crate grows an optional feature.

## The no-secret gate rejects more than secrets

`.github/scripts/check-no-secrets.sh` runs in CI and in the pre-commit hook. Besides
the usual credential shapes it fails on any UUID-shaped string and any absolute
`/Users/...` or `/home/...` path in tracked content. Both patterns are there because
a developer path or an identifier pasted into a committed file is a leak that has to
be scrubbed from history rather than simply deleted.

Keep absolute paths out of committed files, including scripts and doc examples.

## Working directories are gitignored

`.gitignore` excludes every top-level `_*/` directory, so `_spec/`, `_research/`,
`_review/` and `_tracker/` are entirely absent from a clone. (Four
`_spec/oce_g36_gap_specs_v1/reference/` files were force-added exceptions until
2026-07-28; the two conformance fixtures among them now live at
`crates/oce-cxf/tests/fixtures/profile/`.) Two consequences bite in practice:

- **`git add -A` silently stages nothing** for a new file under those paths, and
  exits 0. Tracking a file there anyway needs a deliberate `git add -f <exact path>`
  — and a written reason.
- A reference to `_spec/...` points at a file no clone has. Quote the excerpt you
  depend on rather than citing the path alone.

## Run `cargo clean` between PRs

At every merge boundary, before the next branch is cut, clean the tree you just
worked in. This workspace builds large: a single review worktree has reached 4.9 GB
of `target/` against 1.2 GB in the main checkout, and artifacts otherwise drag
forward from one PR into the next indefinitely.

Two things make this safe to do and easy to get wrong:

- **Never clean a tree while a build is running in it.** A build mid-gate in that
  tree fails in a way that looks like a code defect.
- **Worktrees have independent `target/` directories** — `.cargo/config.toml` sets no
  shared `[build] target-dir`. Cleaning the main checkout cannot disturb work in a
  worktree, and vice versa. Clean each tree on its own schedule.

## Branches

Base branch is `development`. Branch protection on GitHub blocks direct pushes to it, requires
a pull request, requires the `CI OK` status from `pr-gate.yml`, and requires the branch to be up to
date with `development` before merging; everything lands by squash-merge through a GitHub PR. A PR
that falls behind is updated and its gate re-runs, because that run is the only one the change
gets. An update that merges or rebases cleanly needs that CI run and no local one; one with
conflicts re-runs the PR's local proof row. A fix round pushes to the **same** branch — never a second PR for the same
work.
