# CI and the gate

For contributors, and for anyone looking at a green check mark on a pull request and wondering what
it proves. The short answer is: less than you would assume. The split is deliberate, and it is easy
to misread in the dangerous direction.

## Where CI runs

CI runs on GitHub Actions, the project's primary host, from the workflows in
[`.github/workflows/`](../.github/workflows/) on GitHub-hosted Linux runners — x86_64
(`ubuntu-latest`) for most jobs, plus native arm64 (`ubuntu-24.04-arm`) for the aarch64 cells of the
determinism and strict-bit matrices: `pr-gate.yml`
(per-PR into `development`), `release-gate.yml` (release PRs into `main` and a daily
`development`-tip run), `advisories.yml`, `docs-pages.yml`, the tag-verify / manual-publish
`release.yml`, and the manual native-arm64 OpenModelica evidence workflows. Actions in the gating
workflows and `docs-pages.yml` are pinned to full commit SHAs, which `check-workflow-gates.sh`
enforces for the gating workflows. `release.yml` and the OpenModelica evidence workflows are
byte-bound by their own approval and evidence manifests, so they keep tag references until their
next reviewed refresh.
The `ci.yml` in that directory is not the CI: it is the GitHub-era gate that produced the
[retained strict-bit capture archives](strict-bit-evidence.md#retained-native-archives), stays
byte-identical as their history, and is disabled in the repository's Actions settings. (CI briefly ran on
a self-hosted Forgejo instance, now retired.)

Each gating workflow ends in a `CI OK` job that needs every other job and fails unless each one
succeeded — or, in `pr-gate.yml` only, was skipped by its [lane filter](#lanes-what-a-pr-runs);
`check-workflow-gates.sh` asserts that its `needs:` lists every other job in the file, that no
`release-gate.yml` job carries a job-level `if:`, and that every `pr-gate.yml` job-level `if:` is a
lane filter that `CI OK` knows about. Branch protection requires exactly that one status: the
`CI OK` check from `pr-gate.yml` on `development` and from `release-gate.yml` on `main`.

## What runs when

The per-PR gate runs **once per change**, on the pull request:

- **A PR into `development`** runs `pr-gate.yml` (and `docs-pages.yml` when it touches the docs
  paths below). Which of its jobs run depends on the paths the PR changes — see
  [Lanes](#lanes-what-a-pr-runs). A new push to the PR branch cancels the superseded run.
- **A merge into `development`** runs no gate. `pr-gate.yml` has no `push` trigger: the merged
  tree is the one the PR already tested, so a post-merge run would repeat the whole gate for no new
  evidence. Only the path-filtered `docs-pages.yml` runs on that push, and only when the
  merge touches its paths. `check-workflow-gates.sh` refuses any `push` trigger in `pr-gate.yml`,
  so the duplicate run cannot return unnoticed.
- **Daily**, `release-gate.yml` runs the heavy gate against the `development` tip. That is the
  safety net for anything per-PR CI does not cover, including an interaction between two PRs that
  each passed on their own.
- **A PR into `main`, or a push to `main`,** runs `release-gate.yml`.

### Lanes: what a PR runs

`pr-gate.yml` has two lanes under the one `CI OK`. The **determinism lane** is every job that
compiles the engine: the determinism and strict-bit matrices and their fan-ins, the x86_64 crate
suites, clippy, build, rustdoc, cargo-deny, cargo-machete, the database-free check, the golden-gen
firewall and hygiene jobs, and the facade contract and Quickstart. The **leverage lane** is one job,
`leverage lane`, for the crates under [`crates/leverage/`](../crates/leverage/README.md): the
layout and `oce-api`-only dependency rule, then fmt, clippy `-D warnings`, rustdoc `-D warnings`,
nextest and cargo-machete per member (`scripts/ci/leverage-members.sh lane`; with no members it
prints `no leverage crates yet`).

The first job, `classify changed paths`, lists what the PR changes with plain git — the tested merge
commit against its base parent, renames as a deletion plus an addition — and classifies it with
`scripts/ci/classify-changes.sh`. The rules fail closed: a path that matches neither the docs-only
nor the leverage allowlist counts as an engine change.

- **docs-only** paths: `docs/**`, `site/**`, `scripts/docs/**`, a root-level `*.md`,
  `.agents/project-facts.md`, `.github/ISSUE_TEMPLATE/**` and `.github/pull_request_template.md` —
  except documentation a test reads (`README.md`, whose Quickstart a test byte-pins, and
  `docs/cxf-composite-subset.md`), which counts as an engine change. A fixture scans the crates for
  documentation reads so the exception list cannot fall behind.
- **leverage** paths: `crates/leverage/**`.
- **gate-script** paths: `.agents/gate.sh`, `.github/scripts/**`, `scripts/**` outside
  `scripts/docs/`, `.github/workflows/**` and `.config/**`. A change to how CI runs must prove the
  whole gate still passes, so these count as engine changes too.
- **everything else** is an engine change: any crate outside `crates/leverage/`, `Cargo.toml`,
  `Cargo.lock`, `rust-toolchain.toml`, `.cargo/**`, `deny.toml`, `tools/**`, `third_party/**`,
  `.githooks/**`, `.gitattributes`, an unrecognised root file. Adding a leverage crate or changing
  its dependencies changes `Cargo.lock`, so it runs everything.

| Change class | Jobs that run | Jobs skipped | Expected wall-clock |
| --- | --- | --- | --- |
| docs-only | `classify changed paths`, fmt, file-size cap, no-secret scan, docs, gate behavior fixtures, the package/authority/product contract, `CI OK` | the determinism lane, `leverage lane`, `gate (light)` | under 3 minutes (2m40s in run 42476084422) |
| leverage-only (`crates/leverage/**`, optionally with docs) | the docs-only set plus `leverage lane` | the determinism lane, `gate (light)` | about 1 minute with no leverage crates (56s in run 42476081207); plus each member's build and tests |
| engine (any other path) | every job except `gate (light)` | `gate (light)` | about 15–17 minutes (16m52s in run 42476086689; a release determinism cell is the long pole) |
| gate scripts, workflows, tool config | every job | none | about 15–17 minutes (15m21s in run 42476052220; `gate (light)` runs beside the long pole) |

The always-on set runs whatever changed: it compiles nothing (fmt only reads the tree), and the
package/authority/product contract stays in it because its checks read the documentation as well as
the manifests. An engine change also runs the leverage lane, because a change behind the facade can
break a crate built on it.

`gate (light)` — `bash .agents/gate.sh` executed in CI — runs **only** for a gate-script change. It
is a coverage backstop against a `pr-gate.yml` job drifting away from the script, and only an edit to
the script, a workflow or a script a job runs can cause that drift; on any other PR it re-ran what
the other jobs had just run (17m28s measured on 2026-11-08). Its three steps that no other job
covered — the facade compiler contract, its diagnostic controls and the executed Quickstart — moved
into the `facade contract and Quickstart` job in the determinism lane, and the gate-script coverage
check and its fixtures into `gate behavior fixtures`. `release-gate.yml` still runs `gate (full)`
daily on the `development` tip.

**How a skip passes `CI OK`.** Each lane-filtered job has exactly one job-level `if:`,
`needs.changes.outputs.<lane> == 'true'`, and `CI OK` (`scripts/ci/ci_ok.py`) accepts its
`skipped` result only when that output was `false`. A job skipped while its lane ran — the shape of
a job that never started because something it needs failed — fails, as does any skip of an
always-on job, any failure or cancellation, and a failed or self-contradictory classification.
`check-workflow-gates.sh` keeps `CI OK`'s lane map equal to the jobs' `if:` lines, pins each named
job to its lane, and fails if the result check or the classifier outputs are removed; its fixtures
prove each of those controls fails. A manual dispatch runs every lane.

`bash .agents/gate.sh leverage` is the local mirror of a leverage-only PR: the always-on checks and
the leverage lane, without the engine lane. `bash .agents/gate.sh` runs both lanes.

**Cache.** PR runs save `Swatinem/rust-cache` entries into their own PR scope only, and there is
no post-merge push run, so nothing a PR does seeds the cache scope other PRs read. The daily
`release-gate.yml` run does: scheduled runs execute on the default branch (`development`), whose
cache scope every PR into `development` can read. Jobs that build the same thing in both workflows
therefore share one rust-cache `shared-key` (the key replaces rust-cache's per-job component; the
rustc host triple, the `CARGO_*`/`RUST*` environment and `Cargo.lock` still separate entries, so
x86_64 and arm64 never share one):

| `shared-key` | `pr-gate.yml` jobs | `release-gate.yml` jobs |
| --- | --- | --- |
| `nextest` | the eight determinism and strict-bit cells, `crate-tests` | `test-suite` (debug and release, whole workspace) |
| `gate` | `gate (light)` | `gate (full)` |
| `clippy`, `build`, `rustdoc`, `default-no-db` | the job of that name | the job of that name |
| `facade-contract`, `leverage` | `facade contract and Quickstart`, `leverage lane` | — |

For the like-named pairs this only makes explicit what their identical job ids already did
(rust-cache's default key component is the job id); the `nextest` key is the new link. The release-gate `test-suite` entry is a superset (the whole workspace, both codegens), which
restores as a warm start for the narrower PR cells; rust-cache never caches workspace crates, so a hit saves
dependency compile time only. The arm64 cells have no release-gate counterpart: they reuse what an
earlier run of the same PR saved, and build cold on a new PR. rust-cache saves only on an
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

Branch protection on `development` makes the PR run the only way in: a pull request is required,
the `CI OK` status is required, and the branch must be up to date with `development` before it
merges. GitHub PR runs test a synthetic merge of the PR head into its base, and the up-to-date
rule makes that tested merge the commit that lands; a PR that falls behind is updated and re-run
before it merges. Now that two lanes exist, the next step is a merge queue, which tests each merge
group once instead. Until then a leverage-only PR is still serialized by the up-to-date rule: it
skips the determinism lane, but it must be updated and re-run whenever any other PR lands first,
and that re-run classifies only its own changes.

Docs-site validation (`.github/workflows/docs-pages.yml`) is path-filtered to `docs/**`,
`README.md`, `scripts/docs/**`, `scripts/authority_claims/**`, `site/**` and its own workflow
file. It reports its own `build docs artifact` status and is **not** part of `CI OK`, so a PR that
does not touch those paths shows no docs check at all. A push to `main` touching those paths also
deploys the site to GitHub Pages.

## One command, one source of truth

[`.agents/gate.sh`](../.agents/gate.sh) is the only place the gate's command list is written down.
Every other document in this repo — including this page — points at it rather than restating it,
because nine divergent prose copies existed before the script was written and two of them were
materially weaker than CI (see the script's header). There are three invocations:

```
bash .agents/gate.sh            # light    — mirrors the per-PR gate
bash .agents/gate.sh full       # full     — adds the workspace suite and doctests
bash .agents/gate.sh leverage   # leverage — mirrors a PR confined to crates/leverage/
```

CI does not merely mirror that script, it **executes** it: the `gate (light)` job at
[`.github/workflows/pr-gate.yml`](../.github/workflows/pr-gate.yml) on every PR that changes a gate
script, workflow or tool config, and `gate (full)` at
[`.github/workflows/release-gate.yml`](../.github/workflows/release-gate.yml) daily.
So every command in the script gates any pull request that could make a `pr-gate.yml` job diverge
from it, whether or not `pr-gate.yml` also runs it as its own job. Read that as coverage, not as
parity, and note that the implication does not run the other way:
`gate (light)` is `bash .agents/gate.sh` **plus** any steps of its own. The Quickstart-executes step
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
`-D warnings`; supply-chain checks; the determinism subset; and two fixture input-hygiene audits. A
failing step never aborts the run, so one round trip reports every problem instead of the first
(see the script's `step` function).

The [authority index and generated projection](authority-claims.md) add a fast, bounded consistency
check and hostile controls. Native numeric observers run inside the existing `oce-api`/`oce-blocks`
subset; package/public/catalog validators retain ownership. The full gate also executes those owners.
This does not check arbitrary Markdown claims or workflow parity, and regeneration is never gated in.

## Dev-light, release-heavy

**A green PR is not evidence that the change's own tests pass.**

On every engine change (see [Lanes](#lanes-what-a-pr-runs)), the per-PR gate into `development`
runs the state-determinism subset for **`oce-api`, `oce-blocks`, and `oce-expr`**. That is the
`determinism-matrix` job in `.github/workflows/pr-gate.yml`: four
native cells, x86_64 on `ubuntu-latest` and aarch64 on `ubuntu-24.04-arm`, each under debug and
under release codegen. Each cell refuses a runner of the wrong architecture, runs the three-crate
subset and emits populated revision-2 portable and target-bound state vectors. The
`determinism-compare` fan-in (one leg per architecture) downloads all four cells' vectors,
requires portable and target-bound bytes to match across codegen within each architecture,
portable bytes to match across architectures and target-bound bytes to differ, and then parses
the **other** architecture's target-bound snapshot and refuses it through the public restore path
— x86_64 refuses aarch64's, aarch64 refuses x86_64's — natively, from the oce-api unit-test binary
its own debug cell built and archived. Nothing is emulated; every test in the subset runs in every
cell.
The gate script runs the test commands locally and adds two named
`oce-cxf` test binaries, which are input hygiene rather than
engine coverage: the port-order audit sweeps 47 CXF documents, of which 46 are Guideline 36 catalog
fixtures and one is a resolver contract; the structural oracle compares the catalog fixtures it can
pair with vendored modelica-json translations
(see the gate script's fixture input-hygiene section). That oracle compares document structure — instances and undirected
edges — not simulated behavior.

The scoped `oce-conformance` **strict-bit subset also runs per-PR**: `strict_bits` plus the four
affected per-block suite binaries, in the same four native cells, with two independent captures
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

## Every pull request runs the gate

The per-PR gate runs on every PR, drafts included; there is no draft carve-out. The only job-level
`if:` a `pr-gate.yml` job may carry is its lane filter, and `CI OK` reads any other skip as a
failure. A PR with no checks still looks a lot like a PR with no failing checks — confirm `CI OK`
actually reported, and on a docs-only or leverage-only PR expect the determinism jobs to show as
skipped, not green.

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

`release-gate.yml` fires on `development` → `main` PRs, on pushes to `main`, on manual dispatch,
and on a daily cron against the `development` tip (see its trigger block). It is disjoint from
`pr-gate.yml` by base
branch, so the two never both fire on one PR. It re-runs the light correctness gates against the
release tip and adds three things:

| Step | What it covers | Where |
| --- | --- | --- |
| workspace nextest | every unit and integration test in the workspace | `release-gate.yml`, `test-suite` job, unit + integration step |
| workspace nextest, release codegen | release panic-freedom, `debug_assert` paths stripped; inherited `ci-release` runner policy | `release-gate.yml`, `test-suite` job, release step |
| `cargo test --doc` | doctests — nextest cannot run them, so this is a separate step | `release-gate.yml`, `test-suite` job, doctest step |

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
42462665456 the x86_64 release determinism cell, the per-PR long pole, spent 13m05s in its nextest
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
  `ubuntu-latest`, aarch64 on `ubuntu-24.04-arm`, debug and release. Every other job, and the
  whole release gate, runs on x86_64 only. macOS and Windows are not built or tested anywhere.
- **Anything derived from git history.** Only the `classify changed paths` job sets
  `fetch-depth` (2, to reach the tested merge's base parent); every other checkout takes the
  default of a single commit. A check that needs history cannot run in CI. The visible
  consequence: golden provenance records bind to a content digest of the checked-in bytes rather
  than to the engine revision that produced them
  (`crates/oce-cxf/tests/golden_provenance/mod.rs:3-5`).
- **Line-ending behavior.** `.gitattributes:1` pins `* text=auto eol=lf`, but an ubuntu-only CI
  never performs a CRLF checkout, so that normalization is asserted by git configuration and
  exercised by no test. Goldens here are compared bit-exactly, which is precisely where a stray
  `\r` would show up.

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
selection includes the 10 publishable members and skips the two members with `publish = false`:
`oce-bless` and `oce-conformance`.
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
