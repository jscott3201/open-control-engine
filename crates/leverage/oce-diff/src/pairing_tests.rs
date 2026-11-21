//! Pairing by name and occurrence, and what counts as a reordering.

use super::{Pairing, pair, reordered};

fn paired(base: &[&str], head: &[&str]) -> Pairing {
    pair(base, head)
}

#[test]
fn a_list_pairs_with_itself_element_by_element_whatever_it_repeats() {
    for list in [
        &[][..],
        &["a"][..],
        &["a", "b", "c"][..],
        &["a", "a", "a"][..],
        &["a", "b", "a", "", "b", ""][..],
    ] {
        let pairing = paired(list, list);
        let identity: Vec<(usize, usize)> = (0..list.len()).map(|at| (at, at)).collect();
        assert_eq!(pairing.both, identity, "{list:?}");
        assert!(pairing.base_only.is_empty() && pairing.head_only.is_empty());
        assert!(!reordered(&pairing.both));
    }
}

#[test]
fn elements_pair_by_name_wherever_they_stand() {
    let pairing = paired(&["a", "b", "c"], &["c", "x", "a"]);
    assert_eq!(pairing.both, [(0, 2), (2, 0)]);
    assert_eq!(pairing.base_only, [1]);
    assert_eq!(pairing.head_only, [1]);
}

#[test]
fn a_repeated_name_pairs_occurrence_by_occurrence() {
    // The first `a` with the first, the second with the second; the third has no partner.
    let pairing = paired(&["a", "b", "a", "a"], &["a", "a", "b"]);
    assert_eq!(pairing.both, [(0, 0), (1, 2), (2, 1)]);
    assert_eq!(pairing.base_only, [3]);
    assert!(pairing.head_only.is_empty());

    let pairing = paired(&["a"], &["a", "a", "a"]);
    assert_eq!(pairing.both, [(0, 0)]);
    assert_eq!(pairing.head_only, [1, 2]);
}

#[test]
fn an_empty_side_pairs_nothing() {
    let pairing = paired(&[], &["a", "b"]);
    assert!(pairing.both.is_empty() && pairing.base_only.is_empty());
    assert_eq!(pairing.head_only, [0, 1]);
    let pairing = paired(&["a", "b"], &[]);
    assert_eq!(pairing.base_only, [0, 1]);
    assert!(pairing.head_only.is_empty());
}

#[test]
fn only_a_changed_relative_order_of_shared_elements_is_a_reordering() {
    let moved = |base: &[&str], head: &[&str]| reordered(&paired(base, head).both);
    assert!(!moved(&["a", "b", "c"], &["a", "b", "c"]));
    assert!(moved(&["a", "b", "c"], &["a", "c", "b"]));
    assert!(moved(&["a", "b", "c"], &["c", "a", "b"]));
    assert!(moved(&["a", "b"], &["b", "a"]));
    // Inserting, removing and replacing leave the shared elements in order.
    assert!(!moved(&["a", "b", "c"], &["x", "a", "y", "b", "z", "c"]));
    assert!(!moved(&["a", "b", "c"], &["a", "c"]));
    assert!(!moved(&["a", "b", "c"], &["a", "x", "c"]));
    assert!(!moved(&["a"], &["b"]));
    assert!(!moved(&[], &[]));
    // Two equal names that swap are the same list.
    assert!(!moved(&["a", "a"], &["a", "a"]));
    // A removal next to a move is still a move.
    assert!(moved(&["a", "b", "c", "d"], &["d", "a", "c"]));
}
