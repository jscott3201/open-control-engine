//! Oracle data model: the spec-derived value kinds and the in-memory description of one golden.
//!
//! All math that fills these structures lives in the family modules (`reals`, `logical`,
//! `integers_conversions`, `discrete_sources`). Each family re-derives its reference series ONLY
//! from `_spec/03`, `_spec/02`, `_spec/01`, and CDL §7.x — never from `oce-blocks`.

/// Scalar signal kind, matching the conformance `ValueKind` encoding contract (`_spec/07` §9.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueKind {
    /// Real-valued signal, f64 payload, banded/bit-exact compare.
    Real,
    /// Integer-valued signal (i64), encoded exactly as f64 within +/-2^53, exact compare.
    Integer,
    /// Boolean-valued signal, encoded 0.0 / 1.0, exact compare.
    Boolean,
}

impl ValueKind {
    /// Lowercase spec name for provenance JSON.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ValueKind::Real => "Real",
            ValueKind::Integer => "Integer",
            ValueKind::Boolean => "Boolean",
        }
    }
}

/// A scalar sample carrying its declared kind, so encoding to the numeric CSV cell is unambiguous.
#[derive(Clone, Copy, Debug)]
pub enum Sample {
    /// Real f64 payload (may be non-finite for the IEEE Divide oracle).
    Real(f64),
    /// Exact integer payload.
    Integer(i64),
    /// Boolean payload.
    Boolean(bool),
}

impl Sample {
    /// Encode to the numeric CombiTimeTable cell convention (`_spec/07` §9.3).
    ///
    /// Boolean -> 0.0/1.0; Integer -> exact f64; Real -> raw f64 payload (non-finite passes
    /// through, since the self-contained writer does not reject it).
    #[must_use]
    pub fn encode(self) -> f64 {
        match self {
            Sample::Real(x) => x,
            // Exactness within +/-2^53 is guaranteed by the chosen oracle traces; the one
            // intentional 2^53+1 IntegerToReal probe is emitted as a Real sample, not Integer.
            Sample::Integer(i) => i as f64,
            Sample::Boolean(b) => {
                if b {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// One emitted golden: a single output signal of a single block instance.
pub struct Golden {
    /// CDL class path, e.g. `CDL.Reals.Add`.
    pub class_path: &'static str,
    /// Output connector / signal name, e.g. `y`, `passed`, `up`.
    pub signal: &'static str,
    /// Declared value kind of this signal.
    pub kind: ValueKind,
    /// Time samples in seconds (non-decreasing), parallel to `samples`.
    pub time: Vec<f64>,
    /// Reference samples (closed-form spec math), parallel to `time`.
    pub samples: Vec<Sample>,
    /// Human description of the input trace / parameters (for provenance).
    pub input_desc: String,
    /// Short statement of the closed-form rule used (for provenance).
    pub rule_desc: String,
}

impl Golden {
    /// Convenience constructor.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        class_path: &'static str,
        signal: &'static str,
        kind: ValueKind,
        time: Vec<f64>,
        samples: Vec<Sample>,
        input_desc: impl Into<String>,
        rule_desc: impl Into<String>,
    ) -> Self {
        assert_eq!(
            time.len(),
            samples.len(),
            "{class_path}.{signal}: time/sample length mismatch"
        );
        Golden {
            class_path,
            signal,
            kind,
            time,
            samples,
            input_desc: input_desc.into(),
            rule_desc: rule_desc.into(),
        }
    }
}
