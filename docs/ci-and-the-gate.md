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
(per-PR into `development`), `release-gate.yml` (every merge into `development`, release PRs into
`main` and a daily `development`-tip run), `advisories.yml`, `docs-pages.yml`, the tag-verify / manual-publish
`release.yml`, the manual native-arm64 OpenModelica evidence workflows, and the nightly
[emulation run](#the-nightly-emulation-run) (`qemu.yml`). Actions in the gating
workflows and `docs-pages.yml` are pinned to full commit SHAs, which `check-workflow-gates.sh`
enforces for the gating workflows. `release.yml` and the OpenModelica evidence workflows are
byte-bound by their own approval and evidence manifests, so they keep tag references until their
next reviewed refresh.
The `ci.yml` in that directory is not the CI: it is the GitHub-era gate that produced the
[retained strict-bit capture archives](strict-bit-evidence.md#retained-native-archives), stays
byte-identical as their history, and is disabled in the repository's Actions settings.

Each gating workflow ends in a `CI OK` job that needs every other job and fails unless each one
succeeded — or, in `pr-gate.yml` only, was skipped by its [lane filter](#lanes-what-a-pr-runs);
`check-workflow-gates.sh` asserts that its `needs:` lists every other job in the file, that no
`release-gate.yml` job carries a job-level `if:`, and that every `pr-gate.yml` job-level `if:` is a
lane filter that `CI OK` knows about. Branch protection requires exactly that one status: the
`CI OK` check from `pr-gate.yml` on `development` and from `release-gate.yml` on `main`. The
`release-gate.yml` run that a merge into `development` starts is not a required status: it reports
on a commit that has already landed ([below](#what-runs-after-the-merge)).

## What runs when

The per-PR gate runs **once per change**, on the pull request:

- **A PR into `development`** runs `pr-gate.yml` (and `docs-pages.yml` when it touches the docs
  paths below). Which of its jobs run depends on the paths the PR changes — see
  [Lanes](#lanes-what-a-pr-runs). A new push to the PR branch cancels the superseded run.
- **A merge into `development`** runs `release-gate.yml`, the heavy gate, on the merge commit
  (issue #502; [what runs after the merge](#what-runs-after-the-merge)), and the path-filtered
  `docs-pages.yml` when the merge touches its paths. It does not re-run `pr-gate.yml`, which has
  no `push` trigger: the merged tree is the one the PR already tested, so that would repeat the
  per-PR gate for no new evidence, and `check-workflow-gates.sh` refuses any `push` trigger in
  `pr-gate.yml`, so the duplicate run cannot return unnoticed.
- **Daily**, `release-gate.yml` also runs against the `development` tip. That is the safety net
  for what a run on a merge commit can miss: a run that was replaced while it waited, an
  advisory published since, or a runner image that changed under an unchanged tree.
- **A PR into `main`, or a push to `main`,** runs `release-gate.yml`.
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
its `portable snapshot comparison` fan-in. The determinism matrix is two cells per PR (x86_64 and
aarch64, debug codegen) and the strict-bit matrix four (both architectures, debug and release).
The **leverage lane** is one job,
`leverage lane`, for the crates under [`crates/leverage/`](../crates/leverage/README.md): the
layout and `oce-api`-only dependency rule (`oce-bless` is allowed as a dev-dependency only, for the
golden switch; normal and build edges and dev edges are read separately), then fmt, clippy
`-D warnings`, rustdoc `-D warnings`, nextest and cargo-machete per member
(`scripts/ci/leverage-members.sh lane`; with no members it prints `no leverage crates yet`). The
members are `oce-replay`, `oce-author`, `oce-cli` (the `oce` binary) and `oce-vocab`.
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
  except the root `README.md` (next) and documentation an engine test reads
  (`docs/cxf-composite-subset.md`), which counts as an engine change. A fixture scans the crates for
  documentation reads so the exception list cannot fall behind.
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
| docs-only | `classify changed paths`, fmt, file-size cap, no-secret scan, docs, gate behavior fixtures, the package/authority/product contract, `CI OK` | the determinism lane, `leverage lane` | under 3 minutes (2m40s in run 42476084422) |
| README (the root `README.md`, optionally with docs) | the docs-only set plus `facade contract and Quickstart` | the determinism lane, `leverage lane` | the docs-only time or the README job, whichever is longer: that job took 44 s with a warm cache in run 42692342222, and builds the facade and its dev-dependencies cold on a new PR |
| leverage-only (`crates/leverage/**`, optionally with docs) | the docs-only set plus `leverage lane` | the determinism lane, `facade contract and Quickstart` | about 1 minute before the first leverage crate (56s in run 42476081207), plus each member's build and tests (`oce-replay` builds the facade and runs its suite) |
| leverage with the root `README.md` (a landing PR that updates the README's status) | the docs-only set plus `leverage lane` and `facade contract and Quickstart` | the determinism lane | the leverage-only time; the two jobs run side by side |
| engine (any other path, gate scripts, workflows and tool config included) | every job | none | about 4 minutes expected: the longest job left is the macOS cell (150–177 s on 2026-11-09) and every other job took 154 s or less. Before issue #502 it was a median of 18.3 minutes over 69 full-lane runs (2026-11-07 to 2026-11-09), and about 26 with `gate (light)` |

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
a workflow. `release-gate.yml` runs `gate (full)`, a superset, on every merge into `development`
and daily, so an edit to the script, a workflow or a script a job runs is first executed by CI on
its merge commit; before the merge, the author's own `bash .agents/gate.sh full` on the final head
is the run. The three steps `gate (light)` once ran that no other job covered — the facade compiler
contract, its diagnostic controls and the executed Quickstart — are the `facade contract and
Quickstart` job, which runs for every engine change (it is the README lane), and the gate-script
coverage check and its fixtures are in `gate behavior fixtures`.

### What runs after the merge

Three long jobs moved from `pr-gate.yml` to `release-gate.yml`, and that workflow now runs on every
push to `development`, which is every merged PR (issue #502, owner decision of 2026-11-09). The
per-PR gate keeps both architectures in debug codegen and the whole strict-bit matrix.

| Moved | Per-PR wall-clock on 2026-11-09 | What still runs per PR | What `release-gate.yml` runs on the merge commit |
| --- | --- | --- | --- |
| the release-codegen determinism cells | 809–1101 s (x86_64) and 653–671 s (aarch64): 15m47s compiling 105 release test binaries, 26 s running 1,281 tests | the two debug cells and the cross-architecture comparisons | all four cells, and the fan-in with every comparison: portable and target-bound bytes, blobs and round-trip recordings across debug and release as well as across architectures |
| `std build and nextest (Windows x86_64, debug)` | 251–450 s | nothing on Windows | the same cell |
| `gate (light)` | 1540–1600 s, on a gate-script change | the jobs the script mirrors, each as its own job | `gate (full)`, which executes the script in `full` mode |

**Before the merge, the release-codegen evidence is the author's gate run.** `bash .agents/gate.sh`
runs the determinism subset under release codegen on one host, and its tail goes in the PR
description. It cannot compare a debug artifact with a release one across cells, and it cannot run
the other architecture; those are first observed on the merge commit.

**After the merge, watch the run.** `CI OK` from that run is not a required status on
`development`: the commit has landed. After merging a change that runs the determinism lane, watch
the release-gate run on the merge commit; a red run is fixed forward or reverted before the next
determinism-lane merge, so a second change is never built on a tree the heavy gate has refused.

**Several merges in an hour do not starve it.** Runs are grouped by event and ref, and only a
superseded release-PR run is ever cancelled in progress. For pushes to `development` GitHub keeps
the run in progress and one waiting run: a newer push replaces the waiting one, which then shows as
cancelled on its merge commit. The run that replaced it tests a commit that contains the earlier
merge, so it is the evidence for both; a red result there does not say which of the merges it
covers caused it. A push run checks out the pushed commit in every job (`github.sha`), so its cells
and its fan-in build one tree even when another merge lands meanwhile. A scheduled or dispatched
run checks out the `development` tip as each job starts; a merge landing during such a run can
hand the fan-in cells built from two commits, which fails the comparison instead of passing it.
`check-workflow-gates.sh` pins the trigger, the checkout ref and both concurrency lines.

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
README-lane job in CI but reads only the facade's source. `bash .agents/gate.sh` runs every lane.

**Cache.** PR runs save `Swatinem/rust-cache` entries into their own PR scope only, so nothing a
PR does seeds the cache scope other PRs read. The `release-gate.yml` runs on `development` do,
after each merge and daily: they execute on the default branch, whose
cache scope every PR into `development` can read. Jobs that build the same thing in both workflows
therefore share one rust-cache `shared-key` (the key replaces rust-cache's per-job component; the
rustc host triple, the `CARGO_*`/`RUST*` environment and `Cargo.lock` still separate entries, so
x86_64 and arm64 never share one):

| `shared-key` | `pr-gate.yml` jobs | `release-gate.yml` jobs |
| --- | --- | --- |
| `nextest` | the six determinism and strict-bit cells, `crate-tests` | `test-suite` (debug and release, whole workspace) and the four determinism cells |
| `gate` | — | `gate (full)` |
| `clippy`, `build`, `rustdoc`, `default-no-db` | the job of that name | the job of that name |
| `facade-contract`, `leverage` | `facade contract and Quickstart`, `leverage lane` | — |
| `nextest-macos` | `portable snapshot (macOS arm64, debug)` | — |
| `workspace-windows` | — | `std build and nextest (Windows x86_64, debug)` |
| `blob-round-trip-wasm` | `blob round trip (wasm32-wasip1 under wasmtime)` | — |

For the like-named pairs this only makes explicit what their identical job ids already did
(rust-cache's default key component is the job id); the `nextest` key is the new link. The release-gate `test-suite` entry is a superset (the whole workspace, both codegens), which
restores as a warm start for the narrower PR cells; rust-cache never caches workspace crates, so a hit saves
dependency compile time only. The release gate's x86_64 determinism cells share that key but only
restore it (`save-if`), so a fast debug cell cannot write the key before `test-suite` does. Its
arm64 determinism cells do save, because no other release-gate job builds on arm64: that entry is
what a new PR's arm64 cells warm-start from (the debug cell finishes first, so it is normally the
debug dependency set, and the arm64 release strict-bit cells still build their dependencies cold).
The macOS cell has no release-gate
counterpart: it reuses what an earlier run of the same PR saved, and builds cold on a new PR. rust-cache saves only on an
exact-key miss, so under a shared key the first job to finish writes the entry, and its profile's
dependency set is what that key holds until `Cargo.lock`, the toolchain or the environment changes. `golden-gen-hygiene` keeps its own
per-job key (it builds the separate `tools/golden-gen` workspace).

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
inside it pinned in the script, restored first from an `actions/cache` keyed on
`hashFiles('scripts/ci/install-wasmtime.sh')` with no `restore-keys`, and trusted from the cache
only when the binary's digest is the pinned one. The job prints `wasmtime --version` before the run.
Bump the version by replacing every digest in the script in one change.

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
under `pr-gate.yml`'s `CI OK`. The Windows cell runs in `release-gate.yml` — on every merge into
`development`, on release PRs and daily — under that workflow's `CI OK` (issue #502). They
are observations of the platform, not qualification of it.

| Cell | Runner | What runs | What it proves | What it does not prove |
| --- | --- | --- | --- | --- |
| `portable snapshot (macOS arm64, debug)` and its `portable snapshot comparison` fan-in | `macos-15` (arm64); the fan-in on `ubuntu-latest` | `cargo nextest run -p oce-api -p oce-blocks -p oce-expr --profile ci` (debug codegen), emitting the portable and target-bound state vectors; the fan-in compares them with the Linux x86_64 and aarch64 debug cells' vectors | the determinism subset passes on macOS arm64; the populated portable engine-state snapshot is byte-identical to Linux x86_64 and aarch64; the target-bound snapshot differs from both Linux architectures' | release codegen on macOS; any strict-bit or transcendental result (no strict-bit suite runs, and macOS keeps the 1e-12 aligned band, see [strict-bit evidence](strict-bit-evidence.md)); the rest of the workspace on macOS |
| `std build and nextest (Windows x86_64, debug)` | `windows-latest` (x86_64, Git Bash) | `cargo build --workspace --locked`, then `cargo nextest run --workspace --profile ci` without the five strict-bit binaries `scripts/ci/strict-bits-cell.sh` runs (`strict_bits` and the four aligned per-block suites) | the workspace builds on Windows and every other workspace test passes there, including the bit-exact goldens on a checkout whose git defaults to CRLF (`.gitattributes` keeps them LF) | any determinism or snapshot claim; release codegen; doctests; any strict-bit or transcendental result |

The macOS cell lives in `pr-gate.yml`, not in a separate workflow, because `CI OK` is the only
required status and a job in another workflow cannot appear in its `needs:`; a separate workflow
would either be unrequired or add a second required status. It runs only on a PR that changes the
engine, a gate script or a workflow, and has no schedule of its own. The Windows cell moved to
`release-gate.yml` with issue #502: at four to eight minutes it was the longest per-PR job after
the release determinism cells, and it makes no determinism claim. A change that breaks only on
Windows is therefore found on its merge commit, not on the PR. `.agents/gate.sh` cannot run either
cell (wrong operating system) and says so in its closing report.

## One command, one source of truth

[`.agents/gate.sh`](../.agents/gate.sh) is the only place the gate's command list is written down.
Every other document in this repo — including this page — points at it rather than restating it,
because nine divergent prose copies existed before the script was written and two of them were
materially weaker than CI (see the script's header). There are three gate invocations and three
focused ones:

```
bash .agents/gate.sh            # light    — mirrors the per-PR gate, plus the determinism
                                #            subset under release codegen
bash .agents/gate.sh full       # full     — adds the workspace suite and doctests
bash .agents/gate.sh leverage   # leverage — mirrors a PR confined to crates/leverage/
bash .agents/gate.sh nostd      # nostd    — the bare-metal cell alone (light and full run it too)
bash .agents/gate.sh qemu       # qemu     — the nightly emulation check (needs QEMU installed)
bash .agents/gate.sh roundtrip  # roundtrip — the blob round trip: record on this host, replay it
                                #             here and on wasm32-wasip1 (full runs it too)
```

CI does not merely mirror that script, it **executes** it: `gate (full)` at
[`.github/workflows/release-gate.yml`](../.github/workflows/release-gate.yml), on every merge into
`development` and daily. No job in
[`.github/workflows/pr-gate.yml`](../.github/workflows/pr-gate.yml) does (`gate (light)` ran it on
every PR that changed a gate script, workflow or tool config until issue #502). So CI exercises a
command in the script on the merge commit, not on the pull request; before the merge, the author's
own `bash .agents/gate.sh full` on the final head is the run. Read the CI run as coverage, not as
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
for those gates, because a gate that cannot fail is not a gate; build, clippy and rustdoc under
`-D warnings`; supply-chain checks; the determinism subset, in debug and in release codegen (the
release run is the pre-merge release-codegen evidence, since no per-PR job runs it); and two
fixture input-hygiene audits. A
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
light gate):

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
`determinism-matrix` job in `.github/workflows/pr-gate.yml`: two
native cells, x86_64 on `ubuntu-latest` and aarch64 on `ubuntu-24.04-arm`, in debug codegen. Each
cell refuses a runner of the wrong architecture, runs the three-crate
subset and emits populated revision-2 portable and target-bound state vectors. The
`determinism-compare` fan-in (one leg per architecture) downloads both cells' vectors,
requires portable bytes to match across architectures and target-bound bytes to differ, and then
parses the **other** architecture's target-bound snapshot and refuses it through the public restore path
— x86_64 refuses aarch64's, aarch64 refuses x86_64's — natively, from the oce-api unit-test binary
its own cell built and archived. Each cell also writes the 46 compiled-schedule blobs it
encodes (`OCE_BLOB_OUT`, after its `blob_goldens` test has compared them with the checked-in
goldens), and the fan-in requires those bytes identical in both cells
and each file 123,329 bytes, the goldens' total, so a truncated
artifact cannot pass ([compiled-schedule blob](compiled-schedule-blob.md), GitHub #400).
`bash .agents/gate.sh full` writes the artifact the same way and checks its size.
Nothing is emulated; every test in the subset runs in every cell.

**Release codegen runs after the merge** (issue #502). The job of the same name in
`release-gate.yml` runs four cells — both architectures, debug and release codegen — on every
merge into `development`, and its fan-in adds the comparisons a per-PR run no longer has: portable
and target-bound bytes equal across codegen within each architecture, the blobs identical in all
four cells, and the four round-trip recordings identical. A release cell was the per-PR long pole,
11 to 18 minutes of which the tests ran for under half a minute, so a pull request does not wait
for it. What a pull request has before it merges is the author's `bash .agents/gate.sh`, which runs
the subset under release codegen on one host: one architecture, and no comparison between a debug
and a release artifact. A divergence that needs the other architecture or that comparison is found
on the merge commit ([what runs after the merge](#what-runs-after-the-merge)).

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

The remainder waits for the release/full gate. A change outside the named test subsets can show
a fully green PR having executed none of its own tests.
Before claiming tests pass, run `bash .agents/gate.sh full` first-hand and read the tail.

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
| aarch64 (replays) | `determinism-compare`, aarch64 leg | requires the cells' recordings identical (`diff -rq`, 92 files each: the two debug cells per PR, all four cells in `release-gate.yml` after the merge), then runs the suite its own debug cell archived with `OCE_ROUND_TRIP_EXPECT` pointing at the x86_64 debug recording: it decodes the blobs x86_64 encoded and compares its run with the recorded one. The x86_64 leg runs the same replay. |
| wasm32 (replays) | `blob round trip (wasm32-wasip1 under wasmtime)`, on `ubuntu-latest` after the matrix | compiles the suite for `wasm32-wasip1` and runs it under the pinned wasmtime through the cargo target runner in `.cargo/config.toml` (`scripts/ci/wasmtime-runner.sh`, which shows the guest the workspace at its host path and passes only the two round-trip variables and `RUST_BACKTRACE`), against the same recording |

Every leg also compares the facade with the entry on its own target, the blob bytes with the
checked-in goldens and the entry's run with the pinned run digests. `wasm32-unknown-unknown` stays a
build-only target in the [bare-metal cell](#the-bare-metal-cell); a headless-browser run belongs to
the `oce-wasm` crate (M4). `bash .agents/gate.sh roundtrip` (and `full`) records on the local host
and replays the recording there and on `wasm32-wasip1`; it fails, naming the install command, when
the target or wasmtime is missing. The second native architecture is CI's alone.

## Every pull request runs the gate

The per-PR gate runs on every PR, drafts included; there is no draft carve-out. The only job-level
`if:` a `pr-gate.yml` job may carry is its lane filter, and `CI OK` reads any other skip as a
failure.

**The step-condition rule.** No job or step in `pr-gate.yml` or `release-gate.yml` may set
`continue-on-error`. In both, a
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
which a sandboxed lane has. It runs daily in [advisories.yml](../.github/workflows/advisories.yml) and on
release PRs (the `release-gate.yml` cargo-deny job). `advisories.yml` has no `pull_request` trigger at all, so
a PR into `development` that introduces a dependency with a known RustSec advisory merges green and
is caught by the next scheduled run, not by its own gate.

## What the release gate adds

`release-gate.yml` fires on every push to `development` (each merged PR), on `development` →
`main` PRs, on pushes to `main`, on manual dispatch,
and on a daily cron against the `development` tip (see its trigger block). It is disjoint from
`pr-gate.yml` by base
branch, so the two never both fire on one PR. It re-runs the light correctness gates against the
tip it tests and adds six things, the last three moved from the per-PR gate by issue #502
([what runs after the merge](#what-runs-after-the-merge)):

| Step | What it covers | Where |
| --- | --- | --- |
| workspace nextest | every unit and integration test in the workspace | `release-gate.yml`, `test-suite` job, unit + integration step |
| workspace nextest, release codegen | release panic-freedom, `debug_assert` paths stripped; inherited `ci-release` runner policy | `release-gate.yml`, `test-suite` job, release step |
| `cargo test --doc` | doctests — nextest cannot run them, so this is a separate step | `release-gate.yml`, `test-suite` job, doctest step |
| determinism matrix, four native cells | the three-crate subset and the blob round trip under release codegen on x86_64 and aarch64, beside the debug cells; the fan-in compares debug against release (portable and target-bound bytes, blobs, round-trip recordings) and repeats the cross-architecture checks | `release-gate.yml`, `determinism-matrix` and `determinism-compare` jobs |
| Windows cell | the workspace build and suite on Windows, without the strict-bit binaries | `release-gate.yml`, `std-build-windows` job |
| the executed gate script | `bash .agents/gate.sh full`, the only CI execution of the script | `release-gate.yml`, `gate` job |

`--no-tests=fail` is explicit on the nextest steps: a run that discovers zero tests hard-fails
rather than passing, which catches tests that silently stop compiling or being found.

### Nextest policy and reports

Local setup and CI pin cargo-nextest `0.9.143`; `.config/nextest.toml` also declares that version as
both required and recommended, so an older local binary exits before testing. The `default` profile
is fail-fast. Automated debug runs use `ci`; release-codegen runs use `ci-release`, which inherits
the same retries, timeout, leak, and reporter policy instead of copying it. Those three are the
only profiles.

Retries are zero and a flaky pass is still a failure. Ordinary tests terminate after 120 seconds. A
run stops
after 15 minutes, and a child process retaining inherited output handles for more than two seconds
fails as a leak. CI writes Jenkins-compatible JUnit XML to `target/nextest/<profile>/junit.xml` and
requires every expected report to exist and be non-empty; the reports are no longer uploaded as
artifacts. Every cell, both architectures, uses `.config/nextest.toml`.

Partitioning is deliberately off: compilation dominates and test execution takes seconds. In run
42462665456 the x86_64 release determinism cell, then the per-PR long pole and since issue #502 a
`release-gate.yml` job, spent 13m05s in its nextest
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
  `release-gate.yml`, `advisories.yml`, and `docs-pages.yml` (per-PR on `docs/**`, `README.md`, `scripts/docs/**`,
  `scripts/authority_claims/**`, and `site/**`) — runs on GitHub-hosted Linux runners.
  Cross-*architecture* is covered natively for the determinism and strict-bit subsets — x86_64 on
  `ubuntu-latest`, aarch64 on `ubuntu-24.04-arm`, debug and release (the determinism subset's
  release-codegen cells in the release gate, after the merge). The exceptions are the two
  [platform cells](#platform-cells-macos-and-windows): the determinism subset in debug on macOS
  arm64 (per PR), and the workspace build and suite (without the strict-bit binaries) in debug on
  Windows (in the release gate).
  Every other job runs on Linux x86_64 only: nothing runs release
  codegen or doctests on macOS or Windows, and no strict-bit result is observed on either.
- **Release codegen, Windows and the executed gate script, before a merge.** No per-PR job runs
  the determinism subset under release codegen, compares a debug artifact with a release one,
  builds on Windows or executes `.agents/gate.sh`. The author's own gate run covers release codegen
  on one host; the rest is first observed on the merge commit, by a run that does not block the
  merge it reports on.
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
that the script and `pr-gate.yml` still agree. A `light` run additionally does not prove the workspace suite or doctests pass, because the
per-PR gate does not run them.

## Publishing

No Open Control Engine crate is on crates.io yet, and actual publication remains deferred pending
explicit owner authorization. A manifest without `publish = false` is only eligible for a future
registry release.

`release.yml` publishes with the crates.io token held in its `release` GitHub Environment. It is
decoupled from both gates and from each other's triggers. Pushing a `v*` tag runs verify only —
tag/version match, fmt, clippy, a workspace `cargo test`, the package validator and a full
`cargo publish --workspace --locked --dry-run` — with no token and no publish, so a tag can be
re-cut safely (see [release.yml](../.github/workflows/release.yml), `verify` job). Publishing is a
separate manual `workflow_dispatch` into the `release` GitHub Environment. Cargo's workspace
selection includes the 10 publishable members and skips the seven members with `publish = false`:
`oce-bless`, `oce-conformance`, the firmware image `oce-firmware-link` and the leverage crates
`oce-replay`, `oce-author`, `oce-cli` and `oce-vocab`.
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
