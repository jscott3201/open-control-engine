//! Discrete + Sources deterministic semantics: CDL.Discrete.UnitDelay, CDL.Reals.Sources.Constant,
//! CDL.Logical.Sources.SampleTrigger (`_spec/03` §4.1/§4.3/§4.6; `_spec/01` §11.1 sample clock).
//!
//! Conformance drives the EventAligned cadence so a tick lands on every sample instant
//! start+k*period (`_spec/01` §11.1 req 2 / `_spec/07` §5.4); the snap-to-latest coarse-tick path
//! is never exercised. Derived solely from the spec — never from `oce-blocks`.
//!
//! NOTE: CDL.Constants and CDL.Types are NOT steppable blocks (`_spec/03` §4.9/§4.10): they are
//! fold-time literal/ordinal references with no tick trace, so they are emitted as standalone
//! provenance entries (see `constants_types`), not CombiTimeTable CSVs.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

fn r(x: f64) -> Sample {
    Sample::Real(x)
}
fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

fn input_b(name: &'static str, values: impl IntoIterator<Item = bool>) -> InputSeries {
    InputSeries::new(name, ValueKind::Boolean, values.into_iter().map(b).collect())
}

/// Build the steppable Discrete + Sources goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    // UnitDelay: y(k) = u(k-1); y(0) = y_start. EventAligned ticks at t=0..4, period=1.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let u = [0.1, 0.2, 0.3, 0.4, 0.5];
        let y_start = 0.0;
        // emit-from-state: at instant m emit prior held value, then store u(m).
        let mut held = y_start;
        let mut y = Vec::new();
        for &cur in &u {
            y.push(r(held));
            held = cur;
        }
        out.push(Golden::new(
            "CDL.Discrete.UnitDelay",
            "y",
            ValueKind::Real,
            t,
            y,
            "samplePeriod=1.0, start=0.0, y_start=0.0; u=[0.1,0.2,0.3,0.4,0.5] (non-dyadic bit-carry)",
            "y(k) = u(k-1), y(0)=y_start (one-sample delay, loop cut, emit-from-state); _spec/03 §4.6 UnitDelay",
        )
        .with_inputs(vec![input_r("u", u)]));
    }

    // UnitDelay scenario variant: y_start = 2.5.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let u = [0.1, 0.2, 0.3, 0.4, 0.5];
        let mut held = 2.5_f64;
        let mut y = Vec::new();
        for &cur in &u {
            y.push(r(held));
            held = cur;
        }
        out.push(
            Golden::new(
                "CDL.Discrete.UnitDelay",
                "y",
                ValueKind::Real,
                t,
                y,
                "samplePeriod=1.0, start=0.0, y_start=2.5; u=[0.1..0.5] non-dyadic",
                "y(k)=u(k-1), y(0)=y_start (one-sample delay, emit-from-state); _spec/03 §4.6 UnitDelay",
            )
            .with_scenario("y_start_nonzero")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // UnitDelay scenario variant: ticks BETWEEN sample instants (samplePeriod=2, tick step 1).
    // Upstream `when sampleTrigger then u_internal = u; y = pre(u_internal)` holds y constant
    // across the whole interval: the inter-sample ticks at t=1 and t=3 must re-emit the value
    // from the previous instant, NOT the sample staged at the most recent one. This is the
    // oracle-diff scenario for the 2026-07-06 closeout divergence fix ("before the second sample
    // instant, the output y is identical to parameter y_start" — Buildings UnitDelay.mo doc).
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        let u = [0.1, 0.2, 0.3, 0.4, 0.5];
        let y_start = 2.5_f64;
        let mut y_held = y_start; // upstream y between instants
        let mut u_internal = y_start; // upstream staged sample
        let mut y = Vec::new();
        for (k, &cur) in u.iter().enumerate() {
            let instant = k % 2 == 0; // ticks at t=0,2,4 land on instants
            if instant {
                // `when sampleTrigger`: y = pre(u_internal), then u_internal = u.
                y.push(r(u_internal));
                y_held = u_internal;
                u_internal = cur;
            } else {
                y.push(r(y_held));
            }
        }
        out.push(
            Golden::new(
                "CDL.Discrete.UnitDelay",
                "y",
                ValueKind::Real,
                t,
                y,
                "samplePeriod=2.0, tick step 1.0, y_start=2.5; u=[0.1..0.5]; inter-sample ticks hold the previous instant's value",
                "when sampleTrigger: u_internal=u, y=pre(u_internal); y holds between instants; Buildings Discrete/UnitDelay.mo",
            )
            .with_scenario("inter_sample_ticks")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // UnitDelay scenario variant: first tick BETWEEN instants (t=0.5, samplePeriod=1, t0=0).
    // Upstream fires on `when sampleTrigger` with NO initial() clause, so the mid-interval start
    // must NOT stage u(0.5): y and u_internal hold y_start until the first true instant. The
    // poison first input 9.9 must never surface in y. (PR #145 review-confirmed fix.)
    {
        let t = vec![0.5, 1.0, 1.5, 2.0, 3.0];
        let u = [9.9, 0.2, 0.3, 0.4, 0.5];
        let instants = [false, true, false, true, true]; // t=1,2,3 are true sample instants
        let y_start = 2.5_f64;
        let mut y_held = y_start;
        let mut u_internal = y_start;
        let mut y = Vec::new();
        for (k, &cur) in u.iter().enumerate() {
            if instants[k] {
                y.push(r(u_internal));
                y_held = u_internal;
                u_internal = cur;
            } else {
                y.push(r(y_held));
            }
        }
        out.push(
            Golden::new(
                "CDL.Discrete.UnitDelay",
                "y",
                ValueKind::Real,
                t,
                y,
                "samplePeriod=1.0, first tick at t=0.5 (mid-interval), y_start=2.5; u=[9.9,0.2,0.3,0.4,0.5]; the unaligned first input is never staged",
                "when sampleTrigger (NO initial()): a mid-interval start holds y=y_start and stages nothing until the first true instant; Buildings Discrete/UnitDelay.mo",
            )
            .with_scenario("unaligned_first_tick")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // Sampler: initial() samples at simulation start; periodic t0 supports negative start time.
    {
        let t = vec![-0.25, 0.0, 0.5, 1.0, 1.5];
        let u = [10.0, 20.0, 30.0, 40.0, 50.0];
        let y = sampled_sampler_y(&t, 1.0, &u);
        out.push(
            Golden::new(
                "CDL.Discrete.Sampler",
                "y",
                ValueKind::Real,
                t,
                y,
                "samplePeriod=1.0, start=-0.25; u=[10,20,30,40,50]",
                "t0=round(integer(time/samplePeriod)*samplePeriod,n=6); when {sampleTrigger,initial()} then y=u; held between periodic sample instants; Buildings Sampler.mo",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // ZeroOrderHold: initial input feeds through, then y=pre(ySample) at sample instants.
    {
        let t = vec![-0.25, 0.0, 0.5, 1.0, 1.5];
        let u = [10.0, 20.0, 30.0, 40.0, 50.0];
        let y = zero_order_hold_y(&t, 1.0, &u);
        out.push(
            Golden::new(
                "CDL.Discrete.ZeroOrderHold",
                "y",
                ValueKind::Real,
                t,
                y,
                "samplePeriod=1.0, start=-0.25; u=[10,20,30,40,50]",
                "initial time feeds u directly to y; later y=pre(ySample), so a sample instant emits the previously sampled value and then stores current u; Buildings ZeroOrderHold.mo",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // FirstOrderHold: first sample has zero slope; later intervals extrapolate from two samples.
    {
        let t = vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5];
        let u = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let y = first_order_hold_y(&t, 1.0, &u);
        out.push(
            Golden::new(
                "CDL.Discrete.FirstOrderHold",
                "y",
                ValueKind::Real,
                t,
                y,
                "samplePeriod=1.0, start=0.0; u=[0,1,2,3,4,5]",
                "pre(tSample)=t0, pre(uSample)=u, pre(pre_uSample)=u, pre(c)=0; on sample instants y emits the previous sample, then c=(uSample-pre_uSample)/samplePeriod for the following interval; Buildings FirstOrderHold.mo",
            )
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // FirstOrderHold scenario: negative simulation start derives t0 by Modelica integer floor.
    {
        let t = vec![-0.25, 0.0, 0.5];
        let u = [10.0, 12.0, 14.0];
        let y = first_order_hold_y(&t, 1.0, &u);
        out.push(
            Golden::new(
                "CDL.Discrete.FirstOrderHold",
                "y",
                ValueKind::Real,
                t,
                y,
                "scenario=negative_start; samplePeriod=1.0, start=-0.25; u=[10,12,14]",
                "t0=round(integer(-0.25/1.0)*1.0,n=6)=-1.0 using Modelica integer floor; the t=0 sample emits previous u=10, then the following interval uses slope (12-10)/1",
            )
            .with_scenario("negative_start")
            .with_inputs(vec![input_r("u", u)]),
        );
    }

    // TriggeredMax: initial y = u, then y=max(pre(y),abs(u)) on false->true trigger edges.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let u = [-2.0, -3.0, 4.0, 5.0, -6.0, 7.0];
        let trigger = [false, true, true, false, true, false];
        let y = triggered_max_y(&u, &trigger);
        out.push(
            Golden::new(
                "CDL.Discrete.TriggeredMax",
                "y",
                ValueKind::Real,
                t,
                y,
                "u=[-2,-3,4,5,-6,7]; trigger=[false,true,true,false,true,false]",
                "initial equation y=u; on false->true trigger edges y=max(pre(y),abs(u)); held true does not resample; Buildings TriggeredMax.mo",
            )
            .with_inputs(vec![input_r("u", u), input_b("trigger", trigger)]),
        );
    }

    // TriggeredMax scenario: an initially true trigger applies max(initial y, abs(u)).
    {
        let t = vec![0.0, 1.0, 2.0, 3.0];
        let u = [-2.0, -10.0, 8.0, 7.0];
        let trigger = [true, true, false, true];
        let y = triggered_max_y(&u, &trigger);
        out.push(
            Golden::new(
                "CDL.Discrete.TriggeredMax",
                "y",
                ValueKind::Real,
                t,
                y,
                "scenario=trigger_initially_true; u=[-2,-10,8,7]; trigger=[true,true,false,true]",
                "pre(trigger) is false at initialization; initial y=u is then used as pre(y) for the initial rising-trigger event, so negative u becomes abs(u)",
            )
            .with_scenario("trigger_initially_true")
            .with_inputs(vec![input_r("u", u), input_b("trigger", trigger)]),
        );
    }

    // TriggeredMovingMean: initial() samples once, then false->true trigger edges update a ring.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let u = [0.1, 0.2, 99.0, 0.4, 0.7, -3.0, 1.1];
        let trigger = [false, true, true, false, true, false, true];
        let y = triggered_moving_mean_y(3, &u, &trigger);
        out.push(
            Golden::new(
                "CDL.Discrete.TriggeredMovingMean",
                "y",
                ValueKind::Real,
                t,
                y,
                "n=3; u=[0.1,0.2,99,0.4,0.7,-3,1.1]; trigger=[false,true,true,false,true,false,true]",
                "when {initial(),trigger}: initial sample once, then false->true edges; update ring slot mod(pre(iSample),n)+1, sum full ySample vector in index order, divide by saturated counter; Buildings TriggeredMovingMean.mo",
            )
            .with_inputs(vec![input_r("u", u), input_b("trigger", trigger)]),
        );
    }

    // TriggeredMovingMean scenario: initially true trigger is still one initial sample, not two.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0];
        let u = [0.9, 3.0, 6.0, 1.2];
        let trigger = [true, true, false, true];
        let y = triggered_moving_mean_y(3, &u, &trigger);
        out.push(
            Golden::new(
                "CDL.Discrete.TriggeredMovingMean",
                "y",
                ValueKind::Real,
                t,
                y,
                "scenario=trigger_initially_true; n=3; u=[0.9,3,6,1.2]; trigger=[true,true,false,true]",
                "the initial() member of the when-vector samples once even when trigger is already true; held true does not resample",
            )
            .with_scenario("trigger_initially_true")
            .with_inputs(vec![input_r("u", u), input_b("trigger", trigger)]),
        );
    }

    // TriggeredMovingMean scenario: n=1 degenerates to the latest triggered sample.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let u = [4.0, 5.0, -2.0, 7.0, 8.0, 1.25];
        let trigger = [false, false, true, true, false, true];
        let y = triggered_moving_mean_y(1, &u, &trigger);
        out.push(
            Golden::new(
                "CDL.Discrete.TriggeredMovingMean",
                "y",
                ValueKind::Real,
                t,
                y,
                "scenario=n_one; n=1; u=[4,5,-2,7,8,1.25]; trigger=[false,false,true,true,false,true]",
                "with n=1 the saturated counter is 1 and the single ring slot is overwritten on each sample event",
            )
            .with_scenario("n_one")
            .with_inputs(vec![input_r("u", u), input_b("trigger", trigger)]),
        );
    }

    // TriggeredSampler: y = y_start until a false->true trigger, then the current u is held.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let u = [1.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let trigger = [false, true, true, false, true, false];
        let y = triggered_sampler_y(2.5, &u, &trigger);
        out.push(
            Golden::new(
                "CDL.Discrete.TriggeredSampler",
                "y",
                ValueKind::Real,
                t,
                y,
                "y_start=2.5; u=[1,3,4,5,6,7]; trigger=[false,true,true,false,true,false]",
                "y starts at y_start and samples current u only on false->true trigger edges; Buildings TriggeredSampler.mo initial equation y=y_start, when trigger then y=u",
            )
            .with_inputs(vec![input_r("u", u), input_b("trigger", trigger)]),
        );
    }

    // TriggeredSampler scenario: trigger true at the initial tick is a rising edge from pre=false.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0];
        let u = [9.0, 10.0, 11.0, 12.0];
        let trigger = [true, true, false, true];
        let y = triggered_sampler_y(-7.0, &u, &trigger);
        out.push(
            Golden::new(
                "CDL.Discrete.TriggeredSampler",
                "y",
                ValueKind::Real,
                t,
                y,
                "scenario=trigger_initially_true; y_start=-7.0; u=[9,10,11,12]; trigger=[true,true,false,true]",
                "pre(trigger) is false at initialization, so trigger=true at t=0 samples current u; held true does not resample until trigger falls and rises",
            )
            .with_scenario("trigger_initially_true")
            .with_inputs(vec![input_r("u", u), input_b("trigger", trigger)]),
        );
    }

    // Constant: y = k for all t. k = 21.5.
    {
        let t = vec![0.0, 60.0, 120.0];
        let k = 21.5;
        let y = vec![r(k); 3];
        out.push(Golden::new(
            "CDL.Reals.Sources.Constant",
            "y",
            ValueKind::Real,
            t,
            y,
            "k=21.5; ticks t=[0,60,120]",
            "y = k (only truly stateless source, t-invariant); _spec/03 §4.1 Sources.Constant",
        ));
    }

    // SampleTrigger: true exactly on new sample boundary phase+k*period; period=2, shift=1.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let period = 2.0_f64;
        let shift = 1.0_f64;
        out.push(sample_trigger_golden(
            None,
            t,
            period,
            shift,
            "period=2.0, shift=1.0; ticks t=[0,1,2,3,4,5] (boundary + intervening)",
            "y true iff tick crosses new instant start+k*period (k>last_k); _spec/03 §4.3 + _spec/01 §11.1 SampleTrigger",
        ));
    }

    // SampleTrigger scenario: shift >= period folds to phase 0.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        out.push(sample_trigger_golden(
            Some("shift_after_period"),
            t,
            1.0,
            2.0,
            "scenario=shift_after_period; period=1.0, shift=2.0; ticks t=[0,1,2,3,4,5]",
            "phase=mod(2.0,1.0)=0.0 with floored Modelica mod, so y fires at t=0,1,2,...; raw-shift behavior would skip t=0 and t=1",
        ));
    }

    // SampleTrigger scenario: negative shift folds into the positive period interval.
    {
        let t = vec![0.0, 0.5, 1.0, 1.5, 2.0];
        out.push(sample_trigger_golden(
            Some("negative_shift"),
            t,
            1.0,
            -0.5,
            "scenario=negative_shift; period=1.0, shift=-0.5; ticks t=[0,0.5,1,1.5,2]",
            "phase=mod(-0.5,1.0)=0.5 with floored Modelica mod, so y fires at t=0.5,1.5,...; raw-shift behavior would fire at t=0",
        ));
    }

    out
}

fn triggered_max_y(u: &[f64], trigger: &[bool]) -> Vec<Sample> {
    assert_eq!(u.len(), trigger.len());
    let mut prev_trigger = false;
    let mut held = 0.0;
    let mut has_history = false;
    let mut y = Vec::with_capacity(u.len());
    for (&cur, &trig) in u.iter().zip(trigger) {
        let base = if has_history { held } else { cur };
        let out = if trig && !prev_trigger {
            oracle_max(base, cur.abs())
        } else {
            base
        };
        y.push(r(out));
        held = out;
        prev_trigger = trig;
        has_history = true;
    }
    y
}

fn triggered_sampler_y(y_start: f64, u: &[f64], trigger: &[bool]) -> Vec<Sample> {
    assert_eq!(u.len(), trigger.len());
    let mut held = y_start;
    let mut prev_trigger = false;
    let mut y = Vec::with_capacity(u.len());
    for (&cur, &trig) in u.iter().zip(trigger) {
        if trig && !prev_trigger {
            held = cur;
        }
        y.push(r(held));
        prev_trigger = trig;
    }
    y
}

fn triggered_moving_mean_y(n: usize, u: &[f64], trigger: &[bool]) -> Vec<Sample> {
    assert!(n >= 1);
    assert_eq!(u.len(), trigger.len());
    let mut samples = vec![0.0; n];
    let mut counter = 0usize;
    let mut next_index = 0usize;
    let mut prev_trigger = false;
    let mut held = 0.0;
    let mut y = Vec::with_capacity(u.len());
    for (&cur, &trig) in u.iter().zip(trigger) {
        let sample = counter == 0 || (trig && !prev_trigger);
        if sample {
            samples[next_index] = cur;
            counter = (counter + 1).min(n);
            let sum = samples.iter().fold(0.0, |acc, value| acc + value);
            held = sum / counter as f64;
            next_index = (next_index + 1) % n;
        }
        y.push(r(held));
        prev_trigger = trig;
    }
    y
}

fn sampled_sampler_y(t: &[f64], period: f64, u: &[f64]) -> Vec<Sample> {
    assert_eq!(t.len(), u.len());
    assert!(period >= 1e-3);
    let t0 = initial_sample_time(t[0], period);
    let mut last = sample_index(t[0], t0, period);
    let mut held = u[0];
    let mut y = vec![r(held)];
    for (&now, &cur) in t.iter().zip(u).skip(1) {
        let (due, index) = sample_due(now, t0, period, last);
        if due {
            held = cur;
            last = index;
        }
        y.push(r(held));
    }
    y
}

fn zero_order_hold_y(t: &[f64], period: f64, u: &[f64]) -> Vec<Sample> {
    assert_eq!(t.len(), u.len());
    assert!(period >= 1e-3);
    let t0 = initial_sample_time(t[0], period);
    let mut last = sample_index(t[0], t0, period);
    let mut held = u[0];
    let mut y = vec![r(u[0])];
    for (&now, &cur) in t.iter().zip(u).skip(1) {
        let (due, index) = sample_due(now, t0, period, last);
        y.push(r(held));
        if due {
            held = cur;
            last = index;
        }
    }
    y
}

fn first_order_hold_y(t: &[f64], period: f64, u: &[f64]) -> Vec<Sample> {
    assert_eq!(t.len(), u.len());
    assert!(period >= 1e-3);
    let t0 = initial_sample_time(t[0], period);
    let mut last = sample_index(t[0], t0, period);
    let mut t_sample = t0;
    let mut u_sample = u[0];
    let mut pre_u_sample = u[0];
    let mut slope = 0.0;
    let mut y = vec![r(u[0])];
    for (&now, &cur) in t.iter().zip(u).skip(1) {
        let (due, index) = sample_due(now, t0, period, last);
        if due {
            y.push(r(u_sample));
            let previous_u = u_sample;
            let first_trigger = now <= t0 + period / 2.0;
            last = index;
            t_sample = now;
            u_sample = cur;
            pre_u_sample = previous_u;
            slope = if first_trigger {
                0.0
            } else {
                (u_sample - pre_u_sample) / period
            };
        } else {
            y.push(r(pre_u_sample + slope * (now - t_sample)));
        }
    }
    y
}

fn initial_sample_time(t_start: f64, period: f64) -> f64 {
    buildings_round_six((t_start / period).floor() * period)
}

fn buildings_round_six(x: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if x > 0.0 {
        (x * FACTOR + 0.5).floor() / FACTOR
    } else {
        (x * FACTOR - 0.5).ceil() / FACTOR
    }
}

fn sample_index(t_now: f64, t0: f64, period: f64) -> i64 {
    ((t_now - t0) / period + 1e-9).floor() as i64
}

fn sample_due(t_now: f64, t0: f64, period: f64, last_index: i64) -> (bool, i64) {
    let index = sample_index(t_now, t0, period);
    (index > last_index, index)
}

fn oracle_max(a: f64, b: f64) -> f64 {
    if a.is_nan() {
        return b;
    }
    if b.is_nan() {
        return a;
    }
    let m = a.max(b);
    if m == 0.0 && (a.is_sign_positive() || b.is_sign_positive()) {
        0.0
    } else {
        m
    }
}

fn sample_trigger_golden(
    scenario: Option<&'static str>,
    t: Vec<f64>,
    period: f64,
    shift: f64,
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    let y = sample_trigger_y(&t, period, shift);
    let mut golden = Golden::new(
        "CDL.Logical.Sources.SampleTrigger",
        "y",
        ValueKind::Boolean,
        t,
        y,
        input_desc,
        rule_desc,
    );
    if let Some(scenario) = scenario {
        golden = golden.with_scenario(scenario);
    }
    golden
}

fn sample_trigger_y(t: &[f64], period: f64, shift: f64) -> Vec<Sample> {
    assert!(period > 0.0);
    let phase = shift - (shift / period).floor() * period;
    let eps = 1e-9 * period;
    let mut last_k: i64 = -1;
    let mut y = Vec::with_capacity(t.len());
    for &now in t {
        let k = ((now - phase) / period + eps / period).floor() as i64;
        let fired = k > last_k;
        if fired {
            last_k = k;
        }
        y.push(b(fired));
    }
    y
}
