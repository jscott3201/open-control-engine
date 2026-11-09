//! The policy stage and one frame attempt: per slot, an observed value, a substitution, or a
//! refusal or skip; then the engine's slot entry; then exactly one trace row.
//!
//! Precedence within a frame: if any missing input cannot be substituted under `Refuse` or an
//! expired `HoldLast`, the frame is `RefusedMissing`, naming the first such input in slot order;
//! otherwise if any missing input is `SkipFrame`, it is `Skipped`; otherwise it is submitted, and
//! the engine's verdict decides between `Executed`/`Substituted` and `RefusedEngine`. The
//! substitution mask records every input substituted on the row whatever its status, and retained
//! input columns hold every value resolved on the row (zero bits where none was).

use oce_api::{InputDefinition, SlotFrame, Value, ValueType};

use crate::{
    DiagnosticRow, FrameSource, FrameStatus, InputMap, MissingPolicy, RefusalCause, RefusalRow,
    Replay, ReplayError, Trace, TraceColumn, TraceKind, TraceOutput,
};

/// What one attempt left in the trace.
pub(crate) enum Attempt {
    /// A row that did not refuse (executed, substituted or skipped).
    Recorded,
    /// A refused row and its cause.
    Refused(RefusalCause),
}

#[derive(Default)]
struct Resolution {
    refused: Option<usize>,
    skip: bool,
    substituted: bool,
}

/// The per-slot working buffers, allocated once per run.
pub(crate) struct Cells {
    /// This frame's current sample per slot (`None`: missing).
    pub(crate) current: Vec<Option<Value>>,
    /// The latest observed sample per slot and its time; never a substituted value.
    observed: Vec<Option<(Value, f64)>>,
    /// The values submitted, in slot order.
    pub(crate) values: Vec<Value>,
    resolved: Vec<bool>,
    mask_row: Vec<u8>,
}

fn zero_of(value_type: ValueType) -> Value {
    match value_type {
        ValueType::Real => Value::Real(0.0),
        ValueType::Integer => Value::Integer(0),
        ValueType::Boolean => Value::Boolean(false),
        ValueType::String => Value::String("".into()),
        ValueType::Enum(class) => Value::Enum { class, ordinal: 0 },
    }
}

impl Cells {
    pub(crate) fn new(inputs: &[InputDefinition]) -> Self {
        let n = inputs.len();
        Self {
            current: vec![None; n],
            observed: vec![None; n],
            // Never submitted unless every slot resolved this frame; a frame with an unresolved
            // slot is refused or skipped before the engine.
            values: inputs.iter().map(|d| zero_of(d.value_type)).collect(),
            resolved: vec![false; n],
            mask_row: vec![0; n.div_ceil(8)],
        }
    }

    /// Record this row's present cells as observations at `t` (time base `Recorded`).
    pub(crate) fn observe_current(&mut self, t: f64) {
        for (observed, cell) in self.observed.iter_mut().zip(&self.current) {
            if let Some(value) = cell {
                *observed = Some((value.clone(), t));
            }
        }
    }

    /// Record a source row's present cells as observations at `t` (time base `Grid`).
    pub(crate) fn observe(&mut self, row: &[Option<Value>], t: f64) {
        for (observed, cell) in self.observed.iter_mut().zip(row) {
            if let Some(value) = cell {
                *observed = Some((value.clone(), t));
            }
        }
    }

    /// Under `Grid`: a slot is current when its latest observation arrived after `previous`
    /// (the previous tick), or always once observed for a signal source.
    pub(crate) fn current_from_observed(&mut self, previous: f64, signals: bool) {
        for (current, observed) in self.current.iter_mut().zip(&self.observed) {
            *current = match observed {
                Some((value, t)) if signals || *t > previous => Some(value.clone()),
                _ => None,
            };
        }
    }

    fn resolve(&mut self, map: &InputMap, t: f64) -> Resolution {
        let mut r = Resolution::default();
        self.mask_row.fill(0);
        for slot in 0..self.values.len() {
            if let Some(value) = &self.current[slot] {
                self.values[slot].clone_from(value);
                self.resolved[slot] = true;
                continue;
            }
            let substitute = match map.policy(slot) {
                MissingPolicy::Refuse => None,
                MissingPolicy::HoldLast { max_age } => self.observed[slot]
                    .as_ref()
                    .filter(|(_, sampled)| t - sampled <= *max_age)
                    .map(|(value, _)| value),
                MissingPolicy::Fallback(value) => Some(value),
                MissingPolicy::SkipFrame => {
                    r.skip = true;
                    self.resolved[slot] = false;
                    continue;
                }
            };
            match substitute {
                Some(value) => {
                    self.values[slot].clone_from(value);
                    self.resolved[slot] = true;
                    self.mask_row[slot / 8] |= 1 << (slot % 8);
                    r.substituted = true;
                }
                None => {
                    self.resolved[slot] = false;
                    r.refused.get_or_insert(slot);
                }
            }
        }
        r
    }
}

/// Fix the output columns on the first accepted frame (decision 16), check every later frame
/// against them, and append the frame's outputs and warnings.
fn record_outputs(
    trace: &mut Trace,
    paths: &[String],
    frame: &SlotFrame<'_>,
    row: u64,
) -> Result<(), ReplayError> {
    if trace.outputs.is_empty() && frame.output_count() > 0 {
        let rows = trace.times.len();
        let room = trace.times.capacity().saturating_sub(rows);
        trace.meta.outputs.reserve_exact(frame.output_count());
        trace.outputs.reserve_exact(frame.output_count());
        for (slot, value) in frame.outputs().enumerate() {
            let path = paths.get(slot).cloned().unwrap_or_default();
            let Some(kind) = TraceKind::of(value) else {
                return Err(ReplayError::UnsupportedOutputType { path });
            };
            let column = TraceColumn::new(kind, rows, room);
            trace.meta.outputs.push(TraceOutput {
                path: path.clone(),
                kind,
                enum_class_index: match value {
                    Value::Enum { class, .. } => Some(class.0),
                    _ => None,
                },
            });
            trace.outputs.push((path, column));
        }
    }
    let same_set = frame.output_count() == trace.outputs.len()
        && frame
            .outputs()
            .zip(&trace.outputs)
            .all(|(value, (_, column))| TraceKind::of(value) == Some(column.kind()));
    if !same_set {
        return Err(ReplayError::OutputSetChanged { frame: row });
    }
    for (value, (_, column)) in frame.outputs().zip(trace.outputs.iter_mut()) {
        column.push(value);
    }
    for event in frame.diagnostics() {
        trace.sparse_bytes += 24 + (event.block.len() + event.message.len()) as u64;
        trace.diagnostics.push(DiagnosticRow {
            frame: row,
            block: event.block.clone(),
            message: event.message.clone(),
            time: event.t,
            level: event.level,
        });
    }
    Ok(())
}

impl<S: FrameSource> Replay<S> {
    /// Resolve, submit (unless refused or skipped) and write exactly one row at `t`.
    pub(crate) fn attempt(&mut self, t: f64) -> Result<Attempt, ReplayError> {
        let row = self.trace.rows() as u64;
        let resolution = self.cells.resolve(&self.map, t);
        let mut refusal = None;
        let status = if let Some(slot) = resolution.refused {
            refusal = Some(RefusalCause::MissingInput {
                slot,
                path: self.map.inputs()[slot].path.clone(),
            });
            FrameStatus::RefusedMissing
        } else if resolution.skip {
            FrameStatus::Skipped
        } else {
            match self
                .engine
                .execute_slots(&self.slots, t, &self.cells.values)
            {
                Ok(frame) => {
                    record_outputs(&mut self.trace, self.slots.outputs(), &frame, row)?;
                    if resolution.substituted {
                        FrameStatus::Substituted
                    } else {
                        FrameStatus::Executed
                    }
                }
                Err(error) => {
                    refusal = Some(RefusalCause::Engine {
                        message: error.to_string(),
                    });
                    FrameStatus::RefusedEngine
                }
            }
        };
        let trace = &mut self.trace;
        if !status.executed() {
            trace.outputs.iter_mut().for_each(|(_, c)| c.push_zero());
        }
        trace.times.push(t);
        trace.status.push(status as u8);
        if let Some(mask) = &mut trace.mask {
            mask.extend_from_slice(&self.cells.mask_row);
        }
        if let Some(inputs) = &mut trace.inputs {
            for (slot, (_, column)) in inputs.iter_mut().enumerate() {
                // A custom source can lend a cell of the wrong type (the engine then refuses the
                // frame); its column keeps zero bits so every column stays one cell per row.
                if !(self.cells.resolved[slot] && column.push(&self.cells.values[slot])) {
                    column.push_zero();
                }
            }
        }
        match refusal {
            Some(cause) => {
                trace.sparse_bytes += 16
                    + match &cause {
                        RefusalCause::MissingInput { path, .. } => path.len(),
                        RefusalCause::Engine { message } => message.len(),
                    } as u64;
                trace.refusals.push(RefusalRow {
                    frame: row,
                    cause: cause.clone(),
                });
                Ok(Attempt::Refused(cause))
            }
            None => Ok(Attempt::Recorded),
        }
    }
}
