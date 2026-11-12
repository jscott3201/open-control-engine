//! Stage-1 / engine parity for an array element past its block's resolved length (GitHub #626),
//! beside the parity table in `engine_tests.rs`, whose `parameter-out-of-range` row it extends to
//! the eight vector templates: stage 1 refuses the element through the load gate's own rule, and
//! the engine, given the emission with the block's in-length ports, reports the same code at the
//! same subject.

use crate::emit::{EmitOptions, write};
use crate::validate::{Finding, validate_author_side};

use super::judge;

fn pairs(findings: &[Finding]) -> Vec<(&'static str, String)> {
    findings
        .iter()
        .map(|f| (f.code, f.subject.clone()))
        .collect()
}

#[test]
fn an_array_element_past_its_length_meets_the_engines_parameter_out_of_range() {
    // GitHub #626: stage 1 refuses the element through the load gate's own rule, and the engine,
    // given the emission with the block's in-length ports (so it judges the element, not a
    // fallback interface), reports the same code at the same subject.
    for (class, fixed, base, n, signal, outputs, value, kind) in [
        ("CDL.Reals.MultiSum", "", "k", 2, "Real", "y", "1.5", "Real"),
        (
            "CDL.Integers.MultiSum",
            "",
            "k",
            2,
            "Integer",
            "y",
            "7",
            "Integer",
        ),
        (
            "CDL.Routing.RealExtractSignal",
            "nin = 2, nout = 1",
            "extract",
            1,
            "Real",
            "y1",
            "2",
            "Integer",
        ),
        (
            "CDL.Routing.IntegerExtractSignal",
            "nin = 2, nout = 1",
            "extract",
            1,
            "Integer",
            "y1",
            "2",
            "Integer",
        ),
        (
            "CDL.Routing.BooleanExtractSignal",
            "nin = 2, nout = 1",
            "extract",
            1,
            "Boolean",
            "y1",
            "2",
            "Integer",
        ),
        (
            "CDL.Routing.RealVectorFilter",
            "nin = 2, nout = 2",
            "msk",
            2,
            "Real",
            "y1,y2",
            "true",
            "Boolean",
        ),
        (
            "CDL.Routing.IntegerVectorFilter",
            "nin = 2, nout = 2",
            "msk",
            2,
            "Integer",
            "y1,y2",
            "true",
            "Boolean",
        ),
        (
            "CDL.Routing.BooleanVectorFilter",
            "nin = 2, nout = 2",
            "msk",
            2,
            "Boolean",
            "y1,y2",
            "true",
            "Boolean",
        ),
    ] {
        let length = if fixed.is_empty() {
            "nin = 2, ".to_owned()
        } else {
            format!("{fixed}, ")
        };
        let element = format!("{base}_{}", n + 1);
        let mut text = format!(
            "sequence s\nderived_from none\nin u1 : {signal}\nin u2 : {signal}\n\
             block b = {class}({length}{element} = {value})\nu1 -> b.u1\nu2 -> b.u2\n"
        );
        for output in outputs.split(',') {
            text.push_str(&format!(
                "out {output} : {signal}\nb.{output} -> {output}\n"
            ));
        }
        let s = crate::text::parse_text(&text).expect("parses");
        let subject = format!("b.{element}");
        let author = pairs(&validate_author_side(&s));
        assert_eq!(
            author,
            vec![("parameter-out-of-range", subject.clone())],
            "{class}"
        );
        let in_length: Vec<(&str, oce_api::Value)> = length
            .trim_end_matches(", ")
            .split(", ")
            .map(|a| {
                let (name, v) = a.split_once(" = ").expect("an assignment");
                (
                    name,
                    oce_api::Value::Integer(v.parse().expect("an Integer")),
                )
            })
            .collect();
        let ports = oce_api::class_ports(class, &in_length).expect("the in-length ports");
        let options = EmitOptions::default();
        let bytes = write(&s, &options, &[Some(ports)]).expect("emits");
        let engine = pairs(&judge(&s, &bytes, &options.namespace));
        assert_eq!(
            engine,
            vec![("parameter-out-of-range", subject)],
            "{class} ({kind})"
        );
    }
}
