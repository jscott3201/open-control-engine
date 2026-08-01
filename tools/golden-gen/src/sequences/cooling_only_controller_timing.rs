//! Timing-state helpers for the independent CoolingOnly.Controller oracle.

use super::{
    initial_sample_time, sample_due, sample_index, sampler_output, true_delay_output,
};

/// Periodic sampler state with same-tick capture at each sample instant.
#[derive(Default)]
pub(super) struct SamplerState {
    initialized: bool,
    t0: f64,
    last_index: i64,
    held: f64,
}

impl SamplerState {
    /// Emit the current held or captured sample and advance the sampler state.
    pub(super) fn sample(&mut self, t: f64, input: f64, period: f64) -> f64 {
        let output = sampler_output(
            t,
            input,
            period,
            self.initialized,
            self.t0,
            self.last_index,
            self.held,
        );
        if !self.initialized {
            self.t0 = initial_sample_time(t, period);
            self.last_index = sample_index(t, self.t0, period);
            self.held = input;
            self.initialized = true;
        } else {
            let (due, index) = sample_due(t, self.t0, period, self.last_index);
            if due {
                self.last_index = index;
                self.held = input;
            }
        }
        output
    }
}

/// One-period UnitDelay state: held output plus the most recently staged sample.
#[derive(Default)]
pub(super) struct UnitDelayState {
    initialized: bool,
    t0: f64,
    last_index: i64,
    held: f64,
    staged: f64,
}

impl UnitDelayState {
    /// Emit the prior periodic sample and stage input at a current sample instant.
    pub(super) fn sample(&mut self, t: f64, input: f64, period: f64) -> f64 {
        if !self.initialized {
            self.t0 = initial_sample_time(t, period);
            self.last_index = sample_index(t, self.t0, period);
            let sample_position = (t - self.t0) / period;
            if (sample_position - self.last_index as f64).abs() <= 1e-9 {
                self.staged = input;
            }
            self.initialized = true;
            return self.held;
        }

        let (due, index) = sample_due(t, self.t0, period, self.last_index);
        if !due {
            return self.held;
        }
        let output = self.staged;
        self.held = self.staged;
        self.staged = input;
        self.last_index = index;
        output
    }
}

/// Stateful CDL.Logical.TrueDelay recurrence.
#[derive(Default)]
pub(super) struct TrueDelayState {
    previous_time: Option<f64>,
    previous_input: bool,
    held: bool,
    timer: f64,
}

impl TrueDelayState {
    /// Emit the current delay result and advance its prior-state words.
    pub(super) fn output(
        &mut self,
        t: f64,
        input: bool,
        delay_time: f64,
        delay_on_init: bool,
    ) -> bool {
        let (output, timer) = true_delay_output(
            t,
            input,
            delay_time,
            delay_on_init,
            self.previous_time,
            self.previous_input,
            self.held,
            self.timer,
        );
        self.previous_time = Some(t);
        self.previous_input = input;
        self.held = output;
        self.timer = timer;
        output
    }
}

/// Elapsed-time state for CDL.Logical.Timer.
#[derive(Default)]
pub(super) struct TimerState {
    entry_time: Option<f64>,
    previous_input: bool,
}

impl TimerState {
    /// Emit elapsed seconds since the current true interval began.
    pub(super) fn elapsed(&mut self, t: f64, input: bool) -> f64 {
        if input && !self.previous_input {
            self.entry_time = Some(t);
        }
        let output = if input {
            self.entry_time.map_or(0.0, |entry| t - entry)
        } else {
            self.entry_time = None;
            0.0
        };
        self.previous_input = input;
        output
    }
}
