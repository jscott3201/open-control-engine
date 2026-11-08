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

Change a command by changing [`.github/workflows/pr-gate.yml`](../.github/workflows/pr-gate.yml)
first, then the script.

## Hosting and CI

GitHub is the primary host: `origin` is `github.com/jscott3201/open-control-engine`, and branches,
PRs, reviews, issues and CI live there. The self-hosted Forgejo instance that was primary until
October 2026 is retired. CI is GitHub Actions in `.github/workflows/` on GitHub-hosted Linux
runners (plus the two `pr-gate.yml` platform cells on macOS and Windows): `pr-gate.yml` (per-PR, light), `release-gate.yml` (release PRs into `main`, daily against
`development`, heavy), `advisories.yml` (daily), `docs-pages.yml` (docs validation and the Pages
publish from `main`), `release.yml` (tag verify / manual crates.io publish) and the two manual
native-arm64 OpenModelica evidence workflows. `ci.yml` there is byte-frozen and disabled in the
repository's Actions settings — it is not the CI.

The per-PR gate runs once per change, on the PR. A merge into `development` runs no gate:
`pr-gate.yml` has no `push` trigger, and `check-workflow-gates.sh` refuses one. Only the
path-filtered `docs-pages.yml` runs on that push. The daily `release-gate.yml` run on the
`development` tip is the safety net. See [CI and the gate](../docs/ci-and-the-gate.md#what-runs-when).

The per-PR gate has two lanes under the one `CI OK` (issue #393). Its first job classifies the
PR's changed paths with plain git and `scripts/ci/classify-changes.sh`, fail-closed: any path that
is not documentation or under `crates/leverage/` runs the determinism lane — every job that
compiles the engine — and so does any change to a gate script, workflow or tool config, which also
runs `gate (light)`. A `crates/leverage/**` change runs only the `leverage lane` job beside the
always-on checks (fmt, file-size, no-secret, docs, gate fixtures, the package/authority/product
contract); an engine change runs it too. A docs-only PR compiles nothing, and documentation a test
reads (`README.md`, `docs/cxf-composite-subset.md`) is not docs-only. `CI OK` accepts a skip only
when the job's lane output was `false`. Locally, `bash .agents/gate.sh leverage` mirrors a
leverage-only PR; see [lanes](../docs/ci-and-the-gate.md#lanes-what-a-pr-runs).

Two platform cells also run with the determinism lane (issue #394): macOS arm64 checks that the
portable state snapshot equals Linux byte for byte, and Windows builds the workspace and runs its
suite without the strict-bit binaries — observations, not qualification; see
[platform cells](../docs/ci-and-the-gate.md#platform-cells-macos-and-windows).

GitHub PR numbers collide with the retired Forgejo numbering: Forgejo #332 (the Forgejo CI port)
and GitHub #332 are different changes. Bare `(#N)` citations predate the move; cite every PR merged
on GitHub as `(GitHub #N)` in `CHANGELOG.md` and the docs.

Native strict-bit evidence is re-established on every PR: the strict-bit matrix in `pr-gate.yml`
recaptures the pinned corpus in four native Linux cells and compares them from scratch, so no
source file is bound to a stored receipt (the 35-file source-digest binding and its
`recapture-pending` status were retired with GitHub #392). `.github/workflows/ci.yml` stays
byte-frozen and disabled as the history of the retained capture archives. See
[`docs/strict-bit-evidence.md`](../docs/strict-bit-evidence.md).

## A green PR does not mean the tests passed

CI is dev-light and release-heavy, and the split is easy to misread in the
dangerous direction.

The per-PR gate into `development` runs fmt, clippy, build, rustdoc, the file-size
cap, the no-secret scan, the database-free check, the golden-gen firewall and
[hygiene job](#golden-gen-hygiene-is-ci-only-for-now), the
[docs and crate-suite jobs](#the-docs-and-crate-suite-jobs-are-ci-only-for-now), the closed
package/feature/publication contract and its hostile controls, the gate fixtures,
the [authority index/projection and its hostile controls](../docs/authority-claims.md),
`cargo machete` — and the state-determinism tests for **`oce-api`, `oce-blocks`, and `oce-expr`**, via the
determinism matrix: four native cells, x86_64 on `ubuntu-latest` and aarch64 on
`ubuntu-24.04-arm`, each in debug and release codegen.
Its fan-in job compares a populated portable engine-state snapshot byte-for-byte across
architectures, checks portable and target-bound bytes across debug/release codegen, and requires
target-bound bytes to differ across architectures. Each architecture then parses and refuses the
other's target-bound bytes through the public restore path, natively.
The standalone `cargo-deny` CI job runs `cargo deny check bans licenses sources` on every PR, and
`.agents/gate.sh` runs it too.
`advisories` is excluded from the script deliberately: it needs network and a writable
advisory-db, neither of which a sandboxed lane has, so it runs in `advisories.yml`.

The scoped `oce-conformance` strict-bit subset and four affected per-block suites also run per PR
in the same four native Linux architecture/codegen cells, with repeat captures and an eight-capture
cross-cell comparison ([bounded evidence](../docs/strict-bit-evidence.md)). The
`oce-graph`, `oce-validate`, `oce-model`, `oce-cxf` and `oce-store` suites run per
PR too, x86_64 debug only. The remainder — and the release-codegen runs of those five — needs the
release/full gate. A change confined to `oce-diag` or `oce-semantics` can show a fully
green PR having executed none of its own tests. Before claiming your tests pass, run
`bash .agents/gate.sh full` and read the tail.

Pin-advance PRs — any change under `third_party/**` or to the pin constants — run
`bash .agents/gate.sh full` first-hand; see the vendored README's
`## Pin-advance policy` section.

**Confirm the checks ran.** GitHub Actions runs the whole per-PR gate on every PR, drafts
included, and `CI OK` is the one status that summarizes it. A PR with no checks is easy to
mistake for a PR with no failing checks; confirm `CI OK` actually reported, not merely that
nothing is red.

## The core four build `no_std + alloc`

`oce-model`, `oce-expr`, `oce-blocks` and `oce-graph` have `default = ["std"]` and build
`#![no_std]` with `--no-default-features --features alloc` (issue #397). In those four, write
`core::`/`alloc::` paths and import `Vec`, `String`, `Box`, `format!` and `vec!` from `alloc`; call
the crate's `math` module (which re-exports `libm`) for any `f64` method that is not in `core`
(`floor`, `ceil`, `trunc`, `round`, `sqrt`, `powi`, `mul_add`, the transcendentals — the full table is
in `TESTING.md` "Conventions"), because clippy refuses the `std` method in each core crate's `src/` tree, unit tests included
(issue #399; `tests/` files are separate crates outside the lint, where `std` serves as the oracle); use no
hash collections; and give every new dependency `default-features = false`.
A crate that depends on one of the four names `features = ["std"]` on that edge, because their
workspace entries carry `default-features = false`. The `no-std-build` job in `pr-gate.yml` and
`.agents/gate.sh` build them for `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`; the script
installs nothing, so add both targets once with `rustup target add`. See
[the architecture](../docs/architecture.md#no_std--alloc-the-core-four).

The core four use no hash collections at all: use `BTreeMap`, `BTreeSet`, `BinaryHeap` or a sorted
`Vec` (issue #398). Two things enforce this. The `clippy.toml` `disallowed-types` block refuses
`std`'s hash types; it is allowed workspace-wide and denied in each core crate root, so the `std`
crates keep their lookup maps. `crates/oce-api/tests/core_crates_hash_free.rs` also refuses
`hashbrown`, `ahash`, `indexmap` and similar crates, and checks that the lint wiring is still in
place. Outside the core, `clippy::iter_over_hash_type` is denied workspace-wide, so a `for` loop
over a `HashMap`/`HashSet` needs an `#[expect]` with a reason. The order-leak test is
`crates/oce-api/tests/hash_order_independence.rs`: it runs the G36 corpus on 20 independently
seeded threads and byte-compares every output.

## golden-gen hygiene is CI-only for now

`tools/golden-gen` is its own cargo workspace, so the workspace fmt, clippy and test commands
never reach it. The `golden-gen-hygiene` job in `pr-gate.yml` covers it: fmt, clippy
`-D warnings`, its tests, and a release regeneration that must leave `tools/golden-gen/goldens`
byte-identical with no untracked files. `.agents/gate.sh` does not run these yet; wiring them in is
a separate change to the script. Until then, run them by hand after touching the generator or its
goldens:

```bash
cd tools/golden-gen
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo run --release --locked
git status --porcelain -- goldens    # must print nothing
```

## The docs and crate-suite jobs are CI-only for now

Two `pr-gate.yml` jobs that `CI OK` needs run commands `.agents/gate.sh` does not, like the
golden-gen hygiene job: wiring them into the script is a separate change.

- `crate-tests` runs the `oce-graph`, `oce-validate`, `oce-model`, `oce-cxf` and `oce-store`
  suites in x86_64 debug. It is a separate job, not a determinism-matrix
  cell, so it runs beside the cells instead of lengthening them.
- `docs` runs the documentation checks that `docs-pages.yml` also runs. That workflow is
  path-filtered and not required, so before this job a PR could merge green with it failing.
  `docs-pages.yml` still builds and publishes the site from `main`.

Run them by hand after touching those crates or the docs:

```bash
cargo nextest run -p oce-graph -p oce-validate -p oce-model -p oce-cxf -p oce-store \
  --locked --profile ci --no-tests=fail

bash scripts/docs/install-mdbook.sh target/mdbook-bin
python3 scripts/docs/test_check_links.py
python3 scripts/docs/test_authority_literals.py --mdbook target/mdbook-bin/mdbook
python3 scripts/docs/check_integrity.py --output target/docs-book --revision "$(git rev-parse HEAD)"
target/mdbook-bin/mdbook build target/docs-book
python3 scripts/docs/check_links.py target/docs-book/book --site-prefix /open-control-engine/
rm -rf target/docs-book
```

## Clippy lints the default feature set

`cargo clippy --workspace --all-targets --locked -- -D warnings`, matching CI.

Do not add `--all-features`. `oce-api` declares no features today, but the promise
this repo gates on is that the *default* build links no database and no async
runtime. Linting all features would check a configuration that never ships, and let a
default-build regression through once any crate grows an optional feature.

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

Base branch is `development`. Branch protection on GitHub blocks direct pushes to it, requires
a pull request, requires the `CI OK` status from `pr-gate.yml`, and requires the branch to be up to
date with `development` before merging; everything lands by squash-merge through a GitHub PR. A PR
that falls behind is updated and its gate re-runs, because that run is the only one the change
gets. A fix round pushes to the **same** branch — never a second PR for the same
work.
