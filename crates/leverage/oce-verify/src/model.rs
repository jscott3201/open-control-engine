//! What a loaded vector set holds: scenarios, their inputs and their expectations.
//!
//! Every type here is built only by a load ([`crate::VectorSet::from_json_bytes`]), so the rules
//! of the format hold by construction: names are not empty, a step list starts at `t = 0`,
//! increases strictly and never passes the horizon, a window lies inside the clock and holds at
//! least one tick, and a diagnostic instant is a tick. Names are kept as written: resolving them
//! against a sequence's inputs, outputs and parameters, and typing each value by what it lands
//! on, is the binder's work.
//!
//! Maps are held sorted by their keys' UTF-8 bytes, the order the canonical form writes; lists
//! (scenarios, steps, expectations) keep file order.

use crate::number::{Literal, Number};

/// One parameter edit: a parameter path, as written, and the value to set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub(crate) path: String,
    pub(crate) value: Literal,
}

impl Parameter {
    /// The parameter path as written: a canonical path, or a suffix of one. Never empty.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The value as written.
    #[must_use]
    pub fn value(&self) -> Literal {
        self.value
    }
}

/// One mutant the set lists: the path of its CXF document, relative to the vector file, and what
/// it changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mutant {
    pub(crate) path: String,
    pub(crate) description: Option<String>,
}

impl Mutant {
    /// The path as written: `/`-separated, inside the vector file's directory, with no empty,
    /// `.` or `..` segment and no backslash; unique in its set. A run is handed each mutant's
    /// bytes under this path ([`crate::VectorInputs::with_mutants`]).
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The description, when the set gives one.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

/// One step of an input: from `time_s()` on, the input holds `value()` (zero-order hold).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub(crate) t: Number,
    pub(crate) value: Literal,
}

impl Step {
    /// The time in seconds from which the value holds: finite, not negative, at most the
    /// horizon. It need not be a tick; it takes effect at the first tick at or after it.
    #[must_use]
    pub fn time_s(&self) -> f64 {
        self.t.seconds()
    }

    /// The value held, as written.
    #[must_use]
    pub fn value(&self) -> Literal {
        self.value
    }
}

/// How an input was written: one value, or a list of steps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Held {
    /// A constant: one step at `t = 0`, held inline so a set of many constants allocates nothing
    /// per input.
    Constant(Step),
    /// A step list, in time order; never empty.
    Steps(Vec<Step>),
}

/// What one input holds over the clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub(crate) name: String,
    pub(crate) held: Held,
}

impl Input {
    /// The input name as written: a canonical path, or a suffix of one. Never empty.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The steps, in time order: at least one, the first at `t = 0`, each later one strictly
    /// after the one before it, none after the horizon. A constant is one step at `t = 0`.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        match &self.held {
            Held::Constant(step) => std::slice::from_ref(step),
            Held::Steps(steps) => steps,
        }
    }

    /// Whether the set writes this input as a constant rather than as a step list. The two mean
    /// the same schedule when the list has one step; the canonical form keeps the spelling.
    #[must_use]
    pub fn is_constant(&self) -> bool {
        matches!(self.held, Held::Constant(_))
    }
}

/// A window expectation: `output()` equals `equals()` at every tick from `first_tick()` to
/// `last_tick()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub(crate) output: String,
    pub(crate) from: Number,
    pub(crate) to: Number,
    pub(crate) equals: Literal,
    pub(crate) first_tick: u64,
    pub(crate) last_tick: u64,
}

impl Window {
    /// The output name as written: a canonical path, or a suffix of one. Never empty.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.output
    }

    /// The first second of the window: not negative, at most `to_s()`.
    #[must_use]
    pub fn from_s(&self) -> f64 {
        self.from.seconds()
    }

    /// The last second of the window, included: at most the horizon.
    #[must_use]
    pub fn to_s(&self) -> f64 {
        self.to.seconds()
    }

    /// The expected value, as written. The comparison is exact; a Real compares by its bits.
    #[must_use]
    pub fn equals(&self) -> Literal {
        self.equals
    }

    /// The first tick in the window: the first `k` with `from_s() <= k * step_s`.
    #[must_use]
    pub fn first_tick(&self) -> u64 {
        self.first_tick
    }

    /// The last tick in the window: the last `k` with `k * step_s <= to_s()`. Never before
    /// `first_tick()`: a window that holds no tick does not load.
    #[must_use]
    pub fn last_tick(&self) -> u64 {
        self.last_tick
    }
}

/// An instant expectation: the frame at `at_s()` emitted at least one warning whose block is
/// `block()` and, when `message()` is given, whose message equals it byte for byte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticInstant {
    pub(crate) block: String,
    pub(crate) message: Option<String>,
    pub(crate) at: Number,
    pub(crate) tick: u64,
}

impl DiagnosticInstant {
    /// The class path of the block that warns (`CDL.Utilities.Assert`): a class, not an
    /// instance. Never empty.
    #[must_use]
    pub fn block(&self) -> &str {
        &self.block
    }

    /// The message to match byte for byte, when the set gives one.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// The time of the frame in seconds: exactly `tick() as f64 * step_s`.
    #[must_use]
    pub fn at_s(&self) -> f64 {
        self.at.seconds()
    }

    /// The tick of the frame.
    #[must_use]
    pub fn tick(&self) -> u64 {
        self.tick
    }
}

/// One expectation of a scenario, in one of the format's two forms.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expectation {
    /// An output equals a value over a window of ticks.
    Window(Window),
    /// A diagnostic is emitted at one tick.
    Diagnostic(DiagnosticInstant),
}

/// One scenario: what every input holds over the clock, and what is expected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) params: Option<Vec<Parameter>>,
    pub(crate) inputs: Vec<Input>,
    pub(crate) expectations: Vec<Expectation>,
}

impl Scenario {
    /// The scenario name: not empty, unique in its set.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The description, when the set gives one.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// The scenario's own parameter edits, sorted by path bytes, as written and unresolved;
    /// `None` when the scenario has no `params` key (an empty map is `Some` and empty). They
    /// override the set's edits by resolved path once bound.
    #[must_use]
    pub fn params(&self) -> Option<&[Parameter]> {
        self.params.as_deref()
    }

    /// The inputs, sorted by name bytes; no name repeats.
    #[must_use]
    pub fn inputs(&self) -> &[Input] {
        &self.inputs
    }

    /// The expectations, in file order. May be empty: such a scenario checks nothing.
    #[must_use]
    pub fn expectations(&self) -> &[Expectation] {
        &self.expectations
    }
}
