//! The facade's `SimpleController` row (`oce_api::enumeration`, issue #468) checked against the
//! engine's CXF load: each member the row lists is loaded through `Engine::load_cxf` in a minimal
//! CXF document and must ground to the ordinal the row gives it, the row must be exactly the four
//! upstream members in source order, and a name outside the row must be refused. The validator,
//! which reads the same row, agrees member by member.
//!
//! Parity note for the engine-side stage (#464): through this full-path string spelling the engine
//! refuses an unknown member as `grounding-failed`, while stage 1 reports `unknown-enum-literal`
//! (the note's table). The test pins what the engine says today so the parity test sees it.

mod support;

use oce_api::{DiagCode, Engine, Value};
use oce_author::{Block, Literal, ParamValue, validate_author_side};

/// A one-PID document whose `controllerType` is `member` (full CDL path).
fn pid_document(member: &str) -> Vec<u8> {
    let base = "http://example.org#";
    serde_json::json!({
        "@context": { "S231": "http://data.ashrae.org/S231#" },
        "@graph": [
            {
                "@id": format!("{base}S"), "@type": "S231:Block",
                "S231:containsBlock": [{ "@id": format!("{base}S.con") }],
                "S231:hasInput": [{ "@id": format!("{base}S.us") }, { "@id": format!("{base}S.um") }],
                "S231:hasOutput": [{ "@id": format!("{base}S.y") }]
            },
            {
                "@id": format!("{base}S.con"),
                "@type": format!("{base}Buildings.Controls.OBC.CDL.Reals.PID"),
                "S231:hasParameter": [{ "@id": format!("{base}S.con.controllerType") }],
                "S231:hasInput": [{ "@id": format!("{base}S.con.u_s") }, { "@id": format!("{base}S.con.u_m") }],
                "S231:hasOutput": [{ "@id": format!("{base}S.con.y") }]
            },
            {
                "@id": format!("{base}S.con.controllerType"), "@type": "S231:Parameter",
                "S231:isOfDataType": { "@id": format!("{base}Buildings.Controls.OBC.CDL.Types.SimpleController") },
                "S231:value": format!("Buildings.Controls.OBC.CDL.Types.SimpleController.{member}")
            },
            { "@id": format!("{base}S.con.u_s"), "@type": "S231:RealInput", "S231:isOfDataType": { "@id": "S231:Real" } },
            { "@id": format!("{base}S.con.u_m"), "@type": "S231:RealInput", "S231:isOfDataType": { "@id": "S231:Real" } },
            {
                "@id": format!("{base}S.con.y"), "@type": "S231:RealOutput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": [{ "@id": format!("{base}S.y") }]
            },
            {
                "@id": format!("{base}S.us"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": [{ "@id": format!("{base}S.con.u_s") }]
            },
            {
                "@id": format!("{base}S.um"), "@type": "S231:RealInput",
                "S231:isOfDataType": { "@id": "S231:Real" },
                "S231:isConnectedTo": [{ "@id": format!("{base}S.con.u_m") }]
            },
            { "@id": format!("{base}S.y"), "@type": "S231:RealOutput", "S231:isOfDataType": { "@id": "S231:Real" } }
        ]
    })
    .to_string()
    .into_bytes()
}

/// The ordinal the engine grounds `member` to, or the refusal codes.
fn engine_ordinal(member: &str) -> Result<u32, Vec<DiagCode>> {
    let mut engine = Engine::new();
    match engine.load_cxf(&pid_document(member)) {
        Ok(_) => match engine.get_param("http://example.org#S.con.controllerType") {
            Ok(Value::Enum { ordinal, .. }) => Ok(ordinal),
            other => panic!("controllerType reads as {other:?}"),
        },
        Err(error) => {
            let codes: Vec<DiagCode> = error.diagnostics().iter().map(|d| d.code).collect();
            assert!(
                !codes.is_empty(),
                "a refusal with diagnostics for {member}: {error:?}"
            );
            Err(codes)
        }
    }
}

fn author_findings(member: &str) -> Vec<&'static str> {
    let mut block = Block::new(support::id("con"), "CDL.Reals.PID");
    block.assignments.push(oce_author::Assignment::new(
        support::id("controllerType"),
        ParamValue::Literal(Literal::Member(oce_author::MemberRef::parse(&format!(
            "SimpleController.{member}"
        )))),
    ));
    let mut s = oce_author::Sequence::new(support::id("s"));
    s.blocks.push(block);
    validate_author_side(&s)
        .into_iter()
        .map(|f| f.code)
        .collect()
}

#[test]
fn simple_controller_members_ground_to_the_tables_ordinals_in_the_engine() {
    let row = oce_api::enumeration("CDL.Types.SimpleController").expect("facade row");
    assert_eq!(
        row.members,
        ["P", "PI", "PD", "PID"],
        "upstream source order"
    );
    for (ordinal, member) in row.members.iter().enumerate() {
        assert_eq!(
            engine_ordinal(member),
            Ok(u32::try_from(ordinal + 1).expect("small")),
            "{member}"
        );
        assert_eq!(author_findings(member), Vec::<&str>::new(), "{member}");
    }
}

#[test]
fn a_name_outside_the_table_is_refused_by_both() {
    for bogus in ["PIX", "pi", "PDI"] {
        let refused = engine_ordinal(bogus).expect_err("the engine refuses it");
        assert_eq!(refused, vec![DiagCode::GroundingFailed], "{bogus}");
        assert_eq!(
            author_findings(bogus),
            vec!["unknown-enum-literal"],
            "{bogus}"
        );
    }
}
