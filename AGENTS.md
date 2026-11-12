# open-control — Agent Guide

**open-control** (product name **Open Control Engine**; repo
`github.com/jscott3201/open-control-engine`) is a high-performance, **embeddable Rust control
engine** for smart-building equipment. Crates use the **`oce-*`** prefix; the embeddable host
facade is the package **`oce-api`**. **Nothing is published to crates.io yet** —
`open-control-engine` is a reserved umbrella name, not a current alias. Its north-star
specification is the **Control Description Language (CDL)**, published as **ANSI/ASHRAE Standard
231-2026**. The project reads the public OpenBuildingControl (OBC) specification, the informal
text of the same language, and the Modelica Buildings Library; it has read the standard's table of
contents, not its text:

- https://obc.lbl.gov/specification/cdl.html
- https://obc.lbl.gov/specification/index.html
- https://github.com/lbl-srg/modelica-buildings/tree/master/Buildings

It is the **core engine under the hood** for Aionforge projects (`mechanical-muse`,
`open-control-library`, `rusty-bacnet` integration), so the public API and core semantics stay
embeddable, deterministic and database-free. The engine is **greenfield, pre-1.0**: design for best
fit, not for backwards compatibility; APIs freeze at 1.0.

`CLAUDE.md` is a symlink to this file.

---

## 🧭 Start here

Read in this order, every session, before the first change:

1. This file.
2. [`.agents/project-facts.md`](.agents/project-facts.md) — the gate, what CI does and does not run,
   the conventions this repo enforces mechanically.
3. [`docs/architecture.md`](docs/architecture.md) — the layers, the OBC §7.17 seam, what the engine
   will never do.
4. [`TESTING.md`](TESTING.md) — the four pillars; thin coverage is a blocking defect.
5. The GitHub issue you are assigned, then its milestone's tracking issue.

The owner's planning record is two documents kept out of the tree: the **Review & Growth
Ideation** doc (reasoning and decisions) and the **Milestone Work Plan** (M1–M9, library track,
gates). Local checkouts carry exports in `_spec/`. GitHub issues are cut from the plan; **do not
re-litigate a decision recorded there inside a PR** — comment on the issue and keep building.

Other files under `.agents/` beyond `gate.sh`, `project-facts.md` and `revendor-upstream.sh` are
gitignored, checkout-local operating notes. If you cloned this repo and they are absent, you are a
contributor and `project-facts.md` is what you need. The [product contract](docs/product-contract.md)
owns versioned product requirements and their evidence map.

---

## 🎯 Milestone work: issues → branch → PR → merge

Milestone work is executed by coding agents starting cold, so the process is the same for every
work item.

**Pick up.** Work only from an open issue in the current milestone (GitHub Milestones; the
`tracking` issue lists the order and the hard dependencies). An issue labelled `agent-ready`
names its entry points, acceptance checks and a "do not touch" list — those are the contract. An
issue labelled `blocked` waits on another; read the body. If the issue's spec is thin, write the
one-page design note first (`docs/design/<topic>.md`), open it as a **draft PR**, and wait for a
review comment before building (`design-note-first`).

**Branch.** From `development`: `<area>/<what-it-does>` (`facade/pure-engine`,
`ci/native-arm64-runners`). Never a lane tag, milestone or issue number as the name.

**Build.** One PR per issue unless two are inseparable (then say so). Keep the diff to the issue's
scope; a fix you notice elsewhere is a new issue, not a drive-by.

**Prove.** Before opening the PR run the local proof **matched to the lane the PR runs**,
first-hand, on the final head, and **paste the tail of the real output** into the PR description. A
summary of a gate is a claim about a gate, and this engine controls physical equipment.

| The PR's changed paths | Local proof |
| --- | --- |
| docs only | `bash .agents/gate.sh docs`: the always-on checks, no cargo build |
| `crates/leverage/**`, docs, the root `README.md` | `bash .agents/gate.sh leverage` |
| anything else (the engine lane), the core four, `oce-cxf` and `oce-api` included | `bash .agents/gate.sh`, which runs no release codegen |
| a pin advance (`third_party/**`, `tools/reference-catalog/**`, or either of the two pin-guard files `scripts/ci/proof-row.sh` lists), `.agents/gate.sh` itself, `.config/nextest.toml` or `scripts/ci/install-wasmtime.sh` | `bash .agents/gate.sh full`: the engine-lane proof plus the determinism subset under release codegen, the workspace suite in debug and release codegen, the doctests and the blob round trip |

`bash scripts/ci/proof-row.sh` names the row from a list of changed paths (it holds the exact
path lists), and the closing report of every gate run prints the row the checkout needs beside
the row the run covered. Each row runs everything the rows above it run. **No row owes a
release-codegen run** (issue #556): `pr-gate.yml` runs the determinism subset in all four native
cells, debug and release codegen on both architectures, with the debug-against-release
comparisons, on every determinism-lane PR, so the `determinism` row issue #545 introduced is
gone. `bash .agents/gate.sh determinism` remains as a mode, to reproduce a red release cell on
one host. The last row is for what only the workspace suite or `full` mode itself
checks: for a pin advance, `oce-conformance`'s `conformance` binary, three of whose modules read the vendored tree and
the reference catalog, that crate's `clean_room` unit tests and `open_modelica_nand` binary, which
pin the vendored `Nand.mo` by path, line and equation, and the doctests (the pin-guard tests themselves run per PR); and the
script's own `full`-only steps, with the two files only they exercise. `full` is also
what CI executes nightly; no other PR owes it. **A change to `release-gate.yml`, or to anything
else only `full` or the nightly run executes, also dispatches that workflow on the branch
(`gh workflow run release-gate.yml --ref <branch>`) and waits for it to finish green before
merging;** no local run stands in for it. That run posts its own `CI OK` on the head, without the
strict-bit matrix: the merge decision still reads `CI OK` from the `pr-gate` run on the head SHA.
No row runs every crate's suite: also run the tests of each crate the PR changes
(`cargo nextest run -p <crate> --locked --profile ci --no-tests=fail`). A rebase onto
`development` **with no conflicts** needs `CI OK` reported on the new head, not a local re-run; a
rebase with conflicts, or a fix round, re-runs the row's proof. Then confirm the 46 fixture goldens and 422 provenance goldens
are byte-identical (`git status --porcelain -- tools/golden-gen/goldens` prints nothing) unless the
issue says the PR exists to regenerate them — then the description says why and which.

**PR.** Title `type(scope): what it does` in the imperative (`fix(cxf): …`, `ci: …`,
`refactor(api): …`). Body: `Closes #N`, what changed and why in two paragraphs, the gate tail, the
goldens line, the `CHANGELOG.md` entry you added (every behaviour, surface or published-claim change
gets one **in the same PR**, cited `(GitHub #N)`), and the docs you updated. A fix round pushes to
the **same branch**; never a second PR for the same work.

**Merge.** `CI OK` is the one required status; confirm it **reported**, not merely that nothing is
red (a PR with no checks looks like a PR with no failing checks). Squash-merge into `development`
once CI OK is green on a branch that is up to date with `development` (branch protection requires
both) and any requested review is addressed. The merge is a push to `development`, which starts
`merge-gate.yml` on the merge commit: the Windows cell, the one job no pull request runs
([CI and the gate](docs/ci-and-the-gate.md#what-runs-after-the-merge)).
**After merging a change that runs the determinism lane, watch that merge-gate run** (its
aggregate is `Merge gate OK`); a red run is fixed forward or reverted before the next
determinism-lane merge. (A run still waiting when a later merge lands shows as cancelled: the later
run, whose commit contains it, is the evidence.) The heavy set — the workspace suite in debug and
release codegen, the doctests, the executed `gate (full)` and cargo-deny with advisories — runs
**nightly** in `release-gate.yml` on the `development` tip. **Before a determinism-lane merge,
check the latest nightly:**
`gh run list --workflow release-gate.yml --branch development --event schedule --limit 1`.
The merge needs that run not red, or its failure already being fixed forward (an open PR or
issue that names the run); a red nightly nobody has picked up is fixed forward or reverted
first. Branches delete on merge. Release PRs go
`development → main` through `release-gate.yml`, which runs the whole heavy set on them, and are
tagged `v0.x` at the end of each milestone.

**Standing rules for every PR**

1. No `unsafe`, no `todo!`, no database, no async runtime below `oce-api`, no network listener, no
   `.mo` parser. `missing_docs` is deny.
2. Determinism is the invariant. A PR that touches the core four (`oce-model`, `oce-expr`,
   `oce-blocks`, `oce-graph`) or `oce-cxf` leaves every golden byte-identical, or exists to
   regenerate them and says so.
3. `.github/workflows/ci.yml` is frozen and disabled: never edit it. Native strict-bit evidence is
   re-established on every PR by the native cells in `pr-gate.yml`, so no source file is bound to a
   stored receipt any more; the strict-bit evidence tests under
   `crates/oce-conformance/tests/strict_bits/` and their fixtures still change only when an issue
   says to. `docs/strict-bit-evidence.md` describes what the per-PR matrix proves.
4. Change a gate command by changing `.github/workflows/pr-gate.yml` **first**, then
   `.agents/gate.sh`. Never add `--all-features` to clippy (see project-facts).
5. Leverage crates live under `crates/leverage/<name>` and depend on `oce-api` only, never on a
   core crate directly. They run in the lighter leverage CI lane; a change to the core four always
   runs the determinism lane.
6. Keep absolute paths (`/Users/...`, `/home/...`) and UUID-shaped strings out of every tracked
   file; the no-secret gate rejects them and history then has to be scrubbed.
7. Sequences, routines and faults live in `open-control-library`, never here; this repo keeps only
   the minimal conformance corpus.
8. `cargo clean` the tree at every merge boundary; never while a build is running in it.
9. Every format and schema this project defines stays at revision 1 until 1.0.0, the content-id
   tag `oce:canon1:` and each stateful class's `state_contract_revision` included: change it in
   place and re-bless its fixtures and goldens, with no importer, mapping table or dual-read path.
10. Because those revisions stay at 1, the engine version is what tells artefacts of one layout from
    another: the first PR after a release tag that changes a format's bytes or meaning bumps the
    workspace version on `development` to the next milestone's version (unless it is already
    bumped), re-blesses, and includes a test that an artefact of the previous layout is refused
    with a typed error. A compiled-schedule blob and a state snapshot each carry the engine
    version in their header (major, minor and patch from one constant,
    `oce_graph::blob::ENGINE_VERSION`; a pre-release suffix is not recorded) and are accepted
    only by the version that wrote them: any other is `EngineVersionMismatch`, on `BlobError`
    and on `EngineStateError` (issue #530). The `oce:canon1:` tag and each class's
    `state_contract_revision` stay at 1 with the rest (rule 9), so the same version is what
    separates their meanings.

---

## 🌐 Hosting and CI

- **GitHub is the host**: `origin` is `github.com/jscott3201/open-control-engine`; branches, PRs,
  issues, milestones, reviews and CI live there. Use `gh` for PR and issue operations. (Pull
  requests merged on the previous host before the October 2026 move share numbers with GitHub
  PRs, so cite PRs merged on GitHub as `(GitHub #N)`.)
- **CI is GitHub Actions** in `.github/workflows/`, on Linux runners plus two platform observation
  cells: the macOS arm64 portable snapshot in `pr-gate.yml` and the Windows std build and nextest
  in `merge-gate.yml` and `release-gate.yml`. The single required status is `CI OK`: from
  `pr-gate.yml` into `development`, from `release-gate.yml` into `main`. `pr-gate.yml` runs once
  per change, on the PR. A merge into `development` then runs `merge-gate.yml` on the merge
  commit — the Windows cell alone, not a required
  status, so a red run is fixed forward or reverted — and `release-gate.yml`, the heavy gate, runs
  nightly on the `development` tip, on dispatch, on release PRs and on pushes to `main`. `advisories.yml` runs
  daily, `docs-pages.yml` validates docs and publishes `main` to GitHub Pages, `release.yml` keeps
  its tag-verify / manual-publish split, and the two OpenModelica evidence workflows run on native
  arm64 by hand.
- **Three lanes, one `CI OK`.** The first `pr-gate.yml` job classifies the PR's changed paths
  (`scripts/ci/classify-changes.sh`, fail-closed). Anything outside the docs, the root `README.md`
  and `crates/leverage/` runs the **determinism lane** (every compiling job); `crates/leverage/**`
  runs the **leverage lane** (fmt, clippy, rustdoc, nextest and the `oce-api`-only rule per
  leverage crate) and so does every engine change; the root `README.md` runs the **README lane**
  (the `facade contract and Quickstart` job: the Quickstart byte-pin and the executed Quickstart)
  and so does every engine change; a docs-only PR runs only the always-on checks. So a landing PR
  that updates the README beside leverage or documentation paths does not run the determinism
  lane. A gate script, workflow or tool-config change counts as an engine change and runs every
  lane. No per-PR job executes `.agents/gate.sh`: `gate (full)` in `release-gate.yml` does,
  nightly. `CI OK` accepts a skipped job only when its lane filter skipped it.
  `bash .agents/gate.sh leverage` is the local mirror of a PR confined to `crates/leverage/`, the
  docs and the root README; [`docs/ci-and-the-gate.md`](docs/ci-and-the-gate.md#lanes-what-a-pr-runs)
  has the trigger table.
- **`ci.yml` is not the CI.** It is the frozen GitHub-era gate that produced the retained native
  strict-bit capture archives; it is disabled in the Actions settings and stays byte-identical.
- **The determinism and strict-bit matrices run natively**: x86_64 on `ubuntu-latest` and aarch64
  on `ubuntu-24.04-arm`, with fan-in jobs for the cross-cell comparisons. Per PR both
  matrices run four cells (debug and release), and the determinism fan-in compares debug against
  release as well as across architectures (issue #556; `release-gate.yml` runs the determinism
  matrix again nightly). Nothing is emulated; an unavailable runner
  leaves `CI OK` unreported (no merge).
- **CI is dev-light / release-heavy.** A green PR is not evidence that a change's own tests pass.
  Which crates' suites run per PR, which only in the nightly release gate, and which commands are
  CI-only for now is spelled out in [`.agents/project-facts.md`](.agents/project-facts.md) and
  [`docs/ci-and-the-gate.md`](docs/ci-and-the-gate.md); run the suites of the crates you change yourself.
  cargo-nextest is the runner (`.config/nextest.toml`); it cannot run doctests, which is why they
  are a separate step. The git hooks run no tests — keep commits and pushes fast.

---

## 📂 Repository layout

`.gitignore` excludes every top-level `_*/` directory, so a clone contains none of the working
directories — that is expected, and `git add -A` silently stages nothing under them.

| Path | Tracked? | Purpose |
| --- | --- | --- |
| `crates/` | tracked | The engine. Published crates use the `oce-*` prefix. |
| `crates/leverage/` | tracked | Crates above the facade (`oce-author`, `oce-verify`, `oce-host`, bindings…) as the milestones add them; `oce-api` only, never a core crate. |
| `docs/` | tracked | The mdBook (`architecture.md`, contracts, evidence). `docs/design/` holds one-page design notes written before a thinly specified work item. |
| `third_party/` | tracked | Vendored Modelica Buildings `.mo` classes and modelica-json CXF translations, verbatim and uncompiled, read as data by input-hygiene audits. Outside every crate root. Own license; see its README and pin-advance policy. |
| `tools/golden-gen/` | tracked | Its own cargo workspace; generates the checked-in goldens. Hygiene runs as a separate CI job. |
| `.agents/` | partly | `gate.sh`, `project-facts.md`, `revendor-upstream.sh` tracked; everything else local notes. |
| `_spec/`, `_research/`, `_review/`, `_tracker/` | local-only | Planning exports, research, working artifacts. A reference to `_spec/...` points at a file no clone has — quote the excerpt you rely on. |
| `*-id.json` | never committed | Machine-local agent identities. |

---

## 🛠️ Working norms

- **Testing standard (safety-critical):** a wrong result is a physical hazard. Every PR ships
  edge-case tests, golden tests compared bit-exactly, oracle cross-checks and determinism goldens
  per `TESTING.md`. Thin coverage is a **blocking review defect**.
- **The gate:** `bash .agents/gate.sh` mirrors the per-PR gate for an engine change, in debug
  codegen; `docs` and `leverage` mirror the lighter lanes, `determinism` adds the determinism
  subset and the strict-bit step under release codegen, which CI runs per PR in native cells, so
  no PR owes it and it is the way to reproduce a red release cell locally; `full`
  adds the workspace suite and doctests, is owed by a pin advance or a change to the script, and
  is what CI executes nightly ("Prove" above has the table). The script is the single source of truth for gate commands —
  do not restate them anywhere; its closing report names what it cannot cover locally, and a green
  local run is not a green CI. Prose copies of the command list have diverged in both directions
  before; when the script and CI disagree, check which one is behind.
- **Spec before code:** architecture decisions land in a design note (`docs/design/`) or the
  owner's planning docs before crates are scaffolded.
- **Hostile input is the default assumption** for anything that parses: bound memory and CPU at
  load, refuse with a typed error, never panic (GitHub #381, #382 are the pattern).

### Naming, modularization & docs

- Identifiers name the thing, never the change. Banned in any file, module, type, function,
  constant, or test name: lane tags `a1`–`a6`, milestones `m0`/`m1`/`m2`, workstream `c1`,
  `pr`/`prN`, `phaseN`, `laneN`, and split-counter or grab-bag suffixes such as `foo2`, `_extra`,
  `_more`, `misc`, `util`, `util2`. Bookkeeping belongs in commits, PRs and issues, not source names.
- Block-family modules are named for the CDL namespace: `reals*`, `logical*`, `integers*`,
  `conversions`, `discrete`. A size-split suffix names a behavioural sub-topic (`logical_latch`,
  `reals_filters`), never a number. Types mirror the CDL class last segment in `UpperCamelCase`,
  the `class_path` string is the canonical identity, and registry constructors stay
  `make_<snake_block_name>`.
- Test functions name the property or scenario, not the function under test, with no numbered
  prefixes. Test files mirror their source module (`reals_filters.rs` / `reals_filters_tests.rs`).
- Every public item has rustdoc covering what it is, what it does, invariants, units, and panic
  behaviour where applicable. Every module has a `//!` header. Keep the 700-LOC cap by splitting
  per family and moving scenario suites to `tests/` trees.
