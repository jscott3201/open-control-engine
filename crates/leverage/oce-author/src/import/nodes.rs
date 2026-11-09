//! Node-level reading for the importer: the property whitelists, relation and attribute
//! access, connector and parameter typing, the value-shape check against `isOfDataType`, and the
//! document's IRI scheme.

use serde_json::{Map, Value};

use crate::identifier::check_identifier_syntax;
use crate::model::{Assignment, Literal, ParamValue, SignalType};

use super::{ImportError, malformed, unresolved, unsupported};

/// The `S231` context IRI a document must bind.
pub(super) const S231_IRI: &str = "http://data.ashrae.org/S231P#";

/// Keys every node kind may carry besides its own.
pub(super) const COMMON_KEYS: [&str; 3] = ["@id", "@type", "S231:label"];

/// Keys each node kind may carry (beyond [`COMMON_KEYS`]).
pub(super) const ROOT_KEYS: [&str; 4] = [
    "S231:hasParameter",
    "S231:hasInput",
    "S231:hasOutput",
    "S231:containsBlock",
];
pub(super) const BLOCK_KEYS: [&str; 3] = ["S231:hasParameter", "S231:hasInput", "S231:hasOutput"];
pub(super) const PARAM_KEYS: [&str; 8] = [
    "S231:isOfDataType",
    "S231:value",
    "S231:unit",
    "S231:quantity",
    "S231:displayUnit",
    "S231:isArray",
    "S231:numberDimensions",
    "S231:sizeOfDimensions",
];
pub(super) const PORT_KEYS: [&str; 2] = ["S231:isOfDataType", "S231:isConnectedTo"];
pub(super) const BOUNDARY_KEYS: [&str; 5] = [
    "S231:isOfDataType",
    "S231:isConnectedTo",
    "S231:unit",
    "S231:quantity",
    "S231:displayUnit",
];

/// Refuse any key outside `allowed` and [`COMMON_KEYS`].
pub(super) fn check_keys(
    id: &str,
    node: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), ImportError> {
    match node
        .keys()
        .find(|k| !COMMON_KEYS.contains(&k.as_str()) && !allowed.contains(&k.as_str()))
    {
        Some(key) => Err(unsupported(id, &format!("the property `{key}` here"))),
        None => Ok(()),
    }
}

/// The `@id` references of a relation: one `{"@id"}` object or an array of them.
pub(super) fn references<'a>(
    id: &str,
    node: &'a Map<String, Value>,
    key: &str,
) -> Result<Vec<&'a str>, ImportError> {
    let one = |value: &'a Value| -> Result<&'a str, ImportError> {
        match value {
            Value::Object(map) if map.len() == 1 => match map.get("@id") {
                Some(Value::String(iri)) => Ok(iri.as_str()),
                _ => Err(malformed(format!("`{id}`: `{key}` holds a non-reference"))),
            },
            _ => Err(malformed(format!("`{id}`: `{key}` holds a non-reference"))),
        }
    };
    match node.get(key) {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) => items.iter().map(one).collect(),
        Some(value) => Ok(vec![one(value)?]),
    }
}

/// A string-valued attribute, or `None` when absent.
pub(super) fn text_attribute<'a>(
    id: &str,
    node: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, ImportError> {
    match node.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(malformed(format!("`{id}`: `{key}` is not a string"))),
    }
}

/// The signal type an `isOfDataType` reference names, if it names Real, Integer or Boolean.
pub(super) fn data_type(
    id: &str,
    node: &Map<String, Value>,
) -> Result<Option<SignalType>, ImportError> {
    let named = references(id, node, "S231:isOfDataType")?;
    Ok(match named.as_slice() {
        ["S231:Real"] => Some(SignalType::Real),
        ["S231:Integer"] => Some(SignalType::Integer),
        ["S231:Boolean"] => Some(SignalType::Boolean),
        _ => None,
    })
}

/// The IRI scheme the document uses: the root `@id` is `{namespace}{sequence}`, where
/// `sequence` is the trailing identifier, and every other node is `{root}.{local}`.
pub(super) struct Names<'a> {
    pub(super) root: &'a str,
}

impl<'a> Names<'a> {
    /// The local path under the root, checked as dot-separated identifiers.
    pub(super) fn local(&self, subject: &str, iri: &'a str) -> Result<Vec<&'a str>, ImportError> {
        let path = iri
            .strip_prefix(self.root)
            .and_then(|rest| rest.strip_prefix('.'))
            .ok_or_else(|| unresolved(subject, iri))?;
        let parts: Vec<&str> = path.split('.').collect();
        if parts.iter().any(|p| check_identifier_syntax(p).is_err()) {
            return Err(unsupported(
                iri,
                "a local name that is not dot-separated identifiers",
            ));
        }
        Ok(parts)
    }
}

/// A parameter node's `@type`, when written, is `S231:Parameter`.
pub(super) fn check_parameter_type(
    iri: &str,
    node: &Map<String, Value>,
) -> Result<(), ImportError> {
    match node.get("@type") {
        None => Ok(()),
        Some(Value::String(t)) if t == "S231:Parameter" => Ok(()),
        Some(_) => Err(unsupported(
            iri,
            "a parameter whose `@type` is not `S231:Parameter`",
        )),
    }
}

/// A block parameter's `isOfDataType`, when written, agrees with the shape of every value read
/// for it: a reference agrees with any type; otherwise Real takes a Real or Integer literal,
/// Integer an Integer, Boolean a Boolean, String a string and a `CDL.Types` enumeration class a
/// member of that type. Any other datatype is refused.
pub(super) fn check_value_shapes(
    iri: &str,
    node: &Map<String, Value>,
    assignments: &[Assignment],
) -> Result<(), ImportError> {
    if !node.contains_key("S231:isOfDataType") {
        return Ok(());
    }
    let declared = references(iri, node, "S231:isOfDataType")?;
    let [declared] = declared.as_slice() else {
        return Err(unsupported(iri, "more than one `isOfDataType`"));
    };
    let enum_type = declared
        .rsplit_once('#')
        .and_then(|(_, fragment)| fragment.strip_prefix("Buildings.Controls.OBC.CDL.Types."));
    let fits = |literal: &Literal| match (*declared, literal) {
        ("S231:Real", Literal::Real(_) | Literal::Integer(_))
        | ("S231:Integer", Literal::Integer(_))
        | ("S231:Boolean", Literal::Boolean(_))
        | ("S231:String", Literal::Text(_)) => true,
        (_, Literal::Member(member)) => enum_type == Some(member.type_name.as_str()),
        _ => false,
    };
    let known = matches!(
        *declared,
        "S231:Real" | "S231:Integer" | "S231:Boolean" | "S231:String"
    ) || enum_type.is_some_and(|t| !t.is_empty() && !t.contains('.'));
    if !known {
        return Err(unsupported(
            iri,
            "an `isOfDataType` outside the S231 scalars and `CDL.Types`",
        ));
    }
    for assignment in assignments {
        if let ParamValue::Literal(literal) = &assignment.value
            && !fits(literal)
        {
            return Err(unsupported(
                iri,
                "a value whose shape disagrees with its `isOfDataType`",
            ));
        }
    }
    Ok(())
}

/// The sequence name: the trailing identifier of the root `@id`.
pub(super) fn sequence_name(root: &str) -> Result<&str, ImportError> {
    // The byte just past the last character that cannot be part of an identifier; computed on
    // character boundaries, so a multi-byte character never splits.
    let start = root
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map_or(0, |(at, c)| at + c.len_utf8());
    let name = root.get(start..).unwrap_or_default();
    if start == 0 || check_identifier_syntax(name).is_err() {
        return Err(unsupported(
            root,
            "a root `@id` that does not end in an identifier after a delimiter",
        ));
    }
    Ok(name)
}

/// The signal type of a connector node from its `@type` and `isOfDataType`, which must agree.
pub(super) fn connector_type(
    iri: &str,
    node: &Map<String, Value>,
    side: &str,
) -> Result<SignalType, ImportError> {
    let from_type = match node.get("@type") {
        Some(Value::String(t)) => {
            match t.strip_prefix("S231:").and_then(|k| k.strip_suffix(side)) {
                Some("Real") => Some(SignalType::Real),
                Some("Integer") => Some(SignalType::Integer),
                Some("Boolean") => Some(SignalType::Boolean),
                _ => None,
            }
        }
        _ => None,
    };
    let declared = data_type(iri, node)?;
    match (from_type, declared) {
        (Some(a), Some(b)) if a == b => Ok(a),
        (Some(a), None) if !node.contains_key("S231:isOfDataType") => Ok(a),
        _ => Err(unsupported(
            iri,
            &format!("a connector that is not a typed S231 Real, Integer or Boolean {side}"),
        )),
    }
}
