//! The one name rule over a path list that resolves many names (issue #571).
//!
//! [`resolve_path`](crate::resolve_path) looks one name up by scanning every path for a suffix match, which is right
//! for one name and costs names times paths for many. [`PathIndex`] answers the same question for
//! every name after one sort of the list: it orders the paths by their bytes read backwards, so
//! the paths that end with a given name followed by a `.` boundary are one contiguous run, found
//! by binary search. A name then costs `O(log n)` comparisons of at most its own length plus one
//! byte, and the run holds only real matches, so a shared last segment (`uLow` in every block)
//! costs nothing extra.
//!
//! Every answer is [`resolve_path`](crate::resolve_path)'s, byte for byte: the same exact match by binary search over
//! the list as given, the same suffix rule, and the candidates of an ambiguous name in list
//! order. `tests` compares the two over a generated corpus.

use std::cmp::Ordering;

#[cfg(test)]
use crate::map::resolve_path;

/// A path list indexed for the one name rule ([`resolve_path`](crate::resolve_path)), for resolving many names.
///
/// Built once, in `O(n log n)` comparisons over `n` paths, it holds the list and one index per
/// path. [`PathIndex::resolve`] then answers each name as [`resolve_path`](crate::resolve_path) would, without
/// scanning the list.
#[derive(Clone, Debug)]
pub struct PathIndex<'p> {
    /// The paths, in the order given.
    paths: Vec<&'p str>,
    /// The positions of `paths`, ordered by each path's bytes read from the end.
    by_reversed: Vec<usize>,
}

/// Count one path comparison, for the tests that pin how many a lookup makes.
fn counted() {
    #[cfg(test)]
    tests::COMPARISONS.with(|count| count.set(count.get() + 1));
}

/// `path` read from its last byte, against `key`, as byte strings.
fn reversed_cmp(path: &str, key: &[u8]) -> Ordering {
    counted();
    path.bytes().rev().cmp(key.iter().copied())
}

/// Whether `path` read from its last byte starts with `key`.
fn reversed_starts_with(path: &str, key: &[u8]) -> bool {
    counted();
    path.len() >= key.len() && path.bytes().rev().zip(key).all(|(a, b)| a == *b)
}

impl<'p> PathIndex<'p> {
    /// Index `paths`, a list of canonical paths sorted ascending by their UTF-8 bytes with no
    /// path repeated, as [`resolve_path`](crate::resolve_path) takes it. Out of that order, exact matches can be
    /// missed exactly as [`resolve_path`](crate::resolve_path) misses them.
    #[must_use]
    pub fn new<P: AsRef<str>>(paths: &'p [P]) -> Self {
        let paths: Vec<&'p str> = paths.iter().map(AsRef::as_ref).collect();
        let mut by_reversed: Vec<usize> = (0..paths.len()).collect();
        // Stable, so paths equal backwards (a repeated path) keep their list order.
        by_reversed.sort_by(|&a, &b| {
            counted();
            paths[a].bytes().rev().cmp(paths[b].bytes().rev())
        });
        Self { paths, by_reversed }
    }

    /// The index of `name` in the list, exactly as [`resolve_path`](crate::resolve_path) gives it.
    ///
    /// Pure, and never panics.
    ///
    /// # Errors
    /// As [`resolve_path`](crate::resolve_path): every path `name` matches by suffix, in list order, when there is no
    /// exact match and more than one suffix match; an empty list when nothing matches.
    ///
    /// ```
    /// use oce_replay::{PathIndex, resolve_path};
    ///
    /// let paths = ["urn:x#top.ahu.T", "urn:x#top.ahu.uDam", "urn:x#top.zone.T"];
    /// let index = PathIndex::new(&paths);
    /// for name in ["urn:x#top.zone.T", "uDam", "ahu.uDam", "T", "Dam", "top.ahu.uDam", ""] {
    ///     assert_eq!(index.resolve(name), resolve_path(&paths, name));
    /// }
    /// ```
    pub fn resolve(&self, name: &str) -> Result<usize, Vec<String>> {
        if let Ok(index) = self.paths.binary_search_by(|path| {
            counted();
            path.cmp(&name)
        }) {
            return Ok(index);
        }
        if name.is_empty() {
            return Err(Vec::new());
        }
        // A path ends with `name` at a `.` boundary exactly when, read backwards, it starts with
        // `name` read backwards and then a `.`; those paths sit together in `by_reversed`.
        let mut key: Vec<u8> = name.bytes().rev().collect();
        key.push(b'.');
        let first = self
            .by_reversed
            .partition_point(|&index| reversed_cmp(self.paths[index], &key) == Ordering::Less);
        let mut matches: Vec<usize> = self.by_reversed[first..]
            .iter()
            .copied()
            .take_while(|&index| reversed_starts_with(self.paths[index], &key))
            .collect();
        matches.sort_unstable();
        match matches.as_slice() {
            [index] => Ok(*index),
            _ => Err(matches
                .iter()
                .map(|&index| self.paths[index].to_owned())
                .collect()),
        }
    }

    /// `resolve_path` over the same list, for the equivalence tests.
    #[cfg(test)]
    fn linear(&self, name: &str) -> Result<usize, Vec<String>> {
        resolve_path(&self.paths, name)
    }
}

#[cfg(test)]
#[path = "path_index_tests.rs"]
mod tests;
