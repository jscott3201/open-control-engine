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

Change a command by changing [`.forgejo/workflows/ci.yml`](../.forgejo/workflows/ci.yml)
first, then the script.

## Hosting and CI

Forgejo is the primary host: `origin` is the Forgejo instance, and branches, PRs, reviews and CI
live there. GitHub is a public push mirror — never push to it directly, never open PRs there — and
public issues are tracked on GitHub and synced with Forgejo. CI is Forgejo Actions in
`.forgejo/workflows/` (one self-hosted Linux x86_64 runner); `.github/workflows/` keeps only
GitHub-bound work (release publishing, Pages publishing, native-arm64 OpenModelica evidence) plus
a byte-frozen, dormant `ci.yml`, and none of it runs while Actions is disabled on the mirror.

Three tracked files are bound sources of the retained native strict-bit evidence and must not
change casually: `.github/workflows/ci.yml`, `.config/nextest.toml`, and `.agents/gate.sh`. Editing
any of them makes `retained_native_linux_evidence_is_complete_exact_and_source_bound` fail until a
new native receipt is admitted.

## A green PR does not mean the tests passed

CI is dev-light and release-heavy, and the split is easy to misread in the
dangerous direction.

The per-PR gate into `development` runs fmt, clippy, build, rustdoc, the file-size
cap, the no-secret scan, the database-free check, the golden-gen firewall, the closed
package/feature/publication contract and its hostile controls, the gate fixtures,
the [authority index/projection and its hostile controls](../docs/authority-claims.md),
`cargo machete` — and the state-determinism tests for **`oce-api`, `oce-blocks`, and `oce-expr`**, via the
determinism matrix on x86_64 (native) and aarch64 (QEMU user-mode emulation) in debug and release
codegen.
The matrix compares a populated portable engine-state snapshot byte-for-byte across architectures,
checks portable and target-bound bytes across debug/release codegen, and requires target-bound bytes
to differ across architectures. The same job then parses and refuses the aarch64 target-bound bytes
through the public restore path on x86_64.
The standalone `cargo-deny` CI job runs `cargo deny check bans licenses sources` on every PR, and
`.agents/gate.sh` runs it too.
`advisories` is excluded from the script deliberately: it needs network and a writable
advisory-db, neither of which a sandboxed lane has, so it runs in `advisories.yml`.

The scoped `oce-conformance` strict-bit subset and four affected per-block suites also run per PR
in four Linux architecture/codegen cells (x86_64 native, aarch64 emulated), with repeat captures
and cross-cell comparison ([bounded evidence](../docs/strict-bit-evidence.md)). The remainder needs the release/full gate. A
change confined to `oce-cxf`, `oce-store`, or `oce-diag` can show a fully
green PR having executed none of its own tests. Before claiming your tests pass, run
`bash .agents/gate.sh full` and read the tail.

Pin-advance PRs — any change under `third_party/**` or to the pin constants — run
`bash .agents/gate.sh full` first-hand; see the vendored README's
`## Pin-advance policy` section.

**Confirm the checks ran.** Forgejo runs the whole per-PR gate on every PR, work-in-progress
included, and `CI OK` is the one status that summarizes it. A PR with no checks is easy to
mistake for a PR with no failing checks; confirm `CI OK` actually reported, not merely that
nothing is red.

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

`.gitignore` excludes every top-level `_*/` directory, so `_spec/`, `_research/`,
`_review/` and `_tracker/` are entirely absent from a clone. (Four
`_spec/oce_g36_gap_specs_v1/reference/` files were force-added exceptions until
2026-07-28; the two conformance fixtures among them now live at
`crates/oce-cxf/tests/fixtures/profile/`.) Two consequences bite in practice:

- **`git add -A` silently stages nothing** for a new file under those paths, and
  exits 0. Tracking a file there anyway needs a deliberate `git add -f <exact path>`
  — and a written reason.
- A reference to `_spec/...` points at a file no clone has. Quote the excerpt you
  depend on rather than citing the path alone.

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

Base branch is `development`. Branch protection on Forgejo blocks direct pushes to it
and requires `ci / CI OK (pull_request)`; everything lands by squash-merge through a
Forgejo PR. A fix round pushes to the **same** branch — never a second PR for the same
work. Push branches to `origin` (Forgejo) only, never to the GitHub mirror.
