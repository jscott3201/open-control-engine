//! Compile-time PyO3 binding-shape guards (`10` §3 "Testability of §3").
//!
//! The future `oce-py` PyO3 binding wraps a selected `oce-api` subset. The R-API-PY-1..8
//! binding-shape constraints apply to that wrapped subset and to the selected owned/thread-safe
//! shapes pinned below; they do not describe every Rust facade signature. Drift in a pinned shape
//! (a generic that cannot be a `#[pyclass]`, a non-`Clone` type that cannot cross to Python, a
//! borrowed/lifetime/`&dyn Store` return, or a non-`Send` engine) must fail the build here, with **no
//! PyO3 dependency**. Every item below is a never-called `fn` whose mere compilation IS the
//! assertion; `#[allow(dead_code)]` keeps them off the dead-code lint.
//!
//! This is a **non-test** module (not `#[cfg(test)]`) on purpose: the per-PR `ci.yml` gate runs
//! `cargo build` + `cargo clippy` while tests run in the release gate, so a signature/shape drift
//! must be caught by a normal build, not a test target. The complementary
//! layers are the checked-in `cargo public-api` baseline (release-gate; full-surface enumeration /
//! leak detector) and the named frozen-method contract in `08` §11.1.

#![allow(dead_code)]

use std::path::Path;
use std::sync::Arc;

use oce_store_mem::MemStore;

use crate::{
    AssertEvent, AssertLevel, ConnectorId, DeclaredOutput, Diagnostic, Engine, EngineCheckpoint,
    EngineStateError, EngineStateSnapshot, IoClass, IoInventory, IoSummary, LoadErrorContext,
    LoadReport, OcError, OutputTrace, Outputs, ParamAttrs, ParamTable, PassThroughPair,
    PhysicalKind, PointDirection, PointInfo, PointValueType, RunMode, SemanticQuery, SimMetrics,
    SimSpec, StepReport, TemplateRef, Topology, TopologyBlock, TopologyConnection, TrendCfg,
    TrendInterval, Value, ValueType,
};

/// `T: Send + Sync` (used for the concrete thread-safety guards).
fn needs_send_sync<T: Send + Sync + ?Sized>() {}

/// `T: Clone` (R-API-PY-3: every type that crosses to Python is owned-convertible).
fn needs_clone<T: Clone>() {}

// ---- R-API-PY-1 — no generic `#[pyclass]`: the binding wraps the concrete `Engine<MemStore>` ----

/// R-API-PY-1: the PyO3 binding wraps a single concrete `PyEngine(Engine<MemStore>)`, never
/// `Engine<S>` generically (a generic type cannot be a `#[pyclass]`). Pin that `Engine<MemStore>`
/// is the constructible, monomorphic shape the wheel wraps.
fn _assert_engine_monomorphic() {
    let _: fn() -> Engine<MemStore> = Engine::<MemStore>::in_memory;
}

// ---- R-API-PY-2 / R-THREAD-1 — the engine handle is `Send + Sync` (shareable as `Arc<Engine>`) ----

/// R-API-PY-2 / R-THREAD-1 — the **load-bearing** thread-safety guard. It is **concrete** on
/// purpose: `Engine<MemStore>` forces rustc to actually compute the auto-traits (which transitively
/// require `Box<dyn Block>: Send + Sync`, i.e. the `oce-blocks` `Block: Send + Sync` supertrait).
/// This is what fails to compile if that bound is ever removed.
fn _assert_engine_send_sync() {
    needs_send_sync::<Engine<MemStore>>();
    needs_send_sync::<Arc<Engine<MemStore>>>();
}

/// R-API-PY-2, the generic `08` R-ENG-1 *statement* `Engine<S>: Send + Sync for all S: Store`.
/// NOTE: this form is **documentation only and intentionally NON-enforcing** — a `where`-clause on
/// an uninstantiated generic `fn` is *assumed*, never *proven* (it would only be checked at a
/// concrete call site, of which there are none), so it compiles GREEN even if the bound were false.
/// The mechanically load-bearing guard is the concrete [`_assert_engine_send_sync`] above.
fn _doc_engine_send_sync_for_all_stores<S: oce_store::Store>()
where
    Engine<S>: Send + Sync,
{
}

// ---- R-API-PY-3 — every type that reaches Python is owned-convertible (`Clone`, no borrow) ----

/// R-API-PY-3: each public type a Python method returns or accepts is `Clone`, so the binding
/// converts it to an owned Python object (no borrowed-lifetime leak). `OcError` is **excluded by
/// design** — it is `Debug`-only and wraps non-`Clone` `#[from]` source errors; a binding-local
/// mapper owns conversion to `PyErr` (R-API-PY-7), while [`_assert_ocerror_shape`] pins the source
/// error requirements.
fn _assert_python_facing_types_are_clone() {
    needs_clone::<LoadReport>();
    needs_clone::<IoSummary>();
    needs_clone::<PointInfo>();
    needs_clone::<OutputTrace>();
    needs_clone::<StepReport>();
    needs_clone::<AssertEvent>();
    needs_clone::<SimMetrics>();
    needs_clone::<Outputs>();
    needs_clone::<IoInventory>();
    needs_clone::<ParamTable>();
    needs_clone::<ParamAttrs>();
    needs_clone::<TrendCfg>();
    needs_clone::<Diagnostic>();
    needs_clone::<Value>();
    // The `Copy` enums (also `Clone`).
    needs_clone::<RunMode>();
    needs_clone::<IoClass>();
    needs_clone::<PhysicalKind>();
    needs_clone::<AssertLevel>();
    needs_clone::<PointDirection>();
    needs_clone::<PointValueType>();
    needs_clone::<TrendInterval>();
    needs_clone::<ValueType>();
    needs_clone::<ConnectorId>();
    // Every type reachable from the frozen `Engine::topology` return (absent here since #236).
    needs_clone::<Topology>();
    needs_clone::<TopologyBlock>();
    needs_clone::<TopologyConnection>();
    needs_clone::<PassThroughPair>();
    needs_clone::<DeclaredOutput>();
    needs_clone::<EngineCheckpoint>();
    needs_clone::<EngineStateSnapshot>();
}

// ---- R-API-PY-4a — owned-snapshot enumeration is first-class (no borrowed iterator crosses) ----

/// R-API-PY-4a: the hot enumeration accessors return **owned** snapshots (the single most
/// load-bearing conversion the binding performs). Pin their concrete owned return types via
/// fn-pointers; a drift to a borrowed/`impl Trait`/`Cow` return fails to coerce.
fn _assert_outputs_enumerable() {
    let _: fn(&Outputs) -> Vec<(String, Value)> = Outputs::to_map;
    let _: fn(&IoInventory) -> Vec<PointInfo> = IoInventory::to_vec;
    let _: fn(&ParamTable) -> Vec<(String, Value, ParamAttrs)> = ParamTable::to_vec;
}

/// The borrowing `iter()` companions return `impl Iterator` (uncoercible to a fn-pointer), so pin
/// their **item** types at a use-site instead. The owned key + borrowed value of `Outputs::iter`
/// (`(ConnectorId, &Value)`) is exactly the owned-snapshot counterpart `to_map` clones.
fn _assert_enumeration_item_types(o: &Outputs, i: &IoInventory, p: &ParamTable) {
    let _: Option<(ConnectorId, &Value)> = o.iter().next();
    let _: Option<PointInfo> = i.iter().next();
    let _: Option<(String, Value, ParamAttrs)> = p.iter().next();
}

// ---- R-API-PY-5/6 — String-path + `Value`-typed, no generic / lifetime / `Cow` / `&dyn Store` ----

/// R-API-PY-5/6: the frozen method set, each pinned to its exact signature via a fn-pointer. A
/// fn-pointer is monomorphic, lifetime-erased, and concrete-typed, so a leaked generic parameter, an
/// `impl Trait` tied to `&self`, a `Cow<'_, _>`, or a `&dyn Store` in any of these would fail to
/// coerce — a unit-build error, not a runtime surprise. (`load_from_semantic` / `load_modelica` are
/// Rust-frozen here but deferred from the Python-wrapped subset until those loaders are wired.)
#[allow(clippy::type_complexity)]
fn _assert_frozen_signatures() {
    let _: fn() -> Engine<MemStore> = Engine::<MemStore>::in_memory;
    let _: fn(Arc<MemStore>) -> Engine<MemStore> = Engine::<MemStore>::with_store;
    let _: fn(&mut Engine<MemStore>, &[u8]) -> Result<LoadReport, OcError> =
        Engine::<MemStore>::load_cxf;
    let _: fn(&mut Engine<MemStore>, &TemplateRef, &SemanticQuery) -> Result<LoadReport, OcError> =
        Engine::<MemStore>::load_from_semantic;
    let _: fn(&mut Engine<MemStore>, &Path) -> Result<LoadReport, OcError> =
        Engine::<MemStore>::load_modelica;
    let _: fn(&mut Engine<MemStore>, f64) -> Result<&Outputs, OcError> = Engine::<MemStore>::tick;
    let _: fn(&Engine<MemStore>) -> Result<EngineCheckpoint, OcError> =
        Engine::<MemStore>::checkpoint;
    let _: fn(&mut Engine<MemStore>, &EngineCheckpoint) -> Result<(), OcError> =
        Engine::<MemStore>::restore_checkpoint;
    let _: fn(&Engine<MemStore>) -> Result<EngineStateSnapshot, OcError> =
        Engine::<MemStore>::state_snapshot;
    let _: fn(&mut Engine<MemStore>, &EngineStateSnapshot) -> Result<(), OcError> =
        Engine::<MemStore>::restore_state;
    let _: fn(&mut Engine<MemStore>, &str, Value) -> Result<(), OcError> =
        Engine::<MemStore>::set_input;
    let _: fn(&Engine<MemStore>, &str) -> Result<Value, OcError> = Engine::<MemStore>::get_output;
    let _: fn(&Engine<MemStore>, &[&str]) -> Result<Vec<(String, Value)>, OcError> =
        Engine::<MemStore>::watch;
    let _: fn(&mut Engine<MemStore>, &SimSpec) -> Result<SimMetrics, OcError> =
        Engine::<MemStore>::simulate;
    let _: fn(&mut Engine<MemStore>, f64) -> Result<StepReport, OcError> =
        Engine::<MemStore>::step_realtime;
    let _: fn(&mut Engine<MemStore>, u64) = Engine::<MemStore>::set_realtime_epoch_unix_nanos;
    let _: fn(&Engine<MemStore>) -> Option<u64> = Engine::<MemStore>::realtime_epoch_unix_nanos;
    let _: fn(&Engine<MemStore>, &str) -> Result<Value, OcError> = Engine::<MemStore>::get_param;
    let _: fn(&Engine<MemStore>) -> &ParamTable = Engine::<MemStore>::params;
    let _: fn(&mut Engine<MemStore>, &str, Value) -> Result<(), OcError> =
        Engine::<MemStore>::set_param;
    let _: fn(&mut Engine<MemStore>) -> Result<(), OcError> = Engine::<MemStore>::halt;
    let _: fn(&mut Engine<MemStore>) -> Result<(), OcError> = Engine::<MemStore>::resume;
    let _: fn(&Engine<MemStore>) -> RunMode = Engine::<MemStore>::mode;
    let _: fn(&Engine<MemStore>) -> &IoInventory = Engine::<MemStore>::io;
    let _: fn(&Engine<MemStore>) -> IoSummary = Engine::<MemStore>::io_summary;
    let _: fn(&Engine<MemStore>, Option<&str>) -> Result<Vec<PointInfo>, OcError> =
        Engine::<MemStore>::point_list;
    let _: fn(&Engine<MemStore>) -> Result<crate::ExportReport, OcError> =
        Engine::<MemStore>::export_cxf;
    let _: fn(&Engine<MemStore>) -> crate::Topology = Engine::<MemStore>::topology;
    // R-PUB-6 owned-snapshot accessors (also asserted by `_assert_outputs_enumerable`).
    let _: fn(&Outputs) -> Vec<(String, Value)> = Outputs::to_map;
    let _: fn(&IoInventory) -> Vec<PointInfo> = IoInventory::to_vec;
    let _: fn(&ParamTable) -> Vec<(String, Value, ParamAttrs)> = ParamTable::to_vec;
    // R-PUB-1: the oce-model value/IO types + the diagnostic type are nameable through the facade.
    let _: Option<Value> = None;
    let _: Option<ValueType> = None;
    let _: Option<ConnectorId> = None;
    let _: Option<Diagnostic> = None;
    let _: fn(&[u8]) -> Result<EngineStateSnapshot, EngineStateError> =
        EngineStateSnapshot::from_bytes;
    let _: fn(&EngineStateSnapshot) -> &[u8] = EngineStateSnapshot::as_bytes;
    let _: fn(EngineStateSnapshot) -> Vec<u8> = EngineStateSnapshot::into_bytes;
}

// ---- R-API-PY-5/6 — the accessor surface the frozen-method set above does not reach ----

/// The public inherent methods left unpinned by [`_assert_frozen_signatures`]: the engine's
/// borrowed-state accessors and the enumerable snapshot types' readers. Same fn-pointer idiom, same
/// failure mode — a return-type or arity drift stops coercing and the build fails.
///
/// Pinning these here rather than leaving them to `cargo public-api` is deliberate: that baseline
/// runs only in the release gate (PRs into `main` and a daily cron), never on a PR into
/// `development`, so a drift landed mid-cycle would go unseen until release. This module is a
/// non-test module, so the per-PR `cargo build` catches it instead.
///
/// Two pins are narrower than the method they cover, stated so neither is overclaimed:
/// `Engine::store` is pinned at `MemStore` like its neighbours, so it would not catch a newly added
/// `where` bound on `S`; and `TemplateRef::new` is pinned at `&'static str`, one of the concrete
/// types its `impl Into<Arc<str>>` parameter accepts, so it would not catch a narrowing of that
/// parameter to `&'static str` itself. The generic form cannot take the neighbouring idiom at all —
/// a late-bound lifetime cannot be inferred through `impl Into<_>`, so `fn(&str) -> TemplateRef`
/// fails to coerce with `E0308: one type is more general than the other`.
///
/// `ExportReport::content_id` is **deliberately not pinned**, and its absence is a decision rather
/// than an oversight. It is `#[deprecated]`, and this is a non-test module compiled under the gate's
/// `-D warnings`, so naming it here is a hard error (`-D deprecated`). Suppressing that with an
/// `#[allow]` would be worse than the gap: the only change anyone should now make to a deprecated
/// method is deleting it, and a deletion is public-surface removal, which the release-gate baseline
/// is the right review to adjudicate. A per-PR pin would make this gate fight that removal.
fn _assert_accessor_signatures() {
    let _: fn(&Engine<MemStore>) -> &Outputs = Engine::<MemStore>::outputs;
    let _: fn(&Engine<MemStore>) -> &oce_graph::Schedule = Engine::<MemStore>::schedule;
    let _: fn(&Engine<MemStore>) -> &MemStore = Engine::<MemStore>::store;
    let _: fn(&Outputs, ConnectorId) -> Option<&Value> = Outputs::get;
    let _: fn(&Outputs) -> usize = Outputs::len;
    let _: fn(&Outputs) -> bool = Outputs::is_empty;
    let _: fn(&IoInventory, usize) -> Option<&PointInfo> = IoInventory::get;
    let _: fn(&IoInventory) -> usize = IoInventory::len;
    let _: fn(&IoInventory) -> bool = IoInventory::is_empty;
    let _: fn(&ParamTable) -> usize = ParamTable::len;
    let _: fn(&ParamTable) -> bool = ParamTable::is_empty;
    let _: fn(&OutputTrace) -> &[String] = OutputTrace::columns;
    let _: fn(&OutputTrace) -> &[f64] = OutputTrace::times;
    let _: fn(&OutputTrace, usize) -> Option<&[Value]> = OutputTrace::column;
    let _: fn(&OutputTrace) -> usize = OutputTrace::rows;
    let _: fn(&crate::ExportReport) -> Result<String, crate::ContentIdError> =
        crate::ExportReport::content_id_complete;
    let _: fn(&TemplateRef) -> &str = TemplateRef::iri;
    let _: fn(&'static str) -> TemplateRef = TemplateRef::new;
}

// ---- R-API-PY-7 — `OcError` is `Error + Send + Sync + 'static` (mappable to a PyErr) ----

/// R-API-PY-7: `OcError` is a `std::error::Error` that is `Send + Sync + 'static`, so the binding's
/// binding-local mapper can own and re-raise it across the GIL. The non-exhaustive per-family
/// catch-all is a binding-side obligation (it lives in `oce-py`, which adds no dependency here),
/// not a guard.
fn _assert_ocerror_shape() {
    needs_send_sync::<OcError>();
    needs_send_sync::<LoadErrorContext>();
    needs_send_sync::<EngineStateError>();
    fn needs_error<E: std::error::Error + Send + Sync + 'static>() {}
    needs_error::<OcError>();
    needs_error::<LoadErrorContext>();
    needs_error::<EngineStateError>();
}

// ---- the failed-load diagnostics accessors keep their signatures (satisfies R-PUB-7) ----

/// Satisfies R-PUB-7 (`_spec/08` §11.1 — each frozen item carries a compile-shaped assertion) for
/// `OcError::diagnostics` and `OcError::all_diagnostics`. They are the only routes to the resolver
/// seam's diagnostics for a consumer depending on `oce-api` alone and the authoritative route for
/// diagnostics retained across a later failed load stage. The deep-gate seam is separately
/// reachable through `OcError::Validate`'s public field, which needs no accessor and no nameable
/// type.
///
/// Borrowing is the load-bearing part of the signature: returning owned diagnostics would oblige a
/// clone on a path a host may take per failed document, and `Diagnostic` is not `Copy`.
fn _assert_ocerror_diagnostics_shape() {
    let _: fn(&OcError) -> &[Diagnostic] = OcError::diagnostics;
    fn assert_complete_stream(error: &OcError) -> impl Iterator<Item = &Diagnostic> {
        error.all_diagnostics()
    }
    let _ = assert_complete_stream;
}

// ---- R-API-PY-8 — every public method returns `Result<_, OcError>` or is an infallible accessor.
// Enforced at runtime by the non-panicking minimal bodies (exit #6) + the `clippy::todo`/`panic`
// deny lints + the file-size/no-secret gates; the signature freeze above pins the `Result` shape.
