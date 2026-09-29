# CI and the gate

For contributors, and for anyone looking at a green check mark on a pull request and wondering what
it proves. The short answer is: less than you would assume. The split is deliberate, and it is easy
to misread in the dangerous direction.

## Where CI runs

CI runs on Forgejo Actions, the project's primary host, from the workflows in
[`.forgejo/workflows/`](../.forgejo/workflows/) on a self-hosted Linux x86_64 runner. The public
GitHub repository is a push mirror with GitHub Actions disabled; `.github/workflows/` keeps only
GitHub-bound, manually or tag-triggered work (crates.io release, GitHub Pages publish, the
native-arm64 OpenModelica evidence workflows) and a dormant `ci.yml` that stays byte-identical
because it is a bound source of the [retained strict-bit evidence](strict-bit-evidence.md).

Each gating workflow ends in a `CI OK` job that needs every other job and fails unless each one
succeeded; `check-workflow-gates.sh` asserts that its `needs:` lists every other job in the file
and that no gating job carries a job-level `if:`. Branch protection requires exactly that one
status: `ci / CI OK (pull_request)` on `development`, `release-gate / CI OK (pull_request)` on
`main`.

Forgejo PR runs check out and test the PR's head commit, not a synthetic merge with the base
branch as GitHub did. Branch protection therefore also requires a PR branch to be up to date with
its base before merging, so the tested head is what lands.

Docs-site validation (`.forgejo/workflows/docs-pages.yml`) is path-filtered to `docs/**`,
`README.md`, `scripts/docs/**`, `scripts/authority_claims/**`, `site/**` and its own workflow
file. It reports its own `docs-pages / build docs (pull_request)` status and is **not** part of
`CI OK`, so a PR that does not touch those paths shows no docs check at all.

## One command, one source of truth

[`.agents/gate.sh`](../.agents/gate.sh) is the only place the gate's command list is written down.
Every other document in this repo — including this page — points at it rather than restating it,
because nine divergent prose copies existed before the script was written and two of them were
materially weaker than CI (see the script's header). There are two invocations:

```
bash .agents/gate.sh        # light — mirrors the per-PR gate
bash .agents/gate.sh full   # full  — adds the workspace suite and doctests
```

CI does not merely mirror that script, it **executes** it: the `gate (light)` job at
[`.forgejo/workflows/ci.yml`](../.forgejo/workflows/ci.yml) and `gate (full)` at
[`.forgejo/workflows/release-gate.yml`](../.forgejo/workflows/release-gate.yml).
So every command in the script gates a pull request whether or not `ci.yml` also runs it as its own
job. Read that as coverage, not as parity, and note that the implication does not run the other way:
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

The per-PR gate into `development` runs the state-determinism subset for **`oce-api`, `oce-blocks`,
and `oce-expr`**. That is the `determinism-matrix` job in `.forgejo/workflows/ci.yml`: it runs
that three-crate subset natively on x86_64 and, cross-compiled, on aarch64 under QEMU user-mode
emulation, each twice — once under debug codegen, once under release codegen. Each architecture
emits populated revision-2 portable and target-bound state vectors. The job compares both across
codegen profiles, requires the portable files to match and the target-bound files to differ across
architectures, then parses and refuses the aarch64 target-bound bytes on x86_64. The emulated leg
excludes one test that re-executes its own binary (an `OCE_BLESS` truthiness probe, not a
determinism test), which the native leg still runs. Emulation keeps the cross-architecture
comparison on every PR with a single x86_64 runner; it is CI signal, not native aarch64 evidence.
The gate script runs the test commands locally and adds two named
`oce-cxf` test binaries, which are input hygiene rather than
engine coverage: the port-order audit sweeps 47 CXF documents, of which 46 are Guideline 36 catalog
fixtures and one is a resolver contract; the structural oracle compares the catalog fixtures it can
pair with vendored modelica-json translations
(see the gate script's fixture input-hygiene section). That oracle compares document structure — instances and undirected
edges — not simulated behavior.

The scoped `oce-conformance` **strict-bit subset also runs per-PR**: `strict_bits` plus the four
affected per-block suite binaries, in Linux x86_64 (native) / aarch64 (emulated) × debug/release,
with two independent captures per cell and a fail-closed cross-cell comparison. The [retained evidence](strict-bit-evidence.md)
covers exact comparison of 21 pinned Real cases on qualified Linux; unqualified targets retain
the unchanged 1e-12 aligned band. This is not the whole conformance suite or a libm accuracy claim.

The remainder waits for the release/full gate. A change outside the named test subsets can show
a fully green PR having executed none of its own tests.
Before claiming tests pass, run `bash .agents/gate.sh full` first-hand and read the tail.

## Every pull request runs the gate

Forgejo has no GitHub-style draft flag, so the per-PR gate runs on every PR, work-in-progress
included; there is no draft carve-out. A PR with no checks still looks a lot like a PR with no
failing checks — confirm `CI OK` actually reported.

## cargo-deny is not skippable, but advisories do not gate a PR

The standalone `cargo-deny` job in `ci.yml` runs cargo-deny's bans, licenses and sources checks on
every PR, and the gate script runs them too (see the script's cargo-deny step).

`advisories` is a different story, and the carve-out belongs next to the claim. It is deliberately
excluded from the script — it needs network access and a writable advisory database, neither of
which a sandboxed lane has. It runs daily in [advisories.yml](../.forgejo/workflows/advisories.yml) and on
release PRs (the `release-gate.yml` cargo-deny job). `advisories.yml` has no `pull_request` trigger at all, so
a PR into `development` that introduces a dependency with a known RustSec advisory merges green and
is caught by the next scheduled run, not by its own gate.

## What the release gate adds

`release-gate.yml` fires on `development` → `main` PRs, on manual dispatch, and on a daily cron
against the `development` tip (see its trigger block). It is disjoint from `ci.yml` by base
branch, so the two never both fire on one PR. It re-runs the light correctness gates against the
release tip and adds four things:

| Step | What it covers | Where |
| --- | --- | --- |
| workspace nextest | every unit and integration test in the workspace | `release-gate.yml`, `test-suite` job, unit + integration step |
| workspace nextest, release codegen | release panic-freedom, `debug_assert` paths stripped; inherited `ci-release` runner policy | `release-gate.yml`, `test-suite` job, release step |
| `cargo test --doc` | doctests — nextest cannot run them, so this is a separate step | `release-gate.yml`, `test-suite` job, doctest step |
| two `cargo public-api` surface gates | exact public API text for `oce-api` and `oce-store` | `release-gate.yml`, `test-suite` job, per-crate surface steps |

`--no-tests=fail` is explicit on the nextest steps: a run that discovers zero tests hard-fails
rather than passing, which catches tests that silently stop compiling or being found.

### Nextest policy and reports

Local setup and CI pin cargo-nextest `0.9.143`; `.config/nextest.toml` also declares that version as
both required and recommended, so an older local binary exits before testing. The `default` profile
is fail-fast. Automated debug runs use `ci`; release-codegen runs use `ci-release`, which inherits
the same retries, timeout, leak, and reporter policy instead of copying it. The two public-API runs
inherit that policy through separate child profiles because their nested nightly builds need a
longer per-test timeout and separate reports.

Retries are zero and a flaky pass is still a failure. Ordinary tests terminate after 120 seconds;
the public-API surface tests allow 10 minutes for their nested nightly rustdoc builds. A run stops
after 15 minutes, and a child process retaining inherited output handles for more than two seconds
fails as a leak. CI writes Jenkins-compatible JUnit XML to `target/nextest/<profile>/junit.xml` and
requires every expected report to exist and be non-empty; the reports are no longer uploaded as
artifacts. The emulated aarch64 legs use `scripts/ci/nextest-emulated.toml` instead of
`.config/nextest.toml`: the same no-retry, flaky-is-failure policy, with wider time limits because
QEMU runs test binaries several times slower.

Partitioning and build archives are deliberately off: the full test execution takes seconds while
compilation dominates, and each determinism leg must execute the complete selected set under its
own architecture and codegen mode. Experimental record/replay is also off in CI; enabling a feature
that nextest still marks unstable would make the gate depend on a non-stable format. Test groups and
thread reservations remain available when measurement identifies a shared resource or heavy test;
none is known today.

The public-api baselines are the strongest stability evidence in this repo. They are checked-in
text files — `crates/oce-api/tests/public-api.txt` (1357 lines) and
`crates/oce-store/tests/public-api.txt` (1230 lines) — and the tests at
`crates/oce-api/tests/public_api.rs` and `crates/oce-store/tests/public_api.rs` diff the crate's
real surface against them, so any unintended addition, removal or signature change fails the gate
rather than shipping. Two env vars interlock to keep the gate honest: `OCE_PUBLIC_API_NIGHTLY` arms
it and names the pinned nightly to shell out to, and `OCE_REQUIRE_SURFACE_CHECK=1` turns a missing
nightly into a hard panic instead of a silent skip, so disarming the gate turns it red, never green
(see the surface steps' arming environment). The two crates run as separate steps on purpose: merging the package
selectors would let one surviving crate hide the other's vanished test.

The exact rows are classified without replacing these signature baselines by the
[public surface contract](public-surface-contract.md) and its
[machine-checked ledger](public-surface-ledger.json).

## What CI cannot observe

- **Operating systems other than Linux.** Every Forgejo workflow — `ci.yml`, `release-gate.yml`,
  `advisories.yml`, and `docs-pages.yml` (per-PR on `docs/**`, `README.md`, `scripts/docs/**`,
  `scripts/authority_claims/**`, and `site/**`) — runs on `ubuntu-latest`, a Linux x86_64 runner.
  Cross-*architecture* is covered for the determinism and strict-bit subsets — x86_64 native and
  aarch64 emulated, debug and release. macOS and Windows are not built or tested anywhere.
- **Native aarch64 execution.** There is no arm64 runner, so per-PR aarch64 results come from QEMU
  user-mode emulation. The retained native qualification in `qualified-linux/` is still verified on
  every PR, but a new native aarch64 capture needs a native arm64 runner.
- **Anything derived from git history.** No workflow sets `fetch-depth`, so `actions/checkout@v4`
  takes its default of a single commit. A check that needs history cannot run in CI. The visible
  consequence: golden provenance records bind to a content digest of the checked-in bytes rather
  than to the engine revision that produced them
  (`crates/oce-cxf/tests/golden_provenance/mod.rs:3-5`).
- **Line-ending behavior.** `.gitattributes:1` pins `* text=auto eol=lf`, but an ubuntu-only CI
  never performs a CRLF checkout, so that normalization is asserted by git configuration and
  exercised by no test. Goldens here are compared bit-exactly, which is precisely where a stray
  `\r` would show up.

The script says the rest itself, in its closing report: a green local run
does **not** prove the cross-arch determinism matrix passes (one machine cannot reproduce it), does
not prove the two `cargo public-api` surface gates pass (they need the gate-only nightly), does not
prove `cargo deny check advisories` passes, and does not prove that the script and `ci.yml` still
agree. (The script's own closing report still names `ubuntu-24.04-arm` and `ci.yml`; the script
is a bound source of the retained strict-bit evidence, so its text changes only with an evidence
refresh.) A `light` run additionally does not prove the workspace suite or doctests pass, because the
per-PR gate does not run them.

## Publishing

`release.yml` stays on the GitHub mirror, because publishing needs the crates.io credentials held
there; it is dormant while GitHub Actions is disabled. It is decoupled from both gates and from
each other's triggers. Pushing a `v*` tag runs
verify only — tag/version match, fmt, clippy, a workspace `cargo test`, and a full
`cargo publish --dry-run` — with no token and no publish, so a tag can be re-cut safely
(see [release.yml](../.github/workflows/release.yml), `verify` job). Publishing is a separate manual `workflow_dispatch` into the `release` GitHub
Environment. Cargo's workspace selection includes the 12 publishable members and skips the five
members with `publish = false`; the exact split and feature closure are guarded by the
[package, feature, and publication policy](package-publication-policy.md). No crate is on crates.io
yet, and actual publication remains deferred pending explicit owner authorization.

Related: [`host-responsibilities.md`](host-responsibilities.md) for what the engine deliberately
leaves to the embedder, and [`../TESTING.md`](../TESTING.md) for the testing standard a change is
expected to meet.
