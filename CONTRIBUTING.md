# Contributing to the Open Control Engine

Thanks for helping build an embeddable control engine for CDL. This guide is the human onramp;
the architecture and invariants are the design of record.

## How changes land

- Every logical change opens a pull request into the **`development`** branch.
- Development PRs run the CI gates in [`.github/workflows/ci.yml`](.github/workflows/ci.yml);
  `bash .agents/gate.sh` reproduces them locally in CI's exact command form, so the list lives
  in one place rather than being restated here. Only `oce-api`, `oce-blocks`, and `oce-expr` tests
  run per-PR — every other crate's tests run on the `development` → `main` release gate. Releases
  batch `development` → `main`. **Publishing is manual:** a `v*` tag push runs the verify job
  only; the publish job is guarded by `github.event_name == 'workflow_dispatch'`, so a tag alone
  never publishes. The 12-publishable/five-private selection and supported `oce-api` feature matrix
  are governed by the [package publication policy](docs/package-publication-policy.md). No crate is
  published yet; actual publication remains separately owner-authorized.
- **Open your PR non-draft.** Every `ci.yml` job is conditioned on
  `github.event.pull_request.draft == false`, so a draft PR runs no gates at all.
- Keep changes scoped to the crate or subsystem that owns the behavior, and add or update tests
  when you change behavior.

## Release checklist

Promotions into `main` are infrequent enough that every step gets forgotten by someone. In
order (a checklist by convention, not a CI gate):

1. **Bring `CHANGELOG.md` current first.** Check by comparison, not by whether the file was
   touched: list the PRs merged into `development` since the last release with
   `gh pr list --state merged --base development`, and confirm each number appears in
   `CHANGELOG.md`. Any that do not are the entries to recover before opening the PR.

   This step used to be `git log origin/main..development -- CHANGELOG.md`, read as "empty means
   undocumented". The converse does not follow and the converse is how it was used: run against a
   ten-PR gap it returned three commits and passed, because three earlier PRs had each added an
   entry. It answers whether the file was touched in a range, never whether it is current.
2. Open the promotion PR `development` → `main`.
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
codegen.

CI also runs this script directly, as the `gate (light)` job in `ci.yml` and `gate (full)` in
`release-gate.yml`. So the commands here gate your PR whether or not each is separately wired as
its own job — but that is coverage, not proof that the script and the workflows still agree.
Nothing verifies that mechanically; change a command in CI first, then here.

If your change touches a crate outside that three-crate subset, its tests did not run. Add `full`:

```bash
bash .agents/gate.sh full
```

[`.agents/gate.sh`](.agents/gate.sh) is the single source of truth for these commands, and it
prints what it cannot cover locally. Earlier revisions of this file listed the commands inline
and drifted from CI — omitting `cargo machete`, the gate-fixture job, the `--bins` rustdoc pass,
and the determinism matrix — while claiming to mirror it. Change a command in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml) first, then in the script.

## Invariants a change must not violate

- **The library is database-free.** It ships **no first-party database** (D-OWNER-1): the
  execution core (Group A) and the store ports (`oce-store`, `oce-store-mem`) name no database
  type. Durable/queryable backends are app-side adapters behind the `oce-store` port.
- **The build is database-free and async-runtime-free.** `cargo tree -e normal` must list no
  `selene-db`, no `tokio`, no `async-std`. The default-no-db gate enforces this.
- **No `unsafe` code** (`#![forbid(unsafe_code)]` in every crate); public APIs require doc
  comments (the workspace denies missing docs); files stay under the 700-LOC cap.
- **Keep the tick deterministic.** Identical inputs and parameters must yield identical outputs.
  The evaluator performs no hashing, I/O, or store access — keep it that way. Allocation is
  **not** unconditionally zero across the block library, so do not add to the exceptions:
  `Reals.Sort` is stack-backed through `SORT_STACK_WIDTH` (64) inputs and falls back to two
  heap `Vec`s only above that (`reals_matrix.rs:388`, `:399-403`), and `Engine::tick` takes one
  `store.snapshot()` when the model declares store-backed inputs.

  A new block allocation on the evaluator thread **is** caught per-PR.
  `crates/oce-blocks/tests/tick_allocation_census.rs` sweeps the whole registry via `catalog()` and
  carries a permanent positive control (`CDL.Reals.Sort`), and `oce-blocks` is one of the three
  crates the per-PR gate runs (`.agents/gate.sh`). Current blocks do not delegate work to worker threads; a
  block that introduces worker execution also needs an allocation guard for that work. The
  facade-level guard in `oce-api/tests/tick_purity_tests.rs` is narrower — three fixtures — and runs
  per-PR as part of the `oce-api` subset.

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
