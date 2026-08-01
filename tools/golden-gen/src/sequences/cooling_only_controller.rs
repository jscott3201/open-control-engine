//! Independent whole-controller oracle for G36 CoolingOnly.Controller.
//!
//! The transfer function composes all eight configured child graphs directly. It never calls a
//! lane oracle or reads a committed lane golden. Source sine waves are evaluated only here during
//! generation; runtime tests replay the frozen CSV bytes.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

use super::cooling_only_controller_airflow::{
    AlarmInput, AlarmsState, DamperInput, DamperState, SystemRequestInput,
    SystemRequestsState,
};
use super::cooling_only_controller_conditioning::{
    ControlLoopsState, SetpointInput, SetpointsState, TimeSuppressionState,
    ZoneStatesState, active_airflow,
};
use super::{COOLING_ONLY_CONTROLLER, i, input_b, input_i, input_r, r, sequence_golden};

const SAMPLE_STEP: f64 = 60.0;
const STOP_TIME: f64 = 86_400.0;
const ROW_COUNT: usize = 1_441;
const INPUT_DESCRIPTION: &str =
    "Controller.Validation.mo sampled at t=0,60,...,86400: three source sines, three ramps with Round plus RealToInteger where specified, three logical pulses including inverted window status, and four constants";
const SAMPLING_RATIONALE: &str =
    "The sole upstream whole-controller Validation.mo scenario is sampled for its full 86400-second horizon on the engine 60-second tick grid, preserving 120-second sampler interactions and slow 28800-43200-second source periods; runtime tests replay committed CSV bytes and never evaluate sine";

#[derive(Clone, Copy)]
struct ControllerInput {
    zone_temperature: f64,
    cooling_setpoint: f64,
    heating_setpoint: f64,
    window_status: bool,
    occupancy_status: bool,
    operating_mode: i64,
    co2_setpoint: f64,
    co2_concentration: f64,
    discharge_air_temperature: f64,
    supply_air_temperature: f64,
    discharge_airflow: f64,
    airflow_override_index: i64,
    damper_override_index: i64,
    supply_fan_status: bool,
}

#[derive(Clone, Copy)]
struct ControllerOutput {
    airflow_setpoint: f64,
    damper_command: f64,
    adjusted_population_flow: f64,
    adjusted_area_flow: f64,
    minimum_outdoor_airflow: f64,
    zone_temperature_reset_request: i64,
    zone_pressure_reset_request: i64,
    low_airflow_alarm: i64,
    airflow_sensor_alarm: i64,
    leaking_damper_alarm: i64,
}

/// Whole-controller persistent state, one field per stateful child graph.
#[derive(Default)]
struct ControllerState {
    time_suppression: TimeSuppressionState,
    control_loops: ControlLoopsState,
    setpoints: SetpointsState,
    zone_states: ZoneStatesState,
    dampers: DamperState,
    system_requests: SystemRequestsState,
    alarms: AlarmsState,
}

impl ControllerState {
    fn output(&mut self, t: f64, input: ControllerInput) -> ControllerOutput {
        let after_suppression =
            self.time_suppression
                .output(t, input.cooling_setpoint, input.zone_temperature);
        let loops = self.control_loops.output(
            t,
            input.cooling_setpoint,
            input.zone_temperature,
            input.heating_setpoint,
        );
        let setpoints = self.setpoints.output(SetpointInput {
            window_status: input.window_status,
            occupancy_status: input.occupancy_status,
            operating_mode: input.operating_mode,
            co2_setpoint: input.co2_setpoint,
            co2_concentration: input.co2_concentration,
            zone_temperature: input.zone_temperature,
            discharge_air_temperature: input.discharge_air_temperature,
        });
        let active = active_airflow(input.operating_mode, setpoints.occupied_minimum_flow);
        let zone_state = self.zone_states.output(loops.heating, loops.cooling);
        let damper = self.dampers.output(
            t,
            DamperInput {
                active_minimum_flow: active.minimum,
                supply_air_temperature: input.supply_air_temperature,
                zone_temperature: input.zone_temperature,
                cooling_loop: loops.cooling,
                active_cooling_maximum_flow: active.cooling_maximum,
                zone_state,
                airflow_override_index: input.airflow_override_index,
                supply_fan_status: input.supply_fan_status,
                discharge_airflow: input.discharge_airflow,
                damper_override_index: input.damper_override_index,
            },
        );
        let requests = self.system_requests.output(
            t,
            SystemRequestInput {
                after_suppression,
                cooling_setpoint: input.cooling_setpoint,
                zone_temperature: input.zone_temperature,
                cooling_loop: loops.cooling,
                airflow_setpoint: damper.airflow_setpoint,
                discharge_airflow: input.discharge_airflow,
                damper_position: damper.damper_command,
            },
        );
        let alarms = self.alarms.output(
            t,
            AlarmInput {
                discharge_airflow: input.discharge_airflow,
                active_airflow_setpoint: damper.airflow_setpoint,
                supply_fan_status: input.supply_fan_status,
                operation_mode: input.operating_mode,
                damper_position: damper.damper_command,
            },
        );

        ControllerOutput {
            airflow_setpoint: damper.airflow_setpoint,
            damper_command: damper.damper_command,
            adjusted_population_flow: setpoints.adjusted_population_flow,
            adjusted_area_flow: setpoints.adjusted_area_flow,
            minimum_outdoor_airflow: setpoints.minimum_outdoor_airflow,
            zone_temperature_reset_request: requests.temperature,
            zone_pressure_reset_request: requests.pressure,
            low_airflow_alarm: alarms.low_airflow,
            airflow_sensor_alarm: alarms.airflow_sensor,
            leaking_damper_alarm: alarms.leaking_damper,
        }
    }
}

/// Build the ten Tier-A outputs over the complete upstream validation scenario.
pub(super) fn goldens() -> Vec<Golden> {
    let time = (0..ROW_COUNT)
        .map(|index| index as f64 * SAMPLE_STEP)
        .collect::<Vec<_>>();
    assert_eq!(time.first().copied(), Some(0.0));
    assert_eq!(time.last().copied(), Some(STOP_TIME));
    let scenario = time
        .iter()
        .copied()
        .map(validation_input)
        .collect::<Vec<_>>();
    assert_source_coverage(&scenario);

    let mut state = ControllerState::default();
    let outputs = time
        .iter()
        .copied()
        .zip(scenario.iter().copied())
        .map(|(t, input)| state.output(t, input))
        .collect::<Vec<_>>();
    assert_output_domain(&outputs);
    let inputs = reference_inputs(&scenario);

    vec![
        controller_golden(
            "airflow_setpoint",
            ValueKind::Real,
            time.clone(),
            outputs.iter().map(|row| r(row.airflow_setpoint)).collect(),
            "Controller.mo composes Setpoints, ActiveAirFlow, ControlLoops, ZoneStates, and Dampers; Dampers swi1 applies the airflow override ladder",
            inputs.clone(),
        ),
        controller_golden(
            "damper_command",
            ValueKind::Real,
            time.clone(),
            outputs.iter().map(|row| r(row.damper_command)).collect(),
            "Dampers normalizes same-tick airflow setpoint and measurement by 1.5, advances its local PI-with-reset recurrence, then applies damper overrides",
            inputs.clone(),
        ),
        controller_golden(
            "adjusted_population_flow",
            ValueKind::Real,
            time.clone(),
            outputs
                .iter()
                .map(|row| r(row.adjusted_population_flow))
                .collect(),
            "ASHRAE62_1.Setpoints applies occupied/window gates, CO2 interpolation, and occupied-standby suppression to the population breathing-zone component",
            inputs.clone(),
        ),
        controller_golden(
            "adjusted_area_flow",
            ValueKind::Real,
            time.clone(),
            outputs.iter().map(|row| r(row.adjusted_area_flow)).collect(),
            "ASHRAE62_1.Setpoints applies occupied/window and occupied-standby gates to the fixed 0.006 m3/s area breathing-zone component",
            inputs.clone(),
        ),
        controller_golden(
            "minimum_outdoor_airflow",
            ValueKind::Real,
            time.clone(),
            outputs
                .iter()
                .map(|row| r(row.minimum_outdoor_airflow))
                .collect(),
            "ASHRAE62_1.Setpoints sums population and area components and divides by same-tick cooling-supply-dependent distribution effectiveness",
            inputs.clone(),
        ),
        controller_golden(
            "zone_temperature_reset_request",
            ValueKind::Integer,
            time.clone(),
            outputs
                .iter()
                .map(|row| i(row.zone_temperature_reset_request))
                .collect(),
            "SystemRequests composes TimeSuppression, temperature hysteresis and delays, sampled cooling demand, and the 3/2/1 Integer ladder",
            inputs.clone(),
        ),
        controller_golden(
            "zone_pressure_reset_request",
            ValueKind::Integer,
            time.clone(),
            outputs
                .iter()
                .map(|row| i(row.zone_pressure_reset_request))
                .collect(),
            "SystemRequests samples same-tick Dampers outputs and discharge flow on the 120-second grid, then applies setpoint, damper, and starvation request gates",
            inputs.clone(),
        ),
        controller_golden(
            "low_airflow_alarm",
            ValueKind::Integer,
            time.clone(),
            outputs.iter().map(|row| i(row.low_airflow_alarm)).collect(),
            "Alarms consumes same-tick active airflow setpoint and damper output, then applies fan arming, low-flow delays, occupied mode, and level priority",
            inputs.clone(),
        ),
        controller_golden(
            "airflow_sensor_alarm",
            ValueKind::Integer,
            time.clone(),
            outputs
                .iter()
                .map(|row| i(row.airflow_sensor_alarm))
                .collect(),
            "Alarms compares discharge flow against 10 percent of the assembly 1.5 m3/s cooling maximum and delays the fan-off calibration condition",
            inputs.clone(),
        ),
        controller_golden(
            "leaking_damper_alarm",
            ValueKind::Integer,
            time,
            outputs
                .iter()
                .map(|row| i(row.leaking_damper_alarm))
                .collect(),
            "Alarms combines fan status, damper-closed hysteresis, and high-flow hysteresis before the leaking-damper delay and level-4 conversion",
            inputs,
        ),
    ]
}

fn controller_golden(
    signal: &'static str,
    kind: ValueKind,
    time: Vec<f64>,
    samples: Vec<Sample>,
    rule: &'static str,
    inputs: Vec<InputSeries>,
) -> Golden {
    sequence_golden(
        COOLING_ONLY_CONTROLLER,
        signal,
        kind,
        time,
        samples,
        INPUT_DESCRIPTION,
        rule,
        inputs,
    )
    .with_provenance(
        "validation_scenario",
        "Buildings.Controls.OBC.ASHRAE.G36.TerminalUnits.CoolingOnly.Validation.Controller",
    )
    .with_provenance("sampling_rationale", SAMPLING_RATIONALE)
}

fn validation_input(t: f64) -> ControllerInput {
    ControllerInput {
        zone_temperature: source_sin(t, 4.0, 1.0 / 86_400.0, 299.15),
        cooling_setpoint: 297.15,
        heating_setpoint: 293.15,
        window_status: !source_pulse(t, 0.05, 43_200.0, 43_200.0),
        occupancy_status: source_pulse(t, 0.75, 43_200.0, 28_800.0),
        operating_mode: rounded_integer(source_ramp(t, 2.0, 28_800.0, 1.0, 28_800.0)),
        co2_setpoint: 894.0,
        co2_concentration: source_sin(t, 400.0, 1.0 / 28_800.0, 600.0),
        discharge_air_temperature: source_ramp(t, 2.0, 43_200.0, 288.15, 28_800.0),
        supply_air_temperature: source_ramp(t, 2.0, 43_200.0, 287.15, 0.0),
        discharge_airflow: source_sin(t, 0.6, 1.0 / 28_800.0, 1.2),
        airflow_override_index: rounded_integer(source_ramp(t, 2.0, 10_000.0, 0.0, 35_000.0)),
        damper_override_index: rounded_integer(source_ramp(t, 2.0, 5_000.0, 0.0, 60_000.0)),
        supply_fan_status: source_pulse(t, 0.9, 73_200.0, 18_800.0),
    }
}

fn source_sin(t: f64, amplitude: f64, frequency: f64, offset: f64) -> f64 {
    offset
        + amplitude
            * libm::sin(
                2.0 * std::f64::consts::PI * frequency * t,
            )
}

fn source_ramp(t: f64, height: f64, duration: f64, offset: f64, start: f64) -> f64 {
    offset
        + if t < start {
            0.0
        } else if t < start + duration {
            (t - start) * height / duration
        } else {
            height
        }
}

fn source_pulse(t: f64, width: f64, period: f64, shift: f64) -> bool {
    let phase = shift - (shift / period).floor() * period;
    let mut t0 = round_six((t / period).floor() * period + phase);
    let mut t1 = t0 + width * period;
    if t + period < t1 {
        t0 -= period;
        t1 -= period;
    }
    if t >= t1 {
        t0 += period;
    } else if t < t0 {
        t1 -= period;
    }
    if t0 < t1 {
        t >= t0 && t < t1
    } else {
        !(t >= t1 && t < t0)
    }
}

fn round_six(value: f64) -> f64 {
    let factor = 1_000_000.0;
    if value > 0.0 {
        (value * factor + 0.5).floor() / factor
    } else {
        (value * factor - 0.5).ceil() / factor
    }
}

fn rounded_integer(value: f64) -> i64 {
    let rounded = if value > 0.0 {
        (value + 0.5).floor()
    } else {
        (value - 0.5).ceil()
    };
    if rounded > 0.0 {
        (rounded + 0.5).floor() as i64
    } else {
        (rounded - 0.5).ceil() as i64
    }
}

fn assert_source_coverage(rows: &[ControllerInput]) {
    assert_eq!(rows.len(), ROW_COUNT);
    assert_eq!(
        rows.iter().map(|row| row.operating_mode).collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([1, 2, 3])
    );
    assert_eq!(
        rows.iter().map(|row| row.airflow_override_index).collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([0, 1, 2])
    );
    assert_eq!(
        rows.iter().map(|row| row.damper_override_index).collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([0, 1, 2])
    );
    for values in [
        rows.iter().map(|row| row.window_status).collect::<Vec<_>>(),
        rows.iter().map(|row| row.occupancy_status).collect(),
        rows.iter().map(|row| row.supply_fan_status).collect(),
    ] {
        assert!(values.contains(&false) && values.contains(&true));
    }
}

fn assert_output_domain(rows: &[ControllerOutput]) {
    assert_eq!(rows.len(), ROW_COUNT);
    for row in rows {
        for value in [
            row.airflow_setpoint,
            row.damper_command,
            row.adjusted_population_flow,
            row.adjusted_area_flow,
            row.minimum_outdoor_airflow,
        ] {
            assert!(value.is_finite());
        }
        assert!((0..=3).contains(&row.zone_temperature_reset_request));
        assert!((0..=3).contains(&row.zone_pressure_reset_request));
        assert!([0, 2, 3].contains(&row.low_airflow_alarm));
        assert!([0, 3].contains(&row.airflow_sensor_alarm));
        assert!([0, 4].contains(&row.leaking_damper_alarm));
    }
}

fn reference_inputs(rows: &[ControllerInput]) -> Vec<InputSeries> {
    vec![
        input_r("zone_temperature", rows.iter().map(|row| row.zone_temperature)),
        input_r("cooling_setpoint", rows.iter().map(|row| row.cooling_setpoint)),
        input_r("heating_setpoint", rows.iter().map(|row| row.heating_setpoint)),
        input_b("window_status", rows.iter().map(|row| row.window_status)),
        input_b("occupancy_status", rows.iter().map(|row| row.occupancy_status)),
        input_i("operating_mode", rows.iter().map(|row| row.operating_mode)),
        input_r("co2_setpoint", rows.iter().map(|row| row.co2_setpoint)),
        input_r("co2_concentration", rows.iter().map(|row| row.co2_concentration)),
        input_r(
            "discharge_air_temperature",
            rows.iter().map(|row| row.discharge_air_temperature),
        ),
        input_r(
            "supply_air_temperature",
            rows.iter().map(|row| row.supply_air_temperature),
        ),
        input_r("discharge_airflow", rows.iter().map(|row| row.discharge_airflow)),
        input_i(
            "airflow_override_index",
            rows.iter().map(|row| row.airflow_override_index),
        ),
        input_i(
            "damper_override_index",
            rows.iter().map(|row| row.damper_override_index),
        ),
        input_b(
            "supply_fan_status",
            rows.iter().map(|row| row.supply_fan_status),
        ),
    ]
}
