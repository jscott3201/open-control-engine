//! Compile-time PyO3 binding-shape guards (`10` §3 "Testability of §3").
//!
//! The future `oce-py` PyO3 binding wraps this `oce-api` surface, and `10` §3 makes the R-API-PY-1..8
//! binding-shape constraints normative on the facade. Any drift that would make the surface
//! un-bindable (a generic that cannot be a `#[pyclass]`, a non-`Clone` type that cannot cross to
//! Python, a borrowed/lifetime/`&dyn Store` return, a non-`Send` engine) must fail the build here,
//! with **no PyO3 dependency**. Every item below is a never-called `fn` whose mere compilation IS the
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
    AssertEvent, AssertLevel, ConnectorId, Diagnostic, Engine, IoClass, IoInventory, IoSummary,
    LoadReport, OcError, OutputTrace, Outputs, ParamAttrs, ParamTable, PhysicalKind,
    PointDirection, PointInfo, PointValueType, RunMode, SemanticQuery, SimMetrics, SimSpec,
    StepReport, TemplateRef, TrendCfg, TrendInterval, Value, ValueType,
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
/// design** — it is `Debug`-only and wraps non-`Clone` `#[from]` source errors; it crosses via
/// `From<OcError> for PyErr` (R-API-PY-7), whose *shape* is pinned by [`_assert_ocerror_shape`].
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
    let _: fn(&mut Engine<MemStore>, &str, Value) -> Result<(), OcError> =
        Engine::<MemStore>::set_input;
    let _: fn(&Engine<MemStore>, &str) -> Result<Value, OcError> = Engine::<MemStore>::get_output;
    let _: fn(&mut Engine<MemStore>, &SimSpec) -> Result<SimMetrics, OcError> =
        Engine::<MemStore>::simulate;
    let _: fn(&mut Engine<MemStore>, f64) -> Result<StepReport, OcError> =
        Engine::<MemStore>::step_realtime;
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
}

// ---- R-API-PY-7 — `OcError` is `Error + Send + Sync + 'static` (mappable to a PyErr) ----

/// R-API-PY-7: `OcError` is a `std::error::Error` that is `Send + Sync + 'static`, so the binding's
/// `From<OcError> for PyErr` can own and re-raise it across the GIL. The non-exhaustive per-family
/// `_ => OceError::new_err(..)` catch-all is a documented *binding-side* obligation (it lives in
/// `oce-py`, which adds no dependency here), not a guard.
fn _assert_ocerror_shape() {
    needs_send_sync::<OcError>();
    fn needs_error<E: std::error::Error + Send + Sync + 'static>() {}
    needs_error::<OcError>();
}

// ---- R-API-PY-8 — every public method returns `Result<_, OcError>` or is an infallible accessor.
// Enforced at runtime by the non-panicking minimal bodies (exit #6) + the `clippy::todo`/`panic`
// deny lints + the file-size/no-secret gates; the signature freeze above pins the `Result` shape.
