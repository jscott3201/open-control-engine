//! Each refusal the enumeration check raises names the parameter node and says exactly what it
//! said when every refusal was built before it was raised (issue #799): one case for each way a
//! G36 enumeration's value is refused and each way a typing IRI is.

use oce_diag::{DiagCode, Diagnostic, DiagnosticList};
use oce_expr::{EvalResult, Scope};
use oce_model::{EnumClassId, Value};

use super::validate_enumeration_parameter_value;
use crate::dto::{CxfValue, Node};

/// The G36 enumeration the G36 cases declare.
const VENTILATION: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard";

/// A scope where the name `other` holds a `CoolingCoil` value and nothing else is bound.
struct OneName;

impl Scope for OneName {
    fn lookup(&self, name: &str) -> Option<&EvalResult> {
        static COOLING_COIL: EvalResult = EvalResult::Scalar(Value::Enum {
            class: EnumClassId::G36_COOLING_COIL,
            ordinal: 1,
        });
        (name == "other").then_some(&COOLING_COIL)
    }
}

/// What the check raises for the node `urn:g.e` typed by `type_iri` whose value is `value`.
fn raised(type_iri: &str, value: CxfValue) -> Vec<Diagnostic> {
    let node: Node = serde_json::from_value(serde_json::json!({
        "@id": "urn:g.e",
        "S231:isOfDataType": { "@id": type_iri },
    }))
    .expect("a node");
    let mut diags = DiagnosticList::new();
    validate_enumeration_parameter_value(&node, &value, &OneName, &mut diags);
    let (listed, closing) = diags.into_parts();
    assert!(closing.is_none());
    listed
}

/// The one refusal `code` with `message` on the node `urn:g.e`, stated by hand.
fn on_the_node(code: DiagCode, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(code, message).with_subject("urn:g.e".to_owned())]
}

/// A G36 value written as the expression `text`.
fn expr(text: &str) -> CxfValue {
    CxfValue::Expr(text.to_owned())
}

#[test]
fn every_g36_value_refusal_names_the_node_and_says_exactly_what_it_said() {
    let declared = format!("http://example.org#{VENTILATION}");
    let t = declared.as_str();
    let cases: Vec<(CxfValue, DiagCode, String)> = vec![
        (
            CxfValue::Int(1),
            DiagCode::EnumIntegerStandin,
            "G36 enum parameters require canonical enum literals, not numeric stand-ins".into(),
        ),
        (
            CxfValue::Bool(true),
            DiagCode::UnknownEnumLiteral,
            "G36 enum parameter value is not a canonical enum literal".into(),
        ),
        (
            expr("Buildings.Controls.OBC.ASHRAE.G36.Types.DemandLimitLevels.cooling0"),
            DiagCode::EnumIntegerStandin,
            "G36 integer constants cannot stand in for enum literals".into(),
        ),
        (
            expr("other"),
            DiagCode::TypeMismatch,
            format!("G36 enum parameter reference `other` does not match declared type `{t}`"),
        ),
        (
            expr("nothing"),
            DiagCode::UnknownEnumLiteral,
            "G36 enum literal `nothing` is not fully qualified".into(),
        ),
        (
            expr("Foo.Bar.baz"),
            DiagCode::TypeMismatch,
            format!("`Foo.Bar.baz` is not a literal of `{t}`"),
        ),
        (
            expr("Buildings.Controls.OBC.ASHRAE.G36.Types.Nope.x"),
            DiagCode::UnknownEnumType,
            format!("`Buildings.Controls.OBC.ASHRAE.G36.Types.Nope.x` is not a literal of `{t}`"),
        ),
        (
            expr("Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil.DXCoil"),
            DiagCode::TypeMismatch,
            format!(
                "G36 enum literal `Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil.DXCoil` \
                 does not match declared type `{t}`"
            ),
        ),
        (
            expr(&format!("{VENTILATION}.nope")),
            DiagCode::UnknownEnumLiteral,
            format!("unknown G36 enum literal `{VENTILATION}.nope`"),
        ),
    ];
    for (value, code, message) in cases {
        assert_eq!(
            raised(t, value.clone()),
            on_the_node(code, &message),
            "{value:?}"
        );
    }
    // A member of the declared enumeration, and a bare name holding one, raise nothing.
    assert!(raised(t, expr(&format!("{VENTILATION}.ASHRAE62_1"))).is_empty());
}

#[test]
fn every_typing_iri_refusal_names_the_node_and_says_exactly_what_it_said() {
    assert_eq!(
        raised("http://example.org#ZeroTime", CxfValue::Int(1)),
        on_the_node(
            DiagCode::UnknownEnumType,
            "enumeration `ZeroTime` is not written under its canonical class path; write \
             `Buildings.Controls.OBC.CDL.Types.ZeroTime`"
        )
    );
    let unknown_cdl = raised(
        "http://example.org#Buildings.Controls.OBC.CDL.Types.ZeroTim",
        CxfValue::Int(1),
    );
    assert_eq!(
        unknown_cdl,
        on_the_node(
            DiagCode::UnknownEnumType,
            "unknown CDL enumeration type `Buildings.Controls.OBC.CDL.Types.ZeroTim`; the CDL \
             enumerations are Buildings.Controls.OBC.CDL.Types.SimpleController, \
             Buildings.Controls.OBC.CDL.Types.Smoothness, \
             Buildings.Controls.OBC.CDL.Types.Extrapolation, \
             Buildings.Controls.OBC.CDL.Types.ZeroTime"
        )
    );
    assert_eq!(
        raised(
            "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.Nope",
            CxfValue::Int(1)
        ),
        on_the_node(
            DiagCode::UnknownEnumType,
            "unknown G36 enum type \
             `http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.Nope`"
        )
    );
    // A canonical CDL enumeration and an untyped scalar raise nothing here.
    assert!(
        raised(
            "http://example.org#Buildings.Controls.OBC.CDL.Types.ZeroTime",
            CxfValue::Int(1)
        )
        .is_empty()
    );
    assert!(raised("S231:Real", CxfValue::Int(1)).is_empty());
}
