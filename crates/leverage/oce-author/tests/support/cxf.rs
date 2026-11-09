//! Engine-side helpers for the emitter and importer tests: the corpus partition gate item 1
//! runs over, load, export and the identity it compares.

use oce_api::{Engine, LoadReport};

/// The 31 G36 corpus fixtures inside the M2 authoring subset that the importer reads, all of
/// which round-trip (gate item 1; importer note, "Where the corpus stands"). Sorted.
pub const IMPORTED: &[&str] = &[
    "cooling_only_active_air_flow",
    "cooling_only_alarms",
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
    "multizone_vav_economizer_enable",
    "multizone_vav_economizer_limits_common",
    "multizone_vav_economizer_modulations_reliefs",
    "multizone_vav_economizer_modulations_return_fan",
    "multizone_vav_freeze_protection",
    "multizone_vav_outdoor_airflow_sumzone",
    "multizone_vav_relief_damper",
    "multizone_vav_relief_fan_group",
    "multizone_vav_return_fan_airflow_tracking",
    "multizone_vav_return_fan_direct_pressure",
    "reheat_overrides",
    "thermal_zones_control_loops",
    "thermal_zones_zone_states",
    "ventilation_zones_ashrae62_1_setpoints",
];

/// The 15 other corpus fixtures, each with the [`oce_author::ImportErrorKind::code`] it is
/// refused with under the fixed precedence (nested composite, conditional component, expression
/// value, port attribute, name): 4 nested composites, 7 conditional components, 1 expression
/// value (12 outside the M2 subset) and the 3 hand-written fixtures with block-port bounds
/// (decision 8). Sorted.
pub const REFUSED: &[(&str, &str)] = &[
    ("ahu_economizer", "port-attribute"),
    ("ahu_supply_air_temp_reset", "port-attribute"),
    ("cooling_only_controller", "nested-composite"),
    (
        "multizone_vav_economizer_controller_single_damper_relief_damper_fixed_21",
        "nested-composite",
    ),
    (
        "multizone_vav_economizer_modulations_return_fan_relief_damper",
        "conditional-component",
    ),
    ("multizone_vav_outdoor_airflow_ahu", "conditional-component"),
    (
        "multizone_vav_outdoor_airflow_title24_ahu",
        "conditional-component",
    ),
    (
        "multizone_vav_outdoor_airflow_title24_sumzone",
        "conditional-component",
    ),
    ("multizone_vav_plant_requests", "conditional-component"),
    ("multizone_vav_relief_fan", "expression-value"),
    ("multizone_vav_supply_fan", "nested-composite"),
    ("multizone_vav_supply_signals", "conditional-component"),
    ("multizone_vav_supply_temperature", "nested-composite"),
    ("trim_and_respond_have_hol_false", "conditional-component"),
    ("vav_single_zone", "port-attribute"),
];

/// The path of a committed G36 corpus fixture.
pub fn corpus_fixture(name: &str) -> std::path::PathBuf {
    super::crate_dir()
        .join("../../oce-cxf/tests/fixtures/g36")
        .join(format!("{name}.jsonld"))
}

/// Load `bytes` in a fresh engine, or the refusal's display.
pub fn load(bytes: &[u8]) -> Result<(Engine, LoadReport), String> {
    let mut engine = Engine::new();
    let (report, _) = engine.load_cxf(bytes).map_err(|e| e.to_string())?;
    Ok((engine, report))
}

/// Load `bytes` and require a warning-free import (acceptance precondition 4).
pub fn load_clean(bytes: &[u8]) -> Engine {
    let (engine, report) = load(bytes).unwrap_or_else(|e| panic!("load refused: {e}"));
    let codes: Vec<&str> = report.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(codes, Vec::<&str>::new(), "the import warned");
    engine
}

/// The exported CXF bytes of a loaded engine, as text.
pub fn export_text(engine: &Engine) -> String {
    let export = engine.export_cxf().expect("export");
    String::from_utf8(export.bytes).expect("UTF-8 export")
}

/// The identity of a loaded model that gate item 1 compares: the canonical content id
/// (`Engine::content_id`, GitHub #470), defined whether or not the export is partial and
/// independent of authored order and instance namespace. Every comparison goes through this one
/// function.
pub fn identity(engine: &Engine) -> String {
    engine.content_id().expect("content id").to_string()
}

/// The loaded executable as compiled-schedule blob bytes with the content-id field (header bytes
/// 20..52, format 2) and the CRC-32C trailer that covers it zeroed, so two executables compare
/// without their content id. Like [`identity`], the one place the id's position is known.
pub fn executable(engine: &Engine) -> Vec<u8> {
    let blob = engine.compile_blob().expect("blob");
    let mut bytes = blob.bytes;
    assert!(
        blob.content_id.is_some(),
        "a loaded model's blob records its id"
    );
    bytes[20..52].fill(0);
    let len = bytes.len();
    bytes[len - 4..].fill(0);
    bytes
}
