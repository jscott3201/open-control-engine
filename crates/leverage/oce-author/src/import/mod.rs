//! A minimal CXF importer for the flat dialect (authoring note, "Library round trip"; gate item
//! 1), the inverse of [`crate::emit()`] over the authoring subset.
//!
//! [`import_cxf`] reads one flat, single-root document — leaf blocks, boundary inputs and
//! outputs, sequence and block parameters (array parameters included), connections, and the
//! boundary `unit`, `quantity` and `displayUnit` attributes — into a [`Sequence`] in the
//! document's own order: blocks in `containsBlock` order, boundaries in `hasInput` / `hasOutput`
//! order, assignments in `hasParameter` order (array elements row-major), and connections by
//! source node in `@graph` order with each source's targets in array order. Everything else is
//! refused with a typed [`ImportError`], never guessed and never a panic: nested composites,
//! `hasInstance`, `hasConstant`, conditional components, boundary or port `min` / `max`, library
//! enumerations, expressions, a value whose shape disagrees with its `isOfDataType`, a parameter
//! `@type` other than `S231:Parameter`, and any property the subset does not carry. The JSON is
//! read strictly: a key written twice or an integer outside `i64` is refused, not resolved.
//!
//! Deliberately ignored (read, never carried into the model): `S231:label` everywhere; a
//! parameter's descriptive `unit`, `quantity` and `displayUnit`, which the model has no place for;
//! and the root's `@type` (the corpus names a library class there; the emitter writes
//! `S231:Block`). The full importer (library CXF and the
//! prototype's v0 JSON, with its own design note) is a later M2 item; this one is sized to the
//! flat corpus.

mod nodes;
mod strict;
mod values;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use oce_api::{MAX_CXF_BYTES, param_element_name};
use serde_json::{Map, Value};

use crate::identifier::Identifier;
use crate::model::{
    Assignment, Block, Boundary, Connection, Endpoint, Sequence, SequenceParameter,
};
use crate::validate::catalog::{is_template, lookup_class};

use nodes::{
    BLOCK_KEYS, BOUNDARY_KEYS, Names, PARAM_KEYS, PORT_KEYS, ROOT_KEYS, S231_IRI, check_keys,
    check_parameter_type, check_value_shapes, connector_type, data_type, references, sequence_name,
    text_attribute,
};

/// Why [`import_cxf`] refused a document. Never a panic.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportError {
    /// The document is larger than [`oce_api::MAX_CXF_BYTES`].
    TooLarge {
        /// The document size in bytes.
        bytes: usize,
        /// The limit.
        limit: usize,
    },
    /// The bytes are not a JSON-LD document of the expected shape.
    Malformed {
        /// What is wrong.
        detail: String,
    },
    /// The document uses a construct outside the authoring subset.
    Unsupported {
        /// The `@id` (or `document`) the construct sits on.
        subject: String,
        /// The construct.
        construct: String,
    },
    /// A reference names no node, or a node is not under the root's IRI.
    Unresolved {
        /// The node holding the reference.
        subject: String,
        /// The IRI that does not resolve.
        reference: String,
    },
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes, limit } => {
                write!(f, "the document is {bytes} bytes; at most {limit} are read")
            }
            Self::Malformed { detail } => write!(f, "malformed document: {detail}"),
            Self::Unsupported { subject, construct } => {
                write!(
                    f,
                    "`{subject}` uses {construct}, outside the authoring subset"
                )
            }
            Self::Unresolved { subject, reference } => {
                write!(
                    f,
                    "`{subject}` references `{reference}`, which does not resolve"
                )
            }
        }
    }
}

impl std::error::Error for ImportError {}

pub(crate) fn unsupported(subject: &str, construct: &str) -> ImportError {
    ImportError::Unsupported {
        subject: subject.to_owned(),
        construct: construct.to_owned(),
    }
}

pub(super) fn malformed(detail: impl Into<String>) -> ImportError {
    ImportError::Malformed {
        detail: detail.into(),
    }
}

pub(super) fn unresolved(subject: &str, reference: &str) -> ImportError {
    ImportError::Unresolved {
        subject: subject.to_owned(),
        reference: reference.to_owned(),
    }
}

/// Read one flat CXF document into the authoring model.
///
/// # Errors
///
/// [`ImportError::TooLarge`] above [`oce_api::MAX_CXF_BYTES`] (checked before parsing),
/// [`ImportError::Malformed`] for bytes that are not a document of the CXF shape,
/// [`ImportError::Unsupported`] for a construct outside the authoring subset, and
/// [`ImportError::Unresolved`] for a dangling reference. Never panics.
pub fn import_cxf(bytes: &[u8]) -> Result<Sequence, ImportError> {
    if bytes.len() > MAX_CXF_BYTES {
        return Err(ImportError::TooLarge {
            bytes: bytes.len(),
            limit: MAX_CXF_BYTES,
        });
    }
    let document: Value =
        strict::parse(bytes).map_err(|e| malformed(format!("not strict JSON: {e}")))?;
    let Value::Object(top) = &document else {
        return Err(malformed("the document is not an object"));
    };
    check_context(top)?;
    let Some(Value::Array(graph)) = top.get("@graph") else {
        return Err(malformed("the document has no `@graph` array"));
    };
    let mut nodes: BTreeMap<&str, &Map<String, Value>> = BTreeMap::new();
    let mut order: Vec<&str> = Vec::new();
    for node in graph {
        let Value::Object(map) = node else {
            return Err(malformed("a `@graph` entry is not an object"));
        };
        let Some(Value::String(id)) = map.get("@id") else {
            return Err(malformed("a `@graph` node has no string `@id`"));
        };
        if nodes.insert(id.as_str(), map).is_some() {
            return Err(malformed(format!("`{id}` is declared twice")));
        }
        order.push(id.as_str());
    }
    let roots: Vec<&str> = order
        .iter()
        .copied()
        .filter(|id| {
            nodes
                .get(id)
                .is_some_and(|n| n.contains_key("S231:containsBlock"))
        })
        .collect();
    let root_id = match roots.as_slice() {
        [one] => *one,
        [] => return Err(malformed("no node contains blocks; there is no root")),
        [_, second, ..] => return Err(unsupported(second, "a nested or second composite")),
    };
    Reader::new(&nodes, root_id)?.read(&order)
}

/// Accept only the `S231` binding the engine reads (and the corpus's inert `base`).
fn check_context(top: &Map<String, Value>) -> Result<(), ImportError> {
    if let Some(key) = top.keys().find(|k| *k != "@context" && *k != "@graph") {
        return Err(unsupported(
            "document",
            &format!("the top-level key `{key}`"),
        ));
    }
    match top.get("@context") {
        None => Ok(()),
        Some(Value::Object(context)) => {
            for (term, value) in context {
                match (term.as_str(), value) {
                    ("S231", Value::String(iri)) if iri == S231_IRI => {}
                    ("base", Value::String(_)) => {}
                    _ => {
                        return Err(unsupported(
                            "document",
                            &format!("the `@context` term `{term}`"),
                        ));
                    }
                }
            }
            Ok(())
        }
        Some(_) => Err(unsupported(
            "document",
            "a `@context` that is not an object",
        )),
    }
}

/// One import in progress.
struct Reader<'a> {
    nodes: &'a BTreeMap<&'a str, &'a Map<String, Value>>,
    root_id: &'a str,
    root: &'a Map<String, Value>,
    names: Names<'a>,
    /// Every node the subset accounts for; any other node is refused.
    claimed: BTreeSet<&'a str>,
}

impl<'a> Reader<'a> {
    fn new(
        nodes: &'a BTreeMap<&'a str, &'a Map<String, Value>>,
        root_id: &'a str,
    ) -> Result<Self, ImportError> {
        let root = nodes
            .get(root_id)
            .copied()
            .ok_or_else(|| malformed("root vanished"))?;
        Ok(Self {
            nodes,
            root_id,
            root,
            names: Names { root: root_id },
            claimed: BTreeSet::from([root_id]),
        })
    }

    fn node(&mut self, holder: &str, iri: &'a str) -> Result<&'a Map<String, Value>, ImportError> {
        let node = self
            .nodes
            .get(iri)
            .copied()
            .ok_or_else(|| unresolved(holder, iri))?;
        if !self.claimed.insert(iri) {
            return Err(unsupported(iri, "a node referenced twice"));
        }
        Ok(node)
    }

    fn read(mut self, order: &[&'a str]) -> Result<Sequence, ImportError> {
        check_keys(self.root_id, self.root, &ROOT_KEYS)?;
        // The root's `@type` is read and ignored (the corpus names a library class there; the
        // emitter always writes `S231:Block`), but it must be one string.
        if !matches!(self.root.get("@type"), None | Some(Value::String(_))) {
            return Err(malformed(format!(
                "`{}`: the root `@type` is not one string",
                self.root_id
            )));
        }
        let name = sequence_name(self.root_id)?;
        let mut sequence = Sequence::new(Identifier::unchecked(name));
        for iri in references(self.root_id, self.root, "S231:hasParameter")? {
            let node = self.node(self.root_id, iri)?;
            sequence.params.push(self.sequence_parameter(iri, node)?);
        }
        let params: Vec<Identifier> = sequence.params.iter().map(|p| p.name.clone()).collect();
        for (key, outputs) in [("S231:hasInput", false), ("S231:hasOutput", true)] {
            for iri in references(self.root_id, self.root, key)? {
                let node = self.node(self.root_id, iri)?;
                let boundary = self.boundary(iri, node, outputs)?;
                if outputs {
                    sequence.outputs.push(boundary);
                } else {
                    sequence.inputs.push(boundary);
                }
            }
        }
        for iri in references(self.root_id, self.root, "S231:containsBlock")? {
            let node = self.node(self.root_id, iri)?;
            sequence.blocks.push(self.block(iri, node, &params)?);
        }
        if let Some(stray) = order.iter().find(|id| !self.claimed.contains(**id)) {
            return Err(unsupported(stray, "a node outside the root's relations"));
        }
        for &source in order {
            let Some(node) = self.nodes.get(source) else {
                continue;
            };
            for target in references(source, node, "S231:isConnectedTo")? {
                if !self.nodes.contains_key(target) {
                    return Err(unresolved(source, target));
                }
                sequence.connections.push(Connection::new(
                    self.endpoint(source, source)?,
                    self.endpoint(source, target)?,
                ));
            }
        }
        Ok(sequence)
    }

    fn endpoint(&self, holder: &str, iri: &'a str) -> Result<Endpoint, ImportError> {
        match self.names.local(holder, iri)?.as_slice() {
            [name] => Ok(Endpoint::Boundary(Identifier::unchecked(*name))),
            [block, port] => Ok(Endpoint::Port {
                block: Identifier::unchecked(*block),
                port: Identifier::unchecked(*port),
            }),
            _ => Err(unsupported(iri, "a connection endpoint below a block port")),
        }
    }

    fn single_local(&self, iri: &'a str) -> Result<&'a str, ImportError> {
        match self.names.local(self.root_id, iri)?.as_slice() {
            [name] => Ok(name),
            _ => Err(unsupported(
                iri,
                "a root member that is not named directly under the root",
            )),
        }
    }

    fn sequence_parameter(
        &self,
        iri: &'a str,
        node: &Map<String, Value>,
    ) -> Result<SequenceParameter, ImportError> {
        check_keys(iri, node, &PARAM_KEYS)?;
        check_parameter_type(iri, node)?;
        if node.contains_key("S231:isArray") {
            return Err(unsupported(iri, "an array sequence parameter"));
        }
        let value_type = data_type(iri, node)?.ok_or_else(|| {
            unsupported(
                iri,
                "a sequence parameter that is not Real, Integer or Boolean",
            )
        })?;
        let value = node
            .get("S231:value")
            .ok_or_else(|| unsupported(iri, "a sequence parameter without a value"))?;
        let literal = values::sequence_literal(iri, value_type, value)?;
        Ok(SequenceParameter::new(
            Identifier::unchecked(self.single_local(iri)?),
            value_type,
            literal,
        ))
    }

    fn boundary(
        &self,
        iri: &'a str,
        node: &Map<String, Value>,
        output: bool,
    ) -> Result<Boundary, ImportError> {
        check_keys(iri, node, &BOUNDARY_KEYS)?;
        let value_type = connector_type(iri, node, if output { "Output" } else { "Input" })?;
        if output && node.contains_key("S231:isConnectedTo") {
            return Err(unsupported(
                iri,
                "a boundary output that drives a connection",
            ));
        }
        let mut boundary =
            Boundary::new(Identifier::unchecked(self.single_local(iri)?), value_type);
        boundary.semantic.unit = text_attribute(iri, node, "S231:unit")?.map(str::to_owned);
        boundary.quantity = text_attribute(iri, node, "S231:quantity")?.map(str::to_owned);
        boundary.display_unit = text_attribute(iri, node, "S231:displayUnit")?.map(str::to_owned);
        Ok(boundary)
    }

    fn block(
        &mut self,
        iri: &'a str,
        node: &'a Map<String, Value>,
        params: &[Identifier],
    ) -> Result<Block, ImportError> {
        check_keys(iri, node, &BLOCK_KEYS)?;
        let name = self.single_local(iri)?;
        let class_path = match node.get("@type") {
            Some(Value::String(class)) => class
                .rsplit_once('#')
                .and_then(|(_, fragment)| fragment.strip_prefix("Buildings.Controls.OBC."))
                .ok_or_else(|| {
                    unsupported(iri, "a block class outside `Buildings.Controls.OBC`")
                })?,
            _ => return Err(malformed(format!("`{iri}`: a block has no string `@type`"))),
        };
        let mut block = Block::new(Identifier::unchecked(name), class_path);
        for param_iri in references(iri, node, "S231:hasParameter")? {
            let param = self.node(iri, param_iri)?;
            let local = self.member(iri, name, param_iri)?;
            block.assignments.extend(self.assignments(
                param_iri,
                param,
                &block.class_path,
                local,
                params,
            )?);
        }
        for (key, side) in [("S231:hasInput", "Input"), ("S231:hasOutput", "Output")] {
            for port_iri in references(iri, node, key)? {
                let port = self.node(iri, port_iri)?;
                self.member(iri, name, port_iri)?;
                check_keys(port_iri, port, &PORT_KEYS)?;
                connector_type(port_iri, port, side)?;
                if side == "Input" && port.contains_key("S231:isConnectedTo") {
                    return Err(unsupported(
                        port_iri,
                        "a block input that drives a connection",
                    ));
                }
            }
        }
        Ok(block)
    }

    /// The member name of `block.member`.
    fn member(&self, holder: &str, block: &str, iri: &'a str) -> Result<&'a str, ImportError> {
        match self.names.local(holder, iri)?.as_slice() {
            [owner, member] if *owner == block => Ok(member),
            _ => Err(unsupported(iri, "a block member not named `block.member`")),
        }
    }

    fn assignments(
        &self,
        iri: &str,
        node: &Map<String, Value>,
        class_path: &str,
        local: &str,
        params: &[Identifier],
    ) -> Result<Vec<Assignment>, ImportError> {
        check_keys(iri, node, &PARAM_KEYS)?;
        check_parameter_type(iri, node)?;
        let read = self.read_assignments(iri, node, class_path, local, params)?;
        check_value_shapes(iri, node, &read)?;
        Ok(read)
    }

    fn read_assignments(
        &self,
        iri: &str,
        node: &Map<String, Value>,
        class_path: &str,
        local: &str,
        params: &[Identifier],
    ) -> Result<Vec<Assignment>, ImportError> {
        let value = node
            .get("S231:value")
            .ok_or_else(|| unsupported(iri, "a block parameter without a value"))?;
        if node.get("S231:isArray") != Some(&Value::Bool(true)) {
            if node.contains_key("S231:numberDimensions")
                || node.contains_key("S231:sizeOfDimensions")
            {
                return Err(unsupported(iri, "array dimensions on a scalar parameter"));
            }
            let value = values::param_value(iri, value, params)?;
            return Ok(vec![Assignment::new(Identifier::unchecked(local), value)]);
        }
        let Some(Value::String(spelled)) = node.get("S231:sizeOfDimensions") else {
            return Err(unsupported(
                iri,
                "an array parameter without `sizeOfDimensions`",
            ));
        };
        let dims = values::dimensions(iri, spelled)?;
        let rank_ok = matches!(node.get("S231:numberDimensions"), Some(Value::Number(n)) if n.as_u64() == u64::try_from(dims.len()).ok());
        if !rank_ok {
            return Err(unsupported(
                iri,
                "an array whose `numberDimensions` disagrees with its sizes",
            ));
        }
        let Value::Array(items) = value else {
            return Err(unsupported(
                iri,
                "an array parameter whose value is not a list",
            ));
        };
        // Checked: hostile sizes overflow `usize` (and wrap to 0 in release).
        let total = dims.iter().try_fold(1usize, |acc, &d| acc.checked_mul(d));
        if total != Some(items.len()) {
            return Err(unsupported(
                iri,
                "an array whose element count disagrees with its sizes",
            ));
        }
        let template = lookup_class(class_path)
            .and_then(|class| {
                class.metadata.parameters.iter().find(|p| {
                    is_template(p.name)
                        && p.name
                            .split_once("_<")
                            .is_some_and(|(base, _)| base == local)
                })
            })
            .map(|p| p.name)
            .ok_or_else(|| {
                unsupported(iri, "an array parameter the catalog has no template for")
            })?;
        items
            .iter()
            .enumerate()
            .map(|(flat, item)| {
                let indices: Vec<usize> = match dims.as_slice() {
                    [_] => vec![flat + 1],
                    [_, cols] => vec![flat / cols + 1, flat % cols + 1],
                    _ => Vec::new(),
                };
                let name = param_element_name(template, &indices).ok_or_else(|| {
                    unsupported(iri, "an array whose rank differs from the catalog's")
                })?;
                let value = values::param_value(iri, item, params)?;
                Ok(Assignment::new(Identifier::unchecked(name), value))
            })
            .collect()
    }
}
