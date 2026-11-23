//! The CXF documents of `settled_rule_differential.rs` (issue #800, the reviews of GitHub #805):
//! one generated graph as a JSON-LD document of its own writer, the model inputs `a`, `b`, `r`.

use crate::settled_rule_graphs::{Consumer, FallbackKind, Graph, Level, Net, Op, PulseKind};

pub(crate) const M: &str = "http://example.org#diff";

pub(crate) struct Doc {
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
pub(crate) type ConsumerShape = (
    &'static str,
    Vec<(&'static str, String)>,
    Vec<(&'static str, String)>,
    &'static str,
);

/// One block: `(name, class, params, inputs, outputs)` with ports `(name, kind)`.
pub(crate) type Block = (
    String,
    &'static str,
    Vec<(&'static str, String)>,
    Vec<(&'static str, &'static str)>,
    Vec<(&'static str, &'static str)>,
);

pub(crate) fn value(kind: &str, v: &str) -> String {
    format!(r#"{{ "@value": "{v}", "@type": "http://www.w3.org/2001/XMLSchema#{kind}" }}"#)
}

pub(crate) fn document(g: &Graph) -> (String, Vec<String>) {
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
            Consumer::Sampler(t, tainted) => {
                // A tainted data input: `BooleanToReal(Change(a))`, a converted pulse.
                let u = if tainted {
                    blocks.push((
                        format!("{n}c"),
                        "Logical.Change",
                        vec![],
                        vec![("u", "Boolean")],
                        vec![("y", "Boolean")],
                    ));
                    blocks.push((
                        format!("{n}r"),
                        "Conversions.BooleanToReal",
                        vec![],
                        vec![("u", "Boolean")],
                        vec![("y", "Real")],
                    ));
                    wires.push(("a".into(), format!("{n}c.u")));
                    wires.push((format!("{n}c.y"), format!("{n}r.u")));
                    format!("{n}r.y")
                } else {
                    "r".into()
                };
                (
                    "Discrete.TriggeredSampler",
                    vec![("y_start", value("double", "7.0"))],
                    vec![("u", u), ("trigger", net_out[t].clone())],
                    "Real",
                )
            }
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
