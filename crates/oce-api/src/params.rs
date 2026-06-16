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

use std::collections::HashMap;

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
/// DTO (R-API-8). **M1:** `value_type` is real (read from the value); `min`/`max`/`unit`/`quantity`
/// are `None` (per-parameter attribute provenance is not yet carried on `oce_model::ParamTable` —
/// an M2 refinement). Surfaced as the third element of [`ParamTable::iter`].
#[derive(Clone, Debug, PartialEq)]
pub struct ParamAttrs {
    /// Structural type of the parameter.
    pub value_type: ValueType,
    /// Lower validation bound; `None` ⇒ unbounded below. M1: always `None`.
    pub min: Option<f64>,
    /// Upper validation bound; `None` ⇒ unbounded above. M1: always `None`.
    pub max: Option<f64>,
    /// SI computation unit; metadata only. M1: `None`.
    pub unit: Option<String>,
    /// Physical quantity; metadata only. M1: `None`.
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
                let declared = ParamAttrs {
                    value_type: value.value_type(),
                    min: None,
                    max: None,
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
    /// parameter's declared type and (M2) `[min, max]`; no coercion (`01` §5 invariant 3). The edit
    /// is staged and folded into block state on [`Engine::resume`].
    ///
    /// # Errors
    /// [`OcError::ParamWhileRunning`] if not `Halted`; [`OcError::UnknownPoint`] for an unknown path;
    /// [`OcError::ParamType`] on a type mismatch; [`OcError::ParamStructural`] for a conditional-
    /// instance param (never fires in M1); [`OcError::ParamRange`] for an out-of-bounds value
    /// (M1: bounds are unset, so this never fires). Never panics (R-ERR-1).
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
        // Range check: M1 carries no declared bounds (always `None`), so this is inert until param-
        // attribute provenance lands (M2 must extend this to `Integer` bounds via `IntAttrs`).
        if let Ok(v) = value.as_real()
            && (decl.min.is_some_and(|lo| v < lo) || decl.max.is_some_and(|hi| v > hi))
        {
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
            // edits, then rebuild blocks/state/outputs via the shared helpers (critic M1/M4).
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
    /// **M1: always `false`** — no conditional-gate set is tracked yet (an M2 structural concern).
    fn is_structural_param(&self, _idx: usize) -> bool {
        false
    }
}
