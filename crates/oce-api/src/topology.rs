//! Owned, read-only topology snapshots of the resolved engine model.
//!
//! A [`Topology`] holds every fact the canonical content id ([`Engine::content_id`]) encodes,
//! under the document's own IRIs: the blocks with their class and ground parameters, the wiring,
//! and the boundary declarations with their type, `quantity`, `unit`, `displayUnit` and, for an
//! input, its effective acceptance domain. So two loaded models with equal topologies have equal
//! content ids. The converse does not hold: a topology also carries the instance namespace, the
//! root id and the loader's block and connection order, none of which enters the id.

use std::collections::BTreeMap;

use oce_model::resolved_model::DomainKey;
use oce_model::{Attrs, ConnectorId, ModelGraph, Value, ValueType};

use crate::engine::Engine;
use crate::io::connector_path;

/// An owned snapshot of the root identity, blocks, edges, boundary inputs, lowered pass-through
/// pairs, and declared boundary outputs.
///
/// Equality compares every field, `Real` parameter values and input bounds bit-exactly: NaNs with
/// identical bits compare equal, while `+0.0` and `-0.0` compare unequal.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Topology {
    /// The identity of the loaded model, equal to [`crate::LoadReport::model_id`]: the CXF
    /// top-composite `@id` in expanded canonical form when the document carries one, otherwise
    /// the deterministic synthetic key of the resolved-model projection. Empty before a load.
    ///
    /// It is the root every authored instance and boundary IRI of a flat document is written
    /// under (`<root>.<name>`). Read the root from here: it cannot be recovered from the block
    /// paths, because the longest prefix the blocks of a one-block document share is that
    /// block's whole path. A document the engine exported carries the exporter's fixed root
    /// `@id` and keeps its instance IRIs, so its paths do not start with this value.
    pub model_id: DomainKey,
    /// Authored elementary blocks, excluding reserved lowering blocks.
    pub blocks: Vec<TopologyBlock>,
    /// Directed output-to-input edges in graph order.
    pub connections: Vec<TopologyConnection>,
    /// Boundary input connector paths in graph order.
    ///
    /// A shared boundary IRI may occur more than once for a child input and a pass-through input.
    /// Consumers must preserve this order and multiplicity rather than deduplicating paths.
    pub external_inputs: Vec<String>,
    /// The executable boundary inputs, one per distinct path of [`Topology::external_inputs`],
    /// in UTF-8 path order: the same inputs, in the same order, as
    /// [`Engine::input_definitions`](crate::Engine::input_definitions), each with its declared
    /// `quantity`, `unit` and `displayUnit`.
    ///
    /// A declared root input that no connection reads is not part of the executable and is
    /// absent, as it is from the input definitions. An input that feeds only a pass-through is
    /// present.
    pub boundary_inputs: Vec<DeclaredInput>,
    /// Authored boundary pass-throughs represented by reserved lowering blocks internally.
    pub pass_through: Vec<PassThroughPair>,
    /// The **top** composite's root-declared boundary outputs — its authored output contract.
    ///
    /// Order is deterministic for a given document but is NOT the authored `hasOutput` array
    /// order: elided entries follow their declared nodes' source-document `@graph` positions,
    /// and pass-through entries (`path == driver_path`) are appended after them in their
    /// existing graph-position order. Key by [`DeclaredOutput::path`], never by index.
    ///
    /// A pass-through declared output also appears in [`Topology::pass_through`]; the two views
    /// are deliberately not deduplicated. An undriven declared output is absent here — the
    /// load-time `UndrivenBoundaryOutput` warning is its only representation. Nested
    /// composites' declared interfaces do not survive flattening and are not represented.
    pub boundary_outputs: Vec<DeclaredOutput>,
}

/// One executable boundary input: its identity, its exact type, the declaration's `quantity`,
/// `unit` and `displayUnit`, and its effective acceptance domain.
///
/// These are the boundary-input facts the canonical content id encodes, spelled as the facade
/// spells them elsewhere: `path`, `value_type`, `min` and `max` are the fields of the
/// [`InputDefinition`](crate::InputDefinition) of the same input, and `quantity`, `unit` and
/// `display_unit` are named as on [`PointInfo`](crate::PointInfo). The input's fan-out is not
/// repeated here: a block input fed by the boundary carries this `path` in
/// [`TopologyBlock::inputs`], at the port's index.
///
/// Equality compares `Real` bounds bit-exactly: NaNs with identical bits compare equal, while
/// `+0.0` and `-0.0` compare unequal.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DeclaredInput {
    /// The boundary input's authored subject IRI in expanded canonical form.
    pub path: String,
    /// The exact native type, an enumeration with its class.
    pub value_type: ValueType,
    /// The declaration's physical quantity (`"ThermodynamicTemperature"`), as loaded; `None` if
    /// unset or if the input is not a `Real`.
    pub quantity: Option<String>,
    /// The declaration's computation unit, as loaded; `None` if unset or not a `Real`.
    pub unit: Option<String>,
    /// The declaration's display unit, as loaded; `None` if unset or not a `Real`.
    pub display_unit: Option<String>,
    /// The inclusive lower bound of the effective acceptance domain: the declaration's bound
    /// intersected with the bound of every connector the input feeds, exactly as
    /// [`InputDefinition::min`](crate::InputDefinition::min) reports it. `None` when unbounded.
    pub min: Option<Value>,
    /// The inclusive upper bound of the effective acceptance domain, as
    /// [`InputDefinition::max`](crate::InputDefinition::max) reports it. `None` when unbounded.
    pub max: Option<Value>,
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

/// One root-declared boundary output: the authored declared identity and the internal driver
/// whose value it exposes (`_spec/18` R18-3).
///
/// Both paths are valid, distinct latest-state `get_output`/`watch` keys resolving to
/// the same value slot — neither name replaces the other. Driving is many-to-one: one driver
/// may serve several declared outputs, so `driver_path` values may repeat across entries. For a
/// pass-through declared output the two fields are equal.
///
/// The type and the three labels are the boundary-output facts the canonical content id
/// encodes. An output's `min` and `max` are not carried: the engine enforces neither and the id
/// leaves both out.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DeclaredOutput {
    /// The declared output's authored subject IRI in expanded canonical form — its point path.
    pub path: String,
    /// The driving connector's point path (equal to `path` for a pass-through).
    pub driver_path: String,
    /// The exact native type of the value the output exposes (the driving connector's), an
    /// enumeration with its class. It is the type
    /// [`FrameSlots::output_types`](crate::FrameSlots::output_types) reports for this path.
    pub value_type: ValueType,
    /// The declared output's physical quantity after §7.10 unification with its driver; for a
    /// pass-through, the declared node's own. `None` if unset or if the output is not a `Real`.
    pub quantity: Option<String>,
    /// The declared output's computation unit, read as `quantity` is.
    pub unit: Option<String>,
    /// The declared output's display unit, read as `quantity` is.
    pub display_unit: Option<String>,
}

/// A boundary's `quantity`, `unit` and `displayUnit`, which only a `Real` attribute set holds.
fn labels(attrs: Option<&Attrs>) -> (Option<String>, Option<String>, Option<String>) {
    attrs
        .and_then(Attrs::as_real)
        .map_or((None, None, None), |real| {
            (
                real.quantity.as_deref().map(str::to_owned),
                real.unit.as_deref().map(str::to_owned),
                real.display_unit.as_deref().map(str::to_owned),
            )
        })
}

fn bound_equal(left: Option<&Value>, right: Option<&Value>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.bit_eq(right),
        (None, None) => true,
        _ => false,
    }
}

impl PartialEq for DeclaredInput {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.value_type == other.value_type
            && self.quantity == other.quantity
            && self.unit == other.unit
            && self.display_unit == other.display_unit
            && bound_equal(self.min.as_ref(), other.min.as_ref())
            && bound_equal(self.max.as_ref(), other.max.as_ref())
    }
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
        self.model_id == other.model_id
            && self.blocks == other.blocks
            && self.connections == other.connections
            && self.external_inputs == other.external_inputs
            && self.boundary_inputs == other.boundary_inputs
            && self.pass_through == other.pass_through
            && self.boundary_outputs == other.boundary_outputs
    }
}

impl Engine {
    /// Build an owned, read-only topology snapshot.
    ///
    /// Every connector path is the authored `@id` of its host-visible identity node, as written
    /// in the source document. The boundary lists carry what [`Engine::content_id`] encodes of
    /// the boundary, read from the same places: an input's type and effective bounds from the
    /// input definitions, its labels from its declaration, an output's type from its driving
    /// connector and its labels from its declaration (for a pass-through, from the lowered
    /// output connector, which carries the declared node's attributes).
    ///
    /// This off-tick-path accessor allocates for the snapshot, changes no engine state and never
    /// panics for any loaded or unloaded model. It has no precondition: before a load it returns
    /// the empty snapshot, with an empty `model_id`. Only a malformed (out-of-range) connector
    /// index, or an IRI-less hand-built connector, degrades to the synthetic `conn#<id>`
    /// fallback; a declared output whose driving connector is out of range, which the load gate
    /// refuses, has no type to report and is absent, as an undriven one is.
    #[must_use]
    pub fn topology(&self) -> Topology {
        let mut blocks = Vec::with_capacity(self.model.blocks.len());
        let mut pass_through = Vec::new();
        let mut pass_through_outputs = Vec::new();
        for block in &self.model.blocks {
            if block.class_iri.starts_with("urn:oce:lowering#") {
                if let (Some(input), Some(output)) = (block.inputs.first(), block.outputs.first()) {
                    let pair = PassThroughPair {
                        input: path(&self.model, *input),
                        output: path(&self.model, *output),
                    };
                    // The lowered output connector carries the declared output node's type and
                    // attributes; the content id reads a pass-through output from it too.
                    if let Some(minted) = self.model.connectors.get(output.0 as usize) {
                        let (quantity, unit, display_unit) = labels(Some(&minted.attrs));
                        pass_through_outputs.push(DeclaredOutput {
                            path: pair.output.clone(),
                            driver_path: pair.output.clone(),
                            value_type: minted.value_type,
                            quantity,
                            unit,
                            display_unit,
                        });
                    }
                    pass_through.push(pair);
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
        // R18-3: the union of the two boundary-output representations — elided entries in
        // `ModelGraph.boundary_outputs` order, then the pass-through entries (which the model
        // deliberately keeps disjoint) in their existing graph-position order.
        let mut boundary_outputs: Vec<DeclaredOutput> = self
            .model
            .boundary_outputs
            .iter()
            .filter_map(|output| {
                let driver = self.model.connectors.get(output.source.0 as usize)?;
                let (quantity, unit, display_unit) = labels(Some(&output.attrs));
                Some(DeclaredOutput {
                    path: output.iri.to_string(),
                    driver_path: path(&self.model, output.source),
                    value_type: driver.value_type,
                    quantity,
                    unit,
                    display_unit,
                })
            })
            .collect();
        boundary_outputs.extend(pass_through_outputs);
        // One entry per input definition, already in UTF-8 path order. The labels are the
        // declaration's, never a target connector's: that is what the content id encodes.
        let declarations: BTreeMap<&str, &Attrs> = self
            .model
            .boundary_inputs
            .iter()
            .map(|declaration| (declaration.iri.as_ref(), &declaration.attrs))
            .collect();
        let boundary_inputs = self
            .io
            .frame_definitions
            .iter()
            .map(|definition| {
                let (quantity, unit, display_unit) =
                    labels(declarations.get(definition.path.as_str()).copied());
                DeclaredInput {
                    path: definition.path.clone(),
                    value_type: definition.value_type,
                    quantity,
                    unit,
                    display_unit,
                    min: definition.min.clone(),
                    max: definition.max.clone(),
                }
            })
            .collect();
        Topology {
            model_id: self.model_id.clone(),
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
            boundary_inputs,
            pass_through,
            boundary_outputs,
        }
    }
}

#[cfg(test)]
#[path = "topology_tests.rs"]
mod tests;
