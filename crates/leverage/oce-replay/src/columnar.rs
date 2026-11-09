//! The columnar adapter: one time column and one owned typed column per input, the numpy and
//! pandas shape. It owns its columns, so a binding copies foreign arrays in before the run and no
//! foreign buffer is borrowed across a released lock.

use oce_api::{Value, ValueType};

use crate::source::Row;
use crate::{ColumnKind, FrameSource, InputMap, MapId, ReplayLimits, SourceError, TimeColumn};

/// One owned typed column.
#[derive(Clone, Debug)]
pub enum Column {
    /// `f64` values; NaN is missing, ±∞ is a value.
    Real(Vec<f64>),
    /// `i64` values with an optional validity mask (`false` is missing).
    Integer {
        /// The values.
        values: Vec<i64>,
        /// One flag per row, or `None` when every row is valid.
        valid: Option<Vec<bool>>,
    },
    /// `bool` values with an optional validity mask.
    Boolean {
        /// The values.
        values: Vec<bool>,
        /// One flag per row, or `None` when every row is valid.
        valid: Option<Vec<bool>>,
    },
    /// One-based ordinals of the named class with an optional validity mask.
    Enum {
        /// The enumeration class path; must equal the map's column spec.
        class: String,
        /// The ordinals.
        ordinals: Vec<u32>,
        /// One flag per row, or `None` when every row is valid.
        valid: Option<Vec<bool>>,
    },
}

impl Column {
    fn len(&self) -> usize {
        match self {
            Self::Real(values) => values.len(),
            Self::Integer { values, .. } => values.len(),
            Self::Boolean { values, .. } => values.len(),
            Self::Enum { ordinals, .. } => ordinals.len(),
        }
    }

    fn valid(&self) -> Option<&Vec<bool>> {
        match self {
            Self::Real(_) => None,
            Self::Integer { valid, .. }
            | Self::Boolean { valid, .. }
            | Self::Enum { valid, .. } => valid.as_ref(),
        }
    }

    fn matches(&self, kind: &ColumnKind) -> bool {
        match (self, kind) {
            (Self::Real(_), ColumnKind::Real)
            | (Self::Integer { .. }, ColumnKind::Integer)
            | (Self::Boolean { .. }, ColumnKind::Boolean) => true,
            (Self::Enum { class, .. }, ColumnKind::Enum(expected)) => class == expected,
            _ => false,
        }
    }

    /// Bytes held: the value width per row plus one byte per validity flag.
    fn bytes(&self) -> u64 {
        let width: u64 = match self {
            Self::Real(_) | Self::Integer { .. } => 8,
            Self::Boolean { .. } => 1,
            Self::Enum { .. } => 4,
        };
        let rows = self.len() as u64;
        rows * width + self.valid().map_or(0, |v| v.len() as u64)
    }

    fn cell(&self, row: usize, value_type: ValueType) -> Option<Value> {
        if self.valid().is_some_and(|valid| !valid[row]) {
            return None;
        }
        match (self, value_type) {
            (Self::Real(values), _) => Some(values[row]).filter(|x| !x.is_nan()).map(Value::Real),
            (Self::Integer { values, .. }, _) => Some(Value::Integer(values[row])),
            (Self::Boolean { values, .. }, _) => Some(Value::Boolean(values[row])),
            (Self::Enum { ordinals, .. }, ValueType::Enum(class)) => Some(Value::Enum {
                class,
                ordinal: ordinals[row],
            }),
            // `matches` and the map's kind check make this unreachable; a missing cell is the
            // conservative answer.
            (Self::Enum { .. }, _) => None,
        }
    }
}

/// Owned columns bound to an [`InputMap`], yielding rows by slot.
///
/// Bounded at construction: rows by `max_rows`, columns (the time column included) by
/// `max_columns`, and the bytes held (8 per time and Real or Integer cell, 4 per Enum, 1 per
/// Boolean and per validity flag) by `max_source_bytes`. Reading a row allocates nothing.
#[derive(Debug)]
pub struct ColumnarSource {
    map: MapId,
    time: TimeColumn,
    columns: Vec<Column>,
    types: Vec<ValueType>,
    cells: Vec<Option<Value>>,
    row: usize,
}

impl ColumnarSource {
    /// Bind `columns` (by source column name, as in the map) and `time` to `map`.
    ///
    /// # Errors
    /// [`SourceError::Limit`] for limits above their maxima; `TooManyColumns`; `UnknownColumn`
    /// for a column the map does not use; `DuplicateColumn`; `MissingColumn` for a mapped column
    /// not given (first in slot order); `ColumnKind` for a kind or enum class differing from the
    /// spec; `LengthMismatch` for a column (or validity mask) whose length is not the time
    /// column's; `TooManyRows`; `TooManyBytes`.
    pub fn new(
        map: &InputMap,
        time: TimeColumn,
        columns: Vec<(String, Column)>,
        limits: &ReplayLimits,
    ) -> Result<Self, SourceError> {
        limits.validate()?;
        if columns.len() as u64 + 1 > u64::from(limits.max_columns) {
            return Err(SourceError::TooManyColumns {
                limit: limits.max_columns,
            });
        }
        let mut by_slot: Vec<Option<Column>> = vec![None; map.len()];
        for (name, column) in columns {
            let Some(slot) = map.slot_of_column(&name) else {
                return Err(SourceError::UnknownColumn { column: name });
            };
            if by_slot[slot].is_some() {
                return Err(SourceError::DuplicateColumn { column: name });
            }
            if !column.matches(map.kind(slot)) {
                return Err(SourceError::ColumnKind { column: name });
            }
            if column.len() != time.len() || column.valid().is_some_and(|v| v.len() != time.len()) {
                return Err(SourceError::LengthMismatch { column: name });
            }
            by_slot[slot] = Some(column);
        }
        let mut owned = Vec::with_capacity(map.len());
        for (slot, column) in by_slot.into_iter().enumerate() {
            match column {
                Some(column) => owned.push(column),
                None => {
                    return Err(SourceError::MissingColumn {
                        column: map.column(slot).to_owned(),
                    });
                }
            }
        }
        if time.len() as u64 > limits.max_rows {
            return Err(SourceError::TooManyRows {
                limit: limits.max_rows,
            });
        }
        let bytes = time.len() as u64 * 8 + owned.iter().map(Column::bytes).sum::<u64>();
        if bytes > limits.max_source_bytes {
            return Err(SourceError::TooManyBytes {
                limit: limits.max_source_bytes,
            });
        }
        Ok(Self {
            map: map.id().clone(),
            time,
            columns: owned,
            types: map.inputs().iter().map(|d| d.value_type).collect(),
            cells: vec![None; map.len()],
            row: 0,
        })
    }

    /// The number of rows.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.time.len()
    }
}

impl FrameSource for ColumnarSource {
    fn map_id(&self) -> &MapId {
        &self.map
    }

    fn next_row(&mut self) -> Result<Option<Row<'_>>, SourceError> {
        if self.row >= self.time.len() {
            return Ok(None);
        }
        let row = self.row;
        self.row += 1;
        for ((cell, column), value_type) in
            self.cells.iter_mut().zip(&self.columns).zip(&self.types)
        {
            *cell = column.cell(row, *value_type);
        }
        Ok(Some((self.time.seconds(row), &self.cells)))
    }
}
