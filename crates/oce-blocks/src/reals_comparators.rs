//! `CDL.Reals` blocks (`03` §4.1) — scalar math is `[A]`; comparators are param-dependent `[A]`/`[S]`.
//! Non-finite policy is intentionally local for M2: `Min`/`Max` absorb a single NaN operand to match
//! `oce-expr`, while `Divide` and the `Line` slope/intercept arithmetic preserve IEEE NaN/±Inf
//! behavior. Comparator comparisons with NaN are false, so a held hysteretic latch resets to false.
//! Centralized non-finite validation/diagnostics is deferred to the future seam.
//! Reals comparators use the Buildings asymmetric hysteresis band: set at the bare comparison point
//! and reset at `point - h` for `Greater*` or `point + h` for `Less*` (not a ±h/2 band).

use oce_model::{ParamTable, Value};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, read_real};

fn is_hysteretic(h: f64) -> bool {
    h > 0.0
}

fn kind_for_h(h: f64) -> BlockKind {
    if is_hysteretic(h) {
        BlockKind::Stateful
    } else {
        BlockKind::Algebraic
    }
}

fn state_len_for_h(h: f64) -> usize {
    usize::from(is_hysteretic(h))
}

fn prev_bool(region: &[u64]) -> bool {
    region.first().copied().unwrap_or(0) != 0
}

fn write_bool(region: &mut [u64], value: bool) {
    if let Some(word) = region.first_mut() {
        *word = u64::from(value);
    }
}

fn emit_bool(value: bool, emit: &mut dyn FnMut(usize, Value)) {
    emit(0, Value::Boolean(value));
}
/// `CDL.Reals.Greater` — `y = u1 > u2` with optional hysteresis `h` (`03` §4.1).
/// Param-dependent `[A]`/`[S]`: `h == 0` is combinational, `h > 0` stores `pre(y)` in one word.
/// Full feedthrough in both modes: even stateful hysteresis is **not** a loop cut (R-REALS-1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Greater {
    pub(crate) h: f64,
    pub(crate) pre_y_start: bool,
}

impl Greater {
    fn y_no_h(inputs: &[Value]) -> bool {
        read_real(inputs, 0) > read_real(inputs, 1)
    }

    fn y_h(&self, inputs: &[Value], prev: bool) -> bool {
        let u1 = read_real(inputs, 0);
        let u2 = read_real(inputs, 1);
        (!prev && u1 > u2) || (prev && u1 > u2 - self.h)
    }
}

impl Block for Greater {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Greater",
            inputs: &[PortKind::Real, PortKind::Real],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        kind_for_h(self.h)
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn state_len(&self) -> usize {
        state_len_for_h(self.h)
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        write_bool(region, self.pre_y_start);
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(Self::y_no_h(inputs), emit);
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_bool(self.y_h(inputs, prev_bool(region)), emit);
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let y = self.y_h(inputs, prev_bool(region));
        write_bool(region, y);
    }
}

/// `CDL.Reals.GreaterThreshold` — `y = u > t` with optional hysteresis `h` (`03` §4.1).
/// Param-dependent `[A]`/`[S]`: `h == 0` is combinational, `h > 0` stores `pre(y)` in one word.
/// Full feedthrough in both modes: even stateful hysteresis is **not** a loop cut (R-REALS-1).
#[derive(Clone, Copy, Debug, Default)]
pub struct GreaterThreshold {
    pub(crate) t: f64,
    pub(crate) h: f64,
    pub(crate) pre_y_start: bool,
}

impl GreaterThreshold {
    fn y_no_h(&self, inputs: &[Value]) -> bool {
        read_real(inputs, 0) > self.t
    }

    fn y_h(&self, inputs: &[Value], prev: bool) -> bool {
        let u = read_real(inputs, 0);
        (!prev && u > self.t) || (prev && u > self.t - self.h)
    }
}

impl Block for GreaterThreshold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.GreaterThreshold",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        kind_for_h(self.h)
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn state_len(&self) -> usize {
        state_len_for_h(self.h)
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        write_bool(region, self.pre_y_start);
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(self.y_no_h(inputs), emit);
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_bool(self.y_h(inputs, prev_bool(region)), emit);
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let y = self.y_h(inputs, prev_bool(region));
        write_bool(region, y);
    }
}

/// `CDL.Reals.Less` — `y = u1 < u2` with optional hysteresis `h` (`03` §4.1).
/// Param-dependent `[A]`/`[S]`: `h == 0` is combinational, `h > 0` stores `pre(y)` in one word.
/// Full feedthrough in both modes: even stateful hysteresis is **not** a loop cut (R-REALS-1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Less {
    pub(crate) h: f64,
    pub(crate) pre_y_start: bool,
}

impl Less {
    fn y_no_h(inputs: &[Value]) -> bool {
        read_real(inputs, 0) < read_real(inputs, 1)
    }

    fn y_h(&self, inputs: &[Value], prev: bool) -> bool {
        let u1 = read_real(inputs, 0);
        let u2 = read_real(inputs, 1);
        (!prev && u1 < u2) || (prev && u1 < u2 + self.h)
    }
}

impl Block for Less {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Less",
            inputs: &[PortKind::Real, PortKind::Real],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        kind_for_h(self.h)
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn state_len(&self) -> usize {
        state_len_for_h(self.h)
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        write_bool(region, self.pre_y_start);
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(Self::y_no_h(inputs), emit);
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_bool(self.y_h(inputs, prev_bool(region)), emit);
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let y = self.y_h(inputs, prev_bool(region));
        write_bool(region, y);
    }
}

/// `CDL.Reals.LessThreshold` — `y = u < t` with optional hysteresis `h` (`03` §4.1).
/// Param-dependent `[A]`/`[S]`: `h == 0` is combinational, `h > 0` stores `pre(y)` in one word.
/// Full feedthrough in both modes: even stateful hysteresis is **not** a loop cut (R-REALS-1).
#[derive(Clone, Copy, Debug, Default)]
pub struct LessThreshold {
    pub(crate) t: f64,
    pub(crate) h: f64,
    pub(crate) pre_y_start: bool,
}

impl LessThreshold {
    fn y_no_h(&self, inputs: &[Value]) -> bool {
        read_real(inputs, 0) < self.t
    }

    fn y_h(&self, inputs: &[Value], prev: bool) -> bool {
        let u = read_real(inputs, 0);
        (!prev && u < self.t) || (prev && u < self.t + self.h)
    }
}

impl Block for LessThreshold {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.LessThreshold",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        kind_for_h(self.h)
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn state_len(&self) -> usize {
        state_len_for_h(self.h)
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        write_bool(region, self.pre_y_start);
    }
    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        emit_bool(self.y_no_h(inputs), emit);
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_bool(self.y_h(inputs, prev_bool(region)), emit);
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let y = self.y_h(inputs, prev_bool(region));
        write_bool(region, y);
    }
}

/// `CDL.Reals.Hysteresis` — Real-to-Boolean hysteresis with thresholds `uLow`/`uHigh`
/// (`03` §4.1). Always `[S]`, stores `pre(y)` in one word, and still feeds through on current
/// `u`: it is stateful, but not a loop cut. `uHigh > uLow` validation is deferred; the equation
/// remains panic-free for input-derived values.
#[derive(Clone, Copy, Debug)]
pub struct Hysteresis {
    pub(crate) u_low: f64,
    pub(crate) u_high: f64,
    pub(crate) pre_y_start: bool,
}

impl Default for Hysteresis {
    fn default() -> Self {
        Self {
            u_low: 0.0,
            u_high: 1.0,
            pre_y_start: false,
        }
    }
}

impl Hysteresis {
    fn y(&self, inputs: &[Value], prev: bool) -> bool {
        let u = read_real(inputs, 0);
        (!prev && u > self.u_high) || (prev && u >= self.u_low)
    }
}

impl Block for Hysteresis {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Reals.Hysteresis",
            inputs: &[PortKind::Real],
            outputs: &[PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }
    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }
    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }
    fn state_len(&self) -> usize {
        1
    }
    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        write_bool(region, self.pre_y_start);
    }
    fn emit_from_state(
        &self,
        _ctx: &Ctx<'_>,
        inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        emit_bool(self.y(inputs, prev_bool(region)), emit);
    }
    fn update_state(&self, _ctx: &Ctx<'_>, inputs: &[Value], region: &mut [u64]) {
        let y = self.y(inputs, prev_bool(region));
        write_bool(region, y);
    }
}
