//! Parameter values in CXF (authoring note, "Values"; decisions 3, 15 and 16).
//!
//! A Real is a typed literal (`{"@value": "<shortest round-trip>", "@type": xsd:double}`), an
//! Integer a bare JSON integer, a Boolean a JSON boolean, a String the CDL string expression
//! (`"\"text\""`), an enumeration member its full member path, and a reference the bare
//! sequence-parameter name, which the engine grounds from the root's scope (Rule 5). The CXF type
//! follows the parameter's catalog type, so an Integer literal assigned to a Real parameter is
//! written as a Real. Array elements, assigned one at a time in the model, are gathered into one
//! `isArray` node per array parameter at the position of the first element authored, in
//! row-major order (an array with a gap is written element by element under the flattened
//! names instead). Only what the author assigned is written; no default is ever filled.

use std::collections::BTreeMap;

use oce_api::ValueType;

use crate::json::Json;
use crate::model::{Assignment, Block, Literal, ParamValue, SignalType};
use crate::validate::catalog::{
    ClassView, enum_type, is_template, lookup_class, lookup_parameter, match_template,
};

use super::iri::{MODELICA_PACKAGE, XSD_DOUBLE, class_iri};

/// One parameter node to write under a block: a scalar or a whole array.
pub(super) struct ParamNode {
    /// The local name (`uLow`, or the array's base name `k`).
    pub(super) local: String,
    /// The `S231:isOfDataType` IRI, when known.
    pub(super) data_type: Option<String>,
    /// The array shape, `None` for a scalar.
    pub(super) dimensions: Option<Vec<usize>>,
    /// The `S231:value`: one value, or the row-major element list of an array.
    pub(super) value: Json,
}

/// A Real literal in its typed form.
pub(super) fn real(value: f64) -> Json {
    Json::obj([
        ("@value", Json::str(&format!("{value:?}"))),
        ("@type", Json::str(XSD_DOUBLE)),
    ])
}

/// The `S231:isOfDataType` reference of a signal type.
pub(super) fn signal_data_type(value_type: SignalType) -> &'static str {
    match value_type {
        SignalType::Real => "S231:Real",
        SignalType::Integer => "S231:Integer",
        SignalType::Boolean => "S231:Boolean",
    }
}

/// A sequence parameter's value in its declared type (a Real widens an Integer literal).
pub(super) fn sequence_value(value_type: SignalType, literal: &Literal) -> Json {
    match (value_type, literal) {
        (SignalType::Real, Literal::Integer(v)) => real(*v as f64),
        (_, literal) => literal_value(None, literal),
    }
}

/// The CDL string expression of a text value: quoted, with `"`, `\` and line feed escaped.
pub(super) fn cdl_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn data_type_of(value_type: ValueType) -> Option<String> {
    Some(match value_type {
        ValueType::Real => "S231:Real".to_owned(),
        ValueType::Integer => "S231:Integer".to_owned(),
        ValueType::Boolean => "S231:Boolean".to_owned(),
        ValueType::String => "S231:String".to_owned(),
        ValueType::Enum(_) => class_iri(enum_type(value_type)?.class_path),
    })
}

fn literal_data_type(literal: &Literal) -> Option<String> {
    match literal {
        Literal::Integer(_) => Some("S231:Integer".to_owned()),
        Literal::Real(_) => Some("S231:Real".to_owned()),
        Literal::Boolean(_) => Some("S231:Boolean".to_owned()),
        Literal::Text(_) => Some("S231:String".to_owned()),
        Literal::EnumMember(member) if !member.type_name.is_empty() => {
            Some(class_iri(&format!("CDL.Types.{}", member.type_name)))
        }
        Literal::EnumMember(_) => None,
    }
}

/// A literal in the CXF form for a parameter of type `expected` (`None` when the catalog does
/// not know the parameter, which only the stage-1 bypass emits).
fn literal_value(expected: Option<ValueType>, literal: &Literal) -> Json {
    match (expected, literal) {
        (Some(ValueType::Real), Literal::Integer(v)) => real(*v as f64),
        (_, Literal::Real(v)) => real(*v),
        (_, Literal::Integer(v)) => Json::integer(*v),
        (_, Literal::Boolean(v)) => Json::boolean(*v),
        (_, Literal::Text(text)) => Json::str(&cdl_string(text)),
        (_, Literal::EnumMember(member)) => {
            let class_path = expected
                .and_then(enum_type)
                .filter(|t| t.type_name == member.type_name)
                .map_or_else(
                    || format!("CDL.Types.{}", member.type_name),
                    |t| t.class_path.to_owned(),
                );
            Json::str(&format!("{MODELICA_PACKAGE}{class_path}.{}", member.member))
        }
    }
}

fn value_of(expected: Option<ValueType>, value: &ParamValue) -> Json {
    match value {
        ParamValue::Literal(literal) => literal_value(expected, literal),
        ParamValue::Ref(name) => Json::str(name.as_str()),
    }
}

/// The base name of an array template: `k_<i>` is `k`, `K_<row>_<col>` is `K`.
fn template_base(template: &str) -> &str {
    template.split_once("_<").map_or(template, |(base, _)| base)
}

/// An array being gathered: its node position, template and elements by index.
struct ArrayGroup<'a> {
    template: &'static str,
    data_type: Option<String>,
    elements: BTreeMap<Vec<usize>, &'a Assignment>,
    authored: Vec<&'a Assignment>,
    expected: ValueType,
}

/// Lower a block's assignments to parameter nodes in authored order. An array whose assigned
/// elements fill the rectangle from index 1 becomes one array node; one with a gap becomes one
/// scalar node per element (see [`array_node`]).
pub(super) fn block_parameters(block: &Block) -> Vec<ParamNode> {
    let class: Option<ClassView> = lookup_class(&block.class_path);
    let mut nodes: Vec<Result<ParamNode, usize>> = Vec::new();
    let mut groups: Vec<ArrayGroup<'_>> = Vec::new();
    let mut group_of: BTreeMap<&str, usize> = BTreeMap::new();
    for assignment in &block.assignments {
        let name = assignment.name.as_str();
        let parameter = class
            .as_ref()
            .and_then(|class| lookup_parameter(class.metadata, name));
        let template = parameter.filter(|p| is_template(p.name));
        if let (Some(parameter), Some(template)) = (parameter, template) {
            let Some(indices) = match_template(template.name, name) else {
                continue;
            };
            let index = *group_of.entry(template.name).or_insert_with(|| {
                nodes.push(Err(groups.len()));
                groups.push(ArrayGroup {
                    template: template.name,
                    data_type: data_type_of(parameter.value_type),
                    elements: BTreeMap::new(),
                    authored: Vec::new(),
                    expected: parameter.value_type,
                });
                groups.len() - 1
            });
            let group = &mut groups[index];
            if let std::collections::btree_map::Entry::Vacant(slot) = group.elements.entry(indices)
            {
                slot.insert(assignment);
                group.authored.push(assignment);
            }
            continue;
        }
        let expected = parameter.map(|p| p.value_type);
        let data_type = match (expected, &assignment.value) {
            (Some(value_type), _) => data_type_of(value_type),
            (None, ParamValue::Literal(literal)) => literal_data_type(literal),
            (None, ParamValue::Ref(_)) => None,
        };
        nodes.push(Ok(ParamNode {
            local: name.to_owned(),
            data_type,
            dimensions: None,
            value: value_of(expected, &assignment.value),
        }));
    }
    nodes
        .into_iter()
        .flat_map(|node| match node {
            Ok(node) => vec![node],
            Err(group) => array_node(&groups[group]),
        })
        .collect()
}

fn array_node(group: &ArrayGroup<'_>) -> Vec<ParamNode> {
    let rank = group.elements.keys().next().map_or(0, Vec::len);
    let mut dimensions = vec![0usize; rank];
    for indices in group.elements.keys() {
        for (axis, &index) in indices.iter().enumerate() {
            dimensions[axis] = dimensions[axis].max(index);
        }
    }
    let total = dimensions
        .iter()
        .try_fold(1usize, |acc, &d| acc.checked_mul(d));
    if total != Some(group.elements.len()) {
        // A gap: the array form would need a default, which the emitter never fills (decision
        // 16), so each assigned element is its own node under its flattened name, in authored
        // order — the names the engine's class rules read.
        return group
            .authored
            .iter()
            .map(|assignment| ParamNode {
                local: assignment.name.as_str().to_owned(),
                data_type: group.data_type.clone(),
                dimensions: None,
                value: value_of(Some(group.expected), &assignment.value),
            })
            .collect();
    }
    // BTreeMap order over index vectors is row-major order.
    let values = group
        .elements
        .values()
        .map(|assignment| value_of(Some(group.expected), &assignment.value))
        .collect();
    vec![ParamNode {
        local: template_base(group.template).to_owned(),
        data_type: group.data_type.clone(),
        dimensions: Some(dimensions),
        value: Json::Arr(values),
    }]
}
