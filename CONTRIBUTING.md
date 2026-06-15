# Contributing to the Open Control Engine

Thanks for helping build an embeddable control engine for CDL. This guide is the human onramp;
the architecture and invariants are the design of record.

## How changes land

- Every logical change opens a pull request into the **`development`** branch.
- Development PRs run the CI gates in [`.github/workflows/ci.yml`](.github/workflows/ci.yml):
  formatting, the file-size cap, the no-secret scan, `rustdoc -D warnings`,
  `clippy -D warnings`, a workspace build, the default-no-db gate, and — for dependency changes —
  `cargo-deny`. Releases batch `development` → `main`, and a version tag drives the gated publish.
- Keep changes scoped to the crate or subsystem that owns the behavior, and add or update tests
  when you change behavior.

## Local setup

The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml) (Rust 1.95.0, edition
2024); `rustup` installs it automatically on first build. Install the git hooks once after
cloning:

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
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --lib --document-private-items --locked
```

Run the fast repository gates:

```bash
bash .github/scripts/check-file-size.sh
bash .github/scripts/check-no-secrets.sh
bash .github/scripts/check-default-no-db.sh
```

Dependency or manifest changes (`Cargo.lock`, `Cargo.toml`, any `crates/*/Cargo.toml`,
`deny.toml`) also require:

```bash
cargo deny check bans licenses sources
```

These mirror [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

## Invariants a change must not violate

- **The library is database-free.** It ships **no first-party database** (D-OWNER-1): the
  execution core (Group A) and the store ports (`oce-store`, `oce-store-mem`) name no database
  type. Durable/queryable backends are app-side adapters behind the `oce-store` port.
- **The build is database-free and async-runtime-free.** `cargo tree -e normal` must list no
  `selene-db`, no `tokio`, no `async-std`. The default-no-db gate enforces this.
- **No `unsafe` code** (`#![forbid(unsafe_code)]` in every crate); public APIs require doc
  comments (the workspace denies missing docs); files stay under the 700-LOC cap.
- **Keep the tick deterministic.** The hot path performs no allocation, hashing, I/O, or store
  access; identical inputs and parameters must yield identical outputs.

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
