# CI and the gate

For contributors, and for anyone looking at a green check mark on a pull request and wondering what
it proves. The short answer is: less than you would assume. The split is deliberate, and it is easy
to misread in the dangerous direction.

## Where CI runs

CI runs on GitHub Actions, the project's primary host, from the workflows in
[`.github/workflows/`](../.github/workflows/) on GitHub-hosted Linux runners — x86_64
(`ubuntu-latest`) for most jobs, plus native arm64 (`ubuntu-24.04-arm`) for the aarch64 cells of the
determinism and strict-bit matrices, and one `macos-15` and one `windows-latest` job for the
[platform cells](#platform-cells-macos-and-windows): `pr-gate.yml`
(per-PR into `development`), `merge-gate.yml` (every merge into `development`), `release-gate.yml`
(a nightly `development`-tip run, release PRs into `main`, pushes to `main` and manual dispatch),
`advisories.yml`, `docs-pages.yml`, the tag-verify / manual-publish
`release.yml`, the manual native-arm64 OpenModelica evidence workflows, and the nightly
[emulation run](#the-nightly-emulation-run) (`qemu.yml`). Actions in the gating
workflows and `docs-pages.yml` are pinned to full commit SHAs, which `check-workflow-gates.sh`
enforces for the gating workflows. `release.yml` and the OpenModelica evidence workflows are
byte-bound by their own approval and evidence manifests, so they keep tag references until their
next reviewed refresh.
The `ci.yml` in that directory is not the CI: it is the GitHub-era gate that produced the
[retained strict-bit capture archives](strict-bit-evidence.md#retained-native-archives), stays
byte-identical as their history, and is disabled in the repository's Actions settings.

`pr-gate.yml` and `release-gate.yml` each end in a `CI OK` job that needs every other job and
fails unless each one succeeded — or, in `pr-gate.yml` only, was skipped by its
[lane filter](#lanes-what-a-pr-runs); `check-workflow-gates.sh` asserts that its `needs:` lists
every other job in the file, that no `release-gate.yml` job carries a job-level `if:`, and that
every `pr-gate.yml` job-level `if:` is a lane filter that `CI OK` knows about. Branch protection
requires exactly that one status: the `CI OK` check from `pr-gate.yml` on `development` and from
`release-gate.yml` on `main`. `merge-gate.yml` ends in an aggregate of the same shape named
`Merge gate OK`. It is not a required status, because it reports on a commit that has already
landed ([below](#what-runs-after-the-merge)), and it is deliberately not called `CI OK`: a release
PR's head is a `development` commit the merge gate has already reported on, and a check of that
name from its short run could satisfy `main`'s branch protection in place of the release gate's.
`check-workflow-gates.sh` refuses the name there.

## What runs when

The per-PR gate runs **once per change**, on the pull request:

- **A PR into `development`** runs `pr-gate.yml` (and `docs-pages.yml` when it touches the docs
  paths below). Which of its jobs run depends on the paths the PR changes — see
  [Lanes](#lanes-what-a-pr-runs). A new push to the PR branch cancels the superseded run.
- **A merge into `development`** runs `merge-gate.yml` on the merge commit: the Windows cell,
  the one job no pull request runs (issues #502, #545 and #556;
  [what runs after the merge](#what-runs-after-the-merge)), and the path-filtered
  `docs-pages.yml` when the merge touches its paths. It does not re-run `pr-gate.yml`, which has
  no `push` trigger: the merged tree is the one the PR already tested, so that would repeat the
  per-PR gate for no new evidence, and `check-workflow-gates.sh` refuses any `push` trigger in
  `pr-gate.yml`, so the duplicate run cannot return unnoticed.
- **Nightly**, `release-gate.yml`, the heavy gate, runs against the `development` tip (a manual
  dispatch runs it against the ref it is dispatched on): the workspace suite in debug and release codegen, the doctests, the executed
  gate script, cargo-deny with advisories, the light gates again, the determinism matrix in all
  four cells and the Windows cell. It is where every crate's own tests run, and the safety net for what a run on
  a merge commit can miss: a run that was replaced while it waited, an advisory published since,
  or a runner image that changed under an unchanged tree.
- **A PR into `main`, or a push to `main`,** runs `release-gate.yml`, the whole heavy set.
- **Nightly**, and on manual dispatch, `qemu.yml` runs the firmware image under emulation
  ([below](#the-nightly-emulation-run)). It is not part of any `CI OK` and never runs on a PR.

### Lanes: what a PR runs

`pr-gate.yml` has three lanes under the one `CI OK`. The **determinism lane** is every job that
compiles the engine, bar the one the README lane holds: the determinism and strict-bit matrices
and their fan-ins, the
[blob round trip](#the-blob-round-trip)'s wasm32 leg, the x86_64 crate
suites, clippy, build, the [bare-metal cell](#the-bare-metal-cell), rustdoc,
cargo-deny, cargo-machete, the database-free check, the golden-gen firewall and hygiene jobs, and
the macOS
[platform cell](#platform-cells-macos-and-windows) — `portable snapshot (macOS arm64, debug)` and
its `portable snapshot comparison` fan-in. The determinism matrix and the strict-bit matrix are each four cells per PR (x86_64 and
aarch64, debug and release codegen; the determinism matrix's release cells ran after the merge
from issue #502 until issue #556).
The **leverage lane** is one job,
`leverage lane`, for the crates under [`crates/leverage/`](../crates/leverage/README.md): the
layout and `oce-api`-only dependency rule (`oce-bless` is allowed as a dev-dependency only, for the
golden switch; normal and build edges and dev edges are read separately), then fmt, clippy
`-D warnings`, rustdoc `-D warnings`, nextest and cargo-machete per member
(`scripts/ci/leverage-members.sh lane`; with no members it prints `no leverage crates yet`). The
members are `oce-replay`, `oce-author`, `oce-cli` (the `oce` binary), `oce-vocab` and `oce-verify`.
The lane's `oce-cli` suite holds the M3 **"In-repo rules replay"** evidence (vectors note decision
19; GitHub #573). `crates/leverage/oce-cli/tests/replay_gate.rs` runs the built `oce verify` over
both in-repo library rules, read in place under `crates/oce-api/tests/fixtures/library_rules/`. It
requires exit 0 and stdout byte-equal to `oce-verify`'s golden receipts, so every leverage and
engine PR runs it.
The **README lane** is one job, `facade contract and Quickstart` (issue #503). It runs when the
root `README.md` changes and for every engine change, and it holds the two checks of the README's
Quickstart that no always-on job runs: the `readme_quickstart` test, which requires the README's
first Rust block to be byte-identical to `crates/oce-api/examples/quickstart.rs`, and the
execution of that example. The facade compiler contract and its controls share the job because
they build the same facade; they do not read the README.

The first job, `classify changed paths`, lists what the PR changes with plain git — the tested merge
commit against its base parent, renames as a deletion plus an addition — and classifies it with
`scripts/ci/classify-changes.sh`. The rules fail closed: a path that matches neither the docs-only
nor the leverage allowlist, and is not the root README, counts as an engine change.

- **docs-only** paths: `docs/**`, `site/**`, `scripts/docs/**`, a root-level `*.md`,
  `.agents/project-facts.md`, `.github/ISSUE_TEMPLATE/**` and `.github/pull_request_template.md` —
  except the root `README.md` (next) and documentation a test reads
  (`docs/cxf-composite-subset.md`, read by an `oce-cxf` test, and `docs/design/vectors.md`, whose
  worked example an `oce-verify` test compares with its fixture: the note is the specification of
  the vector format, so an edit to it runs the engine lanes), which counts as an engine change. A
  fixture scans the crates for documentation reads so the exception list cannot fall behind.
- **the root `README.md`**, and only that file (`crates/*/README.md` is an engine path,
  `crates/leverage/**/README.md` a leverage path, `docs/README.md` docs-only): it selects the README
  lane and neither of the other two. Its other readers are always-on: docs staging copies its
  Quickstart into the book, the documentation checker contracts compare its verification section
  with the evidence pages, and the product contract check reads its pointer and its Rust example.
  The classifier fixture (`scripts/ci/test-classify-changes.sh`) holds that list: it scans the
  crates and scripts for sources that name the root `README.md` and fails when one is run neither
  by an always-on job nor by the README-lane job, and its controls prove each of those failures.
- **leverage** paths: `crates/leverage/**`.
- **gate-script** paths: `.agents/gate.sh`, `.github/scripts/**`, `scripts/**` outside
  `scripts/docs/`, `.github/workflows/**` and `.config/**`. A change to how CI runs must prove the
  whole gate still passes, so these count as engine changes too. The classifier still reports them
  in its `gate_scripts` output, but no job is filtered on it: like `docs_only` it is informational.
- **everything else** is an engine change: any crate outside `crates/leverage/`, `Cargo.toml`,
  `Cargo.lock`, `rust-toolchain.toml`, `.cargo/**`, `deny.toml`, `tools/**`, `third_party/**`,
  `.githooks/**`, `.gitattributes`, an unrecognised root file. Adding a leverage crate or changing
  its dependencies changes `Cargo.lock`, so it runs everything.

| Change class | Jobs that run | Jobs skipped | Expected wall-clock |
| --- | --- | --- | --- |
| docs-only | `classify changed paths`, fmt, file-size cap, no-secret scan, docs, gate behavior fixtures, the package/authority/product contract, `CI OK` | the determinism lane, `leverage lane` | 3 to 4 minutes, the time of the always-on set's longest job, `gate behavior fixtures`: 166 s and 240 s in runs 43167472832 and 43169110914 (2026-11-12, the same change on two runners), once issue #627 ran its workflow-gate cases four at a time. From GitHub #564 until then that job took 379–393 s (on GitHub #619 and #621), so a docs-only PR waited about 6.5 minutes; it was 2m40s in run 42476084422, before the suite grew |
| README (the root `README.md`, optionally with docs) | the docs-only set plus `facade contract and Quickstart` | the determinism lane, `leverage lane` | the docs-only time: the README job is shorter than `gate behavior fixtures`. It took 41 s with no rust-cache entry to restore in run 42703119408 (13 s for the facade compiler contract, 11 s to build and run the byte-pin) |
| leverage-only (`crates/leverage/**`, optionally with docs) | the docs-only set plus `leverage lane` | the determinism lane, `facade contract and Quickstart` | about 1 minute before the first leverage crate (56s in run 42476081207), plus each member's build and tests (`oce-replay` builds the facade and runs its suite) |
| leverage with the root `README.md` (a landing PR that updates the README's status) | the docs-only set plus `leverage lane` and `facade contract and Quickstart` | the determinism lane | the leverage-only time; the two jobs run side by side |
| engine (any other path, gate scripts, workflows and tool config included) | every job | none | about 5 minutes: 5 min 11 s in run 42953966045 (2026-11-11), the first with the four-cell determinism matrix back (issue #556), against 2.5 to 4.9 minutes over the seven full-lane runs before it. In that run the release determinism cells took 168 s (aarch64) and 226 s (x86_64), the jobs that wait for the matrix (its fan-in, the wasm32 round-trip leg, the macOS comparison) finished within 41 s of it, and `gate behavior fixtures`, which waits for nothing, took 276 s and finished last. That job then grew into the long pole as the workflow checker's cases did: 379–393 s on GitHub #619 and #621, 492 s on the head of GitHub #624. Issue #627 runs its cases concurrently: in runs 43167472832 and 43169110914 (2026-11-12) it took 166 s and 240 s, the x86_64 release determinism cell (265 s and 253 s) was again the longest job, and the jobs spanned 5 min 24 s and 5 min 21 s. Before issue #502 it was a median of 18.3 minutes over 69 full-lane runs (2026-11-07 to 2026-11-09), and about 26 with `gate (light)` |

The always-on set runs whatever changed: it compiles nothing (fmt only reads the tree), and the
package/authority/product contract stays in it because its checks read the documentation as well as
the manifests. An engine change also runs the leverage lane, because a change behind the facade can
break a crate built on it, and the README lane, because a facade change can break the Quickstart.
Before issue #503 the root `README.md` counted as an engine change, so a leverage PR that also
updated the README ran the whole determinism lane (GitHub #485 and #498, about 18 minutes a push
then) where one without a README change ran the leverage lane alone (GitHub #493, 2 minutes).

**No per-PR job executes `.agents/gate.sh`.** `gate (light)` did, for a gate-script change only,
until issue #502 retired it: it was a coverage backstop against a `pr-gate.yml` job drifting away
from the script, and at 25 to 27 minutes it was the long pole of every PR that touched a script or
a workflow. `release-gate.yml` runs `gate (full)`, a superset, nightly on the `development` tip
and on release PRs, so an edit to the script, a workflow or a script a job runs is first executed
by CI on the night after it merges; before the merge, the author's own
[local proof](#the-local-proof) on the final head is the run. The three steps `gate (light)` once ran that no other job covered — the facade compiler
contract, its diagnostic controls and the executed Quickstart — are the `facade contract and
Quickstart` job, which runs for every engine change (it is the README lane), and the gate-script
coverage check and its fixtures are in `gate behavior fixtures`.

### What runs after the merge

One job: the Windows cell, in `merge-gate.yml`, on every push to `development`, which is every
merged PR. The history is three decisions. Issue #502 (2026-11-09) moved three long jobs out of
`pr-gate.yml` into `release-gate.yml`, which then ran in full after every merge. Issue #545
(2026-11-10) split that: `merge-gate.yml` took the jobs a merge needed reported before the next
one, and `release-gate.yml` kept everything and became nightly. Issue #556 (2026-11-10) returned
the determinism cells to the per-PR gate, because the reason they had left was gone.

| Moved out of the per-PR gate by issue #502 | Per-PR wall-clock on 2026-11-09 | Where it runs now |
| --- | --- | --- |
| the release-codegen determinism cells and the debug-against-release comparisons | 809–1101 s (x86_64) and 653–671 s (aarch64): 15m47s compiling 105 release test binaries, 26 s running 1,281 tests | **per PR again** (issue #556), and nightly in `release-gate.yml`. In `merge-gate.yml` run 42948818576, the first on a merge commit after the integration tests of `oce-api`, `oce-author`, `oce-cxf` and `oce-conformance` were built as a few binaries each (GitHub #546, #551, #553, #554), the cells took 174 s (x86_64) and 181 s (aarch64), whole job, and each comparison leg about 20 s |
| `std build and nextest (Windows x86_64, debug)` | 251–450 s | `merge-gate.yml`, after each merge, and `release-gate.yml`, nightly; 333 s in run 42948818576 |
| `gate (light)` | 1540–1600 s, on a gate-script change | nowhere; `gate (full)` in `release-gate.yml` executes the script in `full` mode, nightly |

**How long the merge gate takes.** 5 min 41 s from push to `Merge gate OK` in run 42948818576
(2026-11-11, commit `c9a3409`), when it still ran the four determinism cells; the Windows cell
was the long pole at 5 min 33 s, so the one-job workflow should take about the same. Update this
paragraph, and only this one, when a run gives a newer figure. (Its first run, 42935095468,
before those consolidations, took 19.0 minutes, 18.5 of them the x86_64 release cell.)

**Why the heavy set is nightly.** In run 42767547196 (2026-11-10, on the merge of GitHub #543)
the jobs started 43 minutes after the push, queued behind the previous merge's run, and then took
1 h 29 min: `gate (full)` 88.7 minutes, of which the release-codegen workspace step was 55.7
minutes for 58 seconds of tests, and the full test suite 41.8 minutes. So a merge was reported
about two hours later, and the rule below ("watch the run before the next determinism-lane
merge") made that the interval between merges.

| Runs | `merge-gate.yml` | `release-gate.yml` |
| --- | --- | --- |
| on a push to `development` (each merged PR) | yes | no |
| nightly, on the `development` tip | no | yes |
| on manual dispatch | yes, on the dispatched ref | yes, on the dispatched ref |
| on a `development` → `main` release PR | no | yes |
| on a push to `main` | no | yes |
| jobs | `std build and nextest (Windows x86_64, debug)`, `Merge gate OK` | that cell, the four determinism cells and `determinism cross-cell comparison` (two legs), the workspace suite in debug codegen with the doctests, `gate (full)` (which runs the workspace suite in release codegen), cargo-deny with advisories, fmt, file-size cap, no-secret scan, the database-free check, the golden-gen firewall, clippy, build, rustdoc, cargo-machete, the gate fixtures, `CI OK` |
| required status | no | `CI OK`, on `main` |

`check-workflow-gates.sh` pins that table: each workflow's events and branch filters exactly, each
workflow's job ids exactly (a heavy job deleted from `release-gate.yml` with its `needs:` entry
fails, as does the Windows cell leaving `merge-gate.yml` or any other job entering it, the
determinism cells included), and the Windows job definition byte for byte between the two files.
The determinism cells and their fan-in are held the same way between `pr-gate.yml` and
`release-gate.yml`: every step, command, environment variable, matrix entry and step condition
must be equal, apart from the lane filter, the pinned checkout ref and the cache `save-if`, so a
cell, a codegen mode or a comparison cannot be dropped per PR while the nightly keeps it.
No job in `merge-gate.yml` or `release-gate.yml` may carry a job-level `if:`, which is why the
split is two files and not a condition on the event: `CI OK` would read a skipped job as a
failure, and a step-level condition on the event is refused for the reason given under
[the step-condition rule](#every-pull-request-runs-the-gate).

**Before the merge, release codegen is CI's.** The pull request's own run executes the
determinism subset under release codegen on both architectures and compares it with the debug
cells, so no author owes a local release-codegen run. `bash .agents/gate.sh determinism` runs the
subset under release codegen on one host, for reproducing a red release cell; it cannot compare
across cells or run the other architecture.

**After the merge, watch the run.** `Merge gate OK` is not a required status: the commit has
landed. After merging a change that runs the determinism lane, watch the merge-gate run on the
merge commit; a red run is fixed forward or reverted before the next determinism-lane merge, so a
second change is never built on a tree that does not build or pass on Windows. **A red nightly** is
fixed forward or reverted before the next determinism-lane merge. It names the `development` tip it
tested, not one merge: every merge since the last green nightly is a candidate. So the latest
nightly is a condition of a determinism-lane merge: it must not be red, or its failure must
already be being fixed forward.

```
gh run list --workflow release-gate.yml --branch development --event schedule --limit 1
```

**Several merges in an hour do not starve it.** Merge-gate runs are grouped by event and ref and
never cancelled in progress. For pushes to `development` GitHub keeps the run in progress and one
waiting run: a newer push replaces the waiting one, which then shows as cancelled on its merge
commit. The run that replaced it tests a commit that contains the earlier merge, so it is the
evidence for both; a red result there does not say which of the merges it covers caused it. A
merge-gate run checks out the commit its event names (`github.sha`). So do a push run and a
dispatched run of `release-gate.yml`, in every job, so their cells and fan-in build one tree even
when another merge lands meanwhile: `gh workflow run release-gate.yml --ref <branch>` tests that
branch's head with that branch's workflow file, which is how a PR that changes the release gate
shows it green before it merges. Such a run posts a check named `CI OK` on that head, from the
heavy set, which does not include the strict-bit matrix or the macOS cell; a manual dispatch of
`pr-gate.yml` posts one too. The `CI OK` that decides a merge into `development` is the one from
the `pr-gate` run on the pull request's head SHA: name the workflow when checking
(`gh pr checks <N>` prints each check's run). That is a procedure, not a control; GitHub #552
tracks making it one. A scheduled run checks out the `development` tip as each job
starts; a merge landing during such a run can hand the fan-in cells built from two commits, which
fails the comparison instead of passing it. `check-workflow-gates.sh` pins the triggers, the
checkout refs and both concurrency lines of both files.

**How a skip passes `CI OK`.** Each lane-filtered job has exactly one job-level `if:`,
`needs.changes.outputs.<lane> == 'true'` with lane `core`, `leverage` or `readme` (a job filtered on
`gate_scripts` or `docs_only` fails the checker, and `ci_ok.py` refuses a lane map that names
either), and `CI OK` (`scripts/ci/ci_ok.py`) accepts its
`skipped` result only when that output was `false`. `core` implies the other two, and `ci_ok.py`
refuses a classification that says otherwise. A pull request that adds a lane cannot show its own
filter working, because it changes CI files and so runs every lane; `test_ci_ok.py` therefore
composes the real classifier with the workflow's own lane map and checks, for a README-only, a
README-and-leverage, a leverage-only, a docs-only and an engine change, which filtered jobs run
and that `CI OK` passes, and that a failed README job fails it. A job skipped while its lane ran — the shape of
a job that never started because something it needs failed — fails, as does any skip of an
always-on job, any failure or cancellation, and a failed or self-contradictory classification.
`check-workflow-gates.sh` keeps `CI OK`'s lane map equal to the jobs' `if:` lines, pins each named
job to its lane, and fails if the result check or the classifier outputs are removed; its fixtures
prove each of those controls fails. A manual dispatch runs every lane.

`bash .agents/gate.sh leverage` is the local mirror of a PR confined to `crates/leverage/`, the
documentation and the root `README.md`: the always-on checks, the leverage lane and the README
lane's two checks (the Quickstart byte-pin and the executed Quickstart), without the engine lane.
It runs the README checks whether or not the README changed, because a landing leverage PR usually
changes it and CI then runs them. It leaves out the facade compiler contract, which shares the
README-lane job in CI but reads only the facade's source. `bash .agents/gate.sh docs` is the mirror
of a docs-only PR: the always-on checks alone, with no cargo build. `bash .agents/gate.sh` runs
every lane ([the local proof](#the-local-proof) says which a PR owes).

**Cache.** PR runs save `Swatinem/rust-cache` entries into their own PR scope only, so nothing a
PR does seeds the cache scope other PRs read. Runs on `development` do, since they execute on the
default branch, whose cache scope every PR into `development` can read: the `merge-gate.yml` run
after each merge saves the Windows cell's entry and nothing else, and the nightly
`release-gate.yml` run saves every other entry, the `nextest` ones for both architectures
included. After a `Cargo.lock` change the determinism, strict-bit and crate-suite cells therefore
build their dependencies cold, in every PR, until the next nightly (issue #556 removed the
arm64 save that `merge-gate.yml` made after each merge). Jobs that build the same thing in
those workflows therefore share one rust-cache `shared-key` (the key replaces rust-cache's per-job component; the
rustc host triple, the `CARGO_*`/`RUST*` environment and `Cargo.lock` still separate entries, so
x86_64 and arm64 never share one):

| `shared-key` | `pr-gate.yml` jobs | `merge-gate.yml` and `release-gate.yml` jobs |
| --- | --- | --- |
| `nextest` | the eight determinism and strict-bit cells, `crate-tests` | `test-suite` (debug, whole workspace) and the four determinism cells, in the release gate |
| `gate` | — | `gate (full)` |
| `clippy`, `build`, `rustdoc`, `default-no-db` | the job of that name | the job of that name |
| `facade-contract`, `leverage` | `facade contract and Quickstart`, `leverage lane` | — |
| `nextest-macos` | `portable snapshot (macOS arm64, debug)` | — |
| `workspace-windows` | — | `std build and nextest (Windows x86_64, debug)` (both) |
| `blob-round-trip-wasm` | `blob round trip (wasm32-wasip1 under wasmtime)` | — |

For the like-named pairs this only makes explicit what their identical job ids already did
(rust-cache's default key component is the job id); the `nextest` key is the new link. The release-gate `test-suite` entry is a superset in debug codegen (the whole workspace's dependency set; its release-codegen run moved into `gate (full)` with issue #545, so the entry no longer holds release dependencies), which
restores as a warm start for the narrower PR cells; rust-cache never caches workspace crates, so a hit saves
dependency compile time only, and a release determinism cell spends nearly all its time on workspace crates (11.5 minutes on aarch64 without release dependencies in its entry against 11.7 on x86_64 with them, run 42767547196). The release gate's x86_64 determinism cells share that key but only
restore it (`save-if`), so a fast debug cell cannot write the key before `test-suite` does. Its
arm64 determinism cells do save, because no other job builds on arm64: that entry is
what a new PR's arm64 cells warm-start from (the debug cell finishes first, so it is normally the
debug dependency set, and the arm64 release cells still build their dependencies cold). In
`pr-gate.yml` only the debug determinism cells save (`save-if: matrix.codegen == 'debug'`,
pinned by the checker): a PR run saves into its own scope, which no other PR reads, but the
workflow can be dispatched on `development`, and there a release cell that wrote the key first
would leave every PR restoring release dependencies its debug cells cannot use.
The macOS cell has no release-gate
counterpart: it reuses what an earlier run of the same PR saved, and builds cold on a new PR. rust-cache saves only on an
exact-key miss, so under a shared key the first job to finish writes the entry, and its profile's
dependency set is what that key holds until `Cargo.lock`, the toolchain or the environment changes. `golden-gen-hygiene` keeps its own
per-job key (it builds the separate `tools/golden-gen` workspace).

**Offline tool workspaces.** No cache may be relied on for a crate the root workspace does not use
(issue #610). Some oce-cxf tests build a tool workspace with `cargo run --offline --locked`: the
OpenModelica evidence checks of `fixture_structural_oracle`, which build
`tools/openmodelica-line-reference`, `tools/openmodelica-reliefs-reference` and
`tools/openmodelica-toggle-reference`, directly or through the tools' `assemble.sh` and
`verify_evidence.py`. Each tool has a `Cargo.lock` of its own, and a rust-cache entry holds only
what the root workspace uses. Its key hashes the root `Cargo.lock` and the members' manifests,
never a tool's, and its save prunes the registry to the root workspace's crates
(`cache-all-crates: false`).

After GitHub #605 moved the root to `rustix 1.1.6`, the two tools that pin 1.1.4 kept building only
while a run restored, through the restore-key fallback, an entry saved before that change. The
`release.yml` verify of tag `v0.5.0` (run 43129183575) uses no cache and failed 15 of these tests.
So did pr-gate run 43132109525 of a PR whose own `Cargo.lock` change meant a new key.

`scripts/ci/prefetch-offline-tools.sh` holds the list and runs `cargo fetch --locked --manifest-path`
for each tool. These run it as a step of their own, before the tests:
- pr-gate's `crate-tests`;
- release-gate's `test-suite`;
- `release.yml`'s verify;
- `.agents/gate.sh`, before its first oce-cxf step, which covers `gate (full)`.

The Windows cells run none of these tests, which are all `#[cfg(unix)]`, and need no prefetch.

`check-workflow-gates.sh` refuses a job in any workflow but `ci.yml` that runs the oce-cxf suite,
the workspace's, or a bare `cargo test` from the root without the prefetch before it (its fixtures
try the spellings and the masks). It also refuses a gate script without it. A new test that builds
another tool workspace offline adds the tool to the script's list. Without it, the test passes on a
warm cache and fails on a cold one, as these did. The two tools' lockfiles stay on rustix 1.1.4
until issue #604 moves them with the OpenModelica evidence their digests are bound into.

**mdBook.** The `docs` job and `docs-pages.yml` get mdBook only from `scripts/docs/install-mdbook.sh`,
which downloads the pinned upstream release archive, checks its SHA-256 against
`site/mdbook-checksums.txt`, extracts it, and checks the extracted `mdbook` executable against a second
digest in the same file (the second table, column 2 `<archive>:mdbook`) before installing it. Both
workflows first restore `target/mdbook-bin` with `actions/cache`, keyed on the runner OS and architecture
plus `hashFiles('site/mdbook-checksums.txt', 'scripts/docs/install-mdbook.sh')` and with **no
`restore-keys`**: an exact hit or nothing, so a binary cached for an older pin is never restored after
the pin is bumped (`check-workflow-gates.sh` fails closed on a `restore-keys` line or a cache step after
the install). The installer still runs after the restore and trusts a binary already at the destination
only if its SHA-256 equals the pinned binary digest, printing `already present and verified` and
skipping the download; a wrong or tampered cached file is re-downloaded and re-verified, never trusted,
so the checksum pin stays the integrity guarantee and the cache only removes the live GitHub release
download from the critical path (it returned HTTP 504 for over an hour on 2026-11-08). There is no
second download source on purpose: `cargo install mdbook --locked` would compile mdBook for minutes on
every cold run, and GitHub's API asset URL is served by the same CDN, so the cache is the mitigation. A
cold cache during an outage still fails, loudly.

**wasmtime.** The [blob round trip](#the-blob-round-trip)'s wasm32 leg and `gate (full)` get
wasmtime the same way, from `scripts/ci/install-wasmtime.sh`: version 50.0.1, the SHA-256 of each
supported platform's release archive (the digest GitHub records for the asset) and of the binary
inside it pinned in the script. The wasm32 leg restores it first from an `actions/cache` keyed on
`hashFiles('scripts/ci/install-wasmtime.sh')` with no `restore-keys`, and trusts it from the cache
only when the binary's digest is the pinned one. The job prints `wasmtime --version` before the run.
Bump the version by replacing every digest in the script in one change. Without an argument the
installer writes to a user cache directory, `$OCE_WASMTIME_DIR` when set and otherwise
`${XDG_CACHE_HOME:-$HOME/.cache}/open-control-engine/wasmtime-bin`, and `.agents/gate.sh` asks the
installer for that path (`--print-destination`) instead of repeating it. It was
`target/wasmtime-bin` until issue #545, which the `cargo clean` at every merge boundary deleted, so
the next `roundtrip` or `full` run failed on "wasmtime is not installed". `gate (full)` installs to
the default; the per-PR wasm32 leg still passes `target/wasmtime-bin`, the path its cache step
restores on a runner whose tree is new every job.

Branch protection on `development` makes the PR run the only way in: a pull request is required,
the `CI OK` status is required, and the branch must be up to date with `development` before it
merges. GitHub PR runs test a synthetic merge of the PR head into its base, and the up-to-date
rule makes that tested merge the commit that lands; a PR that falls behind is updated and re-run
before it merges. Now that lanes exist, the next step is a merge queue, which tests each merge
group once instead. Until then a leverage-only PR is still serialized by the up-to-date rule: it
skips the determinism lane, but it must be updated and re-run whenever any other PR lands first,
and that re-run classifies only its own changes.

Docs-site validation (`.github/workflows/docs-pages.yml`) is path-filtered to `docs/**`,
`README.md`, `scripts/docs/**`, `scripts/authority_claims/**`, `site/**` and its own workflow
file. It reports its own `build docs artifact` status and is **not** part of `CI OK`, so a PR that
does not touch those paths shows no docs check at all. A push to `main` touching those paths also
deploys the site to GitHub Pages.

### Platform cells: macOS and Windows

Two cells run outside Linux (issue #394). The macOS cell runs per PR, with the determinism lane,
under `pr-gate.yml`'s `CI OK`. The Windows cell runs in `merge-gate.yml`, on every merge into
`development`, where it is now the only job, and in `release-gate.yml`, nightly and on release
PRs (issues #502, #545 and #556). They
are observations of the platform, not qualification of it.

| Cell | Runner | What runs | What it proves | What it does not prove |
| --- | --- | --- | --- | --- |
| `portable snapshot (macOS arm64, debug)` and its `portable snapshot comparison` fan-in | `macos-15` (arm64); the fan-in on `ubuntu-latest` | `cargo nextest run -p oce-api -p oce-blocks -p oce-expr --profile ci` (debug codegen), emitting the portable and target-bound state vectors; the fan-in compares them with the Linux x86_64 and aarch64 debug cells' vectors | the determinism subset passes on macOS arm64; the populated portable engine-state snapshot is byte-identical to Linux x86_64 and aarch64; the target-bound snapshot differs from both Linux architectures' | release codegen on macOS; any strict-bit or transcendental result (no strict-bit suite runs, and macOS keeps the 1e-12 aligned band, see [strict-bit evidence](strict-bit-evidence.md)); the rest of the workspace on macOS |
| `std build and nextest (Windows x86_64, debug)` | `windows-latest` (x86_64, Git Bash) | `cargo build --workspace --locked`, then `cargo nextest run --workspace --profile ci` without the five strict-bit binaries `scripts/ci/strict-bits-cell.sh` runs (`strict_bits` and the four aligned per-block suites) | the workspace builds on Windows and every other workspace test passes there, including the bit-exact goldens on a checkout whose git defaults to CRLF (`.gitattributes` keeps them LF) | any determinism or snapshot claim; release codegen; doctests; any strict-bit or transcendental result |

The macOS cell lives in `pr-gate.yml`, not in a separate workflow, because `CI OK` is the only
required status and a job in another workflow cannot appear in its `needs:`; a separate workflow
would either be unrequired or add a second required status. It runs only on a PR that changes the
engine, a gate script or a workflow, and has no schedule of its own. The Windows cell moved out of
the per-PR gate with issue #502: at four to eight minutes it was the longest per-PR job after
the release determinism cells, and it makes no determinism claim. It stayed out when those cells
came back (issue #556): at 5 min 33 s in run 42948818576 it would be the per-PR long pole, and
whether it returns is a separate decision. A change that breaks only on
Windows is therefore found on its merge commit, not on the PR. `.agents/gate.sh` cannot run either
cell (wrong operating system) and says so in its closing report.

## One command, one source of truth

[`.agents/gate.sh`](../.agents/gate.sh) is the only place the gate's command list is written down.
Every other document in this repo — including this page — points at it rather than restating it,
because nine divergent prose copies existed before the script was written and two of them were
materially weaker than CI (see the script's header). There are five gate modes, four of them
rows of the local proof, and three focused ones:

```
bash .agents/gate.sh docs         # docs        — a docs-only PR: the always-on checks, no cargo build
bash .agents/gate.sh leverage     # leverage    — adds the leverage lane and the README lane
bash .agents/gate.sh              # light       — adds the engine lane, in debug codegen
bash .agents/gate.sh determinism  # determinism — adds the determinism subset and the strict-bit
                                  #               step under release codegen; not a row, a way to
                                  #               reproduce a red release cell
bash .agents/gate.sh full         # full        — adds the workspace suite (debug and release), the
                                  #               doctests and the blob round trip; CI runs it nightly
bash .agents/gate.sh nostd        # nostd       — the bare-metal cell alone (light and up run it too)
bash .agents/gate.sh qemu         # qemu        — the nightly emulation check (needs QEMU installed)
bash .agents/gate.sh roundtrip    # roundtrip   — the blob round trip: record on this host, replay it
                                  #               here and on wasm32-wasip1 (full runs it too)
```

### The local proof

What an author runs first-hand, on the final head, and pastes the tail of into the pull request
is matched to the lane the PR runs (issue #545, owner decision of 2026-11-10). Before it, every
PR owed `bash .agents/gate.sh full`: 30.1 minutes cold on an idle 10-core arm64 Mac, 21 of them
building release-codegen test binaries, about three times per PR.

| The PR's changed paths | Local proof | What it adds to the row above |
| --- | --- | --- |
| docs only | `bash .agents/gate.sh docs` | fmt, the file-size cap, the no-secret scan, the package/authority/product contract and its controls, the gate fixtures and the documentation checks (staging, mdBook build, links): the always-on jobs of `pr-gate.yml`. Nothing is compiled. |
| `crates/leverage/**`, docs, the root `README.md` | `bash .agents/gate.sh leverage` | the leverage lane (each member's fmt, clippy, rustdoc, nextest and cargo-machete) and the README lane (the Quickstart byte-pin and its execution) |
| anything else (the engine lane), the core four, `oce-cxf` and `oce-api` included | `bash .agents/gate.sh` | everything that compiles the engine, in debug codegen: the database-free check, the golden-gen firewall, the facade compiler contract, cargo-machete, clippy, build, the bare-metal cell, rustdoc, cargo-deny, the determinism subset, the strict-bit corpus and the two input-hygiene audits |
| a pin advance (`third_party/**`, `tools/reference-catalog/**`, or either of the two pin-guard files `scripts/ci/proof-row.sh` lists), `.agents/gate.sh` itself, `.config/nextest.toml` or `scripts/ci/install-wasmtime.sh` | `bash .agents/gate.sh full` | the determinism subset and the strict-bit step under release codegen, the workspace suite in debug and release codegen, the doctests, the blob matrix artifact and the blob round trip |

The first three rows are the owner's table (issue #545). The last is the owner's too: confirmed
on 2026-11-10 (comment on #545), with the dispatch rule below, after the review of GitHub #547
proposed it, and ruled on again on 2026-11-10 and 2026-11-11 (issue #555). It exists because
the script's `full`-only steps run in no lighter mode, which is also why the two files only those
steps exercise are in it, and because a pin advance has tests that no per-PR job runs:
`oce-conformance`'s `conformance` binary, three of whose modules read the vendored tree and the
reference catalog; that crate's `clean_room` unit tests and `open_modelica_nand` binary, which do
not read the vendored files but pin the vendored `Nand.mo` by path, line and equation; and the
doctests. An earlier reason given here, that the G36 half of the pin guard runs
only in the workspace suite, was wrong: `oce-cxf`'s `g36_catalog_tests` run per PR in
`crate tests`, and `oce-blocks`'s `registry::catalog_tests` in the determinism subset.

**There is no release-codegen row** (issue #556, owner decision of 2026-11-10). From issue #545
until then a fourth row, `bash .agents/gate.sh determinism`, was owed by a change to the core
four, `oce-cxf`, `oce-api`, `Cargo.lock`, the root `Cargo.toml`, `rust-toolchain.toml` or
`.cargo/config.toml`, because no per-PR job ran the determinism subset under release codegen.
`pr-gate.yml` runs it in all four native cells again, so those changes owe the engine row and
nothing more, and `proof-row.sh` never names `determinism`. The mode remains.

- **Which row.** The lane boundaries are the classifier's ([Lanes](#lanes-what-a-pr-runs)):
  documentation a test reads, a gate script, a workflow and a tool config are engine changes.
  `scripts/ci/proof-row.sh` reads changed paths on stdin and prints the row
  (`git diff --no-renames --name-only origin/development... | bash scripts/ci/proof-row.sh`); it
  holds the exact path list of the last row (its two source files are the pin-guard files
  `crates/oce-cxf/tests/third_party_manifest/` and
  `crates/oce-blocks/src/registry/catalog_guard_support.rs`), and names the heaviest row for a
  diff it cannot read. Those two are not every file that writes a pinned upstream commit:
  `crates/oce-cxf/src/g36_catalog_guard_data.rs`, five files under `tools/golden-gen/src/` and the
  `oce-vocab` generator do as well, and the row does not list them because a pin advance always
  moves `third_party/**`. Its fixtures (`scripts/ci/test-proof-row.sh`) run on every PR and fail
  if a listed path stops existing, if a pin constant leaves one of the two files, or if the set of
  files that hold one changes.
- **The script says which row a run was.** Every gate run ends with a `LOCAL PROOF` block: the row
  the run covered, and the row the checkout needs, from the paths that differ from the merge base
  with `origin/development` (uncommitted and untracked files included). When the mode is lighter
  than the row, the block says so in the tail the author pastes. It is advice and never changes
  the exit status: a CI checkout has no `origin/development`, and a stale one must not fail a gate.
- **A heavier row always satisfies a lighter one**, because each mode runs everything the modes
  before it run. `scripts/ci/test-gate-modes.sh` holds that: it executes the script in each of the
  five modes with its expensive children stubbed, reads which steps each mode reaches, and fails
  unless every mode runs all of the one before it and more; four mutated copies prove it can fail.
- **A change to `release-gate.yml`, or to anything else only `full` or the nightly run
  executes, is not covered by any row.** Dispatch the workflow on the branch
  (`gh workflow run release-gate.yml --ref <branch>`) and wait for it to finish green before the
  PR merges. The `CI OK` that run posts is not the one that decides the merge
  ([above](#what-runs-after-the-merge)). Named, so that "anything else" is not left to the reader:
  `qemu.yml` and `scripts/ci/emulate-firmware.sh` run only nightly, in a workflow of their own,
  so a change to them runs `bash .agents/gate.sh qemu` or dispatches `qemu.yml`. `merge-gate.yml`
  can be dispatched but need not run before the merge: its one job is byte-identical to the
  Windows cell a dispatched `release-gate.yml` runs, and the checker holds that. Two files that
  only `full` exercises are in the `full` row instead (owner ruling of 2026-11-11): the
  `ci-release-workspace` profile lives in `.config/nextest.toml`, and the default destination of
  `scripts/ci/install-wasmtime.sh` is used only by `full` and `roundtrip`.
- **A rebase onto `development` with no conflicts** needs `CI OK` reported on the new head, not a
  local re-run. **A rebase with conflicts, or a fix round,** re-runs the row's proof.
- **No row runs every crate's suite.** The author still runs the tests of the crates the PR
  changes: `cargo nextest run -p <crate> --locked --profile ci --no-tests=fail`, and
  `cargo test -p <crate> --doc --locked` for its doctests. The engine row runs the `oce-api`,
  `oce-blocks` and `oce-expr` suites and the scoped strict-bit binaries; CI adds `oce-graph`,
  `oce-validate`, `oce-model`, `oce-cxf` and `oce-store` per PR; every other crate's suite first
  runs in CI in the nightly release gate.
- **`full` is also what CI executes nightly.** Only the last row owes it: a pin advance, for the
  tests named above, and a change to the script, to `.config/nextest.toml` or to
  `scripts/ci/install-wasmtime.sh`, because nothing else runs the script's `full` block before
  the merge.

What the rows cost, cold (`cargo clean` first, `target/` empty), on a 10-core arm64 Mac,
measured on 2026-11-10 at the head that introduced them:

| Row | Command | Cold wall-clock | Measured |
| --- | --- | --- | --- |
| docs | `bash .agents/gate.sh docs` | 4.8 min (285 s) | with two unrelated builds running on the machine (load average 5 to 26); nothing is compiled, and mdBook was already installed |
| leverage | `bash .agents/gate.sh leverage` | 5.9 min (352 s) | under the same load |
| engine | `bash .agents/gate.sh` | 7.1 min (423 s) | under the same load |
| full | `bash .agents/gate.sh full` | 16.1 min (966 s) | on 2026-11-11, from an empty `target/` on an otherwise idle machine, at the head of GitHub #557, after GitHub #546, #551, #553 and #554 cut the release test binaries. The same mode took 30.1 min on `fdd70c0` before them, 15.9 min of it the release-codegen workspace suite |

The first three are upper bounds, since the machine was busy. About three minutes of each was then
the gate's own fixture suites, which run in every mode (`test-workflow-gates.sh` alone took 3 min
22 s under that load): they are the floor of every row, and the reason the docs row is not seconds.
Since issue #627 that suite runs its cases concurrently, as many at once as the machine has CPUs
(or `OCE_FIXTURE_JOBS`). On 2026-11-12 it took 1 min 20 s for its 348 cases on the same 10-core
Mac, under a load average of 30, so the floor of each row is lower than the table shows.

CI does not merely mirror that script, it **executes** it: `gate (full)` at
[`.github/workflows/release-gate.yml`](../.github/workflows/release-gate.yml), nightly on the
`development` tip, on release PRs and on dispatch. No job in
[`.github/workflows/pr-gate.yml`](../.github/workflows/pr-gate.yml) does (`gate (light)` ran it on
every PR that changed a gate script, workflow or tool config until issue #502). So CI exercises a
command in the script on the night after the merge, not on the pull request; before the merge, the
author's own local proof on the final head is the run. Read the CI run as coverage, not as
parity, and note that the implication does not run the other way: a job may run a step the script
does not. The Quickstart-executes step
was exactly that for a while — a required check no local run of the script performed — and an
earlier revision of this paragraph used a numeric citation that stopped one line short of it.
Nothing verifies mechanically that the two files still list the same commands. That check was
attempted and withdrawn, and the `gate` job's header in the dormant `.github/workflows/ci.yml`
records why —
every design either compared argv strings that `RUSTFLAGS=--cap-lints=allow` leaves byte-identical
while neutering clippy, or reimplemented enough of the workflow `if:`/`needs:`/matrix semantics to
become its own untested gate.

The script's steps group into: formatting, file-size and secret hygiene; the repository-invariant
gates (the default build links no database or async runtime; the golden generator cannot bless its
own output as the oracle; package, feature, and publication selection is closed); behavior fixtures
for those gates, because a gate that cannot fail is not a gate; the documentation checks; the
leverage and README lanes; build, clippy and rustdoc under
`-D warnings`; supply-chain checks; the determinism subset, in debug and, in `determinism` and
`full`, in release codegen; and two fixture input-hygiene audits. A
failing step never aborts the run, so one round trip reports every problem instead of the first
(see the script's `step` function).

The [authority index and generated projection](authority-claims.md) add a fast, bounded consistency
check and hostile controls. Native numeric observers run inside the existing `oce-api`/`oce-blocks`
subset; package/public/catalog validators retain ownership. The full gate also executes those owners.
This does not check arbitrary Markdown claims or workflow parity, and regeneration is never gated in.

### The bare-metal cell

The bare-metal cell (`bare-metal cell (no_std core, firmware image, size budget)`, job id
`no-std-build`, determinism lane, issues #397 and #402) proves the `no_std` claim per PR by
**build and size**; executing the image on a target is the [nightly emulation
run](#the-nightly-emulation-run)'s, never a PR's. Its steps are thin calls to scripts, so
`bash .agents/gate.sh nostd` runs exactly the job (and `bash .agents/gate.sh` runs it inside the
engine lane):

- **`scripts/ci/check-no-std.sh`** builds `oce-model`, `oce-expr`, `oce-blocks` and `oce-graph` with
  `--no-default-features --features alloc` on `thumbv7em-none-eabihf` (Cortex-M4F) and
  `riscv32imac-unknown-none-elf` (RV32IMAC), bare-metal targets whose standard library does not
  exist, and on `wasm32-unknown-unknown`; runs clippy `-D warnings` on the same configuration; then
  reads each crate's `cargo tree -e normal,no-proc-macro` per target and fails on any package with
  its `std` feature on or any host-only crate (the workspace's `std` crates, `serde_json`, `rustix`).
  A `std`-only dependency, a `std::` path or a `std`-only `f64` method fails the build; a `std`
  feature that still compiles (on `wasm32-unknown-unknown`, where `std` exists) fails the scan.
  `scripts/ci/test-check-no-std.sh` proves both halves can fail: synthetic graphs through the scan,
  `extern crate std;` in a workspace copy's `oce-graph` (the build fails on thumbv7em), and
  `serde/std` turned on through a copy's `oce-model` (build and clippy pass on wasm32; the scan
  names `serde`). The shape is rusty-bacnet's `check-no-std.sh`.
- **The math rule** (issue #399): the clippy `disallowed-methods` rule binds each core crate's
  `src/` tree, unit tests included — `tests/` files are separate crates outside it, where the `std`
  methods serve as the oracle. `scripts/ci/test-math-lint.sh` re-probes which `f64` methods `core`
  has, checks the `clippy.toml` list is total and the scope holds, and requires an `x.sin()`
  mutation in oce-blocks and an `f.sqrt()` mutation in oce-expr, in a workspace copy, to fail clippy.
- **The `oce-expr` unit suite** runs on the host without the `std` feature, on one test thread,
  because that build replaces the thread-local evaluation-fuel meter with a program-wide one.
- **The blob execution entry** (issue #401): `scripts/ci/check-no-std-entry-closure.sh` requires the
  `no_std` `oce-graph` closure to hold no `oce-cxf`, `oce-api`, `serde_json` or `std` (its controls
  must trip on the default build and on `oce-api`'s closure), and `oce-graph`'s `blob_execution` and
  `blob_state_words` suites run against the `no_std + alloc` build linked into a `std` test binary,
  printing the allocation census (zero allocations per frame over every golden) and checking the
  run digests the facade's dual-path test reproduces.
- **The firmware image** ([`firmware/link`](../firmware/link/), package `oce-firmware-link`):
  `scripts/ci/check-firmware-link.sh` scans the image's own graph, lints it, builds its
  `blob-frames` image for both bare-metal targets with the root `firmware` profile
  (`opt-level = "z"`, LTO, one codegen unit, `panic = "abort"`) and runs
  `scripts/ci/firmware_size.py` on each ELF: `.text`, `.rodata`, `.data`, `.bss`, flash and RAM
  against [`firmware/link/size-budget.toml`](../firmware/link/size-budget.toml) (measured + 12 %,
  rounded to 16), with counts of the `core::fmt`, 64-bit division and memmove routines linked in,
  failing over budget and printing the delta. Raising a budget is a reviewed change;
  `check-firmware-link.sh --suggest` prints a table. `python3 -I scripts/ci/test-firmware-size.py`
  is the checker's own controls (it and the checker are copied from rusty-bacnet). Building the
  image proves the core links into a bare-metal binary with no symbol only a hosted target
  provides, which a library build never shows. The image embeds the `multizone_vav_supply_fan`
  blob golden, decodes it with `oce_graph::blob::Executable` into a 64 KiB arena, runs the shared
  160-frame schedule and compares its run digest with
  `crates/oce-graph/tests/fixtures/blob_frame_digests.tsv`. `cargo test -p oce-firmware-link` runs
  that same code on the host: every golden reaches its table digest through it, its schedule equals
  `oce-graph`'s test schedule bit for bit, and the host reference runner prints the lines the
  image prints.

The scripts install nothing: when a target is missing, the script names the `rustup target add`
command and fails. The `gate (full)` job that executes `.agents/gate.sh` installs
all three targets first. What the build means is in
[the architecture](architecture.md#no_std--alloc-the-core-four). The image has no `unsafe` code:
its allocator (`linked_list_allocator`'s spin-locked heap) takes its arena from a `static_cell`, and
`check-workflow-gates.sh` requires `#![forbid(unsafe_code)]` in every firmware lib and bin root.

### The nightly emulation run

`qemu.yml` runs `scripts/ci/emulate-firmware.sh` nightly and on manual dispatch, on `ubuntu-24.04`
with QEMU from the runner's apt archive (`qemu-system-arm`, `qemu-system-misc`; the job prints the
package versions). It builds the image for both targets with the `firmware` profile — the build the
bare-metal cell measures — runs it under QEMU with semihosting (`netduinoplus2`, an STM32F405
Cortex-M4F, for `thumbv7em-none-eabihf`; the riscv32 `virt` machine with `-bios none` for
`riscv32imac-unknown-none-elf`, since `sifive_e` has 16 KiB of RAM, too little for the arena), and
diffs every line against the host reference runner (`cargo run -p oce-firmware-link --bin
blob-frames`): the fixture, the run digest after each of the 160 frames, the final digest against
the table, and the verdict. The image exits 0 `PASS`, 1 `MISMATCH`, 3 `FAULT` (the engine refused)
or 101 on a panic; a hang or CPU fault fails at the per-image timeout. It is not part of any
`CI OK`: emulation never gates a PR, and `check-workflow-gates.sh` refuses `qemu` in the gating
workflows and a `pull_request` or `push` trigger in `qemu.yml`. `bash .agents/gate.sh qemu` runs
the same script locally (macOS: `brew install qemu`). The cadence is this repository's decision;
rusty-bacnet, whose image layout and QEMU invocation this follows, runs its emulation by hand.

## Dev-light, release-heavy

**A green PR is not evidence that the change's own tests pass.**

On every engine change (see [Lanes](#lanes-what-a-pr-runs)), the per-PR gate into `development`
runs the state-determinism subset for **`oce-api`, `oce-blocks`, and `oce-expr`**. That is the
`determinism-matrix` job in `.github/workflows/pr-gate.yml`: four
native cells, x86_64 on `ubuntu-latest` and aarch64 on `ubuntu-24.04-arm`, each in debug and
release codegen. Each
cell refuses a runner of the wrong architecture, runs the three-crate
subset and emits populated portable and target-bound state vectors. The
`determinism-compare` fan-in (one leg per architecture) downloads all four cells' vectors,
requires portable bytes to match across architectures and target-bound bytes to differ, both to
match across debug and release codegen within an architecture, and then
parses the **other** architecture's target-bound snapshot and refuses it through the public restore path
— x86_64 refuses aarch64's, aarch64 refuses x86_64's — natively, from the oce-api unit-test binary
its own cell built and archived. Each cell also writes the 46 compiled-schedule blobs it
encodes (`OCE_BLOB_OUT`, after its `blob_goldens` test has compared them with the checked-in
goldens), and the fan-in requires those bytes identical in all four cells
and each file 123,329 bytes, the goldens' total, so a truncated
artifact cannot pass ([compiled-schedule blob](compiled-schedule-blob.md), GitHub #400).
`bash .agents/gate.sh full` writes the artifact the same way and checks its size.
Nothing is emulated; every test in the subset runs in every cell.

**Release codegen is per-PR again** (issue #556). From issue #502 the release cells of this
matrix and the debug-against-release comparisons ran only after the merge: a release cell was the
per-PR long pole, 11 to 18 minutes of which the tests ran for under half a minute, because every
integration-test file was its own release binary and each one linked the whole dependency graph.
GitHub #546, #551, #553 and #554 built those files as a few binaries per crate, the cells fell to
174 s and 181 s (run 42948818576), and they came back. So a pull request now observes, before it
merges, portable and target-bound bytes equal across codegen within each architecture, the blobs
identical in all four cells and the four round-trip recordings identical; nothing about
determinism is left to the merge commit. `release-gate.yml` runs the same four cells nightly.
`bash .agents/gate.sh determinism` runs the subset under release codegen on one host, to
reproduce a failure: one architecture, and no comparison between a debug and a release artifact.

The gate script runs the test commands locally and adds two named
`oce-cxf` test binaries, which are input hygiene rather than
engine coverage: the port-order audit sweeps 47 CXF documents, of which 46 are Guideline 36 catalog
fixtures and one is a resolver contract; the structural oracle compares the catalog fixtures it can
pair with vendored modelica-json translations
(see the gate script's fixture input-hygiene section). That oracle compares document structure — instances and undirected
edges — not simulated behavior.

The scoped `oce-conformance` **strict-bit subset also runs per-PR**: `strict_bits` plus the four
affected per-block suite binaries, in four native cells (both architectures, debug and release
codegen; this matrix did not move), with two independent captures
per cell and a fail-closed eight-capture comparison in the `strict-bit-compare` fan-in. That
per-PR matrix **is** the native evidence for exact comparison of the 21 pinned Real cases on Linux
([strict-bit evidence](strict-bit-evidence.md#per-pr-native-matrix)); no stored receipt admits
anything, and the fan-in uploads `matrix.json` and a candidate receipt-log record as the
`strict-bit-matrix` artifact. Unqualified targets retain the unchanged 1e-12 aligned band. This is
not the whole conformance suite or a libm accuracy claim.

**A runner type that is unavailable fails the check closed.** A cell that cannot get an
`ubuntu-24.04-arm` runner stays queued, its fan-in never starts, and `CI OK` never reports success;
there is no fallback to emulation and no skipped cell.

The remainder waits for the nightly release gate. A change outside the named test subsets can show
a fully green PR having executed none of its own tests, and its [local proof](#the-local-proof)
row does not run them either.
Before claiming tests pass, run the suites of the crates the change touches first-hand
(`cargo nextest run -p <crate> --locked --profile ci --no-tests=fail`) and read the tail.

### The blob round trip

The `blob_round_trip` suite in `oce-conformance` (issue #403) loads each of the 46 G36 fixtures
through the facade, encodes its blob, decodes it with the `no_std` execution entry, runs the same
160 frames through both and compares every boundary output, every warning and the raw state words
after every frame, bit for bit
([compiled-schedule blob](compiled-schedule-blob.md#the-blob-round-trip)). It has three legs and
one recorded artifact, all under `CI OK`:

| Leg | Where | What it does |
| --- | --- | --- |
| x86_64 (records) | every `determinism-matrix` cell, before the subset; the x86_64 debug cell's recording is the artifact | runs the suite natively with `OCE_ROUND_TRIP_OUT=target/determinism/round-trip`: 46 blobs and 46 traces, uploaded with the cell's vectors |
| aarch64 (replays) | `determinism-compare`, aarch64 leg | requires the cells' recordings identical (`diff -rq`, 92 files each, all four cells), then runs the suite its own debug cell archived with `OCE_ROUND_TRIP_EXPECT` pointing at the x86_64 debug recording: it decodes the blobs x86_64 encoded and compares its run with the recorded one. The x86_64 leg runs the same replay. |
| wasm32 (replays) | `blob round trip (wasm32-wasip1 under wasmtime)`, on `ubuntu-latest` after the matrix | compiles the suite for `wasm32-wasip1` and runs it under the pinned wasmtime through the cargo target runner in `.cargo/config.toml` (`scripts/ci/wasmtime-runner.sh`, which shows the guest the workspace at its host path and passes only the two round-trip variables and `RUST_BACKTRACE`), against the same recording |

Every leg also compares the facade with the entry on its own target, the blob bytes with the
checked-in goldens and the entry's run with the pinned run digests. `wasm32-unknown-unknown` stays a
build-only target in the [bare-metal cell](#the-bare-metal-cell); a headless-browser run belongs to
the `oce-wasm` crate (M4). `bash .agents/gate.sh roundtrip` (and `full`) records on the local host
and replays the recording there and on `wasm32-wasip1`; it fails, naming the `rustup` command,
when the target is missing, and it runs the wasmtime installer itself, which keeps an installed
binary only if its digest is the pinned one, so a different `wasmtime` on PATH is never used. The second native architecture is CI's alone.

## Every pull request runs the gate

The per-PR gate runs on every PR, drafts included; there is no draft carve-out. The only job-level
`if:` a `pr-gate.yml` job may carry is its lane filter, and `CI OK` reads any other skip as a
failure.

**The step-condition rule.** No job or step in `pr-gate.yml`, `merge-gate.yml` or
`release-gate.yml` may set `continue-on-error`. In all three, a
step-level `if:` may take exactly two shapes:

- a static matrix selector — `matrix.<key> == '<value>'`, optionally joined by `&&` (for example
  `matrix.codegen == 'debug'`) — which picks the work a cell does from its fixed identity, and which a
  later step of that cell backs by requiring the selected work's outputs;
- `${{ !cancelled() }}`, which only lets an evidence upload run after an earlier step failed.

Anything else — a lane output, an event name, `false`, `always()`, `||` — fails
`check-workflow-gates.sh`, with a failing fixture for each shape. The reason: `CI OK` sees job
results only, so a step skipped inside a green job would be invisible to it.

A PR with no checks still looks a lot like a PR with no failing checks — confirm `CI OK` actually
reported, and on a docs-only or leverage-only PR expect the determinism jobs to show as skipped, not
green.

## cargo-deny is not skippable, but advisories do not gate a PR

The standalone `cargo-deny` job in `pr-gate.yml` runs cargo-deny's bans, licenses and sources checks
on every PR that changes anything outside the documentation and `crates/leverage/` — and any
dependency change rewrites `Cargo.lock`, which always counts as an engine change — and the gate
script runs them too (see the script's cargo-deny step).

`advisories` is a different story, and the carve-out belongs next to the claim. It is deliberately
excluded from the script — it needs network access and a writable advisory database, neither of
which a sandboxed lane has. It runs daily in [advisories.yml](../.github/workflows/advisories.yml), and
nightly and on release PRs in the `release-gate.yml` cargo-deny job. `advisories.yml` has no `pull_request` trigger at all, so
a PR into `development` that introduces a dependency with a known RustSec advisory merges green and
is caught by the next scheduled run, not by its own gate.

## What the release gate adds

`release-gate.yml` fires nightly against the `development` tip, on manual dispatch, on
`development` → `main` PRs and on pushes to `main` (see its trigger block). It does not fire on a
push to `development`: `merge-gate.yml` does, with the Windows cell
([what runs after the merge](#what-runs-after-the-merge)). The three workflows are disjoint by
event and base branch, so no two fire on one event. It re-runs the light correctness gates against
the tip it tests, runs the determinism matrix again, and adds five things no pull request runs:

| Step | What it covers | Where |
| --- | --- | --- |
| workspace nextest | every unit and integration test in the workspace | `release-gate.yml`, `test-suite` job, unit + integration step |
| `cargo test --doc` | doctests — nextest cannot run them, so this is a separate step | `release-gate.yml`, `test-suite` job, doctest step |
| the executed gate script | `bash .agents/gate.sh full`, the only CI execution of the script | `release-gate.yml`, `gate` job |
| workspace nextest, release codegen | release panic-freedom, `debug_assert` paths stripped; the `ci` runner policy, inherited by the `ci-release-workspace` profile | `release-gate.yml`, `gate` job: the script's `full` mode runs it, and the job requires the `ci-release-workspace` JUnit report afterwards |
| determinism matrix, four native cells | the three-crate subset and the blob round trip under release codegen on x86_64 and aarch64, beside the debug cells; the fan-in compares debug against release (portable and target-bound bytes, blobs, round-trip recordings) and repeats the cross-architecture checks | `pr-gate.yml` (per PR) and `release-gate.yml`, `determinism-matrix` and `determinism-compare` jobs |
| Windows cell | the workspace build and suite on Windows, without the strict-bit binaries | `merge-gate.yml` and `release-gate.yml`, `std-build-windows` job |

The release-codegen workspace suite has one build, not two (issue #545). `test-suite` used to run
it and `gate (full)` ran it again inside the script, each compiling the whole workspace under the
release profile: 55.7 minutes of `gate (full)` in run 42767547196. `test-suite` now runs the debug
suite and the doctests and requires the `ci` JUnit report; `gate (full)` clears the
`ci-release-workspace` report, executes the script and then requires that report. The profile
exists for that one run. `full` mode also runs the determinism subset and the strict-bit step
under release codegen, which write the `ci-release` report, so a requirement on that file was
met with the workspace run removed from the script (found in the review of GitHub #547, by making
the step unreachable in a copy). `check-workflow-gates.sh` pins the three job steps in order; the
`full` block of `.agents/gate.sh`, whose first five lines after the `if`, comments aside, must be
exactly the debug suite, the release suite under that profile and the doctests, so a step that is
dropped, wrapped in a condition or prefixed with one fails; and that no other line of the script
or of any gating workflow names the profile. What a report proves is narrow: the command started
and nextest wrote a report. The script's exit status is what says the suite passed.

`--no-tests=fail` is explicit on the nextest steps: a run that discovers zero tests hard-fails
rather than passing, which catches tests that silently stop compiling or being found.

### Nextest policy and reports

Local setup and CI pin cargo-nextest `0.9.143`; `.config/nextest.toml` also declares that version as
both required and recommended, so an older local binary exits before testing. The `default` profile
is fail-fast. Automated debug runs use `ci`; release-codegen runs use `ci-release`, which inherits
the same retries, timeout, leak, and reporter policy instead of copying it, and the one
release-codegen run of the whole workspace uses `ci-release-workspace`, which inherits it too and
differs only in where its report goes. Those four are the only profiles.

Retries are zero and a flaky pass is still a failure. Ordinary tests terminate after 120 seconds. A
run stops
after 15 minutes, and a child process retaining inherited output handles for more than two seconds
fails as a leak. CI writes Jenkins-compatible JUnit XML to `target/nextest/<profile>/junit.xml` and
requires every expected report to exist and be non-empty; the reports are no longer uploaded as
artifacts. Every cell, both architectures, uses `.config/nextest.toml`.

Partitioning is deliberately off: compilation dominates and test execution takes seconds. In run
42462665456 the x86_64 release determinism cell, then the per-PR long pole (and for that reason a
post-merge job from issue #502 until issue #556), spent 13m05s in its nextest
step of which the 1,089 tests ran for under ten seconds; the rest was building the `oce-api` test
binaries under the release profile (thin LTO, one codegen unit). nextest's `--partition` splits test
execution, not the build, so it would not shorten that cell, and each determinism cell must execute
the complete selected set under its own architecture and codegen mode anyway. Build archives (`cargo nextest archive`) are used only to hand a
test binary that a cell already built to its fan-in job — the oce-api unit tests for the
foreign-snapshot restore and the `strict_bits` binary for the eight-capture comparison — so the
fan-ins compile nothing. The determinism and strict-bit cells of one (runner, codegen) pair stay
separate jobs rather than sharing one archive: their test binaries are disjoint (`oce-api`,
`oce-blocks` and `oce-expr` tests against the `oce-conformance` strict-bit binaries), so a shared
archive could only reuse library builds. Measured in run 42462665456, the strict-bit cells finish in
0m33s (debug) and 1m20s–1m23s (release) beside determinism cells of 0m55s–1m08s and 8m24s–13m24s,
so folding one into the other could only lengthen the long pole. Experimental record/replay is also off in CI; enabling a feature
that nextest still marks unstable would make the gate depend on a non-stable format. Test groups and
thread reservations remain available when measurement identifies a shared resource or heavy test;
none is known today.

There is no public API surface baseline before 1.0. The engine is pre-1.0 and makes no
backwards-compatibility promise, so the checked-in surface snapshots, their nightly toolchain and the
compatibility records were retired (GitHub #388); a surface baseline returns with the first stable
release. Until then the compile-time signature guards in `crates/oce-api/src/guards.rs`, which run
in every per-PR build, are the only pinned surface.

## What CI cannot observe

- **Operating systems other than Linux.** Every gating workflow — `pr-gate.yml`,
  `merge-gate.yml`, `release-gate.yml`, `advisories.yml`, and `docs-pages.yml` (per-PR on `docs/**`, `README.md`, `scripts/docs/**`,
  `scripts/authority_claims/**`, and `site/**`) — runs on GitHub-hosted Linux runners.
  Cross-*architecture* is covered natively for the determinism and strict-bit subsets — x86_64 on
  `ubuntu-latest`, aarch64 on `ubuntu-24.04-arm`, debug and release, per PR. The exceptions are the two
  [platform cells](#platform-cells-macos-and-windows): the determinism subset in debug on macOS
  arm64 (per PR), and the workspace build and suite (without the strict-bit binaries) in debug on
  Windows (in the merge gate).
  Every other job runs on Linux x86_64 only: nothing runs release
  codegen or doctests on macOS or Windows, and no strict-bit result is observed on either.
- **Windows and the executed gate script, before a merge.** No per-PR job builds on Windows or
  executes `.agents/gate.sh`. For Windows the merge commit is the first observation, by a run
  that does not block the merge it reports on; the script is first executed by CI the following
  night. (Release codegen of the determinism subset was on this list from issue #502 until
  issue #556 returned its cells to the per-PR gate.)
- **The workspace suite, on the day of a merge.** Every crate's own tests, the doctests, the
  release-codegen workspace suite and the executed gate script run nightly (issue #545), so a
  merge that breaks a suite the per-PR gate does not name is found the following night unless
  its author ran that crate's suite, as [the local proof](#the-local-proof) requires.
- **WebAssembly beyond one runtime.** The blob round trip's wasm32 leg runs `wasm32-wasip1` under
  one pinned wasmtime release (Cranelift, x86_64 host). No browser engine, other runtime or
  `wasm32-unknown-unknown` build is executed per PR.
- **Execution on a bare-metal target, per PR.** The [bare-metal cell](#the-bare-metal-cell)
  builds the firmware image and measures it; only the [nightly emulation
  run](#the-nightly-emulation-run) executes it, under QEMU, after the PR has merged. No CI job runs
  the image on silicon, and QEMU's netduinoplus2 and riscv32 `virt` machines stand in for real
  parts (their memory maps, not their timing, peripherals or flash wait states).
- **Anything derived from git history.** Only the `classify changed paths` job sets
  `fetch-depth` (2, to reach the tested merge's base parent); every other checkout takes the
  default of a single commit. A check that needs history cannot run in CI. The visible
  consequence: golden provenance records bind to a content digest of the checked-in bytes rather
  than to the engine revision that produced them
  (`crates/oce-cxf/tests/golden_provenance/mod.rs:3-5`).
- **Line-ending behavior, beyond the Windows cell.** `.gitattributes:1` pins
  `* text=auto eol=lf`. The Windows cell (after the merge) checks out on a runner whose git defaults to CRLF and runs
  the bit-exact goldens there, so the normalization is exercised for every test that cell runs; the
  strict-bit fixtures it excludes, and the `tools/golden-gen` workspace, are still only read on
  Linux and macOS.

The script says the rest itself, in its closing report: a green local run
does **not** prove the native cross-arch determinism or strict-bit matrices pass (one machine
cannot reproduce them), does not prove `cargo deny check advisories` passes, and does not prove
that the script and `pr-gate.yml` still agree. Any run but `full` additionally does not prove the workspace suite or doctests pass, because the
per-PR gate does not run them, and a `docs`, `leverage` or `light` run built nothing under release codegen.

## Publishing

No Open Control Engine crate is on crates.io yet, and actual publication remains deferred pending
explicit owner authorization. A manifest without `publish = false` is only eligible for a future
registry release.

`release.yml` publishes with the crates.io token held in its `release` GitHub Environment. It is
decoupled from both gates and from each other's triggers. Pushing a `v*` tag runs verify only —
tag/version match, fmt, clippy, the offline tool prefetch (it has no cache; see
[offline tool workspaces](#what-runs-after-the-merge)), a workspace `cargo test`, the package
validator and a full
`cargo publish --workspace --locked --dry-run` — with no token and no publish, so a tag can be
re-cut safely (see [release.yml](../.github/workflows/release.yml), `verify` job). Publishing is a
separate manual `workflow_dispatch` into the `release` GitHub Environment. Cargo's workspace
selection includes the 10 publishable members and skips the eight members with `publish = false`:
`oce-bless`, `oce-conformance`, the firmware image `oce-firmware-link` and the leverage crates
`oce-replay`, `oce-author`, `oce-cli`, `oce-vocab` and `oce-verify`.
No checked-in record pins those sets any more: the guards are each crate's `publish` flag, the
byte pin on `release.yml` (whose comment names the split) and review.

### The package validator

`scripts/package_policy/validate.py` reads the publication state from each crate's `Cargo.toml`
through live Cargo metadata; nothing else restates it. It runs in the `package-publication-contract`
job of `pr-gate.yml`, in the gate script and in `release.yml`, with hostile controls in
`scripts/package_policy/test_validate.py`. It requires:

- every workspace member to be either private (`publish = false`) or registry-ready: its normal and
  build dependencies on other members are path-backed, carry a registry version and are never
  private, and the publishable graph has a deterministic leaf-to-facade order;
- `oce-api` to declare no cargo features, with the default build — its only selection — keeping a
  normal closure that excludes `oce-store` (the engine is pure, GitHub #390)
  and has no private member, database, `tokio` or `async-std`, and Cargo to refuse any feature name;
- `.github/workflows/release.yml` to match the approved bytes whose SHA-256 is fixed in
  `scripts/package_policy/release_workflow.py`. Even comments, whitespace and line endings count;
  an intentional workflow change updates the workflow, that digest and the byte golden in the
  hostile controls together.

`oce-api` has no storage dependency; `check-default-no-db.sh` refuses `oce-store` (any `oce-store*` name) in its
normal tree as well. The byte check is a drift
guard on the approved release execution, not a sandbox: it does not verify invoked actions, the
`release` environment's protection settings or secret placement.

Related: [`host-responsibilities.md`](host-responsibilities.md) for what the engine deliberately
leaves to the embedder, and [`../TESTING.md`](../TESTING.md) for the testing standard a change is
expected to meet.
