//! The wire DTOs (decision 8): closed serde structs the schema is derived from, and their
//! conversion into the model.
//!
//! Every object denies unknown fields and requires every property; an absent value is `""`,
//! `[]` or `false`. A parameter `value` is one of five shapes (decision 11): a number, a boolean,
//! a string, `{"ref": "<sequence-param>"}` or `{"member": "<Type.Member>"}`. A sequence
//! parameter's value is a literal: the same shapes without the reference.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};

use crate::identifier::{Identifier, ProvenanceId};
use crate::model::{
    Assignment, Block, Boundary, Connection, Endpoint, EnumMember, Literal, ParamValue,
    SemanticPoint, Sequence, SequenceParameter, SignalType,
};

use super::WireError;

/// One authored sequence: a flat, single-root control sequence of boundary connectors, leaf
/// blocks from the Open Control Engine catalog and the connections between them.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Sequence")]
pub(crate) struct WireSequence {
    /// The sequence name: an identifier (`[A-Za-z_][A-Za-z0-9_]*`, at most 64 bytes).
    sequence: String,
    /// The provenance id this sequence derives from (`<format>:<id>`), or "" for none.
    extends: String,
    /// Sequence-level parameters, in authored order; block parameters may reference them.
    params: Vec<WireSequenceParam>,
    /// Boundary inputs, in authored order.
    inputs: Vec<WireBoundary>,
    /// Boundary outputs, in authored order.
    outputs: Vec<WireBoundary>,
    /// Leaf blocks, in authored order.
    blocks: Vec<WireBlock>,
    /// Connections, each "source -> target"; an endpoint is a boundary name or "block.port".
    connections: Vec<String>,
}

/// The type of a sequence parameter or a boundary connector.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[schemars(rename = "SignalType")]
pub(crate) enum WireType {
    // Plain comments, not doc comments: a documented variant would turn the schema's `enum` into
    // a `oneOf` of `const`s, outside the strict profile.
    // IEEE-754 binary64 Real.
    Real,
    // Signed 64-bit Integer.
    Integer,
    // Boolean.
    Boolean,
}

/// A sequence-level parameter.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "SequenceParameter")]
pub(crate) struct WireSequenceParam {
    /// The parameter name, an identifier.
    name: String,
    /// The parameter type.
    #[serde(rename = "type")]
    value_type: WireType,
    /// The value: a number (an Integer is written without a fraction or exponent), a boolean, a
    /// string or an enumeration member.
    value: WireLiteral,
}

/// A boundary input or output.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Boundary")]
pub(crate) struct WireBoundary {
    /// The connector name, an identifier.
    name: String,
    /// The connector type.
    #[serde(rename = "type")]
    value_type: WireType,
    /// The CXF quantity (for example "ThermodynamicTemperature"), or "".
    quantity: String,
    /// The CXF display unit (for example "degC"), or "".
    display_unit: String,
    /// The semantic slot; every field optional ("" or [] or false when absent).
    semantic: WireSemantic,
}

/// The semantic slot of a boundary connector. Checked for syntax only; only `unit` reaches the
/// engine.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "SemanticPoint")]
pub(crate) struct WireSemantic {
    /// A display name, or "".
    name: String,
    /// A label, or "".
    label: String,
    /// The unit (for example "K"), or "".
    unit: String,
    /// A Brick class, or "".
    brick: String,
    /// An ASHRAE 223P class, or "".
    s223: String,
    /// A QUDT quantity kind, or "".
    qudt: String,
    /// Haystack tags.
    haystack: Vec<String>,
    /// Whether the slot is a placeholder still to be filled.
    provisional: bool,
}

/// A leaf block instance.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Block")]
pub(crate) struct WireBlock {
    /// The instance name, an identifier.
    name: String,
    /// The catalog class path.
    #[serde(rename = "class")]
    #[schemars(schema_with = "crate::schema::class_schema")]
    class_path: String,
    /// Parameter assignments, in authored order; an array parameter is one assignment per
    /// element, named by the catalog template ("k_1", "K_1_2").
    params: Vec<WireAssignment>,
}

/// One block parameter assignment.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Assignment")]
pub(crate) struct WireAssignment {
    /// The catalog parameter name, or an array element name.
    name: String,
    /// The value.
    value: WireValue,
}

/// A reference to a sequence-level parameter.
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Reference")]
#[expect(dead_code, reason = "schema-only: `ShapeVisitor` reads the shape")]
pub(crate) struct WireRef {
    /// The sequence parameter's name.
    #[serde(rename = "ref")]
    reference: String,
}

/// An enumeration member.
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "EnumMember")]
#[expect(dead_code, reason = "schema-only: `ShapeVisitor` reads the shape")]
pub(crate) struct WireMember {
    /// The member as "Type.Member", Type the last segment of the enumeration class (for example
    /// "SimpleController.PI").
    member: String,
}

/// The five value shapes, as read.
enum Shape {
    Integer(i64),
    Real(f64),
    Boolean(bool),
    Text(String),
    Ref(String),
    Member(String),
}

struct ShapeVisitor;

impl<'de> Visitor<'de> for ShapeVisitor {
    type Value = Shape;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a number, a boolean, a string, {\"ref\": …} or {\"member\": …}")
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Shape, E> {
        Ok(Shape::Integer(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Shape, E> {
        i64::try_from(v)
            .map(Shape::Integer)
            .map_err(|_| E::custom("integer out of the i64 range"))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Shape, E> {
        Ok(Shape::Real(v))
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Shape, E> {
        Ok(Shape::Boolean(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Shape, E> {
        Ok(Shape::Text(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Shape, E> {
        Ok(Shape::Text(v))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Shape, A::Error> {
        let Some(key) = map.next_key::<String>()? else {
            return Err(de::Error::custom(
                "invalid type: an empty object, expected {\"ref\": …} or {\"member\": …}",
            ));
        };
        let shape = match key.as_str() {
            "ref" => Shape::Ref(map.next_value()?),
            "member" => Shape::Member(map.next_value()?),
            other => return Err(de::Error::unknown_field(other, &["ref", "member"])),
        };
        if let Some(extra) = map.next_key::<String>()? {
            return Err(de::Error::unknown_field(&extra, &[]));
        }
        Ok(shape)
    }
}

/// A block parameter value: any of the five shapes.
pub(crate) struct WireValue(ParamValue);

impl<'de> Deserialize<'de> for WireValue {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = match literal_of(deserializer.deserialize_any(ShapeVisitor)?) {
            Ok(literal) => ParamValue::Literal(literal),
            Err(reference) => ParamValue::Ref(reference),
        };
        Ok(Self(value))
    }
}

impl JsonSchema for WireValue {
    fn schema_name() -> Cow<'static, str> {
        "ParameterValue".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A block parameter value: a number (an Integer is written without a fraction or exponent), a boolean, a string (always text), a reference to a sequence parameter, or an enumeration member.",
            "anyOf": [
                { "type": "number" },
                { "type": "boolean" },
                { "type": "string" },
                generator.subschema_for::<WireRef>(),
                generator.subschema_for::<WireMember>(),
            ]
        })
    }
}

/// A sequence parameter value: the five shapes without the reference.
pub(crate) struct WireLiteral(Literal);

impl<'de> Deserialize<'de> for WireLiteral {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        literal_of(deserializer.deserialize_any(ShapeVisitor)?)
            .map(Self)
            .map_err(|_| {
                de::Error::custom(
                    "invalid type: a reference, expected a literal (a sequence parameter cannot reference another)",
                )
            })
    }
}

impl JsonSchema for WireLiteral {
    fn schema_name() -> Cow<'static, str> {
        "LiteralValue".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A sequence parameter value: a number (an Integer is written without a fraction or exponent), a boolean, a string, or an enumeration member.",
            "anyOf": [
                { "type": "number" },
                { "type": "boolean" },
                { "type": "string" },
                generator.subschema_for::<WireMember>(),
            ]
        })
    }
}

fn literal_of(shape: Shape) -> Result<Literal, Identifier> {
    Ok(match shape {
        Shape::Integer(v) => Literal::Integer(v),
        Shape::Real(v) => Literal::Real(v),
        Shape::Boolean(v) => Literal::Boolean(v),
        Shape::Text(v) => Literal::Text(v),
        Shape::Member(v) => Literal::EnumMember(EnumMember::parse(&v)),
        Shape::Ref(name) => return Err(Identifier::unchecked(name)),
    })
}

fn optional(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

impl WireType {
    fn model(self) -> SignalType {
        match self {
            Self::Real => SignalType::Real,
            Self::Integer => SignalType::Integer,
            Self::Boolean => SignalType::Boolean,
        }
    }
}

impl WireBoundary {
    fn model(self) -> Boundary {
        let s = self.semantic;
        Boundary {
            name: Identifier::unchecked(self.name),
            value_type: self.value_type.model(),
            quantity: optional(self.quantity),
            display_unit: optional(self.display_unit),
            semantic: SemanticPoint {
                name: optional(s.name),
                label: optional(s.label),
                unit: optional(s.unit),
                brick: optional(s.brick),
                s223: optional(s.s223),
                qudt: optional(s.qudt),
                haystack: s.haystack,
                provisional: s.provisional,
            },
        }
    }
}

/// Split a `source -> target` string at its one arrow. The endpoint parts are kept unchecked,
/// so a malformed endpoint becomes a `malformed-connection` finding; a string without exactly one
/// arrow cannot be split into endpoints at all and is refused here.
fn connection_of(index: usize, text: String) -> Result<Connection, WireError> {
    let mut parts = text.split("->");
    match (parts.next(), parts.next(), parts.next()) {
        (Some(source), Some(target), None) => Ok(Connection {
            source: Endpoint::parse(source.trim_matches(' ')),
            target: Endpoint::parse(target.trim_matches(' ')),
        }),
        _ => Err(WireError::MalformedConnection { index, text }),
    }
}

impl WireSequence {
    /// Convert into the model. Names are kept unchecked for the validator.
    pub(super) fn into_model(self) -> Result<Sequence, WireError> {
        let params = self
            .params
            .into_iter()
            .map(|p| SequenceParameter {
                name: Identifier::unchecked(p.name),
                value_type: p.value_type.model(),
                value: p.value.0,
            })
            .collect();
        let blocks = self
            .blocks
            .into_iter()
            .map(|b| Block {
                name: Identifier::unchecked(b.name),
                class_path: b.class_path,
                assignments: b
                    .params
                    .into_iter()
                    .map(|a| Assignment {
                        name: Identifier::unchecked(a.name),
                        value: a.value.0,
                    })
                    .collect(),
            })
            .collect();
        let connections = self
            .connections
            .into_iter()
            .enumerate()
            .map(|(index, text)| connection_of(index, text))
            .collect::<Result<_, _>>()?;
        Ok(Sequence {
            name: Identifier::unchecked(self.sequence),
            extends: optional(self.extends).map(ProvenanceId::unchecked),
            params,
            inputs: self.inputs.into_iter().map(WireBoundary::model).collect(),
            outputs: self.outputs.into_iter().map(WireBoundary::model).collect(),
            blocks,
            connections,
        })
    }
}
