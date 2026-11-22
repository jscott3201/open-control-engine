//! The engine and `oce import` / `oce emit` agree on every spelling of an enumeration's class path
//! (issue #762, the owner's ruling of 2026-11-22; typing IRIs, issue #637), and on `MatrixGain`'s
//! counts (issue #773), over a generated set: each context an enumeration is written in (a block
//! parameter's value, a typed root parameter's value, a root parameter's declared type) crossed
//! with each spelling of the path. Both load the canonical path and both refuse every other.
//!
//! Two differences are outside this set and recorded in the parity table
//! (`docs/design/authoring-layer.md`): the authoring layer requires a declared type on every
//! sequence parameter (an untyped root parameter is `missing-key` there, whatever it holds), and
//! it refuses every expression on a block parameter, so whitespace around a canonical path, which
//! the engine reads as the expression's own whitespace, is refused there.

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
        self.with_members(name, &format!(r#""S231:value": {literal}"#), root)
    }

    /// The document with the block parameter `name` written with the JSON members `members`.
    fn with_members(&self, name: &str, members: &str, root: &[(&str, String)]) -> Vec<u8> {
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
        nodes.push(format!(r#"{{ "@id": "{NS}Seq.blk.{name}", {members} }}"#));
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

const SPELLINGS: [(&str, bool); 10] = [
    ("Buildings.Controls.OBC.CDL.Types.SimpleController", true),
    ("SimpleController", false),
    ("Foo.SimpleController", false),
    (
        "Buildings.Controls.OBC.ASHRAE.G36.Types.SimpleController",
        false,
    ),
    ("Modelica.Blocks.Types.SimpleController", false),
    ("CDL.Types.SimpleController", false),
    ("Controls.OBC.CDL.Types.SimpleController", false),
    ("buildings.controls.obc.cdl.types.SimpleController", false),
    ("Buildings.Controls.OBC.CDL.Types.simpleController", false),
    (
        "Foo.Buildings.Controls.OBC.CDL.Types.SimpleController",
        false,
    ),
];

/// A root parameter `p` declared `datatype` holding `literal`.
fn typed_root(datatype: &str, literal: &str) -> Vec<(&'static str, String)> {
    vec![(
        "p",
        format!(
            r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "{datatype}" }}, "S231:value": {literal}"#
        ),
    )]
}

fn verdicts(cxf: &[u8]) -> (bool, bool) {
    (engine(cxf).is_ok(), authoring(cxf).is_ok())
}

#[test]
fn both_load_the_canonical_path_and_both_refuse_every_other_in_every_context() {
    let canonical_type = format!("{NS}{CDL}.SimpleController");
    let mut compared = 0;
    for (path, loads) in SPELLINGS {
        let value = quoted(&format!("{path}.PD"));
        // A block parameter's value.
        let block = PID.with("controllerType", &value, &[]);
        // A typed root parameter's value, read by the block.
        let root = PID.with(
            "controllerType",
            &quoted("p"),
            &typed_root(&canonical_type, &value),
        );
        for (context, cxf) in [("block value", block), ("root value", root)] {
            assert_eq!(verdicts(&cxf), (loads, loads), "{context} {path}");
            compared += 1;
        }
        // A root parameter's declared type, holding the canonical member.
        let canonical_value = quoted(&format!("{CDL}.SimpleController.PD"));
        let declared = PID.with(
            "controllerType",
            &quoted("p"),
            &typed_root(&format!("{NS}{path}"), &canonical_value),
        );
        assert_eq!(verdicts(&declared), (loads, loads), "declared type {path}");
        compared += 1;
    }
    // The S231 namespace's own enumeration type (the CXF-Core `EnumerationType`) types a root
    // parameter in both.
    let s231 = PID.with(
        "controllerType",
        &quoted("p"),
        &typed_root(
            "S231:Buildings.Controls.OBC.CDL.Types.SimpleController",
            &quoted(&format!("{CDL}.SimpleController.PD")),
        ),
    );
    assert_eq!(verdicts(&s231), (true, true), "S231 typing");
    // Issue #637's reproduction: a typo in the package of a declared type.
    let typo = PID.with(
        "controllerType",
        &quoted("p"),
        &typed_root(
            &format!("{NS}Buildings.Controls.OBC.CDL.Typos.SimpleController"),
            &quoted(&format!("{CDL}.SimpleController.PD")),
        ),
    );
    assert_eq!(verdicts(&typo), (false, false), "Typos");
    // The other enumerations, through `TimeTable`'s two.
    for (name, member) in [
        ("smoothness", "Smoothness.ConstantSegments"),
        ("extrapolation", "Extrapolation.HoldLastPoint"),
    ] {
        let canonical = TIME_TABLE.with(name, &quoted(&format!("{CDL}.{member}")), &[]);
        assert_eq!(verdicts(&canonical), (true, true), "{name}");
        for prefix in ["", "Foo.", "Modelica.Blocks.Types."] {
            let other = TIME_TABLE.with(name, &quoted(&format!("{prefix}{member}")), &[]);
            assert_eq!(verdicts(&other), (false, false), "{name} {prefix}");
            compared += 1;
        }
    }
    let canonical = CALENDAR.with("zerTim", &quoted(&format!("{CDL}.ZeroTime.NY2017")), &[]);
    assert_eq!(verdicts(&canonical), (true, true), "zerTim");
    for prefix in ["", "Foo.", "Buildings.Controls.OBC.CDL.Typos."] {
        let other = CALENDAR.with("zerTim", &quoted(&format!("{prefix}ZeroTime.NY2017")), &[]);
        assert_eq!(verdicts(&other), (false, false), "zerTim {prefix}");
        compared += 1;
    }
    assert_eq!(compared, 39);
}

/// A `MatrixGain` leaf with `K` written whole as `rows x cols`, and the extra parameters.
fn gain(rows: usize, cols: usize, extra: &[(&str, &str)]) -> Vec<u8> {
    let cells: Vec<String> = (0..rows * cols).map(|i| format!("{}.0", i % 3)).collect();
    let mut members: Vec<(&str, String)> = vec![(
        "K",
        format!(
            r#""S231:isArray": true, "S231:numberDimensions": 2, "S231:sizeOfDimensions": "({rows}, {cols})", "S231:value": [{}]"#,
            cells.join(", ")
        ),
    )];
    members.extend(extra.iter().map(|(n, m)| (*n, (*m).to_owned())));
    let inputs: Vec<(String, &str)> = (1..=cols).map(|i| (format!("u{i}"), "Real")).collect();
    let outputs: Vec<(String, &str)> = (1..=rows).map(|i| (format!("y{i}"), "Real")).collect();
    let inputs: Vec<(&str, &str)> = inputs.iter().map(|(n, t)| (n.as_str(), *t)).collect();
    let outputs: Vec<(&str, &str)> = outputs.iter().map(|(n, t)| (n.as_str(), *t)).collect();
    let doc = Doc {
        class: "CDL.Reals.MatrixGain",
        inputs: &inputs,
        outputs: &outputs,
        extra: &[],
    };
    let (first, rest) = members.split_first().unwrap();
    let rest: Vec<(&str, &str)> = rest.iter().map(|(n, m)| (*n, m.as_str())).collect();
    let doc = Doc {
        extra: &rest,
        ..doc
    };
    doc.with_members(first.0, &first.1, &[])
}

#[test]
fn matrix_gain_counts_come_from_k_in_both() {
    for (rows, cols) in [(1, 1), (1, 3), (3, 1), (2, 2), (2, 3)] {
        assert_eq!(
            verdicts(&gain(rows, cols, &[])),
            (true, true),
            "{rows}x{cols}"
        );
    }
    for extra in [
        [("nin", r#""@type": "S231:Parameter", "S231:value": 2"#)],
        [("nout", r#""@type": "S231:Parameter", "S231:value": 7"#)],
    ] {
        let cxf = gain(2, 2, &extra);
        let engine = engine(&cxf);
        assert!(
            engine
                .as_ref()
                .is_err_and(|codes| codes.iter().any(|c| c == "unknown-parameter")),
            "{extra:?}: {engine:?}"
        );
        let authoring = authoring(&cxf);
        assert!(
            authoring
                .as_ref()
                .is_err_and(|codes| codes.iter().any(|c| c == "unknown-parameter")),
            "{extra:?}: {authoring:?}"
        );
    }
}

#[test]
fn what_oce_emit_writes_for_a_matrix_gain_loads_in_the_engine() {
    for (rows, cols) in [(1, 1), (1, 4), (4, 1), (3, 2)] {
        let imported = oce(&["import", "-"], &gain(rows, cols, &[]));
        assert_eq!(imported.code, 0, "{rows}x{cols}: {:?}", imported.report());
        let emitted = oce(&["emit", "-", "--from", "text"], &imported.stdout);
        assert_eq!(emitted.code, 0, "{rows}x{cols}: {:?}", emitted.report());
        assert_eq!(engine(&emitted.stdout), Ok(()), "{rows}x{cols}");
        let text = String::from_utf8_lossy(&imported.stdout);
        assert!(
            !text.contains("nin") && !text.contains("nout"),
            "the authoring layer writes no counts: {text}"
        );
    }
}
