//! Unit tests for enumeration references in `eval.rs` (issue #762): a reference resolves by its
//! canonical class path only. A path whose last segment names a known enumeration or constant
//! package is refused naming the canonical path, inside any expression; an unknown class stays
//! `UnknownIdent`.

use oce_model::{
    EnumClassId, Value, enum_class_id, enum_member_ordinal, enumeration_class_by_path,
};

use super::{EvalResult, ExprError, Scope, eval_str};

/// The evaluator's production lookups behind a scope with no identifiers.
struct EnumScope;

impl Scope for EnumScope {
    fn lookup(&self, _name: &str) -> Option<&EvalResult> {
        None
    }

    fn enum_class(&self, qualified: &str) -> Option<EnumClassId> {
        enum_class_id(qualified)
    }

    fn enum_ordinal(&self, class: EnumClassId, literal: &str) -> Option<u32> {
        enum_member_ordinal(class, literal)
    }
}

fn run(text: &str) -> Result<Value, ExprError> {
    match eval_str(text, &EnumScope)? {
        EvalResult::Scalar(value) => Ok(value),
        other => panic!("expected a scalar, got {other:?}"),
    }
}

const SIMPLE_CONTROLLER: &str = "Buildings.Controls.OBC.CDL.Types.SimpleController";

fn non_canonical(written: &str, canonical: &str, member: &str) -> ExprError {
    ExprError::NonCanonicalEnumeration {
        written: written.to_owned(),
        canonical: enumeration_class_by_path(canonical)
            .expect("a table row")
            .qualified_path,
        member: member.to_owned(),
    }
}

#[test]
fn a_non_canonical_enumeration_path_is_refused_naming_the_canonical_one() {
    for written in [
        "SimpleController",
        "Foo.SimpleController",
        "Modelica.Blocks.Types.SimpleController",
        "Buildings.Controls.OBC.ASHRAE.G36.Types.SimpleController",
        "CDL.Types.SimpleController",
        "buildings.controls.obc.cdl.types.SimpleController",
    ] {
        let expected = non_canonical(written, SIMPLE_CONTROLLER, "PD");
        let text = format!("{written}.PD");
        assert_eq!(run(&text).unwrap_err(), expected, "{text}");
        // Inside a larger expression.
        let branch = format!("not ({SIMPLE_CONTROLLER}.PI <> {written}.PD) or true");
        assert_eq!(run(&branch).unwrap_err(), expected, "{branch}");
        let comparison = format!("{SIMPLE_CONTROLLER}.PD == {written}.PD");
        assert_eq!(run(&comparison).unwrap_err(), expected, "{comparison}");
    }
}

#[test]
fn a_constant_package_by_a_short_path_is_refused_naming_its_canonical_path() {
    let package = "Buildings.Controls.OBC.ASHRAE.G36.Types.OperationModes";
    assert_eq!(
        run("OperationModes.occupied").unwrap_err(),
        non_canonical("OperationModes", package, "occupied")
    );
    assert_eq!(
        run("ASHRAE.G36.Types.OperationModes.occupied + 1").unwrap_err(),
        non_canonical("ASHRAE.G36.Types.OperationModes", package, "occupied")
    );
}

#[test]
fn a_canonical_reference_and_an_unknown_class_keep_their_results() {
    assert!(
        run(&format!("{SIMPLE_CONTROLLER}.PD"))
            .unwrap()
            .bit_eq(&Value::Enum {
                class: EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 3,
            })
    );
    assert!(
        run(&format!("{SIMPLE_CONTROLLER}.PD == {SIMPLE_CONTROLLER}.PD"))
            .unwrap()
            .bit_eq(&Value::Boolean(true))
    );
    assert_eq!(
        run("Foo.Bar.PD").unwrap_err(),
        ExprError::UnknownIdent("Foo.Bar".to_owned())
    );
    // A canonical class with an unknown or miscased member keeps its refusal.
    for member in ["XX", "pd"] {
        let text = format!("{SIMPLE_CONTROLLER}.{member}");
        assert_eq!(
            run(&text).unwrap_err(),
            ExprError::UnknownIdent(text.clone())
        );
    }
}

/// modelica-json (pinned 85721b82) copies a source's relative path as written: CXF-Core's
/// `TimeTable` defaults are `CDL.Types.Extrapolation.Periodic` and
/// `CDL.Types.Smoothness.LinearSegments` (`third_party/modelica-buildings-cdl/cxf-core/
/// CXF-Core.jsonld` lines 8979 and 8999). Each is refused, and the message names the whole
/// canonical member; a member the enumeration lacks is named by the placeholder.
#[test]
fn a_translators_relative_path_is_refused_naming_the_whole_canonical_member() {
    for (written, member, canonical) in [
        (
            "CDL.Types.Extrapolation",
            "Periodic",
            "Buildings.Controls.OBC.CDL.Types.Extrapolation",
        ),
        (
            "CDL.Types.Smoothness",
            "LinearSegments",
            "Buildings.Controls.OBC.CDL.Types.Smoothness",
        ),
    ] {
        let error = run(&format!("{written}.{member}")).unwrap_err();
        assert_eq!(error, non_canonical(written, canonical, member));
        assert!(
            error
                .to_string()
                .ends_with(&format!("write `{canonical}.{member}`")),
            "{error}"
        );
    }
    let unknown = run("CDL.Types.Smoothness.Nope").unwrap_err();
    assert!(
        unknown
            .to_string()
            .ends_with("write `Buildings.Controls.OBC.CDL.Types.Smoothness.<member>`"),
        "{unknown}"
    );
}
