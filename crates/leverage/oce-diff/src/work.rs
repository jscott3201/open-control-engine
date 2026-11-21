//! A measure of the diff's work a test can count (diff note, "Test plan", hostile sizes).
//!
//! The diff promises a cost near-linear in the two models, and a rename pass whose cost does not
//! grow with the number of candidate pairs. Elapsed time cannot hold either promise in a test: it
//! measures the machine as well as the code. A count of what the code does can, because it is a
//! function of the inputs alone.
//!
//! Two things are counted, each as one unit, on a counter local to the working thread and
//! compiled into this crate's own test build only:
//!
//! - every ordering comparison of a key an index, a group or a sort of this crate is built over
//!   ([`Counted`], and [`compared`] for a sort with a comparator of its own);
//! - every element the diff visits outside a comparison: a statement lowered or indexed, an
//!   endpoint classified in a round of the rename pass ([`visited`]).
//!
//! In any other build [`Counted`] compiles to the inner comparison and the two functions to
//! nothing. What is not counted is bounded by the catalog or by a fixed table, never by the
//! models: the class and parameter lookups of `catalog`.

use std::cmp::Ordering;

/// A value compared as itself, each ordering comparison counted in test builds. Equality is the
/// inner value's and is not counted: `std`'s ordered collections and sorts use [`Ord::cmp`].
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

/// Count one comparison made outside [`Counted`]. Does nothing outside a test build.
pub(crate) fn compared() {
    #[cfg(test)]
    tests::add(1);
}

/// Sort `list`, each comparison counted.
pub(crate) fn sort_counted<T: Ord>(list: &mut [T]) {
    list.sort_by(|a, b| {
        compared();
        a.cmp(b)
    });
}

/// Count `elements` visited outside a comparison. Does nothing outside a test build.
pub(crate) fn visited(elements: usize) {
    #[cfg(test)]
    tests::add(elements);
    #[cfg(not(test))]
    let _ = elements;
}

#[cfg(test)]
#[path = "work_tests.rs"]
pub(crate) mod tests;
