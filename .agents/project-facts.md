# Project facts for delegated work

Constants an executor needs and cannot derive from the code. Structure, grounding
discipline, and stop semantics come from the `agent-toolkit` skills — `codex-brief`
for work orders, `codex-lane` for dispatch, `adversarial-review` for review,
`conventions` for the law they all run under. This file adds only what those skills
cannot know about this repo.

## The gate

```bash
bash .agents/gate.sh          # mirrors the per-PR gate
bash .agents/gate.sh full     # adds the workspace suite and doctests
```

That script is the only place gate commands are written down. Every other document
points at it. Run it in the form above and paste the real output — a summary of a
gate is a claim about a gate.

## A green PR does not mean the tests passed

CI is dev-light and release-heavy, and the split is easy to misread in the
dangerous direction.

The per-PR gate into `development` runs fmt, clippy, build, rustdoc, the file-size
cap, the no-secret scan, the database-free check, the golden-gen firewall, the gate
fixtures, `cargo machete` — and engine tests for **`oce-blocks` and `oce-expr`
only**, via the determinism matrix on x86_64 and arm64 in debug and release codegen.
`cargo-deny` runs only when a manifest changed.

Every other crate's tests run **only** on the `development` → `main` release gate.
A change confined to `oce-cxf`, `oce-store`, `oce-api`, or `oce-diag` can show nine
green checks having executed none of its own tests. That is why review runs
`bash .agents/gate.sh full` first-hand rather than reading the PR's check marks.

## Branches and merge

Base branch is `development`. Feature branches carry the executor as the prefix:
`codex/<slug>` for a Codex lane, `claude/<slug>` for an in-house lane. Fix rounds
push to the **same** branch — never a second PR for the same work.

**The reviewer always comes from the other model family than the implementer.** Opus
implements, Codex reviews; Codex implements, Opus reviews. This is a rule, not a route
recommendation to weigh per change — model families share blind spots, so a same-family
review is systematically weakest exactly where it needs to be strongest.

The lead may merge once **a review has been done on the PR and CI is passing**. That is
the owner's standing authorization, and this paragraph is where it is on record. An agent
never merges its own work.

Branch protection blocks direct pushes to `development`. Everything lands by
squash-merge through a PR.

## What a lane can and cannot do

A headless `codex exec` lane runs under `workspace-write` with no network. Measured,
not assumed. Briefs must respect all of it:

- **No push, no PR, no fetch.** DNS does not resolve. A lane's terminal state is
  *committed on a local branch*; the orchestrator pushes and opens the PR.
- **No new dependencies, no `cargo install`, no toolchain change.** Builds succeed
  only because every locked package is already unpacked in the local cargo registry,
  which is readable but not writable. `rust-toolchain.toml` pins the version.
- **No memory tools.** Lanes run lean, so no MCP server reaches them. Strike memory
  instructions from briefs; the orchestrator captures on the lane's behalf, which is
  where memory stewardship belongs anyway.
- **No git hooks.** A clone has no `core.hooksPath`, so the pre-commit and pre-push
  gates never fire. The lane must run `.agents/gate.sh` explicitly.
- **`cargo deny check advisories` cannot run.** It needs network and a writable
  advisory database. It lives in `advisories.yml`.

Give a Codex lane a **clone**, never a worktree, and never the main tree while
`.claude/worktrees/` is populated. Worktrees under that path sit inside the lane's
writable root while holding branches checked out elsewhere: the lane cannot check
such a branch out at all, and a `git clean -ff` inside the lane deletes the worktree
along with any uncommitted work in it. In-house lanes running outside the sandbox
have no such constraint and may use worktrees freely — this is a property of the
Codex sandbox, not of worktrees.

A brief must also name the files to copy in. `_codex-briefs/` is gitignored, so the
brief itself does not exist in a clone unless dispatch puts it there.

**A Codex lane can review even though it cannot implement-and-push.** Review needs no
network: the reviewer reads source, builds, runs the suite, and commits nothing. So the
no-network limit above constrains implementation lanes, not review lanes — which matters,
because the cross-family rule makes Codex the reviewer for every in-house change. Dispatch
still has to copy in the brief, `codex-id.json`, and any `_spec/` excerpt the review
depends on, and the brief must carry no memory instructions.

## Working directories are gitignored

`.gitignore` excludes every top-level `_*/` directory, so `_spec/`, `_research/`,
`_codex-briefs/`, `_review/`, and `_tracker/` are absent from any clone. Two
consequences bite in practice:

- **A brief citing `_spec/...` points at a file the lane does not have.** Inline the
  excerpt the work depends on, or copy the cited files in alongside the brief.
- **`git add -A` silently stages nothing** for a new file under those paths, and
  exits 0. Four conformance fixtures under `_spec/oce_g36_gap_specs_v1/reference/`
  are tracked only because they were force-added. Adding another needs
  `git add -f <exact path>`.

The durable record of decisions lives in Aionforge Memory, not in these directories.

## Identity

Two machine-local, gitignored files at the repo root: `claude-id.json` for the lead,
`codex-id.json` for Codex. Each agent loads its own. They are not interchangeable.

Never write a raw agent UUID into a tracked file. The no-secret scan rejects any
UUID-shaped string and any absolute `/Users/...` or `/home/...` path in tracked
content, and it runs both in CI and in the pre-commit hook.

## Clippy lints the default feature set

`cargo clippy --workspace --all-targets --locked -- -D warnings`, matching CI.

Do not add `--all-features`. `oce-api` declares `default = ["mem"]`, and the promise
this repo gates on is that the *default* build links no database and no async
runtime. Linting all features checks a configuration that never ships and lets a
default-build regression through.
