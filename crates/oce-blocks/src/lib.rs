#![forbid(unsafe_code)]
//! `oce-blocks` — the native CDL elementary-block library and the [`Block`] trait for the
//! Open Control Engine (`03-block-library.md`).
//!
//! Every elementary block (CDL §7.6) implements [`Block`], classified statically as stateless
//! `[A]` ([`BlockKind::Algebraic`]) or stateful `[S]` ([`BlockKind::Stateful`]). Per the **arena
//! model** (`01` §7/§9) a block object is an *immutable* description (class + resolved
//! parameters); all mutable `[S]` state lives in an engine-owned `RunState.words` region, never in
//! the block struct. The split between [`Block::emit_from_state`] (emit from **prior** state) and
//! [`Block::update_state`] (advance state after) is what lets `oce-graph` evaluate a tick in
//! topological order and cut algebraic loops at non-feedthrough stateful blocks. This crate is
//! **Group A**: no store, no database (D-OWNER-1); it carries behavior only — never per-tick state
//! in the struct, never non-computational metadata (CDL §7.17).
//!
//! The arena trait, per-tick context seam, scalar Reals, Logical/Conversions/Integers algebraic
//! blocks, Reals dynamic blocks, PID controllers, logical timing/latch blocks, integer edge/count
//! blocks, and the `Utilities.Assert` diagnostics sink are implemented and registered by canonical
//! CDL class path. The remaining CDL catalog breadth is added family-by-family behind the same
//! trait and registry contracts.

use std::borrow::Cow;

use oce_model::{ParamTable, Value, determinism::canonicalize_real};

mod catalog;
mod conversions;
mod discrete;
mod discrete_sampled;
mod dynamics;
mod integers;
mod integers_edge;
mod integers_stage;
mod logical;
mod logical_latch;
mod logical_proof;
mod logical_timing;
mod logical_variable_pulse;
mod lowering;
mod pid;
pub mod port_names;
mod psychrometrics;
mod reals_arithmetic;
mod reals_comparators;
mod reals_filters;
mod reals_integrator;
mod reals_matrix;
mod reals_ramp;
mod reals_sources;
mod reals_transcendental;
mod reals_vector_reductions;
mod registry;

pub use catalog::{
    CatalogEntry, DefaultLiteral, DefaultSource, ParamDefault, PortInfo, PortNaming, catalog,
};
mod routing;
mod routing_boolean;
mod routing_integer;
mod source_pulse;
mod source_timetable;
mod utilities;

pub use conversions::{BooleanToInteger, BooleanToReal, IntegerToReal, RealToInteger};
pub use discrete::{TriggeredMax, TriggeredMovingMean, TriggeredSampler, UnitDelay};
pub use discrete_sampled::{FirstOrderHold, Sampler, ZeroOrderHold};
pub use integers::{
    IntegerAbs, IntegerAdd, IntegerAddParameter, IntegerConstant, IntegerEqual, IntegerGreater,
    IntegerGreaterEqual, IntegerGreaterEqualThreshold, IntegerGreaterThreshold, IntegerLess,
    IntegerLessEqual, IntegerLessEqualThreshold, IntegerLessThreshold, IntegerMax, IntegerMin,
    IntegerMultiSum, IntegerMultiply, IntegerSubtract, IntegerSwitch,
};
pub use integers_edge::{IntegerChange, OnCounter};
pub use integers_stage::IntegerStage;
pub use logical::{
    And, Edge, LogicalConstant, LogicalSwitch, MultiAnd, MultiOr, Nand, Nor, Not, Or, Pre,
    SampleTrigger, Xor,
};
pub use logical_latch::{FallingEdge, Latch, LogicalChange, Toggle};
pub use logical_proof::Proof;
pub use logical_timing::{Timer, TimerAccumulating, TrueDelay, TrueFalseHold, TrueHoldWithReset};
pub use logical_variable_pulse::LogicalVariablePulse;
pub use lowering::{BooleanPassThrough, IntegerPassThrough, RealPassThrough};
pub use pid::{Pid, PidWithReset};
pub use psychrometrics::{DewPointTDryBulPhi, SpecificEnthalpyTDryBulPhi, WetBulbTDryBulPhi};
pub use reals_arithmetic::{
    Abs, Add, AddParameter, Average, Constant, Divide, Limiter, Line, Max, Min, Modulo, Multiply,
    MultiplyByParameter, Round, Sqrt, Subtract, Switch,
};
pub use reals_comparators::{Greater, GreaterThreshold, Hysteresis, Less, LessThreshold};
pub use reals_filters::{Derivative, LimitSlewRate, MovingAverage};
pub use reals_integrator::IntegratorWithReset;
pub use reals_matrix::{MatrixGain, MatrixMax, MatrixMin, MatrixReductionAxis, Sort};
pub use reals_ramp::Ramp;
pub use reals_sources::{CalendarTime, CivilTime, SourceRamp, SourceSin};
pub use reals_transcendental::{Acos, Asin, Atan, Atan2, Cos, Exp, Log, Log10, Sin, Tan};
pub use reals_vector_reductions::{MultiMax, MultiMin, MultiSum};
pub use registry::lookup;
pub use routing::{
    RealExtractSignal, RealExtractor, RealScalarReplicator, RealVectorFilter, RealVectorReplicator,
};
pub use routing_boolean::{
    BooleanExtractSignal, BooleanExtractor, BooleanScalarReplicator, BooleanVectorFilter,
    BooleanVectorReplicator,
};
pub use routing_integer::{
    IntegerExtractSignal, IntegerExtractor, IntegerScalarReplicator, IntegerVectorFilter,
    IntegerVectorReplicator,
};
pub use source_pulse::{IntegerPulse, LogicalPulse, RealPulse};
pub use source_timetable::{
    IntegerTimeTable, LogicalTimeTable, RealTimeTable, TimeTableExtrapolation, TimeTableSmoothness,
};
pub use utilities::{Assert, SunRiseSet};

/// Wall-clock-free model time in seconds, chosen by the host scheduler (CDL §7.16; `01` §8).
pub type Time = f64;

/// The two execution classes — the whole ballgame (`03` §1.1).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BlockKind {
    /// Stateless `[A]`: `y = f(p, t, u)`. Empty state.
    Algebraic,
    /// Stateful `[S]`: `y = f(p, t, u, x)`; owns per-instance state across ticks.
    Stateful,
}

/// The kind of value a port carries on the hot path (Real/Integer/Boolean only; §7.8 admits no
/// String connector, and enumerations are parameters not signals).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PortKind {
    /// A `Real` (`f64`) port.
    Real,
    /// An `Integer` (`i64`) port.
    Integer,
    /// A `Boolean` (`bool`) port.
    Boolean,
}

/// Static interface descriptor for an elementary block class. Fixed-width blocks use this directly
/// for their BUILD-time port contract. Parameter-width blocks publish their default-width class
/// descriptor here and override [`Block::resolved_signature`] for the authoritative instance
/// signature. Carries no non-computational metadata (§7.17).
#[derive(Clone, Copy, Debug)]
pub struct BlockSignature {
    /// Canonical class path, e.g. `"CDL.Reals.PID"`.
    pub class_path: &'static str,
    /// Input port kinds in declaration order (index = port index).
    pub inputs: &'static [PortKind],
    /// Output port kinds in declaration order (index = port index).
    pub outputs: &'static [PortKind],
    /// Static class-level state hint. For parameter-dependent classes, use [`Block::kind`] on the
    /// resolved instance for the authoritative `[A]`/`[S]` allocation decision.
    pub stateful: bool,
}

/// Maximum resolved width of one vector-style CDL port accepted by native block constructors.
///
/// This bounds input-derived BUILD-time allocation for blocks such as `MultiAnd(nin=...)` while
/// keeping any realistic equipment-control vector well inside the supported range.
pub const MAX_RESOLVED_PORT_WIDTH: usize = 1 << 20;

/// A BUILD-time port shape for vector-style CDL connector declarations after parameter resolution.
///
/// The engine still stores and ticks one scalar [`oce_model::ConnectorId`] per element. A shape only
/// tells the block constructor or validator how many consecutive scalar ports a source connector such
/// as `u[nin]` expands to, in source declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortShape {
    /// Scalar value kind for each flattened element.
    pub kind: PortKind,
    /// Number of flattened scalar elements. `0` is legal for source-verified empty vectors.
    pub width: usize,
}

impl PortShape {
    /// Build a resolved shape for `width` consecutive scalar ports of `kind`.
    #[must_use]
    pub const fn new(kind: PortKind, width: usize) -> Self {
        Self { kind, width }
    }

    /// Build a scalar one-element port shape.
    #[must_use]
    pub const fn scalar(kind: PortKind) -> Self {
        Self::new(kind, 1)
    }

    /// Append this shape's flattened scalar port kinds to `out`.
    pub fn extend_kinds(&self, out: &mut Vec<PortKind>) {
        out.extend(std::iter::repeat_n(self.kind, self.width));
    }

    /// Return this shape as a vector of flattened scalar port kinds.
    #[must_use]
    pub fn to_kinds(self) -> Vec<PortKind> {
        let mut out = Vec::with_capacity(self.width);
        self.extend_kinds(&mut out);
        out
    }
}

/// Instance-resolved block interface descriptor used by BUILD-time validators.
///
/// Fixed-width blocks borrow their static [`BlockSignature`] slices without allocation. Dynamic
/// blocks can borrow vectors stored on the immutable block instance, so port expansion happens once
/// at load and never on the tick.
#[derive(Clone, Debug)]
pub struct ResolvedBlockSignature<'a> {
    /// Canonical class path, e.g. `"CDL.Reals.PID"`.
    pub class_path: &'static str,
    /// Input port kinds in resolved declaration order.
    pub inputs: Cow<'a, [PortKind]>,
    /// Output port kinds in resolved declaration order.
    pub outputs: Cow<'a, [PortKind]>,
}

impl<'a> ResolvedBlockSignature<'a> {
    /// Borrow a fixed-width class signature as a resolved instance signature.
    #[must_use]
    pub fn from_static(signature: &'static BlockSignature) -> Self {
        Self {
            class_path: signature.class_path,
            inputs: Cow::Borrowed(signature.inputs),
            outputs: Cow::Borrowed(signature.outputs),
        }
    }
}

/// A diagnostics sink for `Utilities.Assert` and unit warnings — injected by the scheduler,
/// never a global, to keep the crate store-free and side-effect-explicit (`03` §2.4).
pub trait Diagnostics {
    /// Emit a warning attributed to `source` at model time `t`.
    fn warn(&self, source: &str, message: &str, t: Time);
}

/// Per-tick block execution context. The scheduler owns the diagnostics sink and supplies the
/// model time; blocks read time through [`Ctx::t`] and report warnings through [`Ctx::warn`].
pub struct Ctx<'a> {
    t: Time,
    diag: &'a dyn Diagnostics,
}

impl<'a> Ctx<'a> {
    /// Build a per-tick context from scheduler-owned time and diagnostics sink.
    #[must_use]
    pub fn new(t: Time, diag: &'a dyn Diagnostics) -> Self {
        Self { t, diag }
    }

    /// Current model time in seconds.
    #[must_use]
    pub fn t(&self) -> Time {
        self.t
    }

    /// Emit a warning at this context's model time.
    pub fn warn(&self, source: &str, message: &str) {
        self.diag.warn(source, message, self.t);
    }
}

/// Zero-size diagnostics sink used where assertions are intentionally dropped.
pub struct NoopDiagnostics;

impl Diagnostics for NoopDiagnostics {
    fn warn(&self, _source: &str, _message: &str, _t: Time) {}
}

/// The core elementary-block contract (CDL §7.6) — the **arena model** (`01` §7/§9, the binding
/// FRAME Block-trait resolution). A block object is an *immutable* description (class + parameters
/// resolved at construction); all mutable per-instance `[S]` state lives in an engine-owned flat
/// region (`RunState.words`), never in the block struct. This keeps the frozen schedule shareable
/// and keeps per-instance state out of the allocator (`01` §7 req 4, §9 req 4), so every method
/// takes `&self`. It does **not** make the tick zero-allocation: an implementation may allocate
/// inside [`Block::step_algebraic`], and `CDL.Reals.Sort` does so on every tick.
///
/// `[A]` blocks override [`Block::step_algebraic`]. `[S]` blocks set [`Block::state_len`], seed via
/// [`Block::init_state`], and override [`Block::emit_from_state`] + [`Block::update_state`].
///
/// The trait is **`Send + Sync`** (R-API-PY-2 / `08` R-ENG-1): a block holds only resolved
/// parameters — plain owned `f64`/`bool` data, no `Rc`/`RefCell`/raw pointers (per-instance mutable
/// `[S]` state lives in the engine's `RunState`, never here) — so `Box<dyn Block>` is `Send + Sync`
/// and the frozen schedule is shareable across host threads as `Arc<Engine<S>>`. The bound is a
/// zero-cost marker; it is what lets `oce-api`'s `_assert_send_sync` compile.
pub trait Block: Send + Sync {
    /// Class-level interface descriptor. For fixed-width classes this is the same shape used at
    /// BUILD. For parameter-width classes this is the default-width descriptor; call
    /// [`Block::resolved_signature`] on the resolved instance for authoritative port arity.
    fn signature(&self) -> &'static BlockSignature;

    /// Parameter-resolved interface descriptor. Validators use this after the registry constructor
    /// has folded parameters such as `nin`, so variadic/vector CDL ports expand to scalar port kinds
    /// before `oce-graph` indexes the flattened connector arenas.
    fn resolved_signature(&self) -> ResolvedBlockSignature<'_> {
        ResolvedBlockSignature::from_static(self.signature())
    }

    /// Stateless `[A]` vs stateful `[S]` (CDL §7.2/§7.6). Queried on the parameter-resolved
    /// instance, never the bare class (`01` §4.2 design note).
    fn kind(&self) -> BlockKind;

    /// Direct-feedthrough oracle: returns `true` iff output `out_idx` algebraically (zero-time,
    /// same-tick) depends on input `in_idx`. This is what cuts the DAG for loop-breakers
    /// (`01` §4.2; `03` §3). Loop-breaker `[S]` blocks return `false` for the state-bearing path.
    fn feeds_through(&self, in_idx: usize, out_idx: usize) -> bool;

    /// Word count of this block's `[S]` state region within `RunState.words` (0 for `[A]`). Fixed
    /// at BUILD; the tick never resizes it (`01` §7 req 1).
    fn state_len(&self) -> usize {
        0
    }

    /// Seed the `[S]` state `region` from resolved parameters (CDL has no `start` attribute; the
    /// initial state seeds from a parameter — §7.4, R-TRAIT-4 / `01` §7 req 2). `[A]` blocks: no-op.
    fn init_state(&self, _region: &mut [u64], _params: &ParamTable) {}

    /// `[A]` output: `y = f(p, t, u)`. Emit each output by port index via `emit` (R-TRAIT-1).
    /// `[S]` blocks leave this as the default no-op — their output is [`Block::emit_from_state`].
    fn step_algebraic(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        _emit: &mut dyn FnMut(usize, Value),
    ) {
    }

    /// `[S]` output pass: emit outputs from **prior** state — `region` is read-only here — before
    /// any state update this tick (`01` §9 req 2, R-TRAIT-1). `[A]` blocks: default no-op.
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        _inputs: &[Value],
        _region: &[u64],
        _emit: &mut dyn FnMut(usize, Value),
    ) {
    }

    /// `[S]` state pass: advance `x(t) → x'(t)` into the mutable `region`, exactly once per tick,
    /// after [`Block::emit_from_state`] (`01` §9 req 2, R-TRAIT-2). `[A]` blocks: default no-op.
    fn update_state(&self, _ctx: &Ctx<'_>, _inputs: &[Value], _region: &mut [u64]) {}
}

/// Output-column kind expected for a flattened `CDL.*.Sources.TimeTable` matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeTableValues {
    /// Output columns are numeric Real values.
    Real,
    /// Output columns must be integer-valued Reals.
    Integer,
    /// Output columns must be `0`/`1` Boolean encodings.
    Boolean,
}

/// A class-level parameter validation rule published by the block registry.
///
/// Rules are static metadata for resolved block parameters: `oce-validate` uses them at load time,
/// and `oce-api` mirrors the single-parameter bounds into tune-at-rest metadata. They intentionally
/// describe only CDL semantics, not storage or host UI policy.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamRule {
    /// The named parameter must appear in the resolved [`ParamTable`].
    Required {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// The named parameter changes the block's resolved structure and cannot be edited at rest.
    ///
    /// Examples include vector widths such as `nin`, where changing the value changes the flattened
    /// port arity and therefore requires rebuilding the model rather than resuming an existing
    /// schedule.
    Structural {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// Flattened members of the named array parameter change the resolved block structure.
    ///
    /// Examples include selector arrays such as `extract[nout]` and masks such as `msk[nin]`,
    /// where editing one element changes the source-to-output feedthrough map. Flattened CXF names
    /// use `base_1`, `base_2`, ... and require a fresh model load when changed.
    StructuralArrayElements {
        /// Flattened array base name, for example `"extract"` matching `extract_1`, ...
        base: &'static str,
    },
    /// The named parameter, when present, must be a Boolean value.
    Boolean {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// The named parameter, when present, must be numeric and usable as a `Real`.
    Real {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// The named parameter, when present, must be numeric and finite as a `Real`.
    RealFinite {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
    },
    /// The named `Real` parameter must be strictly greater than `min`.
    RealGreaterThan {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Exclusive lower bound.
        min: f64,
    },
    /// The named `Real` parameter must be finite and strictly greater than `min`.
    RealFiniteGreaterThan {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Exclusive lower bound.
        min: f64,
    },
    /// The named `Integer` parameter must be greater than or equal to `min`.
    IntegerGreaterOrEqual {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive lower bound.
        min: i64,
    },
    /// The named `Integer` parameter must be less than or equal to `max`.
    IntegerLessOrEqualConstant {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive upper bound.
        max: i64,
    },
    /// Present members of a flattened integer array parameter must be integer values.
    ///
    /// The array may be sparse because some CDL array parameters, such as `k[nin]=fill(1,nin)`, have
    /// source defaults for every omitted element. When a member is supplied, this rule rejects a
    /// non-integer value instead of letting the constructor silently fall back to that default.
    IntegerArrayElements {
        /// Flattened array base name, for example `"k"` matching `k_1`, `k_2`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
    },
    /// Present members of a flattened integer array must be in an inclusive parameterized range.
    ///
    /// Sparse members may be allowed when a CDL source default supplies omitted values. When
    /// `default_to_index` is true, omitted member `base_i` validates as value `i`; this models
    /// defaults such as `extract[nout]=1:nout` and catches invalid defaulted selectors.
    IntegerArrayElementsInRange {
        /// Flattened array base name, for example `"extract"` matching `extract_1`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
        /// Default array length when `len` is omitted by source defaulting.
        len_default: i64,
        /// Inclusive lower bound.
        min: i64,
        /// Integer parameter carrying the inclusive upper bound.
        max: &'static str,
        /// Default upper bound when `max` is omitted by source defaulting.
        max_default: i64,
        /// Whether omitted `base_i` defaults to integer value `i`.
        default_to_index: bool,
    },
    /// Present members of a flattened real array parameter must be numeric values usable as `Real`.
    ///
    /// Like [`ParamRule::IntegerArrayElements`], sparse arrays are allowed when the source default
    /// supplies omitted elements. Supplied members accept CDL integer-to-real promotion.
    RealArrayElements {
        /// Flattened array base name, for example `"k"` matching `k_1`, `k_2`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
    },
    /// Present members of a flattened two-dimensional Real matrix parameter must be numeric values.
    ///
    /// Flattened CXF names use one-based row-major keys such as `K_1_1`, `K_1_2`, `K_2_1`, ...
    /// Sparse matrices are allowed only where the source block defines defaults for omitted
    /// members. Supplied members accept CDL integer-to-real promotion.
    RealMatrixElements {
        /// Flattened matrix base name, for example `"K"` matching `K_1_1`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved row count.
        rows: &'static str,
        /// Source default row count when `rows` is omitted.
        default_rows: i64,
        /// Integer parameter carrying the resolved column count.
        cols: &'static str,
        /// Source default column count when `cols` is omitted.
        default_cols: i64,
    },
    /// Flattened source `TimeTable` matrix elements must form a complete rectangular table.
    ///
    /// Flattened names use one-based row-major keys such as `table_1_1`, `table_1_2`, ...
    /// The first column is model time; remaining columns carry typed outputs. `period` is present
    /// only for periodic Integer/Logical sources; `extrapolation` is present only for the Real
    /// source so validation can reject a degenerate periodic range.
    TimeTableMatrix {
        /// Flattened matrix base name, normally `"table"`.
        base: &'static str,
        /// Required value kind for output columns.
        values: TimeTableValues,
        /// Time-scale parameter name.
        time_scale: &'static str,
        /// Optional period parameter name for periodic step tables.
        period: Option<&'static str>,
        /// Optional extrapolation parameter name for Real tables.
        extrapolation: Option<&'static str>,
    },
    /// Present members of a flattened TimeTable offset vector must be numeric and in shape.
    ///
    /// The valid vector length is inferred from the resolved table matrix column count as
    /// `size(table, 2)-1`. Missing members use the CDL source default `0`.
    TimeTableOffset {
        /// Flattened offset vector base name, normally `"offset"`.
        base: &'static str,
        /// Flattened table matrix base name that defines the output count.
        table: &'static str,
    },
    /// Present members of a flattened Boolean array parameter must be Boolean values.
    BooleanArrayElements {
        /// Flattened array base name, for example `"msk"` matching `msk_1`, `msk_2`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
    },
    /// The true count of a flattened Boolean array must equal another integer parameter.
    ///
    /// Missing members use the source default supplied in `default`, so sparse masks validate the
    /// same way source-expanded arrays do.
    BooleanArrayTrueCountEquals {
        /// Flattened array base name, for example `"msk"` matching `msk_1`, ...
        base: &'static str,
        /// Integer parameter carrying the resolved array length.
        len: &'static str,
        /// Integer parameter carrying the expected true count.
        count: &'static str,
        /// Source default for omitted Boolean array members.
        default: bool,
    },
    /// The named enum parameter must be one of the source-verified members.
    EnumMembers {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Allowed member names in source ordinal order.
        members: &'static [&'static str],
    },
    /// The named `Real` parameter must be greater than or equal to `min`.
    RealGreaterOrEqual {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive lower bound.
        min: f64,
    },
    /// The named `Real` parameter must be less than or equal to `max`.
    RealLessOrEqualConstant {
        /// Parameter name as it appears in CDL / the resolved model.
        name: &'static str,
        /// Inclusive upper bound.
        max: f64,
    },
    /// The named `Real` parameter, multiplied by an `Integer` parameter, must be in range.
    RealTimesIntegerInclusiveRange {
        /// Real-valued parameter name.
        real: &'static str,
        /// Integer-valued scale parameter name.
        integer: &'static str,
        /// Inclusive lower bound on `real * integer`.
        min: f64,
        /// Inclusive upper bound on `real * integer`.
        max: f64,
    },
    /// The product of two integer parameters must not exceed a constant upper bound.
    IntegerProductLessOrEqualConstant {
        /// Left-hand integer parameter name.
        left: &'static str,
        /// Right-hand integer parameter name.
        right: &'static str,
        /// Inclusive upper bound on `left * right`.
        max: i64,
    },
    /// The two named `Real` parameters must satisfy `lower <= upper`.
    RealLessOrEqual {
        /// Lower-bound parameter name.
        lower: &'static str,
        /// Upper-bound parameter name.
        upper: &'static str,
    },
    /// The two named `Real` parameters should satisfy `lower <= upper`; violations warn only.
    RealLessOrEqualWarning {
        /// Lower-bound parameter name.
        lower: &'static str,
        /// Upper-bound parameter name.
        upper: &'static str,
    },
    /// The left `Real` parameter should be at least `factor * right`; violations warn only.
    RealGreaterOrEqualScaledWarning {
        /// Left-hand parameter name.
        left: &'static str,
        /// Right-hand parameter name multiplied by `factor`.
        right: &'static str,
        /// Positive scale factor applied to `right`.
        factor: f64,
    },
    /// Equal `Real` parameter values are permitted but should produce a warning.
    RealEqualWarning {
        /// Left-hand parameter name.
        left: &'static str,
        /// Right-hand parameter name.
        right: &'static str,
    },
}

/// A `&'static` registry entry mapping a class path to its block constructor (R-IMPL-2).
pub struct RegistryEntry {
    /// Canonical (or accepted-alias) class path.
    pub class_path: &'static str,
    /// Constructor from a resolved parameter table.
    pub make: fn(&ParamTable) -> Box<dyn Block>,
}

impl RegistryEntry {
    /// Class-level parameter rules for this registered block.
    #[must_use]
    pub fn param_rules(&self) -> &'static [ParamRule] {
        registry::param_rules(self.class_path)
    }
}

// ---- shared hot-path value readers (used by block impls) ------------------------------------
//
// Inputs are gathered from typed connector values that `oce-validate` has already type-checked
// (CDL forbids implicit coercion, §7.10/A.9), so a wrong variant here is a build/validation bug,
// not a runtime condition. The readers never panic on the tick: they `debug_assert` the type and
// fall back to the type's zero in release so a mis-wired model degrades rather than aborting.

/// Read input `i` as a `Real`, defaulting to `0.0` on a (validation-prevented) type mismatch.
#[must_use]
pub(crate) fn read_real(inputs: &[Value], i: usize) -> f64 {
    match inputs.get(i) {
        Some(Value::Real(x)) => *x,
        other => {
            debug_assert!(false, "expected Real input at {i}, found {other:?}");
            0.0
        }
    }
}

/// Emit a `Real` output after canonicalizing target-dependent NaN encodings.
pub(crate) fn emit_real(port: usize, y: f64, emit: &mut dyn FnMut(usize, Value)) {
    emit(port, Value::Real(canonicalize_real(y)));
}

/// Read input `i` as a `Boolean`, defaulting to `false` on a (validation-prevented) type mismatch.
#[must_use]
pub(crate) fn read_bool(inputs: &[Value], i: usize) -> bool {
    match inputs.get(i) {
        Some(Value::Boolean(b)) => *b,
        other => {
            debug_assert!(false, "expected Boolean input at {i}, found {other:?}");
            false
        }
    }
}

/// Read input `i` as an `Integer`, defaulting to `0` on a (validation-prevented) type mismatch.
#[must_use]
pub(crate) fn read_int(inputs: &[Value], i: usize) -> i64 {
    match inputs.get(i) {
        Some(Value::Integer(n)) => *n,
        other => {
            debug_assert!(false, "expected Integer input at {i}, found {other:?}");
            0
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod catalog_tests;

#[cfg(test)]
mod param_rules_tests;

#[cfg(test)]
mod port_names_tests;

#[cfg(test)]
mod reals_arithmetic_tests;

#[cfg(test)]
mod reals_comparators_tests;

#[cfg(test)]
mod reals_transcendental_tests;

#[cfg(test)]
mod reals_matrix_tests;

#[cfg(test)]
mod reals_vector_reductions_tests;

#[cfg(test)]
mod routing_tests;

#[cfg(test)]
mod routing_boolean_tests;

#[cfg(test)]
mod routing_integer_tests;

#[cfg(test)]
mod logical_tests;

#[cfg(test)]
mod conversions_tests;

#[cfg(test)]
mod integers_tests;

#[cfg(test)]
mod logical_latch_tests;

#[cfg(test)]
mod logical_proof_tests;

#[cfg(test)]
mod logical_timing_tests;

#[cfg(test)]
mod logical_timing_contract_tests;

#[cfg(test)]
mod logical_variable_pulse_tests;

#[cfg(test)]
mod integers_edge_tests;

#[cfg(test)]
mod integers_stage_tests;

#[cfg(test)]
mod pid_tests;

#[cfg(test)]
mod psychrometrics_tests;

#[cfg(test)]
mod reals_integrator_tests;

#[cfg(test)]
mod reals_filters_derivative_tests;

#[cfg(test)]
mod reals_filters_tests;

#[cfg(test)]
mod reals_ramp_tests;

#[cfg(test)]
mod reals_sources_tests;

#[cfg(test)]
mod source_pulse_tests;

#[cfg(test)]
mod source_timetable_tests;

#[cfg(test)]
mod discrete_tests;

#[cfg(test)]
mod utilities_tests;
