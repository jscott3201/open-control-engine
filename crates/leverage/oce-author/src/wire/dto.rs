//! The wire DTOs (decision 8): closed serde structs the schema is derived from, and their
//! conversion into the model.
//!
//! Every object denies unknown fields and requires every property; an absent value is `""`,
//! `[]` or `false`. A parameter `value` is one of five shapes (decision 11): a number, a boolean,
//! a string, `{"ref": "<sequence-param>"}` or `{"member": "<Type.Member>"}`. A sequence
//! parameter's value is a literal: the same shapes without the reference. A sequence parameter's
//! `type` is a scalar type or an enumeration type name, and a boundary's `min` and `max` are lists
//! of at most one number (importer note, decisions 1 and 5; issue #492).
//!
//! The reader fills these through the closed-shape adapter (`super::closed`), which reads each
//! struct from an object only and each enum from a string only (GitHub #544). Each is named for its
//! schema definition with `#[serde(rename)]`, which `schemars` reads too, so a refusal names the
//! object the schema defines there (`expected a SemanticPoint object`).

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};

use crate::identifier::{Identifier, ProvenanceId};
use crate::model::{
    Assignment, Block, Bound, Boundary, Connection, Endpoint, Literal, MemberRef, ParamValue,
    ParameterType, SemanticPoint, Sequence, SequenceParameter, SignalType,
};

use super::WireError;

/// One authored sequence: a flat, single-root control sequence of boundary connectors, leaf
/// blocks from the Open Control Engine catalog and the connections between them.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename = "Sequence")]
#[schemars(title = "Sequence")]
pub(crate) struct WireSequence {
    /// The sequence name: an identifier (`[A-Za-z_][A-Za-z0-9_]*`, at most 64 bytes).
    sequence: String,
    /// The provenance id this sequence derives from (`<format>:<id>`), or "" for none.
    derived_from: String,
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

/// The type of a boundary connector.
#[derive(Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename = "SignalType")]
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
#[serde(deny_unknown_fields, rename = "SequenceParameter")]
pub(crate) struct WireSequenceParam {
    /// The parameter name, an identifier.
    name: String,
    /// The parameter type: "Real", "Integer", "Boolean", or an enumeration type name (the last
    /// segment of its class, for example "SimpleController"), whose value is then one of its
    /// members.
    #[serde(rename = "type")]
    #[schemars(schema_with = "crate::schema::parameter_type_schema")]
    value_type: String,
    /// The value: a number (an Integer is written without a fraction or exponent), a boolean, a
    /// string or a member of an enumeration or library constant package.
    value: WireLiteral,
}

/// A boundary input or output.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename = "Boundary")]
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
    /// The CXF minimum: [] for none, or a list of exactly one number (at most one item; a second
    /// is refused). Real or Integer boundaries only; an Integer boundary takes an Integer.
    min: WireBound,
    /// The CXF maximum: [] for none, or a list of exactly one number (at most one item; a second
    /// is refused). Real or Integer boundaries only; an Integer boundary takes an Integer.
    max: WireBound,
    /// The semantic slot; every field optional ("" or [] or false when absent).
    semantic: WireSemantic,
}

/// The semantic slot of a boundary connector. Checked for syntax only; only `unit` reaches the
/// engine.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename = "SemanticPoint")]
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
#[serde(deny_unknown_fields, rename = "Block")]
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
#[serde(deny_unknown_fields, rename = "Assignment")]
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

/// A member of an enumeration or of a library constant package.
#[derive(JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "Member")]
#[expect(dead_code, reason = "schema-only: `ShapeVisitor` reads the shape")]
pub(crate) struct WireMember {
    /// The member as "Type.Member", Type the last segment of the enumeration or constant package
    /// class (for example "SimpleController.PI" or "OperationModes.occupied").
    member: String,
}

/// The message prefix of a bound list with a second item; [`super::classify`] maps it to
/// [`WireError::BoundListTooLong`].
pub(super) const BOUND_LIST_TOO_LONG: &str = "a bound list holds at most one number";

/// A boundary bound: `[]` or `[<number>]`.
pub(crate) struct WireBound(Option<Bound>);

struct BoundVisitor;

impl<'de> Visitor<'de> for BoundVisitor {
    type Value = WireBound;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of at most one number")
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<WireBound, A::Error> {
        let Some(shape) = seq.next_element::<BoundShape>()? else {
            return Ok(WireBound(None));
        };
        if seq.next_element::<de::IgnoredAny>()?.is_some() {
            return Err(de::Error::custom(BOUND_LIST_TOO_LONG));
        }
        Ok(WireBound(Some(shape.0)))
    }
}

impl<'de> Deserialize<'de> for WireBound {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(BoundVisitor)
    }
}

impl JsonSchema for WireBound {
    fn schema_name() -> Cow<'static, str> {
        "BoundList".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "array",
            "items": { "type": "number" },
        })
    }
}

/// One bound item: a number token, kept as an Integer when it has no fraction or exponent.
struct BoundShape(Bound);

impl<'de> Deserialize<'de> for BoundShape {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match deserializer.deserialize_any(ShapeVisitor)? {
            Shape::Integer(v) => Ok(Self(Bound::Integer(v))),
            Shape::Real(v) => Ok(Self(Bound::Real(v))),
            _ => Err(de::Error::custom("invalid type: a bound is a number")),
        }
    }
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
        Shape::Member(v) => Literal::Member(MemberRef::parse(&v)),
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
        let mut boundary = Boundary::new(Identifier::unchecked(self.name), self.value_type.model());
        boundary.quantity = optional(self.quantity);
        boundary.display_unit = optional(self.display_unit);
        boundary.min = self.min.0;
        boundary.max = self.max.0;
        boundary.semantic = SemanticPoint {
            name: optional(s.name),
            label: optional(s.label),
            unit: optional(s.unit),
            brick: optional(s.brick),
            s223: optional(s.s223),
            qudt: optional(s.qudt),
            haystack: s.haystack,
            provisional: s.provisional,
        };
        boundary
    }
}

/// Split a `source -> target` string at its one arrow. The endpoint parts are kept unchecked,
/// so a malformed endpoint becomes a `malformed-connection` finding; a string without exactly one
/// arrow cannot be split into endpoints at all and is refused here.
fn connection_of(index: usize, text: String) -> Result<Connection, WireError> {
    split_connection(&text).ok_or(WireError::MalformedConnection { index, text })
}

/// The model's connection parser: split `source -> target` at its one arrow, trimming spaces
/// around each endpoint and keeping the parts unchecked. `None` without exactly one `->`. The
/// wire form and the v0 graph importer share it.
pub(crate) fn split_connection(text: &str) -> Option<Connection> {
    let mut parts = text.split("->");
    match (parts.next(), parts.next(), parts.next()) {
        (Some(source), Some(target), None) => Some(Connection::new(
            Endpoint::parse(source.trim_matches(' ')),
            Endpoint::parse(target.trim_matches(' ')),
        )),
        _ => None,
    }
}

impl WireSequence {
    /// Convert into the model. Names are kept unchecked for the validator.
    pub(super) fn into_model(self) -> Result<Sequence, WireError> {
        let params = self
            .params
            .into_iter()
            .map(|p| {
                SequenceParameter::new(
                    Identifier::unchecked(p.name),
                    ParameterType::parse(&p.value_type),
                    p.value.0,
                )
            })
            .collect();
        let blocks = self
            .blocks
            .into_iter()
            .map(|b| {
                let mut block = Block::new(Identifier::unchecked(b.name), b.class_path);
                block.assignments = b
                    .params
                    .into_iter()
                    .map(|a| Assignment::new(Identifier::unchecked(a.name), a.value.0))
                    .collect();
                block
            })
            .collect();
        let connections = self
            .connections
            .into_iter()
            .enumerate()
            .map(|(index, text)| connection_of(index, text))
            .collect::<Result<_, _>>()?;
        let mut sequence = Sequence::new(Identifier::unchecked(self.sequence));
        sequence.derived_from = optional(self.derived_from).map(ProvenanceId::unchecked);
        sequence.params = params;
        sequence.inputs = self.inputs.into_iter().map(WireBoundary::model).collect();
        sequence.outputs = self.outputs.into_iter().map(WireBoundary::model).collect();
        sequence.blocks = blocks;
        sequence.connections = connections;
        Ok(sequence)
    }
}
