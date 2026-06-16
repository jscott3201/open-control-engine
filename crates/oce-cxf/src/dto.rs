//! Layer A — the **lossless** JSON-LD DTO (doc 04 §6).
//!
//! Layer A mirrors the CXF JSON-LD 1:1. Its contract (RT-1): `parse_document` then serialize
//! **reproduces the input losslessly**, modulo JSON key ordering / whitespace — editing tools and
//! round-trip export must not drop cosmetic or unknown keys. This is achieved with a
//! `#[serde(flatten)] other` map on both [`CxfDocument`] and [`Node`] (R-8), capturing every key
//! not explicitly modeled (`icon`/`diagram`/`graphics`/`documentation`/`qudt:*`/`cdlLineNum*`/
//! `quantity`/`unit`/`displayUnit`/`redeclare`/… ).
//!
//! Layer A assigns **no meaning** — it is a dumb mirror. All interpretation (classification,
//! library join, overlay merge, connections, grounding) happens in the resolver (§7, M1-PR-5).
//!
//! ## RT-1 round-trip boundaries (intentional normalizations)
//!
//! Losslessness holds for the **CXF value domain** with two deliberate normalizations:
//! - An **empty array** on a [`OneOrMany`] edge (`"S231:hasInput": []`) round-trips to **absent**
//!   — semantically identical (no members) for these structural edges.
//! - Bare JSON **numbers in passthrough (`other`) maps** are subject to `f64` precision (the crate
//!   does not enable `serde_json/arbitrary_precision`, which interacts badly with `flatten` +
//!   untagged enums). This is a non-issue for CXF: typed literals carry their numerics as `@value`
//!   **strings** (exact), and CDL `Integer`s are within `i64`. Only large/high-precision numbers in
//!   *cosmetic* annotations (e.g. an out-of-`i64` integer) would normalize; CXF carries none.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The top-level CXF document: a `@context` and a flat `@graph` of [`Node`]s, plus any other
/// top-level keys (rare) preserved for lossless round-trip.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CxfDocument {
    /// The JSON-LD `@context` (prefix → IRI map, an array of maps, or a string IRI).
    #[serde(rename = "@context")]
    pub context: Context,
    /// The flat node array — no deep nesting of block bodies (§8.1).
    #[serde(rename = "@graph")]
    pub graph: Vec<Node>,
    /// Any top-level keys other than `@context`/`@graph`, kept for lossless round-trip (R-8).
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

/// The JSON-LD `@context`: a single prefix→IRI map, an array of such, or a bare string IRI
/// (modeled permissively, R-2/R-3).
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Context {
    /// A single prefix → IRI map.
    Map(BTreeMap<String, serde_json::Value>),
    /// An array of context entries (maps and/or IRIs).
    List(Vec<serde_json::Value>),
    /// A single string IRI.
    Iri(String),
}

/// One `@graph` node — a flat, relational record (R-1: never a nested block tree). Every
/// structural edge is a [`OneOrMany`] of [`IriRef`] cross-references; scalars/overlays are
/// optional; everything unmodeled flows through `other` (R-8).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Node {
    /// The raw IRI `@id` (expanded lazily against `@context`, R-3).
    #[serde(rename = "@id")]
    pub id: String,
    /// `@type`: an `S231:*` class for library/composite nodes, OR a concrete class IRI for an
    /// instance (G-3). Absent for overlay nodes.
    #[serde(rename = "@type", default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<OneOrMany<String>>,

    // ---- structural edges (each OneOrMany<IriRef>) ----
    /// `S231:hasInput` — boundary/instance input ports.
    #[serde(
        rename = "S231:hasInput",
        default,
        skip_serializing_if = "OneOrMany::is_empty"
    )]
    pub has_input: OneOrMany<IriRef>,
    /// `S231:hasOutput` — boundary/instance output ports.
    #[serde(
        rename = "S231:hasOutput",
        default,
        skip_serializing_if = "OneOrMany::is_empty"
    )]
    pub has_output: OneOrMany<IriRef>,
    /// `S231:hasParameter` — declared parameters.
    #[serde(
        rename = "S231:hasParameter",
        default,
        skip_serializing_if = "OneOrMany::is_empty"
    )]
    pub has_parameter: OneOrMany<IriRef>,
    /// `S231:hasConstant` — declared constants.
    #[serde(
        rename = "S231:hasConstant",
        default,
        skip_serializing_if = "OneOrMany::is_empty"
    )]
    pub has_constant: OneOrMany<IriRef>,
    /// `S231:containsBlock` — child blocks (G-1: presence marks a composite).
    #[serde(
        rename = "S231:containsBlock",
        default,
        skip_serializing_if = "OneOrMany::is_empty"
    )]
    pub contains_block: OneOrMany<IriRef>,
    /// `S231:hasInstance` — an instance's own members.
    #[serde(
        rename = "S231:hasInstance",
        default,
        skip_serializing_if = "OneOrMany::is_empty"
    )]
    pub has_instance: OneOrMany<IriRef>,
    /// `S231:isConnectedTo` — source-anchored output→input connection target(s) (§3.4).
    #[serde(
        rename = "S231:isConnectedTo",
        default,
        skip_serializing_if = "OneOrMany::is_empty"
    )]
    pub is_connected_to: OneOrMany<IriRef>,
    /// `S231:isOfDataType` — the connector's declared data type IRI.
    #[serde(
        rename = "S231:isOfDataType",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub is_of_data_type: Option<IriRef>,

    // ---- scalars / overlays ----
    /// `S231:label` — the local (short) name.
    #[serde(
        rename = "S231:label",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub label: Option<OneOrMany<String>>,
    /// `S231:accessSpecifier` — `public`/`protected`.
    #[serde(
        rename = "S231:accessSpecifier",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub access: Option<String>,
    /// `S231:value` — parameter/constant binding (a [`CxfValue`] sum, R-5).
    #[serde(
        rename = "S231:value",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<CxfValue>,
    /// `S231:min` — lower-bound binding.
    #[serde(rename = "S231:min", default, skip_serializing_if = "Option::is_none")]
    pub min: Option<CxfValue>,
    /// `S231:max` — upper-bound binding.
    #[serde(rename = "S231:max", default, skip_serializing_if = "Option::is_none")]
    pub max: Option<CxfValue>,
    /// `S231:isFinal` — whether an overlay value is final.
    #[serde(
        rename = "S231:isFinal",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub is_final: Option<bool>,
    /// `S231:isArray` — whether this declares an array (preserved encoding, §3.6).
    #[serde(
        rename = "S231:isArray",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub is_array: Option<bool>,
    /// `S231:numberDimensions` — array rank (preserved encoding).
    #[serde(
        rename = "S231:numberDimensions",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub n_dims: Option<i64>,
    /// `S231:sizeOfDimensions` — array shape string, e.g. `"(nin)"` (preserved encoding).
    #[serde(
        rename = "S231:sizeOfDimensions",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub size_dims: Option<String>,
    /// `S231:hasFmuPath` — the FMU reference of an `ExtensionBlock` (§3.7).
    #[serde(
        rename = "S231:hasFmuPath",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fmu_path: Option<String>,

    // ---- optional polymorphism triples (elaborated away upstream; read defensively, R-4) ----
    /// `S231:isConditionalComponent`.
    #[serde(
        rename = "S231:isConditionalComponent",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub is_conditional: Option<bool>,
    /// `S231:conditionalExpression`.
    #[serde(
        rename = "S231:conditionalExpression",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cond_expr: Option<String>,
    /// `S231:isReplaceable`.
    #[serde(
        rename = "S231:isReplaceable",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub is_replaceable: Option<bool>,

    /// Everything else (icon/diagram/graphics/documentation/`qudt:*`/`cdlLineNum*`/quantity/
    /// unit/displayUnit/redeclare/…) — opaque passthrough for lossless round-trip (R-8).
    #[serde(flatten)]
    pub other: BTreeMap<String, serde_json::Value>,
}

/// A JSON-LD cross-reference: `{"@id": "..."}`. Any other keys on the reference object (JSON-LD
/// permits `@index`/`@type` on a reference) flow through `other` for lossless round-trip (R-8).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct IriRef {
    /// The referenced node's IRI.
    #[serde(rename = "@id")]
    pub id: String,
    /// Any other keys on the reference object, preserved for lossless round-trip (R-8).
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub other: BTreeMap<String, serde_json::Value>,
}

/// A JSON-LD property that may appear as a single value **or** an array (R: research §11). Custom
/// serde: deserialize [`OneOrMany::One`] from a scalar/object and [`OneOrMany::Many`] from an
/// array; serialize `One` as a scalar/object (**not** a 1-element array, to match producer byte
/// style) and `Many` as an array. [`OneOrMany::None`] is the field default and is skipped on
/// serialization via `skip_serializing_if = "OneOrMany::is_empty"`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum OneOrMany<T> {
    /// Absent (the field default).
    #[default]
    None,
    /// Exactly one value (serialized as a bare scalar/object).
    One(T),
    /// Several values (serialized as an array).
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    /// View the contents as a slice: `None` → empty, `One` → a 1-element slice, `Many` → the vec.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        match self {
            OneOrMany::None => &[],
            OneOrMany::One(t) => std::slice::from_ref(t),
            OneOrMany::Many(v) => v.as_slice(),
        }
    }

    /// Whether there are no values (`None`, or an empty `Many`).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }

    /// Number of values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    /// Iterate over the values in document order.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    /// Consume into a `Vec` (`None` → empty, `One` → 1-element, `Many` → the vec).
    #[must_use]
    pub fn into_vec(self) -> Vec<T> {
        match self {
            OneOrMany::None => Vec::new(),
            OneOrMany::One(t) => vec![t],
            OneOrMany::Many(v) => v,
        }
    }
}

impl<T: Serialize> Serialize for OneOrMany<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            // `None` fields are skipped via `skip_serializing_if`; serialize as null if ever reached.
            OneOrMany::None => serializer.serialize_none(),
            OneOrMany::One(t) => t.serialize(serializer),
            OneOrMany::Many(v) => v.serialize(serializer),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OneOrMany<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Try array → `Many` first, then a single value → `One`. A bare scalar/object is `One`.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Helper<T> {
            Many(Vec<T>),
            One(T),
        }
        Ok(match Helper::deserialize(deserializer)? {
            Helper::Many(v) => OneOrMany::Many(v),
            Helper::One(t) => OneOrMany::One(t),
        })
    }
}

/// The `S231:value`/`min`/`max` sum type (§3.5, R-5). **Untagged**; the variant order is
/// load-bearing (serde tries top-down): `false`→`Bool`, `0`→`Int`, `1.5`→`Float`, a
/// `{@value,@type}` object→`Typed`, and any remaining string→`Expr`. (A JSON integer matches
/// `Int` before `Float`.) The resolver grounds each variant **literal-natural** (a bare `Int`
/// becomes `Value::Integer`); it does **not** re-type against the parameter's declared type — the
/// block constructor performs Modelica `Int→Real` promotion (`oce-blocks` `real_param`) when a
/// `Real` parameter receives an integer literal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum CxfValue {
    /// A bare boolean literal.
    Bool(bool),
    /// A bare integer literal.
    Int(i64),
    /// A bare real literal.
    Float(f64),
    /// A typed literal object `{"@value": "...", "@type": "...XMLSchema#double"}`. Any other keys
    /// on the object (JSON-LD `@language`/`@index`/…) flow through `extra` for losslessness (R-8).
    Typed {
        /// The literal lexical form.
        #[serde(rename = "@value")]
        value: String,
        /// The XSD datatype IRI.
        #[serde(rename = "@type")]
        datatype: String,
        /// Any other keys on the typed-literal object, preserved for lossless round-trip (R-8).
        #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: BTreeMap<String, serde_json::Value>,
    },
    /// An unevaluated CDL/Modelica expression string, or a fully-qualified enumeration value.
    Expr(String),
}
