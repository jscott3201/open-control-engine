//! The live parameter table — tune-at-rest (`08` §4). CDL parameters are time-invariant and
//! changeable only at rest (CDL §7.4.2), so the facade exposes a typed, dotted-path-addressable
//! table with an explicit **halt → set → resume** lifecycle. Setting a parameter while ticking is a
//! usage error (`OcError::ParamWhileRunning`), not a silent live patch — a mid-horizon change would
//! break the determinism contract (CDL §7.16).
//!
//! The at-rest source of truth is each `BlockInstance.params` on the flat model (D1); this table is
//! a flat, addressable **mirror** over them. The tick reads only the in-memory folded block state,
//! never this table (perf §6). `resume` re-folds any edits back into the model and re-instantiates
//! the affected blocks.

use std::cmp::Ordering;
use std::collections::HashMap;

use oce_blocks::{ParamRule, lookup};
use oce_graph::allocate_state;
use oce_model::{BlockId, ModelGraph, Value, ValueType};
use oce_store::Store;

use crate::engine::{Engine, instantiate_blocks};
use crate::error::OcError;
use crate::sim::Outputs;

/// The engine's coarse lifecycle gate for tuning (`08` §4). `Running` is the post-load default;
/// `set_param` is permitted only in `Halted` (CDL §7.4.2 — params never change while advancing).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum RunMode {
    /// Tune-at-rest window: `set_param` is permitted.
    Halted,
    /// Normal advancing state (the post-load default): `set_param` is rejected.
    #[default]
    Running,
}

/// Declared, non-computational metadata for a parameter (CDL §7.4.1) — a flat, owned, backend-free
/// DTO (R-API-8). `value_type` is read from the value; `min`/`max` come from registry-published
/// class parameter rules where they can be represented as static single-parameter bounds; `unit`
/// and `quantity` remain `None` until per-parameter attribute provenance is carried on
/// `oce_model::ParamTable`. Surfaced as the third element of [`ParamTable::iter`].
#[derive(Clone, Debug, PartialEq)]
pub struct ParamAttrs {
    /// Structural type of the parameter.
    pub value_type: ValueType,
    /// Lower validation bound; `None` ⇒ unbounded below.
    pub min: Option<f64>,
    /// Upper validation bound; `None` ⇒ unbounded above.
    pub max: Option<f64>,
    /// SI computation unit; metadata only.
    pub unit: Option<String>,
    /// Physical quantity; metadata only.
    pub quantity: Option<String>,
}

/// One row of [`ParamTable`]: a dotted-path-addressed mirror of one `BlockInstance.params` entry.
/// Private — `block`/`name` are the write-back address (R-API-8: `BlockId` is an `oce-model` index,
/// kept internal and never yielded by `iter`).
#[derive(Clone, Debug)]
struct ParamEntry {
    path: String,
    block: BlockId,
    name: std::sync::Arc<str>,
    value: Value,
    declared: ParamAttrs,
}

/// Addressable, typed, store-free view of every `parameter` in the loaded model (`08` §4). Keyed by
/// dotted instance path. A **mirror**: the at-rest source of truth is each `BlockInstance.params`.
#[derive(Clone, Debug, Default)]
pub struct ParamTable {
    entries: Vec<ParamEntry>,
    index: HashMap<String, usize>,
}

impl ParamTable {
    /// Build the table from a flattened model — one row per `(block, parameter)`, keyed by the
    /// dotted `"<instance_iri>.<name>"` path (a synthetic `b<id>` base for an unnamed hand-built
    /// block). First-wins on a duplicate path (a well-formed model has unique instance paths).
    pub(crate) fn build_at_load(model: &ModelGraph) -> ParamTable {
        let mut entries = Vec::new();
        let mut index = HashMap::new();
        for blk in &model.blocks {
            let base = blk
                .instance_iri
                .as_deref()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("b{}", blk.id.0));
            for (name, value) in &blk.params.values {
                let path = format!("{base}.{name}");
                let (min, max) = static_real_bounds(&blk.class_iri, name);
                let declared = ParamAttrs {
                    value_type: value.value_type(),
                    min,
                    max,
                    unit: None,
                    quantity: None,
                };
                let idx = entries.len();
                index.entry(path.clone()).or_insert(idx);
                entries.push(ParamEntry {
                    path,
                    block: blk.id,
                    name: name.clone(),
                    value: value.clone(),
                    declared,
                });
            }
        }
        ParamTable { entries, index }
    }

    /// Owned enumeration yielding `(path, value, declared attributes)` (R-PUB-6; satisfies `08`
    /// §11.1 "(String, Value) plus declared attributes"). `ParamAttrs` is the flat DTO — never the
    /// `oce_model::Attrs` enum, which would leak the model's internal attribute shape (R-API-8).
    pub fn iter(&self) -> impl Iterator<Item = (String, Value, ParamAttrs)> + '_ {
        self.entries
            .iter()
            .map(|e| (e.path.clone(), e.value.clone(), e.declared.clone()))
    }

    /// Eager owned snapshot (R-PUB-6).
    #[must_use]
    pub fn to_vec(&self) -> Vec<(String, Value, ParamAttrs)> {
        self.iter().collect()
    }

    /// Number of parameters in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the model has no parameters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<S: Store> Engine<S> {
    /// Read the current value of a parameter by dotted instance path. Always allowed (read-only).
    ///
    /// # Errors
    /// [`OcError::UnknownPoint`] if `path` is not a parameter in the loaded model. Never panics.
    pub fn get_param(&self, path: &str) -> Result<Value, OcError> {
        let idx = *self
            .params
            .index
            .get(path)
            .ok_or_else(|| OcError::UnknownPoint(path.to_string()))?;
        // `idx` came from our own `index` map, so it is in-bounds (R-ERR-1).
        Ok(self.params.entries[idx].value.clone())
    }

    /// The full live parameter table (host GUI / owned enumeration via [`ParamTable::iter`]).
    #[must_use]
    pub fn params(&self) -> &ParamTable {
        &self.params
    }

    /// The current tune-at-rest lifecycle mode.
    #[must_use]
    pub fn mode(&self) -> RunMode {
        self.mode
    }

    /// Halt: enter the tune-at-rest window so `set_param` is accepted. Idempotent.
    ///
    /// # Errors
    /// Infallible today; returns `Result` for forward-compatibility (a future durable adapter may
    /// flush on halt). Never panics.
    pub fn halt(&mut self) -> Result<(), OcError> {
        self.mode = RunMode::Halted;
        Ok(())
    }

    /// Set a parameter by dotted instance path. **Only permitted when `mode() == Halted`** (CDL
    /// §7.4.2 — params never change while the model advances). Validates the value against the
    /// parameter's declared type and any declared `[min, max]`; no coercion (`01` §5 invariant 3).
    /// The edit is staged and folded into block state on [`Engine::resume`].
    ///
    /// # Errors
    /// [`OcError::ParamWhileRunning`] if not `Halted`; [`OcError::UnknownPoint`] for an unknown path;
    /// [`OcError::ParamType`] on a type mismatch; [`OcError::ParamStructural`] for a conditional-
    /// instance param (not yet tracked); [`OcError::ParamRange`] for a value rejected by registry
    /// parameter rules. Never panics (R-ERR-1).
    pub fn set_param(&mut self, path: &str, value: Value) -> Result<(), OcError> {
        if self.mode != RunMode::Halted {
            return Err(OcError::ParamWhileRunning {
                path: path.to_string(),
            });
        }
        let idx = *self
            .params
            .index
            .get(path)
            .ok_or_else(|| OcError::UnknownPoint(path.to_string()))?;
        let decl = &self.params.entries[idx].declared;
        if value.value_type() != decl.value_type {
            return Err(OcError::ParamType {
                path: path.to_string(),
            });
        }
        if self.is_structural_param(idx) {
            return Err(OcError::ParamStructural {
                path: path.to_string(),
            });
        }
        // Inclusive metadata bounds catch ordinary out-of-range edits; strict and cross-parameter
        // class rules are checked just below because `ParamAttrs` has no exclusive/cross-field
        // shape.
        if let Ok(v) = value.as_real()
            && (decl.min.is_some_and(|lo| v < lo) || decl.max.is_some_and(|hi| v > hi))
        {
            return Err(OcError::ParamRange {
                path: path.to_string(),
            });
        }
        if self.class_param_rules_reject(idx, &value) {
            return Err(OcError::ParamRange {
                path: path.to_string(),
            });
        }
        self.params.entries[idx].value = value;
        self.params_dirty = true;
        Ok(())
    }

    /// Resume: re-fold any staged `set_param` edits into block state, re-instantiate the affected
    /// blocks, re-allocate run state, refresh the output snapshot, and return to `Running`. The
    /// schedule is **not** recomputed (structure is value-independent; R-PARAM-3) and conditional-
    /// instance removal is **not** re-run (a structural concern requiring a fresh load, CDL §7.7.4).
    /// Re-seeding `[S]` block state resets the run clock (`prev_t = None`) — a halt/set/resume cycle
    /// restarts the run, the simulation-sweep idiom (R-PARAM-6).
    ///
    /// # Errors
    /// [`OcError::Load`] only if a block class somehow fails to re-instantiate (unreachable on an
    /// already-loaded model). Never panics (R-ERR-1).
    pub fn resume(&mut self) -> Result<(), OcError> {
        if self.params_dirty {
            // CoW the model at rest (off-tick; refcount is 1, so `make_mut` does not clone). Re-fold
            // edits, then rebuild blocks/state/outputs via the shared helpers.
            let (blocks, state, outputs) = {
                let model = std::sync::Arc::make_mut(&mut self.model);
                for e in &self.params.entries {
                    if let Some(slot) = model.blocks[e.block.0 as usize]
                        .params
                        .values
                        .iter_mut()
                        .find(|(n, _)| *n == e.name)
                    {
                        slot.1 = e.value.clone();
                    }
                }
                let blocks = instantiate_blocks(model)?;
                let state = allocate_state(model, &blocks);
                let outputs = Outputs::build(model, &state);
                (blocks, state, outputs)
            };
            self.blocks = blocks;
            self.state = state;
            self.outputs = outputs;
            self.prev_t = None;
            self.params_dirty = false;
        }
        self.mode = RunMode::Running;
        Ok(())
    }

    /// Whether parameter `idx` gates a conditional-instance (`if`-on-Boolean-parameter, CDL §7.7.4).
    /// Currently always `false`; conditional-gate tracking is a structural-load concern.
    fn is_structural_param(&self, _idx: usize) -> bool {
        false
    }

    fn class_param_rules_reject(&self, idx: usize, value: &Value) -> bool {
        let edited = &self.params.entries[idx];
        let Some(block) = self.model.blocks.get(edited.block.0 as usize) else {
            return false;
        };
        let Some(entry) = lookup(&block.class_iri) else {
            return false;
        };
        for rule in entry.param_rules() {
            match *rule {
                ParamRule::Required { .. } | ParamRule::RealEqualWarning { .. } => {}
                ParamRule::RealGreaterThan { name, min } if edited.name.as_ref() == name => {
                    let Some(v) = real_param_value(value) else {
                        return true;
                    };
                    if !real_greater_than(v, min) {
                        return true;
                    }
                }
                ParamRule::RealGreaterThan { .. } => {}
                ParamRule::RealLessOrEqual { lower, upper }
                    if edited.name.as_ref() == lower || edited.name.as_ref() == upper =>
                {
                    let lower_value =
                        block_param_value(&self.params.entries, edited.block, lower, idx, value);
                    let upper_value =
                        block_param_value(&self.params.entries, edited.block, upper, idx, value);
                    if let (Some(lower_value), Some(upper_value)) = (lower_value, upper_value)
                        && !real_less_or_equal(lower_value, upper_value)
                    {
                        return true;
                    }
                }
                ParamRule::RealLessOrEqual { .. } => {}
            }
        }
        false
    }
}

fn static_real_bounds(class_path: &str, name: &str) -> (Option<f64>, Option<f64>) {
    let Some(entry) = lookup(class_path) else {
        return (None, None);
    };
    let mut min = None::<f64>;
    let max = None;
    for rule in entry.param_rules() {
        if let ParamRule::RealGreaterThan {
            name: rule_name,
            min: lower,
        } = *rule
            && rule_name == name
        {
            min = Some(match min {
                Some(current) => current.max(lower),
                None => lower,
            });
        }
    }
    (min, max)
}

fn block_param_value(
    entries: &[ParamEntry],
    block: BlockId,
    name: &str,
    edited_idx: usize,
    edited_value: &Value,
) -> Option<f64> {
    entries
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.block == block && entry.name.as_ref() == name)
        .and_then(|(idx, entry)| {
            if idx == edited_idx {
                real_param_value(edited_value)
            } else {
                real_param_value(&entry.value)
            }
        })
}

fn real_param_value(value: &Value) -> Option<f64> {
    match value {
        Value::Real(v) => Some(*v),
        Value::Integer(v) => Some(*v as f64),
        _ => None,
    }
}

fn real_greater_than(value: f64, min: f64) -> bool {
    matches!(value.partial_cmp(&min), Some(Ordering::Greater))
}

fn real_less_or_equal(lower: f64, upper: f64) -> bool {
    matches!(
        lower.partial_cmp(&upper),
        Some(Ordering::Less | Ordering::Equal)
    )
}
