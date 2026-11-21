//! Pairing the elements of two authored lists by name, and telling whether the pairs moved.
//!
//! A list (the parameters, the inputs, the blocks, a block's assignments, the connections) is
//! matched by key. A key that occurs more than once on a side, which the validator refuses and
//! the diff must still survive, is matched occurrence by occurrence: the first with the first,
//! the second with the second. So a model diffed against itself pairs every element with itself,
//! whatever it holds.

use std::collections::BTreeMap;

use crate::work::{Counted, visited};

/// How two lists pair up, by position in each list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pairing {
    /// `(base, head)` positions of the elements on both sides, in base order.
    pub(crate) both: Vec<(usize, usize)>,
    /// Positions of the elements only base has, in base order.
    pub(crate) base_only: Vec<usize>,
    /// Positions of the elements only head has, in head order.
    pub(crate) head_only: Vec<usize>,
}

/// Pair two lists of keys, occurrence by occurrence.
pub(crate) fn pair<K: Ord + Clone>(base: &[K], head: &[K]) -> Pairing {
    visited(base.len() + head.len());
    let mut positions: BTreeMap<Counted<(K, usize)>, usize> = BTreeMap::new();
    for (position, key) in occurrences(head).into_iter().enumerate() {
        positions.insert(Counted(key), position);
    }
    let mut taken = vec![false; head.len()];
    let mut pairing = Pairing::default();
    for (position, key) in occurrences(base).into_iter().enumerate() {
        match positions.get(&Counted(key)) {
            Some(&partner) => {
                if let Some(slot) = taken.get_mut(partner) {
                    *slot = true;
                }
                pairing.both.push((position, partner));
            }
            None => pairing.base_only.push(position),
        }
    }
    pairing.head_only = taken
        .iter()
        .enumerate()
        .filter(|&(_, &taken)| !taken)
        .map(|(position, _)| position)
        .collect();
    pairing
}

/// Each key with its occurrence number among equal keys before it: the first `a` is `(a, 0)`,
/// the second `(a, 1)`.
fn occurrences<K: Ord + Clone>(keys: &[K]) -> Vec<(K, usize)> {
    let mut seen: BTreeMap<Counted<K>, usize> = BTreeMap::new();
    keys.iter()
        .map(|key| {
            let count = seen.entry(Counted(key.clone())).or_insert(0);
            let occurrence = *count;
            *count += 1;
            (key.clone(), occurrence)
        })
        .collect()
}

/// Whether the given `(base, head)` pairs, taken in base order, are in another order in head.
/// The elements on one side only are never looked at, so adding or removing an element is not a
/// reordering.
pub(crate) fn reordered(pairs: &[(usize, usize)]) -> bool {
    pairs.windows(2).any(|pair| match pair {
        [(_, first), (_, second)] => first > second,
        _ => false,
    })
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
mod tests;
