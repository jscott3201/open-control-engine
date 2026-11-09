//! The document layout and canonical writer (authoring note, "Layout" and "Writer").
//!
//! Fixed layout: the root; the sequence parameters in authored order; each block in authored
//! order, followed by its parameter nodes in authored order and then its port nodes, inputs then
//! outputs, in catalog order; then the boundary inputs, then the boundary outputs, each in
//! authored order. `isConnectedTo` hangs on the source node with its targets in authored
//! connection order. Nothing is sorted. Each node kind has one fixed key order; relation lists
//! are always arrays (decision 17) and an empty relation is left out. The bytes come from the
//! crate's canonical JSON writer: two-space indentation, one entry per line, ASCII-only with
//! `\uXXXX` escapes, one trailing newline.

use std::collections::BTreeMap;

use oce_api::{CatalogPortKind, ClassPorts};

use crate::json::Json;
use crate::model::{Boundary, Endpoint, Sequence};

use super::EmitError;
use super::iri::{S231_IRI, Scheme, class_iri};
use super::params::{
    ParamNode, block_parameters, bound_value, sequence_data_type, sequence_value, signal_data_type,
};

fn reference(iri: &str) -> Json {
    Json::obj([("@id", Json::str(iri))])
}

fn references(iris: &[String]) -> Json {
    Json::Arr(iris.iter().map(|iri| reference(iri)).collect())
}

/// Push `key: [refs]` when `iris` is non-empty.
fn relation(entries: &mut Vec<(String, Json)>, key: &str, iris: &[String]) {
    if !iris.is_empty() {
        entries.push((key.to_owned(), references(iris)));
    }
}

fn kind_name(kind: CatalogPortKind) -> &'static str {
    match kind {
        CatalogPortKind::Integer => "Integer",
        CatalogPortKind::Boolean => "Boolean",
        _ => "Real",
    }
}

/// The IRI an endpoint names.
fn endpoint_iri(scheme: &Scheme, endpoint: &Endpoint) -> String {
    match endpoint {
        Endpoint::Boundary(name) => scheme.local(name.as_str()),
        Endpoint::Port { block, port } => scheme.member(block.as_str(), port.as_str()),
    }
}

/// Write the document. `ports[i]` is block `i`'s resolved interface; a block without one gets
/// no port nodes (only the stage-1 bypass emits such a block).
pub(super) fn document(
    sequence: &Sequence,
    scheme: &Scheme,
    ports: &[Option<ClassPorts>],
) -> Result<String, EmitError> {
    let mut targets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut sources: Vec<String> = Vec::new();
    for connection in &sequence.connections {
        let source = endpoint_iri(scheme, &connection.source);
        let list = targets.entry(source.clone()).or_insert_with(|| {
            sources.push(source);
            Vec::new()
        });
        list.push(endpoint_iri(scheme, &connection.target));
    }
    let mut graph: Vec<Json> = Vec::new();
    graph.push(root(sequence, scheme));
    for param in &sequence.params {
        graph.push(Json::Obj(vec![
            (
                "@id".to_owned(),
                Json::str(&scheme.local(param.name.as_str())),
            ),
            ("@type".to_owned(), Json::str("S231:Parameter")),
            (
                "S231:isOfDataType".to_owned(),
                reference(&sequence_data_type(&param.value_type, &param.value)),
            ),
            (
                "S231:value".to_owned(),
                sequence_value(&param.value_type, &param.value),
            ),
        ]));
    }
    for (index, block) in sequence.blocks.iter().enumerate() {
        let block_name = block.name.as_str();
        let params = block_parameters(block);
        let resolved = ports.get(index).and_then(Option::as_ref);
        let port_iris = |side: fn(&ClassPorts) -> &Vec<oce_api::ClassPort>| -> Vec<String> {
            resolved.map_or_else(Vec::new, |p| {
                side(p)
                    .iter()
                    .map(|port| scheme.member(block_name, &port.name))
                    .collect()
            })
        };
        let inputs = port_iris(|p| &p.inputs);
        let outputs = port_iris(|p| &p.outputs);
        let mut entries = vec![
            ("@id".to_owned(), Json::str(&scheme.local(block_name))),
            ("@type".to_owned(), Json::str(&class_iri(&block.class_path))),
        ];
        let param_iris: Vec<String> = params
            .iter()
            .map(|p| scheme.member(block_name, &p.local))
            .collect();
        relation(&mut entries, "S231:hasParameter", &param_iris);
        relation(&mut entries, "S231:hasInput", &inputs);
        relation(&mut entries, "S231:hasOutput", &outputs);
        graph.push(Json::Obj(entries));
        for (param, iri) in params.into_iter().zip(&param_iris) {
            graph.push(param_node(param, iri));
        }
        if let Some(resolved) = resolved {
            for (port, iri) in resolved.inputs.iter().zip(&inputs) {
                // An input port never drives a validated connection; only the stage-1 bypass
                // can hang one here, and the engine then judges its direction.
                let connected = targets.remove(iri);
                graph.push(port_node(iri, "Input", port.kind, connected));
            }
            for (port, iri) in resolved.outputs.iter().zip(&outputs) {
                let connected = targets.remove(iri);
                graph.push(port_node(iri, "Output", port.kind, connected));
            }
        }
    }
    for boundary in &sequence.inputs {
        let iri = scheme.local(boundary.name.as_str());
        let connected = targets.remove(&iri);
        graph.push(boundary_node(&iri, "Input", boundary, connected));
    }
    for boundary in &sequence.outputs {
        let iri = scheme.local(boundary.name.as_str());
        let connected = targets.remove(&iri);
        graph.push(boundary_node(&iri, "Output", boundary, connected));
    }
    // Connections whose source names no emitted node (only the stage-1 bypass writes one): a
    // bare node carrying the relation, in first-authored order, so the engine judges it.
    for source in sources {
        if let Some(list) = targets.remove(&source) {
            graph.push(Json::Obj(vec![
                ("@id".to_owned(), Json::str(&source)),
                ("S231:isConnectedTo".to_owned(), references(&list)),
            ]));
        }
    }
    let document = Json::Obj(vec![
        (
            "@context".to_owned(),
            Json::obj([("S231", Json::str(S231_IRI))]),
        ),
        ("@graph".to_owned(), Json::Arr(graph)),
    ]);
    Ok(document.to_document())
}

fn root(sequence: &Sequence, scheme: &Scheme) -> Json {
    let names = |iter: &mut dyn Iterator<Item = &str>| -> Vec<String> {
        iter.map(|name| scheme.local(name)).collect()
    };
    let mut entries = vec![
        ("@id".to_owned(), Json::str(scheme.root())),
        ("@type".to_owned(), Json::str("S231:Block")),
    ];
    relation(
        &mut entries,
        "S231:hasParameter",
        &names(&mut sequence.params.iter().map(|p| p.name.as_str())),
    );
    relation(
        &mut entries,
        "S231:hasInput",
        &names(&mut sequence.inputs.iter().map(|b| b.name.as_str())),
    );
    relation(
        &mut entries,
        "S231:hasOutput",
        &names(&mut sequence.outputs.iter().map(|b| b.name.as_str())),
    );
    relation(
        &mut entries,
        "S231:containsBlock",
        &names(&mut sequence.blocks.iter().map(|b| b.name.as_str())),
    );
    Json::Obj(entries)
}

fn param_node(param: ParamNode, iri: &str) -> Json {
    let mut entries = vec![
        ("@id".to_owned(), Json::str(iri)),
        ("@type".to_owned(), Json::str("S231:Parameter")),
    ];
    if let Some(data_type) = &param.data_type {
        entries.push(("S231:isOfDataType".to_owned(), reference(data_type)));
    }
    if let Some(dimensions) = &param.dimensions {
        let spelled: Vec<String> = dimensions.iter().map(ToString::to_string).collect();
        entries.push(("S231:isArray".to_owned(), Json::boolean(true)));
        entries.push((
            "S231:numberDimensions".to_owned(),
            Json::integer(i64::try_from(dimensions.len()).unwrap_or(i64::MAX)),
        ));
        entries.push((
            "S231:sizeOfDimensions".to_owned(),
            Json::str(&format!("({})", spelled.join(", "))),
        ));
    }
    entries.push(("S231:value".to_owned(), param.value));
    Json::Obj(entries)
}

fn port_node(iri: &str, side: &str, kind: CatalogPortKind, connected: Option<Vec<String>>) -> Json {
    let kind = kind_name(kind);
    let mut entries = vec![
        ("@id".to_owned(), Json::str(iri)),
        ("@type".to_owned(), Json::str(&format!("S231:{kind}{side}"))),
        (
            "S231:isOfDataType".to_owned(),
            reference(&format!("S231:{kind}")),
        ),
    ];
    if let Some(list) = connected {
        entries.push(("S231:isConnectedTo".to_owned(), references(&list)));
    }
    Json::Obj(entries)
}

fn boundary_node(
    iri: &str,
    side: &str,
    boundary: &Boundary,
    connected: Option<Vec<String>>,
) -> Json {
    let kind = boundary.value_type.as_str();
    let mut entries = vec![
        ("@id".to_owned(), Json::str(iri)),
        ("@type".to_owned(), Json::str(&format!("S231:{kind}{side}"))),
        (
            "S231:isOfDataType".to_owned(),
            reference(signal_data_type(boundary.value_type)),
        ),
    ];
    let attributes = [
        ("S231:unit", boundary.semantic.unit.as_ref()),
        ("S231:quantity", boundary.quantity.as_ref()),
        ("S231:displayUnit", boundary.display_unit.as_ref()),
    ];
    for (key, value) in attributes {
        if let Some(value) = value {
            entries.push((key.to_owned(), Json::str(value)));
        }
    }
    // Bounds after `displayUnit` (importer note, "Bounds on boundary declarations").
    for (key, bound) in [("S231:min", boundary.min), ("S231:max", boundary.max)] {
        if let Some(bound) = bound {
            entries.push((key.to_owned(), bound_value(boundary.value_type, bound)));
        }
    }
    if let Some(list) = connected {
        entries.push(("S231:isConnectedTo".to_owned(), references(&list)));
    }
    Json::Obj(entries)
}
