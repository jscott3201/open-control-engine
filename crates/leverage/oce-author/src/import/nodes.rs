//! Node-level reading for the CXF importer: the property whitelists and the kind each foreign
//! property is refused with, relation and attribute access, connector and parameter typing
//! through the facade's named-type table, and the document's IRI scheme.

use oce_api::{EnumerationClass, EnumerationKind, enumeration};
use serde_json::{Map, Value};

use crate::identifier::check_identifier_syntax;
use crate::model::SignalType;

use super::error::{ImportError, ImportErrorKind as Kind, SourceForm};

/// The `S231` context IRI: the only one a document may bind `S231` to.
pub(super) const S231_IRI: &str = "http://data.ashrae.org/S231#";

/// The namespace IRI the emitter and the engine wrote through release 0.4.0. Kept only to refuse
/// a document that binds it, under any prefix.
pub(super) const RETIRED_S231_IRI: &str = "http://data.ashrae.org/S231P#";

/// The translation-software label (OBC §8.4). A block node may carry it; the importer reads
/// neither value, and the emitter writes its own on the root.
const TRANSLATION_SOFTWARE_KEYS: [&str; 2] = [
    "S231:translationSoftware",
    "S231:translationSoftwareVersion",
];

/// The package every block class and named type sits under, after the class IRI's `#`.
pub(super) const OBC_PACKAGE: &str = "Buildings.Controls.OBC.";

/// A CXF refusal at `subject`.
pub(super) fn refuse(subject: &str, kind: Kind) -> ImportError {
    ImportError::new(SourceForm::Cxf, subject, kind)
}

/// Keys every node kind may carry besides its own.
const COMMON_KEYS: [&str; 3] = ["@id", "@type", "S231:label"];

/// The node kinds, each with its own key whitelist and its own refusal for a foreign key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeKind {
    /// The root block.
    Root,
    /// A root (sequence) parameter.
    SequenceParameter,
    /// A boundary input or output.
    Boundary,
    /// A leaf block.
    Block,
    /// A block parameter.
    BlockParameter,
    /// A block input or output port.
    Port,
}

/// The descriptive parameter attributes the importer reads, drops and reports (decision 9).
pub(super) const PARAMETER_ATTRIBUTES: [&str; 5] = [
    "S231:unit",
    "S231:quantity",
    "S231:displayUnit",
    "S231:min",
    "S231:max",
];

/// Refuse a descriptive parameter attribute of a malformed shape (GitHub issue #559; owner
/// rulings of 2026-11-11), before it is dropped. The value is never read, but its shape is held
/// to what the engine's loader accepts there: a bound (`S231:min`, `S231:max`) is a JSON
/// number, a typed literal or a string (an expression); a term (`S231:unit`, `S231:quantity`,
/// `S231:displayUnit`) is a string, a typed literal or an IRI node. A typed literal here is an
/// object of exactly `@value` and `@type`, both strings, and an IRI node one of exactly `@id`, a
/// string: the strict shapes the rest of this reader accepts. No such string (the value itself, a
/// typed literal's `@value`, an IRI node's `@id`) may be empty or whitespace-only, as
/// [`str::trim`] reads whitespace. Anything else, `null` and a list included, is `wrong-shape`
/// naming the key.
pub(super) fn check_parameter_attribute(
    id: &str,
    key: &str,
    value: &Value,
) -> Result<(), ImportError> {
    let filled = |text: &str| !text.trim().is_empty();
    let typed_literal = |map: &Map<String, Value>| {
        map.len() == 2
            && matches!(map.get("@value"), Some(Value::String(text)) if filled(text))
            && matches!(map.get("@type"), Some(Value::String(_)))
    };
    let iri_node = |map: &Map<String, Value>| {
        map.len() == 1 && matches!(map.get("@id"), Some(Value::String(text)) if filled(text))
    };
    let bound = matches!(key, "S231:min" | "S231:max");
    let legal = match value {
        Value::String(text) => filled(text),
        Value::Number(_) => bound,
        Value::Object(map) => typed_literal(map) || (!bound && iri_node(map)),
        Value::Null | Value::Bool(_) | Value::Array(_) => false,
    };
    if legal {
        Ok(())
    } else {
        Err(refuse(
            id,
            Kind::WrongShape {
                key: key.to_owned(),
            },
        ))
    }
}

/// The attributes a block port may not carry (decision 8), in the order they are checked.
pub(super) const PORT_ATTRIBUTES: [&str; 6] = [
    "S231:unit",
    "S231:quantity",
    "S231:displayUnit",
    "S231:min",
    "S231:max",
    "S231:nominal",
];

/// The array keys of a parameter node.
const ARRAY_KEYS: [&str; 3] = [
    "S231:isArray",
    "S231:numberDimensions",
    "S231:sizeOfDimensions",
];

impl NodeKind {
    fn allowed(self) -> &'static [&'static str] {
        match self {
            Self::Root => &[
                "S231:hasParameter",
                "S231:hasInput",
                "S231:hasOutput",
                "S231:containsBlock",
                TRANSLATION_SOFTWARE_KEYS[0],
                TRANSLATION_SOFTWARE_KEYS[1],
            ],
            Self::Block => &[
                "S231:hasParameter",
                "S231:hasInput",
                "S231:hasOutput",
                TRANSLATION_SOFTWARE_KEYS[0],
                TRANSLATION_SOFTWARE_KEYS[1],
            ],
            Self::SequenceParameter => &[
                "S231:isOfDataType",
                "S231:value",
                "S231:unit",
                "S231:quantity",
                "S231:displayUnit",
                "S231:min",
                "S231:max",
            ],
            Self::BlockParameter => &[
                "S231:isOfDataType",
                "S231:value",
                "S231:unit",
                "S231:quantity",
                "S231:displayUnit",
                "S231:min",
                "S231:max",
                "S231:isArray",
                "S231:numberDimensions",
                "S231:sizeOfDimensions",
            ],
            Self::Boundary => &[
                "S231:isOfDataType",
                "S231:isConnectedTo",
                "S231:unit",
                "S231:quantity",
                "S231:displayUnit",
                "S231:min",
                "S231:max",
            ],
            Self::Port => &["S231:isOfDataType", "S231:isConnectedTo"],
        }
    }

    /// The kind a key outside the whitelist is refused with: the out-of-subset construct it
    /// spells, or [`Kind::UnknownKey`].
    fn foreign(self, key: &str) -> Kind {
        match (self, key) {
            (_, "S231:isConditionalComponent" | "S231:conditionalExpression") => {
                Kind::ConditionalComponent
            }
            (_, "S231:hasInstance") => Kind::InstanceMember,
            (_, "S231:hasConstant") => Kind::ConstantMember,
            (Self::SequenceParameter, k) if ARRAY_KEYS.contains(&k) => Kind::ArraySequenceParameter,
            (Self::Boundary | Self::Port, k) if ARRAY_KEYS.contains(&k) => Kind::ArrayConnector,
            (Self::Boundary, "S231:nominal" | "S231:unbounded") => Kind::ConnectorAttribute {
                attribute: key.to_owned(),
            },
            (Self::Port, k) if PORT_ATTRIBUTES.contains(&k) => Kind::PortAttribute {
                attribute: key.to_owned(),
            },
            _ => Kind::UnknownKey {
                key: key.to_owned(),
            },
        }
    }
}

/// Refuse any key outside `kind`'s whitelist and the common keys, with the kind it spells.
pub(super) fn check_keys(
    id: &str,
    node: &Map<String, Value>,
    kind: NodeKind,
) -> Result<(), ImportError> {
    let allowed = kind.allowed();
    match node
        .keys()
        .find(|k| !COMMON_KEYS.contains(&k.as_str()) && !allowed.contains(&k.as_str()))
    {
        Some(key) => Err(refuse(id, kind.foreign(key))),
        None => Ok(()),
    }
}

/// The `@id` references of a relation: one `{"@id"}` object or an array of them.
pub(super) fn references<'a>(
    id: &str,
    node: &'a Map<String, Value>,
    key: &str,
) -> Result<Vec<&'a str>, ImportError> {
    let wrong = || {
        refuse(
            id,
            Kind::WrongShape {
                key: key.to_owned(),
            },
        )
    };
    let one = |value: &'a Value| -> Result<&'a str, ImportError> {
        match value {
            Value::Object(map) if map.len() == 1 => match map.get("@id") {
                Some(Value::String(iri)) => Ok(iri.as_str()),
                _ => Err(wrong()),
            },
            _ => Err(wrong()),
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
        Some(_) => Err(refuse(
            id,
            Kind::WrongShape {
                key: key.to_owned(),
            },
        )),
    }
}

/// A value as written, for the report: a string's text, anything else compact JSON.
pub(super) fn spelled(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// What an `isOfDataType` names.
#[derive(Clone, Copy, Debug)]
pub(super) enum DataType {
    /// `S231:Real`, `S231:Integer` or `S231:Boolean`.
    Scalar(SignalType),
    /// `S231:String`.
    String,
    /// A row of the facade's named-type table: an enumeration or a constant package.
    Named(&'static EnumerationClass),
}

/// The one `isOfDataType` reference of a node, or `None` when absent.
pub(super) fn declared_type<'a>(
    id: &str,
    node: &'a Map<String, Value>,
) -> Result<Option<&'a str>, ImportError> {
    if !node.contains_key("S231:isOfDataType") {
        return Ok(None);
    }
    match references(id, node, "S231:isOfDataType")?.as_slice() {
        [one] => Ok(Some(one)),
        _ => Err(refuse(
            id,
            Kind::WrongShape {
                key: "S231:isOfDataType".to_owned(),
            },
        )),
    }
}

/// Resolve an `isOfDataType` IRI: an S231 scalar, or a class IRI whose fragment (after its last
/// `#`) is `Buildings.Controls.OBC.<class path>` of a row the facade table holds. Anything else
/// is [`Kind::UnknownDatatype`].
pub(super) fn data_type(id: &str, iri: &str) -> Result<DataType, ImportError> {
    let resolved = match iri {
        "S231:Real" => Some(DataType::Scalar(SignalType::Real)),
        "S231:Integer" => Some(DataType::Scalar(SignalType::Integer)),
        "S231:Boolean" => Some(DataType::Scalar(SignalType::Boolean)),
        "S231:String" => Some(DataType::String),
        _ => iri
            .rsplit_once('#')
            .and_then(|(_, fragment)| fragment.strip_prefix(OBC_PACKAGE))
            .and_then(enumeration)
            .map(DataType::Named),
    };
    resolved.ok_or_else(|| refuse(id, Kind::UnknownDatatype))
}

/// The IRI scheme the document uses: the root `@id` is `{namespace}{sequence}`, where
/// `sequence` is the trailing identifier, and every other node is `{root}.{local}`.
pub(super) struct Names<'a> {
    pub(super) root: &'a str,
}

impl<'a> Names<'a> {
    /// The local path under the root, checked as dot-separated identifiers.
    pub(super) fn local(&self, subject: &str, iri: &'a str) -> Result<Vec<&'a str>, ImportError> {
        let path = local_path(self.root, iri).ok_or_else(|| {
            refuse(
                subject,
                Kind::Unresolved {
                    reference: iri.to_owned(),
                },
            )
        })?;
        let parts: Vec<&str> = path.split('.').collect();
        if parts.iter().any(|p| check_identifier_syntax(p).is_err()) {
            return Err(refuse(iri, Kind::InvalidName));
        }
        Ok(parts)
    }
}

/// The path of `iri` under `root` (`block.member`), when `iri` is `{root}.{path}`.
pub(super) fn local_path<'a>(root: &str, iri: &'a str) -> Option<&'a str> {
    iri.strip_prefix(root)
        .and_then(|rest| rest.strip_prefix('.'))
}

/// A parameter node's `@type`, when written, is `S231:Parameter`.
pub(super) fn check_parameter_type(
    iri: &str,
    node: &Map<String, Value>,
) -> Result<(), ImportError> {
    match node.get("@type") {
        None => Ok(()),
        Some(Value::String(t)) if t == "S231:Parameter" => Ok(()),
        Some(_) => Err(refuse(
            iri,
            Kind::WrongShape {
                key: "@type".to_owned(),
            },
        )),
    }
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
        return Err(refuse(root, Kind::InvalidName));
    }
    Ok(name)
}

/// A connector's type, read from its `@type` and `isOfDataType`, which must agree.
pub(super) struct ConnectorType<'a> {
    /// The type the connector is read as.
    pub(super) signal: SignalType,
    /// The constant-package IRI an Integer connector was typed by, when it was (decision 21).
    pub(super) package: Option<&'a str>,
}

/// The type of a connector node from its `@type` (`S231:<Kind><side>`) and its `isOfDataType`.
/// An Integer connector typed by a constant package is read as an Integer and the package IRI
/// returned for the report (decision 21); a Real or Boolean one, or one typed by an enumeration,
/// is [`Kind::DatatypeMismatch`].
pub(super) fn connector_type<'a>(
    iri: &str,
    node: &'a Map<String, Value>,
    side: &str,
) -> Result<ConnectorType<'a>, ImportError> {
    let mismatch = || refuse(iri, Kind::DatatypeMismatch);
    let from_type = match node.get("@type") {
        Some(Value::String(t)) => {
            match t.strip_prefix("S231:").and_then(|k| k.strip_suffix(side)) {
                Some("Real") => SignalType::Real,
                Some("Integer") => SignalType::Integer,
                Some("Boolean") => SignalType::Boolean,
                _ => return Err(mismatch()),
            }
        }
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
    let Some(declared) = declared_type(iri, node)? else {
        return Ok(ConnectorType {
            signal: from_type,
            package: None,
        });
    };
    match data_type(iri, declared)? {
        DataType::Scalar(signal) if signal == from_type => Ok(ConnectorType {
            signal,
            package: None,
        }),
        DataType::Named(row)
            if row.kind == EnumerationKind::ConstantPackage && from_type == SignalType::Integer =>
        {
            Ok(ConnectorType {
                signal: SignalType::Integer,
                package: Some(declared),
            })
        }
        _ => Err(mismatch()),
    }
}
