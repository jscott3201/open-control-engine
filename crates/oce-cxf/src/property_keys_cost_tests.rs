//! What normalizing a node's second spellings costs, counted in key comparisons and never in
//! elapsed time (issue #751; `TESTING.md`: a reader that allocates nothing is pinned by a count of
//! its work).
//!
//! Each alias of a node (a key `s<i>:min` through a `@context` term bound to the S231 namespace)
//! asks whether the node was also authored with its compact key. Each alias costs the document a
//! `@context` term and a key, about 55 bytes. Scanned per alias, `A` aliases on a node of `K`
//! keys cost `A × K` comparisons: 100,010,000 at 10,000 aliases. Sorted once per node, they cost
//! the sort and a binary search each.

use super::key_comparisons_counted_by;
use crate::{CxfError, ResolveOptions, import_cxf};
use oce_diag::{DiagCode, Diagnostic};

const MODEL: &str = include_str!("../tests/fixtures/minimal_loop.jsonld");

/// [`MODEL`] with `aliases` `@context` terms `s<i>` bound to the S231 namespace and one more node
/// `urn:x` carrying `"s<i>:min": 0` for each, and `extra` keys `x<j>` beside them.
fn aliased(aliases: usize, extra: usize) -> Vec<u8> {
    let context = "\"base\": \"http://example.org#\"";
    let last = "\"S231:isOfDataType\": { \"@id\": \"S231:Boolean\" } }\n  ]";
    assert_eq!(MODEL.matches(context).count(), 1);
    assert_eq!(MODEL.matches(last).count(), 1);
    let terms: String = (0..aliases)
        .map(|i| format!(", \"s{i}\": \"http://data.ashrae.org/S231#\""))
        .collect();
    let keys: String = (0..aliases)
        .map(|i| format!(", \"s{i}:min\": 0"))
        .chain((0..extra).map(|j| format!(", \"urn:x{j}\": 0")))
        .collect();
    MODEL
        .replacen(context, &format!("{context}{terms}"), 1)
        .replacen(
            last,
            &format!(
                "\"S231:isOfDataType\": {{ \"@id\": \"S231:Boolean\" }} }},\n    \
                 {{ \"@id\": \"urn:x\"{keys} }}\n  ]"
            ),
            1,
        )
        .into_bytes()
}

/// The comparisons one import of `bytes` makes, and its diagnostics.
fn counted(bytes: &[u8]) -> (u64, Vec<Diagnostic>) {
    let (comparisons, _, diagnostics) = counted_with_lookups(bytes);
    (comparisons, diagnostics)
}

/// The comparisons and the lookups one import of `bytes` makes, and its diagnostics.
fn counted_with_lookups(bytes: &[u8]) -> (u64, u64, Vec<Diagnostic>) {
    let (comparisons, lookups, result) =
        key_comparisons_counted_by(|| import_cxf(bytes, &ResolveOptions::default()));
    match result {
        Err(CxfError::Validation(diagnostics)) => (comparisons, lookups, diagnostics),
        Ok(_) => (comparisons, lookups, Vec::new()),
        Err(other) => panic!("not a validation refusal: {other}"),
    }
}

/// `ceil(log2(n))` for `n >= 1`.
fn log2_ceil(n: u64) -> u64 {
    u64::from(n.next_power_of_two().trailing_zeros())
}

/// The comparisons sorting `keys` keys and searching them once for each of `aliases` may make:
/// at most `2 × keys × ceil(log2 keys)` for the sort (the standard library's unstable sort makes
/// fewer than that on every input of these sizes) and `ceil(log2 keys) + 1` a search.
fn ceiling(keys: u64, aliases: u64) -> u64 {
    2 * keys * log2_ceil(keys) + aliases * (log2_ceil(keys) + 1)
}

#[test]
fn ten_thousand_second_spellings_on_one_node_cost_a_sort_and_a_search_each() {
    // The issue's document: 10,000 aliases of `min` on one node of 10,001 keys. Each alias after
    // the first is a second spelling of `min`, refused by name. [measured: 274,901 comparisons;
    // 100,010,000 when every alias scanned every key]
    let (comparisons, lookups, diagnostics) = counted_with_lookups(&aliased(10_000, 0));
    assert!(
        comparisons <= ceiling(10_001, 10_000),
        "{comparisons} comparisons for 10,000 aliases"
    );
    assert!(comparisons < 10_000 * 10_001 / 100);
    // Every alias is looked up in the set, once: a lookup the counter cannot see does not pass
    // for a cheap one.
    assert_eq!(lookups, 10_000);
    let seconds = diagnostics
        .iter()
        .filter(|d| d.code == DiagCode::MalformedDocument && d.message.contains("twice, as"))
        .count();
    assert_eq!(seconds, 9_999);
}

#[test]
fn comparisons_grow_as_keys_times_their_logarithm_from_one_to_four_times_the_aliases() {
    // At 2,500, 5,000 and 10,000 aliases each doubling costs at most 2.3 times the comparisons
    // (keys × log keys doubles to 2 × (log 2K / log K), about 2.17 here); a scan per alias
    // would cost four times. [measured: 59,048 / 127,418 / 274,901]
    let counts: Vec<u64> = [2_500, 5_000, 10_000]
        .into_iter()
        .map(|aliases| counted(&aliased(aliases, 0)).0)
        .collect();
    for pair in counts.windows(2) {
        assert!(
            pair[1] * 10 <= pair[0] * 23,
            "{counts:?}: doubling the aliases more than doubled the work"
        );
    }
}

#[test]
fn other_keys_beside_one_alias_are_sorted_once_and_searched_once() {
    // One alias among 10,000 other keys: one sort and one search, never a scan per key.
    // [measured: 124,209]
    let (comparisons, _) = counted(&aliased(1, 10_000));
    assert!(
        comparisons <= ceiling(10_002, 1),
        "{comparisons} comparisons"
    );
}

#[test]
fn a_node_without_aliases_sorts_nothing() {
    // No alias, no lookup: the keys are never sorted.
    let (comparisons, lookups, _) = counted_with_lookups(&aliased(0, 10_000));
    assert_eq!((comparisons, lookups), (0, 0));
    let (comparisons, lookups, _) = counted_with_lookups(MODEL.as_bytes());
    assert_eq!((comparisons, lookups), (0, 0));
}
