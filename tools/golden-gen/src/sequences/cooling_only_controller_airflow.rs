//! Airflow, system-request, and alarm behavior for CoolingOnly.Controller.

use super::{
    buildings_line, clamp, greater_hysteretic, hysteresis, less_hysteretic,
};
use super::cooling_only_controller_timing::{SamplerState, TrueDelayState};

const DESIGN_MINIMUM_FLOW: f64 = 0.5;
const DESIGN_COOLING_MAXIMUM_FLOW: f64 = 1.5;
const TEMPERATURE_HYSTERESIS: f64 = 0.25;
const FLOW_HYSTERESIS: f64 = 0.01;
const DAMPER_POSITION_HYSTERESIS: f64 = 0.01;
const SAMPLE_PERIOD: f64 = 120.0;

/// Inputs consumed by the configured Dampers child.
pub(super) struct DamperInput {
    pub(super) active_minimum_flow: f64,
    pub(super) supply_air_temperature: f64,
    pub(super) zone_temperature: f64,
    pub(super) cooling_loop: f64,
    pub(super) active_cooling_maximum_flow: f64,
    pub(super) zone_state: i64,
    pub(super) airflow_override_index: i64,
    pub(super) supply_fan_status: bool,
    pub(super) discharge_airflow: f64,
    pub(super) damper_override_index: i64,
}

/// Same-tick airflow setpoint and damper command.
pub(super) struct DamperOutput {
    pub(super) airflow_setpoint: f64,
    pub(super) damper_command: f64,
}

/// Dampers hysteresis and PI-with-reset state.
#[derive(Default)]
pub(super) struct DamperState {
    supply_warm: bool,
    integrator: f64,
    previous_time: Option<f64>,
    previous_fan: bool,
}

impl DamperState {
    /// Emit both active Dampers outputs and advance the local PI state.
    pub(super) fn output(&mut self, t: f64, input: DamperInput) -> DamperOutput {
        let supply_warm = greater_hysteretic(
            &[input.supply_air_temperature],
            &[input.zone_temperature],
            TEMPERATURE_HYSTERESIS,
            self.supply_warm,
        )[0];
        self.supply_warm = supply_warm;
        let mapped = buildings_line(
            0.0,
            input.active_minimum_flow,
            1.0,
            input.active_cooling_maximum_flow,
            input.cooling_loop,
        );
        let active = if supply_warm {
            input.active_minimum_flow
        } else {
            mapped
        };
        let base = if input.zone_state == 3 {
            active
        } else {
            input.active_minimum_flow
        };
        let airflow_setpoint = match input.airflow_override_index {
            1 => 0.0,
            2 => DESIGN_COOLING_MAXIMUM_FLOW,
            3 => DESIGN_MINIMUM_FLOW,
            _ => base,
        };

        let normalized_setpoint = airflow_setpoint / DESIGN_COOLING_MAXIMUM_FLOW;
        let normalized_measurement = input.discharge_airflow / DESIGN_COOLING_MAXIMUM_FLOW;
        let error = normalized_setpoint - normalized_measurement;
        let proportional = 0.5 * error;
        let unlimited = proportional + self.integrator;
        let pid_output = clamp(unlimited, 0.0, 1.0);
        let dt = self
            .previous_time
            .map_or(0.0, |previous| (t - previous).max(0.0));
        self.integrator = if input.supply_fan_status && !self.previous_fan {
            0.01 - proportional
        } else {
            let anti_windup = (unlimited - pid_output) / (0.5 * 0.9);
            self.integrator + (0.5 / 300.0) * (error - anti_windup) * dt
        };
        self.previous_time = Some(t);
        self.previous_fan = input.supply_fan_status;

        DamperOutput {
            airflow_setpoint,
            damper_command: match input.damper_override_index {
                1 => 0.0,
                2 => 1.0,
                _ => pid_output,
            },
        }
    }
}

/// Inputs consumed by the configured SystemRequests child.
pub(super) struct SystemRequestInput {
    pub(super) after_suppression: bool,
    pub(super) cooling_setpoint: f64,
    pub(super) zone_temperature: f64,
    pub(super) cooling_loop: f64,
    pub(super) airflow_setpoint: f64,
    pub(super) discharge_airflow: f64,
    pub(super) damper_position: f64,
}

/// Temperature and pressure reset request levels.
pub(super) struct SystemRequestOutput {
    pub(super) temperature: i64,
    pub(super) pressure: i64,
}

/// SystemRequests sampler, comparator, and delay histories.
#[derive(Default)]
pub(super) struct SystemRequestsState {
    hot_three: bool,
    hot_two: bool,
    hot_three_delay: TrueDelayState,
    hot_two_delay: TrueDelayState,
    cooling_sampler: SamplerState,
    setpoint_sampler: SamplerState,
    discharge_sampler: SamplerState,
    damper_sampler: SamplerState,
    cooling_high: bool,
    setpoint_on: bool,
    damper_high: bool,
    damper_delay: TrueDelayState,
    starved_half: bool,
    starved_seventy: bool,
}

impl SystemRequestsState {
    /// Emit both request ladders from current inputs and sampled histories.
    pub(super) fn output(
        &mut self,
        t: f64,
        input: SystemRequestInput,
    ) -> SystemRequestOutput {
        let temperature_difference = input.zone_temperature - input.cooling_setpoint;
        let hot_three =
            hysteresis(&[temperature_difference], 2.75, 3.0, self.hot_three)[0];
        let hot_two = hysteresis(&[temperature_difference], 1.75, 2.0, self.hot_two)[0];
        self.hot_three = hot_three;
        self.hot_two = hot_two;
        let hot_three_held = self.hot_three_delay.output(t, hot_three, 120.0, false);
        let hot_two_held = self.hot_two_delay.output(t, hot_two, 120.0, false);

        let sampled_cooling =
            self.cooling_sampler
                .sample(t, input.cooling_loop, SAMPLE_PERIOD);
        let cooling_high =
            hysteresis(&[sampled_cooling], 0.94, 0.95, self.cooling_high)[0];
        self.cooling_high = cooling_high;
        let temperature = request_level(
            input.after_suppression && hot_three_held,
            input.after_suppression && hot_two_held,
            cooling_high,
        );

        let sampled_setpoint =
            self.setpoint_sampler
                .sample(t, input.airflow_setpoint, SAMPLE_PERIOD);
        let sampled_discharge =
            self.discharge_sampler
                .sample(t, input.discharge_airflow, SAMPLE_PERIOD);
        let sampled_damper =
            self.damper_sampler
                .sample(t, input.damper_position, SAMPLE_PERIOD);
        let setpoint_on =
            hysteresis(&[sampled_setpoint], 0.005, 0.01, self.setpoint_on)[0];
        let damper_high =
            hysteresis(&[sampled_damper], 0.94, 0.95, self.damper_high)[0];
        self.setpoint_on = setpoint_on;
        self.damper_high = damper_high;
        let damper_held = self.damper_delay.output(t, damper_high, 60.0, false);
        let starved_half = greater_hysteretic(
            &[0.5 * sampled_setpoint],
            &[sampled_discharge],
            FLOW_HYSTERESIS,
            self.starved_half,
        )[0];
        let starved_seventy = greater_hysteretic(
            &[0.7 * sampled_setpoint],
            &[sampled_discharge],
            FLOW_HYSTERESIS,
            self.starved_seventy,
        )[0];
        self.starved_half = starved_half;
        self.starved_seventy = starved_seventy;
        let gate = setpoint_on && damper_held;
        let pressure = request_level(
            gate && starved_half,
            gate && starved_seventy,
            damper_high,
        );

        SystemRequestOutput {
            temperature,
            pressure,
        }
    }
}

fn request_level(three: bool, two: bool, one: bool) -> i64 {
    if three {
        3
    } else if two {
        2
    } else {
        i64::from(one)
    }
}

/// Inputs consumed by the configured Alarms child.
pub(super) struct AlarmInput {
    pub(super) discharge_airflow: f64,
    pub(super) active_airflow_setpoint: f64,
    pub(super) supply_fan_status: bool,
    pub(super) operation_mode: i64,
    pub(super) damper_position: f64,
}

/// Three active Integer alarm levels.
pub(super) struct AlarmOutput {
    pub(super) low_airflow: i64,
    pub(super) airflow_sensor: i64,
    pub(super) leaking_damper: i64,
}

/// Alarm comparator and delay histories.
#[derive(Default)]
pub(super) struct AlarmsState {
    fan_armed: TrueDelayState,
    setpoint_nonzero: bool,
    setpoint_on: TrueDelayState,
    low_half: bool,
    low_seventy: bool,
    low_half_delay: TrueDelayState,
    low_seventy_delay: TrueDelayState,
    flow_high: bool,
    sensor_delay: TrueDelayState,
    damper_closed: bool,
    leak_delay: TrueDelayState,
}

impl AlarmsState {
    /// Emit all active alarm levels for one Controller tick.
    pub(super) fn output(&mut self, t: f64, input: AlarmInput) -> AlarmOutput {
        let occupied = input.operation_mode == 1;
        let fan_armed =
            self.fan_armed
                .output(t, input.supply_fan_status, 1800.0, false);
        let setpoint_nonzero = hysteresis(
            &[input.active_airflow_setpoint],
            0.005,
            0.01,
            self.setpoint_nonzero,
        )[0];
        self.setpoint_nonzero = setpoint_nonzero;
        let setpoint_on =
            self.setpoint_on
                .output(t, setpoint_nonzero, 300.0, false);
        let low_half = less_hysteretic(
            &[input.discharge_airflow],
            &[0.5 * input.active_airflow_setpoint],
            FLOW_HYSTERESIS,
            self.low_half,
        )[0];
        let low_seventy = greater_hysteretic(
            &[0.7 * input.active_airflow_setpoint],
            &[input.discharge_airflow],
            FLOW_HYSTERESIS,
            self.low_seventy,
        )[0];
        self.low_half = low_half;
        self.low_seventy = low_seventy;
        let low_half_held =
            self.low_half_delay
                .output(t, low_half && fan_armed, 300.0, false);
        let low_seventy_held =
            self.low_seventy_delay
                .output(t, low_seventy && fan_armed, 300.0, false);
        let level_two = low_half_held && setpoint_on && occupied;
        let level_three = low_seventy_held && setpoint_on && occupied;
        let low_airflow = if level_two {
            2
        } else if level_three {
            3
        } else {
            0
        };

        let flow_high = greater_hysteretic(
            &[input.discharge_airflow],
            &[0.1 * DESIGN_COOLING_MAXIMUM_FLOW],
            FLOW_HYSTERESIS,
            self.flow_high,
        )[0];
        self.flow_high = flow_high;
        let sensor_held = self.sensor_delay.output(
            t,
            flow_high && !input.supply_fan_status,
            600.0,
            false,
        );
        let airflow_sensor = if sensor_held && occupied { 3 } else { 0 };

        let damper_closed = less_hysteretic(
            &[input.damper_position],
            &[DAMPER_POSITION_HYSTERESIS],
            0.5 * DAMPER_POSITION_HYSTERESIS,
            self.damper_closed,
        )[0];
        self.damper_closed = damper_closed;
        let leak_held = self.leak_delay.output(
            t,
            input.supply_fan_status && damper_closed && flow_high,
            600.0,
            false,
        );

        AlarmOutput {
            low_airflow,
            airflow_sensor,
            leaking_damper: if leak_held { 4 } else { 0 },
        }
    }
}
