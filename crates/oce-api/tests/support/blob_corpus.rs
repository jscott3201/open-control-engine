//! The compiled-schedule blob corpus shared by the blob test binaries: the 46 G36 catalog fixtures
//! (the resolver-contract document `member_list_interface.jsonld` beside them is not a catalog
//! fixture) and their compilation through the facade.

#![allow(dead_code)]

use std::path::PathBuf;

use oce_api::{CompiledBlob, Engine};

/// The 46 G36 catalog fixtures, sorted by name.
pub(crate) const FIXTURES: &[&str] = &[
    "ahu_economizer",
    "ahu_supply_air_temp_reset",
    "cooling_only_active_air_flow",
    "cooling_only_alarms",
    "cooling_only_controller",
    "cooling_only_dampers",
    "cooling_only_system_requests",
    "generic_air_economizer_high_limits_ashrae_differential",
    "generic_air_economizer_high_limits_ashrae_fixed_18",
    "generic_air_economizer_high_limits_ashrae_fixed_21",
    "generic_air_economizer_high_limits_ashrae_fixed_24",
    "generic_air_economizer_high_limits_title24_differential_offset_0",
    "generic_air_economizer_high_limits_title24_differential_offset_1",
    "generic_air_economizer_high_limits_title24_differential_offset_2",
    "generic_air_economizer_high_limits_title24_differential_offset_3",
    "generic_air_economizer_high_limits_title24_fixed_21",
    "generic_air_economizer_high_limits_title24_fixed_22",
    "generic_air_economizer_high_limits_title24_fixed_23",
    "generic_air_economizer_high_limits_title24_fixed_24",
    "generic_time_suppression",
    "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
    "multizone_vav_economizer_enable",
    "multizone_vav_economizer_limits_common",
    "multizone_vav_economizer_modulations_reliefs",
    "multizone_vav_economizer_modulations_return_fan_relief_damper",
    "multizone_vav_economizer_modulations_return_fan",
    "multizone_vav_freeze_protection",
    "multizone_vav_outdoor_airflow_ahu",
    "multizone_vav_outdoor_airflow_sumzone",
    "multizone_vav_outdoor_airflow_title24_ahu",
    "multizone_vav_outdoor_airflow_title24_sumzone",
    "multizone_vav_plant_requests",
    "multizone_vav_relief_damper",
    "multizone_vav_relief_fan_group",
    "multizone_vav_relief_fan",
    "multizone_vav_return_fan_airflow_tracking",
    "multizone_vav_return_fan_direct_pressure",
    "multizone_vav_supply_fan",
    "multizone_vav_supply_signals",
    "multizone_vav_supply_temperature",
    "reheat_overrides",
    "thermal_zones_control_loops",
    "thermal_zones_zone_states",
    "trim_and_respond_have_hol_false",
    "vav_single_zone",
    "ventilation_zones_ashrae62_1_setpoints",
];

pub(crate) fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oce-cxf/tests/fixtures/g36")
}

/// The CXF bytes of one fixture.
pub(crate) fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = fixture_dir().join(format!("{name}.jsonld"));
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Every `.jsonld` document in the G36 fixture directory, by name, sorted.
pub(crate) fn fixture_names_on_disk() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .unwrap()
        .filter_map(|entry| {
            let name = entry.unwrap().file_name().into_string().unwrap();
            name.strip_suffix(".jsonld").map(str::to_owned)
        })
        .collect();
    names.sort_unstable();
    names
}

/// Load one fixture into a fresh engine.
pub(crate) fn load(name: &str) -> Engine {
    let mut engine = Engine::new();
    engine
        .load_cxf(&fixture_bytes(name))
        .unwrap_or_else(|err| panic!("{name}: load: {err}"));
    engine
}

/// Load one fixture and compile its blob.
pub(crate) fn compile(name: &str) -> CompiledBlob {
    load(name)
        .compile_blob()
        .unwrap_or_else(|err| panic!("{name}: compile_blob: {err}"))
}

/// The checked-in golden blob of one fixture.
pub(crate) fn golden(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/blob")
        .join(format!("{name}.oceb"));
    std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}
