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

## 🧠 Memory bootstrap → Aionforge Memory (the long-term substrate)

This file is only a **pointer**. The durable working substrate for this project is the
**Aionforge Memory** MCP server (`aionforge-memory`). Treat it as the source of truth for
decisions, rationale, open work, and handoffs — not these MD files.

**On entering this repo, before substantial work:**

1. **Read the project identity.** The project's **dedicated** Aionforge identity (`agent_id`
   and `namespace`) lives in `claude-id.json` (git-ignored, machine-local). Load it from there —
   never hardcode it — and pass it as `principal.agent_id` (and `viewer`) on every memory call.
   ⛔ Do **not** use the shared steward identity from the environment for project memory; it is
   **off-limits** for open-control.
2. **Recall first.** `search` for relevant prior decisions/preferences/failures and `work_query`
   (status `todo` / `in_progress`) for open work. Recall again when new files, errors, or
   subsystem names appear.
3. **Capture durable facts the moment they land** — decisions, corrections, validation results,
   failed approaches, release/handoff state — via `capture` / `batch_capture`. Don't batch to the
   end; a context compaction can drop them first.
4. **Track work as it moves** — `work_create` for tasks/blockers/TODOs, `work_advance` as status
   changes, `work_link` to tag. Work items are persistent and status-tracked; memory episodes
   decay.
5. **Never store secrets** (keys, tokens, credentials). User direction overrides memory.

The `aionforge-memory` plugin skills (`memory-loop`, `memory-recall`, `memory-capture`,
`work-tracking`, `memory-maintenance`) and commands (`/aionforge-memory:memory-session`,
`/aionforge-memory:memory-handoff`) encode this cadence. A SessionStart hook re-seeds it after a
fresh context, resume, or compaction.

- **Private namespace** — the project's own `agent:` namespace (from `claude-id.json`) →
  open-control project working memory.
- **Shared namespace** — the Aionforge Memory dogfooding team → cross-project dogfooding feedback
  **only**. Keep project internals out of it.

---

## 📂 Repository layout

| Path          | Purpose                                                                    |
| ------------- | -------------------------------------------------------------------------- |
| `_research/`  | Research findings (`cdl/`, `selene-db/`). Inputs to the spec.              |
| `_spec/`      | Architecture & engine specification (the design of record before code).    |
| `claude-id.json` | Machine-local Aionforge identity (git-ignored).                         |

---

## 🛠️ Working norms

- **Ultracode / workflows:** orchestrate substantive research and design via the Workflow tool;
  cap concurrency at **5–10 agents** at a time to avoid API rate-limiting (owner directive).
- Spec before code: land architecture decisions in `_spec/` (and Aionforge Memory) before
  scaffolding crates.
- **Testing standard (safety-critical):** this engine controls real equipment — a wrong result
  is a physical hazard, so testing is a **first-class deliverable, not an afterthought**. Every
  PR ships **extensive edge-case tests, golden tests (checked-in expected outputs compared
  bit-exactly), oracle cross-checks, and determinism goldens** per `TESTING.md`. Thin coverage
  is a **blocking review defect**. See `TESTING.md` for the full standard.
- **Testing & the CI gate split:** **cargo-nextest is the test runner**, locally and in CI. Run
  `cargo nextest run` for unit + integration tests and `cargo test --doc` for doctests (nextest
  cannot run doctests). Config lives at `.config/nextest.toml` (`default` profile = fast local
  fail-fast; `ci` profile = the release gate). CI is **dev-light / release-heavy**: per-PR gates
  into `development` (`ci.yml`) run fmt/clippy/build/rustdoc/file-size/no-secret/default-no-db
  (+ cargo-deny on manifest change) plus the targeted `determinism-matrix` nextest subset for
  `oce-blocks`/`oce-expr` on x86_64 and arm64 in debug and release codegen; the **full test suite
  runs only on `development` -> `main` release PRs** via `release-gate.yml` (which also re-runs the
  light gates against the release tip and runs cargo-deny unconditionally). Tests are NOT run by the
  git hooks — keep commits and pushes fast.

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
