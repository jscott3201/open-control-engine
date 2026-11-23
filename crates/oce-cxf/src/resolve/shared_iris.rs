//! Looking a boundary IRI up once for each allocation that holds it, not once for each input that
//! holds it (issue #815).
//!
//! Every input a boundary input drives holds the boundary's IRI as one shared allocation, the
//! endpoint table's subject. Step 9 sorted those inputs with a key that hashed the IRI on every
//! comparison, and the declarations hashed it again for every input, so a boundary fanned out to
//! `n` inputs cost `n log n` or `n` hashes of its IRI. A view (the address and length of a string's
//! bytes) names the same text whenever it is equal, so each lookup below reads the text only the
//! first time it meets a view; an unequal view says nothing, and its text is read.

use std::collections::{HashMap, HashSet};

/// The address and length of a string's bytes: equal exactly when two `&str`s view the same
/// bytes, which implies equal text.
fn view(text: &str) -> (*const u8, usize) {
    (text.as_ptr(), text.len())
}

/// `@graph` positions by IRI, each looked up once per view.
pub(super) struct PositionsByView<'p, 'a> {
    by_text: &'p HashMap<&'a str, usize>,
    by_view: HashMap<(*const u8, usize), Option<usize>>,
    /// How many times a position was looked up by its text: once per distinct view.
    pub(super) text_lookups: usize,
}

impl<'p, 'a> PositionsByView<'p, 'a> {
    pub(super) fn new(by_text: &'p HashMap<&'a str, usize>) -> Self {
        Self {
            by_text,
            by_view: HashMap::new(),
            text_lookups: 0,
        }
    }

    /// The `@graph` position of `iri`, as `by_text` gives it.
    pub(super) fn get(&mut self, iri: &str) -> Option<usize> {
        if let Some(position) = self.by_view.get(&view(iri)) {
            return *position;
        }
        self.text_lookups += 1;
        let position = self.by_text.get(iri).copied();
        self.by_view.insert(view(iri), position);
        position
    }
}

/// The distinct texts of `iris`, each view's text hashed once, and how many texts were hashed.
pub(super) fn distinct_texts<'a>(iris: impl Iterator<Item = &'a str>) -> (HashSet<&'a str>, usize) {
    let mut seen: HashSet<(*const u8, usize)> = HashSet::new();
    let mut texts = HashSet::new();
    let mut hashed = 0;
    for iri in iris {
        if seen.insert(view(iri)) {
            hashed += 1;
            texts.insert(iri);
        }
    }
    (texts, hashed)
}

#[cfg(test)]
#[path = "shared_iris_tests.rs"]
mod tests;
