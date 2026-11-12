//! The CXF importer (importer note, "The CXF importer"): one flat, single-root document into the
//! model, in the document's own order.
//!
//! Blocks come in `containsBlock` order, boundaries in `hasInput` / `hasOutput` order,
//! assignments in `hasParameter` order (array elements row-major under the catalog's template
//! names), and connections by source node in `@graph` order with each source's targets in array
//! order. Sequence parameters may be Real, Integer, Boolean or an enumeration of the facade
//! table; boundaries carry `unit`, `quantity`, `displayUnit`, `min` and `max`; block values may be
//! members of an enumeration or of a library constant package. Names are carried, never
//! repaired (decision 13): a part that breaks the identifier syntax is refused, a reserved word
//! or a duplicate is left for the validator.

use std::collections::{BTreeMap, BTreeSet};

use oce_api::{EnumerationKind, MAX_CXF_BYTES, param_element_name};
use serde_json::{Map, Value};

use crate::identifier::Identifier;
use crate::model::{
    Assignment, Block, Boundary, Connection, Endpoint, ParameterType, Sequence, SequenceParameter,
};
use crate::validate::catalog::{is_template, lookup_class};

use super::error::{ImportError, ImportErrorKind as Kind, SourceForm};
use super::nodes::{
    DataType, Names, NodeKind, OBC_PACKAGE, PARAMETER_ATTRIBUTES, RETIRED_S231_IRI, S231_IRI,
    check_keys, check_parameter_attribute, check_parameter_type, connector_type, data_type,
    declared_type, references, refuse, sequence_name, spelled, term_attribute, text_attribute,
};
use super::precedence::{self, Graph};
use super::report::{Dropped, DroppedProperty, ImportOptions, ImportReport, Imported, Normalised};
use super::strict;
use super::values;

/// Read one flat CXF document (see [`crate::import_cxf`]).
pub(super) fn import(bytes: &[u8], options: &ImportOptions) -> Result<Imported, ImportError> {
    if bytes.len() > MAX_CXF_BYTES {
        return Err(refuse(
            "document",
            Kind::TooLarge {
                bytes: bytes.len(),
                limit: MAX_CXF_BYTES,
            },
        ));
    }
    let document = strict::parse(bytes).map_err(|kind| refuse("document", kind))?;
    let Value::Object(top) = &document else {
        return Err(refuse(
            "document",
            Kind::WrongShape {
                key: "document".to_owned(),
            },
        ));
    };
    check_context(top)?;
    let graph = match top.get("@graph") {
        Some(Value::Array(graph)) => graph,
        Some(_) => {
            return Err(refuse(
                "document",
                Kind::WrongShape {
                    key: "@graph".to_owned(),
                },
            ));
        }
        None => {
            return Err(refuse(
                "document",
                Kind::MissingKey {
                    key: "@graph".to_owned(),
                },
            ));
        }
    };
    let mut nodes: BTreeMap<&str, &Map<String, Value>> = BTreeMap::new();
    let mut order: Vec<&str> = Vec::new();
    for node in graph {
        let Value::Object(map) = node else {
            return Err(refuse(
                "document",
                Kind::WrongShape {
                    key: "@graph".to_owned(),
                },
            ));
        };
        let id = match map.get("@id") {
            Some(Value::String(id)) => id.as_str(),
            Some(_) => {
                return Err(refuse(
                    "document",
                    Kind::WrongShape {
                        key: "@id".to_owned(),
                    },
                ));
            }
            None => {
                return Err(refuse(
                    "document",
                    Kind::MissingKey {
                        key: "@id".to_owned(),
                    },
                ));
            }
        };
        if nodes.insert(id, map).is_some() {
            return Err(refuse(id, Kind::DuplicateId));
        }
        order.push(id);
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
        [] => return Err(refuse("document", Kind::NoRoot)),
        [_, second, ..] => return Err(refuse(second, Kind::NestedComposite)),
    };
    let graph = Graph { nodes, order };
    precedence::check(&graph, root_id)?;
    let mut imported = Reader::new(&graph, root_id)?.read()?;
    imported
        .sequence
        .derived_from
        .clone_from(&options.derived_from);
    Ok(imported)
}

/// Accept only the `S231` binding the engine reads (and the corpus's inert `base`). `S231` bound
/// to any other IRI, and any term bound to the namespace written through release 0.4.0, is a
/// [`Kind::NamespaceMismatch`] naming the IRI found and the IRI expected; any other term is an
/// unknown key.
fn check_context(top: &Map<String, Value>) -> Result<(), ImportError> {
    if let Some(key) = top.keys().find(|k| *k != "@context" && *k != "@graph") {
        return Err(refuse("document", Kind::UnknownKey { key: key.clone() }));
    }
    match top.get("@context") {
        None => Ok(()),
        Some(Value::Object(context)) => {
            for (term, value) in context {
                match (term.as_str(), value) {
                    ("S231", Value::String(iri)) if iri == S231_IRI => {}
                    ("S231", Value::String(iri)) => return Err(wrong_namespace(term, iri)),
                    (_, Value::String(iri)) if iri == RETIRED_S231_IRI => {
                        return Err(wrong_namespace(term, iri));
                    }
                    ("base", Value::String(_)) => {}
                    _ => return Err(refuse("document", Kind::UnknownKey { key: term.clone() })),
                }
            }
            Ok(())
        }
        Some(_) => Err(refuse(
            "document",
            Kind::WrongShape {
                key: "@context".to_owned(),
            },
        )),
    }
}

/// The refusal for a `@context` term that spells the S231 vocabulary in another namespace.
fn wrong_namespace(term: &str, found: &str) -> ImportError {
    refuse(
        "document",
        Kind::NamespaceMismatch {
            term: term.to_owned(),
            found: found.to_owned(),
            expected: S231_IRI,
        },
    )
}

/// One import in progress.
struct Reader<'a> {
    nodes: &'a BTreeMap<&'a str, &'a Map<String, Value>>,
    order: &'a [&'a str],
    root_id: &'a str,
    root: &'a Map<String, Value>,
    names: Names<'a>,
    /// Every node the subset accounts for; any other node is refused.
    claimed: BTreeSet<&'a str>,
    report: ImportReport,
}

impl<'a> Reader<'a> {
    fn new(graph: &'a Graph<'a>, root_id: &'a str) -> Result<Self, ImportError> {
        let root = graph
            .nodes
            .get(root_id)
            .copied()
            .ok_or_else(|| refuse(root_id, Kind::NoRoot))?;
        Ok(Self {
            nodes: &graph.nodes,
            order: &graph.order,
            root_id,
            root,
            names: Names { root: root_id },
            claimed: BTreeSet::from([root_id]),
            report: ImportReport::new(SourceForm::Cxf),
        })
    }

    fn node(&mut self, holder: &str, iri: &'a str) -> Result<&'a Map<String, Value>, ImportError> {
        let node = self.nodes.get(iri).copied().ok_or_else(|| {
            refuse(
                holder,
                Kind::Unresolved {
                    reference: iri.to_owned(),
                },
            )
        })?;
        if !self.claimed.insert(iri) {
            return Err(refuse(iri, Kind::StrayNode));
        }
        Ok(node)
    }

    fn drop(&mut self, subject: &str, property: DroppedProperty, value: &Value) {
        self.report.dropped.push(Dropped {
            subject: subject.to_owned(),
            property,
            value: spelled(value),
        });
    }

    /// Report a label that differs from the node's local name.
    fn label(
        &mut self,
        iri: &str,
        node: &Map<String, Value>,
        local: &str,
    ) -> Result<(), ImportError> {
        if let Some(label) = text_attribute(iri, node, "S231:label")?
            && label != local
        {
            self.report.dropped.push(Dropped {
                subject: iri.to_owned(),
                property: DroppedProperty::Label,
                value: label.to_owned(),
            });
        }
        Ok(())
    }

    fn read(mut self) -> Result<Imported, ImportError> {
        check_keys(self.root_id, self.root, NodeKind::Root)?;
        let name = sequence_name(self.root_id)?;
        match self.root.get("@type") {
            None => {}
            Some(Value::String(t)) if t == "S231:Block" => {}
            Some(value @ Value::String(_)) => {
                self.drop(self.root_id, DroppedProperty::RootType, value);
            }
            Some(_) => {
                return Err(refuse(
                    self.root_id,
                    Kind::WrongShape {
                        key: "@type".to_owned(),
                    },
                ));
            }
        }
        self.label(self.root_id, self.root, name)?;
        let mut sequence = Sequence::new(Identifier::unchecked(name));
        for iri in references(self.root_id, self.root, "S231:hasParameter")? {
            let node = self.node(self.root_id, iri)?;
            let param = self.sequence_parameter(iri, node)?;
            sequence.params.push(param);
        }
        let params: Vec<Identifier> = sequence.params.iter().map(|p| p.name.clone()).collect();
        for (key, output) in [("S231:hasInput", false), ("S231:hasOutput", true)] {
            for iri in references(self.root_id, self.root, key)? {
                let node = self.node(self.root_id, iri)?;
                let boundary = self.boundary(iri, node, output)?;
                if output {
                    sequence.outputs.push(boundary);
                } else {
                    sequence.inputs.push(boundary);
                }
            }
        }
        for iri in references(self.root_id, self.root, "S231:containsBlock")? {
            let node = self.node(self.root_id, iri)?;
            let block = self.block(iri, node, &params)?;
            sequence.blocks.push(block);
        }
        if let Some(stray) = self.order.iter().find(|id| !self.claimed.contains(**id)) {
            return Err(refuse(stray, Kind::StrayNode));
        }
        for &source in self.order {
            let Some(node) = self.nodes.get(source) else {
                continue;
            };
            for target in references(source, node, "S231:isConnectedTo")? {
                if !self.nodes.contains_key(target) {
                    return Err(refuse(
                        source,
                        Kind::Unresolved {
                            reference: target.to_owned(),
                        },
                    ));
                }
                sequence.connections.push(Connection::new(
                    self.endpoint(source, source)?,
                    self.endpoint(source, target)?,
                ));
            }
        }
        Ok(Imported {
            sequence,
            report: self.report,
        })
    }

    fn endpoint(&self, holder: &str, iri: &'a str) -> Result<Endpoint, ImportError> {
        match self.names.local(holder, iri)?.as_slice() {
            [name] => Ok(Endpoint::Boundary(Identifier::unchecked(*name))),
            [block, port] => Ok(Endpoint::Port {
                block: Identifier::unchecked(*block),
                port: Identifier::unchecked(*port),
            }),
            _ => Err(refuse(iri, Kind::InvalidName)),
        }
    }

    fn single_local(&self, iri: &'a str) -> Result<&'a str, ImportError> {
        match self.names.local(self.root_id, iri)?.as_slice() {
            [name] => Ok(name),
            _ => Err(refuse(iri, Kind::InvalidName)),
        }
    }

    /// Check the shape of a parameter node's descriptive attributes, then drop and report them
    /// (decision 9; the shape check is GitHub issue #559).
    fn parameter_attributes(
        &mut self,
        iri: &str,
        node: &Map<String, Value>,
    ) -> Result<(), ImportError> {
        let properties = [
            DroppedProperty::Unit,
            DroppedProperty::Quantity,
            DroppedProperty::DisplayUnit,
            DroppedProperty::Min,
            DroppedProperty::Max,
        ];
        for (key, property) in PARAMETER_ATTRIBUTES.iter().zip(properties) {
            if let Some(value) = node.get(*key) {
                check_parameter_attribute(iri, key, value)?;
                self.drop(iri, property, value);
            }
        }
        Ok(())
    }

    fn sequence_parameter(
        &mut self,
        iri: &'a str,
        node: &Map<String, Value>,
    ) -> Result<SequenceParameter, ImportError> {
        check_keys(iri, node, NodeKind::SequenceParameter)?;
        check_parameter_type(iri, node)?;
        let local = self.single_local(iri)?;
        let declared = declared_type(iri, node)?.ok_or_else(|| {
            refuse(
                iri,
                Kind::MissingKey {
                    key: "S231:isOfDataType".to_owned(),
                },
            )
        })?;
        let (value_type, package) = match data_type(iri, declared)? {
            DataType::Scalar(signal) => (ParameterType::from(signal), None),
            DataType::Named(row) if row.kind == EnumerationKind::Enumeration => {
                (ParameterType::Enum(row.type_name().to_owned()), None)
            }
            DataType::Named(row) => (ParameterType::Integer, Some(row)),
            // The model has no String sequence parameter.
            DataType::String => return Err(refuse(iri, Kind::UnknownDatatype)),
        };
        let value = node.get("S231:value").ok_or_else(|| {
            refuse(
                iri,
                Kind::MissingKey {
                    key: "S231:value".to_owned(),
                },
            )
        })?;
        let literal = values::sequence_literal(iri, &value_type, package, value)?;
        self.label(iri, node, local)?;
        self.parameter_attributes(iri, node)?;
        Ok(SequenceParameter::new(
            Identifier::unchecked(local),
            value_type,
            literal,
        ))
    }

    fn boundary(
        &mut self,
        iri: &'a str,
        node: &'a Map<String, Value>,
        output: bool,
    ) -> Result<Boundary, ImportError> {
        check_keys(iri, node, NodeKind::Boundary)?;
        let local = self.single_local(iri)?;
        let typed = connector_type(iri, node, if output { "Output" } else { "Input" })?;
        if output && node.contains_key("S231:isConnectedTo") {
            return Err(refuse(iri, Kind::DrivingSink));
        }
        let mut boundary = Boundary::new(Identifier::unchecked(local), typed.signal);
        boundary.semantic.unit = term_attribute(iri, node, "S231:unit")?.map(str::to_owned);
        boundary.quantity = term_attribute(iri, node, "S231:quantity")?.map(str::to_owned);
        boundary.display_unit = term_attribute(iri, node, "S231:displayUnit")?.map(str::to_owned);
        if let Some(value) = node.get("S231:min") {
            boundary.min = Some(values::bound(iri, "S231:min", typed.signal, value)?);
        }
        if let Some(value) = node.get("S231:max") {
            boundary.max = Some(values::bound(iri, "S231:max", typed.signal, value)?);
        }
        self.normalised(iri, typed.package, typed.signal);
        self.label(iri, node, local)?;
        Ok(boundary)
    }

    fn normalised(&mut self, iri: &str, package: Option<&str>, signal: crate::model::SignalType) {
        if let Some(package) = package {
            self.report.normalised.push(Normalised {
                subject: iri.to_owned(),
                data_type: package.to_owned(),
                to: signal,
            });
        }
    }

    fn block(
        &mut self,
        iri: &'a str,
        node: &'a Map<String, Value>,
        params: &[Identifier],
    ) -> Result<Block, ImportError> {
        check_keys(iri, node, NodeKind::Block)?;
        let name = self.single_local(iri)?;
        let class_path = match node.get("@type") {
            Some(Value::String(class)) => class
                .rsplit_once('#')
                .and_then(|(_, fragment)| fragment.strip_prefix(OBC_PACKAGE))
                .ok_or_else(|| refuse(iri, Kind::ClassOutsideObc))?,
            Some(_) => {
                return Err(refuse(
                    iri,
                    Kind::WrongShape {
                        key: "@type".to_owned(),
                    },
                ));
            }
            None => {
                return Err(refuse(
                    iri,
                    Kind::MissingKey {
                        key: "@type".to_owned(),
                    },
                ));
            }
        };
        self.label(iri, node, name)?;
        let mut block = Block::new(Identifier::unchecked(name), class_path);
        for param_iri in references(iri, node, "S231:hasParameter")? {
            let param = self.node(iri, param_iri)?;
            let local = self.member(iri, name, param_iri)?;
            let read = self.assignments(param_iri, param, &block.class_path, local, params)?;
            block.assignments.extend(read);
        }
        for (key, side) in [("S231:hasInput", "Input"), ("S231:hasOutput", "Output")] {
            for port_iri in references(iri, node, key)? {
                let port = self.node(iri, port_iri)?;
                let local = self.member(iri, name, port_iri)?;
                check_keys(port_iri, port, NodeKind::Port)?;
                let typed = connector_type(port_iri, port, side)?;
                if side == "Input" && port.contains_key("S231:isConnectedTo") {
                    return Err(refuse(port_iri, Kind::DrivingSink));
                }
                self.normalised(port_iri, typed.package, typed.signal);
                self.label(port_iri, port, local)?;
            }
        }
        Ok(block)
    }

    /// The member name of `block.member`.
    fn member(&self, holder: &str, block: &str, iri: &'a str) -> Result<&'a str, ImportError> {
        match self.names.local(holder, iri)?.as_slice() {
            [owner, member] if *owner == block => Ok(member),
            _ => Err(refuse(iri, Kind::InvalidName)),
        }
    }

    fn assignments(
        &mut self,
        iri: &str,
        node: &Map<String, Value>,
        class_path: &str,
        local: &str,
        params: &[Identifier],
    ) -> Result<Vec<Assignment>, ImportError> {
        check_keys(iri, node, NodeKind::BlockParameter)?;
        check_parameter_type(iri, node)?;
        let read = read_assignments(iri, node, class_path, local, params)?;
        values::check_value_shapes(iri, node, &read)?;
        self.label(iri, node, local)?;
        self.parameter_attributes(iri, node)?;
        Ok(read)
    }
}

/// The assignments one block parameter node holds: one for a scalar, one per element for an
/// array, named by the catalog template (`k_2`, `K_1_2`).
fn read_assignments(
    iri: &str,
    node: &Map<String, Value>,
    class_path: &str,
    local: &str,
    params: &[Identifier],
) -> Result<Vec<Assignment>, ImportError> {
    let shape = || refuse(iri, Kind::ArrayShape);
    let value = node.get("S231:value").ok_or_else(|| {
        refuse(
            iri,
            Kind::MissingKey {
                key: "S231:value".to_owned(),
            },
        )
    })?;
    match node.get("S231:isArray") {
        None | Some(Value::Bool(false)) => {
            if node.contains_key("S231:numberDimensions")
                || node.contains_key("S231:sizeOfDimensions")
            {
                return Err(shape());
            }
            let value = values::param_value(iri, value, params)?;
            return Ok(vec![Assignment::new(Identifier::unchecked(local), value)]);
        }
        Some(Value::Bool(true)) => {}
        Some(_) => {
            return Err(refuse(
                iri,
                Kind::WrongShape {
                    key: "S231:isArray".to_owned(),
                },
            ));
        }
    }
    let Some(Value::String(spelled)) = node.get("S231:sizeOfDimensions") else {
        return Err(shape());
    };
    let dims = values::dimensions(iri, spelled)?;
    let rank_ok = matches!(node.get("S231:numberDimensions"), Some(Value::Number(n)) if n.as_u64() == u64::try_from(dims.len()).ok());
    if !rank_ok {
        return Err(shape());
    }
    let Value::Array(items) = value else {
        return Err(shape());
    };
    // Checked: hostile sizes overflow `usize` (and wrap to 0 in release).
    let total = dims.iter().try_fold(1usize, |acc, &d| acc.checked_mul(d));
    if total != Some(items.len()) {
        return Err(shape());
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
        .ok_or_else(shape)?;
    items
        .iter()
        .enumerate()
        .map(|(flat, item)| {
            let indices: Vec<usize> = match dims.as_slice() {
                [_] => vec![flat + 1],
                [_, cols] => vec![flat / cols + 1, flat % cols + 1],
                _ => Vec::new(),
            };
            let name = param_element_name(template, &indices).ok_or_else(shape)?;
            if matches!(item, Value::Array(_)) {
                return Err(shape());
            }
            let value = values::param_value(iri, item, params)?;
            Ok(Assignment::new(Identifier::unchecked(name), value))
        })
        .collect()
}
