//! G36 Tier-A independent-oracle checks through the B3 facade driver.

use std::path::{Path, PathBuf};

use oce_conformance::{
    CombiTimeTable, ComparisonMode, ComparisonResult, DriveCadence, DriverInputReplay,
    DriverOptions, IndicatorPattern, PointEnd, PointMapEntry, ReferenceSpec, Tolerances, ValueKind,
    VerifyConfig, drive_trace_with_options,
};
use serde_json::Value;

const AHU_SAT_RESET: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/ahu_supply_air_temp_reset.jsonld");
const AHU_ECONOMIZER: &str = include_str!("../../oce-cxf/tests/fixtures/g36/ahu_economizer.jsonld");
const VAV_SINGLE_ZONE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/vav_single_zone.jsonld");
const SUPPLY_TEMPERATURE: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_temperature.jsonld");
const SUPPLY_FAN: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_fan.jsonld");
const SUPPLY_SIGNALS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_supply_signals.jsonld");
const PLANT_REQUESTS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_plant_requests.jsonld");
const OUTDOOR_AIRFLOW_AHU: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/multizone_vav_outdoor_airflow_ahu.jsonld");

const GOLDEN_DIR: &str = "../../tools/golden-gen/goldens/G36";

const SAT_ZONE_TEMP: &str = "http://example.org#g36.ahu_supply_air_temp_reset.zone_temp";
const SAT_COOLING_SETPOINT: &str =
    "http://example.org#g36.ahu_supply_air_temp_reset.cooling_setpoint";
const SAT_SETPOINT_RUNTIME: &str = "conn#14";
const SAT_COOLING_DEMAND_RUNTIME: &str = "conn#4";

const ECON_RETURN_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.return_air_temp";
const ECON_OUTDOOR_AIR_TEMP: &str = "http://example.org#g36.ahu_economizer.outdoor_air_temp";
const ECON_OPERATING_MODE: &str = "http://example.org#g36.ahu_economizer.operating_mode";
const ECON_ENABLED_RUNTIME: &str = "conn#20";
const ECON_DAMPER_COMMAND_RUNTIME: &str = "conn#26";
const ECON_OPERATING_MODE_REAL_RUNTIME: &str = "conn#10";
const ECON_OA_TEMP_DELTA_RUNTIME: &str = "conn#2";

const VAV_ZONE_TEMP: &str = "http://example.org#g36.vav_single_zone.zone_temp";
const VAV_COOLING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.cooling_setpoint";
const VAV_HEATING_SETPOINT: &str = "http://example.org#g36.vav_single_zone.heating_setpoint";
const VAV_DAMPER_COMMAND_RUNTIME: &str = "conn#18";
const VAV_AIRFLOW_SETPOINT_RUNTIME: &str = "conn#16";
const VAV_COOLING_SIGNAL_RUNTIME: &str = "conn#4";
const VAV_HEATING_ENABLED_RUNTIME: &str = "conn#11";
const SUPPLY_TEMPERATURE_OUTDOOR_AIR: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.TOut";
const SUPPLY_TEMPERATURE_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.u1SupFan";
const SUPPLY_TEMPERATURE_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.uOpeMod";
const SUPPLY_TEMPERATURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_temperature.uZonTemResReq";
const SUPPLY_TEMPERATURE_SETPOINT_RUNTIME: &str = "conn#123";
const SUPPLY_FAN_OPERATING_MODE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uOpeMod";
const SUPPLY_FAN_DUCT_PRESSURE: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.dpDuc";
const SUPPLY_FAN_PRESSURE_REQUESTS: &str =
    "http://example.org#g36.source.multizone_vav_supply_fan.uZonPreResReq";
const SUPPLY_FAN_STATUS_RUNTIME: &str = "conn#110";
const SUPPLY_FAN_SPEED_RUNTIME: &str = "conn#107";
const SUPPLY_SIGNALS_MEASURED_TEMP: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.TAirSup";
const SUPPLY_SIGNALS_SETPOINT: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.TAirSupSet";
const SUPPLY_SIGNALS_FAN_STATUS: &str =
    "http://example.org#g36.source.multizone_vav_supply_signals.u1SupFan";
const SUPPLY_SIGNALS_U_T_SUP_RUNTIME: &str = "conn#7";
const SUPPLY_SIGNALS_COOLING_RUNTIME: &str = "conn#18";
const SUPPLY_SIGNALS_HEATING_RUNTIME: &str = "conn#24";
const PLANT_REQUESTS_SUPPLY_AIR: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.TAirSup";
const PLANT_REQUESTS_SETPOINT: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.TAirSupSet";
const PLANT_REQUESTS_COOLING_VALVE: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.uCooCoiSet";
const PLANT_REQUESTS_HEATING_VALVE: &str =
    "http://example.org#g36.source.multizone_vav_plant_requests.uHeaCoiSet";
const PLANT_REQUESTS_CHILLED_RESET_RUNTIME: &str = "conn#17";
const PLANT_REQUESTS_CHILLER_RUNTIME: &str = "conn#42";
const PLANT_REQUESTS_HOT_RESET_RUNTIME: &str = "conn#57";
const PLANT_REQUESTS_HOT_PLANT_RUNTIME: &str = "conn#81";
const OUTDOOR_AIRFLOW_POPULATION_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VSumAdjPopBreZon_flow";
const OUTDOOR_AIRFLOW_AREA_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VSumAdjAreBreZon_flow";
const OUTDOOR_AIRFLOW_PRIMARY_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VSumZonPri_flow";
const OUTDOOR_AIRFLOW_MAX_FRACTION: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.uOutAirFra_max";
const OUTDOOR_AIRFLOW_MEASURED_FLOW: &str =
    "http://example.org#g36.source.multizone_vav_outdoor_airflow_ahu.VAirOut_flow";
const OUTDOOR_AIRFLOW_UNCORRECTED_RUNTIME: &str = "conn#6";
const OUTDOOR_AIRFLOW_EFFECTIVE_RUNTIME: &str = "conn#24";
const OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED_RUNTIME: &str = "conn#29";
const OUTDOOR_AIRFLOW_MEASURED_NORMALIZED_RUNTIME: &str = "conn#32";

const SAT_INPUTS: &[PointSpec] = &[
    PointSpec::real("zone_temp", SAT_ZONE_TEMP),
    PointSpec::real("cooling_setpoint", SAT_COOLING_SETPOINT),
];
const SAT_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real("sat_setpoint", SAT_SETPOINT_RUNTIME),
    PointSpec::real("cooling_demand", SAT_COOLING_DEMAND_RUNTIME),
];

const ECON_INPUTS: &[PointSpec] = &[
    PointSpec::real("return_air_temp", ECON_RETURN_AIR_TEMP),
    PointSpec::real("outdoor_air_temp", ECON_OUTDOOR_AIR_TEMP),
    PointSpec::integer("operating_mode", ECON_OPERATING_MODE),
];
const ECON_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real("oa_temperature_delta", ECON_OA_TEMP_DELTA_RUNTIME),
    PointSpec::real("operating_mode_real", ECON_OPERATING_MODE_REAL_RUNTIME),
    PointSpec::boolean("economizer_enabled", ECON_ENABLED_RUNTIME),
    PointSpec::real("damper_command", ECON_DAMPER_COMMAND_RUNTIME),
];

const VAV_INPUTS: &[PointSpec] = &[
    PointSpec::real("zone_temp", VAV_ZONE_TEMP),
    PointSpec::real("cooling_setpoint", VAV_COOLING_SETPOINT),
    PointSpec::real("heating_setpoint", VAV_HEATING_SETPOINT),
];
const VAV_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real("damper_command", VAV_DAMPER_COMMAND_RUNTIME),
    PointSpec::real("airflow_setpoint", VAV_AIRFLOW_SETPOINT_RUNTIME),
    PointSpec::real("cooling_signal", VAV_COOLING_SIGNAL_RUNTIME),
    PointSpec::boolean("heating_enabled", VAV_HEATING_ENABLED_RUNTIME),
];
const SUPPLY_TEMPERATURE_INPUTS: &[PointSpec] = &[
    PointSpec::real("outdoor_air_temperature", SUPPLY_TEMPERATURE_OUTDOOR_AIR),
    PointSpec::boolean("supply_fan_status", SUPPLY_TEMPERATURE_FAN_STATUS),
    PointSpec::integer("operating_mode", SUPPLY_TEMPERATURE_OPERATING_MODE),
    PointSpec::integer(
        "zone_temperature_reset_requests",
        SUPPLY_TEMPERATURE_REQUESTS,
    ),
];
const SUPPLY_TEMPERATURE_EXACT_OUTPUTS: &[PointSpec] = &[PointSpec::real(
    "supply_air_temperature_setpoint",
    SUPPLY_TEMPERATURE_SETPOINT_RUNTIME,
)];
const SUPPLY_FAN_INPUTS: &[PointSpec] = &[
    PointSpec::integer("operating_mode", SUPPLY_FAN_OPERATING_MODE),
    PointSpec::real("duct_static_pressure", SUPPLY_FAN_DUCT_PRESSURE),
    PointSpec::integer("zone_pressure_reset_requests", SUPPLY_FAN_PRESSURE_REQUESTS),
];
const SUPPLY_FAN_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::boolean("supply_fan_status", SUPPLY_FAN_STATUS_RUNTIME),
    PointSpec::real("supply_fan_speed", SUPPLY_FAN_SPEED_RUNTIME),
];
const SUPPLY_SIGNALS_INPUTS: &[PointSpec] = &[
    PointSpec::real("supply_air_temperature", SUPPLY_SIGNALS_MEASURED_TEMP),
    PointSpec::real("supply_air_temperature_setpoint", SUPPLY_SIGNALS_SETPOINT),
    PointSpec::boolean("supply_fan_status", SUPPLY_SIGNALS_FAN_STATUS),
];
const SUPPLY_SIGNALS_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real("uTSup", SUPPLY_SIGNALS_U_T_SUP_RUNTIME),
    PointSpec::real("yHeaCoi", SUPPLY_SIGNALS_HEATING_RUNTIME),
    PointSpec::real("yCooCoi", SUPPLY_SIGNALS_COOLING_RUNTIME),
];
const PLANT_REQUESTS_INPUTS: &[PointSpec] = &[
    PointSpec::real("supply_air_temperature", PLANT_REQUESTS_SUPPLY_AIR),
    PointSpec::real("supply_air_temperature_setpoint", PLANT_REQUESTS_SETPOINT),
    PointSpec::real("cooling_coil_valve", PLANT_REQUESTS_COOLING_VALVE),
    PointSpec::real("heating_coil_valve", PLANT_REQUESTS_HEATING_VALVE),
];
const PLANT_REQUESTS_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::integer(
        "chilled_water_reset_request",
        PLANT_REQUESTS_CHILLED_RESET_RUNTIME,
    ),
    PointSpec::integer("chiller_plant_request", PLANT_REQUESTS_CHILLER_RUNTIME),
    PointSpec::integer("hot_water_reset_request", PLANT_REQUESTS_HOT_RESET_RUNTIME),
    PointSpec::integer("hot_water_plant_request", PLANT_REQUESTS_HOT_PLANT_RUNTIME),
];
const OUTDOOR_AIRFLOW_INPUTS: &[PointSpec] = &[
    PointSpec::real("population_flow", OUTDOOR_AIRFLOW_POPULATION_FLOW),
    PointSpec::real("area_flow", OUTDOOR_AIRFLOW_AREA_FLOW),
    PointSpec::real("primary_flow", OUTDOOR_AIRFLOW_PRIMARY_FLOW),
    PointSpec::real("max_outdoor_air_fraction", OUTDOOR_AIRFLOW_MAX_FRACTION),
    PointSpec::real("measured_outdoor_air", OUTDOOR_AIRFLOW_MEASURED_FLOW),
];
const OUTDOOR_AIRFLOW_EXACT_OUTPUTS: &[PointSpec] = &[
    PointSpec::real(
        "uncorrected_outdoor_airflow",
        OUTDOOR_AIRFLOW_UNCORRECTED_RUNTIME,
    ),
    PointSpec::real(
        "effective_minimum_outdoor_airflow",
        OUTDOOR_AIRFLOW_EFFECTIVE_RUNTIME,
    ),
    PointSpec::real(
        "effective_outdoor_airflow_normalized",
        OUTDOOR_AIRFLOW_EFFECTIVE_NORMALIZED_RUNTIME,
    ),
    PointSpec::real(
        "measured_outdoor_airflow_normalized",
        OUTDOOR_AIRFLOW_MEASURED_NORMALIZED_RUNTIME,
    ),
];

const SEQUENCES: &[SequenceSpec] = &[
    SequenceSpec {
        name: "ahu_supply_air_temp_reset",
        cxf: AHU_SAT_RESET,
        t_stop: 4,
        sample_step: 1.0,
        inputs: SAT_INPUTS,
        exact_outputs: SAT_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "ahu_economizer",
        cxf: AHU_ECONOMIZER,
        t_stop: 5,
        sample_step: 1.0,
        inputs: ECON_INPUTS,
        exact_outputs: ECON_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "vav_single_zone",
        cxf: VAV_SINGLE_ZONE,
        t_stop: 5,
        sample_step: 1.0,
        inputs: VAV_INPUTS,
        exact_outputs: VAV_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_supply_temperature",
        cxf: SUPPLY_TEMPERATURE,
        t_stop: 900,
        sample_step: 1.0,
        inputs: SUPPLY_TEMPERATURE_INPUTS,
        exact_outputs: SUPPLY_TEMPERATURE_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_supply_fan",
        cxf: SUPPLY_FAN,
        t_stop: 900,
        sample_step: 1.0,
        inputs: SUPPLY_FAN_INPUTS,
        exact_outputs: SUPPLY_FAN_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_supply_signals",
        cxf: SUPPLY_SIGNALS,
        t_stop: 9,
        sample_step: 1.0,
        inputs: SUPPLY_SIGNALS_INPUTS,
        exact_outputs: SUPPLY_SIGNALS_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_plant_requests",
        cxf: PLANT_REQUESTS,
        t_stop: 19,
        sample_step: 60.0,
        inputs: PLANT_REQUESTS_INPUTS,
        exact_outputs: PLANT_REQUESTS_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
    SequenceSpec {
        name: "multizone_vav_outdoor_airflow_ahu",
        cxf: OUTDOOR_AIRFLOW_AHU,
        t_stop: 4,
        sample_step: 1.0,
        inputs: OUTDOOR_AIRFLOW_INPUTS,
        exact_outputs: OUTDOOR_AIRFLOW_EXACT_OUTPUTS,
        masked_outputs: &[],
    },
];

#[test]
fn g36_tier_a_sequence_oracles_match_engine_outputs() {
    for spec in SEQUENCES {
        let reference = read_reference(spec);
        assert_reference_shape(spec, &reference);
        assert_signal_provenance(spec, &reference);

        if !spec.exact_outputs.is_empty() {
            let run = drive_trace_with_options(
                spec.cxf.as_bytes(),
                &config_for(spec, spec.exact_outputs, Vec::new()),
                &reference,
                &options_for(spec, ComparisonMode::Exact),
            )
            .unwrap_or_else(|err| panic!("{} exact driver run failed: {err}", spec.name));
            assert_exact_comparisons(spec, spec.exact_outputs, reference.n_rows, &run.comparisons);
        }

        if !spec.masked_outputs.is_empty() {
            let run = drive_trace_with_options(
                spec.cxf.as_bytes(),
                &config_for(spec, spec.masked_outputs, vav_mask_indicators()),
                &reference,
                &options_for(spec, ComparisonMode::Funnel),
            )
            .unwrap_or_else(|err| panic!("{} masked driver run failed: {err}", spec.name));
            assert_masked_funnel_comparisons(spec, spec.masked_outputs, &run);
            assert_vav_heating_guard_is_non_vacuous(&run);
        }
    }
}

#[derive(Clone, Copy)]
struct PointSpec {
    reference_name: &'static str,
    cdl_name: &'static str,
    kind: ValueKind,
}

impl PointSpec {
    const fn real(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Real,
        }
    }

    const fn integer(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Integer,
        }
    }

    const fn boolean(reference_name: &'static str, cdl_name: &'static str) -> Self {
        Self {
            reference_name,
            cdl_name,
            kind: ValueKind::Boolean,
        }
    }
}

struct SequenceSpec {
    name: &'static str,
    cxf: &'static str,
    t_stop: u32,
    sample_step: f64,
    inputs: &'static [PointSpec],
    exact_outputs: &'static [PointSpec],
    masked_outputs: &'static [PointSpec],
}

fn read_reference(spec: &SequenceSpec) -> CombiTimeTable {
    CombiTimeTable::read(&reference_path(spec))
        .unwrap_or_else(|err| panic!("{} reference read failed: {err}", spec.name))
}

fn config_for(
    spec: &SequenceSpec,
    outputs: &[PointSpec],
    indicators: Vec<IndicatorPattern>,
) -> VerifyConfig {
    VerifyConfig {
        references: vec![ReferenceSpec {
            model: "g36".to_string(),
            sequence: spec.name.to_string(),
            point_name_mapping: point_mapping(spec.inputs, outputs),
        }],
        tolerances: zero_tolerances(),
        outputs: Vec::new(),
        indicators,
        sampling: Some(spec.sample_step),
        run_controller: true,
    }
}

fn point_mapping(inputs: &[PointSpec], outputs: &[PointSpec]) -> Vec<PointMapEntry> {
    inputs
        .iter()
        .chain(outputs.iter())
        .map(|point| PointMapEntry {
            cdl: point_end(point.cdl_name, point.kind),
            device: point_end(point.reference_name, point.kind),
        })
        .collect()
}

fn point_end(name: &str, kind: ValueKind) -> PointEnd {
    PointEnd {
        name: name.to_string(),
        unit: None,
        kind: Some(kind_name(kind).to_string()),
    }
}

fn options_for(spec: &SequenceSpec, comparison: ComparisonMode) -> DriverOptions {
    DriverOptions {
        cadence: DriveCadence::EventAligned {
            instants: (0..=spec.t_stop)
                .map(|tick| f64::from(tick) * spec.sample_step)
                .collect(),
        },
        input_replay: DriverInputReplay::ReferenceTable,
        comparison,
    }
}

fn vav_mask_indicators() -> Vec<IndicatorPattern> {
    vec![IndicatorPattern {
        pattern: format!("^({VAV_AIRFLOW_SETPOINT_RUNTIME}|{VAV_DAMPER_COMMAND_RUNTIME})$"),
        signals: vec![VAV_HEATING_ENABLED_RUNTIME.to_string()],
    }]
}

fn zero_tolerances() -> Tolerances {
    Tolerances {
        atolx: 0.0,
        atoly: 0.0,
        rtolx: 0.0,
        rtoly: 0.0,
        ltolx: 0.0,
        ltoly: 0.0,
    }
}

fn assert_exact_comparisons(
    spec: &SequenceSpec,
    outputs: &[PointSpec],
    expected_rows: usize,
    comparisons: &[oce_conformance::SignalComparison],
) {
    assert_eq!(
        comparisons.len(),
        outputs.len(),
        "{} exact comparison count",
        spec.name
    );
    for output in outputs {
        let comparison = find_comparison(spec, comparisons, output);
        assert!(
            !comparison.masked,
            "{} {} must be unmasked",
            spec.name, output.reference_name
        );
        assert_eq!(
            comparison.output, output.cdl_name,
            "{} runtime output for {}",
            spec.name, output.reference_name
        );
        match &comparison.result {
            ComparisonResult::Exact(result) => {
                assert!(
                    result.passed,
                    "{} exact comparison failed for {}: {:?}",
                    spec.name, output.reference_name, result.first_mismatch
                );
                assert_eq!(
                    result.compared_points, expected_rows,
                    "{} exact compared points for {}",
                    spec.name, output.reference_name
                );
                assert_eq!(
                    result.first_mismatch, None,
                    "{} exact first mismatch for {}",
                    spec.name, output.reference_name
                );
            }
            other => panic!(
                "{} {} used non-exact comparison: {other:?}",
                spec.name, output.reference_name
            ),
        }
    }
}

fn assert_masked_funnel_comparisons(
    spec: &SequenceSpec,
    outputs: &[PointSpec],
    run: &oce_conformance::DriverRun,
) {
    assert_eq!(
        run.comparisons.len(),
        outputs.len(),
        "{} masked comparison count",
        spec.name
    );
    for output in outputs {
        let comparison = find_comparison(spec, &run.comparisons, output);
        assert!(
            comparison.masked,
            "{} {} must be masked",
            spec.name, output.reference_name
        );
        match &comparison.result {
            ComparisonResult::Funnel(result) => {
                assert!(
                    result.passed,
                    "{} masked funnel failed for {} at {:?}",
                    spec.name, output.reference_name, result.first_failure_x
                );
                assert_eq!(
                    result.max_error.to_bits(),
                    0.0f64.to_bits(),
                    "{} masked funnel max error for {}",
                    spec.name,
                    output.reference_name
                );
                assert!(
                    result.compared_points > 0,
                    "{} masked funnel for {} passed vacuously",
                    spec.name,
                    output.reference_name
                );
                assert_eq!(
                    result.first_failure_x, None,
                    "{} masked funnel first failure for {}",
                    spec.name, output.reference_name
                );
            }
            other => panic!(
                "{} {} used non-funnel comparison: {other:?}",
                spec.name, output.reference_name
            ),
        }
    }
}

fn assert_vav_heating_guard_is_non_vacuous(run: &oce_conformance::DriverRun) {
    let indicator = run
        .trace
        .column(VAV_HEATING_ENABLED_RUNTIME)
        .expect("captured VAV heating indicator");
    assert_eq!(
        run.trace.times,
        vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        "VAV event-aligned mask trace times"
    );
    assert_eq!(
        indicator.values,
        vec![0.0, 0.0, 0.0, 1.0, 1.0, 0.0],
        "VAV heating_enabled must be true at t=3,4 and false around it"
    );
}

fn find_comparison<'a>(
    spec: &SequenceSpec,
    comparisons: &'a [oce_conformance::SignalComparison],
    output: &PointSpec,
) -> &'a oce_conformance::SignalComparison {
    comparisons
        .iter()
        .find(|comparison| comparison.reference_column == output.reference_name)
        .unwrap_or_else(|| {
            panic!(
                "{} missing comparison for {}",
                spec.name, output.reference_name
            )
        })
}

fn assert_reference_shape(spec: &SequenceSpec, reference: &CombiTimeTable) {
    assert_eq!(reference.name, format!("G36_{}_reference", spec.name));
    assert_eq!(reference.n_rows, spec.t_stop as usize + 1);
    assert_eq!(
        reference.col_names.as_deref(),
        Some(reference_columns(spec).as_slice()),
        "{} reference columns",
        spec.name
    );
}

fn reference_columns(spec: &SequenceSpec) -> Vec<String> {
    let mut names = Vec::new();
    names.push("time".to_string());
    names.extend(
        spec.inputs
            .iter()
            .map(|point| point.reference_name.to_string()),
    );
    names.extend(
        spec.exact_outputs
            .iter()
            .chain(spec.masked_outputs.iter())
            .map(|point| point.reference_name.to_string()),
    );
    names
}

fn assert_signal_provenance(spec: &SequenceSpec, reference: &CombiTimeTable) {
    for output in spec.exact_outputs.iter().chain(spec.masked_outputs.iter()) {
        let prov = read_json(&signal_provenance_path(spec, output.reference_name));
        assert_eq!(
            prov["class_path"], "G36",
            "{} {} class_path",
            spec.name, output.reference_name
        );
        assert_eq!(
            prov["scenario"], spec.name,
            "{} {} scenario",
            spec.name, output.reference_name
        );
        assert_eq!(
            prov["signal"], output.reference_name,
            "{} {} signal",
            spec.name, output.reference_name
        );
        assert_eq!(
            prov["tier"], "A",
            "{} {} tier",
            spec.name, output.reference_name
        );
        assert_eq!(
            prov["depends_on_oce_blocks"], false,
            "{} {} must be independent of oce-blocks",
            spec.name, output.reference_name
        );
        assert!(
            prov["source"]
                .as_str()
                .is_some_and(|source| source.contains("Buildings")),
            "{} {} source should cite Buildings semantics",
            spec.name,
            output.reference_name
        );
        assert_eq!(
            json_string_array(&prov["reference_columns"]),
            reference
                .col_names
                .as_ref()
                .expect("reference columns")
                .clone(),
            "{} {} provenance reference columns",
            spec.name,
            output.reference_name
        );
    }
}

fn json_string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("JSON string array")
        .iter()
        .map(|item| item.as_str().expect("JSON string").to_string())
        .collect()
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("read JSON {} failed: {err}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|err| panic!("parse JSON {} failed: {err}", path.display()))
}

fn reference_path(spec: &SequenceSpec) -> PathBuf {
    reference_dir(spec).join("reference.csv")
}

fn signal_provenance_path(spec: &SequenceSpec, signal: &str) -> PathBuf {
    reference_dir(spec).join(format!("{signal}.prov.json"))
}

fn reference_dir(spec: &SequenceSpec) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(GOLDEN_DIR)
        .join(spec.name)
}

fn kind_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Real => "Real",
        ValueKind::Integer => "Integer",
        ValueKind::Boolean => "Boolean",
    }
}
