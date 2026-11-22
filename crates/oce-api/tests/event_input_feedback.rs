//! Feedback into an event input, and the delay that breaks it (issue #768).
//!
//! `PIDWithReset` (PI, PID), `IntegratorWithReset` and `OnCounter` apply an event at the frame
//! where its input rises, so the event inputs feed through to the output, and a model whose output
//! reaches one of them is an `algebraic-loop`. A `Logical.Pre` on a Boolean event input (or a
//! `Discrete.UnitDelay` on `y_reset_in`) makes it load and delays that input's events by one
//! frame. What it does and does not reproduce of the previous engine, where the block applied the
//! event one frame late itself, is pinned here:
//!
//! - `OnCounter` with every event input delayed, none true at frame 0, matches the previous engine
//!   count for count;
//! - the integrating blocks match it in the frame of each reset and, under steady inputs, in
//!   every output up to and including that frame; afterwards they lag it by the integration step
//!   the reset frame does not take, and with changing inputs the reset value itself differs,
//!   because it is taken from the inputs of the frame the delayed event fires at;
//! - the documented exception: an event input true at frame 0, which the previous engine ignored,
//!   becomes an event at frame 1 behind a `Pre` (`pre_u_start = false`).
//!
//! Every `PREVIOUS_ENGINE` vector is the output of the engine before this change on the model
//! without the delay (recorded from it at `development` 84db2d76 and re-derived by hand in the
//! comments); every other vector is hand-derived from the rule.

use oce_api::{Engine, Value};
use serde_json::{Value as Json, json};

const PREFIX: &str = "http://example.org#evt.";

fn iri(local: &str) -> String {
    format!("{PREFIX}{local}")
}

/// One block: name, class under `CDL.`, `(parameter, data type, literal)`, inputs, outputs.
struct Blk {
    name: &'static str,
    class: &'static str,
    params: Vec<(&'static str, &'static str, &'static str)>,
    ins: Vec<(&'static str, &'static str)>,
    outs: Vec<(&'static str, &'static str)>,
}

/// A flat document: blocks, boundary inputs `(name, type, targets)`, boundary outputs and
/// connections `from -> to` (a port `block.port` or a boundary output name).
#[derive(Default)]
struct Doc {
    blocks: Vec<Blk>,
    inputs: Vec<(&'static str, &'static str, Vec<&'static str>)>,
    outputs: Vec<(&'static str, &'static str)>,
    wires: Vec<(&'static str, &'static str)>,
}

fn data_type(kind: &str) -> Json {
    json!({ "@id": format!("S231:{kind}") })
}

fn literal(kind: &str, value: &str) -> (Json, Json) {
    let xsd = |t: &str| format!("http://www.w3.org/2001/XMLSchema#{t}");
    match kind {
        "SimpleController" => (
            json!({ "@id": "http://example.org#Buildings.Controls.OBC.CDL.Types.SimpleController" }),
            json!(format!(
                "Buildings.Controls.OBC.CDL.Types.SimpleController.{value}"
            )),
        ),
        "Real" => (
            data_type(kind),
            json!({ "@value": value, "@type": xsd("double") }),
        ),
        "Integer" => (
            data_type(kind),
            json!({ "@value": value, "@type": xsd("integer") }),
        ),
        _ => (
            data_type(kind),
            json!({ "@value": value, "@type": xsd("boolean") }),
        ),
    }
}

impl Doc {
    fn bytes(&self) -> Vec<u8> {
        let ids = |names: &mut dyn Iterator<Item = String>| {
            names.map(|n| json!({ "@id": iri(&n) })).collect::<Vec<_>>()
        };
        let targets = |port: &str| {
            let mut to: Vec<_> = self
                .wires
                .iter()
                .filter(|(from, _)| *from == port)
                .map(|(_, to)| json!({ "@id": iri(to) }))
                .collect();
            for (name, _, wired) in &self.inputs {
                if *name == port {
                    to.extend(wired.iter().map(|t| json!({ "@id": iri(t) })));
                }
            }
            to
        };
        let mut graph = vec![json!({
            "@id": "http://example.org#evt",
            "@type": "S231:Block",
            "S231:containsBlock": ids(&mut self.blocks.iter().map(|b| b.name.to_owned())),
            "S231:hasInput": ids(&mut self.inputs.iter().map(|i| i.0.to_owned())),
            "S231:hasOutput": ids(&mut self.outputs.iter().map(|o| o.0.to_owned())),
        })];
        for (name, kind, _) in &self.inputs {
            graph.push(json!({
                "@id": iri(name), "@type": format!("S231:{kind}Input"),
                "S231:isOfDataType": data_type(kind), "S231:isConnectedTo": targets(name),
            }));
        }
        for (name, kind) in &self.outputs {
            graph.push(json!({
                "@id": iri(name), "@type": format!("S231:{kind}Output"),
                "S231:isOfDataType": data_type(kind),
            }));
        }
        for block in &self.blocks {
            let port = |p: &str| format!("{}.{p}", block.name);
            let mut node = json!({
                "@id": iri(block.name),
                "@type": format!("http://example.org#Buildings.Controls.OBC.CDL.{}", block.class),
                "S231:hasInput": ids(&mut block.ins.iter().map(|(p, _)| port(p))),
                "S231:hasOutput": ids(&mut block.outs.iter().map(|(p, _)| port(p))),
            });
            if !block.params.is_empty() {
                node["S231:hasParameter"] =
                    json!(ids(&mut block.params.iter().map(|(p, _, _)| port(p))));
            }
            graph.push(node);
            for (name, kind, value) in &block.params {
                let (data, value) = literal(kind, value);
                graph.push(json!({
                    "@id": iri(&port(name)), "@type": "S231:Parameter",
                    "S231:isOfDataType": data, "S231:value": value,
                }));
            }
            for (name, kind) in &block.ins {
                graph.push(json!({
                    "@id": iri(&port(name)), "@type": format!("S231:{kind}Input"),
                    "S231:isOfDataType": data_type(kind),
                }));
            }
            for (name, kind) in &block.outs {
                let mut node = json!({
                    "@id": iri(&port(name)), "@type": format!("S231:{kind}Output"),
                    "S231:isOfDataType": data_type(kind),
                });
                let to = targets(&port(name));
                if !to.is_empty() {
                    node["S231:isConnectedTo"] = json!(to);
                }
                graph.push(node);
            }
        }
        serde_json::to_vec(&json!({
            "@context": { "S231": "http://data.ashrae.org/S231#" },
            "@graph": graph,
        }))
        .expect("serialize document")
    }

    fn pre(&mut self, name: &'static str) {
        self.blocks.push(Blk {
            name,
            class: "Logical.Pre",
            params: vec![],
            ins: vec![("u", "Boolean")],
            outs: vec![("y", "Boolean")],
        });
    }
}

/// Run frames at t = 0, 1, 2, ... and return the first boundary output of each.
fn run(doc: &Doc, frames: &[Vec<(&str, Value)>]) -> Vec<Value> {
    let mut engine = Engine::new();
    engine
        .load_cxf(&doc.bytes())
        .expect("the delayed model loads");
    frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let names: Vec<String> = frame.iter().map(|(n, _)| iri(n)).collect();
            let entries: Vec<_> = names
                .iter()
                .zip(frame)
                .map(|(n, (_, v))| (n.as_str(), v.clone()))
                .collect();
            let prepared = engine.prepare_frame(index as f64, &entries).unwrap();
            let done = engine.execute_frame(prepared).unwrap();
            done.outputs()[0].1.clone()
        })
        .collect()
}

/// Load `doc`, which must be refused as a connector-level algebraic loop whose message names
/// `remedy` (the delay for the event input the loop passes through).
fn assert_refused_naming(doc: &Doc, remedy: &str) {
    let error = Engine::new()
        .load_cxf(&doc.bytes())
        .expect_err("feedback into an event input is refused");
    let message = error.to_string();
    assert!(message.contains("algebraic loop detected"), "{message}");
    assert!(message.contains(remedy), "{message}");
}

const PRE_REMEDY: &str = "insert CDL.Logical.Pre on the path into it";
const UNIT_DELAY_REMEDY: &str = "insert CDL.Discrete.UnitDelay on the path into it";

fn reals(values: &[f64]) -> Vec<Value> {
    values.iter().map(|&v| Value::Real(v)).collect()
}

fn ints(values: &[i64]) -> Vec<Value> {
    values.iter().map(|&v| Value::Integer(v)).collect()
}

fn assert_bits(got: &[Value], want: &[Value]) {
    assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
    for (frame, (g, w)) in got.iter().zip(want).enumerate() {
        assert!(g.bit_eq(w), "frame {frame}: got {g:?}, want {w:?}\n{got:?}");
    }
}

fn bools(pattern: &str) -> Vec<bool> {
    pattern.chars().map(|c| c == 'T').collect()
}

/// `OnCounter` whose count reaches its own `reset` through `GreaterEqualThreshold(2)`; `trigger`
/// is the boundary input `trg`. With `delayed`, both event inputs pass through a `Pre`.
fn self_resetting_counter(delayed: bool) -> Doc {
    let mut doc = Doc {
        blocks: vec![
            Blk {
                name: "c",
                class: "Integers.OnCounter",
                params: vec![("y_start", "Integer", "0")],
                ins: vec![("trigger", "Boolean"), ("reset", "Boolean")],
                outs: vec![("y", "Integer")],
            },
            Blk {
                name: "thr",
                class: "Integers.GreaterEqualThreshold",
                params: vec![("t", "Integer", "2")],
                ins: vec![("u", "Integer")],
                outs: vec![("y", "Boolean")],
            },
        ],
        outputs: vec![("y", "Integer")],
        wires: vec![("c.y", "y"), ("c.y", "thr.u")],
        ..Doc::default()
    };
    if delayed {
        doc.pre("preTrg");
        doc.pre("preRst");
        doc.inputs.push(("trg", "Boolean", vec!["preTrg.u"]));
        doc.wires.extend([
            ("preTrg.y", "c.trigger"),
            ("thr.y", "preRst.u"),
            ("preRst.y", "c.reset"),
        ]);
    } else {
        doc.inputs.push(("trg", "Boolean", vec!["c.trigger"]));
        doc.wires.push(("thr.y", "c.reset"));
    }
    doc
}

fn trigger_frames(pattern: &str) -> Vec<Vec<(&'static str, Value)>> {
    bools(pattern)
        .into_iter()
        .map(|t| vec![("trg", Value::Boolean(t))])
        .collect()
}

#[test]
fn counter_feeding_its_own_reset_is_an_algebraic_loop() {
    assert_refused_naming(&self_resetting_counter(false), PRE_REMEDY);
}

#[test]
fn counter_with_every_event_input_delayed_matches_the_previous_engine_exactly() {
    // Previous engine, no delay: rises at 1, 3, 5, 7, 9 counted one frame late; the count 2 seen
    // at frame 4 resets it at frame 5 (one frame late again).
    const PREVIOUS_ENGINE: [i64; 10] = [0, 0, 1, 1, 2, 0, 1, 1, 2, 0];
    // Delayed trigger F,F,T,F,T,F,T,F,T,F counts at 2, 4, 6, 8; the delayed reset rises at 5
    // and 9 (the count reached 2 at 4 and 8).
    let got = run(&self_resetting_counter(true), &trigger_frames("FTFTFTFTFT"));
    assert_bits(&got, &ints(&PREVIOUS_ENGINE));
}

#[test]
fn documented_exception_pre_turns_an_event_input_true_at_frame_zero_into_an_event_at_frame_one() {
    // The previous engine ignored a trigger already true at frame 0 (and so does the new block):
    // T,T,F,T,F,T counts the rises at 3 and 5, one frame late.
    const PREVIOUS_ENGINE: [i64; 6] = [0, 0, 0, 0, 1, 1];
    // Behind Pre (pre_u_start = false) the trigger reads F,T,T,F,T,F: a rise at frame 1, which
    // counts, then 2 at frame 4 and the delayed reset at frame 5. This is the one case where
    // delaying every event input does not reproduce the previous engine.
    let got = run(&self_resetting_counter(true), &trigger_frames("TTFTFT"));
    assert_bits(&got, &ints(&[0, 1, 1, 1, 2, 0]));
    assert_ne!(PREVIOUS_ENGINE[1], 1);
}

/// PI(D) `k = Ti = 1`, `y_reset = 0`, limits +-100, `u_m = 0`, whose output reaches `trigger`
/// through `GreaterThreshold(2.5)`, with or without a `Pre` on that path.
fn self_resetting_pid(mode: &'static str, delayed: bool) -> Doc {
    let mut doc = Doc {
        blocks: vec![
            Blk {
                name: "pid",
                class: "Reals.PIDWithReset",
                params: vec![
                    ("controllerType", "SimpleController", mode),
                    ("k", "Real", "1"),
                    ("Ti", "Real", "1"),
                    ("Td", "Real", "0.5"),
                    ("y_reset", "Real", "0"),
                    ("yMax", "Real", "100"),
                    ("yMin", "Real", "-100"),
                ],
                ins: vec![("u_s", "Real"), ("u_m", "Real"), ("trigger", "Boolean")],
                outs: vec![("y", "Real")],
            },
            Blk {
                name: "thr",
                class: "Reals.GreaterThreshold",
                params: vec![("t", "Real", "2.5")],
                ins: vec![("u", "Real")],
                outs: vec![("y", "Boolean")],
            },
        ],
        inputs: vec![
            ("us", "Real", vec!["pid.u_s"]),
            ("um", "Real", vec!["pid.u_m"]),
        ],
        outputs: vec![("y", "Real")],
        wires: vec![("pid.y", "y"), ("pid.y", "thr.u")],
    };
    if delayed {
        doc.pre("pre");
        doc.wires
            .extend([("thr.y", "pre.u"), ("pre.y", "pid.trigger")]);
    } else {
        doc.wires.push(("thr.y", "pid.trigger"));
    }
    doc
}

fn setpoint_frames(u_s: &[f64]) -> Vec<Vec<(&'static str, Value)>> {
    u_s.iter()
        .map(|&u| vec![("us", Value::Real(u)), ("um", Value::Real(0.0))])
        .collect()
}

#[test]
fn reset_pid_feeding_its_own_trigger_is_an_algebraic_loop() {
    for mode in ["PI", "PID"] {
        assert_refused_naming(&self_resetting_pid(mode, false), PRE_REMEDY);
    }
}

#[test]
fn reset_pid_behind_pre_resets_at_the_previous_frame_under_steady_inputs_then_lags_a_step() {
    // e = 1 throughout. Previous engine, no Pre: y = 1, 1, 2, 3 (> 2.5: the trigger rises at 3,
    // I := 0 - 1 in that frame's update), 0 at frame 4, then 1, 2, 3, 0, 1.
    const PREVIOUS_ENGINE: [f64; 10] = [1.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0, 3.0, 0.0, 1.0];
    // Behind Pre the event fires at frame 4 (Pre rises): I := 0 - 1, y = 0, and that frame takes
    // no Euler step, so frame 5 emits 0 again: 1, 1, 2, 3, 0, 0, 1, 2, 3, 0.
    let rule = [1.0, 1.0, 2.0, 3.0, 0.0, 0.0, 1.0, 2.0, 3.0, 0.0];
    for mode in ["PI", "PID"] {
        let got = run(
            &self_resetting_pid(mode, true),
            &setpoint_frames(&[1.0; 10]),
        );
        assert_bits(&got, &reals(&rule));
        // The first frame that shows the reset (4) and every output up to it are the previous
        // engine's; the next frame lags it by the step the reset frame does not take.
        assert_bits(&got[..5], &reals(&PREVIOUS_ENGINE[..5]));
        assert_ne!(got[5].as_real().unwrap(), PREVIOUS_ENGINE[5], "{mode}");
    }
}

#[test]
fn reset_pid_behind_pre_takes_the_reset_value_from_the_frame_the_event_fires_at() {
    // u_s = 1, 1, 2, 1, 3, 1, 2, 1, 1, 1. Previous engine, no Pre: y(2) = 3 raises the trigger
    // at frame 2 and its update stores I = 0 - P(2) = -2, so frame 3 emits 1 - 2 = -1.
    const PREVIOUS_ENGINE: [f64; 10] = [1.0, 1.0, 3.0, -1.0, 2.0, 3.0, 1.0, 2.0, 3.0, 0.0];
    // Behind Pre the event fires at frame 3 with that frame's P = 1: I = -1, y = 0. Same frame,
    // different value: hand-derived 1, 1, 3, 0, 2, 3, 0, -1, 0, 1.
    let rule = [1.0, 1.0, 3.0, 0.0, 2.0, 3.0, 0.0, -1.0, 0.0, 1.0];
    let u_s = [1.0, 1.0, 2.0, 1.0, 3.0, 1.0, 2.0, 1.0, 1.0, 1.0];
    let got = run(&self_resetting_pid("PI", true), &setpoint_frames(&u_s));
    assert_bits(&got, &reals(&rule));
    assert_bits(&got[..3], &reals(&PREVIOUS_ENGINE[..3]));
    assert_ne!(got[3].as_real().unwrap(), PREVIOUS_ENGINE[3]);
}

/// `IntegratorWithReset` (`u = 1`, `y_start = 0`). With `into_trigger`, its output reaches
/// `trigger` through `GreaterThreshold(2.5)` and `y_reset_in` is the boundary input `yr`;
/// otherwise its output reaches `y_reset_in` and the boundary input `trg` drives `trigger`.
/// With `delayed`, every event input passes through its delay: `Pre` for `trigger`,
/// `UnitDelay(samplePeriod = 1 s)` for `y_reset_in`.
fn self_resetting_integrator(into_trigger: bool, delayed: bool) -> Doc {
    let mut doc = Doc {
        blocks: vec![Blk {
            name: "i",
            class: "Reals.IntegratorWithReset",
            params: vec![("k", "Real", "1"), ("y_start", "Real", "0")],
            ins: vec![
                ("u", "Real"),
                ("y_reset_in", "Real"),
                ("trigger", "Boolean"),
            ],
            outs: vec![("y", "Real")],
        }],
        inputs: vec![("u", "Real", vec!["i.u"])],
        outputs: vec![("y", "Real")],
        wires: vec![("i.y", "y")],
    };
    let trigger_source = if into_trigger {
        doc.blocks.push(Blk {
            name: "thr",
            class: "Reals.GreaterThreshold",
            params: vec![("t", "Real", "2.5")],
            ins: vec![("u", "Real")],
            outs: vec![("y", "Boolean")],
        });
        doc.wires.push(("i.y", "thr.u"));
        doc.inputs.push(("yr", "Real", vec!["i.y_reset_in"]));
        "thr.y"
    } else {
        "trg"
    };
    match (into_trigger, delayed) {
        (true, true) => {
            doc.pre("pre");
            doc.wires
                .extend([(trigger_source, "pre.u"), ("pre.y", "i.trigger")]);
        }
        (true, false) => doc.wires.push((trigger_source, "i.trigger")),
        (false, true) => {
            doc.pre("pre");
            doc.inputs.push(("trg", "Boolean", vec!["pre.u"]));
            doc.wires.push(("pre.y", "i.trigger"));
            doc.blocks.push(Blk {
                name: "ud",
                class: "Discrete.UnitDelay",
                params: vec![("samplePeriod", "Real", "1")],
                ins: vec![("u", "Real")],
                outs: vec![("y", "Real")],
            });
            doc.wires
                .extend([("i.y", "ud.u"), ("ud.y", "i.y_reset_in")]);
        }
        (false, false) => {
            doc.inputs.push(("trg", "Boolean", vec!["i.trigger"]));
            doc.wires.push(("i.y", "i.y_reset_in"));
        }
    }
    doc
}

#[test]
fn integrator_feeding_either_event_input_is_an_algebraic_loop() {
    assert_refused_naming(&self_resetting_integrator(true, false), PRE_REMEDY);
    assert_refused_naming(&self_resetting_integrator(false, false), UNIT_DELAY_REMEDY);
}

#[test]
fn integrator_behind_pre_resets_at_the_previous_frame_then_lags_a_step() {
    // Previous engine, no Pre: 0, 0, 1, 2, 3 (> 2.5 at frame 4: reset in that update), 0, 1, 2.
    const PREVIOUS_ENGINE: [f64; 8] = [0.0, 0.0, 1.0, 2.0, 3.0, 0.0, 1.0, 2.0];
    // Behind Pre the event fires at frame 5: emits 0 and stores it with no step, so frame 6
    // emits 0 again: 0, 0, 1, 2, 3, 0, 0, 1 (the hand derivation of the design note's 0, 0, 1,
    // 10, 10, 11 shape with reset 0).
    let rule = [0.0, 0.0, 1.0, 2.0, 3.0, 0.0, 0.0, 1.0];
    let frames: Vec<_> = (0..8)
        .map(|_| vec![("u", Value::Real(1.0)), ("yr", Value::Real(0.0))])
        .collect();
    let got = run(&self_resetting_integrator(true, true), &frames);
    assert_bits(&got, &reals(&rule));
    assert_bits(&got[..6], &reals(&PREVIOUS_ENGINE[..6]));
    assert_ne!(got[6].as_real().unwrap(), PREVIOUS_ENGINE[6]);
}

#[test]
fn integrator_with_both_event_inputs_delayed_resets_to_the_previous_engines_value_then_lags() {
    // Boundary trigger rises at frames 2 and 5; y feeds y_reset_in. Previous engine, no delay:
    // frame 2 stores y(2) = 1, shown at 3; 1 + 1 = 2 at 4; frame 5 stores y(5) = 3, shown at 6.
    const PREVIOUS_ENGINE: [f64; 8] = [0.0, 0.0, 1.0, 1.0, 2.0, 3.0, 3.0, 4.0];
    // Delayed (Pre on trigger, UnitDelay(1 s) on y_reset_in): the event fires at 3 with
    // y_reset_in(3) = y(2) = 1, emits 1 with no step, so 1 again at 4; at 6 with y(5) = 2.
    let rule = [0.0, 0.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0];
    let frames: Vec<_> = (0..8)
        .map(|i| vec![("u", Value::Real(1.0)), ("trg", Value::Boolean(i % 3 == 2))])
        .collect();
    let got = run(&self_resetting_integrator(false, true), &frames);
    assert_bits(&got, &reals(&rule));
    assert_bits(&got[..4], &reals(&PREVIOUS_ENGINE[..4]));
    assert_ne!(got[4].as_real().unwrap(), PREVIOUS_ENGINE[4]);
}
