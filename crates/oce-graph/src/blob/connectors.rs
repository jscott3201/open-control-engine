//! The per-connector ownership table the decoder fills while it walks the instances.
//!
//! One 8-byte entry per connector id: the owning block, and the port index, direction, port kind
//! and boundary-input mark packed into one `u32`. The structural pass only proves that the
//! connector count fits a quarter of the instance section (each id needs a 4-byte port slot), so a
//! hostile count costs at most twice the section's length here; the full connector arena
//! (`Vec<Connector>`) is built from this table only after every port list has claimed its ids
//! exactly once and every constructed block's arity has matched its port lists.

use alloc::{vec, vec::Vec};

use oce_blocks::{MAX_RESOLVED_ARITY, PortKind};
use oce_model::{BlockId, Connector, ConnectorId, Dir, ValueType};

use super::error::{BlobError, ConnectorFault};

/// The owner of a connector no port list has claimed yet.
const UNCLAIMED: u32 = u32::MAX;
/// Bits 0..22: the port index (below `MAX_RESOLVED_ARITY`, which is `1 << 21`).
const PORT_MASK: u32 = (1 << 22) - 1;
/// Bit 22: an output port.
const OUTPUT: u32 = 1 << 22;
/// Bits 23..25: the port kind.
const KIND_SHIFT: u32 = 23;
/// Bit 25: a boundary input targets this connector.
const TARGET: u32 = 1 << 25;

const _: () = assert!(MAX_RESOLVED_ARITY as u64 <= PORT_MASK as u64);

#[derive(Clone, Copy)]
struct Owner {
    block: u32,
    packed: u32,
}

/// One claimed connector, unpacked.
#[derive(Clone, Copy)]
pub(super) struct Port {
    pub(super) block: u32,
    pub(super) port: u32,
    pub(super) dir: Dir,
    pub(super) value_type: ValueType,
    pub(super) target: bool,
}

/// The ownership table, indexed by connector id.
pub(super) struct Owners(Vec<Owner>);

impl Owners {
    /// A table of `count` unclaimed connectors (one allocation of `8 * count` bytes).
    pub(super) fn new(count: usize) -> Self {
        Self(vec![
            Owner {
                block: UNCLAIMED,
                packed: 0,
            };
            count
        ])
    }

    /// Claim connector `id` as port `port` of `block`, exactly once.
    pub(super) fn claim(
        &mut self,
        id: u32,
        block: u32,
        port: usize,
        dir: Dir,
        kind: PortKind,
    ) -> Result<(), BlobError> {
        let Some(owner) = self.0.get_mut(usize::try_from(id).unwrap_or(usize::MAX)) else {
            return Err(BlobError::InvalidConnector {
                connector: id,
                fault: ConnectorFault::OutOfRange,
            });
        };
        if owner.block != UNCLAIMED {
            return Err(BlobError::InvalidConnector {
                connector: id,
                fault: ConnectorFault::Duplicated,
            });
        }
        let kind = match kind {
            PortKind::Real => 0,
            PortKind::Integer => 1,
            PortKind::Boolean => 2,
        };
        // `port` is below the arity the structural pass bounded by `MAX_RESOLVED_ARITY`.
        let port = u32::try_from(port).unwrap_or(PORT_MASK) & PORT_MASK;
        let output = if dir == Dir::Out { OUTPUT } else { 0 };
        *owner = Owner {
            block,
            packed: port | output | (kind << KIND_SHIFT),
        };
        Ok(())
    }

    /// The first connector no port list claimed, if any.
    pub(super) fn first_unclaimed(&self) -> Option<u32> {
        self.0
            .iter()
            .position(|owner| owner.block == UNCLAIMED)
            .map(|id| u32::try_from(id).unwrap_or(u32::MAX))
    }

    /// The claimed connector `id`, or `None` when `id` is out of range. Call after every port list
    /// has been claimed and [`Owners::first_unclaimed`] returned `None`.
    pub(super) fn get(&self, id: u32) -> Option<Port> {
        self.0
            .get(usize::try_from(id).ok()?)
            .map(|owner| unpack(*owner))
    }

    /// Mark connector `id` (in range) as a boundary input target.
    pub(super) fn mark_target(&mut self, id: u32) {
        if let Some(owner) = self.0.get_mut(usize::try_from(id).unwrap_or(usize::MAX)) {
            owner.packed |= TARGET;
        }
    }

    /// The connector arena: one [`Connector`] per id with its owner, direction and value type,
    /// `decl_order` equal to the id, default attributes and no IRI.
    pub(super) fn connectors(&self) -> Vec<Connector> {
        // An exact-size iterator: one allocation.
        self.0
            .iter()
            .enumerate()
            .map(|(id, owner)| {
                let id = u32::try_from(id).unwrap_or(u32::MAX);
                let port = unpack(*owner);
                Connector::new(
                    ConnectorId(id),
                    BlockId(port.block),
                    port.dir,
                    port.value_type,
                    id,
                )
            })
            .collect()
    }
}

fn unpack(owner: Owner) -> Port {
    Port {
        block: owner.block,
        port: owner.packed & PORT_MASK,
        dir: if owner.packed & OUTPUT == 0 {
            Dir::In
        } else {
            Dir::Out
        },
        value_type: match (owner.packed >> KIND_SHIFT) & 0b11 {
            0 => ValueType::Real,
            1 => ValueType::Integer,
            _ => ValueType::Boolean,
        },
        target: owner.packed & TARGET != 0,
    }
}
