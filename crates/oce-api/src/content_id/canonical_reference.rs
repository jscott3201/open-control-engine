//! The canonical graph as it was built before issue #811, kept as the tests' reference: every
//! record held its own copy of each name it carried, and a wired port was found by searching its
//! owner's port list. The production graph borrows its names from the model and finds ports in a
//! table built once; [`reference_bytes`] encodes this one with the same encoder, so a test that
//! compares the two compares the records the two builds make, byte for byte.

use oce_model::{Attrs, BlockInstance, ModelGraph, Value, ValueType};
use std::collections::BTreeMap;

use super::ContentIdError;
use super::encode;
use super::graph::{BlockRecord, OutputRecord, labels};
use super::graph::{CanonicalGraph, ConnectionRecord, Driver, InputRecord, Labels, Name};
use crate::io::IoInventory;

/// The `OCECANON` v1 bytes of `model`, built as before issue #811.
///
/// # Errors
/// As [`super::graph::canonical_graph`] and [`encode::encode`].
pub(super) fn reference_bytes(
    model: &ModelGraph,
    io: &IoInventory,
) -> Result<Vec<u8>, ContentIdError> {
    let owned = owned_graph(model, io)?;
    fn name(name: &OwnedName) -> Name<'_> {
        Name {
            form: name.form,
            text: name.text.as_str(),
        }
    }
    fn driver(driver: &OwnedDriver) -> Driver<'_> {
        match driver {
            OwnedDriver::Block(block, port) => Driver::Block(name(block), *port),
            OwnedDriver::Input(input) => Driver::Input(name(input)),
        }
    }
    let graph = CanonicalGraph {
        inputs: owned
            .inputs
            .iter()
            .map(|input| InputRecord {
                name: name(&input.name),
                value_type: input.value_type,
                labels: Labels {
                    quantity: input.labels.quantity,
                    unit: input.labels.unit,
                    display_unit: input.labels.display_unit,
                },
                min: input.min,
                max: input.max,
                targets: input
                    .targets
                    .iter()
                    .map(|(block, port)| (name(block), *port))
                    .collect(),
            })
            .collect(),
        outputs: owned
            .outputs
            .iter()
            .map(|output| OutputRecord {
                name: name(&output.name),
                value_type: output.value_type,
                labels: Labels {
                    quantity: output.labels.quantity,
                    unit: output.labels.unit,
                    display_unit: output.labels.display_unit,
                },
                driver: driver(&output.driver),
            })
            .collect(),
        blocks: owned
            .blocks
            .iter()
            .map(|block| BlockRecord {
                name: name(&block.name),
                class_path: block.class_path,
                signature_hash: block.signature_hash,
                params: block.params.clone(),
            })
            .collect(),
        connections: owned
            .connections
            .iter()
            .map(|edge| ConnectionRecord {
                target: name(&edge.target),
                input_port: edge.input_port,
                source: name(&edge.source),
                output_port: edge.output_port,
            })
            .collect(),
    };
    encode::encode(&graph)
}

/// A reduced IRI. `form` 0 is the name after the namespace (relative); 1 is the whole IRI
/// (absolute), kept when the remainder is empty or collides with another of the same kind.
/// Ordered by `(form, UTF-8 bytes)`, which is the derived order of these fields.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OwnedName {
    form: u8,
    text: String,
}

/// One boundary input record.
struct OwnedInput<'a> {
    name: OwnedName,
    value_type: ValueType,
    labels: Labels<'a>,
    min: Option<&'a Value>,
    max: Option<&'a Value>,
    /// `(block name, input port index)`, sorted; pass-through blocks excluded.
    targets: Vec<(OwnedName, u32)>,
}

/// One boundary output record.
struct OwnedOutput<'a> {
    name: OwnedName,
    value_type: ValueType,
    labels: Labels<'a>,
    driver: OwnedDriver,
}

/// What drives a boundary output.
enum OwnedDriver {
    /// A block's output port, by index.
    Block(OwnedName, u32),
    /// A boundary input, through a reserved pass-through block.
    Input(OwnedName),
}

/// One non-reserved block record, parameters sorted by name.
struct OwnedBlock<'a> {
    name: OwnedName,
    class_path: &'a str,
    signature_hash: u64,
    params: Vec<(&'a str, &'a Value)>,
}

/// One connection: target block and input port, then source block and output port.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct OwnedConnection {
    target: OwnedName,
    input_port: u32,
    source: OwnedName,
    output_port: u32,
}

/// The whole canonical graph, every list already in its canonical order.
struct OwnedGraph<'a> {
    inputs: Vec<OwnedInput<'a>>,
    outputs: Vec<OwnedOutput<'a>>,
    blocks: Vec<OwnedBlock<'a>>,
    connections: Vec<OwnedConnection>,
}

/// The namespace is the IRI up to and including its last `#`, `/` or `:`; the rest is the name.
/// `None` when the rest is empty (the IRI ends in a delimiter).
fn relative_name(iri: &str) -> Option<&str> {
    let start = iri.rfind(['#', '/', ':']).map_or(0, |at| at + 1);
    Some(&iri[start..]).filter(|rest| !rest.is_empty())
}

/// OwnedName every IRI of one kind: relative unless its remainder is empty or another IRI of the same
/// kind reduces to the same remainder, in which case every IRI in that collision stays whole.
/// Full IRIs are unique per kind in a loadable document, so the names are unique too.
fn name_all<'i>(iris: impl Iterator<Item = &'i str> + Clone) -> BTreeMap<&'i str, OwnedName> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for relative in iris.clone().filter_map(relative_name) {
        *seen.entry(relative).or_default() += 1;
    }
    iris.map(|iri| {
        let name = match relative_name(iri) {
            Some(relative) if seen.get(relative) == Some(&1) => OwnedName {
                form: 0,
                text: relative.to_owned(),
            },
            _ => OwnedName {
                form: 1,
                text: iri.to_owned(),
            },
        };
        (iri, name)
    })
    .collect()
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
fn owned_graph<'a>(
    model: &'a ModelGraph,
    io: &'a IoInventory,
) -> Result<OwnedGraph<'a>, ContentIdError> {
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
    let block_name = |index: usize| -> Result<&OwnedName, ContentIdError> {
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
        inputs.push(OwnedInput {
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
            (&source_connector.attrs, OwnedDriver::Input(input.clone()))
        } else {
            let port = port_index(&block.outputs, *source).ok_or(ContentIdError::MalformedModel)?;
            let attrs = declared_outputs
                .get(path.as_str())
                .copied()
                .ok_or(ContentIdError::MalformedModel)?;
            (
                attrs,
                OwnedDriver::Block(block_name(block_index)?.clone(), port),
            )
        };
        outputs.push(OwnedOutput {
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
        blocks.push(OwnedBlock {
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
        connections.push(OwnedConnection {
            target: block_name(target)?.clone(),
            input_port: port_index(&owner(target)?.inputs, edge.to)
                .ok_or(ContentIdError::MalformedModel)?,
            source: block_name(source)?.clone(),
            output_port: port_index(&owner(source)?.outputs, edge.from)
                .ok_or(ContentIdError::MalformedModel)?,
        });
    }
    connections.sort_unstable();

    Ok(OwnedGraph {
        inputs,
        outputs,
        blocks,
        connections,
    })
}
