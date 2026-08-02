//! Owned, read-only topology snapshots of the resolved engine model.

use oce_model::{ConnectorId, ModelGraph, Value};
use oce_store::Store;

use crate::engine::Engine;
use crate::io::connector_path;

/// An owned snapshot of blocks, edges, boundary inputs, and lowered pass-through pairs.
///
/// Equality compares `Real` parameter values bit-exactly: NaNs with identical bits compare equal,
/// while `+0.0` and `-0.0` compare unequal.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Topology {
    /// Authored elementary blocks, excluding reserved lowering blocks.
    pub blocks: Vec<TopologyBlock>,
    /// Directed output-to-input edges in graph order.
    pub connections: Vec<TopologyConnection>,
    /// Boundary input connector paths in graph order.
    ///
    /// A shared boundary IRI may occur more than once for a child input and a pass-through input.
    /// Consumers must preserve this order and multiplicity rather than deduplicating paths.
    pub external_inputs: Vec<String>,
    /// Authored boundary pass-throughs represented by reserved lowering blocks internally.
    pub pass_through: Vec<PassThroughPair>,
}

/// One authored elementary block in a [`Topology`] snapshot.
///
/// Equality compares `Real` parameter values bit-exactly: NaNs with identical bits compare equal,
/// while `+0.0` and `-0.0` compare unequal.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TopologyBlock {
    /// Live-parameter instance path: the instance IRI, or `b<id>` when unnamed.
    ///
    /// This deliberately follows `get_param`/`set_param`; the durable projection instead spells
    /// an unnamed block as `block#<id>`.
    pub instance_path: String,
    /// Canonical class identity.
    pub class_iri: String,
    /// Input connector paths in resolved index order.
    pub inputs: Vec<String>,
    /// Output connector paths in resolved index order.
    pub outputs: Vec<String>,
    /// Parameter name/value pairs in the block's parameter-table order.
    pub params: Vec<(String, Value)>,
}

/// One directed connector edge.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TopologyConnection {
    /// Driving output connector path.
    pub from: String,
    /// Driven input connector path.
    pub to: String,
}

/// One authored boundary input-to-output pass-through.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct PassThroughPair {
    /// Boundary input connector path.
    pub input: String,
    /// Boundary output connector path.
    pub output: String,
}

fn path(model: &ModelGraph, id: ConnectorId) -> String {
    connector_path(
        model
            .connectors
            .get(id.0 as usize)
            .and_then(|connector| connector.iri.as_deref()),
        id,
    )
}

fn params_equal(left: &[(String, Value)], right: &[(String, Value)]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((ln, lv), (rn, rv))| ln == rn && lv.bit_eq(rv))
}

impl PartialEq for TopologyBlock {
    fn eq(&self, other: &Self) -> bool {
        self.instance_path == other.instance_path
            && self.class_iri == other.class_iri
            && self.inputs == other.inputs
            && self.outputs == other.outputs
            && params_equal(&self.params, &other.params)
    }
}

impl PartialEq for Topology {
    fn eq(&self, other: &Self) -> bool {
        self.blocks == other.blocks
            && self.connections == other.connections
            && self.external_inputs == other.external_inputs
            && self.pass_through == other.pass_through
    }
}

impl<S: Store> Engine<S> {
    /// Build an owned, read-only topology snapshot.
    ///
    /// Every connector path is the authored subject IRI. This off-tick-path accessor allocates
    /// for the snapshot and never panics for any loaded or unloaded model; only a malformed
    /// (out-of-range) connector index, or an IRI-less hand-built connector, degrades to the
    /// synthetic `conn#<id>` fallback.
    #[must_use]
    pub fn topology(&self) -> Topology {
        let mut blocks = Vec::with_capacity(self.model.blocks.len());
        let mut pass_through = Vec::new();
        for block in &self.model.blocks {
            if block.class_iri.starts_with("urn:oce:lowering#") {
                if let (Some(input), Some(output)) = (block.inputs.first(), block.outputs.first()) {
                    pass_through.push(PassThroughPair {
                        input: path(&self.model, *input),
                        output: path(&self.model, *output),
                    });
                }
                continue;
            }
            blocks.push(TopologyBlock {
                instance_path: block
                    .instance_iri
                    .as_deref()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("b{}", block.id.0)),
                class_iri: block.class_iri.to_string(),
                inputs: block
                    .inputs
                    .iter()
                    .map(|id| path(&self.model, *id))
                    .collect(),
                outputs: block
                    .outputs
                    .iter()
                    .map(|id| path(&self.model, *id))
                    .collect(),
                params: block
                    .params
                    .values
                    .iter()
                    .map(|(name, value)| (name.to_string(), value.clone()))
                    .collect(),
            });
        }
        Topology {
            blocks,
            connections: self
                .model
                .connections
                .iter()
                .map(|connection| TopologyConnection {
                    from: path(&self.model, connection.from),
                    to: path(&self.model, connection.to),
                })
                .collect(),
            external_inputs: self
                .model
                .external_inputs
                .iter()
                .map(|id| path(&self.model, *id))
                .collect(),
            pass_through,
        }
    }
}
