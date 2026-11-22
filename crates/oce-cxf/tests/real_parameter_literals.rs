//! An Integer bound to a Real parameter imports as the Real of the same value (issue #683).
//!
//! A CXF document may write a Real parameter's number as an Integer: a bare JSON integer, a typed
//! `xsd:integer` literal, an Integer expression, or a reference to an Integer-valued sequence
//! parameter. Every such spelling imports to the model the Real spelling imports to, so the
//! render, the export and everything built on the model agree. An Integer above 2^53 in
//! magnitude is refused with `parameter-out-of-range` on the parameter node that bound it, and an
//! Integer on an Integer parameter is never touched.
//!
//! Every document here is built in the test from one small template (a source block feeding a
//! root output), so no tracked fixture is added and each case states the literal under test.

use crate::render;

use oce_cxf::{CxfError, ResolveOptions, export, import_cxf};
use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::{ModelGraph, Value};
use render::render;

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const TWO_POW_53: i64 = 1 << 53;
const REAL_CONSTANT: &str = "CDL.Reals.Sources.Constant";
const INTEGER_CONSTANT: &str = "CDL.Integers.Sources.Constant";

/// One parameter node of the block `S.blk`: its local name and the JSON members after `@id`.
struct Param<'a> {
    name: &'a str,
    members: String,
}

fn value<'a>(name: &'a str, literal: &str) -> Param<'a> {
    Param {
        name,
        members: format!(r#""S231:value": {literal}"#),
    }
}

fn typed(lexical: &str, datatype: &str) -> String {
    format!(r#"{{ "@value": "{lexical}", "@type": "{XSD}{datatype}" }}"#)
}

/// A root `S` holding one source block `blk` of `class` with `params`, its one output wired to
/// the root output. `root_params` are parameter nodes declared on the root (name, members).
fn document(class: &str, signal: &str, params: &[Param<'_>], root_params: &[Param<'_>]) -> String {
    let refs = |owner: &str, list: &[Param<'_>]| {
        list.iter()
            .map(|param| {
                format!(
                    r#"{{ "@id": "http://example.org#{owner}.{}" }}"#,
                    param.name
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let nodes = |owner: &str, list: &[Param<'_>]| {
        list.iter()
            .map(|param| {
                format!(
                    r#"    {{ "@id": "http://example.org#{owner}.{}", {} }},
"#,
                    param.name, param.members
                )
            })
            .collect::<String>()
    };
    format!(
        r#"{{
  "@context": {{ "S231": "http://data.ashrae.org/S231#" }},
  "@graph": [
    {{ "@id": "http://example.org#S",
      "@type": "S231:Block",
      "S231:containsBlock": {{ "@id": "http://example.org#S.blk" }},
      "S231:hasParameter": [ {root_refs} ],
      "S231:hasOutput": {{ "@id": "http://example.org#S.out" }} }},
{root_nodes}    {{ "@id": "http://example.org#S.blk",
      "@type": "http://example.org#Buildings.Controls.OBC.{class}",
      "S231:hasParameter": [ {refs} ],
      "S231:hasOutput": {{ "@id": "http://example.org#S.blk.y" }} }},
{nodes}    {{ "@id": "http://example.org#S.blk.y",
      "@type": "S231:{signal}Output",
      "S231:isOfDataType": {{ "@id": "S231:{signal}" }},
      "S231:isConnectedTo": {{ "@id": "http://example.org#S.out" }} }},
    {{ "@id": "http://example.org#S.out",
      "@type": "S231:{signal}Output",
      "S231:isOfDataType": {{ "@id": "S231:{signal}" }} }}
  ]
}}"#,
        root_refs = refs("S", root_params),
        root_nodes = nodes("S", root_params),
        refs = refs("S.blk", params),
        nodes = nodes("S.blk", params),
    )
}

/// A Real constant whose `k` is written as `literal`.
fn real_constant(literal: &str) -> String {
    document(REAL_CONSTANT, "Real", &[value("k", literal)], &[])
}

fn import_ok(doc: &str) -> ModelGraph {
    match import_cxf(doc.as_bytes(), &ResolveOptions::default()) {
        Ok((graph, report)) => {
            assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
            graph
        }
        Err(error) => panic!("expected the document to import, got {error:?}\n{doc}"),
    }
}

fn refusal(doc: &str) -> Vec<Diagnostic> {
    match import_cxf(doc.as_bytes(), &ResolveOptions::default()) {
        Err(CxfError::Validation(diags)) => diags,
        other => panic!("expected a resolver refusal, got {other:?}\n{doc}"),
    }
}

/// The parameters of the one authored block, in table order.
fn params(graph: &ModelGraph) -> Vec<(String, Value)> {
    let block = graph
        .blocks
        .iter()
        .find(|block| block.instance_iri.as_deref() == Some("http://example.org#S.blk"))
        .expect("the authored block");
    block
        .params
        .values
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect()
}

fn assert_params(doc: &str, expected: &[(&str, Value)]) {
    let actual = params(&import_ok(doc));
    assert_eq!(actual.len(), expected.len(), "{actual:?}");
    for ((name, value), (expected_name, expected_value)) in actual.iter().zip(expected) {
        assert_eq!(name, expected_name);
        assert!(
            value.bit_eq(expected_value),
            "{name}: {value:?} is not {expected_value:?}\n{doc}"
        );
    }
}

/// The one refusal an Integer past the bound gives, located on `S.blk.<name>`.
fn out_of_range(name: &str, integer: i64, class: &str) -> Diagnostic {
    Diagnostic::error(
        DiagCode::ParameterOutOfRange,
        format!(
            "parameter `{name}` on `http://example.org#S.blk` binds the Integer {integer} to a \
             Real parameter of `{class}`; an Integer above 2^53 in magnitude is not exact as a \
             Real"
        ),
    )
    .with_subject(format!("http://example.org#S.blk.{name}"))
}

#[test]
fn every_integer_spelling_of_a_real_parameter_imports_as_the_model_of_the_real_spelling() {
    let as_real = real_constant(&typed("7.0", "double"));
    let expected = import_ok(&as_real);
    assert_params(&as_real, &[("k", Value::Real(7.0))]);
    let expected_render = render(&expected);
    let expected_export = export(&expected).expect("exports");
    for literal in [
        "7".to_owned(),
        "7.0".to_owned(),
        typed("7", "integer"),
        typed("7", "int"),
        typed("7", "long"),
        typed("7", "decimal"),
        r#""3 + 4""#.to_owned(),
        r#""7""#.to_owned(),
    ] {
        let graph = import_ok(&real_constant(&literal));
        assert_eq!(render(&graph), expected_render, "{literal}");
        assert_eq!(
            export(&graph).expect("exports"),
            expected_export,
            "{literal}"
        );
    }
}

#[test]
fn reference_to_an_integer_sequence_parameter_binds_a_real_parameter_as_a_real() {
    // The shape of G36 `TrimAndRespond`: `Constant numIgnReqCon(k = numIgnReq)` with an Integer
    // `numIgnReq`. Whether the sequence parameter is declared Integer or Real, and whether its
    // value is written `2` or as a typed integer, the block's Real parameter holds 2.0.
    for members in [
        r#""S231:value": 2"#.to_owned(),
        format!(
            r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "S231:Integer" }}, "S231:value": {}"#,
            typed("2", "integer")
        ),
        r#""@type": "S231:Parameter", "S231:isOfDataType": { "@id": "S231:Real" }, "S231:value": 2"#
            .to_owned(),
    ] {
        let doc = document(
            REAL_CONSTANT,
            "Real",
            &[value("k", r#""n""#)],
            &[Param { name: "n", members }],
        );
        assert_params(&doc, &[("k", Value::Real(2.0))]);
    }
}

#[test]
fn integer_zero_is_positive_zero_and_a_real_negative_zero_keeps_its_sign() {
    // A typed `xsd:integer` of `-0` is the one Integer zero, so it is +0.0.
    for literal in [
        "0".to_owned(),
        typed("0", "integer"),
        typed("-0", "integer"),
    ] {
        assert_params(&real_constant(&literal), &[("k", Value::Real(0.0))]);
    }
    // A bare JSON `-0` never was an Integer: the JSON reader takes it as the float -0.0, as it
    // did before this rule, so it is a Real like `-0.0`.
    for literal in ["-0".to_owned(), "-0.0".to_owned(), typed("-0.0", "double")] {
        assert_params(&real_constant(&literal), &[("k", Value::Real(-0.0))]);
    }
    // The two zeros are two models, as they were: only the Integer spelling is normalised.
    assert_ne!(
        render(&import_ok(&real_constant("0"))),
        render(&import_ok(&real_constant("-0.0")))
    );
}

#[test]
fn integers_up_to_two_to_the_fifty_third_convert_exactly() {
    for integer in [
        1,
        -1,
        i64::from(i32::MAX),
        i64::from(i32::MIN),
        TWO_POW_53 - 1,
        -(TWO_POW_53 - 1),
        TWO_POW_53,
        -TWO_POW_53,
    ] {
        for literal in [integer.to_string(), typed(&integer.to_string(), "integer")] {
            let graph = import_ok(&real_constant(&literal));
            let (_, held) = &params(&graph)[0];
            let Value::Real(real) = held else {
                panic!("{literal}: held as {held:?}");
            };
            assert_eq!(*real as i64, integer, "{literal}");
            assert_eq!(real.to_bits(), (integer as f64).to_bits(), "{literal}");
        }
    }
    assert_params(
        &real_constant("9007199254740992"),
        &[("k", Value::Real(f64::from_bits(0x4340_0000_0000_0000)))],
    );
}

#[test]
fn integers_past_two_to_the_fifty_third_are_refused_at_the_parameter_node() {
    for integer in [
        TWO_POW_53 + 1,
        -(TWO_POW_53 + 1),
        1 << 54,
        i64::MAX,
        i64::MIN + 1,
        i64::MIN,
    ] {
        let expected = vec![out_of_range("k", integer, REAL_CONSTANT)];
        for literal in [
            integer.to_string(),
            typed(&integer.to_string(), "integer"),
            typed(&integer.to_string(), "long"),
        ] {
            let diags = refusal(&real_constant(&literal));
            assert_eq!(diags, expected, "{literal}");
            assert_eq!(diags[0].severity, Severity::Error);
            assert_eq!(diags[0].code.as_str(), "parameter-out-of-range");
        }
    }
}

#[test]
fn integer_past_the_bound_reached_through_a_reference_is_refused_at_the_binding_parameter() {
    // The sequence parameter itself is an Integer and holds the value without loss; binding it to
    // a Real parameter is what cannot be exact, so the refusal names the block's parameter.
    let doc = document(
        REAL_CONSTANT,
        "Real",
        &[value("k", r#""n""#)],
        &[value("n", "9007199254740993")],
    );
    assert_eq!(
        refusal(&doc),
        vec![out_of_range("k", TWO_POW_53 + 1, REAL_CONSTANT)]
    );
    // The same reference on an Integer parameter loads.
    let doc = document(
        INTEGER_CONSTANT,
        "Integer",
        &[value("k", r#""n""#)],
        &[value("n", "9007199254740993")],
    );
    assert_params(&doc, &[("k", Value::Integer(TWO_POW_53 + 1))]);
}

#[test]
fn integer_expression_past_the_bound_is_refused_and_one_at_the_bound_converts() {
    let at = real_constant(r#""9007199254740991 + 1""#);
    assert_params(&at, &[("k", Value::Real(TWO_POW_53 as f64))]);
    let past = real_constant(r#""9007199254740992 + 1""#);
    assert_eq!(
        refusal(&past),
        vec![out_of_range("k", TWO_POW_53 + 1, REAL_CONSTANT)]
    );
}

#[test]
fn integer_on_an_integer_parameter_stays_an_integer_at_every_magnitude() {
    for integer in [0, 7, -7, TWO_POW_53 + 1, i64::MAX, i64::MIN] {
        for literal in [integer.to_string(), typed(&integer.to_string(), "integer")] {
            let doc = document(INTEGER_CONSTANT, "Integer", &[value("k", &literal)], &[]);
            assert_params(&doc, &[("k", Value::Integer(integer))]);
        }
    }
}

#[test]
fn integer_parameter_exports_bare_and_regrounds_as_an_integer() {
    // The reverse type-flip guard: an Integer parameter must not grow a fractional part on export
    // or come back as a Real.
    let doc = document(INTEGER_CONSTANT, "Integer", &[value("k", "7")], &[]);
    let graph = import_ok(&doc);
    let bytes = export(&graph).expect("exports");
    let text = std::str::from_utf8(&bytes).expect("export emits UTF-8 JSON");
    assert!(
        text.contains(r#"{"@id":"http://example.org#S.blk.k","S231:value":7}"#),
        "{text}"
    );
    let (again, _) = import_cxf(&bytes, &ResolveOptions::default()).expect("the export imports");
    assert_eq!(render(&again), render(&graph));
    assert!(params(&again)[0].1.bit_eq(&Value::Integer(7)));
}

#[test]
fn real_parameter_written_as_an_integer_exports_as_a_real_and_reaches_a_fixpoint() {
    let graph = import_ok(&real_constant("7"));
    let bytes = export(&graph).expect("exports");
    let text = std::str::from_utf8(&bytes).expect("export emits UTF-8 JSON");
    assert!(
        text.contains(r#"{"@id":"http://example.org#S.blk.k","S231:value":7.0}"#),
        "{text}"
    );
    let (again, _) = import_cxf(&bytes, &ResolveOptions::default()).expect("the export imports");
    assert_eq!(render(&again), render(&graph));
    assert_eq!(export(&again).expect("exports"), bytes);
}

#[test]
fn integer_and_real_siblings_of_one_block_are_each_held_in_their_catalog_type() {
    // `Integers.Sources.Pulse`: `amplitude` and `offset` are Integers, `width`, `period` and
    // `shift` Reals. Every one is written as a bare JSON integer.
    let doc = document(
        "CDL.Integers.Sources.Pulse",
        "Integer",
        &[
            value("amplitude", "3"),
            value("width", "1"),
            value("period", "4"),
            value("shift", "0"),
            value("offset", "2"),
        ],
        &[],
    );
    assert_params(
        &doc,
        &[
            ("amplitude", Value::Integer(3)),
            ("width", Value::Real(1.0)),
            ("period", Value::Real(4.0)),
            ("shift", Value::Real(0.0)),
            ("offset", Value::Integer(2)),
        ],
    );
}

#[test]
fn a_later_sibling_expression_reads_the_real_the_parameter_holds() {
    // Issue #735, the owner's ruling: the sibling scope sees the Real a Real parameter holds, so
    // `amplitude = period` reads the Real 4.0 whether `period` is written `4` or `4.0`, and the
    // two documents import to one model. That model binds a Real to the Integer `amplitude`,
    // which the engine's load gate refuses (`oce-api`'s `parameter_scope_kinds.rs`); before, the
    // `4` spelling loaded with amplitude 4 and the `4.0` spelling ran on the default amplitude 1.
    let models: Vec<String> = ["4", "4.0", r#""2 + 2""#]
        .into_iter()
        .map(|period| {
            let doc = document(
                "CDL.Integers.Sources.Pulse",
                "Integer",
                &[value("period", period), value("amplitude", r#""period""#)],
                &[],
            );
            assert_params(
                &doc,
                &[
                    ("period", Value::Real(4.0)),
                    ("amplitude", Value::Real(4.0)),
                ],
            );
            render(&import_ok(&doc))
        })
        .collect();
    assert!(models.windows(2).all(|pair| pair[0] == pair[1]));
}

/// A `Reals.Sources.TimeTable` whose `table[2,2]` and `offset[1]` are written as `table` and
/// `offset` (the members after `@id` of each node).
fn time_table(table: &str, offset: &str) -> String {
    document(
        "CDL.Reals.Sources.TimeTable",
        "Real",
        &[
            Param {
                name: "table",
                members: table.to_owned(),
            },
            Param {
                name: "offset",
                members: offset.to_owned(),
            },
        ],
        &[],
    )
}

fn array_members(dimensions: usize, size: &str, literal: &str) -> String {
    format!(
        r#""S231:isArray": true, "S231:numberDimensions": {dimensions}, "S231:sizeOfDimensions": "{size}", "S231:value": {literal}"#
    )
}

#[test]
fn integer_elements_of_real_array_parameters_import_as_reals_in_every_array_spelling() {
    let expected = [
        ("table_1_1", Value::Real(0.0)),
        ("table_1_2", Value::Real(1.0)),
        ("table_2_1", Value::Real(3600.0)),
        ("table_2_2", Value::Real(2.5)),
        ("offset_1", Value::Real(0.0)),
    ];
    let as_reals = time_table(
        &array_members(2, "(2, 2)", "[0.0, 1.0, 3600.0, 2.5]"),
        &array_members(1, "(1)", "[0.0]"),
    );
    assert_params(&as_reals, &expected);
    let expected_render = render(&import_ok(&as_reals));
    for (table, offset) in [
        // A JSON list of bare integers beside one float.
        ("[0, 1, 3600, 2.5]".to_owned(), "[0]".to_owned()),
        // Typed integer elements.
        (
            format!(
                "[{}, {}, {}, 2.5]",
                typed("0", "integer"),
                typed("1", "integer"),
                typed("3600", "integer")
            ),
            format!("[{}]", typed("0", "integer")),
        ),
        // An Integer array expression (the expression subset is one-dimensional).
        ("[0, 1, 3600, 2.5]".to_owned(), r#""fill(0, 1)""#.to_owned()),
    ] {
        let doc = time_table(
            &array_members(2, "(2, 2)", &table),
            &array_members(1, "(1)", &offset),
        );
        assert_eq!(
            render(&import_ok(&doc)),
            expected_render,
            "{table} {offset}"
        );
    }
}

#[test]
fn integer_element_past_the_bound_is_refused_at_the_array_parameter_node() {
    let doc = time_table(
        &array_members(2, "(2, 2)", "[0, 1, 9007199254740993, 2]"),
        &array_members(1, "(1)", "[0]"),
    );
    assert_eq!(
        refusal(&doc),
        vec![
            Diagnostic::error(
                DiagCode::ParameterOutOfRange,
                "parameter `table_2_1` on `http://example.org#S.blk` binds the Integer \
             9007199254740993 to a Real parameter of `CDL.Reals.Sources.TimeTable`; an Integer \
             above 2^53 in magnitude is not exact as a Real"
            )
            .with_subject("http://example.org#S.blk.table".to_owned())
        ]
    );
}

#[test]
fn import_of_an_integer_spelling_is_byte_identical_across_runs_and_so_is_its_refusal() {
    let doc = real_constant("7");
    assert_eq!(render(&import_ok(&doc)), render(&import_ok(&doc)));
    let refused = real_constant("9223372036854775807");
    assert_eq!(refusal(&refused), refusal(&refused));
}

#[test]
fn bare_json_number_outside_the_integer_range_was_never_an_integer_and_is_read_as_a_real() {
    // The rule judges Integers. A bare JSON number with no fraction that does not fit an `i64`
    // is not one: the JSON reader takes it as a float, rounded to the nearest binary64 as any
    // decimal Real literal is, exactly as before this rule. So the first value past `i64::MAX`
    // loads where `i64::MAX` itself is refused. Recorded as it stands; whether the reader should
    // refuse such a token is a separate question from this rule (issue #719).
    for (literal, bits) in [
        ("9223372036854775808", 0x43e0_0000_0000_0000u64),
        ("9223372036854775809", 0x43e0_0000_0000_0000),
        ("18446744073709551615", 0x43f0_0000_0000_0000),
        ("-9223372036854775809", 0xc3e0_0000_0000_0000),
        ("100000000000000000000", 1e20f64.to_bits()),
    ] {
        assert_params(
            &real_constant(literal),
            &[("k", Value::Real(f64::from_bits(bits)))],
        );
    }
    // The typed spelling of the same digits is an Integer literal that does not parse.
    let diags = refusal(&real_constant(&typed("9223372036854775808", "integer")));
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, DiagCode::GroundingFailed);
}

#[test]
fn a_sequence_parameter_declared_real_is_the_real_to_every_binding_that_reads_it() {
    // Issue #735: the scope holds a declared-Real sequence parameter as a Real, so an Integer
    // block parameter bound to it imports the Real (and the engine refuses it); before, `2` kept
    // the Integer and imported clean. A Real block parameter reads the same Real either way.
    for literal in ["2", "2.0", &typed("2", "integer"), r#""1 + 1""#] {
        let root = [Param {
            name: "p",
            members: format!(
                r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "S231:Real" }}, "S231:value": {literal}"#
            ),
        }];
        let integer = document(INTEGER_CONSTANT, "Integer", &[value("k", r#""p""#)], &root);
        assert_params(&integer, &[("k", Value::Real(2.0))]);
        let real = document(REAL_CONSTANT, "Real", &[value("k", r#""p * 3""#)], &root);
        assert_params(&real, &[("k", Value::Real(6.0))]);
    }
}

#[test]
fn a_value_of_another_kind_than_a_sequence_parameters_declared_type_is_refused_on_it() {
    for (datatype, literal, kind) in [
        ("Integer", "2.0", "Real"),
        ("Integer", "true", "Boolean"),
        ("Real", "true", "Boolean"),
        ("Boolean", "1", "Integer"),
    ] {
        let doc = document(
            REAL_CONSTANT,
            "Real",
            &[value("k", "1.0")],
            &[Param {
                name: "p",
                members: format!(
                    r#""@type": "S231:Parameter", "S231:isOfDataType": {{ "@id": "S231:{datatype}" }}, "S231:value": {literal}"#
                ),
            }],
        );
        assert_eq!(
            refusal(&doc),
            vec![
                Diagnostic::error(
                    DiagCode::ParameterKindMismatch,
                    format!(
                        "parameter `p` is declared {datatype} and binds a value of kind {kind}"
                    ),
                )
                .with_subject("http://example.org#S.p".to_owned())
            ],
            "{datatype} {literal}"
        );
    }
    // Its own kind loads, and an untyped sequence parameter keeps whatever kind it was written in.
    let integer = document(
        INTEGER_CONSTANT,
        "Integer",
        &[value("k", r#""p""#)],
        &[value("p", "2")],
    );
    assert_params(&integer, &[("k", Value::Integer(2))]);
}
