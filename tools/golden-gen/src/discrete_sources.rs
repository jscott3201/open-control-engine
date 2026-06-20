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

use crate::oracle::{Golden, Sample, ValueKind};

fn r(x: f64) -> Sample {
    Sample::Real(x)
}
fn b(x: bool) -> Sample {
    Sample::Boolean(x)
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
        ));
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

    // SampleTrigger: true exactly on new sample boundary start+k*period; period=2, shift=1.
    {
        let t = vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let period = 2.0_f64;
        let start = 1.0_f64; // = shift
        let eps = 1e-9 * period; // documented epsilon (_spec/01 §11.1 req 3), comfortably clear here
        let mut last_k: i64 = -1;
        let mut y = Vec::new();
        for &now in &t {
            let fired = if now + eps >= start {
                let k = ((now - start) / period + eps / period).floor() as i64;
                if k > last_k {
                    last_k = k;
                    true
                } else {
                    false
                }
            } else {
                false
            };
            y.push(b(fired));
        }
        out.push(Golden::new(
            "CDL.Logical.Sources.SampleTrigger",
            "y",
            ValueKind::Boolean,
            t,
            y,
            "period=2.0, shift=1.0; ticks t=[0,1,2,3,4,5] (boundary + intervening)",
            "y true iff tick crosses new instant start+k*period (k>last_k); _spec/03 §4.3 + _spec/01 §11.1 SampleTrigger",
        ));
    }

    out
}
