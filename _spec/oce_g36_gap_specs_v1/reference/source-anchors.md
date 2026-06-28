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
- `Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/package.order`
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
explicit-CXF `have_hol=false` fixture. `SupplyTemperature.mo` and `SupplyFan.mo` are reviewed
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
identity bridge. `Economizers.Subsequences.Enable.mo` is reviewed only for the restricted dry-bulb
variant with `use_enthalpy=false`, `delTOutHis=1 K`, `retDamFulOpeTim=180 s`, `disDel=15 s`,
supply-fan proof gating, freeze-protection stage zero gating, and the source `TOut - TOutCut`
hysteresis polarity. The enthalpy high-limit branch, full `Economizers.Controller`, package-level
`Economizers.Subsequences`, `Limits`, `Modulations`, and other Enable parameterizations remain
deferred. `ReliefFanGroup`, freeze-protection sequences, and non-default variants remain deferred.
Each runtime claim must stay tied to its explicit checked-in CXF fixture and supported parameter
variant.

For representative-sequence hardening, the AHU supply-air-temperature reset, AHU economizer, and
single-zone VAV fixtures are source-reviewed fragments of the listed upstream files. They remain
fixture-only CXF graphs: `Economizers.Subsequences.Modulations` and other package-order entries are
catalog navigation/deferred branches, not implemented runtime support in this tranche.

## Generated docs

- LBNL generated Modelica help page for `Buildings.Controls.OBC.ASHRAE.G36` is useful for navigation but should not replace source `.mo` verification.
