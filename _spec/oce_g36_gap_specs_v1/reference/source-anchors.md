# Source Anchors for G36 Work

Codex must re-fetch these sources locally and record exact commit SHAs in PRs.

## Open Control Engine

- `README.md` — current status, crate map, deterministic/database-free posture.
- `AGENTS.md` — project norms, testing expectations, naming constraints.
- `TESTING.md` — required test standard.
- `crates/oce-cxf/src/bridge.rs` — current IRI to canonical CDL class-path bridge.
- Existing G36 fixtures/tests to locate before adding new ones.

## Buildings G36

- `Buildings/Controls/OBC/ASHRAE/G36/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Controller.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Limits/Common.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/Reliefs.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Modulations/ReturnFan.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/FreezeProtection.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/PlantRequests.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/package.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/package.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/AHU.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/ASHRAE62_1/SumZone.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/AHU.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/OutdoorAirFlow/Title24/SumZone.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefDamper.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFan.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReliefFanGroup.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanAirflowTracking.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/ReturnFanDirectPressure.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyFan.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplySignals.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/Controller.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/Supply.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/SupplyFan.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/Generic/package.order`
- `Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo`
- `Buildings/Controls/OBC/ASHRAE/G36/Types/package.order`
- every `Buildings/Controls/OBC/ASHRAE/G36/Types/*.mo` file used by the scoped sequence.

For the restricted composite-import slices, `TrimAndRespond.mo` is source evidence for the checked-in
explicit-CXF `have_hol=false` runtime fixture, including delayed activation, sampled request
trim/respond updates, capped response, and device-off reset behavior; `have_hol=true`, the runtime
hold input branch, alternate parameterizations, and arbitrary `.mo` parsing remain deferred.
`SupplyTemperature.mo` and `SupplyFan.mo` are reviewed
multizone VAV setpoint anchors because they nest `G36.Generic.TrimAndRespond`. `SupplySignals.mo`
is a reviewed multizone VAV setpoint anchor for the supply-temperature loop and coil-command
sequencing. `PlantRequests.mo` is a reviewed multizone VAV setpoint anchor for chilled-water and
hot-water reset/plant request logic under the WaterBased coil branches. `OutdoorAirFlow.ASHRAE62_1`
is reviewed only for the scalar AHU leaf with `minOADes=SingleDamper`, `VUncDesOutAir_flow=6`,
and `VDesTotOutAir_flow=8`, plus a fixed `SumZone` variant with `nGro=2`, `nZon=3`,
`zonGroMat=[1,1,0;0,1,1]`, and `zonGroMatTra=[1,0;1,1;0,1]`; ASHRAE62_1 package-level support,
alternate group/zone sizes, alternate matrices, validation variants, and non-default variants
remain deferred. `OutdoorAirFlow.Title24.AHU` is reviewed only for the scalar AHU leaf with
`minOADes=SingleDamper`, `have_CO2Sen=true`,
`VAbsOutAir_flow=3`, and `VDesOutAir_flow=8`. `OutdoorAirFlow.Title24.SumZone` is reviewed only
for a fixed `nGro=2`, `nZon=3`, `have_CO2Sen=true`, `zonGroMat=[1,1,0;0,1,1]` variant; Title24
package-level support, no-CO2, alternate group/zone sizes, alternate matrices,
`DedicatedDampersPressure`, validation variants, and non-default variants remain deferred.
`ReliefDamper.mo` is
reviewed only for the scalar no-fan relief-damper variant with `dpBuiSet=12 Pa`, `k=0.5`,
`controllerType=P`, `reverseActing=false`, and supply-fan proof switching. `ReliefFan.mo` is
reviewed only for the scalar single-relief-fan variant with `relFanSpe_min=0.1`,
`dpBuiSet=12 Pa`, `k=1`, `hys=0.005`, `MovingAverage(delta=300)`, `controllerType=P`,
`reverseActing=false`, relief-damper latch/clear timing, and relief-fan start/stop timing;
`ReturnFanAirflowTracking.mo` is reviewed only for the scalar airflow-tracking variant with
`difFloSet=1 m3/s`, `conTyp=PI`, `k=1`, `Ti=0.5 s`, `Td=0.1 s`, `minSpe=0`, `maxSpe=1`,
supply-fan proof switching, and the source boundary alias `y1RetFan = u1SupFan` represented by
an explicit fixture-local Boolean identity bridge. `ReturnFanDirectPressure.mo` is reviewed only for
the scalar direct-pressure variant with `dpBuiSet=12 Pa`, `p_rel_RetFan_min=2.4 Pa`,
`p_rel_RetFan_max=40 Pa`, `conTyp=PI`, `k=1`, `Ti=0.5 s`, `Td=0.1 s`, parent-default
`disSpe_min=0.1`, parent-default `disSpe_max=1`, `MovingAverage(delta=300)`, default PID
`reverseActing=true`, default clamped `Line` blocks, relief-damper gating by
`u1MinOutAirDam AND u1SupFan`, supply-fan proof switching for `dpDisSet` and `yRetFan`, and the
source boundary alias `y1RetFan = u1SupFan` represented by an explicit fixture-local Boolean
identity bridge. `Generic.AirEconomizerHighLimits.mo` is reviewed only for the restricted
`ecoHigLimCon=FixedDryBulb` and `ecoHigLimCon=DifferentialDryBulb` table buckets: ASHRAE 90.1
FixedDryBulb `TCut=297.15 K` for ASHRAE climate zones
1B/2B/3B/3C/4B/4C/5B/5C/6B/7/8, `TCut=294.15 K` for 5A/6A, and `TCut=291.15 K` for
1A/2A/3A/4A; California Title 24 FixedDryBulb `TCut=297.15 K` for zones
1/3/5/11/12/13/14/15/16, `TCut=296.15 K` for 2/4/10, `TCut=295.15 K` for 6/8/9, and
`TCut=294.15 K` for zone 7; ASHRAE allowed DifferentialDryBulb `TCut=TRet`; and California
Title 24 DifferentialDryBulb return-air offset buckets `TCut=TRet`, `TCut=TRet - 1 K`,
`TCut=TRet - 2 K`, and `TCut=TRet - 3 K`. Zero-offset DifferentialDryBulb fixtures use an
explicit `AddParameter(p=0 K)` identity bridge, while nonzero Title 24 offsets use the source
`addPar*` parameter blocks. The `EnergyStandard.Not_Specified` and invalid-zone assertions,
`FixedDryBulbWithDifferentialDryBulb`, enthalpy branches, `hCut`, and `Economizers.Controller`
variants outside the restricted SingleDamper/ReliefDamper FixedDryBulb Zone_5A assembly remain
deferred.
`Economizers.Subsequences.Enable.mo` is reviewed only for the restricted dry-bulb
variant with `use_enthalpy=false`, `delTOutHis=1 K`, `retDamFulOpeTim=180 s`, `disDel=15 s`,
supply-fan proof gating, freeze-protection stage zero gating, and the source `TOut - TOutCut`
hysteresis polarity. The enthalpy high-limit branch, unrestricted `Economizers.Controller`, package-level
`Economizers.Subsequences`, package-level `Limits`, `Limits.SeparateWithAFMS`,
`Limits.SeparateWithDP`, remaining `Modulations` classes, and other Enable parameterizations
remain deferred. `Economizers.Subsequences.Limits.Common.mo` is reviewed only for the
source-default common-damper limits leaf with `controllerType=PI`, `k=0.05`, `Ti=120 s`,
`Td=0.1 s`, `uRetDam_min=0.5`, physical damper limits `0..1`, `PIDWithReset` triggered by
`u1SupFan`, and `yEnaMinOut = u1SupFan AND (uOpeMod == OperationModes.occupied)`. Package-level
`Limits`, `SeparateWithAFMS`, `SeparateWithDP`, unrestricted `Economizers.Controller`, derivative-term
behavior, and non-default Common parameterizations remain deferred.
`Economizers.Subsequences.Modulations.Reliefs.mo` is reviewed only for the source-default
relief/barometric modulation leaf with `uMin=-0.25`, `uMax=0.25`, `uOutDamMax=0`,
`uRetDamMin=0`, clamped outdoor and return damper `Line` blocks, and final `Min`/`Max` output
clamps. Unrestricted `Economizers.Controller`, package-level `Modulations`, package-level `Limits`, and
non-default Reliefs parameterizations remain deferred.
`Economizers.Subsequences.Modulations.ReturnFan.mo` is reviewed for two explicit variants:
source-default `have_dirCon=true` with `uMin=-0.25`, `uMax=0.25`, `yRetDam` produced by the
clamped return damper `Line` block from `uRetDam_max` at `uMin` to `uRetDam_min` at `uMax`, and
`yOutDam=1`; and `have_dirCon=false`, which additionally activates `yRelDam` through a clamped
relief damper `Line` block from `0` at `uMin` to `1` at `uMax`. Unrestricted
`Economizers.Controller`, package-level `Modulations`, package-level `Limits`, and non-default
ReturnFan parameterizations remain deferred.
`Economizers.Controller.mo` is reviewed only for the restricted first controller assembly with
`minOADes=SingleDamper`, `buiPreCon=ReliefDamper`, `eneStd=ASHRAE90_1`,
`ecoHigLimCon=FixedDryBulb`, and `ashCliZon=Zone_5A` (`TCut=294.15 K`). The active child
composites are `damLim=Limits.Common`, `enaDis=Enable`, `modRel=Modulations.Reliefs`, and
`ecoHigLim=Generic.AirEconomizerHighLimits`; top inputs are
`VOutMinSet_flow_normalized`, `VOut_flow_normalized`, `uTSup`, `TOut`, `u1SupFan`, `uOpeMod`,
and `uFreProSta`; top outputs are `yOutDam_min`, `yEnaMinOut`, `yRetDam`, and `yOutDam`. The
controller final bindings override the standalone `Limits.Common` defaults with `kMinOA=1`,
`TiMinOA=0.5 s`, and `TdMinOA=0.1 s`. `SeparateWithAFMS`, `SeparateWithDP`, return-fan
pressure-control branches, Title24, differential and enthalpy high-limit modes, package-level
Economizers support, arbitrary `.mo` parsing, and full `AHUs.MultiZone.VAV.Controller` assembly
remain deferred.
`ReliefFanGroup.mo` is reviewed only for the source-default `nSupFan=2`, `nRelFan=4`
variant with `relFanSpe_min=0.1`, `staVec={2,3,1,4}`,
`relFanMat={{1,0},{1,0},{0,1},{0,1}}`, `dpBuiSet=12 Pa`, `k=1`, `hys=0.005`,
`MovingAverage(delta=300)`, `controllerType=P`, `reverseActing=false`, stage-up/down timers,
2 s `TrueDelay` proof acknowledgement, and level-2 alarm damper guards. Arbitrary fan
counts/matrices and non-default variants remain deferred. `FreezeProtection.mo` is reviewed only
for the source-default variant with `have_frePro=true`, `buiPreCon=ReliefDamper`,
`minOADes=DedicatedDampersAirflow`, `freSta=No_freeze_stat`, `heaCoi=WaterBased`,
`cooCoi=WaterBased`, `minHotWatReq=2`, `heaCoiCon=PI`, `k=1`, `Ti=0.5 s`, `Td=0.1 s`,
`yMax=1`, `yMin=0`, `THys=0.25 K`, and default heating-coil PID `reverseActing=true`.
No-freeze-protection, BAS/hardwired freeze-stat, return/relief-fan pressure,
`DedicatedDampersPressure` minimum-OA, alternate controller, and non-default variants remain
deferred.
Each runtime claim must stay tied to its explicit checked-in CXF fixture and supported parameter
variant.

For representative-sequence hardening, the AHU supply-air-temperature reset, AHU economizer, and
single-zone VAV fixtures are source-reviewed fragments of the listed upstream files. They remain
fixture-only CXF graphs: other `Economizers.Subsequences.Modulations` package-order entries are
catalog navigation/deferred branches, not implemented runtime support in this tranche.

## Generated docs

- LBNL generated Modelica help page for `Buildings.Controls.OBC.ASHRAE.G36` is useful for navigation but should not replace source `.mo` verification.
