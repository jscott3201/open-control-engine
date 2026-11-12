# Testing standard — Open Control Engine

> **This engine controls real building equipment.** A wrong result is not a failed test — it is
> a physical hazard (a stuck valve, a frozen coil, an overpressured loop). Testing here is where
> the rubber meets the road. It is a **first-class deliverable on every PR**, never an
> afterthought, and **thin coverage is a blocking review defect** — the same severity as a
> compile error.

This document is the contract every change is held to. It is enforced socially in review and
mechanically in CI (the `development -> main` release gate runs the full suite — see
[CI: where tests run](#ci-where-tests-run)).

The [product contract](docs/product-contract.md) maps product requirements to bounded evidence and
future acceptance outcomes; its traceability checks do not replace behavioral tests or host qualification.

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
- **Malformed input must produce a typed diagnostic, never a panic.** Expression nesting and
  construction are bounded by `MAX_NESTING_DEPTH` / `MAX_EXPR_NODES`, returning the specific
  `NestingTooDeep` / `ExpressionTooLarge` variant. Composite lowering is bounded by
  `MAX_COMPOSITE_NESTING_DEPTH`, returning `MalformedDocument`. These limits intentionally
  narrow acceptance: expressions whose parse nesting exceeds 64 guarded entries (measured at 31
  nested parenthesis/brace/call levels or 62 unary signs), expressions deeper than 64 AST nodes or
  larger than 4096 AST nodes, composites deeper than 64, boundary paths beyond 64 non-top hops,
  and boundary walks beyond 65,536 target examinations or 8 MiB of aggregate target-IRI bytes are
  rejected even if a smaller input of the same shape is accepted. Hostile *amplification* is
  bounded the same way (issue #365): a `containsBlock` child reached twice is `SharedInstance`,
  array parameters beyond 1,048,576 elements per document and run state beyond
  `oce_graph::MAX_STATE_WORDS` are `ResourceLimitExceeded`, and an evaluation beyond
  `oce_expr::MAX_EVAL_FUEL` work units (or a document's bindings beyond
  `oce_cxf::MAX_DOCUMENT_EVAL_FUEL` together) is `ExpressionTooExpensive`. Pin each such limit
  at its boundary (the limit accepted, one past it refused) and assert cost with deterministic
  measures — allocation counts and peak bytes, fuel units — never wall-clock time. Assert the
  *specific* `DiagCode` / error variant, not merely "an error occurred." Fuzz-grade hostility is
  the expectation within the bounded ingest paths.

> **Why this is non-negotiable — the C1 lesson.** In M1-PR-1 the expression evaluator passed 22
> tests and all CI gates while silently corrupting integer comparisons above 2^53 (relational
> ops routed `i64` through `f64`). No happy-path test caught it; an adversarial reviewer found it
> by reasoning about the *type domain*. The fix was to compare `i64` exactly — and to add a test
> at the boundary. **Green CI is not evidence of correctness when the test that would fail was
> never written.** Edge-case tests are how we write that test on purpose.

### 2. Golden tests — checked-in expected outputs, compared bit-exactly

The engine's core contract is **bit-identical determinism** (CDL's model of computation, §7.16 of
the OpenBuildingControl (OBC) specification). A golden test turns that
contract into an executable tripwire: a checked-in fixture, a checked-in expected output, and a
comparison that fails if even one bit differs.

- **Input fixtures** live beside the crate (e.g. `crates/oce-cxf/tests/fixtures/*.jsonld`).
- **Expected outputs** are checked in next to them (e.g. a serialized `ModelGraph`, a trace, a
  diagnostics list). They are reviewed artifacts — a diff to a golden file is a deliberate,
  scrutinized change, not noise.
- **Compare floats by bits, never with `==` or an epsilon.** Use `Value::bit_eq` (which compares
  `f64` via `to_bits`, so `NaN == NaN` and `+0.0 != -0.0` as determinism demands). An epsilon
  comparison would mask exactly the drift a golden test exists to catch. One named exception: the
  21 transcendental, psychrometric, and solar Real signal goldens retain their existing
  aligned-tolerance band on unqualified platforms. Their [strict-bit inventory and matrix](docs/strict-bit-evidence.md)
  enforce Linux x86_64/aarch64 exact comparisons on every PR: four native debug/release cells,
  two byte-identical captures per cell, 21 signals each and zero mismatches against the Tier-A
  references and across cells, recomputed from the PR's own checkout through `oce_api::Engine`.
  macOS remains conservative/unqualified until M06-PR02; other targets also retain the unchanged
  1e-12 aligned band. This is empirical pinned corpus evidence,
  not a libm guarantee or a license for epsilon elsewhere — every other golden stays bit-exact.
  The retained native capture archives from runs 35492290613 and 37382761120 are reconstructed
  history, not a substitute for the per-PR matrix.
- **No snapshot magic.** Goldens are explicit files compared by explicit code — reviewable and
  obvious. If a golden needs regenerating, do it deliberately and explain the diff in the PR.

Worked examples of the bar, and where each stands:
- **Resolver** — golden `ModelGraph` lowered from `minimal_loop.jsonld`, plus malformed-input
  edge cases each asserting their `DiagCode`. **Landed**:
  `crates/oce-cxf/tests/fixtures/golden/minimal_loop.modelgraph.txt`.
- **Engine loop** — golden converging trace for the feedback loop, bit-exact at every tick.
  **Landed**: `crates/oce-conformance/tests/fixtures/golden/trace.combi.csv` and the 46 G36 traces
  beside it, each with a `.prov.json`. Read that sidecar before citing a trace as evidence of
  correctness: all 46 record `"tier": "2"` and `depends_on_oce_blocks: true` — engine self-output,
  a determinism snapshot and explicitly **not** a correctness oracle.
- **Arrays** — round-trip goldens comparing preserved and flattened forms bit-for-bit.
  **Landed**: `crates/oce-cxf/tests/fixtures/golden/array.modelgraph.txt`,
  `array2d.modelgraph.txt` and `array_expression.modelgraph.txt`.
- **Compiled-schedule blobs** — one `OCEB` blob per G36 catalog fixture, re-encoded and
  decode-then-re-encoded byte for byte, plus the per-class signature-hash table; the determinism
  matrix requires the bytes identical across architectures and across debug and release
  codegen, per PR. **Landed**:
  `crates/oce-api/tests/fixtures/blob/` (46 `.oceb` files and `class_signatures.tsv`; GitHub #400,
  [compiled-schedule blob](docs/compiled-schedule-blob.md)). Its hostile-input suites follow the
  bar above: one typed refusal per defect with mutations re-checksummed to reach the check under
  test, a seeded 10,000-case byte-mutation property test, a hostile-parameter fuzz over every
  registry constructor (`crates/oce-blocks/tests/hostile_parameters.rs`), and an exact decoder
  allocation count (`blob_allocations.rs`).
- **One frame driver** — the facade, the core frame driver (`oce_graph::frame`) and the blob
  entry (`oce_graph::blob::Executable`) run every G36 fixture's 160 frames and the same bad frames,
  compared bit for bit after every frame and refusal
  (`crates/oce-api/src/tests/blob_dual_path_tests.rs`, GitHub #401). The facade's run must
  reproduce one FNV-1a-64 digest per fixture, and `oce-graph`'s `blob_execution` test checks the
  `no_std + alloc` build of the entry against the same digests. **Landed**:
  `crates/oce-graph/tests/fixtures/blob_frame_digests.tsv` (46 lines). It moves whenever a
  fixture's outputs or state trajectory does; bless deliberately with
  `OCE_BLESS_FRAME_DIGESTS=1 cargo nextest run -p oce-api --lib -E 'test(/blob_dual_path/)'` and
  say which fixtures moved and why. The same `blob_execution` binary is the blob path's allocation
  census: zero allocations per frame, with a positive control.

### 3. Oracle cross-checks — agreement with the reference implementation

The Modelica *Buildings* library supplies reference semantics, and an independently executed
implementation can detect disagreements that an in-repository derivation misses. External output
is a detector, not a verdict: adjudication follows analytical evidence, then Dymola, then
OpenModelica. A mismatch never widens a tolerance or re-blesses a golden.

- Oracle vectors live in the `oce-conformance` crate — the home for reference traces and the CDL
  §7.7.2 expression-semantics vectors (R10.x). `compare()` is implemented
  (`crates/oce-conformance/src/funnel.rs`), and the crate carries golden traces and tolerance
  fixtures under `tests/fixtures/golden/`.
- **The Tier-A layer is `tools/golden-gen`.** It emits **422 Tier-A provenance records — 287 CDL
  records (285 signal goldens plus two fold-time provenance-only records), plus 135 G36 sequence
  signal goldens spanning all 46 catalog fixtures** (two of them in a 1/16 s FreezeProtection
  scenario that reuses its fixture). The
  sequence side is compared by per-fixture suites in `crates/oce-conformance/tests/`: a shared
  table in `g36_funnel_band/sequences.rs` (26 outputs), per-fixture `*_funnel.rs` suites, and four
  `*_oracle.rs` suites whose headers state outright that there is intentionally no funnel-band
  test. **Count from the suites, not from the shared table** — reading only `sequences.rs`
  understates coverage by a factor of five, which is a mistake that has been made twice.
  All 135 are compared **bit-exactly** (`compare_regime` on every golden: 104 `Value::bit_eq`
  f64, 19 exact encoded integer, 12 exact 0.0/1.0). The funnel band is an *additive* Real-only
  layer, never the primary comparison — Boolean and Integer outputs are deliberately kept off it
  because the funnel is type-blind (`g36_funnel_band/policy.rs`).
  Of the 420 signal goldens, 398 check CDL / Buildings source semantics. The other 22 are exact
  HostTick v1 profile references: 20 for `Generic.TimeSuppression`, `CoolingOnly.Controller`, and
  `ReliefFanGroup`, whose fixtures contain `CDL.Logical.Pre`, and the two standalone
  `CDL.Logical.Pre` goldens; they do not claim Modelica event-iteration equivalence. The generator is deliberately kept **off the workspace** and
  **forbidden from depending on `oce-blocks`**; CI enforces that
  code-dependency firewall. For classes whose oracle shares a pinned math kernel or restates the
  same documented recurrence, a Tier-A pass is evidence about plumbing and transcription of the
  shared formula, not an independent check of the formula; a mechanical shared-kernel detector
  is filed as follow-up. Extending Tier-A coverage means adding to that generator, never blessing
  engine output. Per-block Tier-A goldens cover **132 of 133 CDL classes**:
  only `CDL.Logical.Sources.Constant` carries indirect G36-sequence evidence alone.
  `CDL.Logical.Pre`'s standalone goldens pin the HostTick v1 profile, not Modelica event
  iteration. `CDL.Utilities.Assert` has no output port, so its golden is a `warning` observation
  column compared against each completed frame's Warning diagnostics
  (`per_block_utilities_assert.rs`), not an output point.
- Record the oracle's provenance (which tool, which version) alongside the vector so a future
  mismatch is debuggable.
- When no oracle exists for a construct, say so in the test and fall back to a hand-derived
  golden with the derivation documented.

Detected disagreements may be recorded in
`crates/oce-conformance/tests/fixtures/known_divergence/register.json` after human adjudication.
The register is evidence only: membership does not change discrepancies, comparison settings,
tier status, goldens, or test results. Its initial revision is empty because no current clean-room
Nand discrepancy reproduces. A private test reader validates the closed schema and local evidence
digests; the existing `oce-cxf` `fixture_structural_oracle` binary runs the bounded per-PR sentinel.
The separate OpenModelica evidence profiles execute four cases against OMC 1.25.1 and the pinned
Buildings and MSL sources: exhaustive `CDL.Logical.Nand/all_boolean_input_pairs_evented`, the
stateful `CDL.Logical.Toggle/repeated_rises_initial_true_and_clear_priority` schedule, and
`CDL.Reals.Line/four_limit_modes_five_dyadic_regions`, plus the composed G36
`Reliefs/source_default_dyadic_regions` leaf. The Line claim is one finite matrix whose operations
are exact in binary64. Reliefs covers seven complete five-input tuples and its two declared root
outputs at the emitted event timestamp bits. Neither is an arbitrary Real or tolerance result. The
light-gated sentinel validates committed raw output, keep-last projection, schedules, repeat-run
records, OCI identities, pinned host artifact-tool identities, committed/materialized source
records, cross-architecture canonical equality for Line and Reliefs, and mutation controls. Both
two-architecture cases retain keep-first output and metadata per architecture and reproduce them
independently from raw input; Docker does not run in normal CI. These are scoped Tier-3 results for
the four named cases only. The global Tier-3 report remains `Skipped`; no full-sequence, solver, or
cross-architecture raw-byte claim follows.

The Reliefs contract records the exact checkout observed while the native candidates ran. That SHA
does not need to remain an ancestor of the retained head or exist in local Git history. Both
validators bind the native records to the fixed observation, the complete generator-input digest
map, the current exact input bytes, and the retained contract artifact. Generation additionally
checks the observation against checkout `HEAD`. The manual workflow verifies each native artifact,
runs the documented two-architecture assembler in a separate job, and uploads the assembled
candidate. Final retained validation is still required before fixture admission. The Python
validator is POSIX-only. The Rust validator supplies the Windows path boundary and rejects reparse
points, hardlinks, identity changes, and oversized files.

`CDL.Logical.Pre` is not an expected-green OpenModelica case. Its upstream same-time event iteration
differs from the fixed HostTick v1 projection, which advances once per HostTick transition.
`crates/oce-api/src/tests/pre_execution_profile_tests.rs` pins that engine contract, including equal
timestamps, non-convergent feedback, host observations, and snapshot continuation. Those tests are
not an independent Modelica oracle.

### 4. Determinism goldens — same input, bit-identical output, every time

Determinism is a *testable property*, not an assumption. For anything that ingests, orders, or
executes:

- **Re-run and byte-compare.** Import/resolve the same fixture twice and assert the two
  `ModelGraph`s are byte-identical (PR-11 will do this across the conformance corpus). This catches
  `HashMap`-iteration-order leaks — ordering must derive from declaration/array order, never hash
  order.
- **Re-run under independent hasher seeds.** `crates/oce-api/tests/hash_order_independence.rs`
  loads all 46 G36 fixtures and the resolver-contract document on 20 freshly spawned threads, each
  seeding `RandomState` independently, then exports, runs four frames and snapshots. Export bytes,
  content id, load/export/refusal diagnostics, `input_definitions()`, point inventory, resolved
  model, schedule, topology, frame outputs and snapshot bytes must match across all 20. Its control
  runs a synthetic `HashMap`-order emitter through the same harness and requires a divergence. Swap a
  `BTreeMap`, a sort or the schedule's min-heap for hash iteration and this test names the output
  and fixture that moved. In the four `no_std` crates, clippy's `disallowed-types` refuses
  `std`'s hash types, and `core_crates_hash_free.rs` refuses every hash collection or hash crate
  and checks the lint wiring. `clippy::iter_over_hash_type` is denied workspace-wide.
- **Tick determinism.** Run the same model for N ticks twice; the traces must be bit-equal.
- **Blob round trip.** `crates/oce-conformance/tests/blob_round_trip.rs` (issue #403) loads each of
  the 46 G36 fixtures through the facade, encodes its compiled-schedule blob, decodes it with the
  `no_std` execution entry and runs the shared 160-frame schedule through both, comparing every
  boundary output by raw bits, every warning in order and the raw state words after load and
  after every frame. Per PR it runs on x86_64, aarch64 and `wasm32-wasip1` from one recorded
  artifact: every determinism cell records the blobs and expected results, the four cells'
  recordings must be identical (both architectures, debug and release codegen), and the x86_64 debug recording is decoded and replayed natively on aarch64
  and under wasmtime on `wasm32-wasip1`
  ([the blob round trip](docs/compiled-schedule-blob.md#the-blob-round-trip)). Its controls flip
  one bit of a recording and require the report to name the fixture, frame, slot and both bit
  patterns. Locally: `bash .agents/gate.sh roundtrip`.
- **Diagnostic determinism.** Diagnostics are sorted deterministically (e.g. by `ConnectorId`);
  assert the order, not just the set.

---

## What "extensive" means per PR

A PR is **not done** until, for every unit of behavior it adds or changes:

1. Edge cases enumerated and tested (boundaries, non-finite, sign, empty, malformed→typed-error).
2. At least one golden where there is a meaningful output artifact (graph, trace, diagnostics).
3. An oracle cross-check where a reference result exists.
4. A determinism check where the code ingests, orders, or executes.
5. Every error path asserts a **specific** typed variant / `DiagCode`. Expression parsing,
   evaluation, composite nesting, and boundary lowering reject over-limit depth, size, or work with
   typed outcomes.

Reviewers reject PRs that add behavior with only happy-path tests. "I couldn't think of an edge
case" is itself a finding to resolve, not a pass.

---

## Conventions

- **Test location:** inline `#[cfg(test)] mod tests;` for unit tests; `crates/<crate>/tests/` for
  integration tests; `crates/<crate>/tests/fixtures/` for input + golden files. `oce-author`
  builds `tests/*.rs` as one binary (`autotests = false`, GitHub #548): register a new test file
  there as `mod <file>;` in `tests/authoring.rs` (with `use crate::support;` for the shared
  helpers), or give it its own `[[test]]` in `Cargo.toml`, with the reason, when something selects
  it by name; a layout test fails on a file compiled into no binary or into two. `oce-cxf` builds
  them as `exchange` plus the eleven binaries something selects by name (GitHub #549): register a
  new test file as `mod <file>;` in `tests/exchange.rs` (with `use crate::bless;` or
  `use crate::render;` for the shared helpers), or as its own `[[test]]`, with the reason, when
  something selects it by name; its layout test also refuses a declaration it cannot place.
  `oce-api` does the same (GitHub #546): a new file under `crates/oce-api/tests/` is registered
  as a `mod` line in `tests/facade.rs`, or in `tests/facade_allocation_census.rs` if it names
  `allocation_counter`. Select one file's tests with
  `cargo nextest run -p oce-api --test facade -E 'test(/^<file>::/)'`. The layout test in
  `facade.rs` (`every_test_file_is_compiled_into_exactly_one_binary`) fails on a file that is in
  no binary or in two.
  `oce-conformance` builds them as `conformance` plus the seven binaries something selects by name,
  the strict-bit evidence tests among them (GitHub #550): register a new test file as
  `mod <file>;` in `tests/conformance.rs` (with `use crate::<helper>;` for a helper that root
  declares, such as `block_harness`), or as its own `[[test]]`, with the reason, when something
  selects it by name.
- **Float comparison:** `Value::bit_eq` (or `f64::to_bits`) — **never** `==` or `(a-b).abs() < ε`
  in an engine assertion. Sole exception: the 21 transcendental, psychrometric, and solar Real
  signal goldens, whose libm-dependent outputs retain the existing aligned-tolerance band on
  unqualified platforms and have [retained native Linux exact evidence](docs/strict-bit-evidence.md).
  The machine-readable inventory owns the paths; its capture-time candidate labels are historical,
  while the accepted receipt establishes the bounded Linux regime. Tests bind raw artifacts,
  exactly the selected source digests and oracle/CXF digests, not the full compiled dependency
  closure or the final HEAD to the captured synthetic merge SHA.
- **Error assertions:** match the exact variant (`assert!(matches!(err, CxfError::Json(_)))`),
  not `is_err()`.
- **No time/randomness in tests:** deterministic inputs only; no wall-clock, no RNG.
- **Name tests for the property, not the function:** `mod_is_divisor_signed`,
  `resolve_is_byte_identical_across_two_imports`, `int_compare_is_exact_above_2_pow_53`.
- **Math discipline (issue #399):** in the `no_std` core four, call an `f64` method only if `core`
  has it; every other operation goes through the crate's `math` module (`oce-blocks` and `oce-expr`
  each have one), which re-exports the pinned `libm` the strict-bit evidence is bound to. The root
  `clippy.toml` lists every `std`-only `f64`/`f32` method under `disallowed-methods`, each core
  crate root denies the lint (the rest of the workspace and `tools/golden-gen` allow it), so it binds
  each core crate's `src/` tree, unit tests included. Files under `tests/` are separate crates
  outside it, deliberately: that is where the `std` methods serve as the oracle
  (`libm_std_parity.rs`, which declares `#![allow(clippy::disallowed_methods)]`; a core `tests/`
  file that names `libm::` without that declaration is refused). A unit test that must call the
  `std` method allows the lint on that item with a comment, as `math_tests.rs` does.
  `scripts/ci/test-math-lint.sh` re-probes the table below on the pinned toolchain, checks that
  scope, and proves an `x.sin()` in oce-blocks and an `f.sqrt()` in oce-expr each fail clippy.

  | Source on 1.97.1 (`no_std` probe, `thumbv7em-none-eabihf`) | `f64` methods |
  | --- | --- |
  | `core`: call the method | `abs`, `signum`, `copysign`, `min`, `max`, `clamp`, `recip`, `to_degrees`, `to_radians`, `total_cmp`, `midpoint` (and the `is_*`/bit methods) |
  | `std` only: call `math` | `floor`, `ceil`, `round`, `round_ties_even`, `trunc`, `fract`, `mul_add`, `div_euclid`, `rem_euclid`, `powi`, `powf`, `sqrt`, `cbrt`, `hypot`, `exp`, `exp2`, `exp_m1`, `ln`, `ln_1p`, `log`, `log2`, `log10`, `sin`, `cos`, `tan`, `sin_cos`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `asinh`, `acosh`, `atanh`, `abs_sub` |

---

## CI: where tests run

CI is **dev-light / release-heavy** (keep per-change PRs fast; save the heavy suite for releases):

| Gate | Trigger | Runs tests? |
| --- | --- | --- |
| `.github/workflows/pr-gate.yml` (light) | PRs into `development`, manual dispatch (it has no post-merge run of its own; the merge starts the merge gate below) | On an engine change, the `determinism-matrix` job runs **`oce-api`, `oce-blocks`, and `oce-expr`** in four native cells — x86_64 on `ubuntu-latest`, aarch64 on `ubuntu-24.04-arm`, each in debug and release codegen — and its `determinism-compare` fan-in checks that portable snapshots match across architectures, target-bound snapshots differ across architectures, both match across debug and release codegen, each architecture refuses the other's target-bound snapshot on restore, and the 46 compiled-schedule blobs and the round-trip recordings are byte-identical in all four cells. (The release-codegen cells and the debug-against-release comparison ran only after the merge from issue #502 until issue #556; `bash .agents/gate.sh determinism` reproduces the subset under release codegen locally, and no PR owes it.) Separately, scoped **`oce-conformance::strict_bits` controls and the four aligned suite binaries** run in four native cells (both architectures, debug and release), with repeat captures and a fail-closed eight-capture comparison in the `strict-bit-compare` fan-in; that per-PR matrix is the native strict-bit evidence. This is not the full conformance suite. The `crate-tests` job runs the `oce-graph`, `oce-validate`, `oce-model`, `oce-cxf` and `oce-store` suites in x86_64 debug only (their release-codegen runs stay in the heavy gate). One [platform cell](docs/ci-and-the-gate.md#platform-cells-macos-and-windows) observes another operating system in debug codegen: macOS arm64 (`macos-15`) runs the three-crate subset and its fan-in requires the portable snapshot to equal Linux's byte for byte and the target-bound snapshot to differ; it does not qualify its platform or observe a strict-bit result. (The Windows cell is the merge gate's.) The bare-metal cell (`no-std-build`) builds and lints `oce-model`, `oce-expr`, `oce-blocks` and `oce-graph` `no_std + alloc` on `thumbv7em-none-eabihf`, `riscv32imac-unknown-none-elf` and `wasm32-unknown-unknown` and fails on any `std` feature or host-only crate in their dependency graphs (with controls that must trip), checks the `no_std` `oce-graph` closure holds no `oce-cxf`, `oce-api`, `serde_json` or `std`, runs `oce-graph`'s `blob_execution` (allocation census and run digests) and `blob_state_words` suites against the `no_std + alloc` build, runs the `oce-expr` unit suite without the `std` feature, builds the `oce-firmware-link` image for both bare-metal targets within its size budget, and runs that crate's host tests (every golden's run digest through the image's code) ([the bare-metal cell](docs/ci-and-the-gate.md#the-bare-metal-cell)); it compiles the rest of the four's tests nowhere, so their other suites still run only in the default `std` build, and it executes the image nowhere: the nightly `qemu.yml` does. Alongside them: clippy `-D warnings`, build, rustdoc, workspace-wide default-no-db, cargo-machete, cargo-deny bans/licenses/sources and the golden-gen firewall. All of that is the **determinism lane**, which runs only when the PR changes something outside the documentation, the root `README.md` and `crates/leverage/` (fail-closed path rules, [lanes](docs/ci-and-the-gate.md#lanes-what-a-pr-runs)). The **README lane** — the `facade contract and Quickstart` job: the facade compiler contract, the `readme_quickstart` byte-pin of the README's Quickstart and the executed example — runs when the root `README.md` changes and for every engine change. The **leverage lane** — fmt, clippy, rustdoc, nextest and the `oce-api`-only dependency rule for each `crates/leverage/*` crate — runs for a leverage change and for every engine change. Every PR runs fmt, file-size, no-secret, the docs build, the gate fixtures and the package/feature/publication, authority and product-contract checks. No per-PR job executes `.agents/gate.sh`; a gate script, workflow or tool-config change runs the determinism lane like any engine change. A docs-only or leverage-only PR therefore runs **no engine test at all**, and a README-only PR runs only the Quickstart byte-pin and the Quickstart itself. |
| `.github/workflows/merge-gate.yml` (after each merge) | **Every push to `development`** (each merged PR, on its merge commit; not a required status, and its aggregate is `Merge gate OK`, not `CI OK`), manual dispatch | **The Windows suite only** (issues #502, #545 and #556; the determinism matrix this workflow ran until issue #556 is per-PR again) — the Windows cell (`windows-latest`: the workspace build and its whole nextest suite except the five strict-bit binaries; two `oce-cxf` OpenModelica reference test groups are `#[cfg(unix)]` and do not compile there: the symlink-rejection test needs unix symlinks, and the line-reference `manifest_contract_tests` claim temporary directories by unix device and inode; an observation, not qualification). A red run on a merge commit is fixed forward or reverted before the next determinism-lane merge. |
| `.github/workflows/release-gate.yml` (heavy) | **Nightly cron against `development`**, **any** PR targeting `main` (it filters on the base branch only — there is no `head_ref == development` condition), pushes to `main`, manual dispatch. Not a push to `development` (issue #545) | **Yes** — full nextest and doctests (`test-suite`), release-codegen nextest (inside `gate (full)`, the only CI job that executes `.agents/gate.sh`, which then requires that run's report), plus a re-run of the light gates (including stale crate-status header lint), an unconditional cargo-deny with advisories, the determinism matrix in all four cells with its fan-in, and the Windows cell. A determinism-lane merge needs the latest nightly not red, or its failure already being fixed forward. |
| `.github/workflows/advisories.yml` | Daily cron, manual dispatch | **No** — advisory/yanked scan only (`cargo deny check advisories`, `yanked = "deny"`, `ignore = []`). |
| `.github/workflows/qemu.yml` (not gating) | Nightly cron, manual dispatch | **The firmware image only** — builds `oce-firmware-link`'s `blob-frames` image for `thumbv7em-none-eabihf` and `riscv32imac-unknown-none-elf`, runs each under QEMU (`netduinoplus2`, riscv32 `virt`) and requires every output line (a run digest per frame for 160 frames, the final digest against `blob_frame_digests.tsv`, the verdict) to equal the host reference runner's ([the nightly emulation run](docs/ci-and-the-gate.md#the-nightly-emulation-run)). Not part of any `CI OK`; `bash .agents/gate.sh qemu` runs it locally. |

Read the first row in the dangerous direction and you will trust a green PR you
should not. A change outside the listed test subsets can show every check green having run
none of its own tests, and the heavy gate that runs them is nightly, not on the merge. The
[local proof](docs/ci-and-the-gate.md#the-local-proof) a PR owes is matched to its lane and does
not run every crate's suite either. Before claiming tests pass on such a change, run the suites
of the crates it touches yourself:

```bash
cargo nextest run -p <crate> --locked --profile ci --no-tests=fail
cargo test -p <crate> --doc --locked
```

**Runner: [`cargo-nextest`](https://nexte.st/)** (pinned `0.9.143`; the repository config rejects an
older binary).

```bash
cargo nextest show-config version         # verify the installed runner satisfies the repository
cargo nextest run --workspace --locked --no-tests=fail
cargo nextest run --workspace --locked --profile ci --no-tests=fail
cargo nextest run --workspace --locked --profile ci-release --cargo-profile release --no-tests=fail
cargo test --workspace --doc --locked     # doctests — nextest CANNOT run these (separate step)
```

The default profile is fail-fast for local feedback. Automated profiles run every selected test and
retry none. Ordinary tests terminate after 120 seconds. All profiles fail when a child process retains inherited output
handles for more than two seconds, and bound a whole run to 15 minutes. `ci-release` inherits `ci` so
runner semantics cannot drift between debug and release codegen; `ci-release-workspace` inherits it
as well and belongs to one command, the gate script's release-codegen run of the whole workspace,
so that run has a report no other run writes. Each writes JUnit under
`target/nextest/<profile>/junit.xml`; CI uploads those files for 14 days.

> nextest does **not** run doctests (a stable-Rust limitation). A complete local/CI test pass is
> therefore always **two commands**: `cargo nextest run` **and** `cargo test --doc`.

## Input hygiene — guarding the inputs to the tests

One gate step tests **no shipping code at all**, and it is worth understanding why it still gates.

`oce-cxf::fixture_port_order` sweeps 47 CXF documents and checks every authored `hasInput` and
`hasOutput` list against upstream CDL **declaration order**. It derives that order at test time by
parsing 132 vendored upstream Modelica sources in
[`third_party/modelica-buildings-cdl/`](third_party/modelica-buildings-cdl/README.md), copied
verbatim at the pinned reference commit. There is no catalog artifact in between, deliberately: a
checked-in table has to be reviewed entry by entry against upstream, which nobody does reliably —
the generator that produced the previous one shipped five defects before it was correct. The
always-on manifest gate in the `structural oracle (input hygiene)` step checks the vendored tree's
local integrity in both directions; the
bucket-scoped comparison in `third_party/modelica-buildings-cdl/README.md` separately spot-checks
the upstream fidelity of the `upstream-buildings` files. The derivation is code a reviewer reads
rather than data a reviewer spot-checks.

By the four pillars above this is not coverage of anything. It matters one level up: **the
46 catalog fixtures are the inputs to every conformance test in the workspace.** Tier-2 goldens and
Tier-A references are all derived from them, so a transposed catalog fixture fails nothing — it makes
the entire suite validate the wrong sequence, silently and permanently.

Nothing else can see it. The resolver assigns port positions from document array order; the arity
guard checks counts and `oce-validate`'s `check_ports_dir` checks each position's *kind*. A
transposition between two ports of the **same kind** passes both. `Reals.PID` with `u_s`/`u_m`
swapped inverts the control action. **30 of 136 blocks are exposed; 282 instances live across 33
of the 46 catalog fixtures.** The 47th document is the `member_list_interface.jsonld` resolver
contract fixture; its `hasInstance`-derived ports add no comparison to this audit.

It gates because its value is entirely future-tense — the corpus is verified clean, so it stays
silent until someone edits a fixture, which is exactly when a person will not remember to run it.
~1.4 s inside the existing gate job.

```bash
cargo nextest run -p oce-cxf --locked -E 'binary(fixture_port_order)'
```

**Expect it to fail when you legitimately add a fixture.** Seven volume pins are deliberate —
`fixtures == 47`, `checked == 2129`, `skipped_array == 37`, `compared == 104` and
`exempt_array == 28` on the registry cross-check, and on the vendored corpus itself
`classes == 132` and `175 inputs / 144 outputs`. A change that stops discovering ports would
otherwise leave the comparison vacuously green, which is the exact failure mode the check exists
to prevent. Re-pin only after understanding why the count moved — a *rising* skip count means
wirings are being hidden from the audit.

**What the derived table rests on.** Before comparing anything the test cross-checks all 104
non-array classes against the shipping block registry on arity, per-position kind, **and names**.
Names were the gap that motivated this audit: the registry stored kinds only, so a **coordinated
rename** passed — two earlier revisions of this file recorded it as passing — because changing the
reference data and the fixtures together left nothing to disagree. The registry now declares port
names (`oce_blocks::port_names`), so the same edit has to change shipping code as well.

The remaining 28 array-port classes are still compared against nothing at all, on any of the three,
since upstream's single `u[nin]` connector has no arity in common with our N flattened scalars.

The names also have runtime authority now. The resolver matches each block's port IRIs against its
class's declared names and **permutes the ports into signature order**, so a document that lists
ports in some other order is wired correctly rather than mis-wired or rejected. That distinction is
the point: the reference toolchain `modelica-json` renders connectors sorted alphabetically by
label, so rejecting a non-declaration order would refuse valid input. Ordering is a renderer's
choice; identity is not.

Binding applies only when every port IRI names a declared port. A document naming none of them —
this engine's own exports mint `.in0`/`.out0` — binds positionally, exactly as before. A document
naming *some* is reported as `port-name-mismatch`, because it follows no convention that can be
read safely either way. The 28 classes with no declared names always bind positionally; they are
the width-driven ones plus the three `Sources.TimeTable` classes, where a flattened array connector
has no 1:1 name correspondence to record.

**The scanner is asserted against its own silence**, because "found nothing" is how every scope
bug in this extractor's history presented itself. A class that parses to zero ports is a hard
failure — but that only catches *total* loss, and a class shedding one connector is the worse
case. `Reals.Sort` is the worked example: drop `yIdx` and the class still reports ports, still
carries an array flag from `y[nin]`, and is skipped by both the registry cross-check and the
fixture comparison, so nothing else here can see it. The `175 / 144` port totals close that, and
they are blind to no per-class exemption.

Totals are a counting invariant, though, and a count is only as strong as the impossibility of
forging what it counts: a phantom port offsets a real one and the pin never moves. Two ways to
forge one are shut. Comment and string bodies are blanked before the scan, so declaration-shaped
text inside `/* … */` or a `Documentation(info=…)` string is not read as a port. And a
multi-component clause — `IntegerInput index, u[nin];` declares two connectors where the scanner
can read one — is refused by name rather than read in part. A forgery written as ordinary code is
outside what this test can distinguish; the always-on manifest gate detects local byte drift, and
the `upstream-buildings` bucket spot-check in
`third_party/modelica-buildings-cdl/README.md` is the upstream-fidelity control.

The git hooks (`pre-commit`, `pre-push`) deliberately **do not** run tests — they stay fast.
Run the suite on demand when you touch behavior; the release gate — nightly on the
`development` tip and on release PRs — is the enforcement point for the whole workspace, and the
per-PR gate for the determinism subset in debug and release codegen.

Profiles live in [`.config/nextest.toml`](.config/nextest.toml): `default` for a fast local
fail-fast loop, `ci` for the gate (no fail-fast, no retries, slow-test guard). The release gate
passes `--no-tests=fail`, so a run that discovers **zero** tests hard-fails — catching the
regression where tests silently stop compiling or being discovered.

`oce-api` re-exports no storage-port item (GitHub #390). Its storage isolation is asserted three
ways: `.github/scripts/check-default-no-db.sh` refuses `oce-store` (and any `oce-store*` name) in its normal tree,
`crates/oce-api/tests/frame_purity.rs` checks its manifest, lockfile closure and sources, and
`scripts/facade_contract/check.py` proves the removed storage names no longer compile.
