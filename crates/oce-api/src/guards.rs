//! Compile-time PyO3 binding-shape guards (`10` §3 "Testability of §3").
//!
//! The future `oce-py` PyO3 binding wraps a selected `oce-api` subset. The R-API-PY-1..8
//! binding-shape constraints apply to that wrapped subset and to the selected owned/thread-safe
//! shapes pinned below; they do not describe every Rust facade signature. Drift in a pinned shape
//! (a generic that cannot be a `#[pyclass]`, a non-`Clone` type that cannot cross to Python, a
//! borrowed/lifetime/trait-object return, or a non-`Send` engine) must fail the build here, with **no
//! PyO3 dependency**. Every item below is a never-called `fn` whose mere compilation IS the
//! assertion; `#[allow(dead_code)]` keeps them off the dead-code lint.
//!
//! This is a **non-test** module (not `#[cfg(test)]`) on purpose: the per-PR `ci.yml` gate runs
//! `cargo build` + `cargo clippy` while tests run in the release gate, so a signature/shape drift
//! must be caught by a normal build, not a test target. The complementary layer is the named
//! frozen-method contract in `08` §11.1. There is no full-surface enumeration before 1.0: a
//! public API surface baseline returns with the first stable release.

#![allow(dead_code)]

use std::sync::Arc;

use crate::{
    AssertEvent, AssertLevel, ConnectorId, DeclaredOutput, Diagnostic, Engine, EngineCheckpoint,
    EngineStateError, EngineStateSnapshot, IoClass, IoInventory, IoSummary, LoadErrorContext,
    LoadReport, LoadedModel, OcError, ParamAttrs, ParamTable, PassThroughPair, PhysicalKind,
    PointDirection, PointInfo, PointValueType, RunMode, Topology, TopologyBlock,
    TopologyConnection, TrendCfg, TrendInterval, Value, ValueType,
};

/// `T: Send + Sync` (used for the concrete thread-safety guards).
fn needs_send_sync<T: Send + Sync + ?Sized>() {}

/// `T: Clone` (R-API-PY-3: every type that crosses to Python is owned-convertible).
fn needs_clone<T: Clone>() {}

/// Snapshot inspection stays read-only, owned-convertible and thread-safe; not build authority.
fn _assert_state_portability_shape() {
    needs_clone::<crate::StatePortability>();
    needs_send_sync::<crate::StatePortability>();
    let _: fn(&EngineStateSnapshot) -> &crate::StatePortability = EngineStateSnapshot::portability;
}

/// Opaque preparations are moved, while completed results are independently owned and immutable.
#[allow(clippy::type_complexity)]
fn _assert_complete_frame_shapes() {
    needs_send_sync::<crate::PreparedInputFrame>();
    needs_send_sync::<crate::CompletedFrame>();
    needs_clone::<crate::CompletedFrame>();
    let _: fn(&Engine) -> Result<Vec<crate::InputDefinition>, OcError> = Engine::input_definitions;
    let _: fn(&Engine, f64, &[(&str, Value)]) -> Result<crate::PreparedInputFrame, OcError> =
        Engine::prepare_frame;
    let _: fn(&mut Engine, crate::PreparedInputFrame) -> Result<crate::CompletedFrame, OcError> =
        Engine::execute_frame;
    let _: fn(&crate::CompletedFrame) -> u64 = crate::CompletedFrame::sequence;
    let _: fn(&crate::CompletedFrame) -> f64 = crate::CompletedFrame::time;
    let _: fn(&crate::CompletedFrame) -> &[(String, Value)] = crate::CompletedFrame::outputs;
    let _: fn(&crate::CompletedFrame) -> &[AssertEvent] = crate::CompletedFrame::diagnostics;
    // The slot entry (issue #444): the map is owned, cloneable and thread-safe; the frame borrows
    // the engine, so it is a Rust-host shape, not one the Python binding returns.
    needs_send_sync::<crate::FrameSlots>();
    needs_clone::<crate::FrameSlots>();
    let _: fn(&Engine) -> Result<crate::FrameSlots, OcError> = Engine::frame_slots;
    let _: for<'a> fn(
        &'a mut Engine,
        &crate::FrameSlots,
        f64,
        &[Value],
    ) -> Result<crate::SlotFrame<'a>, OcError> = Engine::execute_slots;
    let _: fn(&crate::FrameSlots) -> &[crate::InputDefinition] = crate::FrameSlots::inputs;
    let _: fn(&crate::FrameSlots) -> &[String] = crate::FrameSlots::outputs;
}

// ---- R-API-PY-1 — no generic `#[pyclass]`: the binding wraps the concrete `Engine` ----

/// R-API-PY-1: the PyO3 binding wraps a single concrete `PyEngine(Engine)` (a generic type cannot
/// be a `#[pyclass]`). `Engine` has no type parameter; pin that it is the constructible,
/// monomorphic shape the wheel wraps.
fn _assert_engine_monomorphic() {
    let _: fn() -> Engine = Engine::new;
    let _: fn() -> Engine = <Engine as Default>::default;
}

// ---- R-API-PY-2 / R-THREAD-1 — the engine handle is `Send + Sync` (shareable as `Arc<Engine>`) ----

/// R-API-PY-2 / R-THREAD-1 — the **load-bearing** thread-safety guard. It is **concrete** on
/// purpose: `Engine` forces rustc to actually compute the auto-traits (which transitively
/// require `Box<dyn Block>: Send + Sync`, i.e. the `oce-blocks` `Block: Send + Sync` supertrait).
/// This is what fails to compile if that bound is ever removed.
fn _assert_engine_send_sync() {
    needs_send_sync::<Engine>();
    needs_send_sync::<Arc<Engine>>();
}

// ---- R-API-PY-3 — every type that reaches Python is owned-convertible (`Clone`, no borrow) ----

/// R-API-PY-3: each public type a Python method returns or accepts is `Clone`, so the binding
/// converts it to an owned Python object (no borrowed-lifetime leak). `OcError` is **excluded by
/// design** — it is `Debug`-only and wraps non-`Clone` `#[from]` source errors; a binding-local
/// mapper owns conversion to `PyErr` (R-API-PY-7), while [`_assert_ocerror_shape`] pins the source
/// error requirements.
fn _assert_python_facing_types_are_clone() {
    needs_clone::<LoadReport>();
    needs_clone::<LoadedModel>();
    needs_clone::<IoSummary>();
    needs_clone::<PointInfo>();
    needs_clone::<AssertEvent>();
    needs_clone::<IoInventory>();
    needs_clone::<ParamTable>();
    needs_clone::<ParamAttrs>();
    needs_clone::<TrendCfg>();
    needs_clone::<Diagnostic>();
    needs_clone::<Value>();
    // The `Copy` enums (also `Clone`).
    needs_clone::<RunMode>();
    needs_clone::<crate::NonFiniteInputPolicy>();
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
    let _: fn(&IoInventory) -> Vec<PointInfo> = IoInventory::to_vec;
    let _: fn(&ParamTable) -> Vec<(String, Value, ParamAttrs)> = ParamTable::to_vec;
}

/// The borrowing `iter()` companions return `impl Iterator` (uncoercible to a fn-pointer), so pin
/// their **item** types at a use-site instead.
fn _assert_enumeration_item_types(i: &IoInventory, p: &ParamTable) {
    let _: Option<PointInfo> = i.iter().next();
    let _: Option<(String, Value, ParamAttrs)> = p.iter().next();
}

// ---- R-API-PY-5/6 — String-path + `Value`-typed, no generic / lifetime / `Cow` / `&dyn Trait` ----

/// R-API-PY-5/6: the frozen method set, each pinned to its exact signature via a fn-pointer. A
/// fn-pointer is monomorphic, lifetime-erased, and concrete-typed, so a leaked generic parameter, an
/// `impl Trait` tied to `&self`, a `Cow<'_, _>`, or a trait object in any of these would fail to
/// coerce — a unit-build error, not a runtime surprise.
#[allow(clippy::type_complexity)]
fn _assert_frozen_signatures() {
    let _: fn() -> Engine = Engine::new;
    let _: fn(&mut Engine, &[u8]) -> Result<(LoadReport, LoadedModel), OcError> = Engine::load_cxf;
    let _: fn(
        &mut Engine,
        &[u8],
    ) -> Result<(crate::LoadReceipt, LoadedModel), crate::OperationFailure> =
        Engine::load_cxf_with_receipt;
    let _: fn(&Engine) -> Result<EngineCheckpoint, OcError> = Engine::checkpoint;
    let _: fn(&mut Engine, &EngineCheckpoint) -> Result<(), OcError> = Engine::restore_checkpoint;
    let _: fn(&Engine) -> Result<EngineStateSnapshot, OcError> = Engine::state_snapshot;
    let _: fn(&mut Engine, &EngineStateSnapshot) -> Result<(), OcError> = Engine::restore_state;
    let _: fn(&Engine, &str) -> Result<Value, OcError> = Engine::get_output;
    let _: fn(&Engine, &[&str]) -> Result<Vec<(String, Value)>, OcError> = Engine::watch;
    let _: fn(&Engine, &str) -> Result<Value, OcError> = Engine::get_param;
    let _: fn(&Engine) -> &ParamTable = Engine::params;
    let _: fn(&mut Engine, &str, Value) -> Result<(), OcError> = Engine::set_param;
    let _: fn(&mut Engine) -> Result<(), OcError> = Engine::halt;
    let _: fn(&mut Engine) -> Result<(), OcError> = Engine::resume;
    let _: fn(&Engine) -> RunMode = Engine::mode;
    let _: fn(&Engine) -> &IoInventory = Engine::io;
    let _: fn(&Engine) -> IoSummary = Engine::io_summary;
    let _: fn(&Engine, Option<&str>) -> Result<Vec<PointInfo>, OcError> = Engine::point_list;
    let _: fn(&Engine) -> Result<crate::ExportReport, OcError> = Engine::export_cxf;
    let _: fn(&Engine) -> crate::Topology = Engine::topology;
    // R-PUB-6 owned-snapshot accessors (also asserted by `_assert_outputs_enumerable`).
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
/// These are pinned here because nothing else enumerates the public surface before 1.0. This module
/// is a non-test module, so the per-PR `cargo build` catches a drift as soon as it lands.
///
/// `ExportReport::content_id` is **deliberately not pinned**, and its absence is a decision rather
/// than an oversight. It is `#[deprecated]`, and this is a non-test module compiled under the gate's
/// `-D warnings`, so naming it here is a hard error (`-D deprecated`). Suppressing that with an
/// `#[allow]` would be worse than the gap: the only change anyone should now make to a deprecated
/// method is deleting it, and a deletion is a reviewed public-surface removal. A per-PR pin would
/// make this gate fight that removal.
fn _assert_accessor_signatures() {
    let _: fn(&Engine) -> &oce_graph::Schedule = Engine::schedule;
    let _: fn(&IoInventory, usize) -> Option<&PointInfo> = IoInventory::get;
    let _: fn(&IoInventory) -> usize = IoInventory::len;
    let _: fn(&IoInventory) -> bool = IoInventory::is_empty;
    let _: fn(&ParamTable) -> usize = ParamTable::len;
    let _: fn(&ParamTable) -> bool = ParamTable::is_empty;
    let _: fn(&crate::ExportReport) -> Result<String, crate::ContentIdError> =
        crate::ExportReport::content_id_complete;
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
