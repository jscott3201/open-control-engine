//! The `@id`s an export will emit, held in parts borrowed from the graph (issue #803).
//!
//! A parameter node's `@id` is its block's IRI, `.` and its name, and a port with no IRI of its
//! own is `<block IRI>.in<k>` or `<block IRI>.out<k>`. The plan held each as a `String` of its
//! own, the ports' several times (the block's port list, the duplicate set, the port, each
//! connection's target), so a block whose IRI is `L` bytes cost the plan `L` bytes for every
//! parameter and several for every such port before anything was written, whatever the graph
//! held: a hand-built graph shares one IRI among every parameter and port of its block.
//! [`EmittedId::Member`] and [`PortId::Minted`] hold the parts instead, a fixed size whatever `L`
//! is, and an [`EmittedId`] compares as the bytes it spells: `Member("urn:a", "k")` and
//! `Whole("urn:a.k")` are one `@id`, as the two `String`s were, so the duplicate check finds the
//! same repeats in the same order.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::fmt;

/// One `@id` the document will emit, compared as the bytes it spells.
#[derive(Debug)]
pub(super) enum EmittedId<'g> {
    /// An `@id` spelled whole: a block's IRI, a boundary node's.
    Whole(Cow<'g, str>),
    /// A parameter node's `@id`: the block's IRI, `.` and the parameter's name.
    Member(Cow<'g, str>, &'g str),
    /// A port's `@id`.
    Port(PortId<'g>),
}

/// A port node's `@id`: the connector's own IRI, or one minted from its block's IRI and its
/// position, spelled only where it is written.
#[derive(Clone, Debug)]
pub(super) enum PortId<'g> {
    /// The connector's own IRI, borrowed from the graph.
    Authored(&'g str),
    /// `<block>.in<k>` or `<block>.out<k>`.
    Minted {
        /// The block's IRI (owned only for an IRI-less block, which the plan refuses).
        block: Cow<'g, str>,
        /// Whether the port is an input (`.in`) or an output (`.out`).
        input: bool,
        /// The port's position in the block's list.
        k: u32,
    },
    /// A minted `@id` whose position does not fit a `u32`, spelled whole: a block with more than
    /// 2^32 ports, which no graph that fits in memory has. Kept so every id stays exact.
    Spelled(String),
}

/// Room for the decimal digits of any `u64`.
type Digits = [u8; 20];

/// The decimal digits of `k`, written into `buf`.
fn digits(mut k: u64, buf: &mut Digits) -> &str {
    let mut at = buf.len();
    loop {
        at -= 1;
        buf[at] = b'0' + (k % 10) as u8;
        k /= 10;
        if k == 0 {
            break;
        }
    }
    // ASCII digits only, so never the empty fallback.
    std::str::from_utf8(&buf[at..]).unwrap_or("")
}

impl<'g> PortId<'g> {
    /// The port of `block` at position `k`, an input when `input`.
    pub(super) fn minted(block: Cow<'g, str>, input: bool, k: usize) -> Self {
        match u32::try_from(k) {
            Ok(k) => PortId::Minted { block, input, k },
            Err(_) => PortId::Spelled(format!("{block}{}{k}", if input { ".in" } else { ".out" })),
        }
    }

    /// The `@id` in three parts, some of them empty, the digits spelled into `buf`.
    fn parts<'s>(&'s self, buf: &'s mut Digits) -> [&'s str; 3] {
        match self {
            PortId::Authored(id) => [id, "", ""],
            PortId::Spelled(id) => [id, "", ""],
            PortId::Minted { block, input, k } => [
                block,
                if *input { ".in" } else { ".out" },
                digits(u64::from(*k), buf),
            ],
        }
    }
}

impl fmt::Display for PortId<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = Digits::default();
        self.parts(&mut buf)
            .into_iter()
            .try_for_each(|part| f.write_str(part))
    }
}

impl EmittedId<'_> {
    /// The `@id` in three parts, some of them empty, a port's digits spelled into `buf`.
    fn parts<'s>(&'s self, buf: &'s mut Digits) -> [&'s str; 3] {
        match self {
            EmittedId::Whole(id) => [id, "", ""],
            EmittedId::Member(block, name) => [block, ".", name],
            EmittedId::Port(port) => port.parts(buf),
        }
    }
}

/// Compare two strings each given in parts, as their concatenations compare byte by byte, a
/// common run at a time.
fn compare_spelled(a: [&str; 3], b: [&str; 3]) -> Ordering {
    let (mut a_parts, mut b_parts) = (a.into_iter(), b.into_iter());
    let (mut a_rest, mut b_rest): (&[u8], &[u8]) = (b"", b"");
    loop {
        while a_rest.is_empty() {
            match a_parts.next() {
                Some(part) => a_rest = part.as_bytes(),
                None => break,
            }
        }
        while b_rest.is_empty() {
            match b_parts.next() {
                Some(part) => b_rest = part.as_bytes(),
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
        let (mut mine, mut theirs) = (Digits::default(), Digits::default());
        compare_spelled(self.parts(&mut mine), other.parts(&mut theirs))
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
