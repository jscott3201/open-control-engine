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
//! Status: **M0 scaffold.** The trait surface and a handful of starter blocks are sketched here;
//! block bodies are stubs (`unimplemented!()`). The full ~130-block catalog is phased across
//! M0–M2 per `03` §7.

use oce_model::{ParamTable, Value};

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

/// Static interface descriptor for an elementary block class. Built once at load time and
/// consumed by `oce-graph` to size buffers and build the DAG. Carries no non-computational
/// metadata (§7.17).
#[derive(Clone, Copy, Debug)]
pub struct BlockSignature {
    /// Canonical class path, e.g. `"CDL.Reals.PID"`.
    pub class_path: &'static str,
    /// Input port kinds in declaration order (index = port index).
    pub inputs: &'static [PortKind],
    /// Output port kinds in declaration order (index = port index).
    pub outputs: &'static [PortKind],
    /// `[S]` if stateful, `[A]` if stateless.
    pub stateful: bool,
}

/// A diagnostics sink for `Utilities.Assert` and unit warnings — injected by the scheduler,
/// never a global, to keep the crate store-free and side-effect-explicit (`03` §2.4).
pub trait Diagnostics {
    /// Emit a warning attributed to `source` at model time `t`.
    fn warn(&self, source: &str, message: &str, t: Time);
}

/// The core elementary-block contract (CDL §7.6) — the **arena model** (`01` §7/§9, the binding
/// FRAME Block-trait resolution). A block object is an *immutable* description (class + parameters
/// resolved at construction); all mutable per-instance `[S]` state lives in an engine-owned flat
/// region (`RunState.words`), never in the block struct. This keeps the frozen schedule shareable
/// and the tick zero-allocation (`01` §7 req 4, §9 req 4), so every method takes `&self`.
///
/// `[A]` blocks override [`Block::step_algebraic`]. `[S]` blocks set [`Block::state_len`], seed via
/// [`Block::init_state`], and override [`Block::emit_from_state`] + [`Block::update_state`].
pub trait Block {
    /// Class-level interface descriptor. Drives buffer sizing and the DAG.
    fn signature(&self) -> &'static BlockSignature;

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
    fn step_algebraic(&self, _inputs: &[Value], _t: Time, _emit: &mut dyn FnMut(usize, Value)) {}

    /// `[S]` output pass: emit outputs from **prior** state — `region` is read-only here — before
    /// any state update this tick (`01` §9 req 2, R-TRAIT-1). `[A]` blocks: default no-op.
    fn emit_from_state(
        &self,
        _inputs: &[Value],
        _t: Time,
        _region: &[u64],
        _emit: &mut dyn FnMut(usize, Value),
    ) {
    }

    /// `[S]` state pass: advance `x(t) → x'(t)` into the mutable `region`, exactly once per tick,
    /// after [`Block::emit_from_state`] (`01` §9 req 2, R-TRAIT-2). `[A]` blocks: default no-op.
    fn update_state(&self, _inputs: &[Value], _t: Time, _region: &mut [u64]) {}
}

/// `CDL.Reals.Sources.Constant` — the only truly stateless source: `y = k` (`03` §4.1).
#[derive(Clone, Copy, Debug)]
pub struct Constant {
    /// The constant output value `k`.
    pub k: f64,
}

impl Block for Constant {
    fn signature(&self) -> &'static BlockSignature {
        &BlockSignature {
            class_path: "CDL.Reals.Sources.Constant",
            inputs: &[],
            outputs: &[PortKind::Real],
            stateful: false,
        }
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false // no inputs — pure source root
    }
    fn step_algebraic(&self, _inputs: &[Value], _t: Time, _emit: &mut dyn FnMut(usize, Value)) {
        unimplemented!("Constant::step_algebraic — M0 scaffold (lands in PR-2)")
    }
}

/// `CDL.Reals.Add` — `y = u1 + u2` (stateless, full feedthrough; `03` §4.1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Add;

impl Block for Add {
    fn signature(&self) -> &'static BlockSignature {
        &BlockSignature {
            class_path: "CDL.Reals.Add",
            inputs: &[PortKind::Real, PortKind::Real],
            outputs: &[PortKind::Real],
            stateful: false,
        }
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn step_algebraic(&self, _inputs: &[Value], _t: Time, _emit: &mut dyn FnMut(usize, Value)) {
        unimplemented!("Add::step_algebraic — M0 scaffold (lands in PR-2)")
    }
}

/// `CDL.Logical.Pre` — `y = pre(u)`, the canonical algebraic-loop breaker (`03` §4.3): stateful,
/// with `feeds_through == false` so it cuts the direct-feedthrough DAG. Per the arena model the
/// one-tick boolean memory lives in the `[S]` state region (one word), not in the struct.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pre;

impl Block for Pre {
    fn signature(&self) -> &'static BlockSignature {
        &BlockSignature {
            class_path: "CDL.Logical.Pre",
            inputs: &[PortKind::Boolean],
            outputs: &[PortKind::Boolean],
            stateful: true,
        }
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false // THE loop cut: output is the prior input, not the current one
    }
    fn state_len(&self) -> usize {
        1 // one word holds the one-tick-delayed boolean
    }
    fn init_state(&self, _region: &mut [u64], _params: &ParamTable) {
        unimplemented!(
            "Pre::init_state — M0 scaffold (seed from the `pre`/`y_start` param in PR-2)"
        )
    }
    fn emit_from_state(
        &self,
        _inputs: &[Value],
        _t: Time,
        _region: &[u64],
        _emit: &mut dyn FnMut(usize, Value),
    ) {
        unimplemented!("Pre::emit_from_state — M0 scaffold (emit the prior boolean in PR-2)")
    }
    fn update_state(&self, _inputs: &[Value], _t: Time, _region: &mut [u64]) {
        unimplemented!("Pre::update_state — M0 scaffold (latch the current input in PR-2)")
    }
}

/// A `&'static` registry entry mapping a class path to its block constructor (R-IMPL-2). The
/// full static catalog lands as the block library grows (M0–M2).
pub struct RegistryEntry {
    /// Canonical (or accepted-alias) class path.
    pub class_path: &'static str,
    /// Constructor from a resolved parameter table.
    pub make: fn(&ParamTable) -> Box<dyn Block>,
}

/// Look up an elementary-block constructor by class path. Unknown class paths surface as
/// unresolved externals (extension blocks), never panics (R-IMPL-2).
#[must_use]
pub fn lookup(_class_path: &str) -> Option<&'static RegistryEntry> {
    unimplemented!("oce-blocks::lookup — M0 scaffold (static registry lands with the catalog)")
}
