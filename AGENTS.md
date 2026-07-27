# open-control — Agent Guide

**open-control** (product name **Open Control Engine**; repo
`github.com/jscott3201/open-control-engine`) is a high-performance, **embeddable Rust control engine**
for smart-building equipment. Published crates use the **`oce-*`** prefix; the embeddable host facade
(`oce-api`) is published under the umbrella name `open-control-engine`. Its north-star specification is
the **OBC / LBL Control Description Language (CDL)**:

- https://obc.lbl.gov/specification/cdl.html
- https://obc.lbl.gov/specification/index.html
- https://github.com/lbl-srg/modelica-buildings/tree/master/Buildings

It is built to be the **core engine under the hood** for future Aionforge projects, so the public
API and core semantics must stay embeddable and stable. The library ships **no first-party
database** — it is database-free, full stop; durable/queryable backends (e.g. the in-house graph
database **selene-db**, https://github.com/jscott3201/selene-db) are the consuming application's
responsibility, authored app-side behind the storage port.

---

## 🧭 Start here

Read [`.agents/project-facts.md`](.agents/project-facts.md) before your first change. It
covers the gate, what CI does and does not run, and the conventions this repo enforces
mechanically.

If you are an agent working on this project, `.agents/` also holds local operating notes
kept out of the published tree — the memory protocol and identity handling in
`memory-bootstrap.md`, and delegation and review process in `lane-facts.md`. They are
gitignored, so they exist only in a working checkout. If you cloned this repo and they are
absent, you are a contributor and `project-facts.md` is what you need.

That arrangement governs what is published **from here on**. Earlier revisions of this
file, and a since-deleted `.claude/skills/codex-handoff/SKILL.md`, carried some of the same
orchestration material and remain readable in this repository's public git history. Nothing
there is a credential or an identity value, so it is accepted rather than remediated —
deleting a file stops publishing it, it does not unpublish it.

---

## 📂 Repository layout

Everything below is local-only unless marked tracked. `.gitignore` excludes every
top-level `_*/` directory, so a clone contains none of the working directories except
the four force-added files under `_spec/oce_g36_gap_specs_v1/reference/` — if you
cloned this repo, you will not find the rest, and that is expected.

| Path | Tracked? | Purpose |
| --- | --- | --- |
| `crates/` | tracked | The engine. Published crates use the `oce-*` prefix. |
| `.agents/` | partly | `gate.sh` and `project-facts.md` are tracked; other files there are local operating notes. |
| `_spec/` | mostly local-only | Architecture and engine specification — the design of record before code. Four files under `_spec/oce_g36_gap_specs_v1/reference/` — two conformance fixtures and two reference documents — are force-added and therefore tracked; nothing else under `_spec/` is. |
| `_research/` | local-only | Research findings that feed the spec. |
| `_codex-briefs/`, `_review/`, `_tracker/` | local-only | Working artifacts. |
| `*-id.json` | never committed | Machine-local agent identities. |

---

## 🛠️ Working norms

- Spec before code: land architecture decisions in `_spec/` before scaffolding crates.
- **Testing standard (safety-critical):** this engine controls real equipment — a wrong result
  is a physical hazard, so testing is a **first-class deliverable, not an afterthought**. Every
  PR ships **extensive edge-case tests, golden tests (checked-in expected outputs compared
  bit-exactly), oracle cross-checks, and determinism goldens** per `TESTING.md`. Thin coverage
  is a **blocking review defect**. See `TESTING.md` for the full standard.
- **The gate:** `bash .agents/gate.sh` mirrors the per-PR gate; `bash .agents/gate.sh full` adds
  the workspace suite and doctests. That script is the single source of truth for gate commands —
  do not restate them here or anywhere else, and change one only by changing `ci.yml` first. Nine
  divergent copies of the command list existed before it was written, two of them materially
  weaker than CI.
- **CI is dev-light / release-heavy.** The per-PR gate into `development` runs engine tests for
  **`oce-blocks` and `oce-expr` only** (the `determinism-matrix` job, x86_64 and arm64, debug and
  release codegen). Every other crate's tests run only on `development` -> `main` release PRs via
  `release-gate.yml`. **A green PR is therefore not evidence that a change's own tests pass** —
  run `bash .agents/gate.sh full` first-hand before claiming they do. cargo-nextest is the runner
  (`.config/nextest.toml`: `default` = fast local fail-fast, `ci` = the release gate); it cannot
  run doctests, which is why they are a separate step. The git hooks run no tests — keep commits
  and pushes fast.
- **Project facts:** what this repo expects of a change, beyond what the code says, lives in
  `.agents/project-facts.md` — the gate, the CI split, the clippy feature rule, the gitignored
  working directories, and disk hygiene. Read it before your first PR.

### Naming, modularization & docs

- Identifiers name the thing, never the change. Banned in any file, module, type, function,
  constant, or test name: lane tags `a1` through `a6`, milestones `m0`/`m1`/`m2`, workstream
  `c1`, `pr`/`prN`, `phaseN`, `laneN`, and split-counter or grab-bag suffixes such as `foo2`,
  `_extra`, `_more`, `misc`, and `util2`. Bookkeeping belongs in commits, PRs, and Aionforge
  Memory, not source names.
- Block-family modules are named for the CDL namespace: `reals*`, `logical*`, `integers*`,
  `conversions`, and `discrete`. A size-split suffix must name a behavioral sub-topic such as
  `logical_latch`, `logical_timing`, `reals_filters`, or `reals_integrator`; never use a number.
  Types mirror the CDL class last segment in `UpperCamelCase`, the `class_path` string is the
  canonical identity, and registry constructors stay `make_<snake_block_name>`.
- Test functions name the property or scenario. They do not name the function under test and do
  not carry numbered prefixes. Test files mirror their source module, for example
  `reals_filters.rs` and `reals_filters_tests.rs`.
- Every public item has rustdoc covering what it is, what it does, invariants, units, and panic
  behavior where applicable. Every module has a `//!` header. Keep the 700-LOC cap by using
  per-family modules and `tests/` trees for scenario suites.
