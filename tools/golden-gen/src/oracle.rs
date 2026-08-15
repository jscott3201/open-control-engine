//! Data model for source-derived and execution-profile reference goldens.
//!
//! The family modules derive each series from the applicable CDL source/spec or named execution
//! profile, never from `oce-blocks`.

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

/// One input column that produced a golden trace.
#[derive(Clone)]
pub struct InputSeries {
    /// CDL input connector / signal name, e.g. `u`, `u1`, `clr`.
    pub name: &'static str,
    /// Declared value kind of this input.
    pub kind: ValueKind,
    /// Reference input samples, parallel to the parent golden's `time`.
    pub samples: Vec<Sample>,
}

impl InputSeries {
    /// Convenience constructor.
    #[must_use]
    pub fn new(name: &'static str, kind: ValueKind, samples: Vec<Sample>) -> Self {
        Self {
            name,
            kind,
            samples,
        }
    }
}

/// One emitted golden: a single output signal of a single block instance.
pub struct Golden {
    /// CDL class path, e.g. `CDL.Reals.Add`.
    pub class_path: &'static str,
    /// Optional behavioral scenario discriminator for multiple goldens of the same class path.
    pub scenario: Option<&'static str>,
    /// Output connector / signal name, e.g. `y`, `passed`, `up`.
    pub signal: &'static str,
    /// Declared value kind of this signal.
    pub kind: ValueKind,
    /// Time samples in seconds (non-decreasing), parallel to `samples`.
    pub time: Vec<f64>,
    /// Reference samples, parallel to `time`.
    pub samples: Vec<Sample>,
    /// Input columns that produced this output, parallel to `time`.
    pub inputs: Vec<InputSeries>,
    /// Human description of the input trace / parameters (for provenance).
    pub input_desc: String,
    /// Short statement of the source or profile rule used for provenance.
    pub rule_desc: String,
    /// Extra provenance fields for scenario-specific audit notes.
    pub extra_provenance: Vec<(&'static str, String)>,
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
            scenario: None,
            signal,
            kind,
            time,
            samples,
            inputs: Vec::new(),
            input_desc: input_desc.into(),
            rule_desc: rule_desc.into(),
            extra_provenance: Vec::new(),
        }
    }

    /// Attach a behavioral scenario discriminator, used when one class has multiple goldens.
    #[must_use]
    pub fn with_scenario(mut self, scenario: &'static str) -> Self {
        self.scenario = Some(scenario);
        self
    }

    /// Attach machine-readable input columns to this golden.
    ///
    /// # Panics
    /// Panics if any input column length differs from the golden's time length.
    #[must_use]
    pub fn with_inputs(mut self, inputs: Vec<InputSeries>) -> Self {
        for input in &inputs {
            assert_eq!(
                self.time.len(),
                input.samples.len(),
                "{}.{} input {} length mismatch",
                self.class_path,
                self.signal,
                input.name
            );
        }
        self.inputs = inputs;
        self
    }

    /// Attach an extra provenance JSON string field.
    #[must_use]
    pub fn with_provenance(mut self, key: &'static str, value: impl Into<String>) -> Self {
        assert!(
            !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "provenance key must be a non-empty JSON identifier fragment"
        );
        self.extra_provenance.push((key, value.into()));
        self
    }
}
