# CDL coverage: what "supported" means

For anyone asking "does this run *my* sequence?" This page states which CDL classes the engine
implements, what the 46 conformance fixtures actually are, and — the part that matters most —
what the word "supported" is doing in each of those sentences.

## The standard this page measures against

CDL is published as **ANSI/ASHRAE Standard 231-2026**, *A Control Description Language for
Building Environmental Control Sequences*. This project claims no conformance to the standard.
This page measures coverage of two public things: the
[OpenBuildingControl (OBC) specification](https://obc.lbl.gov/specification/index.html), which is
the informal text of the same language, and the CDL classes of the Modelica Buildings Library
vendored under `third_party/modelica-buildings-cdl/`.

This project has read the standard's published table of contents, not its text. A clause number
of the standard cited in this documentation comes from that table: metadata is §5.13, the Control
Exchange Format (CXF) is §6 and the elementary blocks are §7. The OBC specification numbers its
sections differently: metadata is OBC §7.17 and CXF is OBC §8. An OBC section number cited here is
the public specification's as it was numbered on 2026-11-09. A page that cites the metadata rule
names both (OBC §7.17; ANSI/ASHRAE 231-2026 §5.13).

The standard's table of contents lists no verification, conformance or code-generation clause.
Checking a real controller against a sequence is OBC §12 (Verification), and the
[commissioning verifier](design/commissioning.md) follows that section.

**Elementary blocks.** The standard's public page says: "The normative definitions of elementary
blocks in Standard 231 are based on the LBNL Modelica Buildings Library version 12.1.0"
([data.ashrae.org/standard231](https://data.ashrae.org/standard231/), read 2026-11-09). The
registry and the vendored sources are pinned at Buildings commit `a131864`
(`third_party/modelica-buildings-cdl/README.md`). No CDL block is added or removed between the two:
the file list under `Buildings/Controls/OBC/CDL` is the same at tag `v12.1.0` and at the pin, 343
files of which 318 are `.mo` (checked 2026-11-09). The two trees are not byte-identical, and the
contents of the files were not compared.

**CXF namespace.** On the public evidence the namespace of the standard's CXF vocabulary is
`http://data.ashrae.org/S231#`: OBC §11.3 and modelica-json's `CXF-Core.jsonld` (vendored at
`third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld`) both bind it. This project has not
read the standard's CXF clause (§6), so that rests on those two sources and not on the standard's
text. The engine's exporter (`oce-cxf`) and the authoring emitter (`oce-author`) both bind that
namespace, and both importers read the vocabulary in no other (GitHub #527). The spellings the
importer accepts and refuses are listed under
[host responsibilities](host-responsibilities.md#cxf-point-identities-are-the-authored-ids).

## What the engine does not execute

CDL has three kinds of block. The engine executes two: **elementary blocks**, the registered
classes in the next section, and **composite blocks**, which the CXF resolver lowers to elementary
blocks at load ([CXF composite subset](cxf-composite-subset.md)). It does not execute an
**extension block**. An extension block is implemented by a Functional Mockup Unit (OBC §7.13;
ANSI/ASHRAE 231-2026 §5.9) that its CXF form names with `S231:hasFmuPath` (OBC §8.7; ANSI/ASHRAE
231-2026 §6.7), and the engine has no FMI import: FMI is out of scope before 1.0 (owner ruling,
2026-11-09, GitHub issue #510). Every count on this page, and every use of "supported", is a claim
about elementary and composite blocks. Extension blocks are outside it.

A document that contains an extension block is refused at load with its own code,
`extension-block-unsupported` (GitHub #582, issue #537). A node is an extension block when one
of its `@type` values, at any position of the list, is the S231 class `ExtensionBlock`, when it
carries `S231:hasFmuPath`, whose domain is `ExtensionBlock` (OBC §8.7), or when its `@type`
names such a node of the same document. The class is read the way a modeled property key is
read ([host responsibilities](host-responsibilities.md#cxf-point-identities-are-the-authored-ids)):
its `@type` must expand to `http://data.ashrae.org/S231#ExtensionBlock`, written as that IRI, as
`S231:ExtensionBlock` (under the published binding or, in a document that binds no `S231`, under
none), through another prefix bound to that namespace, or as a `@context` term that expands to
it. A bare `ExtensionBlock` is the S231 class only when a `@context` term expands it there. A class
named `ExtensionBlock` in any other namespace (`http://example.org#ExtensionBlock`, `x:ExtensionBlock`
with `x` bound elsewhere, an unexpanded bare `ExtensionBlock`) is an ordinary class and behaves as a
class of any other name does: unknown as a leaf (`class-not-found`), lowered as a composite when it
lists children, and ignored after a registered class in a type list. The class written in the
`https:` spelling of either S231 namespace is refused as `vocabulary-namespace-mismatch`, as a
modeled key in them is; the namespace the engine wrote through release 0.4.0 is refused that way
for any type, and both refusals come first.

The refusal covers every active node wherever it sits: the root, a child at any depth, an
extension block that lists children of its own, and a node no `containsBlock` edge reaches. One
error per extension block, subject the block's `@id`. It comes before composite lowering and ends
the resolve, so nothing else that lowering or a later stage would find is reported for the
document, about that block or any other node; findings of the earlier `@context` and
specialization passes still are. A node pruned by a false conditional guard is not part of the
schedule and is not refused ([active nodes](cxf-composite-subset.md#active-nodes)). Only a node's
own activity counts: when the document carries an extension-block class's definition, the
document is refused at that definition even if every instance of the class is pruned. An instance
of an extension-block class whose definition the document does not carry cannot be told from any
other unknown class and is still refused with `class-not-found`
([acceptance preconditions](cxf-composite-subset.md#acceptance-preconditions)).

Before the code, a leaf extension block was refused as `class-not-found` and an extension-block
root with no children as `composite/root-count`, and four shapes loaded. An extension block that
listed children, as the root or nested, was lowered as a composite, so those children ran in place
of its FMU; an extension block that no `containsBlock` edge reached was ignored; a node typed
`[<registered class>, S231:ExtensionBlock]` ran the registered class; and `S231:hasFmuPath` on a
registered class was never read. The two fixtures are
`crates/oce-cxf/tests/fixtures/invalid/extension_block_with_fmu_path.jsonld` and
`extension_block_without_fmu_path.jsonld` beside it, and
`crates/oce-cxf/tests/resolve_extension_blocks.rs` pins each position. The advisory code
`missing-fmu-path`, which nothing ever emitted, is removed.

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

Note on `TrueHoldWithReset`: it counts in the `CDL.Logical` row above, but its interface is not the CDL catalog's.
The catalog declares a single `BooleanInput u`. The engine registers a deliberate local two-input
interface (`u` and `clr`; see the class docs in `crates/oce-blocks/src/logical_timing.rs` and the
`port_names.rs` entry). A conforming single-input document is therefore refused by the arity guard.
Whether to keep, alias, or drop the extension is an open owner decision, tracked in
[authority claims](authority-claims.md) (`true-hold-extension`) and product contract PC-022.

Every number in that table is checkable against the checked-in
`tools/reference-catalog/oce-blocks.registry-manifest.json`, a 136-entry ordered JSON array
generated from the registry itself and held byte-identical to it by
`registry::manifest_tests::checked_in_manifest_matches_regenerated_bytes` in `oce-blocks`. The total
is separately pinned in `crates/oce-blocks/src/catalog_tests.rs`.

The three reserved identities are `urn:oce:lowering#PassThrough.Real`, `.Integer`, and `.Boolean`
(`crates/oce-blocks/src/lowering.rs:66-78`). They are what CXF import synthesizes for CDL's direct
boundary input→output connect. They are **not authorable CDL** — a hand-written CXF document cannot
spell them — and each carries `reserved: true` in the catalog so a palette-building host can filter
them out (the `reserved` field of `CatalogEntry`).

## Introspecting the registry at runtime

`oce_api::catalog()` returns the registered classes as `&'static [CatalogEntry]` in deterministic
registry order. A host that depends only on `oce-api` can enumerate every class without adding
`oce-blocks`. Each `CatalogEntry` carries the canonical `class_path`, resolved input and output
ports (`CatalogPort`) in declaration order for default parameters, the port-naming policy
(`CatalogPortNaming`), the complete parameter rules (`CatalogRule`), the authored parameter
defaults (`CatalogParamDefault`), a `width_driven` flag, a conservative `stateful` hint, and the
`reserved` flag. The catalog is metadata only. It carries no units or quantities, and the resolved
instance stays authoritative. Its relationship to the primary facade is classified in the
[product contract](product-contract.md), and its contracts are described in
[facade contracts](facade-contracts.md).

The same module exports the serialization and identity helpers:

- `catalog_to_json` writes any catalog projection as canonical compact JSON (schema revision
  `CATALOG_SCHEMA_REVISION`).
- `catalog_content_id` returns a non-security content tag over those canonical bytes.
- `CATALOG_JSON` is the packaged canonical JSON. A test holds the live registry projection to
  these bytes.

The defaults are **honest**. `CatalogDefault` has seven variants: `Real`, `Integer`, `Boolean`,
`String`, `EnumMember`, `Derived`, and `Required`. A parameter the caller must supply reports
`Required`, not an internal fallback value dressed up as a default. Real literals keep all
binary64 bits, including signed zero.

`oce_blocks::catalog()` remains a supported companion API with its own `CatalogEntry` and
`DefaultSource` types. A host that wants the facade metadata, the JSON form, or the content tag
should use the `oce-api` catalog.

## Port names are an engine contract

`oce_api::class_ports(class_path, params)` names every port of a class as the author configured it,
in port index order (GitHub #440). The catalog above names ports for the 108 fixed-arity classes at
default parameters only; the 25 width-driven classes and the three `TimeTable` sources expose their
ports only once their parameters are bound, so an authoring tool must ask the engine rather than
derive the names itself. The names are part of the engine's contract:

- A scalar connector keeps its CDL name (`u`, `index`, `u_s`).
- A vector connector `u[n]` is `u1`, `u2`, …, `un`: the base name and the 1-based index with no
  separator. This is the spelling the G36 corpus already uses (`mulSum.u1`, `booRep.y4`), and the
  CXF import binds those ports by position in exactly this order.
- A matrix connector `y[r, c]` is `y1_1`, `y1_2`, …, `yr_c`, row-major with the last index
  fastest — the order every matrix block lowers its ports in (`CDL.Reals.MatrixMax` inputs,
  `CDL.Routing.*VectorReplicator` outputs).
- `CDL.Routing.*Extractor` lists `index` before `u1`…`un`, and `CDL.Reals.Sort` lists `y1`…`yn`
  before `yIdx1`…`yIdxn`, as upstream declares them. A `TimeTable` source has one output per value
  column of its table: a `rows x (n + 1)` table gives `y1`…`yn`.

The names are synthesized over the block the registry constructor builds from the parameters
(`oce_blocks::resolved_ports`), so they cannot disagree with the ports a load of the same block
would have. `class_ports` first checks the parameters with the load gate's own parameter rules
(`oce_validate::validate_block_parameters`) and refuses a table a load would refuse with
`ClassPortsError::InvalidParameters`, so a width past the cap is refused rather than named at the
clamped width; an unknown class is `ClassPortsError::UnknownClass`. Neither panics.

Array **parameters** keep the older import spelling, with an underscore before every index: the
catalog lists `k_<i>` and `K_<row>_<col>` once, and `oce_api::param_element_name` expands them to the
`k_1`, `K_2_1` names the CXF import mints for a preserved array.

`oce_api::class_metadata()` adds, for every class in catalog order, a one-sentence `description`
(the upstream Modelica class description string where a vendored source exists, held to it by a
test; engine-written for `CDL.Logical.TrueHoldWithReset` and the three reserved identities) and the
class's parameters, each with its `value_type` (the element type for an array template), its
`default` (a literal, `Derived` with its formula, or `Required`) and a `settable` flag. `settable`
means "an author may assign this parameter": it is false only for the parameters of the three
reserved identities. A derived default such as `MatrixGain.K` or `PIDWithReset.y_reset` is
settable — upstream CDL declares every one as a plain, overridable `parameter`, and the G36 corpus
assigns three of them ([#440 amendment](https://github.com/jscott3201/open-control-engine/issues/440#issuecomment-6730952664)).
None of this enters the revision-1 catalog: `CATALOG_JSON`, `catalog_content_id` and every class
signature hash are unchanged. The names at widths 1, 2 and the maximum, every description, and
every parameter's type, default and settable flag are pinned by
`crates/oce-blocks/tests/fixtures/class_ports.golden.txt`. The metadata is host-side: in
`oce-blocks` it is compiled only with the `std` feature, so no bare-metal image carries it.

## Enumerations and constant packages

`oce_api::enumerations()` lists every named type the engine knows, from the one table every enum
lookup in the engine reads (issue #468): the four CDL enumerations (`CDL.Types.SimpleController`,
`Smoothness`, `Extrapolation`, `ZeroTime`), the ten source-pinned G36 enumerations
(`ASHRAE.G36.Types.ASHRAEClimateZone`, `ControlEconomizer`, `CoolingCoil`, `EnergyStandard`,
`FreezeStat`, `HeatingCoil`, `OutdoorAirSection`, `PressureControl`, `Title24ClimateZone`,
`VentilationStandard`) and the four G36 Integer constant packages (`DemandLimitLevels`,
`FreezeProtectionStages`, `OperationModes`, `ZoneStates`).

- Every row's `class_path` is relative to `Buildings.Controls.OBC.`, the form the catalog uses for
  block classes; `qualified_path` is the full Modelica name.
- An **enumeration** row is a parameter and connector type: it carries the `EnumClassId` a
  `ValueType::Enum` names, and its members, in source order, are one-based ordinals.
- A **constant package** row is never a parameter type. Its members are named Integer constants
  (`OperationModes.occupied` is `1`, `DemandLimitLevels.heating2` is `2`); the CXF import grounds a
  full constant path to its Integer before any expression is evaluated, and a connector typed by
  the package is an Integer connector.
- Last segments are unique across the table, so an authored `Type.Member` names one row.

The table has 18 rows, and their provenance differs. The ten G36 enumerations are checked against
the vendored `.mo` sources, member by member and in order. The four CDL enumerations are not
vendored (`CDL/Types` is outside the vendored subset); they are held to the registry's own
`EnumMembers` rule lists and the typed `SimpleController` / `ZeroTime` parsers. The four constant
packages are not vendored either: they carry exactly the members and values the resolver always
used, pinned by a differential test against the hand-written `match` arms the table replaced, and
nothing upstream checks them. `DemandLimitLevels` lists `cooling0`…`cooling3` then
`heating0`…`heating3`; that order is a choice, not an upstream order — only the name-to-value map of
a constant package carries meaning.

`enumeration(class_path)`, `enumeration_by_id(id)`, `enumeration_class_path(id)`,
`enumeration_member_value(class_path, member)` and `enumeration_types()` (the enumeration rows only)
look rows up without allocating. `enumeration_class_path` returns the **authoring** form above; it
is not the durable spelling. None of it enters the revision-1 catalog: `CATALOG_JSON`, `catalog_content_id` and the
class signature hashes are unchanged. The durable class identity recorded in state snapshots,
compiled-schedule blobs, the replay wire and class signature hashes is
`oce_model::EnumDescriptor::class_path` (from `oce_model::enum_descriptor`), which predates the
table and stays frozen: relative for CDL, **fully qualified** for G36
(`Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil`). A record that must agree with those
writes the durable form, never `enumeration_class_path`.

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

Where a pinned `.mo` graph and its own documentation disagree, OCE executes the graph, and the
Tier-A references follow it too; such references carry a `diverges_from_g36_intent` note. One case
reaches every sequence built on `Generic.TrimAndRespond`: its documentation says the setpoint is
`iniSet` while the device is off, but `iniSet` reaches the reset only through a sampled
`UnitDelay`, so a device-off window that holds no `samplePeriod` instant does not reseed it. After
the re-armed delay the reset resumes from the stale pre-trip value, not from `iniSet`. OCE follows
the `.mo` graph, not the prose (GitHub issue #354).

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
`.mo` sources. CXF arrives already flattened and monomorphic, the `oce-cxf` resolver owns all
lowering, and nothing elaborates the graph after it. Full Modelica elaboration — parameter
propagation, expression folding, conditional-instance removal, `replaceable`/`redeclare`/`extends`
— is not implemented. If your sequence exists only as `.mo`, something
upstream has to produce CXF first.

**Upstream modelica-json output does not load today.** The 44 modelica-json CXF translations
vendored under `third_party/modelica-buildings-cdl/cxf/` all fail to load. They depend on a
cross-document class library the engine does not have, and they use array connectors and array
instances, which Rule 7 of the [CXF composite subset](cxf-composite-subset.md) refuses. The test
header in `crates/oce-cxf/tests/vendored_corpus_delta.rs` states the same limit, and that test pins
the diagnostic counts per document, not a successful load. A host must pre-flatten each sequence
into a monomorphic, scalar-connector CXF document before import, as every supported sequence in
the catalog below already is. Support for the cross-document library and vector ports and
instances is tracked in GitHub #369.

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
then has no cycle, so CDL's model of computation (OBC §7.16; ANSI/ASHRAE 231-2026 §5.12) is
satisfied, but there is no single point at which the block can fire all its outputs.

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
