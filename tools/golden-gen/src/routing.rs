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

fn b(x: bool) -> Sample {
    Sample::Boolean(x)
}

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Real, values.into_iter().map(r).collect())
}

fn input_i(name: &'static str, values: impl IntoIterator<Item = i64>) -> InputSeries {
    InputSeries::new(name, ValueKind::Integer, values.into_iter().map(i).collect())
}

fn input_b(name: &'static str, values: impl IntoIterator<Item = bool>) -> InputSeries {
    InputSeries::new(name, ValueKind::Boolean, values.into_iter().map(b).collect())
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

fn typed_golden(
    class_path: &'static str,
    signal: &'static str,
    kind: ValueKind,
    time: Vec<f64>,
    samples: Vec<Sample>,
    inputs: Vec<InputSeries>,
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    Golden::new(class_path, signal, kind, time, samples, input_desc, rule_desc).with_inputs(inputs)
}

/// Build CDL.Routing typed-family goldens.
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

    // BooleanExtractSignal: y[i] = u[extract[i]], preserving duplicate selectors and order.
    {
        let time = ticks(4);
        let u1 = [true, false, false, true];
        let u2 = [false, false, true, true];
        let u3 = [true, true, false, false];
        let u4 = [false, true, true, false];
        let u5 = [true, false, true, false];
        let inputs = vec![
            input_b("u1", u1),
            input_b("u2", u2),
            input_b("u3", u3),
            input_b("u4", u4),
            input_b("u5", u5),
        ];
        for (signal, values) in [("y1", u5), ("y2", u2), ("y3", u2), ("y4", u1)] {
            out.push(typed_golden(
                "CDL.Routing.BooleanExtractSignal",
                signal,
                ValueKind::Boolean,
                time.clone(),
                values.into_iter().map(b).collect(),
                inputs.clone(),
                "params nin=5,nout=4,extract=[5,2,2,1]; Boolean inputs cover true/false duplicate selection",
                "y[i] = u[extract[i]] with source-validated 1-based selectors",
            ));
        }
    }

    // BooleanExtractor: runtime index clamps low/high after warning in source behavior.
    {
        let time = ticks(5);
        let index = [1, 3, 0, 4, -2];
        let u1 = [true, false, false, true, true];
        let u2 = [false, true, false, false, true];
        let u3 = [false, false, true, true, false];
        let y = [u1[0], u3[1], u1[2], u3[3], u1[4]];
        out.push(typed_golden(
            "CDL.Routing.BooleanExtractor",
            "y",
            ValueKind::Boolean,
            time,
            y.into_iter().map(b).collect(),
            vec![
                input_i("index", index),
                input_b("u1", u1),
                input_b("u2", u2),
                input_b("u3", u3),
            ],
            "param nin=3; index trace covers valid, low, high, and negative selectors",
            "y = u[min(nin, max(1, index))]; out-of-range index emits a warning but still clamps",
        ));
    }

    // BooleanScalarReplicator: y = fill(u, nout).
    {
        let time = ticks(4);
        let u = [true, false, true, false];
        let inputs = vec![input_b("u", u)];
        for signal in ["y1", "y2", "y3"] {
            out.push(typed_golden(
                "CDL.Routing.BooleanScalarReplicator",
                signal,
                ValueKind::Boolean,
                time.clone(),
                u.into_iter().map(b).collect(),
                inputs.clone(),
                "param nout=3; scalar Boolean input alternates true/false",
                "y = fill(u, nout)",
            ));
        }
    }

    // BooleanVectorFilter: y = u[BooleanVectors.index(msk)] in true-mask order.
    {
        let time = ticks(4);
        let u1 = [true, false, true, false];
        let u2 = [false, false, true, true];
        let u3 = [true, true, false, false];
        let u4 = [false, true, true, false];
        let inputs = vec![
            input_b("u1", u1),
            input_b("u2", u2),
            input_b("u3", u3),
            input_b("u4", u4),
        ];
        for (signal, values) in [("y1", u2), ("y2", u3)] {
            out.push(typed_golden(
                "CDL.Routing.BooleanVectorFilter",
                signal,
                ValueKind::Boolean,
                time.clone(),
                values.into_iter().map(b).collect(),
                inputs.clone(),
                "params nin=4,nout=2,msk=[false,true,true,false]",
                "y = u[BooleanVectors.index(msk)] preserving true-mask input order",
            ));
        }
    }

    // BooleanVectorReplicator: y[nout,nin] = fill(u, nout), lowered row-major.
    {
        let time = ticks(4);
        let u1 = [true, false, true, false];
        let u2 = [false, true, false, true];
        let inputs = vec![input_b("u1", u1), input_b("u2", u2)];
        let signals = [
            ("y1", u1),
            ("y2", u2),
            ("y3", u1),
            ("y4", u2),
            ("y5", u1),
            ("y6", u2),
        ];
        for (signal, values) in signals {
            out.push(typed_golden(
                "CDL.Routing.BooleanVectorReplicator",
                signal,
                ValueKind::Boolean,
                time.clone(),
                values.into_iter().map(b).collect(),
                inputs.clone(),
                "params nin=2,nout=3; matrix y[3,2] flattened row-major to y1..y6",
                "y[row,col] = u[col], emitted as row-major scalar outputs",
            ));
        }
    }

    // IntegerExtractSignal: y[i] = u[extract[i]], preserving duplicate selectors and order.
    {
        let time = ticks(4);
        let u1 = [1, -1, 0, 9_007_199_254_740_992];
        let u2 = [2, -2, 20, -9_007_199_254_740_992];
        let u3 = [3, -3, 30, 1024];
        let u4 = [4, -4, 40, -1024];
        let u5 = [5, -5, 50, 0];
        let inputs = vec![
            input_i("u1", u1),
            input_i("u2", u2),
            input_i("u3", u3),
            input_i("u4", u4),
            input_i("u5", u5),
        ];
        for (signal, values) in [("y1", u5), ("y2", u2), ("y3", u2), ("y4", u1)] {
            out.push(typed_golden(
                "CDL.Routing.IntegerExtractSignal",
                signal,
                ValueKind::Integer,
                time.clone(),
                values.into_iter().map(i).collect(),
                inputs.clone(),
                "params nin=5,nout=4,extract=[5,2,2,1]; Integer inputs include signed and exact 2^53 boundary values",
                "y[i] = u[extract[i]] with source-validated 1-based selectors",
            ));
        }
    }

    // IntegerExtractor: runtime index clamps low/high after warning in source behavior.
    {
        let time = ticks(5);
        let index = [1, 3, 0, 4, -2];
        let u1 = [10, 11, 12, 13, 14];
        let u2 = [-20, -21, -22, -23, -24];
        let u3 = [
            9_007_199_254_740_992,
            -9_007_199_254_740_992,
            30,
            31,
            32,
        ];
        let y = [u1[0], u3[1], u1[2], u3[3], u1[4]];
        out.push(typed_golden(
            "CDL.Routing.IntegerExtractor",
            "y",
            ValueKind::Integer,
            time,
            y.into_iter().map(i).collect(),
            vec![
                input_i("index", index),
                input_i("u1", u1),
                input_i("u2", u2),
                input_i("u3", u3),
            ],
            "param nin=3; index trace covers valid, low, high, and negative selectors",
            "y = u[min(nin, max(1, index))]; out-of-range index emits a warning but still clamps",
        ));
    }

    // IntegerScalarReplicator: y = fill(u, nout).
    {
        let time = ticks(4);
        let u = [1, -1, 9_007_199_254_740_992, -9_007_199_254_740_992];
        let inputs = vec![input_i("u", u)];
        for signal in ["y1", "y2", "y3"] {
            out.push(typed_golden(
                "CDL.Routing.IntegerScalarReplicator",
                signal,
                ValueKind::Integer,
                time.clone(),
                u.into_iter().map(i).collect(),
                inputs.clone(),
                "param nout=3; scalar Integer input includes exact 2^53 boundary values",
                "y = fill(u, nout)",
            ));
        }
    }

    // IntegerVectorFilter: y = u[BooleanVectors.index(msk)] in true-mask order.
    {
        let time = ticks(4);
        let u1 = [1, 10, -1, -10];
        let u2 = [2, 20, -2, -20];
        let u3 = [9_007_199_254_740_992, -9_007_199_254_740_992, 3, 30];
        let u4 = [4, 40, -4, -40];
        let inputs = vec![
            input_i("u1", u1),
            input_i("u2", u2),
            input_i("u3", u3),
            input_i("u4", u4),
        ];
        for (signal, values) in [("y1", u2), ("y2", u3)] {
            out.push(typed_golden(
                "CDL.Routing.IntegerVectorFilter",
                signal,
                ValueKind::Integer,
                time.clone(),
                values.into_iter().map(i).collect(),
                inputs.clone(),
                "params nin=4,nout=2,msk=[false,true,true,false]",
                "y = u[BooleanVectors.index(msk)] preserving true-mask input order",
            ));
        }
    }

    // IntegerVectorReplicator: y[nout,nin] = fill(u, nout), lowered row-major.
    {
        let time = ticks(4);
        let u1 = [7, -7, 9_007_199_254_740_992, 0];
        let u2 = [8, -8, -9_007_199_254_740_992, 1];
        let inputs = vec![input_i("u1", u1), input_i("u2", u2)];
        let signals = [
            ("y1", u1),
            ("y2", u2),
            ("y3", u1),
            ("y4", u2),
            ("y5", u1),
            ("y6", u2),
        ];
        for (signal, values) in signals {
            out.push(typed_golden(
                "CDL.Routing.IntegerVectorReplicator",
                signal,
                ValueKind::Integer,
                time.clone(),
                values.into_iter().map(i).collect(),
                inputs.clone(),
                "params nin=2,nout=3; matrix y[3,2] flattened row-major to y1..y6",
                "y[row,col] = u[col], emitted as row-major scalar outputs",
            ));
        }
    }

    out
}
