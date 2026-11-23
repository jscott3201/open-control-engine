//! A differential test of the settled-value rule (issue #800, design note
//! `docs/design/pulse-event-inputs.md`): seeded random graphs of stateless Boolean logic over
//! pulses, samples, levels and fallback leaves, feeding `OnCounter`, `TriggeredSampler`, `Latch`
//! and `Toggle`, run through the facade for ten frames (random frame-0 values, on and off the
//! sample grid) and compared at every frame with an independent model of the rule written here
//! from the note, not from the engine's code.
//!
//! The model: each frame a cone takes three values, `w` (pulses and samples false), `v` (the frame)
//! and `s` (pulses false), and at the initial frame a fourth, `i` (pulses at their frame value,
//! samples false). A cone with a fallback leaf (a `Pre`, an edge block fed by a `Pre`, a level
//! computed from a pulse) or with no pulse or sample takes `w = s = i = v`. A when-only input
//! fires on `w(k-1) -> v -> s` at a later frame and on `i -> w -> s` at the initial one, once per
//! rising step, the body reading that step's values. `Latch` and `Toggle` run their initial
//! equations on `i`, then their `.mo`'s `when` per step; `clr`'s value before a later frame is
//! taken as false (the registered limit `DVG-000008`, which no generated `clr` here can show
//! wrongly: it needs a negated sample in `clr`, and the model applies the same reading).
//!
//! No new dependency: a xorshift generator, a JSON-LD writer of its own.

use oce_api::{Engine, Value};

use crate::settled_rule_graphs::{
    Consumer, FallbackKind, Graph, Level, Net, Op, PulseKind, Rng, generate,
};

// ---------------------------------------------------------------------------------------------
// The independent model.

/// One frame's host inputs.
#[derive(Clone, Copy)]
struct Inputs {
    a: bool,
    b: bool,
    r: f64,
}

/// `SampleTrigger`'s index and grid test, as `Logical/Sources/SampleTrigger.mo` puts the instants
/// at `phase + k * period` (`phase = mod(shift, period)`), with the engine's 1e-9 tolerance.
fn sample_index(t: f64, period: f64, shift: f64) -> (i64, bool) {
    let phase = shift - (shift / period).floor() * period;
    let x = (t - phase) / period + 1e-9;
    let k = x.floor() as i64;
    (k, x - x.floor() < 2e-9 && k >= 0)
}

/// Every net's `(v, w, s, i)` at every frame, and whether each net's cone is active.
fn cones(g: &Graph, times: &[f64], frames: &[Inputs]) -> (Vec<Vec<[bool; 4]>>, Vec<bool>) {
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
    (out, active)
}

/// A settled input at frame `k`: the values its steps go through and that it stores.
#[derive(Clone, Copy)]
struct Steps {
    /// `[before, middle, settled]`: `[w(k-1), v, s]`, or `[i, w, s]` at the initial frame.
    seq: [bool; 3],
    w: bool,
}

fn steps(c: &[[bool; 4]], k: usize, prev_w: bool) -> Steps {
    let [v, w, s, i] = c[k];
    Steps {
        seq: if k == 0 { [i, w, s] } else { [prev_w, v, s] },
        w,
    }
}

/// The model's output of one consumer at every frame.
fn model(consumer: Consumer, c: &[Vec<[bool; 4]>], frames: &[Inputs]) -> Vec<f64> {
    let n = frames.len();
    let mut out = Vec::with_capacity(n);
    let (mut prev_x, mut prev_y) = (false, false);
    let mut held = if matches!(consumer, Consumer::Sampler(_)) {
        7.0
    } else {
        0.0f64
    };
    let mut y = false;
    for (k, x_in) in frames.iter().enumerate() {
        match consumer {
            Consumer::Counter(t) | Consumer::CounterReset(t, _) | Consumer::Sampler(t) => {
                let tr = steps(&c[t], k, prev_x);
                let rs = match consumer {
                    Consumer::CounterReset(_, r) => steps(&c[r], k, prev_y),
                    _ => Steps {
                        seq: [false; 3],
                        w: false,
                    },
                };
                for step in 1..3 {
                    let rose = |s: &Steps| s.seq[step] && !s.seq[step - 1];
                    if rose(&tr) || rose(&rs) {
                        held = match consumer {
                            Consumer::Sampler(_) => x_in.r,
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
                let us = steps(&c[u], k, prev_x);
                let mut cs = steps(&c[cl], k, false);
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

// ---------------------------------------------------------------------------------------------
// The document.

const M: &str = "http://example.org#diff";

struct Doc {
    graph: Vec<String>,
    members: Vec<String>,
    wires: Vec<(String, String)>,
    outputs: Vec<(String, &'static str)>,
}

impl Doc {
    fn port(&mut self, id: &str, kind: &str, dir: &str) {
        self.labelled_port(id, kind, dir, "");
    }
    fn labelled_port(&mut self, id: &str, kind: &str, dir: &str, label: &str) {
        let targets: Vec<String> = self
            .wires
            .iter()
            .filter(|(f, _)| f == id)
            .map(|(_, t)| format!(r#"{{ "@id": "{M}.{t}" }}"#))
            .collect();
        let link = if targets.is_empty() {
            String::new()
        } else {
            format!(r#", "S231:isConnectedTo": [{}]"#, targets.join(", "))
        };
        self.graph.push(format!(
            r#"{{ "@id": "{M}.{id}", "@type": "S231:{kind}{dir}", "S231:isOfDataType": {{ "@id": "S231:{kind}" }}{label}{link} }}"#
        ));
    }
}

/// A consumer block: its class, parameters, inputs `(port, driving net)` and output kind.
type ConsumerShape = (
    &'static str,
    Vec<(&'static str, String)>,
    Vec<(&'static str, String)>,
    &'static str,
);

/// One block: `(name, class, params, inputs, outputs)` with ports `(name, kind)`.
type Block = (
    String,
    &'static str,
    Vec<(&'static str, String)>,
    Vec<(&'static str, &'static str)>,
    Vec<(&'static str, &'static str)>,
);

fn value(kind: &str, v: &str) -> String {
    format!(r#"{{ "@value": "{v}", "@type": "http://www.w3.org/2001/XMLSchema#{kind}" }}"#)
}

fn document(g: &Graph) -> (String, Vec<String>) {
    let mut blocks: Vec<Block> = Vec::new();
    let mut wires: Vec<(String, String)> = Vec::new();
    let mut net_out: Vec<String> = Vec::new();
    let b1 = |name: &str, class: &'static str| -> Block {
        (
            name.to_owned(),
            class,
            vec![],
            vec![("u", "Boolean")],
            vec![("y", "Boolean")],
        )
    };
    blocks.push((
        "fal".into(),
        "Logical.Sources.Constant",
        vec![("k", value("boolean", "false"))],
        vec![],
        vec![("y", "Boolean")],
    ));
    blocks.push((
        "tru".into(),
        "Logical.Sources.Constant",
        vec![("k", value("boolean", "true"))],
        vec![],
        vec![("y", "Boolean")],
    ));
    for (j, net) in g.nets.iter().enumerate() {
        let n = format!("n{j}");
        let out = match net {
            Net::Level(Level::A) => "a".to_owned(),
            Net::Level(Level::B) => "b".to_owned(),
            Net::Level(Level::True) => "tru.y".to_owned(),
            Net::Level(Level::False) => "fal.y".to_owned(),
            Net::Pulse(PulseKind::ChangeA) => {
                blocks.push(b1(&n, "Logical.Change"));
                wires.push(("a".into(), format!("{n}.u")));
                format!("{n}.y")
            }
            Net::Pulse(PulseKind::EdgeA) => {
                blocks.push(b1(&n, "Logical.Edge"));
                wires.push(("a".into(), format!("{n}.u")));
                format!("{n}.y")
            }
            Net::Pulse(PulseKind::FallingB) => {
                blocks.push(b1(&n, "Logical.FallingEdge"));
                wires.push(("b".into(), format!("{n}.u")));
                format!("{n}.y")
            }
            Net::Pulse(PulseKind::IntegerChangeB) => {
                blocks.push((
                    format!("{n}i"),
                    "Conversions.BooleanToInteger",
                    vec![],
                    vec![("u", "Boolean")],
                    vec![("y", "Integer")],
                ));
                blocks.push((
                    n.clone(),
                    "Integers.Change",
                    vec![],
                    vec![("u", "Integer")],
                    vec![("y", "Boolean"), ("up", "Boolean"), ("down", "Boolean")],
                ));
                wires.push(("b".into(), format!("{n}i.u")));
                wires.push((format!("{n}i.y"), format!("{n}.u")));
                format!("{n}.y")
            }
            Net::Sample(period, shift) => {
                blocks.push((
                    n.clone(),
                    "Logical.Sources.SampleTrigger",
                    vec![
                        ("period", value("double", &format!("{period:?}"))),
                        ("shift", value("double", &format!("{shift:?}"))),
                    ],
                    vec![],
                    vec![("y", "Boolean")],
                ));
                format!("{n}.y")
            }
            Net::Fallback(FallbackKind::PreA) => {
                blocks.push(b1(&n, "Logical.Pre"));
                wires.push(("a".into(), format!("{n}.u")));
                format!("{n}.y")
            }
            Net::Fallback(FallbackKind::ChangePreB) => {
                blocks.push(b1(&format!("{n}p"), "Logical.Pre"));
                blocks.push(b1(&n, "Logical.Change"));
                wires.push(("b".into(), format!("{n}p.u")));
                wires.push((format!("{n}p.y"), format!("{n}.u")));
                format!("{n}.y")
            }
            Net::Fallback(FallbackKind::ConvertedChangeA) => {
                blocks.push(b1(&format!("{n}c"), "Logical.Change"));
                blocks.push((
                    format!("{n}r"),
                    "Conversions.BooleanToReal",
                    vec![],
                    vec![("u", "Boolean")],
                    vec![("y", "Real")],
                ));
                blocks.push((
                    n.clone(),
                    "Reals.GreaterThreshold",
                    vec![("t", value("double", "0.5"))],
                    vec![("u", "Real")],
                    vec![("y", "Boolean")],
                ));
                wires.push(("a".into(), format!("{n}c.u")));
                wires.push((format!("{n}c.y"), format!("{n}r.u")));
                wires.push((format!("{n}r.y"), format!("{n}.u")));
                format!("{n}.y")
            }
            Net::Logic(op, ins) => {
                let ports: &[&'static str] = match op.arity() {
                    1 => &["u"],
                    2 => &["u1", "u2"],
                    _ => &["u1", "u2", "u3"],
                };
                let params = match op {
                    Op::MultiAnd | Op::MultiOr => vec![("nin", value("integer", "3"))],
                    _ => vec![],
                };
                blocks.push((
                    n.clone(),
                    op.class(),
                    params,
                    ports.iter().map(|p| (*p, "Boolean")).collect(),
                    vec![("y", "Boolean")],
                ));
                for (p, &i) in ports.iter().zip(ins) {
                    wires.push((net_out[i].clone(), format!("{n}.{p}")));
                }
                format!("{n}.y")
            }
        };
        net_out.push(out);
    }
    // Every host input reaches a model output, so the facade knows all three at every frame.
    blocks.push((
        "keep".into(),
        "Logical.And",
        vec![],
        vec![("u1", "Boolean"), ("u2", "Boolean")],
        vec![("y", "Boolean")],
    ));
    blocks.push((
        "keepr".into(),
        "Reals.GreaterThreshold",
        vec![("t", value("double", "0.0"))],
        vec![("u", "Real")],
        vec![("y", "Boolean")],
    ));
    wires.push(("a".into(), "keep.u1".into()));
    wires.push(("b".into(), "keep.u2".into()));
    wires.push(("r".into(), "keepr.u".into()));
    wires.push(("keep.y".into(), "y_keep".into()));
    wires.push(("keepr.y".into(), "y_keepr".into()));
    let mut outputs = vec![
        ("y_keep".to_owned(), "Boolean"),
        ("y_keepr".to_owned(), "Boolean"),
    ];
    for (c, consumer) in g.consumers.iter().enumerate() {
        let n = format!("c{c}");
        let (class, params, ins, kind): ConsumerShape = match *consumer {
            Consumer::Counter(t) => (
                "Integers.OnCounter",
                vec![("y_start", value("integer", "0"))],
                vec![("trigger", net_out[t].clone()), ("reset", "fal.y".into())],
                "Integer",
            ),
            Consumer::CounterReset(t, r) => (
                "Integers.OnCounter",
                vec![("y_start", value("integer", "0"))],
                vec![
                    ("trigger", net_out[t].clone()),
                    ("reset", net_out[r].clone()),
                ],
                "Integer",
            ),
            Consumer::Sampler(t) => (
                "Discrete.TriggeredSampler",
                vec![("y_start", value("double", "7.0"))],
                vec![("u", "r".into()), ("trigger", net_out[t].clone())],
                "Real",
            ),
            Consumer::Latch(u, cl) => (
                "Logical.Latch",
                vec![],
                vec![("u", net_out[u].clone()), ("clr", net_out[cl].clone())],
                "Boolean",
            ),
            Consumer::Toggle(u, cl) => (
                "Logical.Toggle",
                vec![],
                vec![("u", net_out[u].clone()), ("clr", net_out[cl].clone())],
                "Boolean",
            ),
        };
        let port_kinds: Vec<(&'static str, &'static str)> = ins
            .iter()
            .map(|(p, _)| {
                (
                    *p,
                    if *p == "u" && class.contains("Sampler") {
                        "Real"
                    } else {
                        "Boolean"
                    },
                )
            })
            .collect();
        for (p, from) in &ins {
            wires.push((from.clone(), format!("{n}.{p}")));
        }
        blocks.push((n.clone(), class, params, port_kinds, vec![("y", kind)]));
        wires.push((format!("{n}.y"), format!("y_{n}")));
        outputs.push((format!("y_{n}"), kind));
    }
    let mut doc = Doc {
        graph: Vec::new(),
        members: Vec::new(),
        wires,
        outputs,
    };
    for (name, class, params, ins, outs) in &blocks {
        let id = format!("{M}.{name}");
        doc.members.push(format!(r#"{{ "@id": "{id}" }}"#));
        let refs = |ps: &[(&str, &str)]| {
            ps.iter()
                .map(|(p, _)| format!(r#"{{ "@id": "{id}.{p}" }}"#))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut node = format!(
            r#"{{ "@id": "{id}", "@type": "http://example.org#Buildings.Controls.OBC.CDL.{class}", "S231:label": "{name}""#
        );
        if !params.is_empty() {
            let ps: Vec<(&str, &str)> = params.iter().map(|(p, _)| (*p, "")).collect();
            node += &format!(r#", "S231:hasParameter": [{}]"#, refs(&ps));
        }
        if !ins.is_empty() {
            node += &format!(r#", "S231:hasInput": [{}]"#, refs(ins));
        }
        node += &format!(r#", "S231:hasOutput": [{}] }}"#, refs(outs));
        doc.graph.push(node);
        for (p, v) in params {
            doc.graph
                .push(format!(r#"{{ "@id": "{id}.{p}", "S231:value": {v} }}"#));
        }
        for (p, k) in ins {
            doc.port(&format!("{name}.{p}"), k, "Input");
        }
        for (p, k) in outs {
            doc.port(&format!("{name}.{p}"), k, "Output");
        }
    }
    for (n, k) in [("a", "Boolean"), ("b", "Boolean"), ("r", "Real")] {
        doc.labelled_port(n, k, "Input", &format!(r#", "S231:label": "diff.{n}""#));
    }
    let outs = doc.outputs.clone();
    for (n, k) in &outs {
        doc.port(n, k, "Output");
    }
    let top = format!(
        r#"{{ "@id": "{M}", "@type": "S231:Block", "S231:label": "diff", "S231:containsBlock": [{}], "S231:hasInput": [{}], "S231:hasOutput": [{}] }}"#,
        doc.members.join(", "),
        ["a", "b", "r"]
            .map(|n| format!(r#"{{ "@id": "{M}.{n}" }}"#))
            .join(", "),
        outs.iter()
            .map(|(n, _)| format!(r#"{{ "@id": "{M}.{n}" }}"#))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let text = format!(
        r#"{{ "@context": {{ "S231": "http://data.ashrae.org/S231#", "base": "http://example.org#" }}, "@graph": [
{top},
{}
] }}"#,
        doc.graph.join(",\n")
    );
    // The consumers' outputs, after the two that keep the host inputs.
    (text, outs.into_iter().skip(2).map(|(n, _)| n).collect())
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

/// Graphs per run: a few thousand, which takes seconds under two test threads.
const GRAPHS: u64 = 3000;
const FRAMES: usize = 10;

#[test]
fn random_cones_fire_as_the_settled_value_rule_says_at_every_frame() {
    let (mut active_consumers, mut fallback_consumers, mut frame0_fires) = (0, 0, 0);
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
        let (c, active) = cones(&g, &times, &frames);
        let engine = engine_run(&g, &times, &frames);
        for (i, consumer) in g.consumers.iter().enumerate() {
            let want = model(*consumer, &c, &frames);
            assert_eq!(
                engine[i],
                want,
                "seed {seed}, consumer {consumer:?}, start {start}\nnets {:?}\nframes a {:?} b {:?}",
                g.nets,
                frames.iter().map(|x| x.a).collect::<Vec<_>>(),
                frames.iter().map(|x| x.b).collect::<Vec<_>>()
            );
            let inputs: Vec<usize> = match *consumer {
                Consumer::Counter(t) | Consumer::Sampler(t) => vec![t],
                Consumer::CounterReset(x, y) | Consumer::Latch(x, y) | Consumer::Toggle(x, y) => {
                    vec![x, y]
                }
            };
            if inputs.iter().any(|&n| active[n]) {
                active_consumers += 1;
                let [v, w, s, i] = c[inputs[0]][0];
                let _ = v;
                frame0_fires += usize::from((w && !i) || (s && !w));
            } else {
                fallback_consumers += 1;
            }
        }
    }
    // The generator reaches every case it is for.
    assert!(active_consumers > 2000, "{active_consumers} active");
    assert!(
        fallback_consumers > 1000,
        "{fallback_consumers} on the rising edge"
    );
    assert!(frame0_fires > 100, "{frame0_fires} initial-frame events");
}
