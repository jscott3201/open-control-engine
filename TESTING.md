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

Worked examples — the bar each **upcoming** PR must clear (none of these have landed yet; they
are the targets this standard sets, not existing artifacts):
- **PR-5 (resolver):** golden `ModelGraph` lowered from `minimal_loop.jsonld` + a `decl_order`
  determinism golden + malformed-input edge cases each asserting their `DiagCode`.
- **PR-6 (engine loop):** golden converging trace for the feedback loop (`2.0 → … → 4.0`),
  bit-exact at every tick.
- **PR-9 (arrays):** round-trip goldens — preserved vs flattened forms compared bit-for-bit.

### 3. Oracle cross-checks — agreement with the reference implementation

CDL has a normative reference (the Modelica *Buildings* library / OpenModelica). Where a block or
expression has a reference result, **cross-check against it** rather than against our own
re-derived expectation — otherwise we are grading our own homework.

- Oracle vectors **will live** in the `oce-conformance` crate — its planned role as the home for
  reference traces and the CDL §7.7.2 expression-semantics vectors (R10.x). (Today that crate is
  an M0 scaffold: `compare()` is not yet implemented and it holds no vectors — this standard is
  what it gets built out to satisfy.)
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
| `ci.yml` (light) | PRs into `development` | **No** — fmt, clippy `-D warnings`, build, rustdoc, file-size, no-secret, default-no-db (+ cargo-deny on manifest change). |
| `release-gate.yml` (heavy) | PRs `development -> main` | **Yes** — the full suite, plus a re-run of the light gates against the release tip and an unconditional cargo-deny. |

**Runner: [`cargo-nextest`](https://nexte.st/)** (pinned `0.9.133`).

```bash
cargo nextest run --workspace            # unit + integration tests (the whole workspace)
cargo nextest run --profile ci           # reproduce the release gate's profile locally
cargo test --workspace --doc             # doctests — nextest CANNOT run these (separate step)
```

> nextest does **not** run doctests (a stable-Rust limitation). A complete local/CI test pass is
> therefore always **two commands**: `cargo nextest run` **and** `cargo test --doc`.

The git hooks (`pre-commit`, `pre-push`) deliberately **do not** run tests — they stay fast.
Run the suite on demand when you touch behavior; the release gate is the enforcement point.

Profiles live in [`.config/nextest.toml`](.config/nextest.toml): `default` for a fast local
fail-fast loop, `ci` for the gate (no fail-fast, no retries, slow-test guard). The release gate
passes `--no-tests=fail`, so a run that discovers **zero** tests hard-fails — catching the
regression where tests silently stop compiling or being discovered.
