# Testing standard — Open Control Engine

> **This engine controls real building equipment.** A wrong result is not a failed test — it is
> a physical hazard (a stuck valve, a frozen coil, an overpressured loop). Testing here is where
> the rubber meets the road. It is a **first-class deliverable on every PR**, never an
> afterthought, and **thin coverage is a blocking review defect** — the same severity as a
> compile error.

This document is the contract every change is held to. It is enforced socially in review and
mechanically in CI (the `development -> main` release gate runs the full suite — see
[CI: where tests run](#ci-where-tests-run)).

---

## The four pillars

Every behavioral change ships tests in **all four** categories that apply to it. "It compiles
and the happy path works" is not coverage.

### 1. Edge-case tests — adversarial, not illustrative

Do not test the value you expect to work. Test the value that *breaks* the implementation.
Enumerate the input domain and hit every boundary and degenerate case:

- **Numeric boundaries:** `0`, `-0.0`, `±1`, `i64::MIN`/`i64::MAX`, `i32` limits (CDL Integer is
  32-bit), values **past 2^53** (where `f64` silently loses integer precision — see the lesson
  below), subnormals, `f64::MIN_POSITIVE`.
- **Non-finite floats:** `NaN`, `+inf`, `-inf` — both as inputs and as results. Decide and
  *test* the intended behavior (propagate? reject with a `DomainError`? saturate?).
- **Sign and rounding:** truncation-toward-zero vs floor, divisor-signed `mod` vs dividend-signed
  `rem`, `sign(0) == 0`, division by zero, `sqrt` of a negative.
- **Empty / degenerate structure:** empty arrays, single-element arrays, zero connections,
  in-degree 0 vs 1 vs >1, self-loops, duplicate ids, missing required fields.
- **Malformed input must produce a typed diagnostic, never a panic.** Assert the *specific*
  `DiagCode` / error variant, not merely "an error occurred." Parsers and the resolver are
  total functions over arbitrary bytes: fuzz-grade hostility is the expectation.

> **Why this is non-negotiable — the C1 lesson.** In M1-PR-1 the expression evaluator passed 22
> tests and all CI gates while silently corrupting integer comparisons above 2^53 (relational
> ops routed `i64` through `f64`). No happy-path test caught it; an adversarial reviewer found it
> by reasoning about the *type domain*. The fix was to compare `i64` exactly — and to add a test
> at the boundary. **Green CI is not evidence of correctness when the test that would fail was
> never written.** Edge-case tests are how we write that test on purpose.

### 2. Golden tests — checked-in expected outputs, compared bit-exactly

The engine's core contract is **bit-identical determinism** (CDL §7.16). A golden test turns that
contract into an executable tripwire: a checked-in fixture, a checked-in expected output, and a
comparison that fails if even one bit differs.

- **Input fixtures** live beside the crate (e.g. `crates/oce-cxf/tests/fixtures/*.jsonld`).
- **Expected outputs** are checked in next to them (e.g. a serialized `ModelGraph`, a trace, a
  diagnostics list). They are reviewed artifacts — a diff to a golden file is a deliberate,
  scrutinized change, not noise.
- **Compare floats by bits, never with `==` or an epsilon.** Use `Value::bit_eq` (which compares
  `f64` via `to_bits`, so `NaN == NaN` and `+0.0 != -0.0` as determinism demands). An epsilon
  comparison would mask exactly the drift a golden test exists to catch.
- **No snapshot magic.** Goldens are explicit files compared by explicit code — reviewable and
  obvious. If a golden needs regenerating, do it deliberately and explain the diff in the PR.

Worked examples of the bar, and where each stands:
- **Resolver** — golden `ModelGraph` lowered from `minimal_loop.jsonld`, plus malformed-input
  edge cases each asserting their `DiagCode`. **Landed**:
  `crates/oce-cxf/tests/fixtures/golden/minimal_loop.modelgraph.txt`.
- **Engine loop** — golden converging trace for the feedback loop, bit-exact at every tick.
  **Landed**: `crates/oce-conformance/tests/fixtures/golden/trace.combi.csv` and the G36 traces
  beside it, each with a `.prov.json` recording the oracle's provenance.
- **Arrays** — round-trip goldens comparing preserved and flattened forms bit-for-bit.
  **Landed**: `crates/oce-cxf/tests/fixtures/golden/array.modelgraph.txt`,
  `array2d.modelgraph.txt` and `array_expression.modelgraph.txt`.

### 3. Oracle cross-checks — agreement with the reference implementation

CDL has a normative reference (the Modelica *Buildings* library / OpenModelica). Where a block or
expression has a reference result, **cross-check against it** rather than against our own
re-derived expectation — otherwise we are grading our own homework.

- Oracle vectors live in the `oce-conformance` crate — the home for reference traces and the CDL
  §7.7.2 expression-semantics vectors (R10.x). `compare()` is implemented
  (`crates/oce-conformance/src/funnel.rs`), and the crate carries golden traces and tolerance
  fixtures under `tests/fixtures/golden/`.
- Record the oracle's provenance (which tool, which version) alongside the vector so a future
  mismatch is debuggable.
- When no oracle exists for a construct, say so in the test and fall back to a hand-derived
  golden with the derivation documented.

### 4. Determinism goldens — same input, bit-identical output, every time

Determinism is a *testable property*, not an assumption. For anything that ingests, orders, or
executes:

- **Re-run and byte-compare.** Import/resolve the same fixture twice and assert the two
  `ModelGraph`s are byte-identical (PR-11 will do this across the conformance corpus). This catches
  `HashMap`-iteration-order leaks — ordering must derive from declaration/array order, never hash
  order.
- **Tick determinism.** Run the same model for N ticks twice; the traces must be bit-equal.
- **Diagnostic determinism.** Diagnostics are sorted deterministically (e.g. by `ConnectorId`);
  assert the order, not just the set.

---

## What "extensive" means per PR

A PR is **not done** until, for every unit of behavior it adds or changes:

1. Edge cases enumerated and tested (boundaries, non-finite, sign, empty, malformed→typed-error).
2. At least one golden where there is a meaningful output artifact (graph, trace, diagnostics).
3. An oracle cross-check where a reference result exists.
4. A determinism check where the code ingests, orders, or executes.
5. Every error path asserts a **specific** typed variant / `DiagCode`, and **no input causes a
   panic** (parsers/resolvers are total over arbitrary bytes).

Reviewers reject PRs that add behavior with only happy-path tests. "I couldn't think of an edge
case" is itself a finding to resolve, not a pass.

---

## Conventions

- **Test location:** inline `#[cfg(test)] mod tests;` for unit tests; `crates/<crate>/tests/` for
  integration tests; `crates/<crate>/tests/fixtures/` for input + golden files.
- **Float comparison:** `Value::bit_eq` (or `f64::to_bits`) — **never** `==` or `(a-b).abs() < ε`
  in an engine assertion.
- **Error assertions:** match the exact variant (`assert!(matches!(err, CxfError::Json(_)))`),
  not `is_err()`.
- **No time/randomness in tests:** deterministic inputs only; no wall-clock, no RNG.
- **Name tests for the property, not the function:** `mod_is_divisor_signed`,
  `resolve_is_byte_identical_across_two_imports`, `int_compare_is_exact_above_2_pow_53`.

---

## CI: where tests run

CI is **dev-light / release-heavy** (keep per-change PRs fast; save the heavy suite for releases):

| Gate | Trigger | Runs tests? |
| --- | --- | --- |
| `ci.yml` (light) | PRs into `development` | **`oce-blocks` and `oce-expr` only** — the `determinism-matrix` job runs those two crates on x86_64 and arm64, in debug and release codegen. No other crate's tests run. Alongside them: fmt, clippy `-D warnings`, build, rustdoc, file-size, no-secret, workspace-wide default-no-db, cargo-machete, stale crate-status header lint, golden-gen anti-tautology firewall, gate-fixture smoke, a `gate (light)` job that runs `.agents/gate.sh` itself (+ cargo-deny on manifest change). |
| `release-gate.yml` (heavy) | PRs `development -> main`, daily cron against `development`, manual dispatch | **Yes** — full nextest, release-codegen nextest, doctests, two armed per-crate public-api surface snapshots (`oce-api` and `oce-store`), plus a re-run of the light gates (including stale crate-status header lint) and an unconditional cargo-deny. |
| `advisories.yml` | Daily cron, manual dispatch | **No** — advisory/yanked scan only (`cargo deny check advisories`, `yanked = "deny"`, `ignore = []`). |

Read the first row in the dangerous direction and you will trust a green PR you
should not. A change confined to `oce-cxf`, `oce-store`, `oce-api`, or `oce-diag`
can show every check green having run none of its own tests. Before claiming tests
pass on such a change, run the suite yourself:

```bash
bash .agents/gate.sh full
```

**Runner: [`cargo-nextest`](https://nexte.st/)** (pinned `0.9.133`).

```bash
cargo nextest run --workspace            # unit + integration tests (the whole workspace)
cargo nextest run --profile ci           # reproduce the release gate's profile locally
cargo nextest run --profile ci --cargo-profile release  # release-codegen panic-freedom pass
cargo test --workspace --doc             # doctests — nextest CANNOT run these (separate step)
```

The public-api surface gates need the gate-only nightly and run in `release-gate.yml`. Run them
in **the release gate's own form** — the `NIGHTLY` value is pinned at `release-gate.yml`'s `env:`
block, so read it from there rather than copying the date:

```bash
OCE_PUBLIC_API_NIGHTLY=<release-gate.yml NIGHTLY> OCE_REQUIRE_SURFACE_CHECK=1 \
  cargo nextest run -p oce-api -E 'test(public_api_surface_matches_blessed_baseline)' \
  --profile public-api --locked --no-tests=fail
```

`--no-tests=fail` is what gives it teeth: under raw `cargo test`, a surface test that is renamed,
deleted, `#[ignore]`d or filtered out disappears silently and the run still passes green.
`OCE_REQUIRE_SURFACE_CHECK=1` likewise turns an unarmed skip into a failure. Swap `-p oce-api`
for `-p oce-store` for the other baseline — keep the package selectors in separate invocations,
which is what preserves vanish-to-RED for the re-exported port surface.

> nextest does **not** run doctests (a stable-Rust limitation). A complete local/CI test pass is
> therefore always **two commands**: `cargo nextest run` **and** `cargo test --doc`.

The git hooks (`pre-commit`, `pre-push`) deliberately **do not** run tests — they stay fast.
Run the suite on demand when you touch behavior; the release gate and daily development-tip gate are
the enforcement points.

Profiles live in [`.config/nextest.toml`](.config/nextest.toml): `default` for a fast local
fail-fast loop, `ci` for the gate (no fail-fast, no retries, slow-test guard). The release gate
passes `--no-tests=fail`, so a run that discovers **zero** tests hard-fails — catching the
regression where tests silently stop compiling or being discovered.

The armed public-API baselines are per crate. `oce-api` pins the embeddable facade, and `oce-store`
pins the re-exported storage port surface that the facade baseline sees only as an opaque
`pub use oce_api::oce_store` line. The release gate runs them as separate `cargo nextest` steps so a
renamed, ignored, or deleted test in either crate discovers zero tests and fails that crate's gate.
