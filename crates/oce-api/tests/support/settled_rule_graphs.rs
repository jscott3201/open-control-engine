//! The random graphs of `settled_rule_differential.rs` (issue #800, the review of GitHub #805):
//! a seeded generator of small graphs of stateless Boolean logic over pulses, samples, levels and
//! fallback leaves, and the consumers they feed. No new dependency: a xorshift generator.

/// xorshift64*: a fixed sequence per seed.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub(crate) fn coin(&mut self) -> bool {
        self.next() & 1 == 1
    }
}

/// A net of the generated graph: a leaf or a logic node over earlier nets.
#[derive(Clone, Debug)]
pub(crate) enum Net {
    /// Model input `a` or `b`, or a constant.
    Level(Level),
    /// `Change(a)`, `Edge(a)`, `FallingEdge(b)`, `Integers.Change(BooleanToInteger(b)).y`.
    Pulse(PulseKind),
    /// `SampleTrigger(period, shift)`.
    Sample(f64, f64),
    /// `Pre(a)`, `Change(Pre(b))`, `GreaterThreshold(BooleanToReal(Change(a)))`.
    Fallback(FallbackKind),
    /// A logic block over earlier nets.
    Logic(Op, Vec<usize>),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Level {
    A,
    B,
    True,
    False,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum PulseKind {
    ChangeA,
    EdgeA,
    FallingB,
    IntegerChangeB,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FallbackKind {
    PreA,
    ChangePreB,
    ConvertedChangeA,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Op {
    Not,
    And,
    Or,
    Xor,
    Nand,
    Nor,
    MultiAnd,
    MultiOr,
    Switch,
}

impl Op {
    pub(crate) const ALL: [Op; 9] = [
        Op::Not,
        Op::And,
        Op::Or,
        Op::Xor,
        Op::Nand,
        Op::Nor,
        Op::MultiAnd,
        Op::MultiOr,
        Op::Switch,
    ];
    pub(crate) fn arity(self) -> usize {
        match self {
            Op::Not => 1,
            Op::MultiAnd | Op::MultiOr | Op::Switch => 3,
            _ => 2,
        }
    }
    pub(crate) fn class(self) -> &'static str {
        match self {
            Op::Not => "Logical.Not",
            Op::And => "Logical.And",
            Op::Or => "Logical.Or",
            Op::Xor => "Logical.Xor",
            Op::Nand => "Logical.Nand",
            Op::Nor => "Logical.Nor",
            Op::MultiAnd => "Logical.MultiAnd",
            Op::MultiOr => "Logical.MultiOr",
            Op::Switch => "Logical.Switch",
        }
    }
    pub(crate) fn apply(self, x: &[bool]) -> bool {
        match self {
            Op::Not => !x[0],
            Op::And => x[0] && x[1],
            Op::Or => x[0] || x[1],
            Op::Xor => x[0] ^ x[1],
            Op::Nand => !(x[0] && x[1]),
            Op::Nor => !(x[0] || x[1]),
            Op::MultiAnd => x.iter().all(|&b| b),
            Op::MultiOr => x.iter().any(|&b| b),
            Op::Switch => {
                if x[1] {
                    x[0]
                } else {
                    x[2]
                }
            }
        }
    }
}

/// The consumers of one graph: `(class, event-input nets)`.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Consumer {
    Counter(usize),
    CounterReset(usize, usize),
    Sampler(usize),
    Latch(usize, usize),
    Toggle(usize, usize),
}

pub(crate) struct Graph {
    pub(crate) nets: Vec<Net>,
    pub(crate) consumers: Vec<Consumer>,
}

pub(crate) fn generate(rng: &mut Rng) -> Graph {
    let mut nets = Vec::new();
    let leaves = 2 + rng.below(4);
    for _ in 0..leaves {
        nets.push(match rng.below(10) {
            0 => Net::Level([Level::A, Level::B, Level::True, Level::False][rng.below(4)]),
            1..=4 => Net::Pulse(
                [
                    PulseKind::ChangeA,
                    PulseKind::EdgeA,
                    PulseKind::FallingB,
                    PulseKind::IntegerChangeB,
                ][rng.below(4)],
            ),
            5..=7 => Net::Sample([0.5, 1.0, 2.0, 3.0][rng.below(4)], [0.0, 1.0][rng.below(2)]),
            _ => Net::Fallback(
                [
                    FallbackKind::PreA,
                    FallbackKind::ChangePreB,
                    FallbackKind::ConvertedChangeA,
                ][rng.below(3)],
            ),
        });
    }
    for _ in 0..1 + rng.below(5) {
        let op = Op::ALL[rng.below(Op::ALL.len())];
        let inputs = (0..op.arity()).map(|_| rng.below(nets.len())).collect();
        nets.push(Net::Logic(op, inputs));
    }
    let mut consumers = Vec::new();
    for _ in 0..1 + rng.below(3) {
        let mut pick = || rng.below(nets.len());
        let (x, y) = (pick(), pick());
        consumers.push(match rng.below(5) {
            0 => Consumer::Counter(x),
            1 => Consumer::CounterReset(x, y),
            2 => Consumer::Sampler(x),
            3 => Consumer::Latch(x, y),
            _ => Consumer::Toggle(x, y),
        });
    }
    Graph { nets, consumers }
}
