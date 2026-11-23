//! A differential test of the settled-value rule (issue #800, design note
//! `docs/design/pulse-event-inputs.md`): seeded random graphs of stateless Boolean logic over
//! pulses, samples, levels and fallback leaves, feeding `OnCounter`, `TriggeredSampler` (its data
//! input sometimes a converted pulse), `Latch` and `Toggle`, run through the facade for ten frames
//! (random frame-0 values, on and off the sample grid). Every consumer's column is compared with
//! two oracles written here, not from the engine's code:
//!
//! - **the rule**, from the note: each frame a cone takes `w` (pulses and samples false), `v`
//!   (the frame) and `s` (pulses false), and at the initial frame `i` (pulses at their frame
//!   value, samples false). A consumer with a fallback leaf in any event input's cone (a `Pre`, an
//!   edge block fed by a `Pre`, a level computed from a pulse), or a tainted data input, takes the
//!   plain rising edge on every event input; so does a cone with no pulse or sample. Otherwise a
//!   when-only input fires on `w(k-1) -> v -> s` at a later frame and on `i -> w -> s` at the
//!   initial one, once per rising step, the body reading that step's values; `Latch` and `Toggle`
//!   run their initial equations on `i`, then their `.mo`'s `when` per step, `clr`'s value before
//!   a later frame taken as false. The engine must equal it at every frame.
//! - **Modelica** (`support/settled_rule_modelica.rs`): the snapped event iteration of the
//!   language, independent of the rule. The acceptance bar of the reviews of GitHub #805 is
//!   asserted per column: wherever the plain rising edge (the engine before the rule) gives
//!   Modelica's whole column, the engine does too, except in the registered family
//!   `DVG-000008` (a negated sample in `clr`), named by a predicate below.
//!
//! No new dependency: a xorshift generator, a JSON-LD writer of its own.

use oce_api::{Engine, Value};

use crate::settled_rule_documents::{M, document};
use crate::settled_rule_graphs::{
    Consumer, FallbackKind, Graph, Inputs, Level, Net, PulseKind, Rng, generate, sample_index,
};
use crate::settled_rule_modelica::modelica;

// ---------------------------------------------------------------------------------------------
// The independent model.

/// Every net's `(v, w, s, i)` at every frame, and per net whether its cone is active and whether
/// it holds a fallback leaf.
type Cones = (Vec<Vec<[bool; 4]>>, Vec<bool>, Vec<bool>);

fn cones(g: &Graph, times: &[f64], frames: &[Inputs]) -> Cones {
    // Per leaf per frame: the frame value, and the leaf's kind for the shadows.
    let n = g.nets.len();
    let mut out = vec![vec![[false; 4]; times.len()]; n];
    // (has pulse or sample, has fallback) per net.
    let mut kind = vec![(false, false); n];
    for (j, net) in g.nets.iter().enumerate() {
        let mut last_index = -1i64;
        for k in 0..times.len() {
            let x = frames[k];
            let p = if k == 0 { None } else { Some(frames[k - 1]) };
            let vals = match net {
                Net::Level(l) => {
                    let v = match l {
                        Level::A => x.a,
                        Level::B => x.b,
                        Level::True => true,
                        Level::False => false,
                    };
                    [v, v, v, v]
                }
                Net::Pulse(pk) => {
                    // `pre_u_start = false` (and an Integer pre of 0) before the first frame.
                    let (pa, pb) = p.map_or((false, false), |p| (p.a, p.b));
                    let v = match pk {
                        PulseKind::ChangeA => x.a != pa,
                        PulseKind::EdgeA => x.a && !pa,
                        PulseKind::FallingB => !x.b && pb,
                        PulseKind::IntegerChangeB => x.b != pb,
                    };
                    kind[j].0 = true;
                    [v, false, false, v]
                }
                Net::Sample(period, shift) => {
                    let (index, on_grid) = sample_index(times[k], *period, *shift);
                    let v = if k == 0 { on_grid } else { index > last_index };
                    last_index = last_index.max(index);
                    kind[j].0 = true;
                    [v, false, v, false]
                }
                Net::Fallback(fk) => {
                    let (pa, pb, ppb) = (
                        p.is_some_and(|p| p.a),
                        p.is_some_and(|p| p.b),
                        (k >= 2) && frames[k - 2].b,
                    );
                    let v = match fk {
                        FallbackKind::PreA => pa,
                        FallbackKind::ChangePreB => pb != ppb,
                        FallbackKind::ConvertedChangeA => x.a != pa,
                    };
                    kind[j].1 = true;
                    [v; 4]
                }
                Net::Logic(op, ins) => {
                    let mut r = [false; 4];
                    for (c, slot) in r.iter_mut().enumerate() {
                        let args: Vec<bool> = ins.iter().map(|&i| out[i][k][c]).collect();
                        *slot = op.apply(&args);
                    }
                    let (ev, fb) = ins.iter().fold((false, false), |(e, f), &i| {
                        (e || kind[i].0, f || kind[i].1)
                    });
                    kind[j] = (ev, fb);
                    r
                }
            };
            out[j][k] = vals;
        }
    }
    let active: Vec<bool> = kind.iter().map(|&(ev, fb)| ev && !fb).collect();
    // An inactive cone: `w = s = i = v`.
    for (j, &a) in active.iter().enumerate() {
        if !a {
            for f in &mut out[j] {
                *f = [f[0]; 4];
            }
        }
    }
    let fallback = kind.iter().map(|&(_, fb)| fb).collect();
    (out, active, fallback)
}

/// The consumer's event-input nets.
fn event_inputs(consumer: Consumer) -> Vec<usize> {
    match consumer {
        Consumer::Counter(t) | Consumer::Sampler(t, _) => vec![t],
        Consumer::CounterReset(x, y) | Consumer::Latch(x, y) | Consumer::Toggle(x, y) => vec![x, y],
    }
}

/// Whether the rule puts every event input of `consumer` on the plain rising edge: an input's
/// cone holds a fallback leaf, or a data input is tainted (the tainted sampler).
fn plain(consumer: Consumer, fallback: &[bool]) -> bool {
    event_inputs(consumer).iter().any(|&n| fallback[n])
        || matches!(consumer, Consumer::Sampler(_, true))
}

/// A settled input at frame `k`: the values its steps go through and that it stores.
#[derive(Clone, Copy)]
struct Steps {
    /// `[before, middle, settled]`: `[w(k-1), v, s]`, or `[i, w, s]` at the initial frame.
    seq: [bool; 3],
    w: bool,
}

fn steps(c: &[[bool; 4]], k: usize, prev_w: bool, plain: bool) -> Steps {
    let [v, w, s, i] = if plain { [c[k][0]; 4] } else { c[k] };
    Steps {
        seq: if k == 0 { [i, w, s] } else { [prev_w, v, s] },
        w,
    }
}

/// The model's output of one consumer at every frame; `plain` puts every event input on the
/// plain rising edge (what the engine did before the rule, and does for a fallback consumer).
fn model(consumer: Consumer, c: &[Vec<[bool; 4]>], frames: &[Inputs], plain: bool) -> Vec<f64> {
    let n = frames.len();
    let mut out = Vec::with_capacity(n);
    let (mut prev_x, mut prev_y) = (false, false);
    let mut held = if matches!(consumer, Consumer::Sampler(..)) {
        7.0
    } else {
        0.0f64
    };
    let mut y = false;
    for (k, x_in) in frames.iter().enumerate() {
        match consumer {
            Consumer::Counter(t) | Consumer::CounterReset(t, _) | Consumer::Sampler(t, _) => {
                let tr = steps(&c[t], k, prev_x, plain);
                let rs = match consumer {
                    Consumer::CounterReset(_, r) => steps(&c[r], k, prev_y, plain),
                    _ => Steps {
                        seq: [false; 3],
                        w: false,
                    },
                };
                for step in 1..3 {
                    let rose = |s: &Steps| s.seq[step] && !s.seq[step - 1];
                    if rose(&tr) || rose(&rs) {
                        held = match consumer {
                            // The tainted data input is `BooleanToReal(Change(a))` at the frame.
                            Consumer::Sampler(_, true) => {
                                f64::from(u8::from(x_in.a != (k > 0 && frames[k - 1].a)))
                            }
                            Consumer::Sampler(_, false) => x_in.r,
                            _ if rs.seq[step] => 0.0,
                            _ => held + 1.0,
                        };
                    }
                }
                prev_x = tr.w;
                prev_y = rs.w;
                out.push(held);
            }
            Consumer::Latch(u, cl) | Consumer::Toggle(u, cl) => {
                let toggle = matches!(consumer, Consumer::Toggle(..));
                let us = steps(&c[u], k, prev_x, plain);
                let mut cs = steps(&c[cl], k, false, plain);
                if k > 0 {
                    // `clr`'s value before a later frame is not stored: taken as false.
                    cs.seq[0] = false;
                } else {
                    // The initial equations on `i`.
                    y = !cs.seq[0] && us.seq[0];
                }
                for step in 1..3 {
                    let u_rose = us.seq[step] && !us.seq[step - 1];
                    let clr_rose = cs.seq[step] && !cs.seq[step - 1];
                    if toggle {
                        if !cs.seq[step] && u_rose {
                            y = !y;
                        } else if clr_rose {
                            y = false;
                        }
                    } else if u_rose || clr_rose {
                        y = !cs.seq[step] && us.seq[step];
                    }
                }
                prev_x = us.w;
                out.push(f64::from(u8::from(y)));
            }
        }
    }
    out
}

/// Run one graph through the facade at `times` with `frames`: each consumer's output per frame.
fn engine_run(g: &Graph, times: &[f64], frames: &[Inputs]) -> Vec<Vec<f64>> {
    let (text, outputs) = document(g);
    let mut engine = Engine::new();
    engine
        .load_cxf(text.as_bytes())
        .unwrap_or_else(|e| panic!("the generated graph loads: {e:?}\n{text}"));
    let mut out = vec![Vec::new(); outputs.len()];
    let (pa, pb, pr) = (format!("{M}.a"), format!("{M}.b"), format!("{M}.r"));
    for (t, x) in times.iter().zip(frames) {
        let prepared = engine
            .prepare_frame(
                *t,
                &[
                    (pa.as_str(), Value::Boolean(x.a)),
                    (pb.as_str(), Value::Boolean(x.b)),
                    (pr.as_str(), Value::Real(x.r)),
                ],
            )
            .expect("prepares");
        let done = engine.execute_frame(prepared).expect("executes");
        for (c, name) in outputs.iter().enumerate() {
            let path = format!("{M}.{name}");
            let v = done
                .outputs()
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, v)| v.clone())
                .expect("an output");
            out[c].push(match v {
                Value::Real(x) => x,
                Value::Integer(n) => n as f64,
                Value::Boolean(b) => f64::from(u8::from(b)),
                other => panic!("{other:?}"),
            });
        }
    }
    out
}

/// Graphs per run: 3,000, about 6 s in a debug build (one test, so one thread).
const GRAPHS: u64 = 3000;
const FRAMES: usize = 10;

/// The registered family `DVG-000008`: a `Latch` or `Toggle` whose `clr` cone holds a negated
/// sample, true between instants and false at one (`w` true and `s` false at some frame). The
/// engine stores no previous `clr`, so inside this shape it can differ from Modelica where the
/// plain rising edge matches it, for `Latch` and `Toggle`, possibly over several consecutive
/// frames, though never across a whole column where the rising edge is right (the reviews of
/// GitHub #805; accepted by the orchestrator). The exact fix, storing `clr`'s previous settled
/// value, is issue #810.
fn negated_sample_clear(consumer: Consumer, c: &[Vec<[bool; 4]>]) -> bool {
    match consumer {
        Consumer::Latch(_, cl) | Consumer::Toggle(_, cl) => c[cl].iter().any(|x| x[1] && !x[2]),
        _ => false,
    }
}

#[test]
fn random_cones_fire_as_the_settled_value_rule_says_at_every_frame() {
    let (mut active_consumers, mut fallback_consumers, mut frame0_fires) = (0, 0, 0);
    let (mut mixed, mut tainted, mut negated_clear) = (0, 0, 0);
    let (mut bar_checked, mut bar_exempt, mut fixed) = (0, 0, 0);
    for seed in 1..=GRAPHS {
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let g = generate(&mut rng);
        let start = if rng.coin() { 0.0 } else { 0.25 };
        let times: Vec<f64> = (0..FRAMES).map(|k| start + k as f64).collect();
        let frames: Vec<Inputs> = (0..FRAMES)
            .map(|k| Inputs {
                a: rng.coin(),
                b: rng.coin(),
                r: (k as f64) * 1.5 - 4.0,
            })
            .collect();
        let (c, active, fallback) = cones(&g, &times, &frames);
        let engine = engine_run(&g, &times, &frames);
        for (i, &consumer) in g.consumers.iter().enumerate() {
            let context = || {
                format!(
                    "seed {seed}, consumer {consumer:?}, start {start}\nnets {:?}\nframes a {:?} b {:?}",
                    g.nets,
                    frames.iter().map(|x| x.a).collect::<Vec<_>>(),
                    frames.iter().map(|x| x.b).collect::<Vec<_>>()
                )
            };
            let is_plain = plain(consumer, &fallback);
            // The acceptance bar, against a model of Modelica that knows nothing of the rule:
            // wherever the plain rising edge (the engine before the rule) gives Modelica's whole
            // column, the engine does too, but in the registered family above.
            let today = model(consumer, &c, &frames, true);
            let om = modelica(&g, consumer, &times, &frames);
            if today == om {
                if negated_sample_clear(consumer, &c) && engine[i] != om {
                    bar_exempt += 1;
                } else {
                    bar_checked += 1;
                    assert_eq!(engine[i], om, "the acceptance bar: {}", context());
                }
            } else if engine[i] == om {
                fixed += 1;
            }
            // The engine follows the rule.
            let want = model(consumer, &c, &frames, is_plain);
            assert_eq!(engine[i], want, "the rule: {}", context());
            let inputs = event_inputs(consumer);
            mixed += usize::from(
                inputs.len() == 2
                    && inputs.iter().any(|&n| fallback[n])
                    && inputs.iter().any(|&n| active[n]),
            );
            tainted += usize::from(matches!(consumer, Consumer::Sampler(_, true)));
            negated_clear += usize::from(negated_sample_clear(consumer, &c));
            if !is_plain && inputs.iter().any(|&n| active[n]) {
                active_consumers += 1;
                let [_, w, s, i] = c[inputs[0]][0];
                frame0_fires += usize::from((w && !i) || (s && !w));
            } else {
                fallback_consumers += 1;
            }
        }
    }
    // The generator reaches every case it is for.
    assert!(active_consumers > 3000, "{active_consumers} active");
    assert!(
        fallback_consumers > 2000,
        "{fallback_consumers} on the rising edge"
    );
    assert!(frame0_fires > 400, "{frame0_fires} initial-frame events");
    assert!(
        mixed > 500,
        "{mixed} two-input consumers with a fallback and an active input"
    );
    assert!(
        tainted > 300,
        "{tainted} samplers with a tainted data input"
    );
    assert!(
        negated_clear > 150,
        "{negated_clear} negated samples in clr"
    );
    assert!(
        bar_checked > 3000,
        "{bar_checked} columns where the rising edge was Modelica's"
    );
    assert!(fixed > 1000, "{fixed} columns the rule brings to Modelica");
    assert!(
        bar_exempt < bar_checked / 20,
        "{bar_exempt} exempt (DVG-000008)"
    );
}
