# Contributing to the Open Control Engine

Thanks for helping build an embeddable control engine for CDL. This guide is the human onramp;
the architecture and invariants are the design of record.

## How changes land

- Every logical change opens a pull request into the **`development`** branch.
- Development PRs run the CI gates in [`.github/workflows/ci.yml`](.github/workflows/ci.yml);
  `bash .agents/gate.sh` reproduces them locally in CI's exact command form, so the list lives
  in one place rather than being restated here. Only `oce-blocks` and `oce-expr` tests run
  per-PR — every other crate's tests run on the `development` → `main` release gate. Releases
  batch `development` → `main`. **Publishing is manual:** a `v*` tag push runs the verify job
  only; the publish job is guarded by `github.event_name == 'workflow_dispatch'`, so a tag alone
  never publishes.
- **Open your PR non-draft.** Every `ci.yml` job is conditioned on
  `github.event.pull_request.draft == false`, so a draft PR runs no gates at all.
- Keep changes scoped to the crate or subsystem that owns the behavior, and add or update tests
  when you change behavior.

## Local setup

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml) (Rust 1.95.0, edition
2024); `rustup` installs it automatically on first build.

`.agents/gate.sh` also needs three cargo subcommands that do not ship with rustup. Without them
each step fails with "no such command" on a fresh clone:

```bash
cargo install cargo-nextest --locked --version 0.9.133   # pinned; see TESTING.md
cargo install cargo-machete --locked
cargo install cargo-deny --locked
```

Install the git hooks once after cloning:

```bash
bash scripts/install-hooks.sh
```

This points `core.hooksPath` at the tracked [`.githooks/`](.githooks) directory, so the team
shares the same gates:

- **pre-commit:** `cargo fmt --all --check` + file-size cap + no-secret scan.
- **pre-push:** `cargo clippy --workspace --locked -- -D warnings` + the default-no-db gate.

Escape hatches: `git commit/push --no-verify` (once) or `export OCE_SKIP_HOOKS=1` (whole shell
session).

## Before you open a PR

```bash
bash .agents/gate.sh
```

That runs the per-PR gate in CI's exact command form — formatting, the file-size cap, the
no-secret scan, the database-free and golden-gen invariant checks, the gate fixtures,
`cargo machete`, clippy, build, rustdoc, cargo-deny, and the `oce-blocks`/`oce-expr`
determinism subset in debug and release codegen.

CI also runs this script directly, as the `gate (light)` job in `ci.yml` and `gate (full)` in
`release-gate.yml`. So the commands here gate your PR whether or not each is separately wired as
its own job — but that is coverage, not proof that the script and the workflows still agree.
Nothing verifies that mechanically; change a command in CI first, then here.

If your change touches any other crate, its tests did not run. Add `full`:

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
  The graph evaluator's arithmetic path performs no allocation, hashing, I/O, or store access —
  keep it that way. Two existing carve-outs, neither of which should be widened: `Reals.Log` /
  `Reals.Log10` `format!` a diagnostic string on non-positive input, and `Engine::tick` takes one
  `store.snapshot()` per tick when the model declares store-backed inputs. Do not add a third.

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
