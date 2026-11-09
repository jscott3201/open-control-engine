//! Where each part of a parsed sequence was written, and how a validator finding is placed on
//! the text (the [`crate::Finding::span`] contract of GitHub #466).
//!
//! A finding names its subject by model path: `sequence`, `extends`, a declared name, `block.param`
//! or `connection[N]`. The map resolves the path to the model elements it can mean, in authored
//! model order (parameters, inputs, outputs, blocks), and takes the first. A duplicate is the one
//! case where a path means several elements: the validator reports the k-th `duplicate-name` or
//! `duplicate-parameter` of a subject at its (k+1)-th declaration, so the map does the same.

use std::collections::BTreeMap;

use crate::location::Location;
use crate::model::Sequence;
use crate::validate::{DUPLICATE_NAME, DUPLICATE_PARAMETER, Finding};

/// The text location of every statement, assignment and connection of one parsed document, in
/// model order. Built by [`crate::parse_text_with_spans`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceMap {
    pub(super) sequence: Location,
    pub(super) extends: Location,
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
            extends: sequence,
            params: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            blocks: Vec::new(),
            assignments: Vec::new(),
            connections: Vec::new(),
        }
    }

    /// Every location `subject` can mean in `sequence`, in model order: the `sequence` keyword's
    /// name, the `extends` operand, a declaration's name, an assignment's name (an array element's
    /// value) or a connection's source. Empty when the subject names nothing in the map.
    #[must_use]
    pub fn locations(&self, sequence: &Sequence, subject: &str) -> Vec<Location> {
        match subject {
            "sequence" => return vec![self.sequence],
            "extends" => return vec![self.extends],
            _ => {}
        }
        if let Some(index) = subject
            .strip_prefix("connection[")
            .and_then(|rest| rest.strip_suffix(']'))
            .and_then(|digits| digits.parse::<usize>().ok())
        {
            return self.connections.get(index).copied().into_iter().collect();
        }
        if let Some((block, param)) = subject.split_once('.') {
            let mut found = Vec::new();
            for (i, b) in sequence.blocks.iter().enumerate() {
                if b.name.as_str() != block {
                    continue;
                }
                for (j, a) in b.assignments.iter().enumerate() {
                    if a.name.as_str() == param
                        && let Some(at) = self.assignments.get(i).and_then(|v| v.get(j))
                    {
                        found.push(*at);
                    }
                }
            }
            return found;
        }
        let named = |names: Vec<&str>, at: &[Location]| -> Vec<Location> {
            names
                .into_iter()
                .zip(at)
                .filter(|(name, _)| *name == subject)
                .map(|(_, at)| *at)
                .collect()
        };
        let mut found = named(
            sequence.params.iter().map(|p| p.name.as_str()).collect(),
            &self.params,
        );
        found.extend(named(
            sequence.inputs.iter().map(|b| b.name.as_str()).collect(),
            &self.inputs,
        ));
        found.extend(named(
            sequence.outputs.iter().map(|b| b.name.as_str()).collect(),
            &self.outputs,
        ));
        found.extend(named(
            sequence.blocks.iter().map(|b| b.name.as_str()).collect(),
            &self.blocks,
        ));
        found
    }

    /// Set [`Finding::span`] on each finding of `sequence` (the model this map was parsed with):
    /// the first location of its subject, or, for the k-th `duplicate-name` /
    /// `duplicate-parameter` finding of a subject, its (k+1)-th. A subject the map cannot place
    /// keeps `None`.
    pub fn apply(&self, sequence: &Sequence, findings: &mut [Finding]) {
        let mut repeats: BTreeMap<(&'static str, String), usize> = BTreeMap::new();
        for finding in findings.iter_mut() {
            let at = self.locations(sequence, &finding.subject);
            let index = if finding.code == DUPLICATE_NAME || finding.code == DUPLICATE_PARAMETER {
                let seen = repeats
                    .entry((finding.code, finding.subject.clone()))
                    .or_insert(0);
                *seen += 1;
                (*seen).min(at.len().saturating_sub(1))
            } else {
                0
            };
            finding.span = at.get(index).copied();
        }
    }
}
