//! The host-side durability sequence: what a host runs, once, after a successful load to put the
//! loaded model behind its storage adapter.
//!
//! # Dependency direction
//!
//! The engine is pure: `oce-api` depends on no storage crate and no load calls a storage port.
//! This module is therefore the *host's* code, kept beside the port it drives, and it never names
//! the facade: `oce-store` must not depend on `oce-api`, or the port would pull the whole engine
//! into every adapter and the seam would run both ways. Every function here takes only plain
//! data — the [`ResolvedModel`] DTO (defined in `oce_model::resolved_model`, which both crates
//! share) and an iterator of `(canonical point path, value type)` pairs — so a host passes
//! `loaded.model` and `loaded.input_points()` from `oce_api::LoadedModel` straight in, with no
//! conversion and neither crate naming the other.
//!
//! # The sequence
//!
//! [`persist_loaded_model`] runs, in this order and stopping at the first error:
//!
//! 1. [`recover`] — open the adapter's durability lifecycle ([`Durable::recover`]): rehydrate
//!    whatever the backend kept from a previous run and truncate any uncommitted tail. It runs
//!    first so that the save below lands *after* recovered state and is never overwritten by it.
//! 2. [`save_model`] — persist the resolved model ([`ModelStore::save_model`]).
//! 3. [`validate_inputs`] — resolve one [`PointHandle`] per input point
//!    ([`PointStore::resolve_points`]) and refuse a backend that returns a different count with
//!    [`StoreError::Validation`].
//!
//! This is the sequence the engine itself ran at load before it became pure (GitHub #390). The
//! engine has already committed the load when the host calls it, so a failure here leaves the
//! engine loaded and running; whatever the host does about a partially persisted model (retry,
//! [`ModelStore::delete_model`], refuse to start) is the host's policy. Nothing here is rolled
//! back, and nothing here calls [`Durable::commit`]: the commit boundary is the host's too.

use crate::{
    DomainKey, Durable, ModelStore, PointHandle, PointStore, PointValueType, ResolvedModel, Store,
    StoreError, StoreResult,
};

/// Run the whole post-load sequence against `store`: [`recover`], then [`save_model`], then
/// [`validate_inputs`], stopping at the first error.
///
/// `inputs` is every input point of the loaded model as `(canonical path, value type)` — exactly
/// what `oce_api::LoadedModel::input_points()` yields. Returns the resolved input handles in
/// `inputs` order (empty when the model has no inputs). Never panics.
///
/// # Errors
/// Returns the first [`StoreError`] from [`Durable::recover`], [`ModelStore::save_model`] or
/// [`PointStore::resolve_points`], or [`StoreError::Validation`] when the backend resolves a
/// different number of handles than there are inputs. Effects of the steps that completed before
/// the error are not rolled back.
pub fn persist_loaded_model<'a, S>(
    store: &S,
    model: &ResolvedModel,
    inputs: impl IntoIterator<Item = (&'a str, PointValueType)>,
) -> StoreResult<Vec<PointHandle>>
where
    S: Store + ?Sized,
{
    recover(store)?;
    save_model(store, model)?;
    validate_inputs(store, inputs)
}

/// Step 1: open the adapter's durability lifecycle ([`Durable::recover`]). Run it once, before any
/// other write to `store`. Never panics.
///
/// # Errors
/// Returns whatever [`Durable::recover`] returns, unchanged.
pub fn recover<S: Durable + ?Sized>(store: &S) -> StoreResult<()> {
    store.recover()
}

/// Step 2: persist the resolved model ([`ModelStore::save_model`]). Never panics.
///
/// # Errors
/// Returns whatever [`ModelStore::save_model`] returns, unchanged.
pub fn save_model<S: ModelStore + ?Sized>(store: &S, model: &ResolvedModel) -> StoreResult<()> {
    store.save_model(model)
}

/// Step 3: resolve one [`PointHandle`] per input point and refuse a count mismatch.
///
/// Each canonical path becomes a [`DomainKey`] in iteration order. An empty `inputs` returns an
/// empty vector without calling the store. The value type is carried so a host can pass
/// `LoadedModel::input_points()` unchanged; the port's `resolve_points` takes keys only, so it is
/// not checked here. Never panics.
///
/// # Errors
/// Returns the error from [`PointStore::resolve_points`] unchanged, or [`StoreError::Validation`]
/// when it returns a different number of handles than there are inputs.
pub fn validate_inputs<'a, S: PointStore + ?Sized>(
    store: &S,
    inputs: impl IntoIterator<Item = (&'a str, PointValueType)>,
) -> StoreResult<Vec<PointHandle>> {
    let keys: Vec<DomainKey> = inputs
        .into_iter()
        .map(|(path, _value_type)| DomainKey::new(path))
        .collect();
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let handles = store.resolve_points(&keys)?;
    if handles.len() != keys.len() {
        return Err(StoreError::Validation(format!(
            "PointStore::resolve_points returned {} handles for {} input points",
            handles.len(),
            keys.len()
        )));
    }
    Ok(handles)
}

#[cfg(test)]
#[path = "host_tests.rs"]
mod tests;
