//! Writing a validated export [`Plan`] as CXF JSON-LD bytes, one node at a time, into a buffer
//! of exactly the document's size (issue #803).
//!
//! The document is the root, then each block followed by its parameter nodes, then one port node
//! per connector, then the boundary-input and boundary-output nodes: the order the plan's vectors
//! give. The bytes are those [`crate::write_document`] writes for the whole
//! [`crate::CxfDocument`] of [`Node`](crate::Node)(crate::Node)s: the `{"@context":…,"@graph":[…]}` frame is written as
//! `serde_json`'s compact formatter writes a map and a sequence, and each node by a view
//! ([`RootNode`], [`BlockNode`], [`ParamNode`], [`PortNode`]) that names the keys a [`Node`](crate::Node) with
//! the same fields set would name, in its order, an empty edge left out and a one-element edge a
//! bare object. The tests check the bytes against the document built in full
//! (`export_write_tests.rs`).
//!
//! What it holds. Building every node first held a 1,120-byte [`Node`](crate::Node) for each parameter, more
//! while the node vector grew: a `MatrixGain` whose `K` is 128 by 128 exported 1.7 MB of bytes at a
//! peak of 58.7 MB. Now nothing is built for a node: each `@id` (a block's IRI, a parameter's
//! `<block>.<name>`, a port's own IRI or `<block>.in<k>`) is spelled straight into the output from
//! the graph through `collect_str`, a string parameter's CDL literal is escaped as it is written,
//! and the other bindings and attributes are numbers. The document is written twice, first only
//! to count its bytes and then into a buffer of exactly that size, so the buffer never grows: a
//! growing buffer held up to three times the bytes while it moved.

use std::fmt::{self, Display};
use std::io;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};
use oce_model::Value;
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

use crate::CxfError;
use crate::dto::CxfValue;
use crate::export_attrs::PortAttrs;
use crate::translation_software::LABEL;

use super::{
    EXPORT_ROOT_IRI, MSG_STRUCTURE, Plan, PlannedBlock, PlannedBoundary, PlannedBoundaryOutput,
    PortId, S231_CONTEXT_IRI,
};

#[cfg(test)]
#[path = "export_write_generated_tests.rs"]
mod generated_tests;
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
    // The two passes write the same bytes, so the buffer never grew.
    debug_assert_eq!(out.len(), size.0);
    debug_assert_eq!(out.capacity(), size.0);
    Ok(out)
}

/// Write the document for `plan` into `out`.
fn emit<W: io::Write>(plan: &Plan<'_>, out: &mut W) -> Result<(), CxfError> {
    out.write_all(b"{\"@context\":{\"S231\":")
        .map_err(serde_json::Error::io)?;
    serde_json::to_writer(&mut *out, S231_CONTEXT_IRI)?;
    out.write_all(b"},\"@graph\":[")
        .map_err(serde_json::Error::io)?;
    serde_json::to_writer(&mut *out, &RootNode(plan))?;

    for b in &plan.blocks {
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
        next_node(out, &node)?;
        for (name, value) in b.params {
            let node = ParamNode {
                id: ParamId {
                    block: &b.iri,
                    name,
                },
                value: Binding::of(value).ok_or_else(unbound)?,
            };
            next_node(out, &node)?;
        }
    }
    for port in &plan.ports {
        let node = PortNode {
            id: &port.iri,
            targets: &port.targets,
            datatype: port.datatype,
            attrs: &port.attrs,
        };
        next_node(out, &node)?;
    }
    for boundary in &plan.boundaries {
        let node = PortNode {
            id: &boundary.iri,
            targets: &boundary.targets,
            datatype: boundary.datatype,
            attrs: &boundary.attrs,
        };
        next_node(out, &node)?;
    }
    for output in &plan.boundary_outputs {
        let node = PortNode {
            id: &output.iri,
            targets: &[],
            datatype: output.datatype,
            attrs: &output.attrs,
        };
        next_node(out, &node)?;
    }

    // The document has no top-level key besides `@context` and `@graph`.
    out.write_all(b"]}").map_err(serde_json::Error::io)?;
    Ok(())
}

/// Write `node` as a further element of `@graph`: the root is always the first.
fn next_node<W: io::Write, N: Serialize>(out: &mut W, node: &N) -> Result<(), CxfError> {
    out.write_all(b",").map_err(serde_json::Error::io)?;
    serde_json::to_writer(&mut *out, node)?;
    Ok(())
}

/// The refusal of a planned parameter with no binding: unreachable, as the plan refuses every
/// value [`Binding::of`] has no binding for.
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

/// Something a reference edge names: the `@id` it spells.
trait Named {
    /// The `@id`.
    fn id(&self) -> &dyn Display;
}

impl Named for PortId<'_> {
    fn id(&self) -> &dyn Display {
        self
    }
}

impl Named for PlannedBlock<'_> {
    fn id(&self) -> &dyn Display {
        &self.iri
    }
}

impl Named for PlannedBoundary<'_> {
    fn id(&self) -> &dyn Display {
        &self.iri
    }
}

impl Named for PlannedBoundaryOutput<'_> {
    fn id(&self) -> &dyn Display {
        &self.iri
    }
}

/// An edge as a [`crate::OneOrMany`] of [`crate::IriRef`]s writes it: one `{"@id":…}` bare, more
/// as an array; an empty edge is left out by the node that holds it.
struct Refs<'a, T>(&'a [T]);

impl<T: Named> Serialize for Refs<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            [one] => IdRef(one.id()).serialize(serializer),
            many => {
                let mut seq = serializer.serialize_seq(Some(many.len()))?;
                for item in many {
                    seq.serialize_element(&IdRef(item.id()))?;
                }
                seq.end()
            }
        }
    }
}

/// A `{"@id":…}` reference, as an [`crate::IriRef`] with no other key writes it.
struct IdRef<T>(T);

impl<T: Display> Serialize for IdRef<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut reference = serializer.serialize_map(Some(1))?;
        reference.serialize_entry("@id", &Spelled(&self.0))?;
        reference.end()
    }
}

/// A value written as the one JSON string its [`Display`] spells, never built.
struct Spelled<T>(T);

impl<T: Display> Serialize for Spelled<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

/// The root composite as [`Node`](crate::Node) writes it with `@id`, `@type`, its three edges and the
/// translation-software label (`crate::translation_software::stamp`: two keys of the node's map of
/// other keys, after its fields, in that map's sorted order).
struct RootNode<'a, 'g>(&'a Plan<'g>);

impl Serialize for RootNode<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let plan = self.0;
        let mut node = serializer.serialize_map(None)?;
        node.serialize_entry("@id", EXPORT_ROOT_IRI)?;
        node.serialize_entry("@type", "S231:Block")?;
        if !plan.boundaries.is_empty() {
            node.serialize_entry("S231:hasInput", &Refs(&plan.boundaries))?;
        }
        if !plan.boundary_outputs.is_empty() {
            node.serialize_entry("S231:hasOutput", &Refs(&plan.boundary_outputs))?;
        }
        if !plan.blocks.is_empty() {
            node.serialize_entry("S231:containsBlock", &Refs(&plan.blocks))?;
        }
        for (key, value) in LABEL {
            node.serialize_entry(key, value)?;
        }
        node.end()
    }
}

/// A block node as [`Node`](crate::Node) writes it with `@id`, `@type` and the three edges set.
struct BlockNode<'a> {
    id: &'a str,
    r#type: &'a str,
    has_input: &'a [PortId<'a>],
    has_output: &'a [PortId<'a>],
    has_parameter: ParamRefs<'a>,
}

impl Serialize for BlockNode<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut node = serializer.serialize_map(None)?;
        node.serialize_entry("@id", self.id)?;
        node.serialize_entry("@type", self.r#type)?;
        if !self.has_input.is_empty() {
            node.serialize_entry("S231:hasInput", &Refs(self.has_input))?;
        }
        if !self.has_output.is_empty() {
            node.serialize_entry("S231:hasOutput", &Refs(self.has_output))?;
        }
        if !self.has_parameter.params.is_empty() {
            node.serialize_entry("S231:hasParameter", &self.has_parameter)?;
        }
        node.end()
    }
}

/// A block's `S231:hasParameter` edge, one `{"@id":"<block>.<name>"}` for each of its parameters.
struct ParamRefs<'a> {
    block: &'a str,
    params: &'a [(Arc<str>, Value)],
}

impl<'a> Serialize for ParamRefs<'a> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let id = |name: &'a Arc<str>| ParamId {
            block: self.block,
            name,
        };
        match self.params {
            [(name, _)] => IdRef(id(name)).serialize(serializer),
            many => {
                let mut seq = serializer.serialize_seq(Some(many.len()))?;
                for (name, _) in many {
                    seq.serialize_element(&IdRef(id(name)))?;
                }
                seq.end()
            }
        }
    }
}

/// A parameter node's `@id`, `<block>.<name>`.
#[derive(Clone, Copy)]
struct ParamId<'a> {
    block: &'a str,
    name: &'a str,
}

impl Display for ParamId<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.block)?;
        f.write_str(".")?;
        f.write_str(self.name)
    }
}

/// A parameter node as [`Node`](crate::Node) writes it with `@id` and `S231:value` set.
struct ParamNode<'a> {
    id: ParamId<'a>,
    value: Binding<'a>,
}

impl Serialize for ParamNode<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut node = serializer.serialize_map(None)?;
        node.serialize_entry("@id", &Spelled(self.id))?;
        node.serialize_entry("S231:value", &self.value)?;
        node.end()
    }
}

/// A parameter's binding as written: a bare literal, or a string's CDL literal (the bytes the
/// tests' reference `string_literal` builds), escaped as it is written.
enum Binding<'a> {
    /// A Real, Integer or Boolean literal.
    Literal(CxfValue),
    /// A string value, written as the CDL literal of the text.
    Text(&'a str),
}

impl<'a> Binding<'a> {
    /// The binding of `value`: `None` exactly where the plan's `bindable` refuses it.
    fn of(value: &'a Value) -> Option<Self> {
        match value {
            Value::Real(r) if r.is_finite() => Some(Binding::Literal(CxfValue::Float(*r))),
            Value::Integer(i) => Some(Binding::Literal(CxfValue::Int(*i))),
            Value::Boolean(b) => Some(Binding::Literal(CxfValue::Bool(*b))),
            Value::String(text) => Some(Binding::Text(text)),
            Value::Real(_) | Value::Enum { .. } => None,
        }
    }
}

impl Serialize for Binding<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Binding::Literal(value) => value.serialize(serializer),
            Binding::Text(text) => serializer.collect_str(&CdlLiteral(text)),
        }
    }
}

/// A CDL string literal of a text: `"`, the text with each `"` and `\` escaped by a `\`, `"`.
struct CdlLiteral<'a>(&'a str);

impl Display for CdlLiteral<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("\"")?;
        let mut rest = self.0;
        while let Some(at) = rest.find(['"', '\\']) {
            f.write_str(&rest[..at])?;
            f.write_str("\\")?;
            f.write_str(&rest[at..=at])?;
            rest = &rest[at + 1..];
        }
        f.write_str(rest)?;
        f.write_str("\"")
    }
}

/// A port, boundary-input or boundary-output node as [`Node`](crate::Node) writes it with `@id`,
/// `S231:isConnectedTo`, `S231:isOfDataType` and the classified attributes set: the
/// Bare-Scalar Canonical keys, in [`Node`](crate::Node)'s order.
struct PortNode<'a, I> {
    id: &'a I,
    targets: &'a [PortId<'a>],
    datatype: &'static str,
    attrs: &'a PortAttrs,
}

impl<I: Display> Serialize for PortNode<'_, I> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut node = serializer.serialize_map(None)?;
        node.serialize_entry("@id", &Spelled(self.id))?;
        if !self.targets.is_empty() {
            node.serialize_entry("S231:isConnectedTo", &Refs(self.targets))?;
        }
        node.serialize_entry("S231:isOfDataType", &IdRef(self.datatype))?;
        match self.attrs {
            PortAttrs::Real {
                unit,
                quantity,
                display_unit,
                min,
                max,
                nominal,
            } => {
                if let Some(min) = min {
                    node.serialize_entry("S231:min", &CxfValue::Float(*min))?;
                }
                if let Some(max) = max {
                    node.serialize_entry("S231:max", &CxfValue::Float(*max))?;
                }
                if let Some(nominal) = nominal {
                    node.serialize_entry("S231:nominal", &CxfValue::Float(*nominal))?;
                }
                if let Some(unit) = unit {
                    node.serialize_entry("S231:unit", &**unit)?;
                }
                if let Some(quantity) = quantity {
                    node.serialize_entry("S231:quantity", &**quantity)?;
                }
                if let Some(display_unit) = display_unit {
                    node.serialize_entry("S231:displayUnit", &**display_unit)?;
                }
            }
            PortAttrs::Integer { min, max } => {
                if let Some(min) = min {
                    node.serialize_entry("S231:min", &CxfValue::Int(*min))?;
                }
                if let Some(max) = max {
                    node.serialize_entry("S231:max", &CxfValue::Int(*max))?;
                }
            }
            PortAttrs::None => {}
        }
        node.end()
    }
}
