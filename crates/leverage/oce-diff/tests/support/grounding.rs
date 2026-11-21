//! The authoring model a loaded topology stands for, built from the authoring model alone: the
//! oracle the CXF adapter is held to across the two levels.
//!
//! A load grounds what the authoring level writes symbolically and keeps only what the canonical
//! content id covers. [`grounded`] does the same to a model, by rules typed from the documents
//! that state them and sharing no code with the adapter:
//!
//! - a reference to a sequence parameter is its value, and the sequence parameters go (the
//!   authoring note lowers them onto the root, and a load grounds every reference);
//! - an Integer literal on a parameter the catalog types Real, or a Real sequence parameter, is
//!   the Real it means (the emitter widens it, `oce-author`'s `emit/params.rs`);
//! - a member of an Integer constant package is its Integer (the enumeration table);
//! - a boundary input nothing reads and an output nothing drives are not in the loaded graph
//!   (`docs/cxf-composite-subset.md`, boundary lowering), and an output's bounds are not either;
//! - only a Real boundary carries `unit`, `quantity` and `displayUnit`, and its bounds are Reals;
//! - an Integer input's bounds are its effective domain, the CDL `i32` range where the
//!   declaration leaves a side open (`oce_api::InputDefinition`);
//! - unit suffixes and written numbers are spellings, and go.
//!
//! What it keeps is compared section by section with the CXF diff of the two emitted documents.

use std::collections::BTreeMap;

use oce_api::{
    EnumerationKind, MemberValue, ValueType, class_metadata, enumeration_member_value, enumerations,
};
use oce_author::{Bound, Endpoint, Literal, ParamValue, ParameterType, Sequence, SignalType};
use oce_diff::{Diff, GraphChange, InterfaceChange};

/// Whether the catalog types the parameter `name` of `class_path` (a scalar or an array element
/// `k_2` of `k_<i>`) as a Real.
fn real_parameter(class_path: &str, name: &str) -> bool {
    let Some(class) = class_metadata()
        .iter()
        .find(|metadata| metadata.class_path == class_path)
    else {
        return false;
    };
    class.parameters.iter().any(|parameter| {
        let template = parameter.name;
        let matches = match template.split_once('<') {
            None => template == name,
            Some((stem, _)) => name.strip_prefix(stem).is_some_and(|rest| {
                !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit() || b == b'_')
            }),
        };
        matches && parameter.value_type == ValueType::Real
    })
}

/// The Integer a member of a constant package stands for, if it is one.
fn constant(literal: &Literal) -> Option<i64> {
    let Literal::Member(member) = literal else {
        return None;
    };
    let row = enumerations().iter().find(|row| {
        row.kind == EnumerationKind::ConstantPackage && row.type_name() == member.type_name
    })?;
    match enumeration_member_value(row.class_path, &member.member)? {
        MemberValue::Integer(value) => Some(value),
        MemberValue::Ordinal(_) => None,
    }
}

fn widened(literal: Literal, real: bool) -> Literal {
    match literal {
        Literal::Integer(integer) if real => Literal::Real(integer as f64),
        other => other,
    }
}

/// The model a load of `model`'s emitted document stands for.
pub fn grounded(model: &Sequence) -> Sequence {
    let mut model = model.clone();
    let parameters: BTreeMap<String, Literal> = model
        .params
        .iter()
        .map(|parameter| {
            let real = parameter.value_type == ParameterType::Real;
            (
                parameter.name.as_str().to_owned(),
                widened(parameter.value.clone(), real),
            )
        })
        .collect();
    for block in &mut model.blocks {
        let class = block.class_path.clone();
        for assignment in &mut block.assignments {
            let literal = match &assignment.value {
                ParamValue::Literal(literal) => literal.clone(),
                ParamValue::Ref(name) => match parameters.get(name.as_str()) {
                    Some(literal) => literal.clone(),
                    None => continue,
                },
                _ => continue,
            };
            let literal = match constant(&literal) {
                Some(integer) => Literal::Integer(integer),
                None => literal,
            };
            let real = real_parameter(&class, assignment.name.as_str());
            assignment.value = ParamValue::Literal(widened(literal, real));
            assignment.suffix = None;
            assignment.scaled_from = None;
        }
    }
    model.params.clear();

    let ends = |pick: fn(&oce_author::Connection) -> &Endpoint| -> Vec<String> {
        model
            .connections
            .iter()
            .filter_map(|connection| match pick(connection) {
                Endpoint::Boundary(name) => Some(name.as_str().to_owned()),
                _ => None,
            })
            .collect()
    };
    let read = ends(|connection| &connection.source);
    let driven = ends(|connection| &connection.target);
    model
        .inputs
        .retain(|input| read.iter().any(|name| name == input.name.as_str()));
    model
        .outputs
        .retain(|output| driven.iter().any(|name| name == output.name.as_str()));
    for output in &mut model.outputs {
        output.min = None;
        output.max = None;
    }
    for input in &mut model.inputs {
        if input.value_type == SignalType::Integer {
            input.min = input.min.or(Some(Bound::Integer(i64::from(i32::MIN))));
            input.max = input.max.or(Some(Bound::Integer(i64::from(i32::MAX))));
        }
    }
    for boundary in model.inputs.iter_mut().chain(model.outputs.iter_mut()) {
        if boundary.value_type == SignalType::Real {
            let real = |bound: Option<Bound>| match bound {
                Some(Bound::Integer(integer)) => Some(Bound::Real(integer as f64)),
                other => other,
            };
            boundary.min = real(boundary.min);
            boundary.max = real(boundary.max);
        } else {
            boundary.semantic.unit = None;
            boundary.quantity = None;
            boundary.display_unit = None;
        }
    }
    model
}

/// The part of a diff both levels see: the graph and interface sections without `reordered`
/// entries and with every block's assignments sorted, the two section flags, the block counts,
/// the notes and the two sequence names.
pub fn seen_by_both(diff: &Diff) -> serde_json::Value {
    let graph: Vec<GraphChange> = diff
        .graph
        .iter()
        .filter(|entry| !matches!(entry, GraphChange::Reordered { .. }))
        .cloned()
        .collect();
    let interface: Vec<InterfaceChange> = diff
        .interface
        .iter()
        .filter(|entry| !matches!(entry, InterfaceChange::Reordered { .. }))
        .cloned()
        .collect();
    let mut graph = serde_json::to_value(graph).expect("JSON");
    for entry in graph.as_array_mut().into_iter().flatten() {
        if let Some(params) = entry.get_mut("params").and_then(|p| p.as_array_mut()) {
            params.sort_by_key(|param| param.to_string());
        }
    }
    serde_json::json!({
        "graph": graph,
        "interface": serde_json::to_value(interface).expect("JSON"),
        "graph_equal": diff.summary.graph_equal,
        "interface_equal": diff.summary.interface_equal,
        "blocks": serde_json::to_value(diff.summary.blocks).expect("JSON"),
        "notes": serde_json::to_value(&diff.notes).expect("JSON"),
        "sequences": [diff.base.sequence, diff.head.sequence],
    })
}
