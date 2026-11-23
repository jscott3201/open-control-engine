//! The engine's CXF documents for the pulse-event pilots of issue #800
//! (`tools/openmodelica-event-reference/events/PulseConsumersPilot.mo`, `SampleTriggerPilot.mo`,
//! `SampleTriggerOffGridPilot.mo`): the same CDL blocks wired the same way, the pilots' table
//! columns as the model's inputs, and the compared signals as its outputs.

/// One block of a document: its local name, CDL class path after `Buildings.Controls.OBC.`, its
/// parameters `(name, JSON value)`, and its ports `(name, kind)`.
pub(crate) struct Blk {
    pub(crate) name: &'static str,
    pub(crate) class: &'static str,
    pub(crate) params: Vec<(&'static str, String)>,
    pub(crate) inputs: Vec<(&'static str, &'static str)>,
    pub(crate) outputs: Vec<(&'static str, &'static str)>,
}

/// A whole document: blocks, wires `(from, to)` between `block.port` or model-input names, model
/// inputs `(name, kind)` and model outputs `(name, kind, from)`; output `name` is the model's
/// connector `y_<name>` (a block may share the name).
pub(crate) struct Doc {
    pub(crate) model: &'static str,
    pub(crate) blocks: Vec<Blk>,
    pub(crate) wires: Vec<(&'static str, &'static str)>,
    pub(crate) inputs: Vec<(&'static str, &'static str)>,
    pub(crate) outputs: Vec<(&'static str, &'static str, &'static str)>,
}

fn double(v: &str) -> String {
    format!(r#"{{ "@value": "{v}", "@type": "http://www.w3.org/2001/XMLSchema#double" }}"#)
}
fn integer(v: &str) -> String {
    format!(r#"{{ "@value": "{v}", "@type": "http://www.w3.org/2001/XMLSchema#integer" }}"#)
}
fn boolean(v: &str) -> String {
    format!(r#"{{ "@value": "{v}", "@type": "http://www.w3.org/2001/XMLSchema#boolean" }}"#)
}

fn connector(kind: &str, direction: &str) -> String {
    format!(r#""@type": "S231:{kind}{direction}", "S231:isOfDataType": {{ "@id": "S231:{kind}" }}"#)
}

impl Doc {
    /// The model's IRI prefix.
    pub(crate) fn id(&self) -> String {
        format!("http://example.org#{}", self.model)
    }

    /// The JSON-LD text.
    pub(crate) fn cxf(&self) -> String {
        let m = self.id();
        let mut targets: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for (from, to) in &self.wires {
            targets
                .entry(format!("{m}.{from}"))
                .or_default()
                .push(format!("{m}.{to}"));
        }
        for (name, _, from) in &self.outputs {
            targets
                .entry(format!("{m}.{from}"))
                .or_default()
                .push(format!("{m}.y_{name}"));
        }
        let link = |id: &str| -> String {
            targets.get(id).map_or_else(String::new, |ts| {
                let refs: Vec<String> = ts
                    .iter()
                    .map(|t| format!(r#"{{ "@id": "{t}" }}"#))
                    .collect();
                format!(r#", "S231:isConnectedTo": [{}]"#, refs.join(", "))
            })
        };
        let refs = |ids: Vec<String>| -> String {
            ids.iter()
                .map(|i| format!(r#"{{ "@id": "{i}" }}"#))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut graph = vec![format!(
            r#"{{ "@id": "{m}", "@type": "S231:Block", "S231:label": "{}", "S231:containsBlock": [{}], "S231:hasInput": [{}], "S231:hasOutput": [{}] }}"#,
            self.model,
            refs(
                self.blocks
                    .iter()
                    .map(|b| format!("{m}.{}", b.name))
                    .collect()
            ),
            refs(
                self.inputs
                    .iter()
                    .map(|(n, _)| format!("{m}.{n}"))
                    .collect()
            ),
            refs(
                self.outputs
                    .iter()
                    .map(|(n, _, _)| format!("{m}.y_{n}"))
                    .collect()
            ),
        )];
        for b in &self.blocks {
            let id = format!("{m}.{}", b.name);
            let mut node = format!(
                r#"{{ "@id": "{id}", "@type": "http://example.org#Buildings.Controls.OBC.{}", "S231:label": "{}""#,
                b.class, b.name
            );
            if !b.params.is_empty() {
                node += &format!(
                    r#", "S231:hasParameter": [{}]"#,
                    refs(b.params.iter().map(|(p, _)| format!("{id}.{p}")).collect())
                );
            }
            if !b.inputs.is_empty() {
                node += &format!(
                    r#", "S231:hasInput": [{}]"#,
                    refs(b.inputs.iter().map(|(p, _)| format!("{id}.{p}")).collect())
                );
            }
            node += &format!(
                r#", "S231:hasOutput": [{}] }}"#,
                refs(b.outputs.iter().map(|(p, _)| format!("{id}.{p}")).collect())
            );
            graph.push(node);
            for (p, v) in &b.params {
                graph.push(format!(r#"{{ "@id": "{id}.{p}", "S231:value": {v} }}"#));
            }
            for (p, k) in &b.inputs {
                graph.push(format!(
                    r#"{{ "@id": "{id}.{p}", {} }}"#,
                    connector(k, "Input")
                ));
            }
            for (p, k) in &b.outputs {
                let pid = format!("{id}.{p}");
                graph.push(format!(
                    r#"{{ "@id": "{pid}", {}{} }}"#,
                    connector(k, "Output"),
                    link(&pid)
                ));
            }
        }
        for (n, k) in &self.inputs {
            let pid = format!("{m}.{n}");
            graph.push(format!(
                r#"{{ "@id": "{pid}", {}, "S231:label": "{}.{n}"{} }}"#,
                connector(k, "Input"),
                self.model,
                link(&pid)
            ));
        }
        for (n, k, _) in &self.outputs {
            graph.push(format!(
                r#"{{ "@id": "{m}.y_{n}", {} }}"#,
                connector(k, "Output")
            ));
        }
        format!(
            r#"{{ "@context": {{ "S231": "http://data.ashrae.org/S231#", "base": "http://example.org#" }}, "@graph": [
{}
] }}"#,
            graph.join(",\n")
        )
    }
}

const B: &str = "Boolean";
const R: &str = "Real";
const I: &str = "Integer";

fn blk(
    name: &'static str,
    class: &'static str,
    params: Vec<(&'static str, String)>,
    inputs: Vec<(&'static str, &'static str)>,
    outputs: Vec<(&'static str, &'static str)>,
) -> Blk {
    Blk {
        name,
        class,
        params,
        inputs,
        outputs,
    }
}
fn logic1(name: &'static str, class: &'static str) -> Blk {
    blk(name, class, vec![], vec![("u", B)], vec![("y", B)])
}
fn or(name: &'static str) -> Blk {
    blk(
        name,
        "CDL.Logical.Or",
        vec![],
        vec![("u1", B), ("u2", B)],
        vec![("y", B)],
    )
}
fn and(name: &'static str) -> Blk {
    blk(
        name,
        "CDL.Logical.And",
        vec![],
        vec![("u1", B), ("u2", B)],
        vec![("y", B)],
    )
}
fn counter(name: &'static str) -> Blk {
    blk(
        name,
        "CDL.Integers.OnCounter",
        vec![("y_start", integer("3"))],
        vec![("trigger", B), ("reset", B)],
        vec![("y", I)],
    )
}
fn counter0(name: &'static str) -> Blk {
    blk(
        name,
        "CDL.Integers.OnCounter",
        vec![("y_start", integer("0"))],
        vec![("trigger", B), ("reset", B)],
        vec![("y", I)],
    )
}
fn latch(name: &'static str) -> Blk {
    blk(
        name,
        "CDL.Logical.Latch",
        vec![],
        vec![("u", B), ("clr", B)],
        vec![("y", B)],
    )
}
fn toggle(name: &'static str) -> Blk {
    blk(
        name,
        "CDL.Logical.Toggle",
        vec![],
        vec![("u", B), ("clr", B)],
        vec![("y", B)],
    )
}
fn sampler(name: &'static str) -> Blk {
    blk(
        name,
        "CDL.Discrete.TriggeredSampler",
        vec![("y_start", double("7.0"))],
        vec![("u", R), ("trigger", B)],
        vec![("y", R)],
    )
}
fn sample_trigger(name: &'static str, period: &str, shift: &str) -> Blk {
    blk(
        name,
        "CDL.Logical.Sources.SampleTrigger",
        vec![("period", double(period)), ("shift", double(shift))],
        vec![],
        vec![("y", B)],
    )
}
fn constant_bool(name: &'static str, k: &str) -> Blk {
    blk(
        name,
        "CDL.Logical.Sources.Constant",
        vec![("k", boolean(k))],
        vec![],
        vec![("y", B)],
    )
}
fn constant_real(name: &'static str, k: &str) -> Blk {
    blk(
        name,
        "CDL.Reals.Sources.Constant",
        vec![("k", double(k))],
        vec![],
        vec![("y", R)],
    )
}
fn moving_mean() -> Blk {
    blk(
        "mean",
        "CDL.Discrete.TriggeredMovingMean",
        vec![("n", integer("3"))],
        vec![("u", R), ("trigger", B)],
        vec![("y", R)],
    )
}
fn triggered_max() -> Blk {
    blk(
        "maxer",
        "CDL.Discrete.TriggeredMax",
        vec![],
        vec![("u", R), ("trigger", B)],
        vec![("y", R)],
    )
}
fn integrator() -> Blk {
    blk(
        "integ",
        "CDL.Reals.IntegratorWithReset",
        vec![("k", double("1.0")), ("y_start", double("0.5"))],
        vec![("u", R), ("y_reset_in", R), ("trigger", B)],
        vec![("y", R)],
    )
}
fn pid() -> Blk {
    blk(
        "pid",
        "CDL.Reals.PIDWithReset",
        vec![
            (
                "controllerType",
                r#""Buildings.Controls.OBC.CDL.Types.SimpleController.PI""#.into(),
            ),
            ("k", double("1.0")),
            ("Ti", double("1e20")),
            ("xi_start", double("0.125")),
            ("y_reset", double("0.5")),
            ("yMax", double("10.0")),
            ("yMin", double("-10.0")),
        ],
        vec![("u_s", R), ("u_m", R), ("trigger", B)],
        vec![("y", R)],
    )
}
fn timer() -> Blk {
    blk(
        "tim",
        "CDL.Logical.TimerAccumulating",
        vec![("t", double("100.0"))],
        vec![("u", B), ("reset", B)],
        vec![("y", R), ("passed", B)],
    )
}

/// `PulseConsumersPilot`: every consumer behind `Change(u)`, `u` changing at 2, 3, 5, 7, 8, 9 s.
pub(crate) fn consumers() -> Doc {
    Doc {
        model: "pulse_consumers",
        blocks: vec![
            constant_bool("fal", "false"),
            constant_real("zer", "0.0"),
            constant_real("one", "1.0"),
            logic1("cha", "CDL.Logical.Change"),
            sampler("sampler"),
            triggered_max(),
            moving_mean(),
            integrator(),
            pid(),
            latch("lat"),
            toggle("tog"),
            timer(),
            counter("cntNot"),
            logic1("not1", "CDL.Logical.Not"),
            counter("cntOr"),
            or("or1"),
            counter("cntPre"),
            or("or2"),
            logic1("pre1", "CDL.Logical.Pre"),
            logic1("chaX", "CDL.Logical.Change"),
            latch("latX"),
            toggle("togX"),
        ],
        wires: vec![
            ("u", "cha.u"),
            ("uR", "sampler.u"),
            ("cha.y", "sampler.trigger"),
            ("uR", "maxer.u"),
            ("cha.y", "maxer.trigger"),
            ("uR", "mean.u"),
            ("cha.y", "mean.trigger"),
            ("zer.y", "integ.u"),
            ("uR", "integ.y_reset_in"),
            ("cha.y", "integ.trigger"),
            ("one.y", "pid.u_s"),
            ("um", "pid.u_m"),
            ("cha.y", "pid.trigger"),
            ("cha.y", "lat.u"),
            ("clr", "lat.clr"),
            ("cha.y", "tog.u"),
            ("fal.y", "tog.clr"),
            ("x", "tim.u"),
            ("cha.y", "tim.reset"),
            ("cha.y", "not1.u"),
            ("not1.y", "cntNot.trigger"),
            ("fal.y", "cntNot.reset"),
            ("cha.y", "or1.u1"),
            ("lvl", "or1.u2"),
            ("or1.y", "cntOr.trigger"),
            ("fal.y", "cntOr.reset"),
            ("b", "pre1.u"),
            ("cha.y", "or2.u1"),
            ("pre1.y", "or2.u2"),
            ("or2.y", "cntPre.trigger"),
            ("fal.y", "cntPre.reset"),
            ("x", "chaX.u"),
            ("chaX.y", "latX.u"),
            ("fal.y", "latX.clr"),
            ("chaX.y", "togX.u"),
            ("fal.y", "togX.clr"),
        ],
        inputs: vec![
            ("u", B),
            ("uR", R),
            ("um", R),
            ("clr", B),
            ("lvl", B),
            ("b", B),
            ("x", B),
        ],
        outputs: vec![
            ("sampler", R, "sampler.y"),
            ("maxer", R, "maxer.y"),
            ("mean", R, "mean.y"),
            ("integ", R, "integ.y"),
            ("pid", R, "pid.y"),
            ("lat", B, "lat.y"),
            ("tog", B, "tog.y"),
            ("tim", R, "tim.y"),
            ("cntNot", I, "cntNot.y"),
            ("cntOr", I, "cntOr.y"),
            ("cntPre", I, "cntPre.y"),
            ("latX", B, "latX.y"),
            ("togX", B, "togX.y"),
        ],
    }
}

/// `SampleTriggerPilot`: `SampleTrigger` at 0.5, 1, 2, 1.4 s and 1 s shifted by 0.5 s, from t = 0.
pub(crate) fn sample_triggers() -> Doc {
    Doc {
        model: "pulse_sample_triggers",
        blocks: vec![
            constant_bool("fal", "false"),
            constant_bool("tru", "true"),
            constant_real("zer", "0.0"),
            constant_real("one", "1.0"),
            sample_trigger("s05", "0.5", "0.0"),
            sample_trigger("s1", "1.0", "0.0"),
            sample_trigger("s2", "2.0", "0.0"),
            sample_trigger("s14", "1.4", "0.0"),
            sample_trigger("s1h", "1.0", "0.5"),
            counter0("c05"),
            counter0("c1"),
            counter0("c2"),
            counter0("c14"),
            counter0("c1h"),
            sampler("t05"),
            sampler("t1"),
            sampler("t2"),
            sampler("t14"),
            sampler("tNot"),
            logic1("not1", "CDL.Logical.Not"),
            latch("lat"),
            moving_mean(),
            blk(
                "neg",
                "CDL.Reals.MultiplyByParameter",
                vec![("k", double("-1.0"))],
                vec![("u", R)],
                vec![("y", R)],
            ),
            triggered_max(),
            integrator(),
            pid(),
            timer(),
            toggle("tog"),
            logic1("notHol", "CDL.Logical.Not"),
            and("notHolAndTic"),
            latch("latHol"),
        ],
        wires: vec![
            ("s05.y", "c05.trigger"),
            ("s1.y", "c1.trigger"),
            ("s2.y", "c2.trigger"),
            ("s14.y", "c14.trigger"),
            ("s1h.y", "c1h.trigger"),
            ("fal.y", "c05.reset"),
            ("fal.y", "c1.reset"),
            ("fal.y", "c2.reset"),
            ("fal.y", "c14.reset"),
            ("fal.y", "c1h.reset"),
            ("uR", "t05.u"),
            ("uR", "t1.u"),
            ("uR", "t2.u"),
            ("uR", "t14.u"),
            ("uR", "tNot.u"),
            ("s05.y", "t05.trigger"),
            ("s1.y", "t1.trigger"),
            ("s2.y", "t2.trigger"),
            ("s14.y", "t14.trigger"),
            ("s1.y", "not1.u"),
            ("not1.y", "tNot.trigger"),
            ("s1.y", "lat.u"),
            ("clr", "lat.clr"),
            ("uR", "mean.u"),
            ("s1.y", "mean.trigger"),
            ("uR", "neg.u"),
            ("neg.y", "maxer.u"),
            ("s1.y", "maxer.trigger"),
            ("zer.y", "integ.u"),
            ("uR", "integ.y_reset_in"),
            ("s1.y", "integ.trigger"),
            ("one.y", "pid.u_s"),
            ("uR", "pid.u_m"),
            ("s1.y", "pid.trigger"),
            ("tru.y", "tim.u"),
            ("s1.y", "tim.reset"),
            ("s1.y", "tog.u"),
            ("fal.y", "tog.clr"),
            ("clr", "notHol.u"),
            ("notHol.y", "notHolAndTic.u1"),
            ("s1.y", "notHolAndTic.u2"),
            ("s1.y", "latHol.u"),
            ("notHolAndTic.y", "latHol.clr"),
        ],
        inputs: vec![("uR", R), ("clr", B)],
        outputs: vec![
            ("c05", I, "c05.y"),
            ("c1", I, "c1.y"),
            ("c2", I, "c2.y"),
            ("c14", I, "c14.y"),
            ("c1h", I, "c1h.y"),
            ("t05", R, "t05.y"),
            ("t1", R, "t1.y"),
            ("t2", R, "t2.y"),
            ("t14", R, "t14.y"),
            ("tNot", R, "tNot.y"),
            ("lat", B, "lat.y"),
            ("mean", R, "mean.y"),
            ("maxer", R, "maxer.y"),
            ("integ", R, "integ.y"),
            ("pid", R, "pid.y"),
            ("tim", R, "tim.y"),
            ("tog", B, "tog.y"),
            ("latHol", B, "latHol.y"),
        ],
    }
}

/// `SampleTriggerOffGridPilot`: `SampleTrigger(1 s)` in a run whose frames are at 0.25 + k s.
pub(crate) fn sample_trigger_off_grid() -> Doc {
    Doc {
        model: "pulse_sample_trigger_off_grid",
        blocks: vec![
            constant_bool("fal", "false"),
            sample_trigger("s1", "1.0", "0.0"),
            counter0("c1"),
            sampler("t1"),
            latch("lat"),
            logic1("pre1", "CDL.Logical.Pre"),
            counter0("cPre"),
        ],
        wires: vec![
            ("s1.y", "c1.trigger"),
            ("fal.y", "c1.reset"),
            ("uR", "t1.u"),
            ("s1.y", "t1.trigger"),
            ("s1.y", "lat.u"),
            ("fal.y", "lat.clr"),
            ("s1.y", "pre1.u"),
            ("pre1.y", "cPre.trigger"),
            ("fal.y", "cPre.reset"),
        ],
        inputs: vec![("uR", R)],
        outputs: vec![
            ("c1", I, "c1.y"),
            ("t1", R, "t1.y"),
            ("lat", B, "lat.y"),
            ("cPre", I, "cPre.y"),
        ],
    }
}
