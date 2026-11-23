//! Event inputs fed by one-frame pulses: the settled-value rule (issue #800, design note
//! `docs/design/pulse-event-inputs.md`, option (g), ruled by the owner on 2026-11-22).
//!
//! HostTick v1 shows every event of the frame interval `(t(k-1), t(k)]` at frame `k`. A Boolean
//! built by stateless logic from levels, primitive pulses (`Change`, `Integers.Change`, `Edge`,
//! `FallingEdge` of a net with no pulse, sample or `Pre` behind it) and samples
//! (`SampleTrigger`) takes three values across Modelica's event iteration at that instant:
//! *before* (pulses and samples false, levels old), *during* (the frame value `v`) and *settled*
//! (pulses false, samples still true, levels new). `oce-graph` computes, for each event input a
//! block declares through [`crate::Block::settled_event_inputs`], the cone with pulses and samples
//! false at this frame's levels (`w`) and the cone with pulses false (`s`), and appends them after
//! the block's declared inputs: `w` then `s` for each declared event input, in declaration order.
//! The block stores `w` where it stored the previous trigger and fires on
//! `(not w(k-1) and v(k)) or (not v(k) and s(k))`. At the initial frame the graph also evaluates
//! the cone at initialisation `i` (pulses at their frame value, samples false) and passes it in
//! place of the declared input's frame value; the input then goes `i -> w -> s` and fires on
//! `(not i and w) or (not w and s)`. Where no pulse or sample is upstream (or the
//! cone falls back, design note decision 5) the graph appends nothing, the block reads `w = s = v`
//! and the rule is exactly the rising edge it applied before.

use oce_model::Value;

use crate::read_bool;

/// What a block's outputs are to an event input's cone (`oce-graph` classifies cones from it).
///
/// Reported by [`crate::Block::event_signal`]; it is a property of the class, never of a
/// parameter value, so no `set_param` edit can change it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum EventSignal {
    /// Anything else: a level (host inputs, constants, comparators, other stateful blocks,
    /// conversions). The default.
    Level,
    /// Stateless Boolean logic, applied pointwise to its inputs within one evaluation: `And`,
    /// `Or`, `Not`, `Nand`, `Nor`, `Xor`, `MultiAnd`, `MultiOr`, `Logical.Switch`, the Boolean
    /// routing classes and the lowering's Boolean pass-through. The cone walks through it.
    Logic,
    /// Every output is an instantaneous pulse of the input in Modelica: `Logical.Change`,
    /// `Integers.Change`, `Logical.Edge`, `Logical.FallingEdge`.
    Pulse,
    /// A `sample()` output, true through every event iteration of its instant:
    /// `Logical.Sources.SampleTrigger`.
    Sample,
    /// Modelica's `pre()`: an event-iteration delay that HostTick v1 shows a frame late
    /// (`Logical.Pre`). A cone that reads one falls back to the rising edge.
    Delay,
}

/// One settled event input at this frame: its frame value `v`, the cone with pulses and samples
/// false `w`, and the cone with pulses false `s`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) struct Settled {
    pub(crate) v: bool,
    pub(crate) w: bool,
    pub(crate) s: bool,
}

/// Read declared Boolean input `port` and, when the graph appended them, its `w` and `s` (the
/// `slot`-th declared settled input of a block with `declared` declared inputs). Without them
/// (no pulse or sample upstream, a fallback cone, or a direct call) `w = s = v`.
pub(crate) fn settled_input(
    inputs: &[Value],
    port: usize,
    declared: usize,
    slot: usize,
) -> Settled {
    // A missing input reads `false`, as the rising-edge code it replaces did when it never got
    // as far as reading `trigger` (an initial frame of a direct call with no inputs).
    let v = inputs.get(port).is_some_and(|_| read_bool(inputs, port));
    let base = declared + 2 * slot;
    match (inputs.get(base), inputs.get(base + 1)) {
        (Some(Value::Boolean(w)), Some(Value::Boolean(s))) => Settled { v, w: *w, s: *s },
        _ => Settled { v, w: v, s: v },
    }
}

/// Whether a settled event input fires at this frame, and in which iteration of the instant.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Fire {
    /// A rise from *before* to *during*: the `when` body reads the inputs' frame values.
    pub(crate) first: bool,
    /// A rise from *during* to *settled*: the body reads the settled values.
    pub(crate) second: bool,
}

impl Fire {
    /// Either iteration fires.
    pub(crate) fn any(self) -> bool {
        self.first || self.second
    }
}

/// The rule for a block whose `when` is inactive at initialisation (the when-only classes of
/// decision 3). At a later frame the input goes `w(k-1) -> v -> s`: `first = v and not prev_w`,
/// `second = not v and s`. At the initial frame (decision 3 as revised on 2026-11-22 after the
/// pinned-class probes `InitNegatedPilot` and `InitMixturesPilot`) the graph passes the cone at
/// initialisation `i` (pulses at their frame value, samples false) in place of `v`, and the input
/// goes `i -> w -> s`: `first = w and not i`, `second = not w and s`. A level (no pulse or sample
/// upstream, or a fallback cone) has `i = w = s` and never fires there, as before (issue #768).
pub(crate) fn fire(initial: bool, prev_w: bool, x: Settled) -> Fire {
    if initial {
        Fire {
            first: x.w && !x.v,
            second: !x.w && x.s,
        }
    } else {
        Fire {
            first: x.v && !prev_w,
            second: !x.v && x.s,
        }
    }
}
