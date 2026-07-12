//! Conditioning, ventilation, and zone-state behavior for CoolingOnly.Controller.

use super::{
    buildings_line, clamp, greater_hysteretic, hysteresis, less_hysteretic,
};
use super::cooling_only_controller_timing::{TimerState, TrueDelayState, UnitDelayState};
use super::cooling_only_controller_timing::SamplerState;

const SAMPLE_PERIOD: f64 = 120.0;
const TEMPERATURE_HYSTERESIS: f64 = 0.25;
const LOOP_THRESHOLD: f64 = 0.01;
const LOOP_HYSTERESIS: f64 = 0.008;
const CONTROL_DELAY: f64 = 30.0;
const CONTROL_GAIN: f64 = 0.1;
const CONTROL_INTEGRAL_TIME: f64 = 900.0;
const ANTI_WINDUP_NI: f64 = 0.9;
const DESIGN_MINIMUM_FLOW: f64 = 0.5;
const DESIGN_COOLING_MAXIMUM_FLOW: f64 = 1.5;
const AREA_BREATHING_FLOW: f64 = 0.006;
const POPULATION_BREATHING_FLOW: f64 = 0.005;
const COOLING_DISTRIBUTION_EFFECTIVENESS: f64 = 1.0;
const HEATING_DISTRIBUTION_EFFECTIVENESS: f64 = 0.8;

#[derive(Default)]
struct LatchState {
    held: bool,
    previous_input: bool,
}

impl LatchState {
    fn output(&mut self, input: bool, clear: bool) -> bool {
        let output = !clear && ((input && !self.previous_input) || self.held);
        self.held = output;
        self.previous_input = input;
        output
    }
}

/// TimeSuppression state composed from its two periodic blocks, two latches, and timer.
#[derive(Default)]
pub(super) struct TimeSuppressionState {
    setpoint_sampler: SamplerState,
    previous_sample: UnitDelayState,
    startup_delay: TrueDelayState,
    change_detected: bool,
    suppression_latch: LatchState,
    passed_latch: LatchState,
    timer: TimerState,
    previous_active: bool,
    previous_passed: bool,
    captured_setpoint: f64,
    captured_zone: f64,
}

impl TimeSuppressionState {
    /// Advance TimeSuppression on the Controller tick grid.
    pub(super) fn output(
        &mut self,
        t: f64,
        setpoint_temperature: f64,
        zone_temperature: f64,
    ) -> bool {
        let sampled_setpoint =
            self.setpoint_sampler
                .sample(t, setpoint_temperature, SAMPLE_PERIOD);
        let prior_sample =
            self.previous_sample
                .sample(t, sampled_setpoint, SAMPLE_PERIOD);
        let startup_complete = self.startup_delay.output(t, true, SAMPLE_PERIOD, true);
        let change_magnitude = if startup_complete {
            (sampled_setpoint - prior_sample).abs()
        } else {
            0.0
        };
        let change_detected = greater_hysteretic(
            &[change_magnitude],
            &[TEMPERATURE_HYSTERESIS],
            0.5 * TEMPERATURE_HYSTERESIS,
            self.change_detected,
        )[0];
        self.change_detected = change_detected;

        let active = self
            .suppression_latch
            .output(change_detected, self.previous_passed);
        let rising_active = active && !self.previous_active;
        if rising_active {
            self.captured_setpoint = setpoint_temperature;
            self.captured_zone = zone_temperature;
        }
        let suppression_window =
            (540.0 * (self.captured_setpoint - self.captured_zone).abs()).min(1800.0);
        let elapsed = self.timer.elapsed(t, active);
        let passed = elapsed > suppression_window;
        let passed_latched = self.passed_latch.output(passed, rising_active);
        let output = if active { passed_latched } else { true };

        self.previous_active = active;
        self.previous_passed = passed;
        output
    }
}

#[derive(Default)]
struct LoopPiState {
    integrator: f64,
    previous_time: Option<f64>,
    previous_trigger: bool,
}

impl LoopPiState {
    fn output(&mut self, t: f64, error: f64, trigger: bool) -> f64 {
        let proportional = CONTROL_GAIN * error;
        let unlimited = proportional + self.integrator;
        let limited = clamp(unlimited, 0.0, 1.0);
        let dt = self
            .previous_time
            .map_or(0.0, |previous| (t - previous).max(0.0));
        self.integrator = if trigger && !self.previous_trigger {
            -proportional
        } else {
            let anti_windup =
                (unlimited - limited) / (CONTROL_GAIN * ANTI_WINDUP_NI);
            self.integrator
                + (CONTROL_GAIN / CONTROL_INTEGRAL_TIME) * (error - anti_windup) * dt
        };
        self.previous_time = Some(t);
        self.previous_trigger = trigger;
        limited
    }
}

/// Heating and cooling loop outputs used by downstream Controller children.
pub(super) struct ControlLoopOutput {
    /// Cooling control signal.
    pub(super) cooling: f64,
    /// Heating control signal.
    pub(super) heating: f64,
}

/// ControlLoops state including both PI recurrences and both delayed zero gates.
#[derive(Default)]
pub(super) struct ControlLoopsState {
    cooling_enable: bool,
    heating_enable: bool,
    cooling_pi: LoopPiState,
    heating_pi: LoopPiState,
    cooling_near_zero: bool,
    heating_near_zero: bool,
    cooling_delay: TrueDelayState,
    heating_delay: TrueDelayState,
}

impl ControlLoopsState {
    /// Emit both same-tick loop signals and advance their independent states.
    pub(super) fn output(
        &mut self,
        t: f64,
        cooling_setpoint: f64,
        zone_temperature: f64,
        heating_setpoint: f64,
    ) -> ControlLoopOutput {
        let cooling_enable = less_hysteretic(
            &[cooling_setpoint],
            &[zone_temperature],
            TEMPERATURE_HYSTERESIS,
            self.cooling_enable,
        )[0];
        let heating_enable = less_hysteretic(
            &[zone_temperature],
            &[heating_setpoint],
            TEMPERATURE_HYSTERESIS,
            self.heating_enable,
        )[0];
        self.cooling_enable = cooling_enable;
        self.heating_enable = heating_enable;

        let cooling_raw =
            self.cooling_pi
                .output(t, zone_temperature - cooling_setpoint, cooling_enable);
        let heating_raw =
            self.heating_pi
                .output(t, heating_setpoint - zone_temperature, heating_enable);
        let cooling_near_zero = less_hysteretic(
            &[cooling_raw],
            &[LOOP_THRESHOLD],
            LOOP_HYSTERESIS,
            self.cooling_near_zero,
        )[0];
        let heating_near_zero = less_hysteretic(
            &[heating_raw],
            &[LOOP_THRESHOLD],
            LOOP_HYSTERESIS,
            self.heating_near_zero,
        )[0];
        self.cooling_near_zero = cooling_near_zero;
        self.heating_near_zero = heating_near_zero;
        let cooling_delayed =
            self.cooling_delay
                .output(t, cooling_near_zero, CONTROL_DELAY, false);
        let heating_delayed =
            self.heating_delay
                .output(t, heating_near_zero, CONTROL_DELAY, false);

        ControlLoopOutput {
            cooling: cooling_raw
                * if cooling_delayed && !cooling_enable {
                    0.0
                } else {
                    1.0
                },
            heating: heating_raw
                * if heating_delayed && !heating_enable {
                    0.0
                } else {
                    1.0
                },
        }
    }
}

/// Inputs consumed by the configured ASHRAE 62.1 Setpoints child.
pub(super) struct SetpointInput {
    pub(super) window_status: bool,
    pub(super) occupancy_status: bool,
    pub(super) operating_mode: i64,
    pub(super) co2_setpoint: f64,
    pub(super) co2_concentration: f64,
    pub(super) zone_temperature: f64,
    pub(super) discharge_air_temperature: f64,
}

/// Active Setpoints outputs used or exposed by the Controller.
pub(super) struct SetpointOutput {
    pub(super) adjusted_population_flow: f64,
    pub(super) occupied_minimum_flow: f64,
    pub(super) adjusted_area_flow: f64,
    pub(super) minimum_outdoor_airflow: f64,
}

/// The single cooling-supply hysteresis state in ASHRAE62_1.Setpoints.
#[derive(Default)]
pub(super) struct SetpointsState {
    cooling_supply: bool,
}

impl SetpointsState {
    /// Emit the four active Setpoints outputs for one Controller tick.
    pub(super) fn output(&mut self, input: SetpointInput) -> SetpointOutput {
        let cooling_supply = greater_hysteretic(
            &[input.zone_temperature],
            &[input.discharge_air_temperature],
            TEMPERATURE_HYSTERESIS,
            self.cooling_supply,
        )[0];
        self.cooling_supply = cooling_supply;

        let occupied_mode = input.operating_mode == 1;
        let override_to_zero = !input.window_status || !occupied_mode;
        let co2_loop = buildings_line(
            input.co2_setpoint - 200.0,
            0.0,
            input.co2_setpoint,
            1.0,
            input.co2_concentration,
        );
        let corrected_co2 = if occupied_mode { co2_loop } else { 0.0 };
        let occupied_minimum = buildings_line(
            0.0,
            DESIGN_MINIMUM_FLOW,
            1.0,
            DESIGN_COOLING_MAXIMUM_FLOW,
            corrected_co2,
        );
        let population = buildings_line(
            0.0,
            0.0,
            1.0,
            POPULATION_BREATHING_FLOW,
            corrected_co2,
        );

        // permit_occStandby=true makes the unoccupied multiplier exactly zero.
        let not_occupied = !input.occupancy_status;
        let population = if not_occupied { 0.0 } else { population };
        let area = if not_occupied {
            0.0
        } else {
            AREA_BREATHING_FLOW
        };
        let occupied_minimum = if not_occupied {
            0.0
        } else {
            occupied_minimum
        };
        let population = if override_to_zero { 0.0 } else { population };
        let area = if override_to_zero { 0.0 } else { area };
        let occupied_minimum = if override_to_zero {
            0.0
        } else {
            occupied_minimum
        };
        let distribution_effectiveness = if cooling_supply {
            COOLING_DISTRIBUTION_EFFECTIVENESS
        } else {
            HEATING_DISTRIBUTION_EFFECTIVENESS
        };

        SetpointOutput {
            adjusted_population_flow: population,
            occupied_minimum_flow: occupied_minimum,
            adjusted_area_flow: area,
            minimum_outdoor_airflow: (population + area) / distribution_effectiveness,
        }
    }
}

/// Stateless ActiveAirFlow outputs at the assembly configuration.
pub(super) struct ActiveAirflowOutput {
    pub(super) cooling_maximum: f64,
    pub(super) minimum: f64,
}

/// Apply the occupied/cooldown/setup gates from ActiveAirFlow.
pub(super) fn active_airflow(
    operating_mode: i64,
    occupied_minimum: f64,
) -> ActiveAirflowOutput {
    ActiveAirflowOutput {
        cooling_maximum: if matches!(operating_mode, 1..=3) {
            DESIGN_COOLING_MAXIMUM_FLOW
        } else {
            0.0
        },
        minimum: if operating_mode == 1 {
            occupied_minimum
        } else {
            0.0
        },
    }
}

/// ZoneStates hysteresis history.
#[derive(Default)]
pub(super) struct ZoneStatesState {
    heating_signal: bool,
    cooling_signal: bool,
    heating_tiebreak: bool,
}

impl ZoneStatesState {
    /// Emit the G36 ZoneStates ordinal for the current loop signals.
    pub(super) fn output(&mut self, heating: f64, cooling: f64) -> i64 {
        let heating_signal = hysteresis(&[heating], 0.01, 0.05, self.heating_signal)[0];
        let cooling_signal = hysteresis(&[cooling], 0.01, 0.05, self.cooling_signal)[0];
        let heating_tiebreak =
            hysteresis(&[heating - cooling], -0.01, 0.01, self.heating_tiebreak)[0];
        self.heating_signal = heating_signal;
        self.cooling_signal = cooling_signal;
        self.heating_tiebreak = heating_tiebreak;

        let is_heating = heating_signal && heating_tiebreak;
        let is_cooling = !is_heating && cooling_signal;
        if is_heating {
            1
        } else if is_cooling {
            3
        } else {
            2
        }
    }
}
