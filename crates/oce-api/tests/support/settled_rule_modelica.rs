//! A model of Modelica's snapped event iteration for `settled_rule_differential.rs` (issue #800,
//! the second review of GitHub #805), independent of the settled-value rule: it never computes
//! `w`, `s` or `i`, only what the language does with the generated graph's nodes.
//!
//! Every event of a frame interval is taken at the frame's instant ("snapped", as the design
//! note's `simulate.py`). At a later frame the instant's event iteration starts from the left
//! limit (the hosts at the previous frame, samples, `change` and `edge` false, `pre(x)` at `x`'s
//! left limit) and repeats with the hosts at this frame and each sample due in the interval true,
//! `pre(v) := v` between iterations, until nothing changes (MLS 3.6 operator 3.24). A sample
//! falls without an event after its instant, so no `when` sees that fall. At the initial time:
//! initialisation (hosts at frame 0, `pre_u_start = false`, samples false: "The start value of
//! the special functions initial, terminal, and sample is false", section 8.3.5.1), the event
//! iteration that must follow it with samples false (section 8.6), then the time event of each
//! sample due at the start. That initial sequence is fitted to the pinned-class probes
//! `InitNegatedPilot` and `InitMixturesPilot`. A `when` not enabled with `initial()` fires once per
//! false-to-true step of its condition after initialisation (sections 8.3.5, 8.6); the consumers'
//! bodies are their `.mo` text.

use crate::settled_rule_graphs::{
    Consumer, FallbackKind, Graph, Inputs, Level, Net, PulseKind, sample_frames,
};

/// The node values of one iteration, one per net, and the converted pulse of a tainted sampler.
type State = Vec<bool>;

/// Evaluate every net for one iteration from the hosts `a`, `b`, the samples' values and the
/// previous iteration `prev` (`None` at initialisation, where every `pre` is `pre_u_start`).
fn iterate(g: &Graph, a: bool, b: bool, samples: &[bool], prev: Option<&State>) -> State {
    let mut cur = vec![false; g.nets.len()];
    // `pre(a)`, `pre(b)` and `pre(pre(b))` of this iteration.
    let (pa, pb) = prev.map_or((false, false), |p| (p_host(p, g, 'a'), p_host(p, g, 'b')));
    for (j, net) in g.nets.iter().enumerate() {
        cur[j] = match net {
            Net::Level(Level::A) => a,
            Net::Level(Level::B) => b,
            Net::Level(Level::True) => true,
            Net::Level(Level::False) => false,
            Net::Pulse(PulseKind::ChangeA) | Net::Fallback(FallbackKind::ConvertedChangeA) => {
                a != pa
            }
            Net::Pulse(PulseKind::EdgeA) => a && !pa,
            Net::Pulse(PulseKind::FallingB) => !b && pb,
            Net::Pulse(PulseKind::IntegerChangeB) => b != pb,
            Net::Sample(..) => samples[j],
            Net::Fallback(FallbackKind::PreA) => pa,
            // `change(pre(b))`: `pre(b)` now against its own previous value.
            Net::Fallback(FallbackKind::ChangePreB) => pb != prev.is_some_and(pre_of_b),
            Net::Logic(op, ins) => {
                let args: Vec<bool> = ins.iter().map(|&i| cur[i]).collect();
                op.apply(&args)
            }
        };
    }
    // Carry the hosts and `pre(b)` for the next iteration in two trailing slots.
    cur.push(a);
    cur.push(b);
    cur.push(pb);
    cur
}

fn p_host(p: &State, g: &Graph, which: char) -> bool {
    let n = g.nets.len();
    if which == 'a' { p[n] } else { p[n + 1] }
}

fn pre_of_b(p: &State) -> bool {
    p[p.len() - 1]
}

/// The sequence of states at frame `k`: the left limit (or initialisation) first.
fn sequence(g: &Graph, k: usize, frames: &[Inputs], samples: &[Vec<bool>]) -> Vec<State> {
    let x = frames[k];
    let none = vec![false; g.nets.len()];
    let due: Vec<bool> = (0..g.nets.len())
        .map(|j| {
            samples
                .get(j)
                .is_some_and(|s| s.get(k).copied().unwrap_or(false))
        })
        .collect();
    let settle = |mut seq: Vec<State>, sampling: &[bool]| -> Vec<State> {
        for _ in 0..16 {
            let last = seq.last().expect("a state");
            let next = iterate(g, x.a, x.b, sampling, Some(last));
            if &next == last {
                return seq;
            }
            seq.push(next);
        }
        panic!("the event iteration did not settle");
    };
    if k == 0 {
        // Initialisation, its event iteration, then the samples due at the start.
        let init = iterate(g, x.a, x.b, &none, None);
        let seq = settle(vec![init], &none);
        if due.iter().any(|&d| d) {
            settle(seq, &due)
        } else {
            seq
        }
    } else {
        let p = frames[k - 1];
        // The left limit: hosts at the previous frame, pulses and samples false; `pre(x)` is
        // `x`'s left limit, so the state before it repeats the hosts.
        let before = {
            let mut s = iterate(g, p.a, p.b, &none, None);
            let n = g.nets.len();
            s[n] = p.a;
            s[n + 1] = p.b;
            s[n + 2] = p.b;
            s
        };
        let left = iterate(g, p.a, p.b, &none, Some(&before));
        settle(vec![left], &due)
    }
}

/// The consumer's output at every frame under the model.
pub(crate) fn modelica(
    g: &Graph,
    consumer: Consumer,
    times: &[f64],
    frames: &[Inputs],
) -> Vec<f64> {
    let samples: Vec<Vec<bool>> = g
        .nets
        .iter()
        .map(|n| match n {
            Net::Sample(period, shift) => sample_frames(*period, *shift, times),
            _ => Vec::new(),
        })
        .collect();
    let mut out = Vec::new();
    let mut y = match consumer {
        Consumer::Sampler(..) => 7.0,
        _ => 0.0,
    };
    let mut latched = false;
    for k in 0..frames.len() {
        let seq = sequence(g, k, frames, &samples);
        let at = |s: &State, net: usize| s[net];
        match consumer {
            Consumer::Counter(t) | Consumer::CounterReset(t, _) | Consumer::Sampler(t, _) => {
                let reset = |s: &State| match consumer {
                    Consumer::CounterReset(_, r) => at(s, r),
                    _ => false,
                };
                for w in seq.windows(2) {
                    let rose = (at(&w[1], t) && !at(&w[0], t)) || (reset(&w[1]) && !reset(&w[0]));
                    if rose {
                        y = match consumer {
                            // `y = u` with `u` in this iteration: `r`, or the converted pulse.
                            Consumer::Sampler(_, tainted) => {
                                if tainted {
                                    let pa = w[0][g.nets.len()];
                                    f64::from(u8::from(w[1][g.nets.len()] != pa))
                                } else {
                                    frames[k].r
                                }
                            }
                            _ if reset(&w[1]) => 0.0,
                            _ => y + 1.0,
                        };
                    }
                }
                out.push(y);
            }
            Consumer::Latch(u, cl) | Consumer::Toggle(u, cl) => {
                let toggle = matches!(consumer, Consumer::Toggle(..));
                if k == 0 {
                    // The initial equations on the initialisation values.
                    latched = !at(&seq[0], cl) && at(&seq[0], u);
                }
                for w in seq.windows(2) {
                    let (u0, u1, c0, c1) =
                        (at(&w[0], u), at(&w[1], u), at(&w[0], cl), at(&w[1], cl));
                    if toggle {
                        if !c1 && u1 && !u0 {
                            latched = !latched;
                        } else if c1 && !c0 {
                            latched = false;
                        }
                    } else if (u1 && !u0) || (c1 && !c0) {
                        latched = !c1 && u1;
                    }
                }
                out.push(f64::from(u8::from(latched)));
            }
        }
    }
    out
}
