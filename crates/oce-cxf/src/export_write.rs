//! Writing a validated export [`Plan`] as CXF JSON-LD bytes, one node at a time, into a buffer
//! of exactly the document's size (issue #803).
//!
//! The document is the root, then each block followed by its parameter nodes, then one port node
//! per connector, then the boundary-input and boundary-output nodes: the order the plan's vectors
//! give. The bytes are those [`crate::write_document`] writes for the whole
//! [`crate::CxfDocument`]: the `{"@context":…,"@graph":[…]}` frame is written as `serde_json`'s
//! compact formatter writes a map and a sequence, the root, port and boundary nodes are each a
//! [`Node`] serialized with the same derive, and a block node and a parameter node are written by
//! [`BlockNode`] and [`ParamNode`], which name the same keys in the same order. The tests check the
//! bytes against the document built in full (`export_write_tests.rs`).
//!
//! What it holds. Building every node first held a 1,120-byte [`Node`] for each parameter, more
//! while the node vector grew: a `MatrixGain` whose `K` is 128 by 128 exported 1.7 MB of bytes at a
//! peak of 58.7 MB. Now a parameter costs nothing here but its bytes: its `@id`, the block's IRI,
//! `.` and its name, is spelled straight into the output from the graph, in the block's
//! `S231:hasParameter` list and in its own node, and its binding is built as it is written. The
//! document is written twice, first only to count its bytes and then into a buffer of exactly that
//! size, so the buffer never grows: a growing buffer held up to three times the bytes while it
//! moved. One root, port or boundary node is built at a time, from the plan.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::Value;
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Serialize, Serializer};

use crate::CxfError;
use crate::dto::{Context, CxfValue, IriRef, Node, OneOrMany};
use crate::export_attrs::emit_port_attrs;

use super::{EXPORT_ROOT_IRI, MSG_STRUCTURE, Plan, S231_CONTEXT_IRI, param_binding};

#[cfg(test)]
#[path = "export_write_tests.rs"]
mod tests;

/// The bytes of the export document for `plan`: those `crate::write_document` writes for the
/// document built in full.
///
/// # Errors
/// [`CxfError::Json`] if a node does not serialize, as `crate::write_document` reports it.
/// [`CxfError::Validation`] with one [`MSG_STRUCTURE`] error if a planned parameter has no
/// binding, which the plan rules out (kept total, never a panic).
pub(super) fn write(plan: &Plan<'_>) -> Result<Vec<u8>, CxfError> {
    let mut size = ByteCount(0);
    emit(plan, &mut size)?;
    let mut out = Vec::with_capacity(size.0);
    emit(plan, &mut out)?;
    Ok(out)
}

/// Write the document for `plan` into `out`.
fn emit<W: io::Write>(plan: &Plan<'_>, out: &mut W) -> Result<(), CxfError> {
    out.write_all(b"{\"@context\":")
        .map_err(serde_json::Error::io)?;
    serde_json::to_writer(&mut *out, &context())?;
    out.write_all(b",\"@graph\":[")
        .map_err(serde_json::Error::io)?;

    let mut root = blank_node(EXPORT_ROOT_IRI.to_owned());
    root.r#type = Some(OneOrMany::One("S231:Block".to_owned()));
    root.has_input = refs(plan.boundaries.iter().map(|b| b.iri.clone()).collect());
    root.has_output = refs(
        plan.boundary_outputs
            .iter()
            .map(|output| output.iri.clone())
            .collect(),
    );
    root.contains_block = refs(plan.blocks.iter().map(|b| b.iri.clone()).collect());
    crate::translation_software::stamp(&mut root);
    serde_json::to_writer(&mut *out, &root)?;
    drop(root);

    for b in &plan.blocks {
        out.write_all(b",").map_err(serde_json::Error::io)?;
        let node = BlockNode {
            id: &b.iri,
            r#type: &b.type_iri,
            has_input: &b.input_ports,
            has_output: &b.output_ports,
            has_parameter: ParamRefs {
                block: &b.iri,
                params: b.params,
            },
        };
        serde_json::to_writer(&mut *out, &node)?;
        for (name, value) in b.params {
            let Some(binding) = param_binding(value) else {
                return Err(unbound());
            };
            out.write_all(b",").map_err(serde_json::Error::io)?;
            let node = ParamNode {
                id: ParamId {
                    block: &b.iri,
                    name: name.as_ref(),
                },
                value: binding,
            };
            serde_json::to_writer(&mut *out, &node)?;
        }
    }

    for port in &plan.ports {
        let mut node = blank_node(port.iri.clone());
        node.is_of_data_type = Some(iri_ref(port.datatype.to_owned()));
        node.is_connected_to = refs(port.targets.clone());
        emit_port_attrs(&mut node, &port.attrs);
        next_node(out, &node)?;
    }

    for boundary in &plan.boundaries {
        let mut node = blank_node(boundary.iri.clone());
        node.is_of_data_type = Some(iri_ref(boundary.datatype.to_owned()));
        node.is_connected_to = refs(boundary.targets.clone());
        emit_port_attrs(&mut node, &boundary.attrs);
        next_node(out, &node)?;
    }

    for output in &plan.boundary_outputs {
        let mut node = blank_node(output.iri.clone());
        node.is_of_data_type = Some(iri_ref(output.datatype.to_owned()));
        emit_port_attrs(&mut node, &output.attrs);
        next_node(out, &node)?;
    }

    // The document has no top-level key besides `@context` and `@graph`.
    out.write_all(b"]}").map_err(serde_json::Error::io)?;
    Ok(())
}

/// Write `node` as a further element of `@graph`: the root is always the first.
fn next_node<W: io::Write>(out: &mut W, node: &Node) -> Result<(), CxfError> {
    out.write_all(b",").map_err(serde_json::Error::io)?;
    serde_json::to_writer(&mut *out, node)?;
    Ok(())
}

/// The refusal of a planned parameter with no binding: unreachable, as the plan refuses every
/// value [`param_binding`] has no binding for.
fn unbound() -> CxfError {
    CxfError::Validation(vec![Diagnostic::error(
        DiagCode::ExportUnsupported,
        MSG_STRUCTURE,
    )])
}

/// A sink that counts the bytes written to it and keeps none.
struct ByteCount(usize);

impl io::Write for ByteCount {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 = self.0.saturating_add(buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A block node as [`Node`] writes it with `@id`, `@type` and the three edges set: the same keys
/// in the same order, an empty edge left out, a one-element edge a bare object.
struct BlockNode<'a> {
    id: &'a str,
    r#type: &'a str,
    has_input: &'a [String],
    has_output: &'a [String],
    has_parameter: ParamRefs<'a>,
}

impl Serialize for BlockNode<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut node = serializer.serialize_struct("Node", 5)?;
        node.serialize_field("@id", self.id)?;
        node.serialize_field("@type", self.r#type)?;
        if !self.has_input.is_empty() {
            node.serialize_field("S231:hasInput", &Refs(self.has_input))?;
        }
        if !self.has_output.is_empty() {
            node.serialize_field("S231:hasOutput", &Refs(self.has_output))?;
        }
        if !self.has_parameter.params.is_empty() {
            node.serialize_field("S231:hasParameter", &self.has_parameter)?;
        }
        node.end()
    }
}

/// An edge of `@id`s as [`OneOrMany<IriRef>`] writes it: one `{"@id":…}` bare, more as an array.
struct Refs<'a>(&'a [String]);

impl Serialize for Refs<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            [one] => IdRef(one.as_str()).serialize(serializer),
            many => {
                let mut seq = serializer.serialize_seq(Some(many.len()))?;
                for id in many {
                    seq.serialize_element(&IdRef(id.as_str()))?;
                }
                seq.end()
            }
        }
    }
}

/// A block's `S231:hasParameter` edge, one `{"@id":"<block>.<name>"}` for each of its parameters,
/// as [`OneOrMany<IriRef>`] writes it.
struct ParamRefs<'a> {
    block: &'a str,
    params: &'a [(Arc<str>, Value)],
}

impl<'a> Serialize for ParamRefs<'a> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let id = |name: &'a Arc<str>| {
            IdRef(ParamId {
                block: self.block,
                name: name.as_ref(),
            })
        };
        match self.params {
            [(name, _)] => id(name).serialize(serializer),
            many => {
                let mut seq = serializer.serialize_seq(Some(many.len()))?;
                for (name, _) in many {
                    seq.serialize_element(&id(name))?;
                }
                seq.end()
            }
        }
    }
}

/// A `{"@id":…}` reference, as an [`IriRef`] with no other key writes it.
struct IdRef<T>(T);

impl<T: Serialize> Serialize for IdRef<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut reference = serializer.serialize_struct("IriRef", 1)?;
        reference.serialize_field("@id", &self.0)?;
        reference.end()
    }
}

/// A parameter node's `@id`, `<block>.<name>`, written as one JSON string and never built.
#[derive(Clone, Copy)]
struct ParamId<'a> {
    block: &'a str,
    name: &'a str,
}

impl fmt::Display for ParamId<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.block)?;
        f.write_str(".")?;
        f.write_str(self.name)
    }
}

impl Serialize for ParamId<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A parameter node as [`Node`] writes it with `@id` and `S231:value` set.
struct ParamNode<'a> {
    id: ParamId<'a>,
    value: CxfValue,
}

impl Serialize for ParamNode<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut node = serializer.serialize_struct("Node", 2)?;
        node.serialize_field("@id", &self.id)?;
        node.serialize_field("S231:value", &self.value)?;
        node.end()
    }
}

/// The export's `@context`: the one `S231` prefix binding.
fn context() -> Context {
    let mut context = BTreeMap::new();
    context.insert(
        "S231".to_owned(),
        serde_json::Value::String(S231_CONTEXT_IRI.to_owned()),
    );
    Context::Map(context)
}

/// A [`Node`] with only its `@id` set — every other field absent/empty.
fn blank_node(id: String) -> Node {
    Node {
        id,
        r#type: None,
        has_input: OneOrMany::None,
        has_output: OneOrMany::None,
        has_parameter: OneOrMany::None,
        has_constant: OneOrMany::None,
        contains_block: OneOrMany::None,
        has_instance: OneOrMany::None,
        is_connected_to: OneOrMany::None,
        is_of_data_type: None,
        label: None,
        access: None,
        value: None,
        min: None,
        max: None,
        nominal: None,
        unit: None,
        quantity: None,
        display_unit: None,
        is_final: None,
        is_array: None,
        n_dims: None,
        size_dims: None,
        fmu_path: None,
        is_conditional: None,
        cond_expr: None,
        is_replaceable: None,
        other: BTreeMap::new(),
    }
}

/// A bare `{"@id": …}` cross-reference.
fn iri_ref(id: String) -> IriRef {
    IriRef {
        id,
        other: BTreeMap::new(),
    }
}

/// Wrap `@id`s as a [`OneOrMany`] edge: absent for zero, a bare object for one (the producer
/// byte style [`OneOrMany::One`] serializes to), an array otherwise.
fn refs(ids: Vec<String>) -> OneOrMany<IriRef> {
    let mut ids = ids;
    match ids.len() {
        0 => OneOrMany::None,
        1 => OneOrMany::One(iri_ref(ids.remove(0))),
        _ => OneOrMany::Many(ids.into_iter().map(iri_ref).collect()),
    }
}
