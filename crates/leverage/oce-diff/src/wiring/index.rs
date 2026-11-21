//! One side's lookups, built once: the first block of each name, the declared boundary names and
//! the links at each block's ports.

use std::collections::{BTreeMap, BTreeSet};

use crate::work::{Counted, sort_counted, visited};

use super::{End, Side};

/// One link end at a block's port, seen from the block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Touch<'a> {
    /// Whether the port is an output of the block (the link leaves it) or an input.
    pub(crate) output: bool,
    /// The block's port.
    pub(crate) port: &'a str,
    /// The other end of the link: the driver of an input, a target of an output.
    pub(crate) other: End<'a>,
}

/// One side's lookups.
#[derive(Clone, Debug, Default)]
pub(crate) struct Index<'a> {
    first: BTreeMap<Counted<&'a str>, usize>,
    declared: BTreeSet<Counted<&'a str>>,
    touching: Vec<Vec<Touch<'a>>>,
}

impl<'a> Index<'a> {
    /// Index `side`. A link end that names a block touches the first block of that name.
    pub(crate) fn new(side: &Side<'a>) -> Self {
        visited(side.nodes.len() + side.boundaries.len() + side.links.len());
        let mut first = BTreeMap::new();
        for (position, node) in side.nodes.iter().enumerate() {
            first.entry(Counted(node.name)).or_insert(position);
        }
        let declared = side.boundaries.iter().map(|&name| Counted(name)).collect();
        let mut index = Self {
            first,
            declared,
            touching: vec![Vec::new(); side.nodes.len()],
        };
        for &(source, target) in &side.links {
            index.touch(target, false, source);
            index.touch(source, true, target);
        }
        for touches in &mut index.touching {
            sort_counted(touches);
        }
        index
    }

    fn touch(&mut self, end: End<'a>, output: bool, other: End<'a>) {
        let End::Port { block, port } = end else {
            return;
        };
        let Some(position) = self.block(block) else {
            return;
        };
        if let Some(touches) = self.touching.get_mut(position) {
            touches.push(Touch {
                output,
                port,
                other,
            });
        }
    }

    /// The position of the first block named `name`.
    pub(crate) fn block(&self, name: &str) -> Option<usize> {
        self.first.get(&Counted(name)).copied()
    }

    /// Whether `name` is a declared boundary.
    pub(crate) fn declares(&self, name: &str) -> bool {
        self.declared.contains(&Counted(name))
    }

    /// The link ends at the ports of the block at `position`, sorted: inputs before outputs,
    /// then by port, then by the other end.
    pub(crate) fn touching(&self, position: usize) -> &[Touch<'a>] {
        self.touching.get(position).map_or(&[], Vec::as_slice)
    }
}
