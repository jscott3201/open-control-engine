//! An Integer bound to a catalog-Real parameter is held as a Real, exactly or not at all
//! (issue #683).
//!
//! These tests drive the table pass alone, on tables built by hand, so every case states the
//! parameter's class, name and grounded value. `tests/real_parameter_literals.rs` runs the same
//! rule through documents.

use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic, Severity};
use oce_model::{EnumClassId, Value};

use crate::diagnostic_list::DiagnosticList;

use super::real_parameters::{
    MAX_EXACT_INTEGER_AS_REAL, exact_real, hold_real_parameters_as_reals,
};

const TWO_POW_53: i64 = 1 << 53;
const OWNER: &str = "urn:test#Seq.blk";

type Table = Vec<(Arc<str>, Value)>;

fn table(entries: &[(&str, Value)]) -> Table {
    entries
        .iter()
        .map(|(name, value)| (Arc::from(*name), value.clone()))
        .collect()
}

/// Run the pass with one source IRI per entry, `urn:test#Seq.blk.<name>`.
fn held(class_path: &str, entries: &[(&str, Value)]) -> (Table, Vec<Diagnostic>) {
    let mut table = table(entries);
    let sources: Vec<String> = entries
        .iter()
        .map(|(name, _)| format!("{OWNER}.{name}"))
        .collect();
    let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
    let mut diags = DiagnosticList::new();
    hold_real_parameters_as_reals(class_path, OWNER, &mut table, &sources, &mut diags);
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
fn the_exact_bound_is_two_to_the_fifty_third() {
    assert_eq!(MAX_EXACT_INTEGER_AS_REAL, 9_007_199_254_740_992);
    assert_eq!(MAX_EXACT_INTEGER_AS_REAL, TWO_POW_53.unsigned_abs());
}

#[test]
fn every_integer_up_to_the_bound_converts_to_the_real_of_the_same_value() {
    for integer in [
        0,
        1,
        -1,
        2,
        i64::from(i32::MAX),
        i64::from(i32::MIN),
        TWO_POW_53 - 1,
        -(TWO_POW_53 - 1),
        TWO_POW_53,
        -TWO_POW_53,
    ] {
        let real = exact_real(integer).expect("within the bound");
        // Exact both ways: the Real is integral and reads back as the Integer it came from.
        assert_eq!(real as i64, integer, "{integer}");
        assert_eq!(real.to_bits(), (integer as f64).to_bits(), "{integer}");
    }
    assert_eq!(
        exact_real(TWO_POW_53).unwrap().to_bits(),
        0x4340_0000_0000_0000
    );
    assert_eq!(
        exact_real(-TWO_POW_53).unwrap().to_bits(),
        0xc340_0000_0000_0000
    );
}

#[test]
fn integer_zero_converts_to_positive_zero() {
    // No Integer spells a negative zero: `-0` parses to the one Integer zero.
    let zero = exact_real(0).unwrap();
    assert_eq!(zero.to_bits(), 0.0f64.to_bits());
    assert_ne!(zero.to_bits(), (-0.0f64).to_bits());
}

#[test]
fn every_integer_past_the_bound_is_refused_including_the_exact_powers_of_two() {
    for integer in [
        TWO_POW_53 + 1,
        -(TWO_POW_53 + 1),
        TWO_POW_53 + 2,
        1 << 54,
        1 << 62,
        i64::MAX,
        i64::MIN + 1,
        // -2^63 is exact as a binary64, and has no `i64` magnitude: refused by its magnitude.
        i64::MIN,
    ] {
        assert_eq!(exact_real(integer), None, "{integer}");
    }
    // What a cast would have done at the first refused value: land on its neighbour.
    assert_eq!((TWO_POW_53 + 1) as f64 as i64, TWO_POW_53);
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
fn a_refusal_without_a_recorded_source_is_located_on_the_owning_instance() {
    let mut table = table(&[("uLow", Value::Integer(i64::MAX))]);
    let mut diags = DiagnosticList::new();
    hold_real_parameters_as_reals("CDL.Reals.Hysteresis", OWNER, &mut table, &[], &mut diags);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].subject.as_deref(), Some(OWNER));
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
