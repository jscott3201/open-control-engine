//! `CDL.Logical.VariablePulse` goldens derived from Buildings source equations.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

const CLASS: &str = "CDL.Logical.VariablePulse";

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

/// Build `CDL.Logical.VariablePulse` oracle traces.
pub fn goldens() -> Vec<Golden> {
    vec![
        golden(
            None,
            TraceParams {
                period: 4.0,
                delta_u: 0.01,
                min_hold: 0.04,
            },
            &[
                (0.0, 0.75),
                (2.999_999, 0.75),
                (3.0, 0.75),
                (3.5, 0.75),
                (4.0, 0.75),
            ],
            "period=4, deltaU=0.01, minTruFalHol=0.04; constant u=0.75",
            "Buildings VariablePulse.mo Cycle: t0 reset on initial width sample, t_sta=t0+round(integer((time-t0)/period)*period,n=6), t_end=t_sta+u*period, true on [t_sta,t_end), then TrueFalseHold",
        ),
        golden(
            Some("endpoints"),
            TraceParams {
                period: 4.0,
                delta_u: 0.01,
                min_hold: 0.04,
            },
            &[(0.0, 0.0), (1.0, 0.0), (2.0, 1.0), (5.0, 1.0)],
            "scenario=endpoints; period=4, deltaU=0.01, minTruFalHol=0.04; u exercises exact 0 and 1 duty-ratio endpoints",
            "Source declares u in [0,1]; Cycle output is always false for u<=0 and always true for u>=1",
        ),
        golden(
            Some("delta_reset"),
            TraceParams {
                period: 4.0,
                delta_u: 0.125,
                min_hold: 0.04,
            },
            &[
                (0.0, 0.25),
                (1.0, 0.25),
                (2.0, 0.375),
                (2.001, 0.75),
            ],
            "scenario=delta_reset; period=4, deltaU=0.125, minTruFalHol=0.04; u diff exactly equal to delta then above delta just after the non-reset tick",
            "GreaterThreshold(t=deltaU,h=0) is strict: abs(sampled_u-u)>deltaU resets Cycle.t0, equality does not",
        ),
        golden(
            Some("minimum_hold"),
            TraceParams {
                period: 3.0,
                delta_u: 0.01,
                min_hold: 1.0,
            },
            &[
                (0.0, 0.5),
                (1.5, 0.5),
                (1.6, 0.9),
                (2.49, 0.9),
                (2.6, 0.9),
                (3.2, 0.5),
                (4.7, 0.5),
                (5.0, 0.5),
            ],
            "scenario=minimum_hold; Buildings Validation/VariablePulseMinHold-style width jump while previous output state has not met minTruFalHol",
            "TrueFalseHold keeps the previous Boolean output until the current held state has lasted at least minTruFalHol",
        ),
        golden(
            Some("adjusted_period"),
            TraceParams {
                period: 1.0,
                delta_u: 0.01,
                min_hold: 1.0,
            },
            &[(0.0, 0.5), (0.75, 0.5), (1.02, 0.5)],
            "scenario=adjusted_period; period=1, minTruFalHol=1",
            "Source warning path uses adjustedPeriod=max(period,2.02*minTruFalHol) for Cycle while warning that period must be >= 2*minTruFalHol",
        ),
    ]
}

fn golden(
    scenario: Option<&'static str>,
    params: TraceParams,
    steps: &[(f64, f64)],
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    let time = steps.iter().map(|(t, _)| *t).collect::<Vec<_>>();
    let u = steps.iter().map(|(_, u)| *u).collect::<Vec<_>>();
    let samples = trace(params, steps).into_iter().map(b).collect();
    let mut golden = Golden::new(
        CLASS,
        "y",
        ValueKind::Boolean,
        time,
        samples,
        input_desc,
        rule_desc,
    )
    .with_inputs(vec![InputSeries::new(
        "u",
        ValueKind::Real,
        u.into_iter().map(r).collect(),
    )]);
    if let Some(scenario) = scenario {
        golden = golden.with_scenario(scenario);
    }
    golden
}

#[derive(Clone, Copy)]
struct TraceParams {
    period: f64,
    delta_u: f64,
    min_hold: f64,
}

#[derive(Clone, Copy)]
struct TraceState {
    sampled_u: f64,
    t0: f64,
    held: bool,
    timer: f64,
    prev_t: f64,
    initialized: bool,
}

fn trace(params: TraceParams, steps: &[(f64, f64)]) -> Vec<bool> {
    let mut state = TraceState {
        sampled_u: 0.0,
        t0: 0.0,
        held: false,
        timer: 0.0,
        prev_t: 0.0,
        initialized: false,
    };
    let mut out = Vec::with_capacity(steps.len());
    for &(time, u) in steps {
        let first = !state.initialized;
        let changed = (state.sampled_u - u).abs() > params.delta_u;
        let t0 = if first || changed { time } else { state.t0 };
        let sampled_u = if changed { u } else { state.sampled_u };
        let cycle_y = cycle_y(time, u, t0, adjusted_period(params));

        let (held, timer) = if first {
            (cycle_y, 0.0)
        } else {
            let timer = state.timer + (time - state.prev_t);
            if cycle_y == state.held {
                (state.held, timer)
            } else if timer >= params.min_hold {
                (cycle_y, 0.0)
            } else {
                (state.held, timer)
            }
        };

        state = TraceState {
            sampled_u,
            t0,
            held,
            timer,
            prev_t: time,
            initialized: true,
        };
        out.push(held);
    }
    out
}

fn cycle_y(time: f64, u: f64, t0: f64, period: f64) -> bool {
    if u <= 0.0 {
        return false;
    }
    if u >= 1.0 {
        return true;
    }
    let t_start = t0 + buildings_round_six(((time - t0) / period).floor() * period);
    let t_end = t_start + u * period;
    time >= t_start && time < t_end
}

fn adjusted_period(params: TraceParams) -> f64 {
    params.period.max(params.min_hold * 2.02)
}

fn buildings_round_six(x: f64) -> f64 {
    const FACTOR: f64 = 1_000_000.0;
    if x > 0.0 {
        (x * FACTOR + 0.5).floor() / FACTOR
    } else {
        (x * FACTOR - 0.5).ceil() / FACTOR
    }
}
