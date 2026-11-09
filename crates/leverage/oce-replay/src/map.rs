//! The one-time name→slot mapping (design note decision 4): source columns resolved to engine
//! input slots, with each slot's missing-sample policy.
//!
//! Slot `i` is row `i` of `Engine::input_definitions()`, canonical lexical UTF-8 path order, which
//! is also `FrameSlots::inputs()` order and the compiled-schedule blob's input order. After
//! [`InputMap::resolve`] no frame touches a string.

use std::sync::Arc;

use oce_api::{InputDefinition, Value, ValueType};

use crate::{MapError, MissingPolicy};

/// The kind of a source column. An Enum column names its class path, which the trace records in
/// its metadata. That path is a **source-declared, unverified label**: the facade exposes no
/// lookup from the engine's enum class to its path, so a wrong path (`Enum("Wrong.Class")`) on an
/// Enum input resolves and is recorded as given. Verification waits for that lookup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnKind {
    /// IEEE-754 `f64`; NaN is missing.
    Real,
    /// `i64`.
    Integer,
    /// `bool`.
    Boolean,
    /// A one-based ordinal of the named enumeration class.
    Enum(String),
}

impl ColumnKind {
    /// Whether a column of this kind can carry an input of `value_type`. Kinds match exactly; no
    /// numeric widening.
    #[must_use]
    pub fn carries(&self, value_type: ValueType) -> bool {
        matches!(
            (self, value_type),
            (Self::Real, ValueType::Real)
                | (Self::Integer, ValueType::Integer)
                | (Self::Boolean, ValueType::Boolean)
                | (Self::Enum(_), ValueType::Enum(_))
        )
    }
}

/// One source column: its name and kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnSpec {
    /// The column name as the source spells it.
    pub name: String,
    /// The column kind.
    pub kind: ColumnKind,
}

impl ColumnSpec {
    /// A column spec.
    #[must_use]
    pub fn new(name: impl Into<String>, kind: ColumnKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }
}

/// Source column → engine input. A target is a canonical input path or a suffix that matches
/// exactly one path at a `.` boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColumnMap {
    /// `(source column, input path or suffix)` pairs.
    pub entries: Vec<(String, String)>,
}

impl ColumnMap {
    /// Map every column to the target of the same name (exact path or unique suffix).
    #[must_use]
    pub fn by_name(columns: &[ColumnSpec]) -> Self {
        Self {
            entries: columns
                .iter()
                .map(|c| (c.name.clone(), c.name.clone()))
                .collect(),
        }
    }
}

/// The identity of one resolved [`InputMap`]: a source bound to a map carries it, and
/// [`crate::Replay::new`] refuses a source bound to another map. Clones compare equal.
#[derive(Clone, Debug)]
pub struct MapId(Arc<()>);

impl PartialEq for MapId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for MapId {}

/// Names resolved to slots once; slot order is `Engine::input_definitions()` order.
///
/// Holds, per slot, the engine's input definition, the source column that supplies it, that
/// column's kind and the slot's missing-sample policy (the run-level policy or its override).
/// Building it allocates; reading it does not.
#[derive(Clone, Debug)]
pub struct InputMap {
    id: MapId,
    inputs: Vec<InputDefinition>,
    columns: Vec<String>,
    kinds: Vec<ColumnKind>,
    policies: Vec<MissingPolicy>,
    ignored: Vec<String>,
}

impl InputMap {
    /// Resolve `map` over `inputs` (from `Engine::input_definitions()`) and the source's
    /// `columns`, with the run-level `missing` policy and per-input `overrides` (target path or
    /// unique suffix → policy).
    ///
    /// Columns the map does not name are ignored and listed by [`Self::ignored_columns`].
    ///
    /// # Errors
    /// A [`MapError`]: inputs out of order or of String type, duplicate column specs, a map entry
    /// naming an unknown column, an unknown or ambiguous target, two columns on one input, a kind
    /// that cannot carry its input, an input no column supplies (first in slot order), a
    /// run-level `Fallback`, an invalid `max_age`, two overrides on one input, or a fallback of
    /// the wrong type or outside the input's domain (NaN included). Checked in that order.
    pub fn resolve(
        inputs: &[InputDefinition],
        columns: &[ColumnSpec],
        map: &ColumnMap,
        missing: &MissingPolicy,
        overrides: &[(String, MissingPolicy)],
    ) -> Result<Self, MapError> {
        for pair in inputs.windows(2) {
            if pair[0].path >= pair[1].path {
                return Err(MapError::InputsNotCanonical {
                    path: pair[1].path.clone(),
                });
            }
        }
        if let Some(input) = inputs
            .iter()
            .find(|d| matches!(d.value_type, ValueType::String))
        {
            return Err(MapError::UnsupportedInputType {
                path: input.path.clone(),
            });
        }
        for (index, column) in columns.iter().enumerate() {
            if columns[..index].iter().any(|c| c.name == column.name) {
                return Err(MapError::DuplicateColumn {
                    column: column.name.clone(),
                });
            }
        }
        let mut slot_column: Vec<Option<usize>> = vec![None; inputs.len()];
        let mut named = vec![false; columns.len()];
        for (column_name, target) in &map.entries {
            let Some(column) = columns.iter().position(|c| &c.name == column_name) else {
                return Err(MapError::UnknownColumn {
                    column: column_name.clone(),
                });
            };
            let slot = resolve_target(inputs, target).map_err(|candidates| {
                if candidates.is_empty() {
                    MapError::UnknownTarget {
                        column: column_name.clone(),
                        target: target.clone(),
                    }
                } else {
                    MapError::AmbiguousTarget {
                        target: target.clone(),
                        candidates,
                    }
                }
            })?;
            if let Some(first) = slot_column[slot] {
                return Err(MapError::TwoColumnsOneInput {
                    input: inputs[slot].path.clone(),
                    first: columns[first].name.clone(),
                    second: column_name.clone(),
                });
            }
            if !columns[column].kind.carries(inputs[slot].value_type) {
                return Err(MapError::TypeMismatch {
                    column: column_name.clone(),
                    input: inputs[slot].path.clone(),
                });
            }
            slot_column[slot] = Some(column);
            named[column] = true;
        }
        let mut chosen = Vec::with_capacity(inputs.len());
        for (slot, column) in slot_column.iter().enumerate() {
            match column {
                Some(column) => chosen.push(*column),
                None => {
                    return Err(MapError::UnmappedInput {
                        path: inputs[slot].path.clone(),
                    });
                }
            }
        }
        let policies = resolve_policies(inputs, missing, overrides)?;
        Ok(Self {
            id: MapId(Arc::new(())),
            inputs: inputs.to_vec(),
            columns: chosen.iter().map(|&c| columns[c].name.clone()).collect(),
            kinds: chosen.iter().map(|&c| columns[c].kind.clone()).collect(),
            policies,
            ignored: columns
                .iter()
                .zip(&named)
                .filter(|(_, named)| !**named)
                .map(|(c, _)| c.name.clone())
                .collect(),
        })
    }

    /// The map's identity.
    #[must_use]
    pub fn id(&self) -> &MapId {
        &self.id
    }

    /// The number of input slots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    /// Whether the engine has no inputs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// The input definitions in slot order.
    #[must_use]
    pub fn inputs(&self) -> &[InputDefinition] {
        &self.inputs
    }

    /// The source column that supplies `slot`. Panics if `slot >= self.len()`.
    #[must_use]
    pub fn column(&self, slot: usize) -> &str {
        &self.columns[slot]
    }

    /// The kind of the column that supplies `slot`. Panics if `slot >= self.len()`.
    #[must_use]
    pub fn kind(&self, slot: usize) -> &ColumnKind {
        &self.kinds[slot]
    }

    /// The slot the source column `name` supplies, or `None` when the map does not use it.
    #[must_use]
    pub fn slot_of_column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    /// The missing-sample policy of `slot`. Panics if `slot >= self.len()`.
    #[must_use]
    pub fn policy(&self, slot: usize) -> &MissingPolicy {
        &self.policies[slot]
    }

    /// Source columns the map does not name, in the order given.
    #[must_use]
    pub fn ignored_columns(&self) -> &[String] {
        &self.ignored
    }

    /// Whether any slot's policy can substitute (the trace then carries a substitution mask).
    #[must_use]
    pub fn can_substitute(&self) -> bool {
        self.policies.iter().any(MissingPolicy::can_substitute)
    }

    /// Whether this map still describes `definitions`: the same paths, types and bounds in the
    /// same order. Bounds compare by value bits.
    #[must_use]
    pub fn matches(&self, definitions: &[InputDefinition]) -> bool {
        self.inputs.len() == definitions.len()
            && self.inputs.iter().zip(definitions).all(|(a, b)| {
                a.path == b.path
                    && a.value_type == b.value_type
                    && option_bits_eq(a.min.as_ref(), b.min.as_ref())
                    && option_bits_eq(a.max.as_ref(), b.max.as_ref())
            })
    }
}

/// Exact path, else the unique `.`-boundary suffix match. `Err` carries every candidate (empty
/// when none matched).
fn resolve_target(inputs: &[InputDefinition], target: &str) -> Result<usize, Vec<String>> {
    if let Ok(slot) = inputs.binary_search_by(|d| d.path.as_str().cmp(target)) {
        return Ok(slot);
    }
    if target.is_empty() {
        return Err(Vec::new());
    }
    let matches: Vec<usize> = inputs
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            d.path
                .strip_suffix(target)
                .is_some_and(|head| head.ends_with('.'))
        })
        .map(|(slot, _)| slot)
        .collect();
    match matches.as_slice() {
        [slot] => Ok(*slot),
        _ => Err(matches
            .iter()
            .map(|&slot| inputs[slot].path.clone())
            .collect()),
    }
}

fn valid_max_age(policy: &MissingPolicy) -> bool {
    match policy {
        MissingPolicy::HoldLast { max_age } => max_age.is_finite() && *max_age >= 0.0,
        _ => true,
    }
}

fn resolve_policies(
    inputs: &[InputDefinition],
    missing: &MissingPolicy,
    overrides: &[(String, MissingPolicy)],
) -> Result<Vec<MissingPolicy>, MapError> {
    if matches!(missing, MissingPolicy::Fallback(_)) {
        return Err(MapError::FallbackIsPerInput);
    }
    if !valid_max_age(missing) {
        return Err(MapError::InvalidMaxAge { input: None });
    }
    let mut policies = vec![missing.clone(); inputs.len()];
    let mut overridden = vec![false; inputs.len()];
    for (target, policy) in overrides {
        let slot = resolve_target(inputs, target).map_err(|candidates| {
            if candidates.is_empty() {
                MapError::UnknownTarget {
                    column: String::new(),
                    target: target.clone(),
                }
            } else {
                MapError::AmbiguousTarget {
                    target: target.clone(),
                    candidates,
                }
            }
        })?;
        let input = &inputs[slot];
        if overridden[slot] {
            return Err(MapError::OverrideTwice {
                input: input.path.clone(),
            });
        }
        if !valid_max_age(policy) {
            return Err(MapError::InvalidMaxAge {
                input: Some(input.path.clone()),
            });
        }
        if let MissingPolicy::Fallback(value) = policy {
            if !value_has_type(value, input.value_type) {
                return Err(MapError::FallbackType {
                    input: input.path.clone(),
                });
            }
            if !in_domain(value, input.min.as_ref(), input.max.as_ref()) {
                return Err(MapError::FallbackDomain {
                    input: input.path.clone(),
                });
            }
        }
        overridden[slot] = true;
        policies[slot] = policy.clone();
    }
    Ok(policies)
}

/// Whether `value` has exactly `value_type` (enum class included).
pub(crate) fn value_has_type(value: &Value, value_type: ValueType) -> bool {
    match (value, value_type) {
        (Value::Real(_), ValueType::Real)
        | (Value::Integer(_), ValueType::Integer)
        | (Value::Boolean(_), ValueType::Boolean)
        | (Value::String(_), ValueType::String) => true,
        (Value::Enum { class, .. }, ValueType::Enum(expected)) => *class == expected,
        _ => false,
    }
}

/// The inclusive domain check, by the same rule the engine applies: Real NaN never passes; an
/// absent bound admits everything on that side.
fn in_domain(value: &Value, min: Option<&Value>, max: Option<&Value>) -> bool {
    let above = |bound: &Value| match (value, bound) {
        (Value::Real(x), Value::Real(lo)) => x >= lo,
        (Value::Integer(x), Value::Integer(lo)) => x >= lo,
        (Value::Enum { ordinal: x, .. }, Value::Enum { ordinal: lo, .. }) => x >= lo,
        _ => true,
    };
    let below = |bound: &Value| match (value, bound) {
        (Value::Real(x), Value::Real(hi)) => x <= hi,
        (Value::Integer(x), Value::Integer(hi)) => x <= hi,
        (Value::Enum { ordinal: x, .. }, Value::Enum { ordinal: hi, .. }) => x <= hi,
        _ => true,
    };
    if matches!(value, Value::Real(x) if x.is_nan()) {
        return false;
    }
    min.is_none_or(above) && max.is_none_or(below)
}

/// Value equality by exact bits (Real by `to_bits`).
pub(crate) fn value_bits_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Real(x), Value::Real(y)) => x.to_bits() == y.to_bits(),
        (Value::Integer(x), Value::Integer(y)) => x == y,
        (Value::Boolean(x), Value::Boolean(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (
            Value::Enum {
                class: c1,
                ordinal: o1,
            },
            Value::Enum {
                class: c2,
                ordinal: o2,
            },
        ) => c1 == c2 && o1 == o2,
        _ => false,
    }
}

fn option_bits_eq(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => value_bits_eq(a, b),
        _ => false,
    }
}
