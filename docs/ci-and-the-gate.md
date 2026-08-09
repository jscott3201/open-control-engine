# CI and the gate

For contributors, and for anyone looking at a green check mark on a pull request and wondering what
it proves. The short answer is: less than you would assume. The split is deliberate, and it is easy
to misread in the dangerous direction.

## One command, one source of truth

[`.agents/gate.sh`](../.agents/gate.sh) is the only place the gate's command list is written down.
Every other document in this repo — including this page — points at it rather than restating it,
because nine divergent prose copies existed before the script was written and two of them were
materially weaker than CI (`.agents/gate.sh:3-7`). There are two invocations:

```
bash .agents/gate.sh        # light — mirrors the per-PR gate
bash .agents/gate.sh full   # full  — adds the workspace suite and doctests
```

CI does not merely mirror that script, it **executes** it: the `gate (light)` job at
`.github/workflows/ci.yml:273-289` and `gate (full)` at
`.github/workflows/release-gate.yml:334-349`.
So every command in the script gates a pull request whether or not `ci.yml` also runs it as its own
job. Read that as coverage, not as parity, and note that the implication does not run the other way:
`gate (light)` is `bash .agents/gate.sh` **plus** any steps of its own. The Quickstart-executes step
was exactly that for a while — a required check no local run of the script performed — and an
earlier revision of this paragraph cited the job as `ci.yml:256-270`, stopping one line short of it.
Nothing verifies mechanically that the two files still list the same commands. That check was
attempted and withdrawn, and `ci.yml:244-267` records why —
every design either compared argv strings that `RUSTFLAGS=--cap-lints=allow` leaves byte-identical
while neutering clippy, or reimplemented enough of GitHub's `if:`/`needs:`/matrix semantics to
become its own untested gate.

The script's steps group into: formatting, file-size and secret hygiene; the repository-invariant
gates (the default build links no database or async runtime; the golden generator cannot bless its
own output as the oracle); behavior fixtures for those gates, because a gate that cannot fail is not
a gate; build, clippy and rustdoc under `-D warnings`; supply-chain checks; the determinism subset;
and two fixture input-hygiene audits. A failing step never aborts the run, so one round trip reports
every problem instead of the first (`.agents/gate.sh:41-56`).

## Dev-light, release-heavy

**A green PR is not evidence that the change's own tests pass.**

The per-PR gate into `development` runs engine tests for **`oce-blocks` and `oce-expr` only**. That
is the `determinism-matrix` job: two runners, `ubuntu-latest` and `ubuntu-24.04-arm`
(`ci.yml:148-156`), each running that two-crate subset twice — once under debug codegen, once under
release codegen (`ci.yml:165-168`). No other crate's test suite runs. The gate script additionally
runs two named `oce-cxf` test binaries, and it is explicit that they are input hygiene rather than
engine coverage: the port-order audit sweeps 47 CXF documents, of which 46 are Guideline 36 catalog
fixtures and one is a resolver contract; the structural oracle compares the catalog fixtures it can
pair with vendored modelica-json translations
(`.agents/gate.sh:126-154`). That oracle compares document structure — instances and undirected
edges — not simulated behavior.

Everything else waits for the release gate. A change confined to `oce-api`, `oce-cxf`, `oce-store`,
`oce-conformance` or `oce-diag` can show a fully green PR having executed none of its own tests.
Before claiming tests pass, run `bash .agents/gate.sh full` first-hand and read the tail.

## Draft pull requests run nothing

Not a reduced subset — nothing. All fourteen jobs in `ci.yml` are conditioned on
`github.event.pull_request.draft == false || github.event_name == 'workflow_dispatch'`, from
`ci.yml:55` through `ci.yml:275`. A draft PR with no checks looks a lot like a PR with no failing
checks. Confirm the checks actually ran.

## cargo-deny is not skippable, but advisories do not gate a PR

The standalone `cargo-deny` job in `ci.yml:229-242` is conditional on a manifest change, computed by
the paths filter at `ci.yml:64-69`. That conditional does not make the check skippable: the gate
script runs cargo-deny's bans, licenses and sources checks unconditionally
(`.agents/gate.sh:110-114`), and CI runs the script. Leaving manifests alone does not dodge it.

`advisories` is a different story, and the carve-out belongs next to the claim. It is deliberately
excluded from the script — it needs network access and a writable advisory database, neither of
which a sandboxed lane has. It runs daily in `advisories.yml` (`advisories.yml:11-14, 38`) and on
release PRs (`release-gate.yml:310-322`). `advisories.yml` has no `pull_request` trigger at all, so
a PR into `development` that introduces a dependency with a known RustSec advisory merges green and
is caught by the next scheduled run, not by its own gate.

## What the release gate adds

`release-gate.yml` fires on `development` → `main` PRs, on manual dispatch, and on a daily cron
against the `development` tip (`release-gate.yml:46-54`). It is disjoint from `ci.yml` by base
branch, so the two never both fire on one PR. It re-runs the light correctness gates against the
release tip and adds four things:

| Step | What it covers | Where |
| --- | --- | --- |
| workspace nextest | every unit and integration test in all 17 crates | `release-gate.yml:109-110` |
| workspace nextest, release codegen | release panic-freedom, `debug_assert` paths stripped; inherited `ci-release` runner policy | `release-gate.yml:114-115` |
| `cargo test --doc` | doctests — nextest cannot run them, so this is a separate step | `release-gate.yml:117-118` |
| two `cargo public-api` surface gates | exact public API text for `oce-api` and `oce-store` | `release-gate.yml:136-153` |

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
uploads the determinism-matrix and release-suite reports for 14 days, including failure output and
ignored tests.

Partitioning and build archives are deliberately off: the full test execution takes seconds while
compilation dominates, and each determinism runner must execute the complete selected set under its
own architecture and codegen mode. Experimental record/replay is also off in CI; enabling a feature
that nextest still marks unstable would make the gate depend on a non-stable format. Test groups and
thread reservations remain available when measurement identifies a shared resource or heavy test;
none is known today.

The public-api baselines are the strongest stability evidence in this repo. They are checked-in
text files — `crates/oce-api/tests/public-api.txt` (1230 lines) and
`crates/oce-store/tests/public-api.txt` (1230 lines) — and the tests at
`crates/oce-api/tests/public_api.rs` and `crates/oce-store/tests/public_api.rs` diff the crate's
real surface against them, so any unintended addition, removal or signature change fails the gate
rather than shipping. Two env vars interlock to keep the gate honest: `OCE_PUBLIC_API_NIGHTLY` arms
it and names the pinned nightly to shell out to, and `OCE_REQUIRE_SURFACE_CHECK=1` turns a missing
nightly into a hard panic instead of a silent skip, so disarming the gate turns it red, never green
(`release-gate.yml:127-153`). The two crates run as separate steps on purpose: merging the package
selectors would let one surviving crate hide the other's vanished test.

## What CI cannot observe

- **Operating systems other than Linux.** Every `runs-on:` in all five workflows — `ci.yml`,
  `release-gate.yml`, `advisories.yml`, `release.yml`, and `docs-pages.yml` (per-PR on `docs/**`,
  `README.md`, `scripts/docs/**`, and `site/**`) — is `ubuntu-latest` or `ubuntu-24.04-arm`.
  Cross-*architecture* is covered — x86_64 and arm64, debug and release. macOS and Windows are not
  built or tested anywhere.
- **Anything derived from git history.** No workflow sets `fetch-depth`, so `actions/checkout@v4`
  takes its default of a single commit. A check that needs history cannot run in CI. The visible
  consequence: golden provenance records bind to a content digest of the checked-in bytes rather
  than to the engine revision that produced them
  (`crates/oce-cxf/tests/golden_provenance/mod.rs:3-5`).
- **Line-ending behavior.** `.gitattributes:1` pins `* text=auto eol=lf`, but an ubuntu-only CI
  never performs a CRLF checkout, so that normalization is asserted by git configuration and
  exercised by no test. Goldens here are compared bit-exactly, which is precisely where a stray
  `\r` would show up.

The script says the rest itself, in its closing report (`.agents/gate.sh:180-207`): a green local run
does **not** prove the cross-arch determinism matrix passes (one machine cannot reproduce it), does
not prove the two `cargo public-api` surface gates pass (they need the gate-only nightly), does not
prove `cargo deny check advisories` passes, and does not prove that the script and `ci.yml` still
agree. A `light` run additionally does not prove the workspace suite or doctests pass, because the
per-PR gate does not run them.

## Publishing

`release.yml` is decoupled from both gates and from each other's triggers. Pushing a `v*` tag runs
verify only — tag/version match, fmt, clippy, a workspace `cargo test`, and a full
`cargo publish --dry-run` — with no token and no publish, so a tag can be re-cut safely
(`release.yml:39-71`). Publishing is a separate manual `workflow_dispatch` into the `release` GitHub
Environment (`release.yml:73-87`). The crates are not on crates.io yet.

Related: [`host-responsibilities.md`](host-responsibilities.md) for what the engine deliberately
leaves to the embedder, and [`../TESTING.md`](../TESTING.md) for the testing standard a change is
expected to meet.
