//! A leaf's own declarations at the conditional-guard pass are held in its class's catalog types
//! (issue #759): what `evaluate_declarations` puts in the guard scope for a leaf chain, member by
//! member, beside the composite chain's declared types (issue #735). Whole-document behaviour
//! is pinned through the facade in `oce-api/tests/leaf_guard_types.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use oce_expr::EvalResult;
use oce_model::{Value, enum_class_id};

use crate::dto::Node;
use crate::scope_chain::ScopeFrame;

use super::declaration_scope::{Pass, evaluate_declarations};
use super::real_parameters::in_catalog_type;
use crate::parameter_reads::DocumentGroundings;

const TWO_POW_53: i64 = 1 << 53;

fn node(json: serde_json::Value) -> Node {
    serde_json::from_value(json).expect("test node parses")
}

/// The guard scope of a leaf `ex:M.blk` whose members are `members` (local name, JSON value),
/// evaluated at the specialize invocation with `leaf_class`.
fn scope(
    leaf_class: Option<&str>,
    members: &[(&str, serde_json::Value)],
) -> Vec<(Arc<str>, EvalResult)> {
    let nodes: Vec<Node> = members
        .iter()
        .map(|(name, value)| {
            node(serde_json::json!({ "@id": format!("ex:M.blk.{name}"), "S231:value": value }))
        })
        .collect();
    let leaf = node(serde_json::json!({
        "@id": "ex:M.blk",
        "S231:hasParameter": members
            .iter()
            .map(|(name, _)| serde_json::json!({ "@id": format!("ex:M.blk.{name}") }))
            .collect::<Vec<_>>(),
    }));
    let map: HashMap<&str, &Node> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    evaluate_declarations(
        &leaf,
        &[],
        &ScopeFrame::root(),
        &map,
        &mut DocumentGroundings::of(std::iter::empty(), &map),
        Pass::Specialize {
            composite_chain: false,
            leaf_class,
        },
    )
    .scope
    .flattened()
}

fn value(entries: &[(Arc<str>, EvalResult)], name: &str) -> Option<Value> {
    entries
        .iter()
        .find(|(entry, _)| entry.as_ref() == name)
        .map(|(_, result)| match result {
            EvalResult::Scalar(value) => value.clone(),
            other => panic!("{name} is not scalar: {other:?}"),
        })
}

fn assert_holds(entries: &[(Arc<str>, EvalResult)], name: &str, expected: &Value) {
    let actual = value(entries, name).unwrap_or_else(|| panic!("{name} did not ground"));
    assert!(
        actual.bit_eq(expected),
        "{name}: {actual:?} is not {expected:?}"
    );
}

const PID: &str = "CDL.Reals.PID";

#[test]
fn an_integer_on_a_real_member_is_the_real_and_a_sibling_reads_the_real() {
    for literal in [
        serde_json::json!(TWO_POW_53),
        serde_json::json!({ "@value": TWO_POW_53.to_string(), "@type": "http://www.w3.org/2001/XMLSchema#integer" }),
        serde_json::json!("9007199254740991 + 1"),
    ] {
        let entries = scope(
            Some(PID),
            &[
                ("yMax", literal.clone()),
                ("reverseActing", serde_json::json!("yMax + 1 > yMax")),
            ],
        );
        assert_holds(&entries, "yMax", &Value::Real(9_007_199_254_740_992.0));
        // The Real sum rounds back to 2^53: the sibling reads the Real, not the Integer.
        assert_holds(&entries, "reverseActing", &Value::Boolean(false));
        // Without the class (a composite chain's view), the member is the Integer as written.
        let as_written = scope(
            None,
            &[
                ("yMax", literal.clone()),
                ("reverseActing", serde_json::json!("yMax + 1 > yMax")),
            ],
        );
        assert_holds(&as_written, "yMax", &Value::Integer(TWO_POW_53));
        assert_holds(&as_written, "reverseActing", &Value::Boolean(true));
    }
}

#[test]
fn every_other_kind_and_an_integer_member_are_left_as_written() {
    let controller = enum_class_id("Buildings.Controls.OBC.CDL.Types.SimpleController")
        .expect("the CDL enumeration");
    let entries = scope(
        Some(PID),
        &[
            ("k", serde_json::json!(2.5)),
            ("Ti", serde_json::json!(-0.0)),
            ("reverseActing", serde_json::json!(true)),
            (
                "controllerType",
                serde_json::json!("Buildings.Controls.OBC.CDL.Types.SimpleController.PI"),
            ),
        ],
    );
    assert_holds(&entries, "k", &Value::Real(2.5));
    assert_holds(&entries, "Ti", &Value::Real(-0.0));
    assert_holds(&entries, "reverseActing", &Value::Boolean(true));
    assert!(matches!(
        value(&entries, "controllerType"),
        Some(Value::Enum { class, .. }) if class == controller
    ));
    // `RealVectorFilter.nin` is an Integer: it stays one, and a Real on it is not narrowed.
    let filter = "CDL.Routing.RealVectorFilter";
    assert_holds(
        &scope(Some(filter), &[("nin", serde_json::json!(2))]),
        "nin",
        &Value::Integer(2),
    );
    assert_holds(
        &scope(Some(filter), &[("nin", serde_json::json!(2.0))]),
        "nin",
        &Value::Real(2.0),
    );
    // Its Boolean mask elements are read as written, element by element.
    assert_holds(
        &scope(
            Some(filter),
            &[
                ("nin", serde_json::json!(2)),
                ("msk_1", serde_json::json!("nin > 1")),
            ],
        ),
        "msk_1",
        &Value::Boolean(true),
    );
}

#[test]
fn a_flattened_real_array_element_is_held_as_a_real() {
    // `TimeTable` cells are Real elements by their flattened names.
    let entries = scope(
        Some("CDL.Reals.Sources.TimeTable"),
        &[
            ("table_1_1", serde_json::json!(0)),
            ("table_1_2", serde_json::json!(TWO_POW_53)),
        ],
    );
    assert_holds(&entries, "table_1_1", &Value::Real(0.0));
    assert_holds(&entries, "table_1_2", &Value::Real(9_007_199_254_740_992.0));
}

#[test]
fn an_integer_past_two_to_the_fifty_third_stays_as_grounded_for_step_seven_to_refuse() {
    for integer in [TWO_POW_53 + 1, -TWO_POW_53 - 1, i64::MAX, i64::MIN] {
        let entries = scope(Some(PID), &[("yMax", serde_json::json!(integer))]);
        assert_holds(&entries, "yMax", &Value::Integer(integer));
    }
    // The boundary itself converts, on both signs.
    for integer in [TWO_POW_53, -TWO_POW_53] {
        let entries = scope(Some(PID), &[("yMax", serde_json::json!(integer))]);
        assert_holds(&entries, "yMax", &Value::Real(integer as f64));
    }
}

#[test]
fn an_unregistered_class_or_an_undeclared_name_types_nothing() {
    for (class, name) in [("CDL.Reals.NoSuchClass", "yMax"), (PID, "notAMember")] {
        let entries = scope(Some(class), &[(name, serde_json::json!(2))]);
        assert_holds(&entries, name, &Value::Integer(2));
    }
}

#[test]
fn the_catalog_rule_is_the_one_step_seven_applies() {
    // `in_catalog_type` is shared with `hold_in_catalog_type`; its whole table, kind by kind.
    let cases = [
        ("yMax", Value::Integer(2), Ok(Value::Real(2.0))),
        ("yMax", Value::Integer(TWO_POW_53 + 1), Err(TWO_POW_53 + 1)),
        ("yMax", Value::Real(f64::MAX), Ok(Value::Real(f64::MAX))),
        ("reverseActing", Value::Integer(1), Ok(Value::Integer(1))),
        (
            "reverseActing",
            Value::Boolean(false),
            Ok(Value::Boolean(false)),
        ),
        (
            "yMax",
            Value::String(Arc::from("2")),
            Ok(Value::String(Arc::from("2"))),
        ),
    ];
    for (name, given, expected) in cases {
        let actual = in_catalog_type(PID, name, given.clone());
        match (&actual, &expected) {
            (Ok(a), Ok(e)) => assert!(a.bit_eq(e), "{name} {given:?}: {a:?}"),
            (Err(a), Err(e)) => assert_eq!(a, e),
            _ => panic!("{name} {given:?}: {actual:?}, expected {expected:?}"),
        }
    }
}
