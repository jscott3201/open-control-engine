//! An enumeration is named by its canonical class path in every place a CXF document names one
//! (issue #762, the owner's ruling of 2026-11-22; typing IRIs, issue #637): a parameter's value,
//! a value handed down through nested composites, a conditional guard's literal, a typing IRI,
//! and a G36 Integer constant. Any other path whose last segment names a known enumeration or
//! constant package is refused `unknown-enum-type`, once, naming the canonical path expected and
//! quoting the written text by its excerpt, made visible. Before, a CDL enumeration was matched by
//! its last segment, and a typing IRI that matched nothing typed nothing.

use oce_cxf::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};
use oce_model::ModelGraph;

const NESTED: &str = include_str!("fixtures/nested_composite.jsonld");

const G36: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types";
const VENTILATION: &str = "Buildings.Controls.OBC.ASHRAE.G36.Types.VentilationStandard";

/// The ways the ruling names of writing `VentilationStandard` other than its canonical path.
const OTHER_PATHS: [&str; 6] = [
    "VentilationStandard",
    "Foo.VentilationStandard",
    "ASHRAE.G36.Types.VentilationStandard",
    "Modelica.Blocks.Types.VentilationStandard",
    "Buildings.Controls.OBC.CDL.Types.VentilationStandard",
    "Buildings.Controls.OBC.ASHRAE.G36.Typos.VentilationStandard",
];

/// A root `R` with the G36-typed parameter `venStd` (typing IRI `datatype`, value `value`), one
/// Boolean `Constant` `cmp` whose `k` is `venStd == <literal>` wired to the root output, and one
/// Real `Constant` `opt` conditional on the guard `guard`.
fn gated(datatype: &str, value: &str, literal: &str, guard: &str) -> String {
    let ns = "http://example.org#R";
    format!(
        r#"{{ "@context": {{ "S231": "http://data.ashrae.org/S231#" }}, "@graph": [
  {{ "@id": "{ns}", "@type": "S231:Block",
    "S231:hasParameter": {{ "@id": "{ns}.venStd" }},
    "S231:containsBlock": [ {{ "@id": "{ns}.cmp" }}, {{ "@id": "{ns}.opt" }} ],
    "S231:hasOutput": {{ "@id": "{ns}.y" }} }},
  {{ "@id": "{ns}.venStd", "@type": "S231:Parameter",
    "S231:isOfDataType": {{ "@id": "{datatype}" }}, "S231:value": "{value}" }},
  {{ "@id": "{ns}.y", "@type": "S231:BooleanOutput", "S231:isOfDataType": {{ "@id": "S231:Boolean" }} }},
  {{ "@id": "{ns}.cmp", "@type": "http://example.org#Buildings.Controls.OBC.CDL.Logical.Sources.Constant",
    "S231:hasParameter": {{ "@id": "{ns}.cmp.k" }}, "S231:hasOutput": {{ "@id": "{ns}.cmp.y" }} }},
  {{ "@id": "{ns}.cmp.k", "S231:value": "venStd == {literal}" }},
  {{ "@id": "{ns}.cmp.y", "@type": "S231:BooleanOutput", "S231:isOfDataType": {{ "@id": "S231:Boolean" }},
    "S231:isConnectedTo": {{ "@id": "{ns}.y" }} }},
  {{ "@id": "{ns}.opt", "@type": "http://example.org#Buildings.Controls.OBC.CDL.Reals.Sources.Constant",
    "S231:isConditionalComponent": true, "S231:conditionalExpression": "{guard}",
    "S231:hasParameter": {{ "@id": "{ns}.opt.k" }}, "S231:hasOutput": {{ "@id": "{ns}.opt.y" }} }},
  {{ "@id": "{ns}.opt.k", "S231:value": 1.0 }},
  {{ "@id": "{ns}.opt.y", "@type": "S231:RealOutput", "S231:isOfDataType": {{ "@id": "S231:Real" }} }}
] }}"#
    )
}

/// [`gated`] with every spelling canonical and the member `California_Title_24` throughout.
fn canonical_gated() -> String {
    let member = format!("{VENTILATION}.California_Title_24");
    gated(
        &format!("http://example.org#{VENTILATION}"),
        &member,
        &member,
        &format!("venStd == {member}"),
    )
}

fn import(doc: &str) -> Result<ModelGraph, Vec<Diagnostic>> {
    match import_cxf(doc.as_bytes(), &ResolveOptions::default()) {
        Ok((graph, _)) => Ok(graph),
        Err(CxfError::Validation(diags)) => Err(diags),
        Err(other) => panic!("not a validation refusal: {other:?}"),
    }
}

/// The one `unknown-enum-type` refusal naming `expected`, and nothing else of that code.
fn assert_named(diags: &[Diagnostic], expected: &str, what: &str) {
    let named: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == DiagCode::UnknownEnumType)
        .collect();
    assert_eq!(named.len(), 1, "{what}: {diags:#?}");
    assert!(
        named[0].message.contains(expected),
        "{what}: {}",
        named[0].message
    );
}

/// Whether the conditional block survived specialization.
fn has_opt(graph: &ModelGraph) -> bool {
    graph.blocks.iter().any(|b| {
        b.instance_iri
            .as_deref()
            .is_some_and(|iri| iri.ends_with(".opt"))
    })
}

/// The `k` the comparison block grounded.
fn compared(graph: &ModelGraph) -> bool {
    graph
        .blocks
        .iter()
        .find(|b| {
            b.instance_iri
                .as_deref()
                .is_some_and(|iri| iri.ends_with(".cmp"))
        })
        .and_then(|b| b.params.values.iter().find(|(n, _)| n.as_ref() == "k"))
        .is_some_and(|(_, v)| matches!(v, oce_model::Value::Boolean(true)))
}

#[test]
fn a_conditional_guard_and_a_comparison_read_their_literal_by_the_canonical_path_only() {
    let typed = format!("http://example.org#{VENTILATION}");
    let member = |m: &str| format!("{VENTILATION}.{m}");
    // The canonical literal compares: the member written decides the guard and the comparison.
    let graph = import(&canonical_gated()).expect("the canonical document loads");
    assert!(has_opt(&graph) && compared(&graph));
    let other = member("ASHRAE62_1");
    let graph = import(&gated(
        &typed,
        &member("California_Title_24"),
        &other,
        &format!("venStd == {other}"),
    ))
    .expect("loads");
    assert!(!has_opt(&graph) && !compared(&graph));
    for path in OTHER_PATHS {
        let written = format!("{path}.California_Title_24");
        let canonical = member("California_Title_24");
        // In the guard.
        let doc = gated(
            &typed,
            &canonical,
            &canonical,
            &format!("venStd == {written}"),
        );
        assert_named(
            &import(&doc).expect_err(path),
            &format!("{VENTILATION}.<member>"),
            path,
        );
        // In an expression a block parameter grounds.
        let doc = gated(
            &typed,
            &canonical,
            &written,
            &format!("venStd == {canonical}"),
        );
        assert_named(
            &import(&doc).expect_err(path),
            &format!("{VENTILATION}.<member>"),
            path,
        );
    }
    // A canonical class with a member it lacks keeps its refusal; another case is no path.
    let canonical = member("California_Title_24");
    let unknown = gated(
        &typed,
        &canonical,
        &canonical,
        &format!("venStd == {}", member("Nope")),
    );
    let codes: Vec<DiagCode> = import(&unknown)
        .unwrap_err()
        .iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(codes, [DiagCode::UnknownEnumLiteral]);
    let lower = gated(
        &typed,
        &canonical,
        &canonical,
        &format!("venStd == {}", canonical.to_lowercase()),
    );
    assert!(import(&lower).is_err(), "another letter case is refused");
}

#[test]
fn a_parameter_value_names_its_member_by_the_canonical_path_only() {
    let typed = format!("http://example.org#{VENTILATION}");
    let canonical = format!("{VENTILATION}.California_Title_24");
    for path in OTHER_PATHS {
        let written = format!("{path}.California_Title_24");
        let doc = gated(
            &typed,
            &written,
            &canonical,
            &format!("venStd == {canonical}"),
        );
        let diags = import(&doc).expect_err(path);
        assert_named(&diags, &format!("{VENTILATION}.<member>"), path);
        assert!(
            diags.iter().all(|d| d.code != DiagCode::TypeMismatch),
            "{path}: reported once, as unknown-enum-type: {diags:#?}"
        );
    }
}

#[test]
fn a_typing_iri_names_its_enumeration_by_the_canonical_path_only() {
    let canonical = format!("{VENTILATION}.California_Title_24");
    let guard = format!("venStd == {canonical}");
    for path in OTHER_PATHS {
        let doc = gated(
            &format!("http://example.org#{path}"),
            &canonical,
            &canonical,
            &guard,
        );
        assert_named(&import(&doc).expect_err(path), VENTILATION, path);
    }
    // Issue #637's reproduction and a typo in a CDL type's own name: the declared type of a
    // parameter whose value is a canonical CDL member. The comparison and guard read `venStd`, so
    // the CDL member there is refused for its kind; what is asserted is the typing refusal.
    let zero = "Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017";
    for datatype in [
        "http://example.org#Buildings.Controls.OBC.CDL.Types.ZeroTime",
        "http://data.ashrae.org/S231#Buildings.Controls.OBC.CDL.Types.ZeroTime",
    ] {
        let diags = import(&gated(datatype, zero, &canonical, &guard)).unwrap_err();
        assert!(
            diags.iter().all(|d| d.code != DiagCode::UnknownEnumType),
            "{datatype} types the parameter: {diags:#?}"
        );
    }
    for (datatype, expected) in [
        (
            "http://example.org#Buildings.Controls.OBC.CDL.Typos.ZeroTime",
            "Buildings.Controls.OBC.CDL.Types.ZeroTime",
        ),
        (
            "http://example.org#ZeroTime",
            "Buildings.Controls.OBC.CDL.Types.ZeroTime",
        ),
        (
            "http://example.org#Modelica.Blocks.Types.ZeroTime",
            "Buildings.Controls.OBC.CDL.Types.ZeroTime",
        ),
        (
            "http://example.org#Buildings.Controls.OBC.CDL.Types.ZeroTim",
            "the CDL enumerations are",
        ),
    ] {
        let diags = import(&gated(datatype, zero, &canonical, &guard)).expect_err(datatype);
        assert_named(&diags, expected, datatype);
    }
}

#[test]
fn a_value_handed_down_through_two_composite_scopes_is_read_by_the_canonical_path_only() {
    let canonical = format!("\"{VENTILATION}.California_Title_24\"");
    assert!(NESTED.contains(&canonical));
    import(NESTED).expect("the fixture loads");
    for path in ["VentilationStandard", "Foo.VentilationStandard"] {
        let doc = NESTED.replace(&canonical, &format!("\"{path}.California_Title_24\""));
        let diags = import(&doc).expect_err(path);
        assert_named(&diags, &format!("{VENTILATION}.<member>"), path);
    }
    // The inner composite's own parameter, typed by a non-canonical path, is refused on itself.
    let typed = format!("\"http://example.org#{VENTILATION}\"");
    let second = NESTED
        .match_indices(&typed)
        .nth(1)
        .expect("the inner declaration")
        .0;
    let inner = format!(
        "{}\"http://example.org#VentilationStandard\"{}",
        &NESTED[..second],
        &NESTED[second + typed.len()..]
    );
    assert_ne!(inner, NESTED);
    assert_named(
        &import(&inner).expect_err("inner typing"),
        VENTILATION,
        "inner",
    );
}

#[test]
fn a_g36_integer_constant_is_named_by_its_package_canonical_path_only() {
    // `venStd` declared an Integer through the constant package's path, compared in the block
    // with an Integer: the canonical member stands for its Integer; any other spelling is refused.
    let typed = format!("http://example.org#{G36}.OperationModes");
    let canonical = format!("{VENTILATION}.California_Title_24");
    let doc = |value: &str| {
        gated(&typed, value, "7", &format!("venStd == {canonical}"))
            .replace(
                &format!(
                    "\"S231:isConditionalComponent\": true, \"S231:conditionalExpression\": \"venStd == {canonical}\","
                ),
                "",
            )
    };
    let graph = import(&doc(&format!("{G36}.OperationModes.unoccupied")))
        .expect("the canonical constant loads");
    assert!(compared(&graph), "unoccupied is 7");
    let graph = import(&doc(&format!("{G36}.OperationModes.occupied"))).expect("loads");
    assert!(!compared(&graph), "occupied is 1");
    for written in [
        "OperationModes.unoccupied",
        "ASHRAE.G36.Types.OperationModes.unoccupied",
        "Foo.OperationModes.unoccupied",
    ] {
        let diags = import(&doc(written)).expect_err(written);
        assert_named(&diags, &format!("{G36}.OperationModes.<member>"), written);
    }
    // An IRI-prefixed member is no value spelling: refused.
    assert!(
        import(&doc(&format!(
            "http://example.org#{G36}.OperationModes.unoccupied"
        )))
        .is_err()
    );
}

#[test]
fn a_hostile_written_path_is_quoted_by_its_excerpt_made_visible() {
    let typed = format!("http://example.org#{VENTILATION}");
    let canonical = format!("{VENTILATION}.California_Title_24");
    let long = format!(
        "{}.VentilationStandard.California_Title_24",
        "A".repeat(10_000)
    );
    let doc = gated(&typed, &long, &canonical, &format!("venStd == {canonical}"));
    let diags = import(&doc).expect_err("refused");
    let named: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == DiagCode::UnknownEnumType)
        .collect();
    assert_eq!(named.len(), 1, "{diags:#?}");
    let message = &named[0].message;
    assert!(message.len() < 512, "{} bytes", message.len());
    assert!(message.contains(VENTILATION), "{message}");
    // A typing IRI carrying a long junk path before a known type name, likewise.
    let junk = format!("http://example.org#{}.ZeroTime", "B".repeat(10_000));
    let doc = gated(
        &junk,
        "Buildings.Controls.OBC.CDL.Types.ZeroTime.NY2017",
        &canonical,
        &format!("venStd == {canonical}"),
    );
    let diags = import(&doc).expect_err("refused");
    let named: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == DiagCode::UnknownEnumType)
        .collect();
    assert_eq!(named.len(), 1, "{diags:#?}");
    assert!(named[0].message.len() < 512, "{}", named[0].message.len());
}
