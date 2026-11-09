//! The typed authoring model (authoring note, "The authoring model").
//!
//! One [`Sequence`] is one flat, single-root document: sequence parameters, boundary inputs and
//! outputs, leaf blocks and connections, every list an ordered vector kept in authored order.
//! Nothing here is sorted, evaluated or resolved: the model is what the author wrote, and
//! [`crate::validate()`] judges it against the catalog.
//!
//! Every statement (a sequence parameter, a boundary, a block, an assignment, a connection) carries
//! [`Annotations`]: text-form-only facts (leading comments, a blank line before it) that the model
//! keeps so the text form's `fmt` loses nothing (decision 13). The sequence itself keeps the
//! comments above its `sequence` line ([`Sequence::annotations`]) and those after its last statement
//! ([`Sequence::trailing_comments`]); the text form needed both (issue #482). Assignments and
//! sequence parameters also keep a text-only unit suffix. The wire form and the emitter ignore all
//! of them; equality is structural and does compare them.
//!
//! The model types are `#[non_exhaustive]`: build them through their constructors or
//! [`crate::SequenceBuilder`], then set the public fields.

use crate::identifier::{Identifier, ProvenanceId};

/// Text-form-only annotations of one statement. Never written to the wire form and never
/// emitted; a wire document reads with empty annotations.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Annotations {
    /// Comment lines immediately before the statement, without the leading `#`, in order.
    pub comments: Vec<String>,
    /// Whether a blank line precedes the statement (the printer collapses runs to one).
    pub blank_line_before: bool,
}

/// One authored sequence.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Sequence {
    /// The sequence name; the root block's local name.
    pub name: Identifier,
    /// The opaque provenance id this sequence derives from (decision 5), or `None`.
    pub extends: Option<ProvenanceId>,
    /// Sequence-level parameters, lowered onto the root block (decision 3).
    pub params: Vec<SequenceParameter>,
    /// Boundary inputs.
    pub inputs: Vec<Boundary>,
    /// Boundary outputs.
    pub outputs: Vec<Boundary>,
    /// Leaf blocks.
    pub blocks: Vec<Block>,
    /// Connections, `source -> target`.
    pub connections: Vec<Connection>,
    /// Text-form annotations of the header: the comment lines above the `sequence` line (a
    /// comment between `sequence` and `extends` folds in here). `blank_line_before` has no meaning
    /// at the top of a file; the text printer ignores it and the parser never sets it. Never
    /// written to the wire form and never emitted (issue #482).
    pub annotations: Annotations,
    /// Comment lines after the last statement, without the leading `#`, in order. Text-only like
    /// [`Annotations`] (issue #482).
    pub trailing_comments: Vec<String>,
}

impl Sequence {
    /// An empty sequence named `name`, extending nothing, with no annotations.
    #[must_use]
    pub fn new(name: Identifier) -> Self {
        Self {
            name,
            extends: None,
            params: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            blocks: Vec::new(),
            connections: Vec::new(),
            annotations: Annotations::default(),
            trailing_comments: Vec::new(),
        }
    }
}

/// The type of a boundary connector, and the scalar types of a sequence parameter
/// ([`ParameterType`]).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SignalType {
    /// IEEE-754 binary64 Real.
    Real,
    /// Signed Integer, carried in an `i64`.
    Integer,
    /// Boolean.
    Boolean,
}

impl SignalType {
    /// The spelling in both forms: `Real`, `Integer` or `Boolean`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Real => "Real",
            Self::Integer => "Integer",
            Self::Boolean => "Boolean",
        }
    }
}

/// The type of a sequence parameter: a scalar [`SignalType`], or an enumeration named by the last
/// segment of its class (`SimpleController`, `EnergyStandard`), the name the `Type.Member` spelling
/// uses (importer note, decision 1; issue #492). The type name is kept as authored and resolved
/// by the validator through `oce_api::enumeration_types()`; a constant package is never a
/// parameter type (`unknown-enum-type`).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParameterType {
    /// IEEE-754 binary64 Real.
    Real,
    /// Signed Integer, carried in an `i64`.
    Integer,
    /// Boolean.
    Boolean,
    /// An enumeration, by the last segment of its class path.
    Enum(String),
}

impl ParameterType {
    /// Read a spelling in either form: `Real`, `Integer` and `Boolean` are the scalar types and
    /// any other spelling is an enumeration type name, kept unchecked for the validator.
    #[must_use]
    pub fn parse(spelling: &str) -> Self {
        match spelling {
            "Real" => Self::Real,
            "Integer" => Self::Integer,
            "Boolean" => Self::Boolean,
            other => Self::Enum(other.to_owned()),
        }
    }

    /// The spelling in both forms: `Real`, `Integer`, `Boolean` or the enumeration type name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Real => "Real",
            Self::Integer => "Integer",
            Self::Boolean => "Boolean",
            Self::Enum(type_name) => type_name,
        }
    }

    /// The scalar type, or `None` for an enumeration.
    #[must_use]
    pub fn signal(&self) -> Option<SignalType> {
        match self {
            Self::Real => Some(SignalType::Real),
            Self::Integer => Some(SignalType::Integer),
            Self::Boolean => Some(SignalType::Boolean),
            Self::Enum(_) => None,
        }
    }
}

impl From<SignalType> for ParameterType {
    fn from(value_type: SignalType) -> Self {
        match value_type {
            SignalType::Real => Self::Real,
            SignalType::Integer => Self::Integer,
            SignalType::Boolean => Self::Boolean,
        }
    }
}

/// A sequence-level parameter: a name, a type and a literal value.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceParameter {
    /// The parameter name.
    pub name: Identifier,
    /// The declared type.
    pub value_type: ParameterType,
    /// The value. A `Real` parameter accepts an [`Literal::Integer`]; an `Integer` parameter
    /// accepts a member of a constant package; an `Enum` parameter takes a member of its own
    /// type; anything else must match the declared type (the validator reports
    /// `parameter-kind-mismatch` or `unknown-enum-literal` otherwise).
    pub value: Literal,
    /// The text form's unit suffix on the value (`1 K`), text-only (decision 13).
    pub suffix: Option<String>,
    /// Text-form annotations.
    pub annotations: Annotations,
}

impl SequenceParameter {
    /// A parameter with no suffix and no annotations. `value_type` is a [`ParameterType`] or a
    /// [`SignalType`].
    #[must_use]
    pub fn new(name: Identifier, value_type: impl Into<ParameterType>, value: Literal) -> Self {
        Self {
            name,
            value_type: value_type.into(),
            value,
            suffix: None,
            annotations: Annotations::default(),
        }
    }
}

/// A boundary input or output declaration.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Boundary {
    /// The connector name.
    pub name: Identifier,
    /// The connector type.
    pub value_type: SignalType,
    /// The CXF `quantity` attribute, if authored (decision 21); an atom.
    pub quantity: Option<String>,
    /// The CXF `displayUnit` attribute, if authored (decision 21); an atom.
    pub display_unit: Option<String>,
    /// The CXF `min` attribute, if authored (importer note, decision 4; issue #492). On an input
    /// it bounds the effective acceptance domain, which is inside the canonical content id.
    pub min: Option<Bound>,
    /// The CXF `max` attribute, if authored (as [`Boundary::min`]).
    pub max: Option<Bound>,
    /// The semantic slot (decision 2).
    pub semantic: SemanticPoint,
    /// Text-form annotations.
    pub annotations: Annotations,
}

impl Boundary {
    /// A boundary with no quantity, display unit, bound or semantic field.
    #[must_use]
    pub fn new(name: Identifier, value_type: SignalType) -> Self {
        Self {
            name,
            value_type,
            quantity: None,
            display_unit: None,
            min: None,
            max: None,
            semantic: SemanticPoint::default(),
            annotations: Annotations::default(),
        }
    }
}

/// A boundary bound as authored: a number literal, never a reference (the engine grounds bounds
/// in an empty scope). Equality compares a Real by its bits, as [`Literal`] does. The validator
/// accepts an Integer on a Real or Integer boundary (widened on a Real one, exactly up to 2^53) and
/// a finite Real on a Real boundary only (importer note, "Bounds on boundary declarations").
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub enum Bound {
    /// An integer token: no fraction and no exponent.
    Integer(i64),
    /// A real token: a fraction or an exponent.
    Real(f64),
}

impl PartialEq for Bound {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Real(a), Self::Real(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl Bound {
    /// The bound as the engine reads it on a Real connector: a Real as it is, an Integer
    /// converted with `as` (exact up to 2^53 in magnitude, which stage 1 enforces).
    #[must_use]
    pub fn as_f64(self) -> f64 {
        match self {
            Self::Integer(v) => v as f64,
            Self::Real(v) => v,
        }
    }

    /// The bound's kind, for messages: `Integer` or `Real`.
    #[must_use]
    pub fn kind_name(self) -> &'static str {
        match self {
            Self::Integer(_) => "Integer",
            Self::Real(_) => "Real",
        }
    }
}

/// The semantic slot of a boundary connector (decision 2): a closed struct of optional fields,
/// syntax-checked by the validator and never emitted as a semantic record before M8. `unit` is
/// emitted as the CXF `unit` attribute.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticPoint {
    /// A display name (free text).
    pub name: Option<String>,
    /// A label (free text).
    pub label: Option<String>,
    /// The unit, an atom (`K`, `Pa`).
    pub unit: Option<String>,
    /// A Brick class, an atom.
    pub brick: Option<String>,
    /// An ASHRAE 223P class, an atom.
    pub s223: Option<String>,
    /// A QUDT quantity kind, an atom.
    pub qudt: Option<String>,
    /// Haystack tags, each an atom, in authored order.
    pub haystack: Vec<String>,
    /// Whether the slot is a placeholder still to be filled.
    pub provisional: bool,
}

/// A leaf block instance.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The instance name.
    pub name: Identifier,
    /// The catalog class path, verbatim (`CDL.Reals.Hysteresis`).
    pub class_path: String,
    /// Parameter assignments, in authored order.
    pub assignments: Vec<Assignment>,
    /// Text-form annotations.
    pub annotations: Annotations,
}

impl Block {
    /// A block with no assignments.
    #[must_use]
    pub fn new(name: Identifier, class_path: impl Into<String>) -> Self {
        Self {
            name,
            class_path: class_path.into(),
            assignments: Vec::new(),
            annotations: Annotations::default(),
        }
    }
}

/// One parameter assignment on a block. An array parameter is assigned one element at a time,
/// under the element name [`oce_api::param_element_name`] mints (`k_2`, `K_1_2`; decision 15).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    /// The catalog parameter name, or an array element name.
    pub name: Identifier,
    /// The value.
    pub value: ParamValue,
    /// The text form's unit suffix on a literal value (`86400 s`), text-only (decision 13).
    pub suffix: Option<String>,
    /// Text-form annotations.
    pub annotations: Annotations,
}

impl Assignment {
    /// An assignment with no suffix and no annotations.
    #[must_use]
    pub fn new(name: Identifier, value: impl Into<ParamValue>) -> Self {
        Self {
            name,
            value: value.into(),
            suffix: None,
            annotations: Annotations::default(),
        }
    }
}

/// The value of a block parameter assignment (decision 3): a literal or a reference to a
/// sequence parameter, never an expression.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    /// A literal: a number, a Boolean, a string or a member of a named type.
    Literal(Literal),
    /// A reference to a sequence parameter, by name, resolved by the engine's scope rules.
    Ref(Identifier),
}

impl From<Literal> for ParamValue {
    fn from(literal: Literal) -> Self {
        Self::Literal(literal)
    }
}

/// A tagged literal. Equality compares a Real by its bits, so `-0.0` and `0.0` differ and a NaN
/// equals itself: the model is compared exactly, as its serializations are.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Literal {
    /// An integer token: no fraction and no exponent.
    Integer(i64),
    /// A real token: a fraction or an exponent.
    Real(f64),
    /// `true` or `false`.
    Boolean(bool),
    /// A string; always text, never a reference (decision 11).
    Text(String),
    /// A member of a named type, `Type.Member`: an enumeration member (`SimpleController.PI`) or
    /// an Integer constant of a library constant package (`OperationModes.occupied`). The type
    /// name decides which, through `oce_api::enumerations()`, never the parameter's kind.
    Member(MemberRef),
}

impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Real(a), Self::Real(b)) => a.to_bits() == b.to_bits(),
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Member(a), Self::Member(b)) => a == b,
            _ => false,
        }
    }
}

impl Literal {
    /// The literal's kind, for messages: `Integer`, `Real`, `Boolean`, `String` or `member`.
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Integer(_) => "Integer",
            Self::Real(_) => "Real",
            Self::Boolean(_) => "Boolean",
            Self::Text(_) => "String",
            Self::Member(_) => "member",
        }
    }
}

/// A member of a named type as authored: `type_name` is the last segment of the enumeration or
/// constant package class (`SimpleController`, `OperationModes`) and `member` the member (`PI`,
/// `occupied`). `oce_api::enumerations()` resolves the full path.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberRef {
    /// The type's last segment; empty when the author wrote no `Type.` prefix.
    pub type_name: String,
    /// The member name.
    pub member: String,
}

impl MemberRef {
    /// Split a `Type.Member` spelling at its first dot. A spelling with no dot is a member with
    /// an empty type name, which the validator refuses (`unknown-enum-literal`).
    #[must_use]
    pub fn parse(spelling: &str) -> Self {
        match spelling.split_once('.') {
            Some((type_name, member)) => Self {
                type_name: type_name.to_owned(),
                member: member.to_owned(),
            },
            None => Self {
                type_name: String::new(),
                member: spelling.to_owned(),
            },
        }
    }

    /// The `Type.Member` spelling; the inverse of [`MemberRef::parse`].
    #[must_use]
    pub fn spelling(&self) -> String {
        if self.type_name.is_empty() {
            self.member.clone()
        } else {
            format!("{}.{}", self.type_name, self.member)
        }
    }
}

/// A directed connection, `source -> target`.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// The driving endpoint: a boundary input or a block output port.
    pub source: Endpoint,
    /// The driven endpoint: a boundary output or a block input port.
    pub target: Endpoint,
    /// Text-form annotations.
    pub annotations: Annotations,
}

/// One end of a connection. Port names are never authored as declarations; they come from the
/// catalog (#440), and the validator checks them.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// A boundary input or output, by name.
    Boundary(Identifier),
    /// A block port, `block.port`.
    Port {
        /// The block instance name.
        block: Identifier,
        /// The port's contract name (`u`, `u2`, `y1_1`).
        port: Identifier,
    },
}

impl Endpoint {
    /// Parse an endpoint spelling, `name` or `block.port`, splitting at the first dot. The parts
    /// are kept unchecked, so a malformed part (`a.b.c`, an empty name) is reported by the
    /// validator as `malformed-connection`.
    #[must_use]
    pub fn parse(spelling: &str) -> Self {
        match spelling.split_once('.') {
            Some((block, port)) => Self::Port {
                block: Identifier::unchecked(block),
                port: Identifier::unchecked(port),
            },
            None => Self::Boundary(Identifier::unchecked(spelling)),
        }
    }

    /// The `name` or `block.port` spelling.
    #[must_use]
    pub fn spelling(&self) -> String {
        match self {
            Self::Boundary(name) => name.as_str().to_owned(),
            Self::Port { block, port } => format!("{block}.{port}"),
        }
    }
}

impl Connection {
    /// A connection with no annotations.
    #[must_use]
    pub fn new(source: Endpoint, target: Endpoint) -> Self {
        Self {
            source,
            target,
            annotations: Annotations::default(),
        }
    }

    /// The canonical `source -> target` spelling.
    #[must_use]
    pub fn spelling(&self) -> String {
        format!("{} -> {}", self.source.spelling(), self.target.spelling())
    }
}
