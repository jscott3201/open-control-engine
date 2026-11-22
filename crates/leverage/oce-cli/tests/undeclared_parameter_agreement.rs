//! The engine and `oce import` / `oce emit` agree on every parameter name (issue #748): a name
//! the block's class does not declare is refused by both, under the one code
//! `unknown-parameter`, and the declared name loads in both.
//!
//! Before, the engine ignored such a name: it loaded every document below but `Constant { K }`,
//! which it refused only because the misspelling leaves the required `k` unset, and ran the
//! block on a default (`Integers.Sources.Pulse { amplitud = 3 }` output the default amplitude
//! 1), while the authoring layer refused each at its stage 1.

mod support;

use oce_api::Engine;
use serde_json::Value;
use support::oce;

const NS: &str = "http://example.org#";

/// One block of `class` with `inputs` (name, type) wired from root inputs of the same name and
/// its output `y` of type `output` wired to the root output, binding `params` (name, JSON
/// members after `@id`).
struct Doc<'a> {
    class: &'a str,
    inputs: &'a [(&'a str, &'a str)],
    output: &'a str,
}

impl Doc<'_> {
    fn with(&self, params: &[(&str, &str)]) -> Vec<u8> {
        let refs = |owner: &str, names: &mut dyn Iterator<Item = &str>| {
            names
                .map(|n| format!(r#"{{ "@id": "{NS}{owner}.{n}" }}"#))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let output = self.output;
        let mut nodes = vec![format!(
            r#"{{ "@id": "{NS}Seq", "@type": "S231:Block",
  "S231:containsBlock": {{ "@id": "{NS}Seq.blk" }}, "S231:hasInput": [ {} ],
  "S231:hasOutput": {{ "@id": "{NS}Seq.y" }} }}"#,
            refs("Seq", &mut self.inputs.iter().map(|(n, _)| *n))
        )];
        nodes.push(format!(
            r#"{{ "@id": "{NS}Seq.blk", "@type": "{NS}Buildings.Controls.OBC.{}",
  "S231:hasParameter": [ {} ], "S231:hasInput": [ {} ],
  "S231:hasOutput": {{ "@id": "{NS}Seq.blk.y" }} }}"#,
            self.class,
            refs("Seq.blk", &mut params.iter().map(|(n, _)| *n)),
            refs("Seq.blk", &mut self.inputs.iter().map(|(n, _)| *n)),
        ));
        for (name, members) in params {
            nodes.push(format!(r#"{{ "@id": "{NS}Seq.blk.{name}", {members} }}"#));
        }
        for (name, kind) in self.inputs {
            nodes.push(format!(
                r#"{{ "@id": "{NS}Seq.{name}", "@type": "S231:{kind}Input",
  "S231:isOfDataType": {{ "@id": "S231:{kind}" }}, "S231:isConnectedTo": {{ "@id": "{NS}Seq.blk.{name}" }} }}"#
            ));
            nodes.push(format!(
                r#"{{ "@id": "{NS}Seq.blk.{name}", "@type": "S231:{kind}Input",
  "S231:isOfDataType": {{ "@id": "S231:{kind}" }} }}"#
            ));
        }
        nodes.push(format!(
            r#"{{ "@id": "{NS}Seq.blk.y", "@type": "S231:{output}Output",
  "S231:isOfDataType": {{ "@id": "S231:{output}" }}, "S231:isConnectedTo": {{ "@id": "{NS}Seq.y" }} }}"#
        ));
        nodes.push(format!(
            r#"{{ "@id": "{NS}Seq.y", "@type": "S231:{output}Output",
  "S231:isOfDataType": {{ "@id": "S231:{output}" }} }}"#
        ));
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

const PULSE: Doc<'static> = Doc {
    class: "CDL.Integers.Sources.Pulse",
    inputs: &[],
    output: "Integer",
};
const CONSTANT: Doc<'static> = Doc {
    class: "CDL.Reals.Sources.Constant",
    inputs: &[],
    output: "Real",
};
const HYSTERESIS: Doc<'static> = Doc {
    class: "CDL.Reals.Hysteresis",
    inputs: &[("u", "Real")],
    output: "Boolean",
};
const PID: Doc<'static> = Doc {
    class: "CDL.Reals.PID",
    inputs: &[("u_s", "Real"), ("u_m", "Real")],
    output: "Real",
};
const SUM: Doc<'static> = Doc {
    class: "CDL.Reals.MultiSum",
    inputs: &[("u1", "Real"), ("u2", "Real")],
    output: "Real",
};
const NOT: Doc<'static> = Doc {
    class: "CDL.Logical.Not",
    inputs: &[("u", "Boolean")],
    output: "Boolean",
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

/// `"S231:value": literal`.
fn value(literal: &str) -> String {
    format!(r#""S231:value": {literal}"#)
}

const MEMBER_P: &str = r#""Buildings.Controls.OBC.CDL.Types.SimpleController.P""#;

#[test]
fn a_name_the_class_does_not_declare_is_refused_by_both_under_one_code() {
    let band = [("uLow", value("0.0")), ("uHigh", value("1.0"))];
    let cases: Vec<(&Doc<'_>, Vec<(&str, String)>)> = vec![
        (
            &PULSE,
            vec![("period", value("4")), ("amplitud", value("3"))],
        ),
        (&CONSTANT, vec![("K", value("2.5"))]),
        (
            &CONSTANT,
            vec![("k", value("2.5")), ("bogus", value("1.0"))],
        ),
        (
            &HYSTERESIS,
            vec![
                band[0].clone(),
                band[1].clone(),
                ("pre_y_strat", value("true")),
            ],
        ),
        (&PID, vec![("controlerType", value(MEMBER_P))]),
        (&SUM, vec![("nin", value("2")), ("k_x", value("1.0"))]),
        (&SUM, vec![("nin", value("2")), ("k", value("1.0"))]),
        (&SUM, vec![("nin", value("2")), ("gain_1", value("1.0"))]),
        (&NOT, vec![("nin", value("1"))]),
    ];
    for (doc, params) in cases {
        let params: Vec<(&str, &str)> = params.iter().map(|(n, m)| (*n, m.as_str())).collect();
        let cxf = doc.with(&params);
        let what = format!("{} {params:?}", doc.class);
        let engine = engine(&cxf).expect_err(&format!("{what}: the engine loaded it"));
        assert!(
            engine.iter().any(|code| code == "unknown-parameter"),
            "{what}: {engine:?}"
        );
        let authoring =
            authoring(&cxf).expect_err(&format!("{what}: the authoring layer emitted it"));
        assert!(
            authoring.iter().any(|code| code == "unknown-parameter"),
            "{what}: {authoring:?}"
        );
    }
}

#[test]
fn the_declared_names_load_in_both() {
    let cases: Vec<(&Doc<'_>, Vec<(&str, String)>)> = vec![
        (
            &PULSE,
            vec![("period", value("4")), ("amplitude", value("3"))],
        ),
        (&CONSTANT, vec![("k", value("2.5"))]),
        (
            &HYSTERESIS,
            vec![
                ("uLow", value("0.0")),
                ("uHigh", value("1.0")),
                ("pre_y_start", value("true")),
            ],
        ),
        (&PID, vec![("controllerType", value(MEMBER_P))]),
        (
            &SUM,
            vec![
                ("nin", value("2")),
                ("k_1", value("2.0")),
                ("k_2", value("3.0")),
            ],
        ),
        (&NOT, vec![]),
    ];
    for (doc, params) in cases {
        let params: Vec<(&str, &str)> = params.iter().map(|(n, m)| (*n, m.as_str())).collect();
        let cxf = doc.with(&params);
        assert_eq!(engine(&cxf), Ok(()), "{} {params:?}", doc.class);
        assert_eq!(authoring(&cxf), Ok(()), "{} {params:?}", doc.class);
    }
}
