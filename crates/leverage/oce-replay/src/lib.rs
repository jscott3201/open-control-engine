#![forbid(unsafe_code)]
//! Historical replay over the Open Control Engine facade (design note `docs/design/replay.md`,
//! issue #445).
//!
//! Status: the first leverage crate (M3). It sits above the facade and depends on `oce-api` only;
//! it is host policy applied offline, never engine policy. It runs no thread, async runtime, clock,
//! file-system walk or network listener, and reads no file by path: sources take owned columns,
//! step lists or a byte reader.
//!
//! It holds four things:
//!
//! - **Frame sources** ([`FrameSource`]): rows by slot, `(t, &[Option<Value>])`, after a one-time
//!   name→slot mapping ([`InputMap::resolve`], by the one name rule [`resolve_path`], which
//!   [`PathIndex`] answers for many names at once); three adapters — [`ColumnarSource`] (owned typed
//!   columns, the numpy and pandas shape), [`CsvSource`] (a bounded byte reader with a column map
//!   and a time origin) and [`StepSource`] (per-input change lists).
//! - **Missing-sample policy** ([`MissingPolicy`], [`OnRefusal`], [`DuplicateTime`], [`TimeBase`]):
//!   refuse and stop by default; every substitution, skip and refusal is written into the trace.
//!   NaN, NaT, a blank cell and a cleared validity flag are always missing, never a value, whatever
//!   the non-finite policy says; ±∞ is a value, governed by [`ReplayOptions::non_finite`]
//!   (default `Refuse`), which [`Replay::new`] sets on its engine and records.
//! - **The trace** ([`Trace`]): columnar, typed, raw bits, one row per attempted frame with a
//!   status byte; sparse diagnostic and refusal tables; opt-in input columns and state snapshots;
//!   a dense substitution mask while any policy can substitute; a header-only wall-clock origin;
//!   [`Trace::canonical_bytes`] for goldens. Bytes per frame are documented in [`trace`] and
//!   asserted by the tests.
//! - **A resumable cursor** ([`Replay::advance`]): at most a chunk of frames per call, with a
//!   blocking [`Replay::run`] built on it; chunk sizes never change the trace.
//!
//! ```
//! use oce_api::Engine;
//! use oce_replay::{
//!     ColumnKind, ColumnMap, ColumnSpec, InputMap, MissingPolicy, Replay, ReplayLimits,
//!     ReplayOptions, StepSource, TimeBase,
//! };
//! # fn demo(engine: Engine) -> Result<(), Box<dyn std::error::Error>> {
//! let inputs = engine.input_definitions()?;
//! // One column per input, named by the input's last path segment (a unique suffix).
//! let columns: Vec<ColumnSpec> = inputs
//!     .iter()
//!     .map(|d| ColumnSpec::new(d.path.rsplit('.').next().unwrap_or(&d.path), ColumnKind::Real))
//!     .collect();
//! let map = InputMap::resolve(&inputs, &columns, &ColumnMap::by_name(&columns),
//!     &MissingPolicy::Refuse, &[])?;
//! let steps = columns
//!     .iter()
//!     .map(|c| (c.name.clone(), vec![(0.0, oce_api::Value::Real(1.0))]))
//!     .collect();
//! let source = StepSource::new(&map, steps, &ReplayLimits::default())?;
//! let options = ReplayOptions::new(TimeBase::Grid { t0: 0.0, step: 5.0 });
//! let (_engine, trace) = Replay::new(engine, map, source, options)?.run()?;
//! assert!(trace.is_clean());
//! # Ok(()) }
//! ```
//!
//! **Enum classes (decision 21 as built).** The facade exposes no lookup from an enum value's
//! class to its class path, so an Enum *input* column carries the class path its source declared
//! ([`ColumnKind::Enum`]), an unverified label replay cannot check against the engine's class;
//! an Enum *output* column keeps the facade's process-local class index in memory only
//! ([`TraceOutput::enum_class_index`]), and [`Trace::canonical_bytes`] writes the ordinal with an
//! empty class path for every output. No G36 corpus fixture has an Enum boundary port; a facade
//! class-path lookup is a follow-up.

pub mod canonical;
mod columnar;
mod csv_source;
mod error;
mod frame;
mod limits;
mod map;
mod path_index;
mod policy;
mod replay;
mod source;
mod step;
pub mod time;
pub mod trace;

pub use canonical::{TRACE_MAGIC, TRACE_VERSION};
pub use columnar::{Column, ColumnarSource};
pub use csv_source::{CsvFormat, CsvSource, CsvTime};
pub use error::{MapError, ReplayError, SourceError};
pub use limits::{LimitError, ReplayLimits};
pub use map::{ColumnKind, ColumnMap, ColumnSpec, InputMap, MapId, resolve_path};
pub use path_index::PathIndex;
pub use policy::{DuplicateTime, FrameStatus, MissingPolicy, OnRefusal, SourceKind, TimeBase};
pub use replay::{Progress, Replay, ReplayOptions, TraceOptions};
pub use source::{FrameSource, Row};
pub use step::StepSource;
pub use time::{TimeColumn, TimeOrigin};
pub use trace::{
    DiagnosticRow, EngineRefusal, RefusalCause, RefusalRow, Trace, TraceColumn, TraceInput,
    TraceKind, TraceMeta, TraceOutput,
};
