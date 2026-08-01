//! G36 VentilationZones ASHRAE 62.1 Setpoints source-verified sequence oracle.
//!
//! This independent transfer function follows the pinned capstone specialization of
//! `VentilationZones/ASHRAE62_1/Setpoints.mo`. Three clamped Line blocks form the P-only CO2
//! reset paths, a priority Switch ladder applies operation/window/standby overrides, and the sole
//! stateful block is the discharge-effectiveness `Greater` hysteresis recurrence.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    ASHRAE62_1_SETPOINTS, buildings_line, clamp, greater_hysteretic, input_b, input_i, input_r, r,
    sequence_golden,
};

const V_AREA_BREATHING_ZONE: f64 = 0.006;
const V_POPULATION_BREATHING_ZONE: f64 = 0.005;
const V_MINIMUM: f64 = 0.5;
const V_COOLING_MAXIMUM: f64 = 1.5;
const COOLING_DISTRIBUTION_EFFECTIVENESS: f64 = 1.0;
const HEATING_DISTRIBUTION_EFFECTIVENESS: f64 = 0.8;
const TEMPERATURE_HYSTERESIS: f64 = 0.25;
const CO2_LOWER_KNEE_OFFSET: f64 = -200.0;
const SAMPLE_STEP: f64 = 60.0;
const ROW_COUNT: usize = 60;

#[derive(Clone, Copy)]
struct Row {
    window_status: bool,
    occupancy_status: bool,
    operating_mode: i64,
    co2_setpoint: f64,
    co2_concentration: f64,
    zone_temperature: f64,
    discharge_air_temperature: f64,
}

struct Trace {
    co2_loop: Vec<f64>,
    cooling_supply: Vec<bool>,
    adjusted_population_flow: Vec<f64>,
    occupied_minimum_flow: Vec<f64>,
    adjusted_area_flow: Vec<f64>,
    minimum_outdoor_airflow: Vec<f64>,
}

/// Build independent Tier-A goldens for all four ASHRAE 62.1 zone setpoint outputs.
///
/// Time is seconds on a 60-second grid, temperatures are kelvin, CO2 values are ppm, and flows
/// are cubic metres per second. The 60-row schedule covers both Line clamps and every knot,
/// setpoint-dependent lower-knee movement, occupied-mode CO2 gating, occupied-standby and window
/// override priorities, compound overrides, and the Greater set/hold/release/re-entry sequence.
pub(super) fn goldens() -> Vec<Golden> {
    let rows = schedule();
    let time = (0..ROW_COUNT)
        .map(|tick| tick as f64 * SAMPLE_STEP)
        .collect::<Vec<_>>();
    let trace = setpoint_trace(&rows);

    assert!(trace.co2_loop[0..7].iter().all(|value| *value == 0.0));
    assert_eq!(trace.co2_loop[8].to_bits(), 0.5f64.to_bits());
    assert!(trace.co2_loop[10..14].iter().all(|value| *value == 1.0));
    assert_eq!(
        &trace.cooling_supply[14..20],
        &[true, true, true, false, false, true]
    );
    for output in [
        &trace.adjusted_population_flow,
        &trace.occupied_minimum_flow,
        &trace.adjusted_area_flow,
        &trace.minimum_outdoor_airflow,
    ] {
        assert!(output[20..44].iter().all(|value| *value == 0.0));
        assert!(output[52..54].iter().all(|value| *value == 0.0));
    }
    assert_eq!(trace.adjusted_population_flow[44].to_bits(), 0.0025f64.to_bits());
    assert_eq!(trace.occupied_minimum_flow[44].to_bits(), 1.0f64.to_bits());
    assert_eq!(trace.adjusted_population_flow[54].to_bits(), 0.005f64.to_bits());
    assert_eq!(trace.occupied_minimum_flow[54].to_bits(), 1.5f64.to_bits());
    assert_eq!(trace.adjusted_area_flow[54].to_bits(), 0.006f64.to_bits());
    assert_eq!(trace.minimum_outdoor_airflow[54].to_bits(), 0.011f64.to_bits());

    let inputs = inputs(&rows);
    vec![
        sequence_golden(
            ASHRAE62_1_SETPOINTS,
            "adjusted_population_flow",
            ValueKind::Real,
            time.clone(),
            trace
                .adjusted_population_flow
                .into_iter()
                .map(r)
                .collect(),
            "ASHRAE62_1 Setpoints adjusted population flow: both CO2 clamps, moving lower knee, occupied-mode gate, standby/window priorities, compound override, and recovery",
            "Setpoints.mo lines 142-178 and 290-447: lin -> occupied-gated co2Con -> popBreOutAir, then unpPopBreAir and priority modPopBreAir Switches",
            inputs.clone(),
        ),
        sequence_golden(
            ASHRAE62_1_SETPOINTS,
            "occupied_minimum_flow",
            ValueKind::Real,
            time.clone(),
            trace.occupied_minimum_flow.into_iter().map(r).collect(),
            "ASHRAE62_1 Setpoints occupied minimum flow: VMin-to-VCooMax CO2 reset through active gai2 plus standby/window/operation overrides",
            "Setpoints.mo lines 165-175, 210-235, and 304-427: occMinAirSet f2 is driven only through gai2(k=1), followed by unpMinZonAir and occMinAir priority Switches",
            inputs.clone(),
        ),
        sequence_golden(
            ASHRAE62_1_SETPOINTS,
            "adjusted_area_flow",
            ValueKind::Real,
            time.clone(),
            trace.adjusted_area_flow.into_iter().map(r).collect(),
            "ASHRAE62_1 Setpoints adjusted area flow: design-area pass-through and distinct standby/window/operation zero paths",
            "Setpoints.mo lines 207-232 and 342-447: permit_occStandby drives a typed-zero multiply into unpAreBreAir before the priority modAreBreAir Switch",
            inputs.clone(),
        ),
        sequence_golden(
            ASHRAE62_1_SETPOINTS,
            "minimum_outdoor_airflow",
            ValueKind::Real,
            time,
            trace.minimum_outdoor_airflow.into_iter().map(r).collect(),
            "ASHRAE62_1 Setpoints minimum outdoor airflow: population-plus-area numerator divided by stateful cooling/heating distribution effectiveness",
            "Setpoints.mo lines 137-141, 248-252, and 400-435: reqBreAir=modPopBreAir+modAreBreAir; cooSup Greater(h=0.25) selects effectiveness 1.0 or 0.8; minOA divides",
            inputs,
        ),
    ]
}

fn schedule() -> Vec<Row> {
    let baseline = Row {
        window_status: true,
        occupancy_status: true,
        operating_mode: 1,
        co2_setpoint: 900.0,
        co2_concentration: 650.0,
        zone_temperature: 297.0,
        discharge_air_temperature: 297.5,
    };
    let mut rows = vec![baseline; ROW_COUNT];

    for (row, concentration) in rows[6..14]
        .iter_mut()
        .zip([700.0, 750.0, 800.0, 850.0, 900.0, 950.0, 1000.0, 1050.0])
    {
        row.co2_concentration = concentration;
    }
    for (row, temperature) in rows[14..20]
        .iter_mut()
        .zip([297.6, 297.4, 297.3, 297.2, 297.45, 297.8])
    {
        row.co2_concentration = 800.0;
        row.zone_temperature = temperature;
    }
    for row in &mut rows[20..28] {
        row.operating_mode = 2;
        row.co2_concentration = 1000.0;
        row.zone_temperature = 297.8;
    }
    for row in &mut rows[28..36] {
        row.occupancy_status = false;
        row.co2_concentration = 1000.0;
        row.zone_temperature = 297.8;
    }
    for (index, row) in rows[36..44].iter_mut().enumerate() {
        row.window_status = false;
        row.co2_concentration = 1000.0;
        row.zone_temperature = if index < 4 { 297.8 } else { 297.2 };
    }
    for row in &mut rows[44..52] {
        row.co2_setpoint = 1000.0;
        row.co2_concentration = 900.0;
        row.zone_temperature = 297.0;
    }
    for row in &mut rows[52..54] {
        row.window_status = false;
        row.occupancy_status = false;
        row.operating_mode = 4;
        row.co2_setpoint = 1000.0;
        row.co2_concentration = 900.0;
        row.zone_temperature = 297.0;
    }
    for row in &mut rows[54..60] {
        row.co2_setpoint = 1000.0;
        row.co2_concentration = 1050.0;
        row.zone_temperature = 297.9;
    }

    rows
}

fn setpoint_trace(rows: &[Row]) -> Trace {
    let zone_temperatures = rows
        .iter()
        .map(|row| row.zone_temperature)
        .collect::<Vec<_>>();
    let discharge_temperatures = rows
        .iter()
        .map(|row| row.discharge_air_temperature)
        .collect::<Vec<_>>();
    let cooling_supply = greater_hysteretic(
        &zone_temperatures,
        &discharge_temperatures,
        TEMPERATURE_HYSTERESIS,
        false,
    );

    let mut co2_loop = Vec::with_capacity(rows.len());
    let mut adjusted_population_flow = Vec::with_capacity(rows.len());
    let mut occupied_minimum_flow = Vec::with_capacity(rows.len());
    let mut adjusted_area_flow = Vec::with_capacity(rows.len());
    let mut minimum_outdoor_airflow = Vec::with_capacity(rows.len());

    for (row, &cooling) in rows.iter().zip(&cooling_supply) {
        let window_open = !row.window_status;
        let occupied_mode = row.operating_mode == 1;
        let override_to_zero = window_open || !occupied_mode;
        let lower_knee = row.co2_setpoint + CO2_LOWER_KNEE_OFFSET;
        let loop_output = buildings_line(
            lower_knee,
            0.0,
            row.co2_setpoint,
            1.0,
            row.co2_concentration,
        );
        assert_eq!(
            clamp(loop_output, 0.0, 1.0).to_bits(),
            loop_output.to_bits(),
            "clamped Line output must remain in [0,1]"
        );
        co2_loop.push(loop_output);

        let occupied_indicator = if occupied_mode { 1.0 } else { 0.0 };
        let corrected_co2 = occupied_indicator * loop_output;
        let gained_cooling_maximum = 1.0 * V_COOLING_MAXIMUM;
        let occupied_minimum_setpoint = buildings_line(
            0.0,
            V_MINIMUM,
            1.0,
            gained_cooling_maximum,
            corrected_co2,
        );
        let population_breathing_air = buildings_line(
            0.0,
            0.0,
            1.0,
            V_POPULATION_BREATHING_ZONE,
            corrected_co2,
        );

        let not_occupied = !row.occupancy_status;
        let standby_factor = 0.0;
        let unpopulated_area_breathing_air = standby_factor * V_AREA_BREATHING_ZONE;
        let unpopulated_minimum_flow = standby_factor * V_MINIMUM;
        let population_after_standby = if not_occupied {
            0.0
        } else {
            population_breathing_air
        };
        let area_after_standby = if not_occupied {
            unpopulated_area_breathing_air
        } else {
            V_AREA_BREATHING_ZONE
        };
        let minimum_after_standby = if not_occupied {
            unpopulated_minimum_flow
        } else {
            occupied_minimum_setpoint
        };
        let population = if override_to_zero {
            0.0
        } else {
            population_after_standby
        };
        let area = if override_to_zero {
            0.0
        } else {
            area_after_standby
        };
        let occupied_minimum = if override_to_zero {
            0.0
        } else {
            minimum_after_standby
        };
        let distribution_effectiveness = if cooling {
            COOLING_DISTRIBUTION_EFFECTIVENESS
        } else {
            HEATING_DISTRIBUTION_EFFECTIVENESS
        };
        assert!(
            distribution_effectiveness.to_bits()
                == COOLING_DISTRIBUTION_EFFECTIVENESS.to_bits()
                || distribution_effectiveness.to_bits()
                    == HEATING_DISTRIBUTION_EFFECTIVENESS.to_bits()
        );
        let required_breathing_air = population + area;
        let minimum_outdoor = required_breathing_air / distribution_effectiveness;

        adjusted_population_flow.push(population);
        occupied_minimum_flow.push(occupied_minimum);
        adjusted_area_flow.push(area);
        minimum_outdoor_airflow.push(minimum_outdoor);
    }

    Trace {
        co2_loop,
        cooling_supply,
        adjusted_population_flow,
        occupied_minimum_flow,
        adjusted_area_flow,
        minimum_outdoor_airflow,
    }
}

fn inputs(rows: &[Row]) -> Vec<InputSeries> {
    vec![
        input_b("window_status", rows.iter().map(|row| row.window_status)),
        input_b(
            "occupancy_status",
            rows.iter().map(|row| row.occupancy_status),
        ),
        input_i(
            "operating_mode",
            rows.iter().map(|row| row.operating_mode),
        ),
        input_r("co2_setpoint", rows.iter().map(|row| row.co2_setpoint)),
        input_r(
            "co2_concentration",
            rows.iter().map(|row| row.co2_concentration),
        ),
        input_r(
            "zone_temperature",
            rows.iter().map(|row| row.zone_temperature),
        ),
        input_r(
            "discharge_air_temperature",
            rows.iter().map(|row| row.discharge_air_temperature),
        ),
    ]
}
