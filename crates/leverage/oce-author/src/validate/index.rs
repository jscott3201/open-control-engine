//! Name indexes built once per validation run, so every lookup is logarithmic and a document at
//! the wire bounds validates in time near-linear in its size (the issue #463 review measured a
//! quadratic scan at 3.4 s for 30,000 blocks and 60,000 connections).

use std::collections::{BTreeMap, BTreeSet};

use oce_api::ClassPorts;

use crate::model::Sequence;

/// The first declaration of each name, by kind.
pub(super) struct Declared<'a> {
    pub(super) params: BTreeMap<&'a str, usize>,
    pub(super) inputs: BTreeMap<&'a str, usize>,
    pub(super) outputs: BTreeMap<&'a str, usize>,
    pub(super) blocks: BTreeMap<&'a str, usize>,
}

fn first<'a>(names: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut map = BTreeMap::new();
    for (index, name) in names.enumerate() {
        map.entry(name).or_insert(index);
    }
    map
}

impl<'a> Declared<'a> {
    pub(super) fn new(sequence: &'a Sequence) -> Self {
        Self {
            params: first(sequence.params.iter().map(|p| p.name.as_str())),
            inputs: first(sequence.inputs.iter().map(|b| b.name.as_str())),
            outputs: first(sequence.outputs.iter().map(|b| b.name.as_str())),
            blocks: first(sequence.blocks.iter().map(|b| b.name.as_str())),
        }
    }
}

/// A block's resolved ports with name sets for logarithmic lookup (a width-driven block may have
/// up to 1,048,576 ports).
pub(super) struct Ports {
    pub(super) resolved: ClassPorts,
    pub(super) inputs: BTreeSet<String>,
    pub(super) outputs: BTreeSet<String>,
}

impl Ports {
    pub(super) fn new(resolved: ClassPorts) -> Self {
        let inputs = resolved.inputs.iter().map(|p| p.name.clone()).collect();
        let outputs = resolved.outputs.iter().map(|p| p.name.clone()).collect();
        Self {
            resolved,
            inputs,
            outputs,
        }
    }
}
