#![forbid(unsafe_code)]
//! `oce-blocks` — the native CDL elementary-block library and the [`Block`] trait for the
//! Open Control Engine (`03-block-library.md`).
//!
//! Every elementary block (CDL §7.6) is a Rust state struct implementing [`Block`], classified
//! statically as stateless `[A]` ([`BlockKind::Algebraic`]) or stateful `[S]`
//! ([`BlockKind::Stateful`]). The split between [`Block::output`] (compute from prior state) and
//! [`Block::update_state`] is what lets `oce-graph` evaluate a tick in topological order and cut
//! algebraic loops at non-feedthrough stateful blocks. This crate is **Group A**: no store, no
//! selene-db (D-OWNER-1); it carries behavior + per-instance state only, never non-computational
//! metadata (CDL §7.17).
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
/// never a global, to keep the crate selene-free and side-effect-explicit (`03` §2.4).
pub trait Diagnostics {
    /// Emit a warning attributed to `source` at model time `t`.
    fn warn(&self, source: &str, message: &str, t: Time);
}

/// The core elementary-block contract (CDL §7.6). `output` computes outputs from
/// `(p, t, u(t), x(t))` without advancing committed state; `update_state` advances
/// `x(t) → x'(t)` once per tick after all `output` calls have settled (`03` §2.2).
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

    /// Initialize/re-initialize state from parameters (CDL has no `start` attribute; the initial
    /// state seeds from a parameter — §7.4, R-TRAIT-4). `[A]` blocks: no-op.
    fn init_state(&mut self, _params: &ParamTable, _t0: Time) {}

    /// Compute outputs from `(p, t, u(t), x(t))` without mutating committed state. For `[A]`
    /// blocks this is the whole computation; for `[S]` blocks it reads prior state (R-TRAIT-1).
    fn output(&self, inputs: &[Value], t: Time, out: &mut Vec<Value>);

    /// Advance state using this tick's inputs/time. Called exactly once per block per tick, after
    /// all `output` calls of the tick are done (R-TRAIT-2). `[A]` blocks: empty default.
    fn update_state(&mut self, _inputs: &[Value], _t: Time) {}
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
    fn output(&self, _inputs: &[Value], _t: Time, _out: &mut Vec<Value>) {
        unimplemented!("Constant::output — M0 scaffold")
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
    fn output(&self, _inputs: &[Value], _t: Time, _out: &mut Vec<Value>) {
        unimplemented!("Add::output — M0 scaffold")
    }
}

/// `CDL.Logical.Pre` — `y = pre(u)`, the canonical algebraic-loop breaker (`03` §4.3): stateful,
/// with `feeds_through == false` so it cuts the direct-feedthrough DAG.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pre {
    /// The one-tick-delayed boolean memory (seeded from a parameter in `init_state`).
    prev: bool,
}

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
    fn output(&self, _inputs: &[Value], _t: Time, _out: &mut Vec<Value>) {
        let _ = self.prev;
        unimplemented!("Pre::output — M0 scaffold")
    }
    fn update_state(&mut self, _inputs: &[Value], _t: Time) {
        unimplemented!("Pre::update_state — M0 scaffold")
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
