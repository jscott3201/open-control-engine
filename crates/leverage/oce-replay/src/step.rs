//! The step-list adapter: per input, a list of `(t, value)` changes. A step list is a signal, not
//! a sample: its value is defined on every interval it covers and is never missing after its
//! first step (decision 11). A constant is a one-step list.

use oce_api::Value;

use crate::map::value_has_type;
use crate::source::Row;
use crate::{FrameSource, InputMap, MapId, ReplayLimits, SourceError, SourceKind};

/// Per-slot change lists bound to an [`InputMap`].
///
/// Rows are the sorted union of every list's step times (and any declared instants): one row per
/// distinct time, each cell the latest step at or before it, `None` before an input's first step.
/// A step to an unchanged value is legal and still makes its time a row. Bounded at construction: each list by `max_steps`,
/// the union by `max_rows`, the lists (the time column counted) by `max_columns`. Reading a row
/// allocates nothing.
#[derive(Debug)]
pub struct StepSource {
    map: MapId,
    steps: Vec<Vec<(f64, Value)>>,
    times: Vec<f64>,
    cursors: Vec<usize>,
    cells: Vec<Option<Value>>,
    row: usize,
}

impl StepSource {
    /// Bind `steps` (source column name → change list) to `map`.
    ///
    /// # Errors
    /// [`SourceError::Limit`]; `TooManyColumns`; `UnknownColumn`; `DuplicateColumn`;
    /// `TooManySteps`; `BadStep` for a non-finite or non-increasing time, a NaN value or a value
    /// whose type is not the input's; `MissingColumn` for a mapped column with no list (first in
    /// slot order); `TooManyRows` for a union of times above `max_rows`.
    pub fn new(
        map: &InputMap,
        steps: Vec<(String, Vec<(f64, Value)>)>,
        limits: &ReplayLimits,
    ) -> Result<Self, SourceError> {
        Self::with_instants(map, steps, Vec::new(), limits)
    }

    /// As [`Self::new`], with `instants` added to the rows: evaluation times at which no input
    /// changes (a run of a model with no inputs consists of these alone). Each must be finite.
    ///
    /// # Errors
    /// As [`Self::new`]; a non-finite instant is `BadStep` for the column `(instants)`.
    pub fn with_instants(
        map: &InputMap,
        steps: Vec<(String, Vec<(f64, Value)>)>,
        instants: Vec<f64>,
        limits: &ReplayLimits,
    ) -> Result<Self, SourceError> {
        limits.validate()?;
        if let Some(step) = instants.iter().position(|t| !t.is_finite()) {
            return Err(SourceError::BadStep {
                column: "(instants)".to_owned(),
                step,
            });
        }
        if steps.len() as u64 + 1 > u64::from(limits.max_columns) {
            return Err(SourceError::TooManyColumns {
                limit: limits.max_columns,
            });
        }
        let mut by_slot: Vec<Option<Vec<(f64, Value)>>> = vec![None; map.len()];
        for (name, list) in steps {
            let Some(slot) = map.slot_of_column(&name) else {
                return Err(SourceError::UnknownColumn { column: name });
            };
            if by_slot[slot].is_some() {
                return Err(SourceError::DuplicateColumn { column: name });
            }
            if list.len() as u64 > limits.max_steps {
                return Err(SourceError::TooManySteps {
                    column: name,
                    limit: limits.max_steps,
                });
            }
            let value_type = map.inputs()[slot].value_type;
            for (index, (t, value)) in list.iter().enumerate() {
                let increasing = index == 0 || list[index - 1].0 < *t;
                let nan = matches!(value, Value::Real(x) if x.is_nan());
                if !t.is_finite() || !increasing || nan || !value_has_type(value, value_type) {
                    return Err(SourceError::BadStep {
                        column: name,
                        step: index,
                    });
                }
            }
            by_slot[slot] = Some(list);
        }
        let mut lists = Vec::with_capacity(map.len());
        for (slot, list) in by_slot.into_iter().enumerate() {
            match list {
                Some(list) => lists.push(list),
                None => {
                    return Err(SourceError::MissingColumn {
                        column: map.column(slot).to_owned(),
                    });
                }
            }
        }
        let mut times: Vec<f64> = lists.iter().flatten().map(|(t, _)| *t).collect();
        times.extend(instants);
        times.sort_by(f64::total_cmp);
        // -0.0 and 0.0 are the same instant; keep the first spelling in total order.
        times.dedup_by(|b, a| a == b);
        if times.len() as u64 > limits.max_rows {
            return Err(SourceError::TooManyRows {
                limit: limits.max_rows,
            });
        }
        Ok(Self {
            map: map.id().clone(),
            cursors: vec![0; lists.len()],
            cells: vec![None; lists.len()],
            steps: lists,
            times,
            row: 0,
        })
    }

    /// The number of rows (distinct step times).
    #[must_use]
    pub fn rows(&self) -> usize {
        self.times.len()
    }
}

impl FrameSource for StepSource {
    fn map_id(&self) -> &MapId {
        &self.map
    }

    fn kind(&self) -> SourceKind {
        SourceKind::Signals
    }

    fn next_row(&mut self) -> Result<Option<Row<'_>>, SourceError> {
        let Some(&t) = self.times.get(self.row) else {
            return Ok(None);
        };
        self.row += 1;
        for ((list, cursor), cell) in self
            .steps
            .iter()
            .zip(self.cursors.iter_mut())
            .zip(self.cells.iter_mut())
        {
            while *cursor < list.len() && list[*cursor].0 <= t {
                *cell = Some(list[*cursor].1.clone());
                *cursor += 1;
            }
        }
        Ok(Some((t, &self.cells)))
    }
}
