# OCE G36 JSON/CXF Profile

**Profile id:** `oce-g36-cxf-profile-v1`
**Status:** catalog/profile foundation; runtime composite import is not implemented by this profile.

This profile defines the checked-in JSON/CXF subset Open Control Engine will use for
`Buildings.Controls.OBC.ASHRAE.G36` sequence work. It is deliberately narrower than arbitrary
Modelica translation and deliberately broader than the current hand-authored representative
fixtures, so later PRs can implement G36 types, load-time specialization, and composite flattening
without changing the contract.

## Scope

The profile covers:

- source-proven G36 class and package identity;
- fixture-only pre-flattened representative sequences already in the repo;
- structural G36 enum and integer-constant packages;
- profile-only examples for composite identity and parameter-gated connectors;
- fail-closed decisions for validation packages, unsupported variants, and conditional guards.

The profile does not add runtime support for canonical `Buildings.Controls.OBC.ASHRAE.G36.*`
composite class loading. Today, `oce-cxf` still lowers already-flattened CXF graphs whose child
instances resolve to native `CDL.*` registry entries.

## Class Identity

Canonical upstream G36 paths must use fully-qualified class paths:

```text
Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Controller
Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplyTemperature
Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard
```

The existing fixture-only examples retain their fixture-local top ids:

```text
http://example.org#g36.ahu_supply_air_temp_reset
http://example.org#g36.ahu_economizer
http://example.org#g36.vav_single_zone
```

Those ids are evidence for the current fixture topology only. They are not canonical upstream G36
class paths and must not be counted as `supported-runtime-sequence`.

## Instance Identity

All profile examples preserve hierarchical instance paths in `@id` fragments. A future flattener may
lower them to dense scheduler indices, but user-facing diagnostics, point-list export, and sequence
evidence must retain the source path.

Rules:

- a top sequence node is `S231:Block` with `S231:containsBlock`;
- child instances use an `@type` class IRI;
- member/overlay nodes use the child path plus a final member segment;
- flattened array elements use existing 1-based row-major underscore naming from the base CXF
  profile.

## Parameters

Parameters must record enough information for source review and deterministic specialization:

- `S231:value`;
- `S231:isOfDataType` when the value type is not inferable;
- `S231:unit`, `S231:quantity`, `S231:min`, `S231:max`, and `S231:isFinal` when present upstream;
- enum literals as fully-qualified strings.

G36 enum values must use the canonical literal form:

```json
{
  "@id": "http://example.org#g36.example.venStd",
  "S231:isOfDataType": {
    "@id": "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard"
  },
  "S231:value": "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard.California_Title_24"
}
```

Integer stand-ins for enum literals are rejected by this profile. G36 integer-constant packages such
as `OperationModes` and `ZoneStates` are cataloged separately and must be referenced by package and
constant name when used as structural constants.

## Conditional Components And Connectors

Conditional graph structure is load-time specialization only. Runtime connectors, time, functions,
and arithmetic are not allowed inside guards for this profile.

Allowed guard forms:

- `parameter`;
- `!parameter`;
- `parameter == fully.qualified.Enum.literal`;
- `parameter != fully.qualified.Enum.literal`;
- boolean `and` / `or`;
- parentheses for grouping.

Rejected guard forms:

- runtime connector references;
- time references;
- function calls;
- arithmetic;
- unknown parameters;
- integer stand-ins for enum literals.

Until the specialization importer lands, conditional examples are profile fixtures only. A runtime
fixture that carries `S231:isConditionalComponent` or `S231:conditionalExpression` without a matching
supported specialization path must fail closed or remain unaccepted.

## Validation Packages

Any path containing `.Validation` under `Buildings.Controls.OBC.ASHRAE.G36` is reference/test
material only. Validation classes may support oracle development, but the catalog guard must fail if
they are marked `supported-runtime-sequence`.

## Fixture Evidence Levels

`supported-fixture-only`

: A checked-in pre-flattened CXF fixture loads today because all executable child instances are
  native `CDL.*` blocks. It has local deterministic and oracle evidence, but no canonical upstream
  G36 composite import claim.

`supported-runtime-sequence`

: A canonical `Buildings.Controls.OBC.ASHRAE.G36.*` class path loads, specializes, flattens,
  validates, freezes, and simulates with explicit source provenance, supported parameter variants,
  fixture path, golden trace, and independent oracle evidence.

`validation-only`

: Upstream validation/reference package; never production runtime support.

`structural-type`

: G36 enum or integer-constant package used by parameters and guards.

`deferred` / `unknown-pending-source-review`

: Cataloged but not supportable yet.

## Checked-In Profile Fixtures

- `fixtures/small-composite.jsonld` is a small profile-only composite with two constants and one
  supported CDL child block.
- `fixtures/parameter-gated-connector.jsonld` is a profile-only G36 enum parameter plus a conditional
  connector guard over that parameter.

The offline guard validates these examples against the catalog and enum literal registry. It does not
load them as supported runtime sequences.
