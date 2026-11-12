//! Which effective edit lists are one: the same parameters in the same order with values of the
//! same kind and bits, the set's edits shared and a scenario's own winning where both edit one
//! parameter. Nothing is compared after resolution but that.

use super::{Edit, Edits, distinct};
use crate::bind::BoundScenario;
use crate::receipt::ReceiptValue::{self, Boolean, Integer, Real};

fn scenario(own: &[Edit]) -> BoundScenario {
    BoundScenario {
        steps: Vec::new(),
        edits: own.to_vec(),
        expectations: Vec::new(),
    }
}

fn edits(set: &[Edit]) -> Edits {
    Edits {
        paths: ["a", "b", "c"].map(str::to_owned).to_vec(),
        set: set.to_vec(),
    }
}

/// The first scenario of each distinct list, in order.
fn firsts(edits: &Edits, scenarios: &[BoundScenario]) -> Vec<usize> {
    distinct(edits, scenarios)
        .resolved()
        .map(|(first, _)| first)
        .collect()
}

fn bits(list: &[(&str, ReceiptValue)]) -> Vec<(String, u64)> {
    list.iter()
        .map(|(path, value)| match value {
            Real(x) => ((*path).to_owned(), x.to_bits()),
            other => panic!("{other:?}"),
        })
        .collect()
}

#[test]
fn a_list_repeated_anywhere_is_kept_once_at_its_first_scenario() {
    let none = edits(&[]);
    let a = [(0, Real(1.0))];
    let b = [(0, Real(2.0))];
    let scenarios = [
        scenario(&[]),
        scenario(&a),
        scenario(&b),
        scenario(&a),
        scenario(&[]),
        scenario(&b),
    ];
    assert_eq!(firsts(&none, &scenarios), [1, 2]);
    assert_eq!(distinct(&none, &scenarios).len(), 2);
    // A set without edits has no list, so binding loads nothing to try one.
    assert_eq!(distinct(&none, &[scenario(&[]), scenario(&[])]).len(), 0);
}

#[test]
fn values_are_one_only_with_the_same_kind_and_bits() {
    let none = edits(&[]);
    let scenarios = [
        scenario(&[(0, Real(1.0))]),
        scenario(&[(0, Integer(1))]),
        scenario(&[(0, Real(0.0))]),
        scenario(&[(0, Real(-0.0))]),
        scenario(&[(0, Boolean(true))]),
        scenario(&[(0, Integer(-1))]),
        scenario(&[(0, Integer(i64::MAX))]),
        scenario(&[(0, Integer(-1))]),
        scenario(&[(0, Real(-0.0))]),
    ];
    assert_eq!(firsts(&none, &scenarios), [0, 1, 2, 3, 4, 5, 6]);
}

#[test]
fn parameters_and_their_order_belong_to_the_list() {
    let none = edits(&[]);
    let scenarios = [
        scenario(&[(0, Real(1.0)), (1, Real(2.0))]),
        scenario(&[(0, Real(1.0))]),
        scenario(&[(0, Real(1.0)), (2, Real(2.0))]),
        scenario(&[(0, Real(1.0)), (1, Real(2.0))]),
        scenario(&[(1, Real(2.0))]),
    ];
    assert_eq!(firsts(&none, &scenarios), [0, 1, 2, 4]);
}

#[test]
fn a_scenarios_own_edit_wins_over_the_sets_and_the_merge_is_the_effective_list() {
    let shared = edits(&[(0, Real(1.0)), (2, Real(3.0))]);
    let scenarios = [
        // Nothing of its own: the set's list.
        scenario(&[]),
        // Its own value for `c`, and one for `b` the set leaves alone.
        scenario(&[(1, Real(2.0)), (2, Real(9.0))]),
        // The set's own value for `a`, written again: the same effective list as the first.
        scenario(&[(0, Real(1.0))]),
    ];
    assert_eq!(
        bits(&shared.resolved(&scenarios[1].edits)),
        [
            ("a".to_owned(), 1.0_f64.to_bits()),
            ("b".to_owned(), 2.0_f64.to_bits()),
            ("c".to_owned(), 9.0_f64.to_bits())
        ]
    );
    assert_eq!(
        bits(&shared.resolved(&scenarios[0].edits)),
        [
            ("a".to_owned(), 1.0_f64.to_bits()),
            ("c".to_owned(), 3.0_f64.to_bits())
        ]
    );
    // Effective lists are compared, not the scenarios' own maps: the third is the first's.
    assert_eq!(firsts(&shared, &scenarios), [0, 1]);
    // With set-level edits, a scenario with none of its own still has a list to try.
    assert!(!shared.none(&scenarios[0].edits));
    assert!(edits(&[]).none(&scenarios[0].edits));
}
