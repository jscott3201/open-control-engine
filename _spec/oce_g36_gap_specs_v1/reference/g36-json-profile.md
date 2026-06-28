# OCE G36 JSON/CXF Profile

**Profile id:** `oce-g36-cxf-profile-v1`
**Status:** restricted explicit-CXF composite import evidence exists, with selected
`supported-runtime-sequence` claims only for checked-in explicit-CXF variants.

This profile defines the checked-in JSON/CXF subset Open Control Engine will use for
`Buildings.Controls.OBC.ASHRAE.G36` sequence work. It is deliberately narrower than arbitrary
Modelica translation and deliberately broader than the hand-authored representative fixtures. It
covers source-verified explicit-CXF composite import fixtures while preserving the higher bar for
full G36 runtime-sequence support.

## Scope

The profile covers:

- source-proven G36 class and package identity;
- fixture-only pre-flattened representative sequences already in the repo, including
  source-reviewed fragments with explicit upstream source, IO, parameter, and deferred-branch
  manifests;
- structural G36 enum and integer-constant packages;
- profile-only examples for composite identity and parameter-gated connectors;
- restricted explicit-CXF composite import after load-time specialization;
- fail-closed decisions for validation packages, unsupported variants, and conditional guards.

The profile does not add arbitrary `.mo` parsing. `supported-runtime-sequence` claims apply only to
the cataloged checked-in explicit-CXF variants with source, oracle, and determinism evidence.
`oce-cxf` supports a restricted explicit CXF subset: active nested composite nodes are specialized at
load time, then lowered into native `CDL.*` registry blocks with deterministic source-path identity,
parent parameter propagation, boundary connection expansion, and fail-closed diagnostics for
unsupported Modelica constructs.

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

The specialization importer evaluates these guards during CXF load, prunes inactive graph nodes
before block and connector ids are assigned, and emits typed diagnostics for unresolved or
out-of-profile guards. Guard results are not stored in the executable `ModelGraph`, and no runtime
tick path evaluates guard expressions.

## Validation Packages

Any path containing `.Validation` under `Buildings.Controls.OBC.ASHRAE.G36` is reference/test
material only. Validation classes may support oracle development, but the catalog guard must fail if
they are marked `supported-runtime-sequence`.

## Fixture Evidence Levels

`supported-fixture-only`

: A checked-in pre-flattened CXF fixture loads today because all executable child instances are
  native `CDL.*` blocks. It has local deterministic and oracle evidence, but no canonical upstream
  G36 composite import claim. When `source_mapping_status` is `source-reviewed-fragment`, the row
  must also record the reviewed upstream source files, source commit, fixture-local required inputs,
  outputs, active child components, parameters, known deferred branches, and unsupported variants.

`supported-import-fixture`

: A checked-in explicit CXF fixture has a canonical G36 top class, source `.mo` provenance, a
  supported parameter variant, deterministic resolver/API import tests, and a modelgraph golden. It
  proves restricted composite import behavior, but does not claim arbitrary `.mo` translation or
  independent whole-sequence correctness.

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
- `crates/oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld` is the first
  source-verified restricted composite runtime fixture. It encodes
  `Buildings.Controls.OBC.ASHRAE.G36.Generic.TrimAndRespond` for the explicit `have_hol=false`
  variant with independent oracle coverage for delayed activation, sampled trim/respond updates,
  capped negative response, device-off reset, and restart-before-delay behavior. Optional hold
  input/subgraph nodes are present as inactive post-specialization evidence and pruned before the
  executable graph is frozen; `have_hol=true`, hold-runtime behavior, alternate parameterizations,
  and arbitrary `.mo` parsing remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld`,
  `crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld`, and
  `crates/oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld` remain fixture-only representative
  graphs. Their catalog rows are source-reviewed fragments of upstream G36 files, not canonical
  runtime-sequence imports.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_supply_signals.jsonld` is a source-verified
  restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.SupplySignals` with
  `have_heaCoi=true`, `have_cooCoi=true`, and `controllerType=PI`.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_plant_requests.jsonld` is a source-verified
  restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.PlantRequests` with
  `heaCoi=WaterBased`, `cooCoi=WaterBased`, `THys=0.1`, and `posHys=0.05`.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_ahu.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.OutdoorAirFlow.ASHRAE62_1.AHU`
  with `minOADes=SingleDamper`, `VUncDesOutAir_flow=6`, and `VDesTotOutAir_flow=8`.
  Package-level `OutdoorAirFlow`/`ASHRAE62_1` and non-`SingleDamper` variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_sumzone.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.OutdoorAirFlow.ASHRAE62_1.SumZone`
  with `nGro=2`, `nZon=3`, `zonGroMat=[1,1,0;0,1,1]`, and
  `zonGroMatTra=[1,0;1,1;0,1]`. Package-level `ASHRAE62_1`, alternate group/zone sizes,
  alternate matrices, validation variants, and non-default parameter variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_ahu.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.OutdoorAirFlow.Title24.AHU`
  with `minOADes=SingleDamper`, `have_CO2Sen=true`, `VAbsOutAir_flow=3`, and
  `VDesOutAir_flow=8`. No-CO2, `DedicatedDampersPressure`, and non-default AHU parameter
  variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_title24_sumzone.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.OutdoorAirFlow.Title24.SumZone`
  with `nGro=2`, `nZon=3`, `have_CO2Sen=true`, and
  `zonGroMat=[1,1,0;0,1,1]`. Package-level `Title24`, no-CO2, alternate group/zone sizes,
  alternate matrices, validation variants, and non-default parameter variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_relief_damper.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.ReliefDamper` with
  `dpBuiSet=12 Pa`, `k=0.5`, `controllerType=P`, `reverseActing=false`, and supply-fan proof
  switching. Non-default PID variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan.jsonld` is a source-verified
  restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.ReliefFan` with
  `relFanSpe_min=0.1`, `dpBuiSet=12 Pa`, `k=1`, `hys=0.005`, `MovingAverage(delta=300)`,
  `controllerType=P`, `reverseActing=false`, relief-damper latch/clear timing, and relief-fan
  start/stop timing. Non-default PID variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_relief_fan_group.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.ReliefFanGroup` with
  source-default `nSupFan=2`, `nRelFan=4`, `relFanSpe_min=0.1`, `staVec={2,3,1,4}`,
  `relFanMat={{1,0},{1,0},{0,1},{0,1}}`, `dpBuiSet=12 Pa`, `k=1`, `hys=0.005`,
  `MovingAverage(delta=300)`, `controllerType=P`, `reverseActing=false`, stage-up/down timers,
  2 s `TrueDelay` proof acknowledgement, and level-2 alarm damper guards. Arbitrary fan
  counts/matrices and non-default parameter variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_freeze_protection.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.FreezeProtection` with
  source-default `have_frePro=true`, `buiPreCon=ReliefDamper`,
  `minOADes=DedicatedDampersAirflow`, `freSta=No_freeze_stat`, `heaCoi=WaterBased`,
  `cooCoi=WaterBased`, `minHotWatReq=2`, `heaCoiCon=PI`, `k=1`, `Ti=0.5 s`, `Td=0.1 s`,
  `yMax=1`, `yMin=0`, `THys=0.25 K`, staged timers/latches, and default heating-coil PID
  `reverseActing=true`. No-freeze-protection, BAS or hardwired freeze-stat, return/relief-fan
  pressure, `DedicatedDampersPressure` minimum-OA, alternate controller, and non-default parameter
  variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_airflow_tracking.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.ReturnFanAirflowTracking` with
  `difFloSet=1 m3/s`, `conTyp=PI`, `k=1`, `Ti=0.5 s`, `Td=0.1 s`, `minSpe=0`, `maxSpe=1`, and
  supply-fan proof switching return-fan speed to zero. The source boundary alias
  `y1RetFan = u1SupFan` is represented by a fixture-local Boolean identity bridge so the executable
  graph can expose it as an output point. Alternate controller types and non-default parameter
  variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_return_fan_direct_pressure.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.SetPoints.ReturnFanDirectPressure` with
  `dpBuiSet=12 Pa`, `p_rel_RetFan_min=2.4 Pa`, `p_rel_RetFan_max=40 Pa`, `conTyp=PI`, `k=1`,
  `Ti=0.5 s`, `Td=0.1 s`, parent-default `disSpe_min=0.1`, parent-default `disSpe_max=1`,
  `MovingAverage(delta=300)`, default PID `reverseActing=true`, default clamped `Line` blocks,
  relief-damper gating by `u1MinOutAirDam AND u1SupFan`, supply-fan proof switching for
  `dpDisSet` and `yRetFan`, and a fixture-local Boolean identity bridge for the source boundary
  alias `y1RetFan = u1SupFan`. Alternate controller types and non-default parameter variants remain
  deferred.
- `crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_24.jsonld`,
  `crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_21.jsonld`,
  `crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_ashrae_fixed_18.jsonld`,
  `crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_24.jsonld`,
  `crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_23.jsonld`,
  `crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_22.jsonld`,
  and
  `crates/oce-cxf/tests/fixtures/g36/generic_air_economizer_high_limits_title24_fixed_21.jsonld`
  are source-verified restricted runtime-sequence fixtures for
  `Buildings.Controls.OBC.ASHRAE.G36.Generic.AirEconomizerHighLimits` with
  `ecoHigLimCon=FixedDryBulb`. They cover the ASHRAE 90.1 dry-bulb cutoff buckets
  `TCut=297.15 K` for zones 1B/2B/3B/3C/4B/4C/5B/5C/6B/7/8, `TCut=294.15 K` for 5A/6A,
  and `TCut=291.15 K` for 1A/2A/3A/4A, plus the California Title 24 fixed-dry-bulb buckets
  `TCut=297.15 K` for zones 1/3/5/11/12/13/14/15/16, `TCut=296.15 K` for 2/4/10,
  `TCut=295.15 K` for 6/8/9, and `TCut=294.15 K` for 7. `EnergyStandard.Not_Specified`,
  `DifferentialDryBulb`, `FixedDryBulbWithDifferentialDryBulb`, enthalpy branches, `hCut`,
  return-air inputs, and `Economizers.Controller` variants outside the restricted
  SingleDamper/ReliefDamper FixedDryBulb Zone_5A assembly remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_enable.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Enable` with
  `use_enthalpy=false`, `delTOutHis=1 K`, `retDamFulOpeTim=180 s`, `disDel=15 s`, supply-fan
  proof gating, freeze-protection stage zero gating, and the source dry-bulb polarity
  `TOut - TOutCut`. The enthalpy high-limit branch, unrestricted `Economizers.Controller`,
  package-level `Economizers.Subsequences`, package-level `Limits`, `Limits.SeparateWithAFMS`,
  `Limits.SeparateWithDP`, remaining `Modulations` classes, and non-default Enable parameter
  variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_limits_common.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Limits.Common`
  with source-default `controllerType=PI`, `k=0.05`, `Ti=120 s`, `Td=0.1 s`,
  `uRetDam_min=0.5`, physical damper limits `0..1`, `PIDWithReset` triggered by `u1SupFan`, and
  minimum-outdoor-air loop enablement gated by occupied mode plus supply-fan proof.
  Package-level `Limits`, `SeparateWithAFMS`, `SeparateWithDP`, unrestricted `Economizers.Controller`,
  derivative-term behavior, and non-default Common parameter variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_reliefs.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Modulations.Reliefs`
  with source-default `uMin=-0.25`, `uMax=0.25`, `uOutDamMax=0`, `uRetDamMin=0`, clamped outdoor
  and return damper `Line` blocks, and final `Min`/`Max` output clamps. Unrestricted
  `Economizers.Controller`, package-level `Modulations`, package-level `Limits`, and non-default
  Reliefs parameter variants remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_return_fan.jsonld` is a
  source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Subsequences.Modulations.ReturnFan`
  with source-default `have_dirCon=true`, `uMin=-0.25`, `uMax=0.25`, `yRetDam` produced by the
  clamped return damper `Line` block from `uRetDam_max` at `uMin` to `uRetDam_min` at `uMax`, and
  `yOutDam=1`.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_modulations_return_fan_relief_damper.jsonld`
  is a source-verified restricted runtime-sequence fixture for the same upstream ReturnFan class
  with `have_dirCon=false`, `uMin=-0.25`, `uMax=0.25`, `yRetDam` produced by the clamped return
  damper `Line` block, active `yRelDam` produced by a clamped relief damper `Line` block from `0`
  at `uMin` to `1` at `uMax`, and `yOutDam=1`. Unrestricted `Economizers.Controller`,
  package-level `Modulations`, package-level `Limits`, and non-default ReturnFan parameter variants
  remain deferred.
- `crates/oce-cxf/tests/fixtures/g36/multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21.jsonld`
  is a source-verified restricted runtime-sequence fixture for
  `Buildings.Controls.OBC.ASHRAE.G36.AHUs.MultiZone.VAV.Economizers.Controller` with
  `minOADes=SingleDamper`, `buiPreCon=ReliefDamper`, `eneStd=ASHRAE90_1`,
  `ecoHigLimCon=FixedDryBulb`, and `ashCliZon=Zone_5A` selecting `TCut=294.15 K`. The active
  child composites are `damLim=Limits.Common`, `enaDis=Enable`, `modRel=Modulations.Reliefs`, and
  `ecoHigLim=Generic.AirEconomizerHighLimits`; top inputs are `VOutMinSet_flow_normalized`,
  `VOut_flow_normalized`, `uTSup`, `TOut`, `u1SupFan`, `uOpeMod`, and `uFreProSta`; top outputs
  are `yOutDam_min`, `yEnaMinOut`, `yRetDam`, and `yOutDam`. The controller final bindings
  override standalone `Limits.Common` defaults with `kMinOA=1`, `TiMinOA=0.5 s`, and
  `TdMinOA=0.1 s`. `SeparateWithAFMS`, `SeparateWithDP`, return-fan pressure-control branches,
  Title24, differential and enthalpy high-limit modes, package-level Economizers support,
  arbitrary `.mo` parsing, and full `AHUs.MultiZone.VAV.Controller` assembly remain deferred.
- `crates/oce-cxf/tests/fixtures/boundary_fanout.jsonld` is a synthetic regression fixture proving a
  top composite boundary input can fan out to multiple internal input connectors while the facade and
  durable point projection expose one logical host point.

The offline guard validates these examples against the catalog and enum literal registry. It does not
load them as supported runtime sequences.
