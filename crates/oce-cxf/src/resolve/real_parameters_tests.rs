//! An Integer bound to a Real parameter is held as a Real, exactly or not at all (issues #683,
//! #735): a block member by its catalog type, a composite parameter by its declared type.
//!
//! These tests drive the two holds alone, on values built by hand, so every case states the
//! parameter's class (or declaration), name and grounded value. `tests/real_parameter_literals.rs`
//! runs the same rule through documents, and `oce-api`'s `parameter_scope_kinds.rs` through the
//! scope, end to end.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::{EnumClassId, Value, ValueType};

use oce_diag::DiagnosticList;

use crate::dto::Node;

use super::real_parameters::{DeclaredTypeFault, hold_in_catalog_type, hold_in_declared_type};

const TWO_POW_53: i64 = 1 << 53;
const OWNER: &str = "urn:test#Seq.blk";

type Table = Vec<(Arc<str>, Value)>;

/// Hold each entry in its catalog type, in order, with the source IRI `urn:test#Seq.blk.<name>`.
fn held(class_path: &str, entries: &[(&str, Value)]) -> (Table, Vec<Diagnostic>) {
    let mut diags = DiagnosticList::new();
    let table = entries
        .iter()
        .map(|(name, value)| {
            let source = format!("{OWNER}.{name}");
            let value =
                hold_in_catalog_type(class_path, OWNER, &source, name, value.clone(), &mut diags);
            (Arc::from(*name), value)
        })
        .collect();
    (table, diags.to_vec())
}

fn assert_bit_eq(actual: &Table, expected: &[(&str, Value)]) {
    assert_eq!(actual.len(), expected.len());
    for ((name, value), (expected_name, expected_value)) in actual.iter().zip(expected) {
        assert_eq!(name.as_ref(), *expected_name);
        assert!(
            value.bit_eq(expected_value),
            "{name}: {value:?} is not {expected_value:?}"
        );
    }
}

#[test]
fn integer_on_a_real_scalar_becomes_the_real_and_other_entries_keep_their_order() {
    let (table, diags) = held(
        "CDL.Reals.Hysteresis",
        &[
            ("uLow", Value::Integer(0)),
            ("uHigh", Value::Integer(-3)),
            ("pre_y_start", Value::Boolean(false)),
        ],
    );
    assert!(diags.is_empty(), "{diags:?}");
    assert_bit_eq(
        &table,
        &[
            ("uLow", Value::Real(0.0)),
            ("uHigh", Value::Real(-3.0)),
            ("pre_y_start", Value::Boolean(false)),
        ],
    );
}

#[test]
fn integer_on_an_integer_parameter_stays_an_integer() {
    let (table, diags) = held("CDL.Integers.Sources.Constant", &[("k", Value::Integer(7))]);
    assert!(diags.is_empty());
    assert_bit_eq(&table, &[("k", Value::Integer(7))]);

    // The width of a Real-vector class is an Integer beside its Real elements, and an Integer
    // past 2^53 on an Integer parameter is not this rule's to judge.
    let (table, diags) = held(
        "CDL.Reals.MultiSum",
        &[
            ("nin", Value::Integer(2)),
            ("k_1", Value::Integer(4)),
            ("k_2", Value::Real(0.5)),
        ],
    );
    assert!(diags.is_empty());
    assert_bit_eq(
        &table,
        &[
            ("nin", Value::Integer(2)),
            ("k_1", Value::Real(4.0)),
            ("k_2", Value::Real(0.5)),
        ],
    );
    let (table, diags) = held(
        "CDL.Integers.Sources.Constant",
        &[("k", Value::Integer(i64::MAX))],
    );
    assert!(diags.is_empty());
    assert_bit_eq(&table, &[("k", Value::Integer(i64::MAX))]);
}

#[test]
fn integer_elements_of_an_integer_array_stay_integers() {
    let (table, diags) = held(
        "CDL.Integers.MultiSum",
        &[
            ("nin", Value::Integer(2)),
            ("k_1", Value::Integer(4)),
            ("k_2", Value::Integer(-1)),
        ],
    );
    assert!(diags.is_empty());
    assert_bit_eq(
        &table,
        &[
            ("nin", Value::Integer(2)),
            ("k_1", Value::Integer(4)),
            ("k_2", Value::Integer(-1)),
        ],
    );
}

#[test]
fn integer_elements_of_a_real_matrix_become_reals() {
    let (table, diags) = held(
        "CDL.Reals.Sources.TimeTable",
        &[
            ("table_1_1", Value::Integer(0)),
            ("table_1_2", Value::Integer(1)),
            ("table_2_1", Value::Integer(3600)),
            ("table_2_2", Value::Real(2.5)),
            ("timeScale", Value::Integer(1)),
            ("offset_1", Value::Integer(0)),
        ],
    );
    assert!(diags.is_empty());
    assert_bit_eq(
        &table,
        &[
            ("table_1_1", Value::Real(0.0)),
            ("table_1_2", Value::Real(1.0)),
            ("table_2_1", Value::Real(3600.0)),
            ("table_2_2", Value::Real(2.5)),
            ("timeScale", Value::Real(1.0)),
            ("offset_1", Value::Real(0.0)),
        ],
    );
}

#[test]
fn enumeration_ordinal_written_as_an_integer_is_not_a_real_parameter() {
    let (table, diags) = held(
        "CDL.Reals.PID",
        &[
            ("controllerType", Value::Integer(2)),
            ("k", Value::Integer(2)),
        ],
    );
    assert!(diags.is_empty());
    assert_bit_eq(
        &table,
        &[
            ("controllerType", Value::Integer(2)),
            ("k", Value::Real(2.0)),
        ],
    );
}

#[test]
fn values_that_are_not_integers_are_untouched_bit_for_bit() {
    let entries = [
        ("uLow", Value::Real(-0.0)),
        ("uHigh", Value::Real(f64::MIN_POSITIVE)),
        ("pre_y_start", Value::Boolean(true)),
        ("note", Value::String(Arc::from("0"))),
        (
            "kind",
            Value::Enum {
                class: EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 1,
            },
        ),
    ];
    let (table, diags) = held("CDL.Reals.Hysteresis", &entries);
    assert!(diags.is_empty());
    assert_bit_eq(&table, &entries);
}

#[test]
fn names_the_class_does_not_declare_and_unregistered_classes_are_left_as_grounded() {
    // Whether such a document loads is the validator's question; nothing is guessed here.
    for (class_path, name) in [
        ("CDL.Reals.Hysteresis", "notAParameter"),
        ("CDL.Reals.Hysteresis", "uLow_1"),
        ("CDL.Reals.MultiSum", "k"),
        ("CDL.Reals.MultiSum", "k_0"),
        ("CDL.Reals.MultiSum", "k_01"),
        ("CDL.Reals.MultiSum", "k_+1"),
        ("CDL.Reals.MultiSum", "k_1_1"),
        ("CDL.Reals.Sources.TimeTable", "table_1"),
        ("CDL.Reals.Sources.TimeTable", "table_1_2_3"),
        ("CDL.Reals.NoSuchClass", "k"),
        ("", "k"),
    ] {
        let (table, diags) = held(class_path, &[(name, Value::Integer(i64::MAX))]);
        assert!(diags.is_empty(), "{class_path} {name}");
        assert_bit_eq(&table, &[(name, Value::Integer(i64::MAX))]);
    }
}

#[test]
fn integer_past_the_bound_on_a_real_parameter_is_refused_at_its_parameter_node() {
    for integer in [TWO_POW_53 + 1, -(TWO_POW_53 + 1), i64::MAX, i64::MIN] {
        let (table, diags) = held("CDL.Reals.Hysteresis", &[("uLow", Value::Integer(integer))]);
        assert_eq!(diags.len(), 1, "{integer}");
        let diagnostic = &diags[0];
        assert_eq!(diagnostic.code, DiagCode::ParameterOutOfRange);
        assert_eq!(diagnostic.code.as_str(), "parameter-out-of-range");
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.subject.as_deref(), Some("urn:test#Seq.blk.uLow"));
        assert_eq!(
            diagnostic.message,
            format!(
                "parameter `uLow` on `urn:test#Seq.blk` binds the Integer {integer} to a Real \
                 parameter of `CDL.Reals.Hysteresis`; an Integer above 2^53 in magnitude is not \
                 exact as a Real"
            )
        );
        // The refused value is not rounded into the table.
        assert_bit_eq(&table, &[("uLow", Value::Integer(integer))]);
    }
}

#[test]
fn each_refused_entry_is_reported_once_in_table_order_and_the_rest_still_convert() {
    let (table, diags) = held(
        "CDL.Reals.MultiSum",
        &[
            ("nin", Value::Integer(3)),
            ("k_1", Value::Integer(i64::MAX)),
            ("k_2", Value::Integer(TWO_POW_53)),
            ("k_3", Value::Integer(i64::MIN)),
        ],
    );
    let subjects: Vec<_> = diags
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code,
                diagnostic.subject.as_deref().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        subjects,
        [
            (
                DiagCode::ParameterOutOfRange,
                "urn:test#Seq.blk.k_1".to_owned()
            ),
            (
                DiagCode::ParameterOutOfRange,
                "urn:test#Seq.blk.k_3".to_owned()
            ),
        ]
    );
    assert_bit_eq(
        &table,
        &[
            ("nin", Value::Integer(3)),
            ("k_1", Value::Integer(i64::MAX)),
            ("k_2", Value::Real(TWO_POW_53 as f64)),
            ("k_3", Value::Integer(i64::MIN)),
        ],
    );
}

#[test]
fn the_pass_is_idempotent_and_repeats_bit_for_bit() {
    let entries = [
        ("uLow", Value::Integer(-2)),
        ("uHigh", Value::Integer(TWO_POW_53)),
    ];
    let (once, _) = held("CDL.Reals.Hysteresis", &entries);
    let again: Vec<(&str, Value)> = once
        .iter()
        .map(|(name, value)| (name.as_ref(), value.clone()))
        .collect();
    let (twice, diags) = held("CDL.Reals.Hysteresis", &again);
    assert!(diags.is_empty());
    assert_bit_eq(&twice, &again);
    let (repeat, _) = held("CDL.Reals.Hysteresis", &entries);
    assert_bit_eq(&repeat, &again);
}

/// A composite parameter node declared `datatype` (`None`: no `S231:isOfDataType`).
fn declared(datatype: Option<&str>) -> Node {
    let typed = datatype
        .map(|id| format!(r#", "S231:isOfDataType": {{ "@id": "{id}" }}"#))
        .unwrap_or_default();
    serde_json::from_str(&format!(r#"{{ "@id": "urn:test#Seq.p"{typed} }}"#)).expect("a node")
}

#[test]
fn an_integer_on_a_declared_real_is_held_as_the_real_and_exactly_or_not_at_all() {
    let real = declared(Some("S231:Real"));
    for integer in [0, 4, -4, TWO_POW_53, -TWO_POW_53] {
        let held = hold_in_declared_type(&real, Value::Integer(integer)).expect("exact");
        assert!(held.bit_eq(&Value::Real(integer as f64)), "{integer}");
    }
    assert!(
        hold_in_declared_type(&real, Value::Integer(0))
            .unwrap()
            .bit_eq(&Value::Real(0.0))
    );
    for integer in [TWO_POW_53 + 1, i64::MAX, i64::MIN] {
        let (fault, kept) = hold_in_declared_type(&real, Value::Integer(integer)).unwrap_err();
        assert_eq!(fault, DeclaredTypeFault::InexactInteger(integer));
        assert!(kept.bit_eq(&Value::Integer(integer)), "kept as grounded");
        let diagnostic = fault.diagnostic("urn:test#Seq.p", "p", &kept);
        assert_eq!(diagnostic.code, DiagCode::ParameterOutOfRange);
        assert_eq!(diagnostic.subject.as_deref(), Some("urn:test#Seq.p"));
    }
    // A Real stays the Real it was, bit for bit.
    for real_value in [-0.0, 2.5, f64::MAX] {
        assert!(
            hold_in_declared_type(&real, Value::Real(real_value))
                .unwrap()
                .bit_eq(&Value::Real(real_value))
        );
    }
}

#[test]
fn a_value_of_another_kind_than_the_declared_one_is_refused_on_the_declaration() {
    for (datatype, declared_type, value) in [
        ("S231:Integer", ValueType::Integer, Value::Real(2.0)),
        ("S231:Integer", ValueType::Integer, Value::Real(-0.0)),
        ("S231:Integer", ValueType::Integer, Value::Boolean(true)),
        ("S231:Real", ValueType::Real, Value::Boolean(false)),
        ("S231:Real", ValueType::Real, Value::String(Arc::from("2"))),
        ("S231:Boolean", ValueType::Boolean, Value::Integer(1)),
    ] {
        let node = declared(Some(datatype));
        let (fault, kept) = hold_in_declared_type(&node, value.clone()).unwrap_err();
        assert_eq!(
            fault,
            DeclaredTypeFault::WrongKind {
                declared: declared_type
            }
        );
        assert!(kept.bit_eq(&value));
        let diagnostic = fault.diagnostic("urn:test#Seq.p", "p", &kept);
        assert_eq!(diagnostic.code, DiagCode::ParameterKindMismatch);
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.subject.as_deref(), Some("urn:test#Seq.p"));
        assert!(
            diagnostic.message.starts_with("parameter `p` is declared"),
            "{}",
            diagnostic.message
        );
    }
}

#[test]
fn an_undeclared_parameter_keeps_what_the_document_wrote() {
    // A non-canonical enumeration path declares no type here: it is refused on the declaration
    // before grounding (`enumeration_parameters`), so the hold never sees it as a type.
    for datatype in [
        None,
        Some("S231:NotAType"),
        Some("http://example.org#SimpleController"),
    ] {
        let node = declared(datatype);
        for value in [
            Value::Integer(4),
            Value::Real(4.0),
            Value::Boolean(true),
            Value::Enum {
                class: EnumClassId::SIMPLE_CONTROLLER,
                ordinal: 1,
            },
        ] {
            assert!(
                hold_in_declared_type(&node, value.clone())
                    .unwrap()
                    .bit_eq(&value),
                "{datatype:?} {value:?}"
            );
        }
    }
    // A declared CDL enumeration keeps a member of its own class, and refuses every other value
    // (issue #762); a declared G36 enumeration is judged before grounding and kept here.
    let simple = declared(Some("Buildings.Controls.OBC.CDL.Types.SimpleController"));
    let member = Value::Enum {
        class: EnumClassId::SIMPLE_CONTROLLER,
        ordinal: 3,
    };
    assert!(
        hold_in_declared_type(&simple, member.clone())
            .unwrap()
            .bit_eq(&member)
    );
    for value in [
        Value::Integer(3),
        Value::Real(3.0),
        Value::Boolean(true),
        Value::String("Buildings.Controls.OBC.CDL.Types.SimpleController.PD".into()),
        Value::Enum {
            class: EnumClassId::SMOOTHNESS,
            ordinal: 1,
        },
    ] {
        let (fault, kept) = hold_in_declared_type(&simple, value.clone()).unwrap_err();
        assert_eq!(
            fault,
            DeclaredTypeFault::WrongKind {
                declared: ValueType::Enum(EnumClassId::SIMPLE_CONTROLLER)
            },
            "{value:?}"
        );
        assert!(kept.bit_eq(&value));
        let message = fault.diagnostic("urn:p", "p", &kept).message;
        assert!(
            message.contains("Buildings.Controls.OBC.CDL.Types.SimpleController.<member>"),
            "{message}"
        );
    }
    let coil = declared(Some(
        "http://example.org#Buildings.Controls.OBC.ASHRAE.G36.Types.CoolingCoil",
    ));
    assert!(
        hold_in_declared_type(&coil, Value::Integer(2))
            .unwrap()
            .bit_eq(&Value::Integer(2))
    );
    // A declared Integer or Boolean keeps its own kind untouched.
    assert!(
        hold_in_declared_type(&declared(Some("S231:Integer")), Value::Integer(i64::MAX))
            .unwrap()
            .bit_eq(&Value::Integer(i64::MAX))
    );
    assert!(
        hold_in_declared_type(&declared(Some("S231:Boolean")), Value::Boolean(false))
            .unwrap()
            .bit_eq(&Value::Boolean(false))
    );
}
