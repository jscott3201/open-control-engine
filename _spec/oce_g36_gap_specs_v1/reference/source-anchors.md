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

For the restricted composite-import slice, `TrimAndRespond.mo` is source evidence for the checked-in
explicit-CXF `have_hol=false` fixture. `SupplyTemperature.mo` is a reviewed next-tranche anchor
because it nests `G36.Generic.TrimAndRespond`; it is not implemented by the current fixture.

For representative-sequence hardening, the AHU supply-air-temperature reset, AHU economizer, and
single-zone VAV fixtures are source-reviewed fragments of the listed upstream files. They remain
fixture-only CXF graphs: `Economizers.Subsequences.Modulations` and other package-order entries are
catalog navigation/deferred branches, not implemented runtime support in this tranche.

## Generated docs

- LBNL generated Modelica help page for `Buildings.Controls.OBC.ASHRAE.G36` is useful for navigation but should not replace source `.mo` verification.
