# Project facts

Things worth knowing before you change this repo that the code does not tell you.

## The gate

```bash
bash .agents/gate.sh          # mirrors the per-PR gate
bash .agents/gate.sh full     # adds the workspace suite and doctests
```

That script is the single source of truth for gate commands. Every other document
points at it. Run it in the form above and read the real output — a summary of a
gate is a claim about a gate, and this engine controls physical equipment.

Change a command by changing [`.github/workflows/ci.yml`](../.github/workflows/ci.yml)
first, then the script.

## A green PR does not mean the tests passed

CI is dev-light and release-heavy, and the split is easy to misread in the
dangerous direction.

The per-PR gate into `development` runs fmt, clippy, build, rustdoc, the file-size
cap, the no-secret scan, the database-free check, the golden-gen firewall, the gate
fixtures, `cargo machete` — and engine tests for **`oce-blocks` and `oce-expr`
only**, via the determinism matrix on x86_64 and arm64 in debug and release codegen.
`cargo-deny` runs only when a manifest changed.

Every other crate's tests run **only** on the `development` → `main` release gate. A
change confined to `oce-cxf`, `oce-store`, `oce-api`, or `oce-diag` can show a fully
green PR having executed none of its own tests. Before claiming your tests pass, run
`bash .agents/gate.sh full` and read the tail.

**Open the PR non-draft.** Every job in `ci.yml` is conditioned on
`github.event.pull_request.draft == false`, so a draft PR runs *no* gates at all — and
a PR with no checks is easy to mistake for a PR with no failing checks. Confirm the
checks actually ran, not merely that none are red.

## Clippy lints the default feature set

`cargo clippy --workspace --all-targets --locked -- -D warnings`, matching CI.

Do not add `--all-features`. `oce-api` declares `default = ["mem"]`, and the promise
this repo gates on is that the *default* build links no database and no async
runtime. Linting all features checks a configuration that never ships, and lets a
default-build regression through.

## The no-secret gate rejects more than secrets

`.github/scripts/check-no-secrets.sh` runs in CI and in the pre-commit hook. Besides
the usual credential shapes it fails on any UUID-shaped string and any absolute
`/Users/...` or `/home/...` path in tracked content. Both patterns are there because
a developer path or an identifier pasted into a committed file is a leak that has to
be scrubbed from history rather than simply deleted.

Keep absolute paths out of committed files, including scripts and doc examples.

## Working directories are gitignored

`.gitignore` excludes every top-level `_*/` directory, so `_research/`, `_review/` and
`_tracker/` are absent from a clone, and `_spec/` is present only as the four
force-added files under `_spec/oce_g36_gap_specs_v1/reference/`. Two consequences bite
in practice:

- **`git add -A` silently stages nothing** for a new file under those paths, and
  exits 0. Those four files — two conformance fixtures and two reference documents —
  are tracked only because they were force-added. Adding another needs
  `git add -f <exact path>`.
- A reference to `_spec/...` points at a file most clones do not have. Quote the
  excerpt you depend on rather than citing the path alone.

## Run `cargo clean` between PRs

At every merge boundary, before the next branch is cut, clean the tree you just
worked in. This workspace builds large: a single review worktree has reached 4.9 GB
of `target/` against 1.2 GB in the main checkout, and artifacts otherwise drag
forward from one PR into the next indefinitely.

Two things make this safe to do and easy to get wrong:

- **Never clean a tree while a build is running in it.** A build mid-gate in that
  tree fails in a way that looks like a code defect.
- **Worktrees have independent `target/` directories** — `.cargo/config.toml` sets no
  shared `[build] target-dir`. Cleaning the main checkout cannot disturb work in a
  worktree, and vice versa. Clean each tree on its own schedule.

## Branches

Base branch is `development`. Branch protection blocks direct pushes to it;
everything lands by squash-merge through a PR. A fix round pushes to the **same**
branch — never a second PR for the same work.
