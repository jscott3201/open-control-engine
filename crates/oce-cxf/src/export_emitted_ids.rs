//! The `@id`s an export will emit, held to find two nodes that would share one (issue #803).
//!
//! A parameter node's `@id` is its block's IRI, `.` and its name. The set held each one as a
//! `String` of its own, so a block whose IRI is `L` bytes with `N` parameters cost the plan `N × L`
//! bytes before anything was written, whatever the graph held: a hand-built graph shares one IRI
//! among every parameter of its block. [`EmittedId::Member`] borrows the two parts from the graph
//! instead, so the set holds a fixed size for each parameter, and compares as the bytes the `@id`
//! spells: `Member("urn:a", "k")` and `Whole("urn:a.k")` are one `@id`, as the two `String`s
//! were, so the same repeats are found in the same order.

use std::borrow::Cow;
use std::cmp::Ordering;

/// One `@id` the document will emit, compared as the bytes it spells.
#[derive(Debug)]
pub(super) enum EmittedId<'g> {
    /// An `@id` spelled whole: a block's IRI, a port's, a boundary node's.
    Whole(Cow<'g, str>),
    /// A parameter node's `@id`: the block's IRI, `.` and the parameter's name.
    Member(Cow<'g, str>, &'g str),
}

impl EmittedId<'_> {
    /// The bytes the `@id` spells, in three parts, some of them empty.
    fn parts(&self) -> [&[u8]; 3] {
        match self {
            EmittedId::Whole(id) => [id.as_bytes(), b"", b""],
            EmittedId::Member(block, name) => [block.as_bytes(), b".", name.as_bytes()],
        }
    }
}

/// Compare two byte strings each given in parts, as their concatenations compare, a common run
/// at a time.
fn compare_spelled(a: [&[u8]; 3], b: [&[u8]; 3]) -> Ordering {
    let (mut a_parts, mut b_parts) = (a.into_iter(), b.into_iter());
    let (mut a_rest, mut b_rest): (&[u8], &[u8]) = (b"", b"");
    loop {
        while a_rest.is_empty() {
            match a_parts.next() {
                Some(part) => a_rest = part,
                None => break,
            }
        }
        while b_rest.is_empty() {
            match b_parts.next() {
                Some(part) => b_rest = part,
                None => break,
            }
        }
        if a_rest.is_empty() || b_rest.is_empty() {
            return a_rest.len().cmp(&b_rest.len());
        }
        let run = a_rest.len().min(b_rest.len());
        match a_rest[..run].cmp(&b_rest[..run]) {
            Ordering::Equal => {
                a_rest = &a_rest[run..];
                b_rest = &b_rest[run..];
            }
            unequal => return unequal,
        }
    }
}

impl Ord for EmittedId<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_spelled(self.parts(), other.parts())
    }
}

impl PartialOrd for EmittedId<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for EmittedId<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for EmittedId<'_> {}

#[cfg(test)]
#[path = "export_emitted_ids_tests.rs"]
mod tests;
