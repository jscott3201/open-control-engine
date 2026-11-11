//! Index keys whose comparisons a test can count (issue #478).
//!
//! Validation promises a cost near-linear in the document: every name is looked up in an ordered
//! index built once, never by a scan of the declarations. Elapsed time cannot hold that promise
//! in a test, because it measures the machine as well as the code. The number of key comparisons
//! can: it is a function of the input alone, and a scan per lookup multiplies it by the size of
//! the document.
//!
//! [`Counted`] is the key of every index stage 1 and the text placement build over the document:
//! the declared names, a block's assignments, a block's resolved ports, the driven targets and the
//! finding subjects. It orders exactly as the value it wraps, so an index keyed by it holds and
//! finds the same entries in the same order. In a test build each comparison also adds one to a
//! counter local to the comparing thread; in any other build the wrapper compiles to the inner
//! comparison. A sort with a comparator of its own reports each comparison through [`compared`].
//!
//! Some lookups of stage 1 do not go through it and are not counted. Each is bounded by the
//! catalog or by a fixed table, not by the document: the class index and the scan of one class's
//! parameters (`validate::catalog`), the scan of the named types (`validate::members`), the
//! membership test against `STAGE_ONE_CODES`, whatever `oce_api::class_ports` does to resolve
//! one block, and a unit suffix's checks (`validate::suffixes`, issue #501): the declared-unit
//! index over the embedded vocabulary, keyed by catalog class path and parameter name, the scan
//! of the three-row `oce_vocab::SCALE_FACTORS` and of `BASE_UNIT_SYMBOLS`, and the agreement of
//! one assignment's suffix, written number and value, each a lookup per suffixed assignment
//! bounded by the catalog or a fixed table and a suffix of at most 256 bytes. The removal of adjacent equal findings after the sort compares each neighbouring
//! pair once with `==`, which is linear and not counted either. A lookup over the document that
//! compares names with a plain `==` would be invisible in the same way, so a new one is keyed by
//! [`Counted`].

use std::cmp::Ordering;

/// A value compared as itself, each ordering comparison counted in test builds.
///
/// Equality (`==`) is the inner value's and is not counted: the ordered collections of `std`
/// place and find a key by [`Ord::cmp`] alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Counted<T>(pub(crate) T);

impl<T: Ord> Ord for Counted<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        compared();
        self.0.cmp(&other.0)
    }
}

impl<T: Ord> PartialOrd for Counted<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Count one comparison made outside [`Counted`] (a sort's own comparator). Does nothing outside
/// a test build.
pub(crate) fn compared() {
    #[cfg(test)]
    tests::COMPARISONS.with(|count| count.set(count.get() + 1));
}

#[cfg(test)]
#[path = "counted_tests.rs"]
mod tests;
