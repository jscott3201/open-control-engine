//! Every stateless class the event plan treats as cone logic, through the facade (issue #800, the
//! review of GitHub #805): each class with a pulse leaf (`Change(a)`), a sample leaf
//! (`SampleTrigger(1 s)`) and a level leaf (`a`) at one input and the level `b` at the others,
//! into `OnCounter`, compared at every frame with the counts the settled-value rule gives, worked
//! out here from the class's function and the rule (`w(k-1) -> v -> s` at a later frame,
//! `i -> w -> s` at the initial one); a composite boundary, a vector element, a pass-through and a
//! `Pre` on a feedback path into the counter beside them.

use oce_api::{Engine, Value};

const M: &str = "http://example.org#cls";

fn value(kind: &str, v: &str) -> String {
    format!(r#"{{ "@value": "{v}", "@type": "http://www.w3.org/2001/XMLSchema#{kind}" }}"#)
}

/// One block: name, class after `Buildings.Controls.OBC.CDL.`, parameters, input and output
/// ports `(name, kind)`.
struct Blk {
    name: String,
    class: String,
    params: Vec<(&'static str, String)>,
    inputs: Vec<(String, &'static str)>,
    outputs: Vec<(String, &'static str)>,
}

fn blk(
    name: &str,
    class: &str,
    params: Vec<(&'static str, String)>,
    ins: &[&str],
    outs: &[&str],
) -> Blk {
    let kind = |p: &&str| {
        if p.starts_with("index") {
            "Integer"
        } else {
            "Boolean"
        }
    };
    Blk {
        name: name.into(),
        class: class.into(),
        params,
        inputs: ins.iter().map(|p| ((*p).to_owned(), kind(p))).collect(),
        outputs: outs.iter().map(|p| ((*p).to_owned(), "Boolean")).collect(),
    }
}

/// A document: blocks, wires between `block.port`, model inputs `a`, `b` and output `y` from
/// `cnt.y`; `composite` adds the composite `sub` with Boolean boundary `sub.u -> Not -> sub.y` and
/// the pass-through `sub.p -> sub.q`.
fn document(blocks: &[Blk], wires: &[(String, String)], composite: bool) -> String {
    let mut graph = Vec::new();
    let link = |id: &str| -> String {
        let to: Vec<String> = wires
            .iter()
            .filter(|(f, _)| f == id)
            .map(|(_, t)| format!(r#"{{ "@id": "{M}.{t}" }}"#))
            .collect();
        if to.is_empty() {
            String::new()
        } else {
            format!(r#", "S231:isConnectedTo": [{}]"#, to.join(", "))
        }
    };
    let port = |id: &str, kind: &str, dir: &str, extra: &str| -> String {
        format!(
            r#"{{ "@id": "{M}.{id}", "@type": "S231:{kind}{dir}", "S231:isOfDataType": {{ "@id": "S231:{kind}" }}{extra}{} }}"#,
            link(id)
        )
    };
    let refs = |ids: Vec<String>| -> String {
        ids.iter()
            .map(|i| format!(r#"{{ "@id": "{M}.{i}" }}"#))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut members: Vec<String> = blocks.iter().map(|b| b.name.clone()).collect();
    for b in blocks {
        let mut node = format!(
            r#"{{ "@id": "{M}.{}", "@type": "http://example.org#Buildings.Controls.OBC.CDL.{}", "S231:label": "{}""#,
            b.name, b.class, b.name
        );
        if !b.params.is_empty() {
            node += &format!(
                r#", "S231:hasParameter": [{}]"#,
                refs(
                    b.params
                        .iter()
                        .map(|(p, _)| format!("{}.{p}", b.name))
                        .collect()
                )
            );
        }
        if !b.inputs.is_empty() {
            node += &format!(
                r#", "S231:hasInput": [{}]"#,
                refs(
                    b.inputs
                        .iter()
                        .map(|(p, _)| format!("{}.{p}", b.name))
                        .collect()
                )
            );
        }
        node += &format!(
            r#", "S231:hasOutput": [{}] }}"#,
            refs(
                b.outputs
                    .iter()
                    .map(|(p, _)| format!("{}.{p}", b.name))
                    .collect()
            )
        );
        graph.push(node);
        for (p, v) in &b.params {
            graph.push(format!(
                r#"{{ "@id": "{M}.{}.{p}", "S231:value": {v} }}"#,
                b.name
            ));
        }
        for (p, k) in &b.inputs {
            graph.push(port(&format!("{}.{p}", b.name), k, "Input", ""));
        }
        for (p, k) in &b.outputs {
            graph.push(port(&format!("{}.{p}", b.name), k, "Output", ""));
        }
    }
    if composite {
        members.push("sub".into());
        graph.push(format!(
            r#"{{ "@id": "{M}.sub", "@type": "http://example.org#Local.Sub", "S231:label": "sub", "S231:containsBlock": [{}], "S231:hasInput": [{}], "S231:hasOutput": [{}] }}"#,
            refs(vec!["sub.n".into()]),
            refs(vec!["sub.u".into(), "sub.p".into()]),
            refs(vec!["sub.y".into(), "sub.q".into()])
        ));
        graph.push(format!(
            r#"{{ "@id": "{M}.sub.n", "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Not", "S231:label": "n", "S231:hasInput": [{}], "S231:hasOutput": [{}] }}"#,
            refs(vec!["sub.n.u".into()]),
            refs(vec!["sub.n.y".into()])
        ));
        graph.push(port("sub.n.u", "Boolean", "Input", ""));
        graph.push(port("sub.n.y", "Boolean", "Output", ""));
        for p in ["sub.u", "sub.p"] {
            graph.push(port(p, "Boolean", "Input", ""));
        }
        for p in ["sub.y", "sub.q"] {
            graph.push(port(p, "Boolean", "Output", ""));
        }
    }
    for n in ["a", "b"] {
        graph.push(port(
            n,
            "Boolean",
            "Input",
            &format!(r#", "S231:label": "cls.{n}""#),
        ));
    }
    graph.push(port("y", "Integer", "Output", ""));
    graph.insert(
        0,
        format!(
            r#"{{ "@id": "{M}", "@type": "S231:Block", "S231:label": "cls", "S231:containsBlock": [{}], "S231:hasInput": [{}], "S231:hasOutput": [{}] }}"#,
            refs(members),
            refs(vec!["a".into(), "b".into()]),
            refs(vec!["y".into()])
        ),
    );
    format!(
        r#"{{ "@context": {{ "S231": "http://data.ashrae.org/S231#", "base": "http://example.org#" }}, "@graph": [
{}
] }}"#,
        graph.join(",\n")
    )
}

/// The host inputs: `a` changes on consecutive frames and stays; `b` a level.
const A: [bool; 10] = [
    true, false, true, true, false, false, true, false, true, true,
];
const B: [bool; 10] = [
    false, true, true, false, true, false, false, true, true, false,
];

#[derive(Clone, Copy, PartialEq)]
enum Leaf {
    Pulse,
    Sample,
    Level,
}

/// The leaf's `[v, w, s, i]` at frame `k` (frames at whole seconds from 0).
fn leaf(l: Leaf, k: usize) -> [bool; 4] {
    match l {
        Leaf::Pulse => {
            let v = A[k] != (k > 0 && A[k - 1]);
            [v, false, false, v]
        }
        Leaf::Sample => [true, false, true, false],
        Leaf::Level => [A[k]; 4],
    }
}

/// The counts the rule gives for `OnCounter(trigger = cone)` (`y_start = 0`, reset false).
fn counts(l: Leaf, f: impl Fn(bool, bool) -> bool) -> Vec<i64> {
    let (mut n, mut prev_w) = (0, false);
    (0..10)
        .map(|k| {
            let x = leaf(l, k);
            let c: Vec<bool> = x.iter().map(|&xv| f(xv, B[k])).collect();
            // A cone with no pulse or sample: every value is the frame's.
            let [v, w, s, i] = if l == Leaf::Level {
                [c[0]; 4]
            } else {
                [c[0], c[1], c[2], c[3]]
            };
            let seq = if k == 0 { [i, w, s] } else { [prev_w, v, s] };
            n += i64::from(seq[1] && !seq[0]) + i64::from(seq[2] && !seq[1]);
            prev_w = w;
            n
        })
        .collect()
}

/// Run a document with the counter's output `y`.
fn run(text: &str) -> Vec<i64> {
    let mut engine = Engine::new();
    engine
        .load_cxf(text.as_bytes())
        .unwrap_or_else(|e| panic!("loads: {e:?}\n{text}"));
    let (pa, pb, py) = (format!("{M}.a"), format!("{M}.b"), format!("{M}.y"));
    (0..10)
        .map(|k| {
            let f = engine
                .prepare_frame(
                    k as f64,
                    &[
                        (pa.as_str(), Value::Boolean(A[k])),
                        (pb.as_str(), Value::Boolean(B[k])),
                    ],
                )
                .expect("prepares");
            let done = engine.execute_frame(f).expect("executes");
            match done
                .outputs()
                .iter()
                .find(|(p, _)| *p == py)
                .map(|(_, v)| v.clone())
            {
                Some(Value::Integer(n)) => n,
                other => panic!("{other:?}"),
            }
        })
        .collect()
}

/// The leaf's block and its output, `a` for the level.
fn leaf_blocks(l: Leaf) -> (Vec<Blk>, Vec<(String, String)>, String) {
    match l {
        Leaf::Pulse => (
            vec![blk("lf", "Logical.Change", vec![], &["u"], &["y"])],
            vec![("a".into(), "lf.u".into())],
            "lf.y".into(),
        ),
        Leaf::Sample => (
            vec![blk(
                "lf",
                "Logical.Sources.SampleTrigger",
                vec![
                    ("period", value("double", "1.0")),
                    ("shift", value("double", "0.0")),
                ],
                &[],
                &["y"],
            )],
            vec![],
            "lf.y".into(),
        ),
        Leaf::Level => (vec![], vec![], "a".into()),
    }
}

fn counter() -> Blk {
    let mut c = blk(
        "cnt",
        "Integers.OnCounter",
        vec![("y_start", value("integer", "0"))],
        &["trigger", "reset"],
        &[],
    );
    c.outputs = vec![("y".into(), "Integer")];
    c
}

/// One class case: the block, the port the leaf drives, its other ports tied to `b` (or to the
/// constant `false` for an Integer index), the output the counter reads, and the function.
struct Case {
    block: Blk,
    leaf_port: &'static str,
    out: &'static str,
    f: fn(bool, bool) -> bool,
}

fn cases() -> Vec<Case> {
    let n = |name: &str, class: &str, ins: &[&str]| blk(name, class, vec![], ins, &["y"]);
    let nin3 = || vec![("nin", value("integer", "3"))];
    vec![
        Case {
            block: n("x", "Logical.Nand", &["u1", "u2"]),
            leaf_port: "u1",
            out: "y",
            f: |x, b| !(x && b),
        },
        Case {
            block: n("x", "Logical.Nor", &["u1", "u2"]),
            leaf_port: "u1",
            out: "y",
            f: |x, b| !(x || b),
        },
        Case {
            block: n("x", "Logical.Xor", &["u1", "u2"]),
            leaf_port: "u2",
            out: "y",
            f: |x, b| x ^ b,
        },
        Case {
            block: blk("x", "Logical.MultiAnd", nin3(), &["u1", "u2", "u3"], &["y"]),
            leaf_port: "u2",
            out: "y",
            f: |x, b| x && b,
        },
        Case {
            block: blk("x", "Logical.MultiOr", nin3(), &["u1", "u2", "u3"], &["y"]),
            leaf_port: "u3",
            out: "y",
            f: |x, b| x || b,
        },
        // `Logical.Switch`: `y = if u2 then u1 else u3`; the leaf as data, then as the selector.
        Case {
            block: n("x", "Logical.Switch", &["u1", "u2", "u3"]),
            leaf_port: "u1",
            out: "y",
            f: |x, b| if b { x } else { b },
        },
        Case {
            block: n("x", "Logical.Switch", &["u1", "u2", "u3"]),
            leaf_port: "u2",
            out: "y",
            f: |x, b| b && x || !x && b,
        },
        // A vector element: the second of a replicated pair.
        Case {
            block: blk(
                "x",
                "Routing.BooleanScalarReplicator",
                vec![("nout", value("integer", "2"))],
                &["u"],
                &["y1", "y2"],
            ),
            leaf_port: "u",
            out: "y2",
            f: |x, _| x,
        },
        Case {
            block: blk(
                "x",
                "Routing.BooleanVectorReplicator",
                vec![
                    ("nin", value("integer", "2")),
                    ("nout", value("integer", "1")),
                ],
                &["u1", "u2"],
                &["y1_1", "y1_2"],
            ),
            leaf_port: "u2",
            out: "y1_2",
            f: |x, _| x,
        },
    ]
}

#[test]
fn every_cone_class_passes_pulses_samples_and_levels_as_the_rule_says() {
    for case in cases() {
        for l in [Leaf::Pulse, Leaf::Sample, Leaf::Level] {
            let (mut blocks, mut wires, leaf_out) = leaf_blocks(l);
            let mut fal = blk(
                "fal",
                "Logical.Sources.Constant",
                vec![("k", value("boolean", "false"))],
                &[],
                &["y"],
            );
            fal.inputs.clear();
            blocks.push(fal);
            for (p, _) in &case.block.inputs {
                let from = if p == case.leaf_port {
                    leaf_out.clone()
                } else {
                    "b".to_owned()
                };
                wires.push((from, format!("x.{p}")));
            }
            wires.push((format!("x.{}", case.out), "cnt.trigger".into()));
            wires.push(("fal.y".into(), "cnt.reset".into()));
            wires.push(("cnt.y".into(), "y".into()));
            // `a` and `b` must each reach a block whatever the leaf.
            blocks.push(blk("keep", "Logical.And", vec![], &["u1", "u2"], &["y"]));
            wires.push(("a".into(), "keep.u1".into()));
            wires.push(("b".into(), "keep.u2".into()));
            let class = case.block.class.clone();
            blocks.push(case_block(&case));
            blocks.push(counter());
            let got = run(&document(&blocks, &wires, false));
            assert_eq!(got, counts(l, case.f), "{class} leaf at {}", case.leaf_port);
        }
    }
}

fn case_block(case: &Case) -> Blk {
    Blk {
        name: case.block.name.clone(),
        class: case.block.class.clone(),
        params: case.block.params.clone(),
        inputs: case.block.inputs.clone(),
        outputs: case.block.outputs.clone(),
    }
}

#[test]
fn a_composite_boundary_and_a_pass_through_are_transparent_to_the_cone() {
    // `Change(a)` into the composite's `sub.u -> Not -> sub.y`, and `Change(a)` through the
    // pass-through `sub.p -> sub.q`, each into a counter: `Not(Change(a))` and `Change(a)`.
    for (into, from, f) in [
        (
            "sub.u",
            "sub.y",
            (|x: bool, _: bool| !x) as fn(bool, bool) -> bool,
        ),
        ("sub.p", "sub.q", |x, _| x),
    ] {
        let (mut blocks, mut wires, leaf_out) = leaf_blocks(Leaf::Pulse);
        let mut fal = blk(
            "fal",
            "Logical.Sources.Constant",
            vec![("k", value("boolean", "false"))],
            &[],
            &["y"],
        );
        fal.inputs.clear();
        blocks.push(fal);
        blocks.push(counter());
        // Both boundary inputs are driven; the counter reads one of the two outputs.
        wires.push((leaf_out.clone(), "sub.u".into()));
        wires.push((leaf_out, "sub.p".into()));
        wires.push(("sub.u".into(), "sub.n.u".into()));
        wires.push(("sub.n.y".into(), "sub.y".into()));
        wires.push(("sub.p".into(), "sub.q".into()));
        wires.push((from.into(), "cnt.trigger".into()));
        wires.push(("fal.y".into(), "cnt.reset".into()));
        wires.push(("cnt.y".into(), "y".into()));
        // `b` must reach something: an unused And.
        blocks.push(blk("keep", "Logical.And", vec![], &["u1", "u2"], &["y"]));
        wires.push(("a".into(), "keep.u1".into()));
        wires.push(("b".into(), "keep.u2".into()));
        let got = run(&document(&blocks, &wires, true));
        assert_eq!(got, counts(Leaf::Pulse, f), "{into}");
    }
}

#[test]
fn a_pre_on_the_feedback_path_into_the_counter_keeps_the_rising_edge() {
    // `OnCounter(trigger = Or(Change(a), Pre(GreaterEqualThreshold(cnt.y, 3))))`: the `Pre` breaks
    // the loop and puts the cone on the rising edge (decision 5), so consecutive changes merge.
    let (mut blocks, mut wires, leaf_out) = leaf_blocks(Leaf::Pulse);
    let mut fal = blk(
        "fal",
        "Logical.Sources.Constant",
        vec![("k", value("boolean", "false"))],
        &[],
        &["y"],
    );
    fal.inputs.clear();
    blocks.push(fal);
    blocks.push(counter());
    blocks.push(blk("or", "Logical.Or", vec![], &["u1", "u2"], &["y"]));
    let mut ge = blk(
        "ge",
        "Integers.GreaterEqualThreshold",
        vec![("t", value("integer", "3"))],
        &[],
        &["y"],
    );
    ge.inputs = vec![("u".into(), "Integer")];
    blocks.push(ge);
    blocks.push(blk("pre", "Logical.Pre", vec![], &["u"], &["y"]));
    blocks.push(blk("keep", "Logical.And", vec![], &["u1", "u2"], &["y"]));
    wires.extend([
        (leaf_out, "or.u1".to_owned()),
        ("pre.y".into(), "or.u2".into()),
        ("or.y".into(), "cnt.trigger".into()),
        ("fal.y".into(), "cnt.reset".into()),
        ("cnt.y".into(), "ge.u".into()),
        ("ge.y".into(), "pre.u".into()),
        ("cnt.y".into(), "y".into()),
        ("a".into(), "keep.u1".into()),
        ("b".into(), "keep.u2".into()),
    ]);
    let got = run(&document(&blocks, &wires, false));
    // The rising edge of `Or(Change(a), Pre(cnt >= 3))`, worked out by hand: Change(a) is
    // T,T,T,F,T,F,T,T,T,F (frame 0's change is initialisation); the count reaches 3 at frame 6
    // and the `Pre` holds the cone true from frame 7, so nothing rises after.
    let mut want = Vec::new();
    let (mut n, mut prev, mut pre) = (0i64, false, false);
    for k in 0..10 {
        let ch = A[k] != (k > 0 && A[k - 1]);
        let x = ch || pre;
        if k > 0 && x && !prev {
            n += 1;
        }
        prev = x;
        pre = n >= 3;
        want.push(n);
    }
    assert_eq!(got, want);
}
