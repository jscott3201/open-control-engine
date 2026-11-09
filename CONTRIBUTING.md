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
Only `--target wasm32-wasip1` runs (the blob round trip's wasm32 leg, `bash .agents/gate.sh roundtrip` and `full`) need `rustup target add wasm32-wasip1` and wasmtime on PATH or in `target/wasmtime-bin` (`bash scripts/ci/install-wasmtime.sh`); without wasmtime the cargo runner exits with that install hint.
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

```bash
bash .agents/gate.sh
```

That runs the per-PR gate in CI's exact command form — formatting, the file-size cap, the no-secret
scan, the database-free and golden-gen invariant checks, the closed package/feature/publication
contract and its hostile controls, the gate fixtures, `cargo machete`, clippy, build, rustdoc,
cargo-deny, and the `oce-api`/`oce-blocks`/`oce-expr` determinism subset in debug and release
codegen, plus the scoped strict-bit conformance subset in both codegen profiles.

CI also runs this script directly: as the `gate (light)` job in `.github/workflows/pr-gate.yml` on
any PR that changes a gate script, workflow or tool config, and as `gate (full)` in
`.github/workflows/release-gate.yml` daily. So the commands here gate any PR that could make a
workflow job diverge from them — but that is coverage, not proof that the script and the workflows
still agree. Which `pr-gate.yml` jobs a PR runs depends on the paths it changes; see
[the lanes](docs/ci-and-the-gate.md#lanes-what-a-pr-runs). `bash .agents/gate.sh leverage` mirrors
a PR confined to `crates/leverage/`.
Nothing verifies that mechanically; change a command in CI first, then here.

If your change falls outside those named subsets, a green PR need not have run its tests. Add `full`:

```bash
bash .agents/gate.sh full
```

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

CI is Linux-only. There is no macOS or Windows CI leg, and there never was one; macOS observations
are local and are not qualification evidence.

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
