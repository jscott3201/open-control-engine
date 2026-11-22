//! The engine and `oce import` / `oce emit` agree on every enumeration parameter spelling
//! (issue #734, the owner's ruling R2 of 2026-11-21): an enumeration parameter takes a qualified
//! member of its own enumeration and nothing else, so an ordinal, a G36 integer constant, an
//! Integer expression, an Integer root parameter, a string and another enumeration's member are
//! refused by both, and the qualified member loads in both.
//!
//! Before the ruling the engine loaded these and ran a value the document did not name, or its
//! default: `PID.controllerType = OperationModes.occupied` grounds to the Integer 1 and ran the
//! default PI, `controllerType = "Foo.Bar.PD"` ran PD, `zerTim = 11` ran as `NY2017`, and
//! `TimeTable.smoothness = 2` as `ConstantSegments`.

mod support;

use oce_api::Engine;
use serde_json::Value;
use support::oce;

const NS: &str = "http://example.org#";
const CDL: &str = "Buildings.Controls.OBC.CDL.Types";

/// One block of `class` with `inputs` and `outputs` (name, type), each wired to a root port of
/// the same name, its parameter `name` written `literal`, beside `extra` block parameters
/// (name, JSON members) and root parameters `root` (name, JSON members).
struct Doc<'a> {
    class: &'a str,
    inputs: &'a [(&'a str, &'a str)],
    outputs: &'a [(&'a str, &'a str)],
    extra: &'a [(&'a str, &'a str)],
}

impl Doc<'_> {
    fn with(&self, name: &str, literal: &str, root: &[(&str, String)]) -> Vec<u8> {
        let refs = |owner: &str, names: &mut dyn Iterator<Item = &str>| {
            names
                .map(|n| format!(r#"{{ "@id": "{NS}{owner}.{n}" }}"#))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut nodes = Vec::new();
        let root_params = refs("Seq", &mut root.iter().map(|(n, _)| *n));
        let root_inputs = refs("Seq", &mut self.inputs.iter().map(|(n, _)| *n));
        let root_outputs = refs("Seq", &mut self.outputs.iter().map(|(n, _)| *n));
        let block_params = refs(
            "Seq.blk",
            &mut std::iter::once(name).chain(self.extra.iter().map(|(n, _)| *n)),
        );
        let block_inputs = refs("Seq.blk", &mut self.inputs.iter().map(|(n, _)| *n));
        let block_outputs = refs("Seq.blk", &mut self.outputs.iter().map(|(n, _)| *n));
        nodes.push(format!(
            r#"{{ "@id": "{NS}Seq", "@type": "S231:Block",
  "S231:containsBlock": {{ "@id": "{NS}Seq.blk" }},
  "S231:hasParameter": [ {root_params} ], "S231:hasInput": [ {root_inputs} ],
  "S231:hasOutput": [ {root_outputs} ] }}"#
        ));
        for (n, members) in root {
            nodes.push(format!(r#"{{ "@id": "{NS}Seq.{n}", {members} }}"#));
        }
        nodes.push(format!(
            r#"{{ "@id": "{NS}Seq.blk", "@type": "{NS}Buildings.Controls.OBC.{}",
  "S231:hasParameter": [ {block_params} ], "S231:hasInput": [ {block_inputs} ],
  "S231:hasOutput": [ {block_outputs} ] }}"#,
            self.class
        ));
        nodes.push(format!(
            r#"{{ "@id": "{NS}Seq.blk.{name}", "S231:value": {literal} }}"#
        ));
        for (n, members) in self.extra {
            nodes.push(format!(r#"{{ "@id": "{NS}Seq.blk.{n}", {members} }}"#));
        }
        for (n, t) in self.inputs {
            nodes.push(format!(
                r#"{{ "@id": "{NS}Seq.{n}", "@type": "S231:{t}Input",
  "S231:isOfDataType": {{ "@id": "S231:{t}" }}, "S231:isConnectedTo": {{ "@id": "{NS}Seq.blk.{n}" }} }}"#
            ));
            nodes.push(format!(
                r#"{{ "@id": "{NS}Seq.blk.{n}", "@type": "S231:{t}Input",
  "S231:isOfDataType": {{ "@id": "S231:{t}" }} }}"#
            ));
        }
        for (n, t) in self.outputs {
            nodes.push(format!(
                r#"{{ "@id": "{NS}Seq.blk.{n}", "@type": "S231:{t}Output",
  "S231:isOfDataType": {{ "@id": "S231:{t}" }}, "S231:isConnectedTo": {{ "@id": "{NS}Seq.{n}" }} }}"#
            ));
            nodes.push(format!(
                r#"{{ "@id": "{NS}Seq.{n}", "@type": "S231:{t}Output",
  "S231:isOfDataType": {{ "@id": "S231:{t}" }} }}"#
            ));
        }
        format!(
            r#"{{ "@context": {{ "S231": "http://data.ashrae.org/S231#" }},
  "@graph": [
{}
  ] }}"#,
            nodes.join(",\n")
        )
        .into_bytes()
    }
}

const PID: Doc<'static> = Doc {
    class: "CDL.Reals.PID",
    inputs: &[("u_s", "Real"), ("u_m", "Real")],
    outputs: &[("y", "Real")],
    extra: &[],
};

const CALENDAR: Doc<'static> = Doc {
    class: "CDL.Reals.Sources.CalendarTime",
    inputs: &[],
    outputs: &[
        ("year", "Integer"),
        ("month", "Integer"),
        ("day", "Integer"),
        ("hour", "Integer"),
        ("minute", "Real"),
        ("weekDay", "Integer"),
    ],
    extra: &[],
};

const TIME_TABLE: Doc<'static> = Doc {
    class: "CDL.Reals.Sources.TimeTable",
    inputs: &[],
    outputs: &[("y1", "Real")],
    extra: &[(
        "table",
        r#""S231:isArray": true, "S231:numberDimensions": 2, "S231:sizeOfDimensions": "(2, 2)", "S231:value": [0.0, 0.0, 2.0, 2.0]"#,
    )],
};

/// Every code the report of a run carries: its errors' diagnostic codes and its findings' codes.
fn codes(report: &Value) -> Vec<String> {
    let mut codes = Vec::new();
    for error in report["errors"].as_array().into_iter().flatten() {
        for code in error["diagnostic_codes"].as_array().into_iter().flatten() {
            codes.extend(code.as_str().map(str::to_owned));
        }
        codes.extend(error["code"].as_str().map(str::to_owned));
    }
    for finding in report["findings"].as_array().into_iter().flatten() {
        codes.extend(finding["code"].as_str().map(str::to_owned));
    }
    codes
}

/// The engine's verdict: loads, or the codes of its refusal.
fn engine(cxf: &[u8]) -> Result<(), Vec<String>> {
    Engine::new().load_cxf(cxf).map(|_| ()).map_err(|error| {
        error
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code.as_str().to_owned())
            .collect()
    })
}

/// The authoring layer's verdict: `oce import` reads the document and `oce emit` emits what it
/// read, or the codes of whichever refuses it.
fn authoring(cxf: &[u8]) -> Result<(), Vec<String>> {
    let imported = oce(&["import", "-"], cxf);
    if imported.code != 0 {
        return Err(codes(&imported.report()));
    }
    let emitted = oce(&["emit", "-", "--from", "text"], &imported.stdout);
    if emitted.code == 0 {
        Ok(())
    } else {
        Err(codes(&emitted.report()))
    }
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "\\\""))
}

/// One refused spelling: the document, the parameter, its literal and the root parameters.
type Case<'a> = (&'a Doc<'a>, &'a str, String, &'a [(&'a str, String)]);

#[test]
fn every_spelling_other_than_a_qualified_member_is_refused_by_both() {
    let integer_root = [(
        "n",
        r#""@type": "S231:Parameter", "S231:isOfDataType": { "@id": "S231:Integer" }, "S231:value": 3"#
            .to_owned(),
    )];
    let cases: Vec<Case<'_>> = vec![
        (
            &PID,
            "controllerType",
            quoted("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied"),
            &[],
        ),
        (&PID, "controllerType", quoted("1+2"), &[]),
        (&PID, "controllerType", "3".to_owned(), &[]),
        (&PID, "controllerType", quoted("n"), &integer_root),
        (
            &PID,
            "controllerType",
            quoted(&format!("{CDL}.Smoothness.ConstantSegments")),
            &[],
        ),
        (&PID, "controllerType", quoted(r#""Foo.Bar.PD""#), &[]),
        (&PID, "controllerType", quoted(r#"".PD""#), &[]),
        (
            &PID,
            "controllerType",
            quoted(&format!(r#""{CDL}.Smoothness.PD""#)),
            &[],
        ),
        (
            &PID,
            "controllerType",
            quoted(&format!(r#""{CDL}.SimpleController.PD""#)),
            &[],
        ),
        (&CALENDAR, "zerTim", "11".to_owned(), &[]),
        (&CALENDAR, "zerTim", quoted(r#""NY2017""#), &[]),
        (&TIME_TABLE, "smoothness", "2".to_owned(), &[]),
        (
            &TIME_TABLE,
            "smoothness",
            quoted("Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes.occupied"),
            &[],
        ),
        (
            &TIME_TABLE,
            "extrapolation",
            quoted(&format!("{CDL}.SimpleController.PD")),
            &[],
        ),
    ];
    for (doc, name, literal, root) in cases {
        let cxf = doc.with(name, &literal, root);
        let engine = engine(&cxf);
        assert!(
            engine.as_ref().is_err_and(|codes| !codes.is_empty()),
            "{} {name} = {literal}: the engine loaded it",
            doc.class
        );
        let authoring = authoring(&cxf);
        assert!(
            authoring.is_err(),
            "{} {name} = {literal}: the engine refused it ({engine:?}) and the authoring layer \
             emitted it",
            doc.class
        );
    }
}

#[test]
fn a_qualified_member_loads_in_both() {
    for (doc, name, member) in [
        (&PID, "controllerType", format!("{CDL}.SimpleController.PD")),
        (&CALENDAR, "zerTim", format!("{CDL}.ZeroTime.NY2017")),
        (
            &TIME_TABLE,
            "smoothness",
            format!("{CDL}.Smoothness.ConstantSegments"),
        ),
        (
            &TIME_TABLE,
            "extrapolation",
            format!("{CDL}.Extrapolation.HoldLastPoint"),
        ),
    ] {
        let cxf = doc.with(name, &quoted(&member), &[]);
        assert_eq!(engine(&cxf), Ok(()), "{} {name} = {member}", doc.class);
        assert_eq!(authoring(&cxf), Ok(()), "{} {name} = {member}", doc.class);
    }
}
