//! The canonical graph: the loaded flat model reduced to sorted, name-keyed records with no
//! `BlockId`, `ConnectorId`, declaration order or schedule in them (`docs/design/content-id.md`,
//! "The canonical graph").

use oce_model::{Attrs, BlockInstance, ModelGraph, Value, ValueType};
use std::collections::BTreeMap;

use super::ContentIdError;
use crate::io::IoInventory;

/// A reduced IRI. `form` 0 is the name after the namespace (relative); 1 is the whole IRI
/// (absolute), kept when the remainder is empty or collides with another of the same kind.
/// Ordered by `(form, UTF-8 bytes)`, which is the derived order of these fields.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Name {
    pub(super) form: u8,
    pub(super) text: String,
}

/// One boundary input record.
pub(super) struct InputRecord<'a> {
    pub(super) name: Name,
    pub(super) value_type: ValueType,
    pub(super) labels: Labels<'a>,
    pub(super) min: Option<&'a Value>,
    pub(super) max: Option<&'a Value>,
    /// `(block name, input port index)`, sorted; pass-through blocks excluded.
    pub(super) targets: Vec<(Name, u32)>,
}

/// One boundary output record.
pub(super) struct OutputRecord<'a> {
    pub(super) name: Name,
    pub(super) value_type: ValueType,
    pub(super) labels: Labels<'a>,
    pub(super) driver: Driver,
}

/// What drives a boundary output.
pub(super) enum Driver {
    /// A block's output port, by index.
    Block(Name, u32),
    /// A boundary input, through a reserved pass-through block.
    Input(Name),
}

/// A boundary's `quantity`, `unit` and `displayUnit`, each optional, as loaded.
#[derive(Default)]
pub(super) struct Labels<'a> {
    pub(super) quantity: Option<&'a str>,
    pub(super) unit: Option<&'a str>,
    pub(super) display_unit: Option<&'a str>,
}

/// One non-reserved block record, parameters sorted by name.
pub(super) struct BlockRecord<'a> {
    pub(super) name: Name,
    pub(super) class_path: &'a str,
    pub(super) signature_hash: u64,
    pub(super) params: Vec<(&'a str, &'a Value)>,
}

/// One connection: target block and input port, then source block and output port.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ConnectionRecord {
    pub(super) target: Name,
    pub(super) input_port: u32,
    pub(super) source: Name,
    pub(super) output_port: u32,
}

/// The whole canonical graph, every list already in its canonical order.
pub(super) struct CanonicalGraph<'a> {
    pub(super) inputs: Vec<InputRecord<'a>>,
    pub(super) outputs: Vec<OutputRecord<'a>>,
    pub(super) blocks: Vec<BlockRecord<'a>>,
    pub(super) connections: Vec<ConnectionRecord>,
}

/// The namespace is the IRI up to and including its last `#`, `/` or `:`; the rest is the name.
/// `None` when the rest is empty (the IRI ends in a delimiter).
fn relative_name(iri: &str) -> Option<&str> {
    let start = iri.rfind(['#', '/', ':']).map_or(0, |at| at + 1);
    Some(&iri[start..]).filter(|rest| !rest.is_empty())
}

/// Name every IRI of one kind: relative unless its remainder is empty or another IRI of the same
/// kind reduces to the same remainder, in which case every IRI in that collision stays whole.
/// Full IRIs are unique per kind in a loadable document, so the names are unique too.
fn name_all<'i>(iris: impl Iterator<Item = &'i str> + Clone) -> BTreeMap<&'i str, Name> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for relative in iris.clone().filter_map(relative_name) {
        *seen.entry(relative).or_default() += 1;
    }
    iris.map(|iri| {
        let name = match relative_name(iri) {
            Some(relative) if seen.get(relative) == Some(&1) => Name {
                form: 0,
                text: relative.to_owned(),
            },
            _ => Name {
                form: 1,
                text: iri.to_owned(),
            },
        };
        (iri, name)
    })
    .collect()
}

fn labels(attrs: &Attrs) -> Labels<'_> {
    attrs.as_real().map_or_else(Labels::default, |real| Labels {
        quantity: real.quantity.as_deref(),
        unit: real.unit.as_deref(),
        display_unit: real.display_unit.as_deref(),
    })
}

fn port_index(ports: &[oce_model::ConnectorId], id: oce_model::ConnectorId) -> Option<u32> {
    ports
        .iter()
        .position(|port| *port == id)
        .and_then(|index| u32::try_from(index).ok())
}

/// Whether a class path is an engine-reserved lowering identity: the catalog's `reserved` flag,
/// set for exactly the three `urn:oce:lowering#PassThrough.*` classes.
fn is_reserved(class_path: &str) -> bool {
    oce_blocks::catalog()
        .iter()
        .any(|entry| entry.reserved && entry.class_path == class_path)
}

/// Build the canonical graph of a loaded model and its boundary I/O table.
///
/// # Errors
/// [`ContentIdError`] for a hand-built model no public load path produces: an unnamed
/// non-reserved block, an unregistered class, a dangling connector or a reserved block wired
/// through an ordinary connection. Never panics.
pub(super) fn canonical_graph<'a>(
    model: &'a ModelGraph,
    io: &'a IoInventory,
) -> Result<CanonicalGraph<'a>, ContentIdError> {
    let reserved: Vec<bool> = model
        .blocks
        .iter()
        .map(|block| is_reserved(&block.class_iri))
        .collect();
    let mut block_iris: Vec<Option<&str>> = vec![None; model.blocks.len()];
    for (block, is_reserved) in model.blocks.iter().zip(&reserved) {
        if *is_reserved {
            continue;
        }
        let iri = block
            .instance_iri
            .as_deref()
            .ok_or(ContentIdError::UnnamedBlock { block: block.id.0 })?;
        if let Some(slot) = block_iris.get_mut(block.id.0 as usize) {
            *slot = Some(iri);
        }
    }
    let block_names = name_all(block_iris.iter().flatten().copied());
    let block_name = |index: usize| -> Result<&Name, ContentIdError> {
        block_iris
            .get(index)
            .copied()
            .flatten()
            .and_then(|iri| block_names.get(iri))
            .ok_or(ContentIdError::MalformedModel)
    };
    let connector = |id: oce_model::ConnectorId| {
        model
            .connectors
            .get(id.0 as usize)
            .ok_or(ContentIdError::MalformedModel)
    };
    let owner = |index: usize| -> Result<&BlockInstance, ContentIdError> {
        model
            .blocks
            .get(index)
            .ok_or(ContentIdError::MalformedModel)
    };

    let input_names = name_all(io.frame_definitions.iter().map(|d| d.path.as_str()));
    let declared_inputs: BTreeMap<&str, &Attrs> = model
        .boundary_inputs
        .iter()
        .map(|declaration| (declaration.iri.as_ref(), &declaration.attrs))
        .collect();
    let mut inputs = Vec::with_capacity(io.frame_definitions.len());
    for definition in &io.frame_definitions {
        let path = definition.path.as_str();
        let mut targets = Vec::new();
        for target in io.input_targets(path).unwrap_or_default() {
            let block = connector(*target)?.block.0 as usize;
            if reserved.get(block).copied().unwrap_or(false) {
                continue;
            }
            let port =
                port_index(&owner(block)?.inputs, *target).ok_or(ContentIdError::MalformedModel)?;
            targets.push((block_name(block)?.clone(), port));
        }
        targets.sort_unstable();
        inputs.push(InputRecord {
            name: input_names[path].clone(),
            value_type: definition.value_type,
            labels: declared_inputs
                .get(path)
                .map_or_else(Labels::default, |attrs| labels(attrs)),
            min: definition.min.as_ref(),
            max: definition.max.as_ref(),
            targets,
        });
    }
    inputs.sort_unstable_by(|left, right| left.name.cmp(&right.name));

    let output_names = name_all(io.frame_outputs.iter().map(|(path, _)| path.as_str()));
    let declared_outputs: BTreeMap<&str, &Attrs> = model
        .boundary_outputs
        .iter()
        .map(|declaration| (declaration.iri.as_ref(), &declaration.attrs))
        .collect();
    let mut outputs = Vec::with_capacity(io.frame_outputs.len());
    for (path, source) in &io.frame_outputs {
        let source_connector = connector(*source)?;
        let block_index = source_connector.block.0 as usize;
        let block = owner(block_index)?;
        let (attrs, driver) = if reserved.get(block_index).copied().unwrap_or(false) {
            // A pass-through: the minted output connector carries the declared output's authored
            // attributes, and the driver is the boundary input feeding the block's input.
            let feed = block
                .inputs
                .first()
                .copied()
                .ok_or(ContentIdError::MalformedModel)?;
            let feed_path = connector(feed)?
                .iri
                .as_deref()
                .ok_or(ContentIdError::MalformedModel)?;
            let input = input_names
                .get(feed_path)
                .ok_or(ContentIdError::MalformedModel)?;
            (&source_connector.attrs, Driver::Input(input.clone()))
        } else {
            let port = port_index(&block.outputs, *source).ok_or(ContentIdError::MalformedModel)?;
            let attrs = declared_outputs
                .get(path.as_str())
                .copied()
                .ok_or(ContentIdError::MalformedModel)?;
            (attrs, Driver::Block(block_name(block_index)?.clone(), port))
        };
        outputs.push(OutputRecord {
            name: output_names[path.as_str()].clone(),
            value_type: source_connector.value_type,
            labels: labels(attrs),
            driver,
        });
    }
    outputs.sort_unstable_by(|left, right| left.name.cmp(&right.name));

    let mut signatures: BTreeMap<&str, u64> = BTreeMap::new();
    let mut blocks = Vec::with_capacity(model.blocks.len());
    for (index, block) in model.blocks.iter().enumerate() {
        if reserved.get(index).copied().unwrap_or(false) {
            continue;
        }
        let class_path = block.class_iri.as_ref();
        let signature_hash = match signatures.get(class_path) {
            Some(hash) => *hash,
            None => {
                let hash = oce_blocks::class_signature_hash(class_path).ok_or_else(|| {
                    ContentIdError::UnregisteredClass {
                        class_path: class_path.to_owned(),
                    }
                })?;
                signatures.insert(class_path, hash);
                hash
            }
        };
        let mut params: Vec<(&str, &Value)> = block
            .params
            .values
            .iter()
            .map(|(name, value)| (name.as_ref(), value))
            .collect();
        params.sort_unstable_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
        blocks.push(BlockRecord {
            name: block_name(index)?.clone(),
            class_path,
            signature_hash,
            params,
        });
    }
    blocks.sort_unstable_by(|left, right| left.name.cmp(&right.name));

    let mut connections = Vec::with_capacity(model.connections.len());
    for edge in &model.connections {
        let from = connector(edge.from)?;
        let to = connector(edge.to)?;
        let (source, target) = (from.block.0 as usize, to.block.0 as usize);
        connections.push(ConnectionRecord {
            target: block_name(target)?.clone(),
            input_port: port_index(&owner(target)?.inputs, edge.to)
                .ok_or(ContentIdError::MalformedModel)?,
            source: block_name(source)?.clone(),
            output_port: port_index(&owner(source)?.outputs, edge.from)
                .ok_or(ContentIdError::MalformedModel)?,
        });
    }
    connections.sort_unstable();

    Ok(CanonicalGraph {
        inputs,
        outputs,
        blocks,
        connections,
    })
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
