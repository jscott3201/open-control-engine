//! Which effective edit lists are one: the same canonical paths in the same order with values of
//! the same kind and bits. Nothing is compared after resolution but that.

use super::distinct;
use crate::bind::BoundScenario;
use crate::receipt::ReceiptValue;

fn scenario(edits: &[(&str, ReceiptValue)]) -> BoundScenario {
    BoundScenario {
        steps: Vec::new(),
        edits: edits
            .iter()
            .map(|(path, value)| ((*path).to_owned(), *value))
            .collect(),
        expectations: Vec::new(),
    }
}

/// The first scenario of each distinct list, in order.
fn firsts(scenarios: &[BoundScenario]) -> Vec<usize> {
    distinct(scenarios).iter().map(|list| list.first).collect()
}

#[test]
fn a_list_repeated_anywhere_is_kept_once_at_its_first_scenario() {
    use ReceiptValue::Real;
    let a = [("p", Real(1.0))];
    let b = [("p", Real(2.0))];
    let scenarios = [
        scenario(&[]),
        scenario(&a),
        scenario(&b),
        scenario(&a),
        scenario(&[]),
        scenario(&b),
    ];
    assert_eq!(firsts(&scenarios), [1, 2]);
    let lists = distinct(&scenarios);
    assert!(lists[0].edits[0].1.bit_eq(&Real(1.0)));
    // A set without edits has no list, so binding loads nothing to try one.
    assert!(distinct(&[scenario(&[]), scenario(&[])]).is_empty());
}

#[test]
fn values_are_one_only_with_the_same_kind_and_bits() {
    use ReceiptValue::{Boolean, Integer, Real};
    let scenarios = [
        scenario(&[("p", Real(1.0))]),
        scenario(&[("p", Integer(1))]),
        scenario(&[("p", Real(0.0))]),
        scenario(&[("p", Real(-0.0))]),
        scenario(&[("p", Boolean(true))]),
        scenario(&[("p", Integer(-1))]),
        scenario(&[("p", Integer(i64::MAX))]),
        scenario(&[("p", Integer(-1))]),
        scenario(&[("p", Real(-0.0))]),
    ];
    assert_eq!(firsts(&scenarios), [0, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn paths_and_their_order_belong_to_the_list() {
    use ReceiptValue::Real;
    let scenarios = [
        scenario(&[("a", Real(1.0)), ("b", Real(2.0))]),
        scenario(&[("a", Real(1.0))]),
        scenario(&[("a", Real(1.0)), ("c", Real(2.0))]),
        scenario(&[("a", Real(1.0)), ("b", Real(2.0))]),
        scenario(&[("b", Real(2.0))]),
    ];
    assert_eq!(firsts(&scenarios), [0, 1, 2, 4]);
}
