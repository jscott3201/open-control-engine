//! CDL.Routing Real-family goldens.
//!
//! The reference formulas are re-derived from the CDL source equations: selector routing,
//! warning-free in-range extraction, scalar/vector fill, mask filtering, and row-major matrix
//! lowering for vector replication.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

fn ticks(n: usize) -> Vec<f64> {
    (0..n).map(|k| (k as f64) * 60.0).collect()
}

fn r(x: f64) -> Sample {
    Sample::Real(x)
}

fn i(x: i64) -> Sample {
    Sample::Integer(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

fn input_i(name: &'static str, values: impl IntoIterator<Item = i64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Integer, values.into_iter().map(i).collect())
}

fn golden(
    class_path: &'static str,
    signal: &'static str,
    time: Vec<f64>,
    samples: Vec<Sample>,
    inputs: Vec<InputSeries>,
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    Golden::new(
        class_path,
        signal,
        ValueKind::Real,
        time,
        samples,
        input_desc,
        rule_desc,
    )
    .with_inputs(inputs)
}

/// Build CDL.Routing Real-family goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();

    // RealExtractSignal: y[i] = u[extract[i]], preserving duplicate selectors and order.
    {
        let time = ticks(4);
        let u1 = [1.0, -0.0, 10.0, -4.0];
        let u2 = [2.0, 20.0, -0.0, 8.0];
        let u3 = [3.0, 30.0, 12.0, -8.0];
        let u4 = [4.0, 40.0, 14.0, 16.0];
        let u5 = [5.0, 50.0, 16.0, -16.0];
        let inputs = vec![
            input_r("u1", u1),
            input_r("u2", u2),
            input_r("u3", u3),
            input_r("u4", u4),
            input_r("u5", u5),
        ];
        let signals = [
            ("y1", u5),
            ("y2", u2),
            ("y3", u2),
            ("y4", u1),
        ];
        for (signal, values) in signals {
            out.push(golden(
                "CDL.Routing.RealExtractSignal",
                signal,
                time.clone(),
                values.into_iter().map(r).collect(),
                inputs.clone(),
                "params nin=5,nout=4,extract=[5,2,2,1]; inputs cover first/last and duplicate selection",
                "y[i] = u[extract[i]] with source-validated 1-based selectors",
            ));
        }
    }

    // RealExtractor: runtime index clamps low/high after warning in source behavior.
    {
        let time = ticks(5);
        let index = [1, 3, 0, 4, -2];
        let u1 = [10.0, 11.0, 12.0, 13.0, 14.0];
        let u2 = [20.0, 21.0, 22.0, 23.0, 24.0];
        let u3 = [30.0, 31.0, 32.0, 33.0, 34.0];
        let y = [u1[0], u3[1], u1[2], u3[3], u1[4]];
        out.push(golden(
            "CDL.Routing.RealExtractor",
            "y",
            time,
            y.into_iter().map(r).collect(),
            vec![
                input_i("index", index),
                input_r("u1", u1),
                input_r("u2", u2),
                input_r("u3", u3),
            ],
            "param nin=3; index trace covers valid, low, high, and negative selectors",
            "y = u[min(nin, max(1, index))]; out-of-range index emits a warning but still clamps",
        ));
    }

    // RealScalarReplicator: y = fill(u, nout).
    {
        let time = ticks(4);
        let u = [1.25, -0.0, -3.5, 8.0];
        let inputs = vec![input_r("u", u)];
        for signal in ["y1", "y2", "y3"] {
            out.push(golden(
                "CDL.Routing.RealScalarReplicator",
                signal,
                time.clone(),
                u.into_iter().map(r).collect(),
                inputs.clone(),
                "param nout=3; scalar input includes signed-zero preservation",
                "y = fill(u, nout)",
            ));
        }
    }

    // RealVectorFilter: y = u[BooleanVectors.index(msk)] in true-mask order.
    {
        let time = ticks(4);
        let u1 = [1.0, 10.0, -1.0, -10.0];
        let u2 = [2.0, 20.0, -2.0, -20.0];
        let u3 = [3.0, 30.0, -3.0, -30.0];
        let u4 = [4.0, 40.0, -4.0, -40.0];
        let inputs = vec![
            input_r("u1", u1),
            input_r("u2", u2),
            input_r("u3", u3),
            input_r("u4", u4),
        ];
        for (signal, values) in [("y1", u2), ("y2", u3)] {
            out.push(golden(
                "CDL.Routing.RealVectorFilter",
                signal,
                time.clone(),
                values.into_iter().map(r).collect(),
                inputs.clone(),
                "params nin=4,nout=2,msk=[false,true,true,false]",
                "y = u[BooleanVectors.index(msk)] preserving true-mask input order",
            ));
        }
    }

    // RealVectorReplicator: y[nout,nin] = fill(u, nout), lowered row-major.
    {
        let time = ticks(4);
        let u1 = [7.0, -7.0, 0.5, -0.0];
        let u2 = [8.0, -8.0, -0.5, 0.0];
        let inputs = vec![input_r("u1", u1), input_r("u2", u2)];
        let signals = [
            ("y1", u1),
            ("y2", u2),
            ("y3", u1),
            ("y4", u2),
            ("y5", u1),
            ("y6", u2),
        ];
        for (signal, values) in signals {
            out.push(golden(
                "CDL.Routing.RealVectorReplicator",
                signal,
                time.clone(),
                values.into_iter().map(r).collect(),
                inputs.clone(),
                "params nin=2,nout=3; matrix y[3,2] flattened row-major to y1..y6",
                "y[row,col] = u[col], emitted as row-major scalar outputs",
            ));
        }
    }

    out
}
