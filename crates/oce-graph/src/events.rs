//! Event-input cones and their shadow evaluation (issue #800, design note
//! `docs/design/pulse-event-inputs.md`, option (g), ruled by the owner on 2026-11-22).
//!
//! A block declares, through [`Block::settled_event_inputs`], the inputs whose `when` events follow
//! the settled-value rule. For each, BUILD (and the blob decoder, from the wiring it already
//! validated: no blob byte is added) classifies the stateless Boolean logic upstream of it, its
//! *cone* ([`EventSignal::Logic`] blocks), down to its leaves:
//!
//! - a **sample**: an [`EventSignal::Sample`] output;
//! - a **primitive pulse**: an [`EventSignal::Pulse`] output whose block reads no *tainted* net;
//! - a **fallback leaf**: a [`EventSignal::Delay`] output, a pulse block that reads a tainted net,
//!   or any other leaf whose block is tainted;
//! - a **level**: every other leaf (model inputs, constants, untainted blocks).
//!
//! A block is *tainted* when a pulse, sample or delay block reaches it through any chain of
//! inputs, through conversions, arithmetic and state included: its output can then carry a pulse
//! the cone cannot see as one (`Integers.Change(BooleanToInteger(Change(u)))` is a `Change` of a
//! pulse; decision 5 of the note sends such a cone to the rising edge). An input whose cone holds a
//! pulse or a sample and no fallback leaf is *active*: each frame the shadow pass evaluates the
//! cone twice more, with pulses and samples false (`w`) and with pulses false (`s`), and the block
//! receives both after its declared inputs (see `oce_blocks`' `settled` module). Every other
//! declared input receives nothing and keeps the rising edge, bit for bit.
//!
//! **Cost.** Classification is linear in blocks plus connections: one breadth-first taint pass
//! with an explicit queue, one summary per logic block in schedule order, and one marking pass
//! with an explicit stack; no recursion, so a long `Not` chain cannot exhaust the stack. Per
//! frame, each marked logic block is evaluated twice more, once per net however many consumers
//! share it, into buffers sized at BUILD; nothing allocates.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use oce_blocks::{Block, EventSignal};
use oce_model::{ConnectorId, Model};

use crate::Schedule;

const NONE: u32 = u32::MAX;

/// Where one shadow value pair `(w, s)` comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowSource {
    /// A level (or an inactive input): `w = s =` the current value of this connector.
    Level(ConnectorId),
    /// A primitive pulse: `w = s = false`.
    Pulse,
    /// A sample: `w = false`, `s =` the current value of this connector.
    Sample(ConnectorId),
    /// The output of a marked logic block: its shadow values.
    Node(ConnectorId),
}

/// The frozen event plan of a schedule: which logic blocks the shadow pass evaluates and which
/// blocks receive settled values. Empty when no declared event input has an active cone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventPlan {
    /// Per block (`BlockId.0`): `(start, len)` into `sources` of a marked logic block's declared
    /// inputs, or `None`.
    steps: Vec<Option<(u32, u32)>>,
    /// Per block (`BlockId.0`): `(start, len)` into `sources` of a consumer's settled inputs
    /// (one per [`Block::settled_event_inputs`] entry), or `None`.
    hidden: Vec<Option<(u32, u32)>>,
    /// The flat source table.
    sources: Vec<ShadowSource>,
}

/// What a net is to a cone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leaf {
    Level,
    Pulse,
    Sample,
    Fallback,
    /// The output of a logic block: not a leaf.
    Logic,
}

impl EventPlan {
    /// Whether no block takes part (the evaluator's fast path).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// The shadow sources of block `b` when it is a marked logic block.
    #[must_use]
    pub fn step(&self, b: usize) -> Option<&[ShadowSource]> {
        let (start, len) = (*self.steps.get(b)?)?;
        self.sources.get(start as usize..(start + len) as usize)
    }

    /// The settled sources of block `b` when it receives settled values.
    #[must_use]
    pub fn hidden(&self, b: usize) -> Option<&[ShadowSource]> {
        let (start, len) = (*self.hidden.get(b)?)?;
        self.sources.get(start as usize..(start + len) as usize)
    }

    /// Derive the plan of `model` from its instantiated `blocks` and its schedule's `order` and
    /// `driver_of` (which must be filled). Linear in blocks plus connections; never panics on a
    /// model BUILD (or the blob decoder) accepted.
    #[must_use]
    pub fn derive(model: &Model, blocks: &[Box<dyn Block>], schedule: &Schedule) -> Self {
        if !needs_plan(blocks) {
            return Self::default();
        }
        let nb = model.blocks.len();
        let nc = model.connectors.len();
        let driver = |c: ConnectorId| -> usize {
            schedule
                .driver_of
                .get(c.0 as usize)
                .map_or(c.0 as usize, |d| d.0 as usize)
        };
        // Owner block of every output connector.
        let mut owner = vec![NONE; nc];
        for blk in &model.blocks {
            for out in &blk.outputs {
                if let Some(slot) = owner.get_mut(out.0 as usize) {
                    *slot = blk.id.0;
                }
            }
        }
        let signal: Vec<EventSignal> = blocks.iter().map(|b| b.event_signal()).collect();
        let tainted = taint(model, &owner, &signal, &driver);
        let leaf_of = |c: usize| -> Leaf {
            let Some(&ob) = owner.get(c) else {
                return Leaf::Level;
            };
            if ob == NONE {
                return Leaf::Level;
            }
            let ob = ob as usize;
            match signal[ob] {
                EventSignal::Logic => Leaf::Logic,
                EventSignal::Sample => Leaf::Sample,
                EventSignal::Delay => Leaf::Fallback,
                EventSignal::Pulse => {
                    let reads_taint = model.blocks[ob].inputs.iter().any(|&i| {
                        owner
                            .get(driver(i))
                            .is_some_and(|&o| o != NONE && tainted[o as usize])
                    });
                    if reads_taint {
                        Leaf::Fallback
                    } else {
                        Leaf::Pulse
                    }
                }
                EventSignal::Level => {
                    if tainted[ob] {
                        Leaf::Fallback
                    } else {
                        Leaf::Level
                    }
                }
            }
        };
        // Summaries of logic blocks, in schedule order (their producers come first: logic feeds
        // through every input). bit 0: an event leaf below; bit 1: a fallback leaf below.
        let mut summary = vec![0u8; nb];
        for &bid in &schedule.order {
            let b = bid.0 as usize;
            if signal[b] != EventSignal::Logic {
                continue;
            }
            let mut acc = 0u8;
            for &input in &model.blocks[b].inputs {
                let d = driver(input);
                acc |= match leaf_of(d) {
                    Leaf::Pulse | Leaf::Sample => 1,
                    Leaf::Fallback => 2,
                    Leaf::Level => 0,
                    Leaf::Logic => summary[owner[d] as usize],
                };
            }
            summary[b] = acc;
        }
        let active = |d: usize| -> bool {
            match leaf_of(d) {
                Leaf::Pulse | Leaf::Sample => true,
                Leaf::Logic => summary[owner[d] as usize] == 1,
                Leaf::Level | Leaf::Fallback => false,
            }
        };
        let source_of = |d: usize, active: bool| -> ShadowSource {
            let c = ConnectorId(d as u32);
            if !active {
                return ShadowSource::Level(c);
            }
            match leaf_of(d) {
                Leaf::Pulse => ShadowSource::Pulse,
                Leaf::Sample => ShadowSource::Sample(c),
                Leaf::Logic => ShadowSource::Node(c),
                Leaf::Level | Leaf::Fallback => ShadowSource::Level(c),
            }
        };

        // Every source is a settled input or a logic block's input, and every push onto the stack
        // follows one such edge: one allocation each, never grown.
        let capacity = blocks
            .iter()
            .enumerate()
            .map(|(b, block)| {
                let logic = if signal[b] == EventSignal::Logic {
                    model.blocks[b].inputs.len()
                } else {
                    0
                };
                block.settled_event_inputs().len() + logic
            })
            .sum::<usize>();
        let mut plan = Self {
            steps: vec![None; nb],
            hidden: vec![None; nb],
            sources: Vec::with_capacity(capacity),
        };
        let mut marked = vec![false; nb];
        let mut stack: Vec<u32> = Vec::with_capacity(capacity);
        for (b, block) in blocks.iter().enumerate() {
            let settled = block.settled_event_inputs();
            let inputs = &model.blocks[b].inputs;
            let drive = |p: usize| inputs.get(p).map(|&i| driver(i));
            if settled.iter().any(|&p| drive(p).is_none())
                || !settled.iter().any(|&p| drive(p).is_some_and(&active))
            {
                continue;
            }
            let start = plan.sources.len() as u32;
            for d in settled.iter().filter_map(|&p| drive(p)) {
                let is_active = active(d);
                plan.sources.push(source_of(d, is_active));
                if is_active && leaf_of(d) == Leaf::Logic {
                    stack.push(owner[d]);
                }
            }
            plan.hidden[b] = Some((start, settled.len() as u32));
            // Mark the cone's logic blocks once each (explicit stack, no recursion).
            while let Some(lb) = stack.pop() {
                let lb = lb as usize;
                if core::mem::replace(&mut marked[lb], true) {
                    continue;
                }
                for &input in &model.blocks[lb].inputs {
                    let d = driver(input);
                    if leaf_of(d) == Leaf::Logic && !marked[owner[d] as usize] {
                        stack.push(owner[d]);
                    }
                }
            }
        }
        for (b, &is_marked) in marked.iter().enumerate() {
            if !is_marked {
                continue;
            }
            let start = plan.sources.len() as u32;
            for &input in &model.blocks[b].inputs {
                let d = driver(input);
                plan.sources.push(source_of(d, true));
            }
            plan.steps[b] = Some((start, model.blocks[b].inputs.len() as u32));
        }
        if plan.sources.is_empty() {
            return Self::default();
        }
        plan
    }
}

/// Whether a model's blocks can need an event plan: some block declares settled event inputs
/// and some block is a pulse or a sample. Allocates nothing. When false, [`EventPlan::derive`]
/// returns the empty plan at once and [`crate::allocate_state`] gives the shadow buffers no
/// capacity.
#[must_use]
pub fn needs_plan(blocks: &[Box<dyn Block>]) -> bool {
    blocks.iter().any(|b| !b.settled_event_inputs().is_empty())
        && blocks
            .iter()
            .any(|b| matches!(b.event_signal(), EventSignal::Pulse | EventSignal::Sample))
}

/// Mark every block a pulse, sample or delay block reaches through any chain of inputs (the
/// blocks themselves included). Breadth-first with an explicit queue over a reader table built
/// once: linear in blocks plus connections.
fn taint(
    model: &Model,
    owner: &[u32],
    signal: &[EventSignal],
    driver: &dyn Fn(ConnectorId) -> usize,
) -> Vec<bool> {
    let nb = model.blocks.len();
    // Readers of each block's outputs, as a compressed adjacency table.
    let mut counts = vec![0u32; nb + 1];
    for blk in &model.blocks {
        for &input in &blk.inputs {
            if let Some(&o) = owner.get(driver(input))
                && o != NONE
            {
                counts[o as usize + 1] += 1;
            }
        }
    }
    for i in 0..nb {
        counts[i + 1] += counts[i];
    }
    let mut fill = counts.clone();
    let mut readers = vec![0u32; counts[nb] as usize];
    for blk in &model.blocks {
        for &input in &blk.inputs {
            if let Some(&o) = owner.get(driver(input))
                && o != NONE
            {
                readers[fill[o as usize] as usize] = blk.id.0;
                fill[o as usize] += 1;
            }
        }
    }
    let mut tainted = vec![false; nb];
    // Each block enters the queue at most once.
    let mut queue: Vec<u32> = Vec::with_capacity(nb);
    for (b, s) in signal.iter().enumerate() {
        if matches!(
            s,
            EventSignal::Pulse | EventSignal::Sample | EventSignal::Delay
        ) {
            tainted[b] = true;
            queue.push(b as u32);
        }
    }
    let mut head = 0usize;
    while let Some(&b) = queue.get(head) {
        head += 1;
        let b = b as usize;
        for &r in &readers[counts[b] as usize..counts[b + 1] as usize] {
            if !core::mem::replace(&mut tainted[r as usize], true) {
                queue.push(r);
            }
        }
    }
    tainted
}
