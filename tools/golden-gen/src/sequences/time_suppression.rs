//! G36 Generic.TimeSuppression source-verified sequence oracle.
//!
//! This per-tick recurrence is independently derived from pinned `TimeSuppression.mo`. It models
//! the 120-second sample grid, startup mask, triggered raw-input capture, suppression timer, and the
//! deliberate one-tick `Pre` break in the latch-clear loop. The schedule makes TS1's expiry-tick
//! setpoint change observable: the active latch is still true on that tick, so no new edge occurs
//! and the captured suppression window does not restart.
//!
//! The source expression `greThr.h=0.5*dTHys` at lines 72-74 is pre-grounded to `0.125` for
//! `dTHys=0.25`. The only golden output is Boolean and is therefore compared exactly.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{
    TIME_SUPPRESSION, b, buildings_round_six, edge, greater_hysteretic, input_r, latch, pre,
    sample_due, sample_index, sampler_output, sequence_golden, timer, triggered_sampler,
    true_delay_on_init, unit_delay,
};

const SAMPLE_PERIOD: f64 = 120.0;
const CHANGE_THRESHOLD: f64 = 0.25;
const CHANGE_HYSTERESIS: f64 = 0.125;
const CHANGE_RATE: f64 = 540.0;
const MAXIMUM_SUPPRESSION_TIME: f64 = 1_800.0;
const ROW_COUNT: usize = 91;

#[derive(Clone, Copy)]
struct Row {
    setpoint_temperature: f64,
    zone_temperature: f64,
}

const fn row(setpoint_temperature: f64, zone_temperature: f64) -> Row {
    Row {
        setpoint_temperature,
        zone_temperature,
    }
}

fn schedule() -> Vec<Row> {
    (0..ROW_COUNT)
        .map(|tick| match tick {
            // TS3 + TS7: quiescent t=0. The artificial sampler-minus-zero delay value is masked,
            // and yAftSup starts true because the active-suppression latch is false.
            0 => row(294.15, 294.15),
            // TS2 + TS3: misaligned change at t=60 is invisible until the t=120 sample instant.
            1 => row(295.15, 294.15),
            // TS3 + TS6: t=120 captures |TSet-TZon|=1 K, giving an exact 540-second window.
            2..=5 => row(295.15, 294.15),
            // TS2 + TS8: an aligned t=360 change while suppression is active is swallowed.
            6..=7 => row(296.15, 294.15),
            // TS4: the t=480 delta is 0.2 K, inside the live GreaterThreshold hold band.
            8..=9 => row(296.35, 294.15),
            // TS4: no new sample delta at t=600 releases change detection through 0.125 K.
            10..=11 => row(296.35, 294.15),
            // TS1 + TS6 + TS7: a fresh sampled change collides with the t=720 expiry tick.
            // The Pre-delayed clear has not fired, so there is no new edge and no re-capture.
            12 => row(297.15, 294.15),
            // TS1: the latch clears one tick later; the swallowed change cannot restart it.
            13..=14 => row(297.15, 294.15),
            // TS2 + TS5: a misaligned t=900 setpoint change is detected at t=960; the captured
            // zone temperature makes |TSet-TZon|=0.5 K although the setpoint step is 1.3 K.
            15 => row(298.45, 297.95),
            // TS5 + TS7: the short 270-second window is still active at elapsed 240 seconds.
            16..=20 => row(298.45, 297.95),
            // TS5 + TS7: strict greater becomes true at elapsed 300 seconds (t=1260).
            21 => row(298.45, 297.95),
            22 => row(298.45, 297.95),
            // TS4: a standalone 0.2 K sub-threshold change begins off the sample grid.
            23 => row(298.65, 297.95),
            // TS4: t=1440 samples the 0.2 K delta; it never starts a suppression window.
            24..=47 => row(298.65, 297.95),
            // TS5: aligned t=2880 change captures a 4.5 K zone distance (not the 4.0 K step),
            // so min(540*4.5, 1800) exercises the maximum suppression-time cap.
            48 => row(302.65, 298.15),
            49..=77 => row(302.65, 298.15),
            // TS5 + TS6: elapsed time equals the 1,800-second cap at t=4680; strict > is false.
            78 => row(302.65, 298.15),
            // TS5 + TS7: the capped window first passes at elapsed 1,860 seconds.
            79 => row(302.65, 298.15),
            // TS7: clear occurs one tick later and the output remains true through the tail.
            80..=90 => row(302.65, 298.15),
            _ => unreachable!(),
        })
        .collect()
}

/// Build the independent Tier-A Boolean golden for Generic.TimeSuppression.
///
/// Time is in seconds and temperatures are in Kelvin. The 60-second, 91-row schedule covers
/// TS1-TS8: the Pre lag, sample-grid latency, startup masking, hysteresis, short/capped suppression
/// formulas, strict-greater equality, output polarity, and changes swallowed during an active
/// window.
pub(super) fn goldens() -> Vec<Golden> {
    let rows = schedule();
    let time: Vec<f64> = (0..rows.len()).map(|tick| tick as f64 * 60.0).collect();
    let after_suppression = suppression_trace(&time, &rows);
    let inputs = suppression_inputs(&rows);

    vec![sequence_golden(
        TIME_SUPPRESSION,
        "after_suppression",
        ValueKind::Boolean,
        time,
        after_suppression.into_iter().map(b).collect(),
        "TimeSuppression: 60-second TS1-TS8 schedule with aligned/misaligned changes, startup mask, hysteresis hold/release, short/exact/capped windows, and expiry collisions",
        "TimeSuppression.mo lines 124-193: 120-second Sampler/UnitDelay change detection drives latch/edge raw-input capture; min(540*abs(TSet-TZon),1800) is compared strictly with Timer.y, and Pre delays latch clear by one tick",
        inputs,
    )]
}

fn suppression_trace(time: &[f64], rows: &[Row]) -> Vec<bool> {
    let setpoint: Vec<f64> = rows.iter().map(|row| row.setpoint_temperature).collect();
    let zone: Vec<f64> = rows.iter().map(|row| row.zone_temperature).collect();
    let sampled_setpoint = sampler_trace(time, &setpoint);
    let previous_sample = unit_delay(time, &sampled_setpoint, SAMPLE_PERIOD, 0.0);
    let startup_input = vec![true; time.len()];
    let startup_mask = true_delay_on_init(time, &startup_input, SAMPLE_PERIOD);
    let change_magnitude: Vec<f64> = sampled_setpoint
        .iter()
        .zip(&previous_sample)
        .zip(&startup_mask)
        .map(|((&current, &previous), &mask)| {
            if mask {
                (current - previous).abs()
            } else {
                0.0
            }
        })
        .collect();
    let threshold = vec![CHANGE_THRESHOLD; time.len()];
    let change_detected =
        greater_hysteretic(&change_magnitude, &threshold, CHANGE_HYSTERESIS, false);

    // The only feedback is through Pre. Grow the prefix one tick at a time so each clear value is
    // derived exclusively from the previous tick's strict-greater result.
    let mut suppression_passed = Vec::with_capacity(time.len());
    for index in 0..time.len() {
        let mut pre_input = suppression_passed.clone();
        pre_input.push(false);
        let clear = pre(&pre_input, false);
        let active = latch(&change_detected[..=index], &clear);
        let rising = edge(&active, false);
        let captured_setpoint = triggered_sampler(&setpoint[..=index], &rising, 0.0);
        let captured_zone = triggered_sampler(&zone[..=index], &rising, 0.0);
        let suppression_window_s: Vec<f64> = captured_setpoint
            .iter()
            .zip(&captured_zone)
            .map(|(&set, &zon)| (CHANGE_RATE * (set - zon).abs()).min(MAXIMUM_SUPPRESSION_TIME))
            .collect();
        let elapsed_s = timer(&time[..=index], &active);
        suppression_passed.push(elapsed_s[index] > suppression_window_s[index]);
    }

    let clear = pre(&suppression_passed, false);
    let active = latch(&change_detected, &clear);
    let rising = edge(&active, false);
    let captured_setpoint = triggered_sampler(&setpoint, &rising, 0.0);
    let captured_zone = triggered_sampler(&zone, &rising, 0.0);
    let suppression_window_s: Vec<f64> = captured_setpoint
        .iter()
        .zip(&captured_zone)
        .map(|(&set, &zon)| (CHANGE_RATE * (set - zon).abs()).min(MAXIMUM_SUPPRESSION_TIME))
        .collect();
    let elapsed_s = timer(time, &active);
    let strict_greater: Vec<bool> = elapsed_s
        .iter()
        .zip(&suppression_window_s)
        .map(|(&elapsed, &window)| elapsed > window)
        .collect();
    assert_eq!(
        suppression_passed, strict_greater,
        "Pre-broken suppression recurrence did not converge in one forward pass"
    );
    let passed_latch = latch(&strict_greater, &rising);
    let after_suppression: Vec<bool> = active
        .iter()
        .zip(&passed_latch)
        .map(|(&is_active, &has_passed)| if is_active { has_passed } else { true })
        .collect();

    assert!(after_suppression[0], "TS7 rest polarity");
    assert!(!after_suppression[2], "TS3/TS7 active polarity");
    assert!(!after_suppression[11], "TS6 equality must not pass");
    assert!(after_suppression[12], "TS1/TS6 expiry tick must pass");
    assert!(
        !rising[12],
        "TS1 expiry collision must not create a new edge"
    );
    assert_eq!(
        captured_setpoint[12].to_bits(),
        295.15f64.to_bits(),
        "TS1 expiry collision must not re-capture the setpoint"
    );
    assert!(!after_suppression[20], "TS5 short-window drop probe");
    assert!(after_suppression[21], "TS5 short-window expiry");
    assert!(!after_suppression[78], "TS5 capped equality probe");
    assert!(after_suppression[79], "TS5 capped-window expiry");

    after_suppression
}

fn sampler_trace(time: &[f64], input: &[f64]) -> Vec<f64> {
    assert_eq!(
        time.len(),
        input.len(),
        "Sampler time/input length mismatch"
    );
    let mut initialized = false;
    let mut t0 = 0.0;
    let mut last_index = -1;
    let mut held = 0.0;
    let mut out = Vec::with_capacity(time.len());

    for (&t, &value) in time.iter().zip(input) {
        out.push(sampler_output(
            t,
            value,
            SAMPLE_PERIOD,
            initialized,
            t0,
            last_index,
            held,
        ));
        if !initialized {
            t0 = buildings_round_six((t / SAMPLE_PERIOD).floor() * SAMPLE_PERIOD);
            last_index = sample_index(t, t0, SAMPLE_PERIOD);
            held = value;
            initialized = true;
        } else {
            let (due, index) = sample_due(t, t0, SAMPLE_PERIOD, last_index);
            if due {
                last_index = index;
                held = value;
            }
        }
    }
    out
}

fn suppression_inputs(rows: &[Row]) -> Vec<InputSeries> {
    vec![
        input_r(
            "setpoint_temperature",
            rows.iter().map(|row| row.setpoint_temperature),
        ),
        input_r(
            "zone_temperature",
            rows.iter().map(|row| row.zone_temperature),
        ),
    ]
}
