//! Name indexes built once per validation run, so every lookup is logarithmic and a document at
//! the wire bounds validates in time near-linear in its size (the issue #463 review measured a
//! quadratic scan at 3.4 s for 30,000 blocks and 60,000 connections). Every index is keyed by
//! [`Counted`], so the tests of `crate::counted` pin the number of comparisons a run makes.

use std::collections::{BTreeMap, BTreeSet};

use oce_api::{ClassPort, ClassPorts};

use crate::counted::Counted;
use crate::model::Sequence;

/// The first declaration of each name of one kind, by name.
pub(super) struct Names<'a>(BTreeMap<Counted<&'a str>, usize>);

impl<'a> Names<'a> {
    fn new(names: impl Iterator<Item = &'a str>) -> Self {
        let mut map = BTreeMap::new();
        for (index, name) in names.enumerate() {
            map.entry(Counted(name)).or_insert(index);
        }
        Self(map)
    }

    /// The position of the first declaration named `name`, if any.
    pub(super) fn get(&self, name: &str) -> Option<usize> {
        self.0.get(&Counted(name)).copied()
    }

    /// Whether a declaration is named `name`.
    pub(super) fn contains(&self, name: &str) -> bool {
        self.0.contains_key(&Counted(name))
    }
}

/// The first declaration of each name, by kind.
pub(super) struct Declared<'a> {
    pub(super) params: Names<'a>,
    pub(super) inputs: Names<'a>,
    pub(super) outputs: Names<'a>,
    pub(super) blocks: Names<'a>,
}

impl<'a> Declared<'a> {
    pub(super) fn new(sequence: &'a Sequence) -> Self {
        Self {
            params: Names::new(sequence.params.iter().map(|p| p.name.as_str())),
            inputs: Names::new(sequence.inputs.iter().map(|b| b.name.as_str())),
            outputs: Names::new(sequence.outputs.iter().map(|b| b.name.as_str())),
            blocks: Names::new(sequence.blocks.iter().map(|b| b.name.as_str())),
        }
    }
}

/// The port names of one side of a block, for logarithmic lookup.
pub(super) struct PortNames<'a>(BTreeSet<Counted<&'a str>>);

impl<'a> PortNames<'a> {
    fn new(ports: &'a [ClassPort]) -> Self {
        Self(ports.iter().map(|p| Counted(p.name.as_str())).collect())
    }

    /// Whether the side has a port named `name`.
    pub(super) fn contains(&self, name: &str) -> bool {
        self.0.contains(&Counted(name))
    }
}

/// A block's resolved ports with name sets for logarithmic lookup (a width-driven block may have
/// up to 1,048,576 ports).
pub(super) struct Ports<'a> {
    pub(super) resolved: &'a ClassPorts,
    pub(super) inputs: PortNames<'a>,
    pub(super) outputs: PortNames<'a>,
}

impl<'a> Ports<'a> {
    pub(super) fn new(resolved: &'a ClassPorts) -> Self {
        Self {
            resolved,
            inputs: PortNames::new(&resolved.inputs),
            outputs: PortNames::new(&resolved.outputs),
        }
    }
}
