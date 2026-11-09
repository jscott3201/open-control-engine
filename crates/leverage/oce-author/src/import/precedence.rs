//! The fixed refusal precedence (importer note, "`ImportError`"; decision 16).
//!
//! A document with several out-of-subset constructs is refused for the first of these classes,
//! whatever its node order: nested composite, conditional component, expression value, port
//! attribute, name. The CXF reader checks the first (a second root) when it finds the root; this
//! pass checks the other four, class by class over the whole graph, before any value is read. The
//! pass is lenient about structure it does not need (a malformed relation is skipped here and
//! refused by the reader, which then reports it in reading order).

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::error::{ImportError, ImportErrorKind as Kind};
use super::nodes::{Names, PORT_ATTRIBUTES, local_path, refuse, sequence_name};
use super::values::is_expression;

/// The nodes by `@id`, and the `@id`s in `@graph` order.
pub(super) struct Graph<'a> {
    pub(super) nodes: BTreeMap<&'a str, &'a Map<String, Value>>,
    pub(super) order: Vec<&'a str>,
}

/// Check conditional components, expression values, port attributes and names, in that order.
pub(super) fn check(graph: &Graph<'_>, root: &str) -> Result<(), ImportError> {
    conditional(graph)?;
    expression(graph, root)?;
    port_attribute(graph, root)?;
    names(graph, root)
}

/// The `@id`s a relation names, skipping anything that is not a reference.
fn lenient<'a>(node: &'a Map<String, Value>, key: &str) -> Vec<&'a str> {
    let one = |value: &'a Value| match value {
        Value::Object(map) => match map.get("@id") {
            Some(Value::String(iri)) => Some(iri.as_str()),
            _ => None,
        },
        _ => None,
    };
    match node.get(key) {
        Some(Value::Array(items)) => items.iter().filter_map(one).collect(),
        Some(value) => one(value).into_iter().collect(),
        None => Vec::new(),
    }
}

fn conditional(graph: &Graph<'_>) -> Result<(), ImportError> {
    for id in &graph.order {
        if graph.nodes.get(id).is_some_and(|node| {
            node.contains_key("S231:isConditionalComponent")
                || node.contains_key("S231:conditionalExpression")
        }) {
            return Err(refuse(id, Kind::ConditionalComponent));
        }
    }
    Ok(())
}

fn expression(graph: &Graph<'_>, root: &str) -> Result<(), ImportError> {
    let Some(root_node) = graph.nodes.get(root) else {
        return Ok(());
    };
    let params: Vec<&str> = lenient(root_node, "S231:hasParameter")
        .into_iter()
        .filter_map(|iri| local_path(root, iri))
        .collect();
    for id in &graph.order {
        let Some(value) = graph.nodes.get(id).and_then(|node| node.get("S231:value")) else {
            continue;
        };
        let texts: Vec<&str> = match value {
            Value::String(text) => vec![text.as_str()],
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if texts.iter().any(|text| is_expression(text, &params)) {
            return Err(refuse(id, Kind::ExpressionValue));
        }
    }
    Ok(())
}

fn port_attribute(graph: &Graph<'_>, root: &str) -> Result<(), ImportError> {
    let Some(root_node) = graph.nodes.get(root) else {
        return Ok(());
    };
    for block in lenient(root_node, "S231:containsBlock") {
        let Some(block_node) = graph.nodes.get(block) else {
            continue;
        };
        for key in ["S231:hasInput", "S231:hasOutput"] {
            for port in lenient(block_node, key) {
                let Some(port_node) = graph.nodes.get(port) else {
                    continue;
                };
                if let Some(attribute) =
                    PORT_ATTRIBUTES.iter().find(|a| port_node.contains_key(**a))
                {
                    return Err(refuse(
                        port,
                        Kind::PortAttribute {
                            attribute: (*attribute).to_owned(),
                        },
                    ));
                }
            }
        }
    }
    Ok(())
}

fn names(graph: &Graph<'_>, root: &str) -> Result<(), ImportError> {
    sequence_name(root)?;
    let scheme = Names { root };
    for id in &graph.order {
        if local_path(root, id).is_some() {
            scheme.local(root, id)?;
        }
    }
    Ok(())
}
