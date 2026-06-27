//! Checked-in data bindings for the ASHRAE G36 catalog guard tests.

pub(super) const CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/Buildings.Controls.OBC.ASHRAE.G36.catalog.json"
));
pub(super) const PROV_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tools/reference-catalog/Buildings.Controls.OBC.ASHRAE.G36.prov.json"
));

pub(super) const AHU_SAT_RESET: &str =
    include_str!("../tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");
pub(super) const AHU_ECONOMIZER: &str = include_str!("../tests/fixtures/g36/ahu_economizer.jsonld");
pub(super) const VAV_SINGLE_ZONE: &str =
    include_str!("../tests/fixtures/g36/vav_single_zone.jsonld");
pub(super) const G36_TRIM_AND_RESPOND: &str =
    include_str!("../tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld");
pub(super) const G36_SUPPLY_TEMPERATURE: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_supply_temperature.jsonld");
pub(super) const G36_SUPPLY_FAN: &str =
    include_str!("../tests/fixtures/g36/multizone_vav_supply_fan.jsonld");
pub(super) const PROFILE_SMALL_COMPOSITE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../_spec/oce_g36_gap_specs_v1/reference/fixtures/small-composite.jsonld"
));
pub(super) const PROFILE_PARAMETER_GATED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../_spec/oce_g36_gap_specs_v1/reference/fixtures/parameter-gated-connector.jsonld"
));

pub(super) const EXPECTED_REFERENCE_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";
pub(super) const EXPECTED_CATALOG_FINGERPRINT: &str = "31ef604134497a1c";

pub(super) const EXPECTED_PACKAGE_ORDER_FILES: &[&str] = &[
    "Buildings/Controls/OBC/ASHRAE/G36/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Generic/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/DemandLimitLevels/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/FreezeProtectionStages/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/OperationModes/package.order",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/ZoneStates/package.order",
];
pub(super) const EXPECTED_SEQUENCE_SOURCE_FILES: &[&str] = &[
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Controller.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Controller.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/Economizers/Subsequences/Enable.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyFan.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/MultiZone/VAV/SetPoints/SupplyTemperature.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/Controller.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/Supply.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/AHUs/SingleZone/VAV/SetPoints/SupplyFan.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Generic/TrimAndRespond.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Generic/AirEconomizerHighLimits.mo",
];
pub(super) const EXPECTED_TYPE_SOURCE_FILES: &[&str] = &[
    "Buildings/Controls/OBC/ASHRAE/G36/Types/ASHRAEClimateZone.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/ControlEconomizer.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/CoolingCoil.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/EnergyStandard.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/FreezeStat.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/HeatingCoil.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/OutdoorAirSection.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/PressureControl.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/Title24ClimateZone.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/VentilationStandard.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/DemandLimitLevels/package.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/FreezeProtectionStages/package.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/OperationModes/package.mo",
    "Buildings/Controls/OBC/ASHRAE/G36/Types/ZoneStates/package.mo",
];

#[derive(Clone, Copy)]
pub(super) struct FixtureSource {
    pub(super) name: &'static str,
    pub(super) path: &'static str,
    pub(super) text: &'static str,
}

pub(super) const RUNTIME_FIXTURES: &[FixtureSource] = &[
    FixtureSource {
        name: "ahu_supply_air_temp_reset",
        path: "crates/oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld",
        text: AHU_SAT_RESET,
    },
    FixtureSource {
        name: "ahu_economizer",
        path: "crates/oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld",
        text: AHU_ECONOMIZER,
    },
    FixtureSource {
        name: "vav_single_zone",
        path: "crates/oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld",
        text: VAV_SINGLE_ZONE,
    },
];

pub(super) const COMPOSITE_IMPORT_FIXTURES: &[FixtureSource] = &[
    FixtureSource {
        name: "trim_and_respond_have_hol_false",
        path: "crates/oce-cxf/tests/fixtures/g36/trim_and_respond_have_hol_false.jsonld",
        text: G36_TRIM_AND_RESPOND,
    },
    FixtureSource {
        name: "multizone_vav_supply_temperature",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_supply_temperature.jsonld",
        text: G36_SUPPLY_TEMPERATURE,
    },
    FixtureSource {
        name: "multizone_vav_supply_fan",
        path: "crates/oce-cxf/tests/fixtures/g36/multizone_vav_supply_fan.jsonld",
        text: G36_SUPPLY_FAN,
    },
];

pub(super) const PROFILE_FIXTURES: &[FixtureSource] = &[
    FixtureSource {
        name: "small_composite",
        path: "_spec/oce_g36_gap_specs_v1/reference/fixtures/small-composite.jsonld",
        text: PROFILE_SMALL_COMPOSITE,
    },
    FixtureSource {
        name: "parameter_gated_connector",
        path: "_spec/oce_g36_gap_specs_v1/reference/fixtures/parameter-gated-connector.jsonld",
        text: PROFILE_PARAMETER_GATED,
    },
];
