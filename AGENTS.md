# open-control — Agent Guide

**open-control** (product name **Open Control Engine**; repo
`github.com/jscott3201/open-control-engine`) is a high-performance, **embeddable Rust control
engine** for smart-building equipment. Crates use the **`oce-*`** prefix; the embeddable host
facade is the package **`oce-api`**. **Nothing is published to crates.io yet** —
`open-control-engine` is a reserved umbrella name, not a current alias. Its north-star
specification is the **OBC / LBL Control Description Language (CDL)**:

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
3. [`docs/architecture.md`](docs/architecture.md) — the layers, the CDL §7.17 seam, what the engine
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

**Prove.** Before opening the PR run `bash .agents/gate.sh full` first-hand and **paste the tail of
the real output** into the PR description. A summary of a gate is a claim about a gate, and this
engine controls physical equipment. Then confirm the 46 fixture goldens and 422 provenance goldens
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
both; the merge itself runs no gate) and any requested review is addressed. Branches delete on
merge. Release PRs go `development → main` through `release-gate.yml` and are tagged `v0.x` at the
end of each milestone.

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

---

## 🌐 Hosting and CI

- **GitHub is the host**: `origin` is `github.com/jscott3201/open-control-engine`; branches, PRs,
  issues, milestones, reviews and CI live there. Use `gh` for PR and issue operations. (A
  self-hosted Forgejo instance was primary until October 2026; its PR numbers overlap GitHub's, so
  cite PRs merged on GitHub as `(GitHub #N)`.)
- **CI is GitHub Actions** in `.github/workflows/`, on Linux runners plus two `pr-gate.yml`
  platform observation cells (macOS arm64 portable snapshot, Windows std build and nextest). The single required status is `CI OK`: from
  `pr-gate.yml` into `development`, from `release-gate.yml` into `main`. `pr-gate.yml` runs once
  per change, on the PR; a merge into `development` runs no gate, and the daily `release-gate.yml`
  run on the `development` tip is the safety net. `advisories.yml` runs
  daily, `docs-pages.yml` validates docs and publishes `main` to GitHub Pages, `release.yml` keeps
  its tag-verify / manual-publish split, and the two OpenModelica evidence workflows run on native
  arm64 by hand.
- **Two lanes, one `CI OK`.** The first `pr-gate.yml` job classifies the PR's changed paths
  (`scripts/ci/classify-changes.sh`, fail-closed). Anything outside the docs and `crates/leverage/`
  runs the **determinism lane** (every compiling job); `crates/leverage/**` runs the **leverage
  lane** (fmt, clippy, rustdoc, nextest and the `oce-api`-only rule per leverage crate) and so does
  every engine change; a docs-only PR runs only the always-on checks. `gate (light)` runs only when
  a gate script, workflow or tool config changes, and such a change runs everything. `CI OK`
  accepts a skipped job only when its lane filter skipped it. `bash .agents/gate.sh leverage` is
  the local mirror of a leverage-only PR; [`docs/ci-and-the-gate.md`](docs/ci-and-the-gate.md#lanes-what-a-pr-runs)
  has the trigger table.
- **`ci.yml` is not the CI.** It is the frozen GitHub-era gate that produced the retained native
  strict-bit capture archives; it is disabled in the Actions settings and stays byte-identical.
- **The determinism and strict-bit matrices run natively**: x86_64 on `ubuntu-latest` and aarch64
  on `ubuntu-24.04-arm`, debug and release, four cells each, with fan-in jobs for the cross-cell
  comparisons. Nothing is emulated; an unavailable runner leaves `CI OK` unreported (no merge).
- **CI is dev-light / release-heavy.** A green PR is not evidence that a change's own tests pass.
  Which crates' suites run per PR, which only in the release gate, and which commands are CI-only
  for now is spelled out in [`.agents/project-facts.md`](.agents/project-facts.md) and
  [`docs/ci-and-the-gate.md`](docs/ci-and-the-gate.md); run `bash .agents/gate.sh full` yourself.
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
- **The gate:** `bash .agents/gate.sh` mirrors the per-PR gate; `bash .agents/gate.sh full` adds
  the workspace suite and doctests. The script is the single source of truth for gate commands —
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
