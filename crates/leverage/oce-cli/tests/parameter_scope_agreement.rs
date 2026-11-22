//! The engine and `oce import` / `oce emit` agree on one model for a block parameter bound to a
//! sequence parameter (issue #735, counterexample 2).
//!
//! Root `p` is declared `S231:Real`; `Integers.Sources.Constant` binds `k = p`. Before, `p`
//! written `4` loaded in the engine (the scope kept the Integer the document wrote) while the
//! authoring layer refused its import at `oce emit` with `parameter-kind-mismatch`, and `p`
//! written `4.0` was refused by both. The scope now sees the Real `p` holds, so the engine refuses
//! both spellings as the authoring layer does. A `p` declared `S231:Integer` loads in both.

mod support;

use oce_api::Engine;
use serde_json::Value;
use support::oce;

/// The document: root `p` declared `datatype` and written `literal`, bound to an Integer
/// constant's `k`.
fn document(datatype: &str, literal: &str) -> Vec<u8> {
    format!(
        r#"{{
  "@context": {{ "S231": "http://data.ashrae.org/S231#" }},
  "@graph": [
    {{ "@id": "http://example.org#Seq", "@type": "S231:Block",
      "S231:containsBlock": {{ "@id": "http://example.org#Seq.con" }},
      "S231:hasParameter": [ {{ "@id": "http://example.org#Seq.p" }} ],
      "S231:hasOutput": {{ "@id": "http://example.org#Seq.y" }} }},
    {{ "@id": "http://example.org#Seq.p", "@type": "S231:Parameter",
      "S231:isOfDataType": {{ "@id": "S231:{datatype}" }}, "S231:value": {literal} }},
    {{ "@id": "http://example.org#Seq.con",
      "@type": "http://example.org#Buildings.Controls.OBC.CDL.Integers.Sources.Constant",
      "S231:hasParameter": [ {{ "@id": "http://example.org#Seq.con.k" }} ],
      "S231:hasOutput": {{ "@id": "http://example.org#Seq.con.y" }} }},
    {{ "@id": "http://example.org#Seq.con.k", "S231:value": "p" }},
    {{ "@id": "http://example.org#Seq.con.y", "@type": "S231:IntegerOutput",
      "S231:isOfDataType": {{ "@id": "S231:Integer" }},
      "S231:isConnectedTo": {{ "@id": "http://example.org#Seq.y" }} }},
    {{ "@id": "http://example.org#Seq.y", "@type": "S231:IntegerOutput",
      "S231:isOfDataType": {{ "@id": "S231:Integer" }} }}
  ]
}}"#
    )
    .into_bytes()
}

/// Every code the report of a run carries: its errors' diagnostic codes and its findings' codes.
fn codes(report: &Value) -> Vec<String> {
    let mut codes = Vec::new();
    for error in report["errors"].as_array().into_iter().flatten() {
        for code in error["diagnostic_codes"].as_array().into_iter().flatten() {
            codes.extend(code.as_str().map(str::to_owned));
        }
    }
    for finding in report["findings"].as_array().into_iter().flatten() {
        codes.extend(finding["code"].as_str().map(str::to_owned));
    }
    codes
}

/// The engine's verdict on `cxf`: loads, or the codes of its refusal.
fn engine(cxf: &[u8]) -> Result<(), Vec<String>> {
    Engine::new().load_cxf(cxf).map(|_| ()).map_err(|error| {
        error
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.code.as_str().to_owned())
            .collect()
    })
}

/// `oce emit`'s verdict on what `oce import` reads from `cxf` (the import itself judges
/// nothing): emitted, or the codes of its refusal.
fn import_then_emit(cxf: &[u8]) -> Result<(), Vec<String>> {
    let imported = oce(&["import", "-"], cxf);
    assert_eq!(imported.code, 0, "{}", imported.transcript());
    let emitted = oce(&["emit", "-", "--from", "text"], &imported.stdout);
    let report = emitted.report();
    if report["ok"] == true {
        Ok(())
    } else {
        Err(codes(&report))
    }
}

#[test]
fn a_real_root_parameter_bound_to_an_integer_constant_is_refused_by_both_in_both_spellings() {
    for literal in [
        "4",
        "4.0",
        r#"{ "@value": "4", "@type": "http://www.w3.org/2001/XMLSchema#integer" }"#,
    ] {
        let cxf = document("Real", literal);
        let engine = engine(&cxf);
        assert_eq!(
            engine,
            Err(vec!["parameter-kind-mismatch".to_owned()]),
            "{literal}"
        );
        let authoring = import_then_emit(&cxf);
        assert!(
            authoring
                .as_ref()
                .is_err_and(|codes| codes.iter().any(|code| code == "parameter-kind-mismatch")),
            "{literal}: {authoring:?}"
        );
        // And `oce id`, which loads the document in the engine, has no id for it.
        let id = oce(&["id", "-", "--from", "cxf", "--json"], &cxf);
        assert_eq!(id.code, 2, "{}", id.transcript());
        assert_eq!(
            codes(&id.report()),
            ["parameter-kind-mismatch"],
            "{literal}"
        );
    }
}

#[test]
fn an_integer_root_parameter_bound_to_an_integer_constant_loads_in_both() {
    let cxf = document("Integer", "4");
    assert_eq!(engine(&cxf), Ok(()));
    let id = oce(&["id", "-", "--from", "cxf"], &cxf);
    assert_eq!(id.code, 0, "{}", id.transcript());
    let imported = oce(&["import", "-"], &cxf);
    assert_eq!(imported.code, 0, "{}", imported.transcript());
    // The authoring layer emits it.
    let emitted = oce(&["emit", "-", "--from", "text"], &imported.stdout);
    assert_eq!(emitted.code, 0, "{}", emitted.transcript());
}

/// Issue #759's reproduction: a `PID` leaf whose own output is conditional on `reverseActing =
/// "yMax + 1 > yMax"`, with `yMax` written `literal`.
fn leaf_guard_document(literal: &str) -> Vec<u8> {
    let ns = "http://example.org#";
    let real = |id: &str, kind: &str, extra: &str| {
        format!(
            r#"{{ "@id": "{ns}{id}", "@type": "S231:Real{kind}", "S231:isOfDataType": {{ "@id": "S231:Real" }}{extra} }}"#
        )
    };
    let nodes = [
        format!(
            r#"{{ "@id": "{ns}S", "@type": "S231:Block", "S231:containsBlock": {{ "@id": "{ns}S.pid" }},
               "S231:hasInput": [ {{ "@id": "{ns}S.u_s" }}, {{ "@id": "{ns}S.u_m" }} ],
               "S231:hasOutput": {{ "@id": "{ns}S.y" }} }}"#
        ),
        real(
            "S.u_s",
            "Input",
            &format!(r#", "S231:isConnectedTo": {{ "@id": "{ns}S.pid.u_s" }}"#),
        ),
        real(
            "S.u_m",
            "Input",
            &format!(r#", "S231:isConnectedTo": {{ "@id": "{ns}S.pid.u_m" }}"#),
        ),
        format!(
            r#"{{ "@id": "{ns}S.pid", "@type": "{ns}Buildings.Controls.OBC.CDL.Reals.PID",
               "S231:hasParameter": [ {{ "@id": "{ns}S.pid.controllerType" }}, {{ "@id": "{ns}S.pid.yMax" }},
                                      {{ "@id": "{ns}S.pid.reverseActing" }} ],
               "S231:hasInput": [ {{ "@id": "{ns}S.pid.u_s" }}, {{ "@id": "{ns}S.pid.u_m" }} ],
               "S231:hasOutput": {{ "@id": "{ns}S.pid.y" }} }}"#
        ),
        format!(
            r#"{{ "@id": "{ns}S.pid.controllerType", "S231:value": "Buildings.Controls.OBC.CDL.Types.SimpleController.P" }}"#
        ),
        format!(r#"{{ "@id": "{ns}S.pid.yMax", "S231:value": {literal} }}"#),
        format!(r#"{{ "@id": "{ns}S.pid.reverseActing", "S231:value": "yMax + 1 > yMax" }}"#),
        real("S.pid.u_s", "Input", ""),
        real("S.pid.u_m", "Input", ""),
        real(
            "S.pid.y",
            "Output",
            &format!(
                r#", "S231:isConditionalComponent": true, "S231:conditionalExpression": "reverseActing", "S231:isConnectedTo": {{ "@id": "{ns}S.y" }}"#
            ),
        ),
        real("S.y", "Output", ""),
    ];
    format!(
        r#"{{ "@context": {{ "S231": "http://data.ashrae.org/S231#" }}, "@graph": [ {} ] }}"#,
        nodes.join(",\n")
    )
    .into_bytes()
}

#[test]
fn a_leaf_guard_over_a_real_member_is_one_verdict_in_every_spelling_for_the_engine_and_oce() {
    // Issue #759: over `yMax` at 2^53 the Real sum rounds back, the guard is false and the PID
    // is left without its output. Before, the Integer spellings loaded (`oce id` printed an id)
    // and the Real ones were refused. The authoring layer has no conditional components: `oce
    // import` refuses every spelling alike, with one code.
    let spellings = [
        "9007199254740992",
        "9007199254740992.0",
        r#"{ "@value": "9007199254740992", "@type": "http://www.w3.org/2001/XMLSchema#integer" }"#,
        r#"{ "@value": "9007199254740992.0", "@type": "http://www.w3.org/2001/XMLSchema#double" }"#,
    ];
    let first = leaf_guard_document(spellings[0]);
    let expected = engine(&first).expect_err("the guarded output is pruned");
    assert!(
        expected.iter().any(|code| code == "malformed-document"),
        "{expected:?}"
    );
    let imported = oce(&["import", "-"], &first);
    assert_eq!(imported.code, 2, "{}", imported.transcript());
    let import_codes = imported.report()["errors"][0]["code"].clone();
    assert_eq!(import_codes, "conditional-component");
    for literal in spellings {
        let cxf = leaf_guard_document(literal);
        assert_eq!(engine(&cxf), Err(expected.clone()), "{literal}");
        let id = oce(&["id", "-", "--from", "cxf", "--json"], &cxf);
        assert_eq!(id.code, 2, "{literal}: {}", id.transcript());
        assert_eq!(codes(&id.report()), expected, "{literal}");
        let imported = oce(&["import", "-"], &cxf);
        assert_eq!(imported.code, 2, "{literal}: {}", imported.transcript());
        assert_eq!(
            imported.report()["errors"][0]["code"],
            import_codes,
            "{literal}"
        );
    }
}
