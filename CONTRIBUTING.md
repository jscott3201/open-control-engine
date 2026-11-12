# Contributing to the Open Control Engine

Thanks for helping build an embeddable control engine for CDL. This guide is the human onramp;
the architecture and invariants are the design of record.

## Where the project lives

- **GitHub is the host:** `github.com/jscott3201/open-control-engine`. Clone from, push
  branches to, open pull requests and file issues there. CI runs on GitHub Actions
  (`.github/workflows/`): Linux runners plus the macOS and Windows observation cells.
- **PR numbers overlap.** Pull requests merged on the previous host before the October 2026 move
  to GitHub share numbers with GitHub PRs (#332 names two different changes). Bare `(#N)`
  citations predate the move; cite every PR merged on GitHub as `(GitHub #N)` in `CHANGELOG.md`
  and the docs.

## How changes land

- Every logical change opens a pull request into the **`development`** branch on GitHub.
- Development PRs run the CI gates in
  [`.github/workflows/pr-gate.yml`](.github/workflows/pr-gate.yml), summarized by the single
  required status `CI OK`;
  `bash .agents/gate.sh` reproduces them locally in CI's exact command form, so the list lives
  in one place rather than being restated here. Only `oce-api`, `oce-blocks`, and `oce-expr` tests
  form the state-determinism subset; the scoped `oce-conformance` strict-bit subset and four
  per-block suites also run per-PR ([bounded evidence](docs/strict-bit-evidence.md)). The remainder
  needs the release/full gate. Releases
  batch `development` → `main`. **Publishing is manual:** in `.github/workflows/release.yml`
  (crates.io credentials live in its `release` environment), a `v*` tag push runs its verify job
  only (including a workspace
  `cargo test`); the publish job is guarded by `github.event_name == 'workflow_dispatch'`, so a tag
  alone never publishes. The 12-publishable/five-private selection and supported `oce-api` feature matrix
  are checked by the package validator ([CI and the gate](docs/ci-and-the-gate.md#publishing)). No crate is
  published yet; actual publication remains separately owner-authorized.
- **Every PR runs the full per-PR gate**, including work-in-progress PRs. Confirm the checks
  actually ran and that `CI OK` is green before asking for review.
- **The merge gate runs on the merge commit, and the release gate nightly.** Merging into
  `development` starts [`.github/workflows/merge-gate.yml`](.github/workflows/merge-gate.yml): the
  Windows cell, the one job no pull request runs. Every night
  [`.github/workflows/release-gate.yml`](.github/workflows/release-gate.yml) runs the heavy set on
  the `development` tip: the full workspace suite in debug and release codegen, the doctests, the
  executed gate script and cargo-deny with advisories. The per-PR gate runs none of those. A red
  merge-gate run is fixed forward or reverted before the next determinism-lane merge, and a
  determinism-lane merge also needs the latest nightly not red, or its failure already being
  fixed forward
  ([what runs after the merge](docs/ci-and-the-gate.md#what-runs-after-the-merge)).
- Keep changes scoped to the crate or subsystem that owns the behavior, and add or update tests
  when you change behavior.

## Release checklist

Promotions into `main` are infrequent enough that every step gets forgotten by someone. In
order (a checklist by convention, not a CI gate):

1. **Bring `CHANGELOG.md` current first.** Check by comparison, not by whether the file was
   touched: list the PRs merged into `development` since the last release — every squash-merge
   subject ends in `(#N)`, so `git log --oneline origin/main..origin/development` lists them, as
   does GitHub's closed-PR view filtered to base `development` — and confirm each number appears
   in `CHANGELOG.md`, cited as `(GitHub #N)`
   (see [Where the project lives](#where-the-project-lives)).
   Any that do not are the entries to recover before opening the PR.

   This step used to be `git log origin/main..development -- CHANGELOG.md`, read as "empty means
   undocumented". The converse does not follow and the converse is how it was used: run against a
   ten-PR gap it returned three commits and passed, because three earlier PRs had each added an
   entry. It answers whether the file was touched in a range, never whether it is current.
2. Open the promotion PR `development` → `main` on GitHub.
3. Merge with a **merge commit**, never squash — both prior promotions (`a57d860`, `cf70c80`)
   are true merges, and squashing would rewrite the history `development` continues from.
4. The release gate and manual publishing then apply as described under
   [How changes land](#how-changes-land) — the full-workspace release gate runs on the
   promotion PR, and publishing stays manual (`workflow_dispatch`; a tag alone never
   publishes).

## Local setup

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml) (Rust 1.97.1, edition
2024); `rustup` installs it automatically on first build.

`.agents/gate.sh` also needs three cargo subcommands that do not ship with rustup. Without them
each step fails with "no such command" on a fresh clone:

```bash
cargo install cargo-nextest --locked --version 0.9.143   # pinned; see TESTING.md
cargo install cargo-machete --locked
cargo install cargo-deny --locked
cargo nextest show-config version                         # must report required/recommended: ok
```

The installed version matches CI; [`.config/nextest.toml`](.config/nextest.toml) rejects older
versions while allowing a newer compatible local runner.
Only `--target wasm32-wasip1` runs (the blob round trip's wasm32 leg, `bash .agents/gate.sh roundtrip` and `full`) need `rustup target add wasm32-wasip1` and wasmtime on PATH or where `bash scripts/ci/install-wasmtime.sh` puts it (a user cache directory outside `target/`, so `cargo clean` leaves it alone; the script prints the path); without wasmtime the cargo runner exits with that install hint.
The command above builds nextest from source; CI uses its pre-built binary through
`taiki-e/install-action`. On macOS, if even trivial nextest cases take more than about 0.2 seconds,
follow nextest's [XProtect guidance](https://nexte.st/docs/installation/macos/): enable your terminal
under Developer Tools, restart it, and run `cargo clean` once.

Install the git hooks once after cloning:

```bash
bash scripts/install-hooks.sh
```

This points `core.hooksPath` at the tracked [`.githooks/`](.githooks) directory, so the team
shares the same gates:

- **pre-commit:** `cargo fmt --all --check` + file-size cap + no-secret scan.
- **pre-push:** `cargo clippy --workspace --locked -- -D warnings` + the default-no-db gate.

Escape hatches: `git commit/push --no-verify` (once) or `export OCE_SKIP_HOOKS=1` (whole shell
session). Hook skipping follows the repository truthiness policy: empty, `0`, and `false`
(case-insensitive, with surrounding ASCII whitespace ignored) do not skip; every other value does.

## Before you open a PR

For indexed claims, follow the [source-owner update procedure](docs/authority-claims.md#updating-a-legitimate-claim).
The generated summary is not a replacement policy; native and delegated verifiers remain separate
from its fast schema/projection check. Regeneration is always explicit, never part of the gate.

Run the local proof that matches the lane your PR runs, first-hand, on the final head, and paste
the tail of its output into the PR:

| The PR's changed paths | Local proof |
| --- | --- |
| docs only | `bash .agents/gate.sh docs` |
| `crates/leverage/**`, docs, the root `README.md` | `bash .agents/gate.sh leverage` |
| anything else (the engine lane), the core four, `oce-cxf` and `oce-api` included | `bash .agents/gate.sh` |
| a pin advance (`third_party/**`, `tools/reference-catalog/**`, or either of the two pin-guard files `scripts/ci/proof-row.sh` lists), `.agents/gate.sh` itself, `.config/nextest.toml` or `scripts/ci/install-wasmtime.sh` | `bash .agents/gate.sh full` |

`git diff --no-renames --name-only origin/development... | bash scripts/ci/proof-row.sh` names
your row, and every gate run ends by printing the row your checkout needs beside the row it
covered. Each row runs everything the rows above it run, in CI's exact command form; what each
mode runs, the rules for a rebase and a fix round, and measured timings are in
[the local proof](docs/ci-and-the-gate.md#the-local-proof). No row asks for a release-codegen
run: the per-PR gate runs the determinism subset under release codegen itself, on x86_64 and
arm64, and compares it with the debug cells (`bash .agents/gate.sh determinism` reproduces a red
release cell on your machine). The last row adds that, the workspace suite and the doctests, for
the two kinds of change nothing lighter checks. If
you change `.github/workflows/release-gate.yml`, or anything else only `full` or the nightly run
executes, also run `gh workflow run release-gate.yml --ref <your branch>` and wait for it to
finish green before the PR merges; the `CI OK` that decides the merge is still the one from the
`pr-gate` run on your head commit.

No row runs every crate's tests. If your change falls outside the suites the per-PR gate names, a
green PR need not have run its tests, so run the suites of the crates you changed as well:

```bash
cargo nextest run -p <crate> --locked --profile ci --no-tests=fail
cargo test -p <crate> --doc --locked
```

CI also runs the script directly, as `gate (full)` in `.github/workflows/release-gate.yml`: nightly
on the `development` tip, on release PRs and on dispatch. No per-PR job executes the script, so a
change to it, a CI script or a workflow is first executed by CI on the night after it merges, and
before that only by your own run. That is coverage, not proof that the script and the workflows
still agree: nothing verifies that mechanically; change a command in CI first, then here. Which
`pr-gate.yml` jobs a PR runs depends on the paths it changes; see
[the lanes](docs/ci-and-the-gate.md#lanes-what-a-pr-runs).

[`.agents/gate.sh`](.agents/gate.sh) is the single source of truth for these commands, and it
prints what it cannot cover locally. Earlier revisions of this file listed the commands inline
and drifted from CI — omitting `cargo machete`, the gate-fixture job, the `--bins` rustdoc pass,
and the determinism matrix — while claiming to mirror it. Change a command in
[`.github/workflows/pr-gate.yml`](.github/workflows/pr-gate.yml) first, then in the script.

The dormant, disabled `.github/workflows/ci.yml` stays byte-frozen as the history of the retained
strict-bit capture archives. Native strict-bit evidence is re-established on every PR by the native
cells in `pr-gate.yml` ([strict-bit evidence](docs/strict-bit-evidence.md)), so `.agents/gate.sh`
and `.config/nextest.toml` are ordinary files again: change them like any other gate change,
`pr-gate.yml` first.

Qualification is Linux-only. The macOS arm64 cell (per PR) and the Windows cell (in the merge
gate, after each merge) are observations of those platforms, not qualification evidence
([platform cells](docs/ci-and-the-gate.md#platform-cells-macos-and-windows)); neither runs a
strict-bit suite.

## Invariants a change must not violate

- **The library is database-free.** It ships **no first-party database** (D-OWNER-1): the
  execution core (Group A) and the store ports (`oce-store`, including its in-memory `MemStore`)
  name no database type. Durable/queryable backends are app-side adapters behind the `oce-store` port.
- **The build is database-free and async-runtime-free.** `cargo tree -e normal` must list no
  `selene-db`, no `tokio`, no `async-std`. The default-no-db gate enforces this.
- **No `unsafe` code** (`#![forbid(unsafe_code)]` in every crate); public APIs require doc
  comments (the workspace denies missing docs); files stay under the 700-LOC cap.
- **Keep the tick deterministic.** Identical inputs and parameters must yield identical outputs.
  The evaluator performs no hashing, I/O, or store access — keep it that way. Allocation is
  **not** unconditionally zero across the block library, so do not add to the exceptions:
  `Reals.Sort` is stack-backed through `SORT_STACK_WIDTH` (64) inputs and falls back to two
  heap `Vec`s only above that (`reals_matrix.rs:388`, `:399-403`). Complete-frame preparation and
  result capture have explicit structural allocation budgets; the engine has no storage port to call.

  A new block allocation on the evaluator thread **is** caught per-PR.
  `crates/oce-blocks/tests/tick_allocation_census.rs` sweeps the whole registry via `catalog()` and
  carries a permanent positive control (`CDL.Reals.Sort`), and `oce-blocks` is one of the three
  crates the per-PR gate runs (`.agents/gate.sh`). Current blocks do not delegate work to worker threads; a
  block that introduces worker execution also needs an allocation guard for that work. The
  facade guards in `oce-api/tests/frame_purity.rs` and `frame_observations.rs` check engine purity,
  storage isolation and frame allocation budgets per-PR as part of the `oce-api` subset.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/) with a scope:

```
feat(graph): ...
fix(cxf): ...
docs: ...
ci: ...
chore: ...
```

Disclose AI assistance transparently. When an AI assistant materially helped with a change, add a
trailer naming the assisting model, for example:

```
Co-Authored-By: <Model Name> <noreply@anthropic.com>
```

## License

By contributing, you agree your contributions are dual-licensed under
[Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at the user's option, unless stated otherwise.
