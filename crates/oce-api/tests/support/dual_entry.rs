//! Two entries, one frame driver: the named path (`prepare_frame` then `execute_frame`) and the
//! slot path (`frame_slots` once, then `execute_slots`) on two loads of the same CXF, stepped
//! together and compared after every attempt (issue #444).
//!
//! A step submits the same frame to both: the slot path receives `values` in slot order, the named
//! path the same values keyed by `FrameSlots::inputs()` paths (so a short vector omits the
//! trailing inputs on both). The two must agree on the outcome — the same refusal (by `Debug`,
//! which names the variant, every path and every time) or the same accepted sequence, time bits,
//! output identities and values bit for bit, and warnings — and afterwards on the raw state words
//! and every boundary output's latest value. [`DualEntry::compare_state`] adds the durable state
//! snapshot bytes (every connector value, state word and clock); callers run it after every
//! refusal and at intervals, because a snapshot costs milliseconds in debug codegen. The private
//! `blob_dual_path_tests.rs` compares the slot entry's whole `RunState` after every frame.

#![allow(dead_code)]

use oce_api::{CompletedFrame, Engine, FrameSlots, NonFiniteInputPolicy, OcError, Value};

/// What one step produced, identical on both entries.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// Accepted: the sequence and the outputs in slot order.
    Accepted {
        /// The accepted-frame sequence.
        sequence: u64,
        /// Boundary outputs in output-slot order.
        outputs: Vec<Value>,
        /// The number of warnings.
        warnings: usize,
    },
    /// Refused with this error (`Debug` text).
    Refused(String),
}

/// A named-path engine and a slot-path engine of the same executable.
pub(crate) struct DualEntry {
    pub(crate) named: Engine,
    pub(crate) slotted: Engine,
    pub(crate) slots: FrameSlots,
}

fn bits(value: &Value) -> String {
    match value {
        Value::Real(x) => format!("Real:{:016x}", x.to_bits()),
        other => format!("{other:?}"),
    }
}

impl DualEntry {
    /// Load `cxf` twice and resolve the slot map once.
    pub(crate) fn load(cxf: &[u8]) -> Self {
        let mut named = Engine::new();
        named.load_cxf(cxf).unwrap();
        let mut slotted = Engine::new();
        slotted.load_cxf(cxf).unwrap();
        let slots = slotted.frame_slots().unwrap();
        Self {
            named,
            slotted,
            slots,
        }
    }

    /// Select the same non-finite input policy on both engines.
    pub(crate) fn set_policy(&mut self, policy: NonFiniteInputPolicy) {
        self.named.set_non_finite_input_policy(policy);
        self.slotted.set_non_finite_input_policy(policy);
    }

    /// The named path's attempt: `values` keyed by the slot paths (`zip` drops the tail of a short
    /// vector, which is the omission the slot path reports as missing).
    fn named_attempt(&mut self, time: f64, values: &[Value]) -> Result<CompletedFrame, OcError> {
        let entries: Vec<(&str, Value)> = self
            .slots
            .inputs()
            .iter()
            .zip(values)
            .map(|(definition, value)| (definition.path.as_str(), value.clone()))
            .collect();
        let plan = self.named.prepare_frame(time, &entries)?;
        self.named.execute_frame(plan)
    }

    /// Submit one frame to both entries and require the same outcome, state words and latest
    /// outputs.
    pub(crate) fn step(&mut self, time: f64, values: &[Value]) -> Result<Outcome, String> {
        assert!(
            values.len() <= self.slots.inputs().len(),
            "the named path has no spelling for an extra slot"
        );
        let named = self.named_attempt(time, values);
        let slotted = self
            .slotted
            .execute_slots(&self.slots, time, values)
            .map(|frame| {
                (
                    frame.sequence(),
                    frame.time().to_bits(),
                    frame.outputs().cloned().collect::<Vec<_>>(),
                    format!("{:?}", frame.diagnostics()),
                    frame.output_count(),
                )
            });
        let outcome = match (named, slotted) {
            (Ok(completed), Ok((sequence, time_bits, outputs, diagnostics, count))) => {
                if completed.sequence() != sequence || completed.time().to_bits() != time_bits {
                    return Err(format!(
                        "sequence/time: named {} {:x}, slot {sequence} {time_bits:x}",
                        completed.sequence(),
                        completed.time().to_bits()
                    ));
                }
                if count != outputs.len() || completed.outputs().len() != outputs.len() {
                    return Err("output count".into());
                }
                for (slot, ((path, value), slot_value)) in
                    completed.outputs().iter().zip(&outputs).enumerate()
                {
                    if self.slots.outputs()[slot] != *path {
                        return Err(format!("output slot {slot}: {path} vs the map's"));
                    }
                    if !value.bit_eq(slot_value) {
                        return Err(format!(
                            "{path}: named {} vs slot {}",
                            bits(value),
                            bits(slot_value)
                        ));
                    }
                }
                if format!("{:?}", completed.diagnostics()) != diagnostics {
                    return Err("warnings differ".into());
                }
                Outcome::Accepted {
                    sequence,
                    outputs,
                    warnings: completed.diagnostics().len(),
                }
            }
            (Err(named), Err(slotted)) => {
                let (named, slotted) = (format!("{named:?}"), format!("{slotted:?}"));
                if named != slotted {
                    return Err(format!("refusals differ: named {named}, slot {slotted}"));
                }
                Outcome::Refused(named)
            }
            (named, slotted) => {
                return Err(format!(
                    "one entry accepted: named {:?}, slot {:?}",
                    named.map(|c| c.sequence()),
                    slotted.map(|s| s.0)
                ));
            }
        };
        self.compare_words_and_outputs()?;
        Ok(outcome)
    }

    /// The two engines' durable snapshot bytes (every connector value, state word and clock), then
    /// [`Self::compare_words_and_outputs`]. Capturing a snapshot costs milliseconds in debug
    /// codegen, so corpus runs call this at intervals and after every refusal.
    pub(crate) fn compare_state(&self) -> Result<(), String> {
        let named = self.named.state_snapshot().map_err(|e| e.to_string())?;
        let slotted = self.slotted.state_snapshot().map_err(|e| e.to_string())?;
        if named.as_bytes() != slotted.as_bytes() {
            return Err("state snapshot bytes differ".into());
        }
        self.compare_words_and_outputs()
    }

    /// The two engines' raw state words and every boundary output's latest value, bit for bit.
    pub(crate) fn compare_words_and_outputs(&self) -> Result<(), String> {
        if self.named.state_words() != self.slotted.state_words() {
            return Err("state words differ".into());
        }
        for path in self.slots.outputs() {
            let (a, b) = (
                self.named.get_output(path).map_err(|e| e.to_string())?,
                self.slotted.get_output(path).map_err(|e| e.to_string())?,
            );
            if !a.bit_eq(&b) {
                return Err(format!("latest {path}: {} vs {}", bits(&a), bits(&b)));
            }
        }
        Ok(())
    }
}
