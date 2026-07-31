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
the `reserved` flag.

The defaults are **honest**. `DefaultSource` (`crates/oce-blocks/src/catalog.rs:47-57`) has three
variants — `Literal`, `Derived { formula }`, and `Required`. A parameter the caller must supply
reports `Required`, not an internal fallback value dressed up as a default. That distinction is the
whole reason the type exists.

**The caveat that will cost you ten minutes:** `catalog()` lives in `oce-blocks`, and **`oce-api`
does not re-export it**. Reading `crates/oce-api/src/lib.rs`, the string `catalog` does not appear
anywhere in `crates/oce-api/src/` — the `pub use` block at `crates/oce-api/src/lib.rs:49-76`
re-exports `Engine`, the error types, `ExportReport`, IO and sim types, `LoadReport`, the parameter
table, `Topology`, `oce_diag::Diagnostic`, `oce_model::{ConnectorId, Value, ValueType}`, and
`oce_store` — and nothing from `oce_blocks`. `oce-api` depends on `oce-blocks`
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
  canonical runtime-sequence claim.

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

## 46 fixtures is not 46 sequences

`crates/oce-conformance/tests/fixtures/golden/g36_traces/` holds 46 `.csv` traces and 46 matching
`.prov.json` provenance records — 92 files. The same 46 names are pinned as
`EXPECTED_G36_FIXTURES` at `crates/oce-cxf/tests/export_g36_roundtrip.rs:44`, and they are exactly
the 43 runtime-sequence rows plus the 3 fixture-only rows from the G36 catalog: the two name sets
match with no leftovers on either side.

**These are conformance fixtures — configurations — not 46 distinct G36 sequences.** Across the 43
runtime variants, the 31 distinct canonical class paths distribute like this:

- 29 class paths appear **once**;
- `…Economizers.Subsequences.Modulations.ReturnFan` appears **twice**;
- `Buildings.Controls.OBC.ASHRAE.G36.Generic.AirEconomizerHighLimits` appears **12 times**.

Those 12 are the air-economizer high-limit family: 4 ASHRAE 90.1 variants (`differential`, and fixed
dry-bulb at 18 / 21 / 24) and 8 Title 24 variants (4 differential offsets and fixed dry-bulb at 21 /
22 / 23 / 24). Twelve fixtures, one class, twelve parameterizations.

So the honest reading of "46" is: **46 checked-in configurations covering 31 canonical G36 class
paths plus 3 non-canonical fragments.** Any count of distinct sequences is smaller than 46, and the
public-facing number should say which of the two it means.

## What the coverage is evidence *of*

Breadth of fixtures is not the same as correctness against the standard, and this repo separates the
two deliberately. The evidence layers — engine-self-output determinism goldens, the per-PR structural
diff against vendored `modelica-json` translations, and the oracle layer generated behind a
code-dependency firewall — are described in [`../README.md`](../README.md) and
[`../TESTING.md`](../TESTING.md).

The load-bearing limitation, stated plainly: **no sequence here has been executed against the
official reference implementation of the standard.** Tier 3, the cross-implementation differential
against an external Modelica / Buildings toolchain, is not wired. That is the deliberately deferred
tail, and no number on this page should be read as standing in for it.

For what happens when you export a loaded sequence back out to CXF, see
[`cxf-round-trip.md`](cxf-round-trip.md).
