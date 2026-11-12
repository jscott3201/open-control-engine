//! Where each part of a parsed sequence was written, and how a validator finding is placed on
//! the text (the [`crate::Finding::span`] contract of GitHub #466).
//!
//! A finding names its subject by model path: `sequence`, `derived_from`, a declared name,
//! `block.param` or `connection[N]`. A path can mean several elements: a declaration may be named
//! like a header word (`in sequence : Real`, refused as `invalid-name` but still a declaration
//! with findings of its own, GitHub #585), and a name or an assignment may be repeated. So the
//! map does not choose by the path alone: each element is indexed by its subject and its
//! position in the authored model (the validator's `Anchor`, which every finding carries), and a
//! finding is placed on the element both name. The index is a `BTreeMap` built once per
//! [`SourceMap::apply`], keyed by [`Counted`] so the tests of `crate::counted` pin the comparisons
//! a placement makes.

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::counted::Counted;
use crate::location::Location;
use crate::model::Sequence;
use crate::validate::{Anchor, Finding};

/// The text location of every statement, assignment and connection of one parsed document, in
/// model order. Built by [`crate::parse_text_with_spans`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    pub(super) sequence: Location,
    pub(super) derived_from: Location,
    pub(super) params: Vec<Location>,
    pub(super) inputs: Vec<Location>,
    pub(super) outputs: Vec<Location>,
    pub(super) blocks: Vec<Location>,
    pub(super) assignments: Vec<Vec<Location>>,
    pub(super) connections: Vec<Location>,
}

impl SourceMap {
    pub(super) fn new(sequence: Location) -> Self {
        Self {
            sequence,
            derived_from: sequence,
            params: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            blocks: Vec::new(),
            assignments: Vec::new(),
            connections: Vec::new(),
        }
    }

    /// Every location `subject` can mean in `sequence`, in model order: the `sequence` keyword's
    /// name, the `derived_from` operand, a declaration's name (parameters, inputs, outputs,
    /// blocks), an assignment's name (an array element's value) or a connection's source. Empty
    /// when the subject names nothing in the map. Builds the index each call;
    /// [`SourceMap::apply`] builds it once for a whole finding list.
    #[must_use]
    pub fn locations(&self, sequence: &Sequence, subject: &str) -> Vec<Location> {
        Index::new(self, sequence).all(subject).collect()
    }

    /// Set [`Finding::span`] on each finding of `sequence` (the model this map was parsed with):
    /// the location of the element the validator reported it on, which its subject names. So a
    /// finding about a declaration named `sequence` or `derived_from` is placed on that
    /// declaration and a finding about the header on the header, and a `duplicate-name` or
    /// `duplicate-parameter` finding on the repeat it reports, never on the first declaration. A
    /// finding whose subject does not name that element (a caller rewrote the subject) is
    /// placed on the subject's location only when the subject names exactly one element; when
    /// it names several, the finding keeps `None` rather than a guess among them. A subject the
    /// map cannot place (an engine subject no statement names) keeps `None` too. The index is
    /// built once, so the cost is linear in the findings and the declarations, with one
    /// logarithmic lookup each.
    pub fn apply(&self, sequence: &Sequence, findings: &mut [Finding]) {
        let index = Index::new(self, sequence);
        for finding in findings.iter_mut() {
            finding.span = index.place(&finding.subject, finding.anchor);
        }
    }
}

/// The first anchor in model order, and the last: every key of one subject lies between them.
const FIRST: Anchor = Anchor::Sequence;
const LAST: Anchor = Anchor::Connection(usize::MAX);

/// (subject, anchor) → location, built once per lookup batch. One subject's entries are adjacent
/// and in anchor order, which is model order: the header (`sequence`, `derived_from`),
/// parameters, inputs, outputs, each block before its assignments, then connections.
struct Index<'a> {
    elements: BTreeMap<Counted<(Cow<'a, str>, Anchor)>, Location>,
}

impl<'a> Index<'a> {
    fn new(map: &SourceMap, sequence: &'a Sequence) -> Self {
        let mut elements: BTreeMap<Counted<(Cow<'a, str>, Anchor)>, Location> = BTreeMap::new();
        let mut add = |subject: Cow<'a, str>, anchor: Anchor, at: Location| {
            elements.insert(Counted((subject, anchor)), at);
        };
        add(Cow::Borrowed("sequence"), Anchor::Sequence, map.sequence);
        add(
            Cow::Borrowed("derived_from"),
            Anchor::DerivedFrom,
            map.derived_from,
        );
        for (i, at) in map.connections.iter().enumerate() {
            add(
                Cow::Owned(format!("connection[{i}]")),
                Anchor::Connection(i),
                *at,
            );
        }
        let params = sequence.params.iter().map(|p| &p.name).zip(&map.params);
        let inputs = sequence.inputs.iter().map(|b| &b.name).zip(&map.inputs);
        let outputs = sequence.outputs.iter().map(|b| &b.name).zip(&map.outputs);
        let blocks = sequence.blocks.iter().map(|b| &b.name).zip(&map.blocks);
        let declared = params
            .enumerate()
            .map(|(i, d)| (Anchor::Param(i), d))
            .chain(inputs.enumerate().map(|(i, d)| (Anchor::Input(i), d)))
            .chain(outputs.enumerate().map(|(i, d)| (Anchor::Output(i), d)))
            .chain(blocks.enumerate().map(|(i, d)| (Anchor::Block(i, None), d)));
        for (anchor, (name, at)) in declared {
            add(Cow::Borrowed(name.as_str()), anchor, *at);
        }
        for (i, (block, spans)) in sequence.blocks.iter().zip(&map.assignments).enumerate() {
            for (j, (a, at)) in block.assignments.iter().zip(spans).enumerate() {
                add(
                    Cow::Owned(format!("{}.{}", block.name, a.name)),
                    Anchor::Block(i, Some(j)),
                    *at,
                );
            }
        }
        Self { elements }
    }

    /// Every location of `subject`, in model order.
    fn all<'s>(&'s self, subject: &'s str) -> impl Iterator<Item = Location> + 's {
        let key = |anchor| Counted((Cow::Borrowed(subject), anchor));
        self.elements
            .range(key(FIRST)..=key(LAST))
            .map(|(_, at)| *at)
    }

    /// The location of the element `subject` and `anchor` both name; else the subject's
    /// location when it names exactly one element, and `None` when it names several, because
    /// choosing one of them would guess.
    fn place(&self, subject: &str, anchor: Anchor) -> Option<Location> {
        if let Some(at) = self
            .elements
            .get(&Counted((Cow::Borrowed(subject), anchor)))
        {
            return Some(*at);
        }
        let mut named = self.all(subject);
        match (named.next(), named.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    }
}
