//! Where each part of a parsed sequence was written, and how a validator finding is placed on
//! the text (the [`crate::Finding::span`] contract of GitHub #466).
//!
//! A finding names its subject by model path: `sequence`, `derived_from`, a declared name,
//! `block.param` or `connection[N]`. The map resolves the path to the model elements it can mean,
//! in authored model order (parameters, inputs, outputs, blocks), and takes the first. A duplicate
//! is the one case where a path means several elements: the validator reports the k-th
//! `duplicate-name` or `duplicate-parameter` of a subject at its (k+1)-th declaration in model
//! section order, so the map does the same. The subject index is a `BTreeMap` built once per
//! [`SourceMap::apply`].

use std::collections::BTreeMap;

use crate::location::Location;
use crate::model::Sequence;
use crate::validate::{DUPLICATE_NAME, DUPLICATE_PARAMETER, Finding};

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
    /// name, the `derived_from` operand, a declaration's name, an assignment's name (an array
    /// element's value) or a connection's source. Empty when the subject names nothing in the map.
    /// Builds the subject index each call; [`SourceMap::apply`] builds it once for a whole finding
    /// list.
    #[must_use]
    pub fn locations(&self, sequence: &Sequence, subject: &str) -> Vec<Location> {
        Index::new(self, sequence).lookup(subject).to_vec()
    }

    /// Set [`Finding::span`] on each finding of `sequence` (the model this map was parsed with):
    /// the first location of its subject, or, for the k-th `duplicate-name` /
    /// `duplicate-parameter` finding of a subject, its (k+1)-th declaration in model section
    /// order (parameters, inputs, outputs, blocks) — the later declaration, as the validator
    /// reports it. A subject the map cannot place (an engine subject no statement names) keeps
    /// `None`. The subject index is built once, so the cost is linear in the findings and the
    /// declarations.
    pub fn apply(&self, sequence: &Sequence, findings: &mut [Finding]) {
        let index = Index::new(self, sequence);
        let mut repeats: BTreeMap<(&'static str, String), usize> = BTreeMap::new();
        for finding in findings.iter_mut() {
            let at = index.lookup(&finding.subject);
            let position = if finding.code == DUPLICATE_NAME || finding.code == DUPLICATE_PARAMETER
            {
                let seen = repeats
                    .entry((finding.code, finding.subject.clone()))
                    .or_insert(0);
                *seen += 1;
                (*seen).min(at.len().saturating_sub(1))
            } else {
                0
            };
            finding.span = at.get(position).copied();
        }
    }
}

/// Subject → locations, in model order, built once per lookup batch: the fixed subjects
/// (`sequence`, `derived_from`, `connection[N]`) first, then declarations, then `block.param`.
struct Index {
    subjects: BTreeMap<String, Vec<Location>>,
}

impl Index {
    fn new(map: &SourceMap, sequence: &Sequence) -> Self {
        let mut subjects: BTreeMap<String, Vec<Location>> = BTreeMap::new();
        let mut add = |subject: String, at: Location| subjects.entry(subject).or_default().push(at);
        add("sequence".to_owned(), map.sequence);
        add("derived_from".to_owned(), map.derived_from);
        for (i, at) in map.connections.iter().enumerate() {
            add(format!("connection[{i}]"), *at);
        }
        let declared = sequence
            .params
            .iter()
            .map(|p| &p.name)
            .zip(&map.params)
            .chain(sequence.inputs.iter().map(|b| &b.name).zip(&map.inputs))
            .chain(sequence.outputs.iter().map(|b| &b.name).zip(&map.outputs))
            .chain(sequence.blocks.iter().map(|b| &b.name).zip(&map.blocks));
        for (name, at) in declared {
            add(name.as_str().to_owned(), *at);
        }
        for (block, spans) in sequence.blocks.iter().zip(&map.assignments) {
            for (a, at) in block.assignments.iter().zip(spans) {
                add(format!("{}.{}", block.name, a.name), *at);
            }
        }
        Self { subjects }
    }

    fn lookup(&self, subject: &str) -> &[Location] {
        self.subjects.get(subject).map_or(&[], Vec::as_slice)
    }
}
