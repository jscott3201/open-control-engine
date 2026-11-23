//! The engine's CXF documents for the pilots of the review round of GitHub #805 (issue #800):
//! `InitNegatedPilot` (and its off-grid copy), `InitMixturesPilot` (and its off-grid copy) and
//! `LatchClearPilot` in `tools/openmodelica-event-reference/events/`, the same CDL blocks wired
//! the same way, built from tables rather than by hand (their names are generated, so they are
//! leaked into `'static` strings; a test process builds each document a handful of times).

use crate::pulse_event_documents::{Blk, Doc, boolean, double, integer};

const B: &str = "Boolean";
const R: &str = "Real";
const I: &str = "Integer";

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// A document under construction: blocks, wires and outputs with generated names.
#[derive(Default)]
struct Build {
    blocks: Vec<Blk>,
    wires: Vec<(&'static str, &'static str)>,
    outputs: Vec<(&'static str, &'static str, &'static str)>,
}

impl Build {
    fn block(
        &mut self,
        name: &str,
        class: &'static str,
        params: Vec<(&'static str, String)>,
        inputs: &[(&'static str, &'static str)],
        outputs: &[(&'static str, &'static str)],
    ) {
        self.blocks.push(Blk {
            name: leak(name.to_owned()),
            class,
            params,
            inputs: inputs.to_vec(),
            outputs: outputs.to_vec(),
        });
    }
    fn wire(&mut self, from: &str, to: &str) {
        self.wires
            .push((leak(from.to_owned()), leak(to.to_owned())));
    }
    fn output(&mut self, name: &str, kind: &'static str) {
        self.outputs
            .push((leak(name.to_owned()), kind, leak(format!("{name}.y"))));
    }
    fn logic1(&mut self, name: &str, class: &'static str, from: &str) {
        self.block(name, class, vec![], &[("u", B)], &[("y", B)]);
        self.wire(from, &format!("{name}.u"));
    }
    fn logic2(&mut self, name: &str, class: &'static str, u1: &str, u2: &str) {
        self.block(name, class, vec![], &[("u1", B), ("u2", B)], &[("y", B)]);
        self.wire(u1, &format!("{name}.u1"));
        self.wire(u2, &format!("{name}.u2"));
    }
    fn constant_bool(&mut self, name: &str, k: &str) {
        let k = boolean(k);
        self.block(
            name,
            "CDL.Logical.Sources.Constant",
            vec![("k", k)],
            &[],
            &[("y", B)],
        );
    }
    fn constant_real(&mut self, name: &str, k: &str) {
        let k = double(k);
        self.block(
            name,
            "CDL.Reals.Sources.Constant",
            vec![("k", k)],
            &[],
            &[("y", R)],
        );
    }
    fn sample_trigger(&mut self, name: &str, period: &str) {
        self.sample_trigger_shifted(name, period, "0.0");
    }
    fn sample_trigger_shifted(&mut self, name: &str, period: &str, shift: &str) {
        self.block(
            name,
            "CDL.Logical.Sources.SampleTrigger",
            vec![("period", double(period)), ("shift", double(shift))],
            &[],
            &[("y", B)],
        );
    }
    /// `OnCounter`, `Latch` and `Toggle` on `src` (`clr` and `reset` false).
    fn three(&mut self, tag: &str, src: &str) {
        let (cnt, lat, tog) = (
            format!("cnt{tag}"),
            format!("lat{tag}"),
            format!("tog{tag}"),
        );
        self.block(
            &cnt,
            "CDL.Integers.OnCounter",
            vec![("y_start", integer("0"))],
            &[("trigger", B), ("reset", B)],
            &[("y", I)],
        );
        self.wire(src, &format!("{cnt}.trigger"));
        self.wire("fal.y", &format!("{cnt}.reset"));
        self.output(&cnt, I);
        for (name, class) in [(&lat, "CDL.Logical.Latch"), (&tog, "CDL.Logical.Toggle")] {
            self.block(name, class, vec![], &[("u", B), ("clr", B)], &[("y", B)]);
            self.wire(src, &format!("{name}.u"));
            self.wire("fal.y", &format!("{name}.clr"));
            self.output(name, B);
        }
    }
    fn sampler(&mut self, name: &str, u: &str, trigger: &str) {
        self.block(
            name,
            "CDL.Discrete.TriggeredSampler",
            vec![("y_start", double("7.0"))],
            &[("u", R), ("trigger", B)],
            &[("y", R)],
        );
        self.wire(u, &format!("{name}.u"));
        self.wire(trigger, &format!("{name}.trigger"));
        self.output(name, R);
    }
    /// `TriggeredMax`, `TriggeredMovingMean(n = 3)`, `IntegratorWithReset` (`u = 0`, reset to
    /// `u`) and `PIDWithReset` (PI, integral held, `u_m = u`) on `u` and `trigger`.
    fn data_consumers(&mut self, tag: &str, u: &str, trigger: &str) {
        let max = format!("max{tag}");
        self.block(
            &max,
            "CDL.Discrete.TriggeredMax",
            vec![],
            &[("u", R), ("trigger", B)],
            &[("y", R)],
        );
        self.wire(u, &format!("{max}.u"));
        self.wire(trigger, &format!("{max}.trigger"));
        self.output(&max, R);
        let mean = format!("mean{tag}");
        self.block(
            &mean,
            "CDL.Discrete.TriggeredMovingMean",
            vec![("n", integer("3"))],
            &[("u", R), ("trigger", B)],
            &[("y", R)],
        );
        self.wire(u, &format!("{mean}.u"));
        self.wire(trigger, &format!("{mean}.trigger"));
        self.output(&mean, R);
        let integ = format!("integ{tag}");
        self.block(
            &integ,
            "CDL.Reals.IntegratorWithReset",
            vec![("k", double("1.0")), ("y_start", double("0.5"))],
            &[("u", R), ("y_reset_in", R), ("trigger", B)],
            &[("y", R)],
        );
        self.wire("zer.y", &format!("{integ}.u"));
        self.wire(u, &format!("{integ}.y_reset_in"));
        self.wire(trigger, &format!("{integ}.trigger"));
        self.output(&integ, R);
        let pid = format!("pid{tag}");
        self.block(
            &pid,
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
            &[("u_s", R), ("u_m", R), ("trigger", B)],
            &[("y", R)],
        );
        self.wire("one.y", &format!("{pid}.u_s"));
        self.wire(u, &format!("{pid}.u_m"));
        self.wire(trigger, &format!("{pid}.trigger"));
        self.output(&pid, R);
    }
    fn timer(&mut self, name: &str, reset: &str) {
        self.block(
            name,
            "CDL.Logical.TimerAccumulating",
            vec![("t", double("100.0"))],
            &[("u", B), ("reset", B)],
            &[("y", R), ("passed", B)],
        );
        self.wire("tru.y", &format!("{name}.u"));
        self.wire(reset, &format!("{name}.reset"));
        self.output(name, R);
    }
    fn doc(self, model: &'static str, inputs: Vec<(&'static str, &'static str)>) -> Doc {
        Doc {
            model,
            blocks: self.blocks,
            wires: self.wires,
            inputs,
            outputs: self.outputs,
        }
    }
}

/// The constants every pilot here shares.
fn constants(b: &mut Build) {
    b.constant_bool("fal", "false");
    b.constant_bool("tru", "true");
    b.constant_real("zer", "0.0");
    b.constant_real("one", "1.0");
}

/// `InitNegatedPilot` (and `InitNegatedOffGridPilot`): `Not(Change(x))` and `Not(Edge(x))` with
/// `x1` true and `x0` false from the start, and `Not(SampleTrigger(1 s))`, into the consumers.
pub(crate) fn init_negated() -> Doc {
    let mut b = Build::default();
    constants(&mut b);
    for x in ["1", "0"] {
        b.logic1(&format!("cha{x}"), "CDL.Logical.Change", &format!("x{x}"));
        b.logic1(
            &format!("notCha{x}"),
            "CDL.Logical.Not",
            &format!("cha{x}.y"),
        );
        full(&mut b, &format!("C{x}"), &format!("notCha{x}.y"));
        b.logic1(&format!("edg{x}"), "CDL.Logical.Edge", &format!("x{x}"));
        b.logic1(
            &format!("notEdg{x}"),
            "CDL.Logical.Not",
            &format!("edg{x}.y"),
        );
        partial(&mut b, &format!("E{x}"), &format!("notEdg{x}.y"));
    }
    b.sample_trigger("s1", "1.0");
    b.logic1("notS1", "CDL.Logical.Not", "s1.y");
    full(&mut b, "S", "notS1.y");
    b.doc("pulse_init_negated", vec![("x1", B), ("x0", B), ("uR", R)])
}

/// `OnCounter`, `TriggeredSampler`, `Latch` and `Toggle` on `src` (the `.mo`'s partial set).
fn partial(b: &mut Build, tag: &str, src: &str) {
    b.three(tag, src);
    b.sampler(&format!("sam{tag}"), "uR", src);
}

/// Every consumer class on `src`.
fn full(b: &mut Build, tag: &str, src: &str) {
    partial(b, tag, src);
    b.data_consumers(tag, "uR", src);
    b.timer(&format!("tim{tag}"), src);
}

/// `InitMixturesPilot` (and `InitMixturesOffGridPilot`): mixed cones at t = 0 into `OnCounter`,
/// `Latch` and `Toggle`.
pub(crate) fn init_mixtures() -> Doc {
    let mut b = Build::default();
    constants(&mut b);
    b.logic1("cha", "CDL.Logical.Change", "x1");
    b.logic1("notCha", "CDL.Logical.Not", "cha.y");
    b.sample_trigger("s1", "1.0");
    b.sample_trigger("s2", "2.0");
    b.logic1("notS1", "CDL.Logical.Not", "s1.y");
    b.logic1("notS2", "CDL.Logical.Not", "s2.y");
    b.logic2("orChaSam", "CDL.Logical.Or", "cha.y", "s1.y");
    b.logic2("andNotSam", "CDL.Logical.And", "notCha.y", "s1.y");
    b.logic2("xorChaSam", "CDL.Logical.Xor", "cha.y", "s1.y");
    b.logic2("orNotTru", "CDL.Logical.Or", "notCha.y", "tru.y");
    b.logic2("orNotFal", "CDL.Logical.Or", "notCha.y", "fal.y");
    b.logic1("pre1", "CDL.Logical.Pre", "fal.y");
    b.logic2("orNotPre", "CDL.Logical.Or", "notCha.y", "pre1.y");
    b.logic2("andNotNot", "CDL.Logical.And", "notCha.y", "notS1.y");
    b.logic2("andChaNotS2", "CDL.Logical.And", "cha.y", "notS2.y");
    b.logic2("orAndS1", "CDL.Logical.Or", "andChaNotS2.y", "s1.y");
    b.logic2("xorNotS1", "CDL.Logical.Xor", "notCha.y", "s1.y");
    for (tag, src) in [
        ("ChaSam", "orChaSam.y"),
        ("NotSam", "andNotSam.y"),
        ("XorSam", "xorChaSam.y"),
        ("NotTru", "orNotTru.y"),
        ("NotFal", "orNotFal.y"),
        ("NotPre", "orNotPre.y"),
        ("NotNot", "andNotNot.y"),
        ("Three", "orAndS1.y"),
        ("XorNot", "xorNotS1.y"),
    ] {
        b.three(tag, src);
    }
    b.doc("pulse_init_mixtures", vec![("x1", B)])
}

/// `LatchClearPilot`: `Latch` and `Toggle` whose `clr` carries a pulse or a sample, and data inputs
/// that carry a converted pulse.
pub(crate) fn latch_clear() -> Doc {
    let mut b = Build::default();
    constants(&mut b);
    b.logic1("chaA", "CDL.Logical.Change", "a");
    b.logic1("chaB", "CDL.Logical.Change", "b");
    b.logic1("notChaA", "CDL.Logical.Not", "chaA.y");
    b.logic1("edgA", "CDL.Logical.Edge", "a");
    b.logic1("notEdgA", "CDL.Logical.Not", "edgA.y");
    b.sample_trigger("s2", "2.0");
    b.logic1("notS2", "CDL.Logical.Not", "s2.y");
    for (tag, u, clr) in [
        ("A", "notChaA.y", "chaA.y"),
        ("B", "notChaA.y", "chaB.y"),
        ("C", "chaA.y", "notEdgA.y"),
        ("D", "s2.y", "notS2.y"),
    ] {
        for (name, class) in [
            (format!("lat{tag}"), "CDL.Logical.Latch"),
            (format!("tog{tag}"), "CDL.Logical.Toggle"),
        ] {
            b.block(&name, class, vec![], &[("u", B), ("clr", B)], &[("y", B)]);
            b.wire(u, &format!("{name}.u"));
            b.wire(clr, &format!("{name}.clr"));
            b.output(&name, B);
        }
    }
    b.block(
        "pulR",
        "CDL.Conversions.BooleanToReal",
        vec![],
        &[("u", B)],
        &[("y", R)],
    );
    b.wire("chaA.y", "pulR.u");
    b.sampler("samD", "pulR.y", "notChaA.y");
    b.data_consumers("D", "pulR.y", "notChaA.y");
    b.doc("pulse_latch_clear", vec![("a", B), ("b", B)])
}

/// `MixedFallbackPilot`: two-input consumers with one input on a fallback cone and the other on
/// the rule (the second review of GitHub #805), and a `Toggle` whose `clr` holds negated samples.
pub(crate) fn mixed_fallback() -> Doc {
    let mut b = Build::default();
    b.logic1("chaA", "CDL.Logical.Change", "a");
    b.logic1("notChaA", "CDL.Logical.Not", "chaA.y");
    b.logic1("preNotChaA", "CDL.Logical.Pre", "notChaA.y");
    b.logic1("chaB", "CDL.Logical.Change", "b");
    b.logic1("edgChaB", "CDL.Logical.Edge", "chaB.y");
    b.sample_trigger_shifted("s3", "3.0", "0.5");
    b.logic1("notS3", "CDL.Logical.Not", "s3.y");
    b.logic1("preB", "CDL.Logical.Pre", "b");
    b.sample_trigger("s15", "1.5");
    b.sample_trigger("s2", "2.0");
    b.sample_trigger("s3b", "3.0");
    b.logic2("norS", "CDL.Logical.Nor", "s2.y", "s3b.y");
    for (name, class, u, clr) in [
        ("latA", "CDL.Logical.Latch", "chaA.y", "preNotChaA.y"),
        ("togC", "CDL.Logical.Toggle", "notS3.y", "preB.y"),
        ("togD", "CDL.Logical.Toggle", "s15.y", "norS.y"),
    ] {
        b.block(name, class, vec![], &[("u", B), ("clr", B)], &[("y", B)]);
        b.wire(u, &format!("{name}.u"));
        b.wire(clr, &format!("{name}.clr"));
        b.output(name, B);
    }
    b.block(
        "cntB",
        "CDL.Integers.OnCounter",
        vec![("y_start", integer("0"))],
        &[("trigger", B), ("reset", B)],
        &[("y", I)],
    );
    b.wire("chaB.y", "cntB.trigger");
    b.wire("edgChaB.y", "cntB.reset");
    b.output("cntB", I);
    b.doc("pulse_mixed_fallback", vec![("a", B), ("b", B)])
}
