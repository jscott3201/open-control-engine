//! Stage block goldens for `CDL.Integers`.
//!
//! The reference traces are re-derived from Buildings `Integers/Stage.mo`: held `pre(y)`, evented
//! `tNext`, one-based thresholds, and source-preserved `y=0` behavior.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

fn ticks(n: usize) -> Vec<f64> {
    (0..n).map(|k| k as f64).collect()
}

/// Build Stage goldens.
pub fn goldens() -> Vec<Golden> {
    vec![
        stage_golden(
            "n1_initial_zero",
            StageOracle {
                n: 1,
                hold_duration: 0.0,
                h: 0.02,
                pre_y_start: 0,
            },
            vec![0.0, 1.0, 2.0],
            &[0.0, 0.03, 0.0],
            "scenario=n1_initial_zero; n=1, holdDuration=0, h=0.02, pre_y_start=0; u=[0,0.03,0]",
            "no event fires on the initial tick; staThr[1]=0 and the later u>h update selects y=1; y=0 is not clamped away",
        ),
        stage_golden(
            "threshold_boundaries",
            StageOracle {
                n: 4,
                hold_duration: 0.0,
                h: 0.001,
                pre_y_start: 0,
            },
            ticks(4),
            &[0.0, 0.249_999_999_999_999_97, 0.0, 0.25],
            "scenario=threshold_boundaries; n=4, holdDuration=0, h=0.001; u=[0,next_down(0.25),0,0.25]",
            "the full when condition must re-arm; first event below staThr[2] selects y=1, while exact staThr[2] later remains inside the upper hysteresis deadband",
        ),
        stage_golden(
            "hysteresis_hold",
            StageOracle {
                n: 4,
                hold_duration: 0.0,
                h: 0.05,
                pre_y_start: 0,
            },
            ticks(6),
            &[0.0, 0.31, 0.46, 0.44, 0.22, 0.19],
            "scenario=hysteresis_hold; n=4, holdDuration=0, h=0.05; u=[0,0.31,0.46,0.44,0.22,0.19]",
            "after selecting y=2, the output holds at u=0.22 inside the lower hysteresis band and only downstages once u drops below lowerThreshold-h",
        ),
        stage_golden(
            "hold_duration",
            StageOracle {
                n: 4,
                hold_duration: 2.0,
                h: 0.05,
                pre_y_start: 0,
            },
            ticks(5),
            &[1.0, 1.0, 1.0, 0.0, 0.0],
            "scenario=hold_duration; n=4, holdDuration=2, h=0.05; u=[1,1,1,0,0]",
            "updates are blocked until time >= pre(tNext); firing sets tNext=time+holdDuration, not pre(tNext)+holdDuration",
        ),
        stage_golden(
            "unclamped_zero",
            StageOracle {
                n: 4,
                hold_duration: 0.0,
                h: 0.05,
                pre_y_start: 3,
            },
            ticks(3),
            &[0.0, -0.10, 0.0],
            "scenario=unclamped_zero; n=4, holdDuration=0, h=0.05, pre_y_start=3; u=[0,-0.10,0]",
            "the authored pre_y_start is emitted on the initial tick and a re-armed out-of-range low input can emit y=0 despite IntegerOutput(min=1)",
        ),
    ]
}

#[derive(Clone, Copy)]
struct StageOracle {
    n: i64,
    hold_duration: f64,
    h: f64,
    pre_y_start: i64,
}

fn stage_golden(
    scenario: &'static str,
    params: StageOracle,
    time: Vec<f64>,
    u: &[f64],
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    let y = stage_y(params, &time, u);
    Golden::new(
        "CDL.Integers.Stage",
        "y",
        ValueKind::Integer,
        time,
        y,
        input_desc,
        rule_desc,
    )
    .with_inputs(vec![input_r("u", u.iter().copied())])
    .with_scenario(scenario)
}

fn stage_y(params: StageOracle, time: &[f64], u: &[f64]) -> Vec<Sample> {
    assert_eq!(time.len(), u.len());
    let n = params.n.max(1);
    let mut y = params.pre_y_start;
    let mut t_next = time.first().copied().unwrap_or(0.0) + params.hold_duration;
    let mut upper_threshold = 0.0;
    let mut lower_threshold = 0.0;
    let mut prev_check_upper = false;
    let mut prev_check_lower = true;
    let mut prev_when_condition = false;
    let mut initialized = false;
    let mut out = Vec::with_capacity(time.len());

    for (&t, &u) in time.iter().zip(u) {
        let check_upper = (!prev_check_upper && u > upper_threshold + params.h)
            || (prev_check_upper && u >= upper_threshold - params.h);
        let check_lower = (!prev_check_lower && u > lower_threshold + params.h)
            || (prev_check_lower && u >= lower_threshold - params.h);

        let condition = t >= t_next && (check_upper || !check_lower);
        if initialized && condition && !prev_when_condition {
            y = stage_for_u(u, n);
            t_next = t + params.hold_duration;
            upper_threshold = if y == n {
                stage_threshold(n, n)
            } else {
                stage_threshold(y + 1, n)
            };
            lower_threshold = if y == 0 {
                lower_threshold
            } else {
                stage_threshold(y, n)
            };
        }

        out.push(i(y));
        prev_check_upper = check_upper;
        prev_check_lower = check_lower;
        prev_when_condition = condition;
        initialized = true;
    }

    out
}

fn stage_for_u(u: f64, n: i64) -> i64 {
    if u >= stage_threshold(n, n) {
        return n;
    }
    for i_stage in 2..=n {
        if u < stage_threshold(i_stage, n) && u >= stage_threshold(i_stage - 1, n) {
            return i_stage - 1;
        }
    }
    0
}

fn stage_threshold(stage_idx: i64, n: i64) -> f64 {
    (stage_idx - 1) as f64 / n as f64
}
