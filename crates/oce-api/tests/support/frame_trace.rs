//! Host-owned complete-frame trace collection for deterministic scenario tests.
//! Internal point observations are comparison evidence, not boundary-output receipts.

use oce_api::{Engine, OcError, Value};

pub(crate) struct FrameRun {
    pub(crate) ticks: u64,
    pub(crate) trace: FrameTrace,
}

pub(crate) struct FrameTrace {
    paths: Vec<String>,
    times: Vec<f64>,
    values: Vec<Vec<Value>>,
}

impl FrameRun {
    /// Drive a fresh fixture at a fixed multiply-based cadence. Every row supplies all inputs;
    /// there is no engine reset, sparse staging, Store access or implicit value substitution.
    pub(crate) fn record(
        engine: &mut Engine,
        start: f64,
        stop: f64,
        step: f64,
        inputs: impl Fn(f64) -> Vec<(String, Value)>,
        paths: Vec<String>,
    ) -> Result<Self, OcError> {
        assert!(start.is_finite() && stop.is_finite() && step.is_finite());
        assert!(step > 0.0 && stop >= start);
        for path in &paths {
            engine.get_output(path)?;
        }
        let mut trace = FrameTrace {
            values: vec![Vec::new(); paths.len()],
            paths,
            times: Vec::new(),
        };
        let count = ((stop - start) / step).floor() as u64;
        for index in 0..=count {
            let time = start + index as f64 * step;
            let observations = inputs(time);
            let entries: Vec<_> = observations
                .iter()
                .map(|(p, v)| (p.as_str(), v.clone()))
                .collect();
            let prepared = engine.prepare_frame(time, &entries)?;
            let completed = engine.execute_frame(prepared)?;
            assert_eq!(completed.time().to_bits(), time.to_bits());
            trace.times.push(completed.time());
            for (path, column) in trace.paths.iter().zip(&mut trace.values) {
                column.push(engine.get_output(path)?);
            }
        }
        Ok(Self {
            ticks: trace.rows() as u64,
            trace,
        })
    }
}

impl FrameTrace {
    pub(crate) fn columns(&self) -> &[String] {
        &self.paths
    }
    pub(crate) fn times(&self) -> &[f64] {
        &self.times
    }
    pub(crate) fn column(&self, index: usize) -> Option<&[Value]> {
        self.values.get(index).map(Vec::as_slice)
    }
    pub(crate) fn rows(&self) -> usize {
        self.times.len()
    }
}
