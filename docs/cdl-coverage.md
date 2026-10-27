# CDL coverage: what "supported" means

For anyone asking "does this run *my* sequence?" This page states which CDL classes the engine
implements, what the 46 conformance fixtures actually are, and — the part that matters most —
what the word "supported" is doing in each of those sentences.

## The elementary-block registry

The engine registers **136 block classes**: 133 CDL elementary classes plus 3 reserved internal
lowering identities.

| Family | Classes |
| --- | --- |
| `CDL.Reals` | 52 |
| `CDL.Logical` | 26 |
| `CDL.Integers` | 24 |
| `CDL.Routing` | 15 |
| `CDL.Discrete` | 7 |
| `CDL.Conversions` | 4 |
| `CDL.Psychrometrics` | 3 |
| `CDL.Utilities` | 2 |
| **CDL total** | **133** |
| Reserved lowering identities | 3 |
| **Registry total** | **136** |

Every number in that table is checkable against the checked-in
`tools/reference-catalog/oce-blocks.registry-manifest.json`, a 136-entry ordered JSON array
generated from the registry itself and held byte-identical to it by
`registry::manifest_tests::checked_in_manifest_matches_regenerated_bytes` in `oce-blocks`. The total
is separately pinned at `crates/oce-blocks/src/catalog_tests.rs:27`.

The three reserved identities are `urn:oce:lowering#PassThrough.Real`, `.Integer`, and `.Boolean`
(`crates/oce-blocks/src/lowering.rs:66-78`). They are what CXF import synthesizes for CDL's direct
boundary input→output connect. They are **not authorable CDL** — a hand-written CXF document cannot
spell them — and each carries `reserved: true` in the catalog so a palette-building host can filter
them out (`crates/oce-blocks/src/catalog.rs:98-100`).

## Introspecting the registry at runtime

`oce_blocks::catalog()` (`crates/oce-blocks/src/catalog.rs:155`) returns the registered classes in
deterministic registry order. Each `CatalogEntry` (`crates/oce-blocks/src/catalog.rs:74-101`) carries
resolved input and output ports in declaration order, the port-naming policy, the static parameter
rules, the authored parameter defaults, a `width_driven` flag, a conservative `stateful` hint, and
the `reserved` flag. This is an actively supported companion API, not an internal implementation
detail; its relationship to the primary facade is classified in the
[public surface contract](public-surface-contract.md).

The defaults are **honest**. `DefaultSource` (`crates/oce-blocks/src/catalog.rs:47-57`) has three
variants — `Literal`, `Derived { formula }`, and `Required`. A parameter the caller must supply
reports `Required`, not an internal fallback value dressed up as a default. That distinction is the
whole reason the type exists.

**The caveat that will cost you ten minutes:** `catalog()` lives in `oce-blocks`, and **`oce-api`
does not re-export it**. Reading `crates/oce-api/src/lib.rs`, the string `catalog` does not appear
anywhere in `crates/oce-api/src/` — the `pub use` block at `crates/oce-api/src/lib.rs:50-77`
re-exports `Engine`, the error types, `ExportReport`, IO and sim types, `LoadReport`, the parameter
table, the topology view (`Topology`, `TopologyBlock`, `TopologyConnection`, `DeclaredOutput`,
`PassThroughPair`), `oce_diag::Diagnostic`, `oce_model::{ConnectorId, Value, ValueType}`, and
`oce_store` (including `SemanticQuery`) — and nothing from `oce_blocks`. `oce-api` depends on
`oce-blocks`
(`crates/oce-api/Cargo.toml:29`) and uses it internally, but a consumer depending only on `oce-api`
must add `oce-blocks` as its own dependency to call `catalog()`.

## What "supported" means for a G36 sequence

The repo's own catalog is explicit, and it under-claims on purpose. The `support_policy` block in
`tools/reference-catalog/Buildings.Controls.OBC.ASHRAE.G36.catalog.json` records
`runtime_sequence_status: "selected-explicit-cxf-variants-supported"` and states that supported rows
"are limited to the listed checked-in explicit-CXF variants and do not imply arbitrary ASHRAE G36
composite support."

Concretely, what exists today is:

- **43 pre-specialized runtime variants** (`runtime_sequences`, all status
  `supported-runtime-sequence`) over **31 distinct canonical class paths**.
- **3 hand-authored fixture-only fragments** (`fixture_only_sequences`, status
  `supported-fixture-only`): `ahu_supply_air_temp_reset`, `ahu_economizer`, and `vav_single_zone`.
  Each carries `canonical_g36_class_path_status:
  "fragment-of-canonical-source-not-runtime-sequence"` — they are pre-flattened CXF graphs built
  from supported CDL elementary blocks, with `source-reviewed-fragment` evidence, and they make no
  canonical runtime-sequence claim. Their Tier-A goldens are labelled
  `sequence_derivation: "fixture-local-composition"`, because they use fixture-local inputs in
  place of the G36 sequence's own inputs. They are not answer keys for the G36 sequences they
  borrow from.

All of them are **pre-flattened CXF at specific parameterizations**. A variant is a fixture at a
fixed set of parameter values, not a general instantiation of the class.

The support vocabulary itself is defined at `tools/reference-catalog/README.md:77-86`. Note that one
of its three terms, `supported-import-fixture`, currently labels **zero rows** — grepping the G36
catalog for that string returns no hits. It is defined vocabulary, not present state.

## The promotion bar

A canonical class path is promoted to `supported-runtime-sequence` only once **all six** of these
exist (`tools/reference-catalog/README.md:77-80`), and each runtime row carries the corresponding
field:

| Requirement | Field on the catalog row |
| --- | --- |
| Canonical `Buildings.Controls.OBC.ASHRAE.G36.*` class path | `class_path` |
| Source provenance | `source` |
| Supported parameter variants | `supported_variant` |
| Fixture | `fixture` |
| Deterministic golden trace | `golden_trace`, `determinism_provenance` |
| Independent oracle evidence | `oracle_reference`, `oracle_test` |

A missing element is a missing promotion. That is the bar, and it is why the supported set is small.

## The engine does not read Modelica

The engine executes the block graph that CXF hands it. It does **not** parse or flatten Modelica
`.mo` sources. `oce-flatten` is a reserved seam and an identity passthrough today: CXF arrives
already flattened and monomorphic, the `oce-cxf` resolver owns lowering, and full Modelica
elaboration — parameter propagation, expression folding, conditional-instance removal,
`replaceable`/`redeclare`/`extends` — is explicitly deferred
(`crates/oce-flatten/src/lib.rs:2-20`). If your sequence exists only as `.mo`, something upstream has
to produce CXF first.

## `CDL.Logical.Pre` uses HostTick v1

Registry support for `CDL.Logical.Pre` means the engine accepts its interface and executes the fixed
[HostTick v1 profile](execution-profile.md). It does not mean exact Modelica event-iteration
equivalence. `Pre` emits `pre_u_start` on its first successful host tick, then emits the input from
the preceding successful tick. Repeated calls at one timestamp are separate state transitions.

The scheduler treats `Pre` as a feedthrough cut and accepts a feedback loop without proving that the
corresponding Modelica event iteration converges. This semantic projection applies to any fixture
containing `Pre`; fixture support and deterministic output do not broaden the conformance claim.
The Tier-A references for `Generic.TimeSuppression`, `CoolingOnly.Controller`, and
`ReliefFanGroup` therefore classify their 20 output signals as HostTick v1 profile checks, not
Modelica source-semantics oracles.

## Atomic block firing refuses some CDL-legal feedback

This is a coverage limit: the engine refuses some models that CDL allows. It is tracked as
GitHub issue #347.

**What triggers it.** The engine fires each block atomically: one call produces all of the block's
outputs (`crates/oce-graph/src/topo.rs`). Blocks are then ordered so that a block fires after
every block that drives an input it reads at that call. An algebraic block reads all of its inputs
when it fires. A stateful block reads every input that feeds through to any of its outputs. The
refusal happens when a path leaves one output of a block and comes back into an input that the
block reads when it fires, but that the output does not depend on under CDL. The connector graph
then has no cycle, so CDL §7.16 is satisfied, but there is no single point at which the block can
fire all its outputs.

The reproduction is `CDL.Routing.RealExtractSignal` with `nin = nout = 2`, so `y1 = u1` and
`y2 = u2`, where `y1` reaches the block's own `u2` through a gain:

```text
u -> sel.u1;  sel.y1 -> gai.u;  gai.y -> sel.u2;  sel.y2 -> y
```

The load fails at stage `Schedule` with `BuildError::BlockAlgebraicLoop`. The load receipt holds one
`block-algebraic-loop` error for each block in the cycle (`sel` and `gai`); see the
[facade contract](facade-contracts.md). The error text and every receipt message say that the
model may be CDL-legal, and they name the remedy.

**It fails closed.** The model is refused at load. It is never scheduled in an order that would read
a value from the wrong tick. A refused reload leaves the previously loaded model serving unchanged.

**Which block classes can trigger it.** A block can trigger the refusal only if some input it reads
when it fires does not feed through to at least one of its outputs. Today twelve registered classes
qualify, each only under the parameters given below: nine `CDL.Routing` classes, the two
`CDL.Reals` matrix reductions, and `CDL.Reals.PIDWithReset` in proportional-only mode. Every one of
them is stateless under those parameters, with mixed feedthrough: some output depends on only some
of the inputs the block reads.

| Classes | Feedthrough | Shape that can trigger it |
| --- | --- | --- |
| `CDL.Routing.RealExtractSignal`, `IntegerExtractSignal`, `BooleanExtractSignal` | `y[i]` depends only on `u[extract[i]]` | `nin ≥ 2` and `nout ≥ 1`. This includes a single-output extract (`nout = 1`), because an input it does not select is still read when the block fires. |
| `CDL.Routing.RealVectorFilter`, `IntegerVectorFilter`, `BooleanVectorFilter` | each output depends only on the one input that `msk` selects for it | `nin ≥ 2` and `nout ≥ 1` (at least one `msk` element true) |
| `CDL.Routing.RealVectorReplicator`, `IntegerVectorReplicator`, `BooleanVectorReplicator` | `y[i, j]` depends only on `u[j]` | `nin ≥ 2` and `nout ≥ 1` |
| `CDL.Reals.MatrixMax`, `CDL.Reals.MatrixMin` | with `rowMax` / `rowMin = true` (the default), `y[i]` depends only on row `i`, `u[i, 1..nCol]`; with `false`, `y[j]` depends only on column `j`, `u[1..nRow, j]` | two or more outputs: `nRow ≥ 2` when reducing rows, `nCol ≥ 2` when reducing columns. For example, `MatrixMin` with `nRow = 2, nCol = 1, rowMin = true` and feedback `y[1] → u[2, 1]` is refused. |
| `CDL.Reals.PIDWithReset` | `y` depends only on `u_s` and `u_m`, never on `trigger` | `controllerType = P`. With no integrator the controller holds no state, so it reads `trigger` when it fires even though `trigger` affects nothing. For example, `y → GreaterThreshold → trigger` is refused. In the `PI`, `PD` and `PID` modes the controller is stateful, `trigger` is read only by the state update, and the same feedback loads. |

The list is pinned by a census test in `oce-blocks` (`src/tests/mixed_feedthrough_census.rs`,
with its grid in `mixed_feedthrough_grid.rs`). It builds every registered class with its registry
defaults and at every point of a grid derived from the class's parameter rules and defaults. The
grid sweeps each structural width over `0..=3`, each Boolean and enumeration parameter over every
value, and each selector or mask array over every valid value for those widths. Every other
scalar parameter with a default is probed: at each grid point it is set alone to a few sample
values, and the census checks that no probe changes the mixed pairs. The parameters whose probes
change whether a block is stateful are a pinned, reviewed list; today that is only comparator
hysteresis `h`. The test then checks that the set of classes with mixed feedthrough is exactly
these twelve, and that the shape condition in the table matches every grid point. String
parameters, non-structural arrays and widths above 3 are not varied, so the census is exhaustive
over its grid and sampled beyond it, not a proof. A class added later with mixed feedthrough
over that grid fails the test until this table names it. `oce-graph` and `oce-api` tests show
each listed class refusing feedback of this shape.

Every other registered class, and every listed class outside its trigger condition, has outputs
that all depend on every input it reads when it fires. That set includes `MatrixGain`, `Sort`, the
scalar replicators, `Extractor`, a single-output `MatrixMax` or `MatrixMin`, `CDL.Reals.PID` in
every mode, `PIDWithReset` in its stateful modes, and the multi-output stateful blocks
`Logical.Timer`, `Logical.TimerAccumulating`, `Logical.Proof` and `Integers.Change`. Feedback
through any of them is either a genuine connector-level loop (`algebraic-loop`) or is cut by a
state-holding block. The scheduler also handles a stateful block that has a state-only output next
to a feedthrough output, but no registered class has that shape today. The scheduler's own tests
cover that case with synthetic blocks.

**Remedies.** Restructure the model so that the output on the loop and the input fed back to it are
not on the same block:

- **Split the routing block or matrix reduction.** Replace it with separate blocks whose input
  vectors hold only the elements each output uses. For a matrix reduction, that means one
  single-output reduction per row or column on the loop. In the reproduction, a
  `RealExtractSignal` with `nin = 1` would read `u1` and produce `y1`, and a second block would
  carry `u2` to `y2`. When the selection is fixed, you can also connect the selected element
  directly.
- **Drop the reset input of a proportional-only reset PID.** In P mode `trigger` has no effect,
  so `CDL.Reals.PID` with `controllerType = P` computes the same output without it.
- **Insert a delay,** for example `CDL.Discrete.UnitDelay` or `CDL.Logical.Pre`, on the feedback
  path, but only where a one-step delay is acceptable for the sequence. This changes the model's
  behaviour, so it is not equivalent to the original.

Firing a block's outputs separately, using the per-output feedthrough map, would schedule these
models without restructuring. That change to scheduling semantics is tracked in GitHub issue #347.
It would have to leave the determinism goldens and strict-bit evidence unchanged for every model
accepted today.

**Corpus status.** No checked-in G36 fixture, Library rule fixture, composite-contract fixture,
conformance fixture or vendored modelica-json translation uses an affected routing class, or a
multi-output `MatrixMax` or `MatrixMin` in feedback. The same is true of the vendored `.mo` sources,
apart from the class definitions themselves. Every tracked CXF document that instantiates
`PIDWithReset` sets `controllerType = PI` or leaves the `PI` default. Some vendored `.mo` sequences
pass `controllerType` through from their own parameters, and those were not checked for feedback
into `trigger`. The only tracked document refused with `block-algebraic-loop` is the reproduction
fixture, `crates/oce-api/src/tests/fixtures/extract_signal_block_loop.jsonld`.

## 47 fixture documents is not 47 sequences

`crates/oce-conformance/tests/fixtures/golden/g36_traces/` holds 46 `.csv` traces and 46 matching
`.prov.json` provenance records — 92 files. `EXPECTED_G36_FIXTURES` at
`crates/oce-cxf/tests/export_g36_roundtrip.rs:46` pins 47 CXF documents: those 46 catalog fixtures
plus `member_list_interface.jsonld`, a resolver contract fixture with no conformance trace or G36
catalog claim.

**The 46 catalog fixtures are configurations, not 46 distinct G36 sequences.** Across the 43 runtime
variants, the 31 distinct canonical class paths distribute like this:

- 29 class paths appear **once**;
- `…Economizers.Subsequences.Modulations.ReturnFan` appears **twice**;
- `Buildings.Controls.OBC.ASHRAE.G36.Generic.AirEconomizerHighLimits` appears **12 times**.

Those 12 are the air-economizer high-limit family: 4 ASHRAE 90.1 variants (`differential`, and fixed
dry-bulb at 18 / 21 / 24) and 8 Title 24 variants (4 differential offsets and fixed dry-bulb at 21 /
22 / 23 / 24). Twelve fixtures, one class, twelve parameterizations.

So the honest reading is: **47 checked-in CXF documents comprise 46 catalog configurations covering
31 canonical G36 class paths plus 3 non-canonical fragments, and one resolver contract fixture.**
Any count of distinct sequences is smaller, and a public-facing number must say which set it means.

## What the coverage is evidence *of*

Breadth of fixtures is not the same as correctness against the standard, and this repo separates the
two deliberately. The evidence layers — engine-self-output determinism goldens, the per-PR structural
diff against vendored `modelica-json` translations, and the oracle layer generated behind a
code-dependency firewall — are described in [`../README.md`](../README.md) and
[`../TESTING.md`](../TESTING.md).

The load-bearing limitation, stated plainly: **no complete G36 sequence here has been executed against
an external Modelica / Buildings toolchain.** Scoped OpenModelica evidence covers one exhaustive
`CDL.Logical.Nand` Boolean case, one stateful `CDL.Logical.Toggle` event schedule, one finite
`CDL.Reals.Line` matrix with exact binary64 operations, and one seven-state exact-bit case for the
composed G36 `Reliefs` leaf. The Line and Reliefs results do not cover arbitrary Real inputs,
general tolerances, solver behavior, or broader G36 behavior. `CDL.Logical.Pre` is explicitly
excluded from an expected-green OpenModelica differential under HostTick v1. The global Tier-3
report remains skipped, and no number on this page stands in for that deferred coverage.

For what happens when you export a loaded sequence back out to CXF, see
[`cxf-round-trip.md`](cxf-round-trip.md).
