# Leverage crates

This directory holds the crates that sit **above the facade**: authoring, verification, diffing,
hosting, bindings and packaging (`oce-author`, `oce-verify`, `oce-diff`, `oce-host`, `oce-bind`,
`oce-bacnet`, `oce-wasm`, `oce-py`, …) as the milestones add them. There are none yet.

Each one lives at `crates/leverage/<name>`, is named `oce-<name>`, and is a workspace member through
the `crates/leverage/*` glob in the root `Cargo.toml`.

## The rule

1. **A leverage crate depends on `oce-api` only — never on a core crate directly.** Among `oce-*`
   packages, its direct dependencies (normal, build and dev) may name `oce-api` and other leverage
   crates, nothing else. The engine's internals (`oce-model`, `oce-expr`, `oce-blocks`,
   `oce-graph`, `oce-cxf`, …) are reached through the facade or not at all, so a leverage crate
   can never come to depend on behaviour the facade does not promise.
   `bash scripts/ci/leverage-members.sh check` enforces it with `cargo tree` on every leverage-lane
   run.
2. **It ships the same goldens.** Tests compare checked-in expected outputs bit-exactly, as the
   engine crates do (see `TESTING.md`); a leverage crate is not a lighter testing tier.
3. **It runs in the leverage CI lane.** A pull request that touches `crates/leverage/**` and
   nothing that compiles into the engine runs the `leverage lane` job — fmt, clippy
   `-D warnings`, nextest and the dependency rule for each member — and skips the determinism lane.
   A change to the engine runs both lanes, because it can break a crate built on the facade.
   Adding a leverage crate or changing its dependencies changes `Cargo.lock`, which counts as an
   engine change and runs everything. `docs/ci-and-the-gate.md` ("What runs when") has the trigger
   table; `bash .agents/gate.sh leverage` runs the same lane locally.

This line is a CI lane probe (issue #393) and is never merged.
