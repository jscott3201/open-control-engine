//! G36 VentilationZones ASHRAE 62.1 Setpoints Tier-2 determinism golden.
//!
//! This fixture is an engine self-output snapshot, not an independent correctness oracle.

use oce_api::Value;
use oce_conformance::drive_trace_with_options;

#[allow(dead_code)]
#[path = "g36_determinism/support.rs"]
mod support;

use support::{
    PointSpec, SequenceSpec, assert_exact_comparisons_pass, assert_output_table_shape,
    assert_provenance_matches_outputs, bless_enabled, bless_sequence, captured_output_table,
    config_for, driver_reference_from_output_golden, options_for, pair, read_output_golden,
};

const SETPOINTS: &str =
    include_str!("../../oce-cxf/tests/fixtures/g36/ventilation_zones_ashrae62_1_setpoints.jsonld");

const WINDOW_STATUS: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.u1Win";
const OCCUPANCY_STATUS: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.u1Occ";
const OPERATING_MODE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.uOpeMod";
const CO2_SETPOINT: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.ppmCO2Set";
const CO2_CONCENTRATION: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.ppmCO2";
const ZONE_TEMPERATURE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.TZon";
const DISCHARGE_AIR_TEMPERATURE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.TDis";

const ADJUSTED_POPULATION_FLOW_SOURCE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.VAdjPopBreZon_flow";
const OCCUPIED_MINIMUM_FLOW_SOURCE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.VOccZonMin_flow";
const ADJUSTED_AREA_FLOW_SOURCE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.VAdjAreBreZon_flow";
const MINIMUM_OUTDOOR_AIRFLOW_SOURCE: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.VMinOA_flow";

const ADJUSTED_POPULATION_FLOW_PATH: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.modPopBreAir.y";
const OCCUPIED_MINIMUM_FLOW_PATH: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.occMinAir.y";
const ADJUSTED_AREA_FLOW_PATH: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.modAreBreAir.y";
const MINIMUM_OUTDOOR_AIRFLOW_PATH: &str =
    "http://example.org#g36.source.ventilation_zones_ashrae62_1_setpoints.minOA.y";

const ROWS: usize = 60;
const INPUTS: &[PointSpec] = &[
    PointSpec::boolean(WINDOW_STATUS),
    PointSpec::boolean(OCCUPANCY_STATUS),
    PointSpec::integer(OPERATING_MODE),
    PointSpec::real(CO2_SETPOINT),
    PointSpec::real(CO2_CONCENTRATION),
    PointSpec::real(ZONE_TEMPERATURE),
    PointSpec::real(DISCHARGE_AIR_TEMPERATURE),
];
const OUTPUTS: &[PointSpec] = &[
    PointSpec::real_alias(
        ADJUSTED_POPULATION_FLOW_SOURCE,
        ADJUSTED_POPULATION_FLOW_PATH,
    ),
    PointSpec::real_alias(OCCUPIED_MINIMUM_FLOW_SOURCE, OCCUPIED_MINIMUM_FLOW_PATH),
    PointSpec::real_alias(ADJUSTED_AREA_FLOW_SOURCE, ADJUSTED_AREA_FLOW_PATH),
    PointSpec::real_alias(MINIMUM_OUTDOOR_AIRFLOW_SOURCE, MINIMUM_OUTDOOR_AIRFLOW_PATH),
];
const SPEC: SequenceSpec = SequenceSpec {
    name: "ventilation_zones_ashrae62_1_setpoints",
    cxf: SETPOINTS,
    t_stop: 59,
    sample_step: 60.0,
    inputs: INPUTS,
    outputs: OUTPUTS,
    input_fn: setpoint_inputs,
};

#[derive(Clone, Copy)]
struct InputRow {
    window_status: bool,
    occupancy_status: bool,
    operating_mode: i64,
    co2_setpoint: f64,
    co2_concentration: f64,
    zone_temperature: f64,
}

#[test]
fn ventilation_zones_ashrae62_1_setpoints_outputs_match_determinism_golden() {
    if bless_enabled() {
        bless_sequence(&SPEC);
    }

    let golden = read_output_golden(&SPEC);
    assert_provenance_matches_outputs(&SPEC, &golden);
    let reference = driver_reference_from_output_golden(&SPEC, &golden);
    let run = drive_trace_with_options(
        SPEC.cxf.as_bytes(),
        &config_for(&SPEC),
        &reference,
        &options_for(&SPEC),
    )
    .unwrap_or_else(|err| panic!("{} driver run failed: {err}", SPEC.name));

    assert_output_table_shape(&SPEC, &golden);
    assert_eq!(
        captured_output_table(&SPEC, &run),
        golden,
        "{} captured table drifted from committed golden",
        SPEC.name
    );
    assert_exact_comparisons_pass(&SPEC, golden.n_rows, &run.comparisons);
}

fn setpoint_inputs(t: f64) -> Vec<(String, Value)> {
    let tick = (t / 60.0).round() as usize;
    assert!(tick < ROWS, "unexpected input time {t}");
    let row = input_row(tick);
    vec![
        pair(WINDOW_STATUS, Value::Boolean(row.window_status)),
        pair(OCCUPANCY_STATUS, Value::Boolean(row.occupancy_status)),
        pair(OPERATING_MODE, Value::Integer(row.operating_mode)),
        pair(CO2_SETPOINT, Value::Real(row.co2_setpoint)),
        pair(CO2_CONCENTRATION, Value::Real(row.co2_concentration)),
        pair(ZONE_TEMPERATURE, Value::Real(row.zone_temperature)),
        pair(DISCHARGE_AIR_TEMPERATURE, Value::Real(297.5)),
    ]
}

fn input_row(tick: usize) -> InputRow {
    let mut row = InputRow {
        window_status: true,
        occupancy_status: true,
        operating_mode: 1,
        co2_setpoint: 900.0,
        co2_concentration: 650.0,
        zone_temperature: 297.0,
    };

    match tick {
        0..=5 => {}
        6..=13 => {
            row.co2_concentration =
                [700.0, 750.0, 800.0, 850.0, 900.0, 950.0, 1000.0, 1050.0][tick - 6];
        }
        14..=19 => {
            row.co2_concentration = 800.0;
            row.zone_temperature = [297.6, 297.4, 297.3, 297.2, 297.45, 297.8][tick - 14];
        }
        20..=27 => {
            row.operating_mode = 2;
            row.co2_concentration = 1000.0;
            row.zone_temperature = 297.8;
        }
        28..=35 => {
            row.occupancy_status = false;
            row.co2_concentration = 1000.0;
            row.zone_temperature = 297.8;
        }
        36..=43 => {
            row.window_status = false;
            row.co2_concentration = 1000.0;
            row.zone_temperature = if tick < 40 { 297.8 } else { 297.2 };
        }
        44..=51 => {
            row.co2_setpoint = 1000.0;
            row.co2_concentration = 900.0;
        }
        52..=53 => {
            row.window_status = false;
            row.occupancy_status = false;
            row.operating_mode = 4;
            row.co2_setpoint = 1000.0;
            row.co2_concentration = 900.0;
        }
        54..=59 => {
            row.co2_setpoint = 1000.0;
            row.co2_concentration = 1050.0;
            row.zone_temperature = 297.9;
        }
        _ => unreachable!("validated tick {tick}"),
    }

    row
}
