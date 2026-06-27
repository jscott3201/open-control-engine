//! Typed `CDL.*.Sources.TimeTable` goldens from Buildings source semantics.

use crate::oracle::{Golden, Sample, ValueKind};

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}

/// Build Reals/Integers/Logical `Sources.TimeTable` goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    {
        let time = vec![-1.0, 0.0, 1.0, 1.5, 4.0];
        let table = [
            [0.0, 0.0, 10.0],
            [1.0, 0.0, 20.0],
            [1.0, 1.0, 30.0],
            [2.0, 4.0, 40.0],
            [3.0, 9.0, 50.0],
        ];
        let offsets = [0.5, -1.0];
        for out_idx in 0..2 {
            out.push(Golden::new(
                "CDL.Reals.Sources.TimeTable",
                if out_idx == 0 { "y1" } else { "y2" },
                ValueKind::Real,
                time.clone(),
                time.iter()
                    .copied()
                    .map(|t| r(real_table_linear_last_two(t, &table, out_idx + 1) + offsets[out_idx]))
                    .collect(),
                "table=[0,0,10;1,0,20;1,1,30;2,4,40;3,9,50], smoothness=LinearSegments, extrapolation=LastTwoPoints, offset=[0.5,-1]",
                "Buildings Reals/Sources/TimeTable.mo delegates to CombiTimeTable: duplicate timestamps use after-event row at sampled boundary; LastTwoPoints extrapolates linearly through first/last two distinct points",
            ));
        }
    }

    {
        let time = vec![-1.0, 0.0, 2.0, 3.0, 5.0];
        let table = [[0.0, 0.0], [1.0, 10.0], [2.0, 20.0]];
        out.push(
            Golden::new(
                "CDL.Reals.Sources.TimeTable",
                "y",
                ValueKind::Real,
                time.clone(),
                time.iter()
                    .copied()
                    .map(|t| r(real_table_periodic_linear(t, &table, 2.0)))
                    .collect(),
                "scenario=periodic_scaled; table=[0,0;1,10;2,20], timeScale=2, extrapolation=Periodic",
                "Periodic Reals TimeTable repeats the scaled table time range; timeScale multiplies first-column times before lookup",
            )
            .with_scenario("periodic_scaled"),
        );
    }

    {
        let time = vec![-1.0, 0.0, 1.999_999_5, 2.0, 5.0, 6.25];
        let table = [[0.0, -2.0, 7.0], [2.0, 3.0, 8.0], [5.0, 4.0, 9.0]];
        for out_idx in 0..2 {
            out.push(Golden::new(
                "CDL.Integers.Sources.TimeTable",
                if out_idx == 0 { "y1" } else { "y2" },
                ValueKind::Integer,
                time.clone(),
                time.iter()
                    .copied()
                    .map(|t| i(integer_table_value(t, &table, out_idx + 1, 6.0)))
                    .collect(),
                "table=[0,-2,7;2,3,8;5,4,9], period=6; periodic step lookup with 1e-6 timestamp guard",
                "Buildings Integers/Sources/TimeTable.mo: tS=mod(time,period); choose last timestamp with tS >= timeStamp - 1e-6; y=integer(table+small)",
            ));
        }
    }

    {
        let time = vec![-0.5, 0.5, 1.0, 3.5, 4.25];
        let table = [[0.0, 0.0, 1.0], [1.0, 1.0, 0.0], [3.0, 0.0, 1.0]];
        for out_idx in 0..2 {
            out.push(Golden::new(
                "CDL.Logical.Sources.TimeTable",
                if out_idx == 0 { "y1" } else { "y2" },
                ValueKind::Boolean,
                time.clone(),
                time.iter()
                    .copied()
                    .map(|t| b(integer_table_value(t, &table, out_idx + 1, 4.0) > 0))
                    .collect(),
                "table=[0,0,1;1,1,0;3,0,1], period=4; Logical table wraps Integer table through GreaterThreshold(t=0)",
                "Buildings Logical/Sources/TimeTable.mo delegates to Integers.Sources.TimeTable and converts each output with GreaterThreshold(t=0)",
            ));
        }
    }

    out
}

fn real_table_periodic_linear(t: f64, table: &[[f64; 2]], time_scale: f64) -> f64 {
    let scaled = [
        [table[0][0] * time_scale, table[0][1]],
        [table[1][0] * time_scale, table[1][1]],
        [table[2][0] * time_scale, table[2][1]],
    ];
    let range = scaled[2][0] - scaled[0][0];
    let local = scaled[0][0] + (t - scaled[0][0]).rem_euclid(range);
    linear_between_distinct(local, &scaled)
}

fn real_table_linear_last_two(t: f64, table: &[[f64; 3]], col: usize) -> f64 {
    if t <= table[0][0] {
        return interpolate(t, table[0][0], table[1][0], table[0][col], table[1][col]);
    }
    if t >= table[4][0] {
        return interpolate(t, table[3][0], table[4][0], table[3][col], table[4][col]);
    }
    if t == 1.0 {
        return table[2][col];
    }
    if t < 1.0 {
        return interpolate(t, table[0][0], table[1][0], table[0][col], table[1][col]);
    }
    if t < 2.0 {
        return interpolate(t, table[2][0], table[3][0], table[2][col], table[3][col]);
    }
    interpolate(t, table[3][0], table[4][0], table[3][col], table[4][col])
}

fn linear_between_distinct(t: f64, table: &[[f64; 2]; 3]) -> f64 {
    if t == table[0][0] {
        return table[0][1];
    }
    if t < table[1][0] {
        return interpolate(t, table[0][0], table[1][0], table[0][1], table[1][1]);
    }
    if t == table[1][0] {
        return table[1][1];
    }
    interpolate(t, table[1][0], table[2][0], table[1][1], table[2][1])
}

fn integer_table_value(t: f64, table: &[[f64; 3]], col: usize, period: f64) -> i64 {
    let t_shifted = t.rem_euclid(period);
    let mut row = 0;
    for (idx, candidate) in table.iter().enumerate().rev() {
        if t_shifted >= candidate[0] - 1.0e-6 {
            row = idx;
            break;
        }
    }
    (table[row][col] + 1.0e-37).floor() as i64
}

fn interpolate(t: f64, t0: f64, t1: f64, y0: f64, y1: f64) -> f64 {
    y0 + (t - t0) / (t1 - t0) * (y1 - y0)
}
