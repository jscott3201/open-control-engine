//! The typed authoring model (authoring note, "The authoring model").
//!
//! One [`Sequence`] is one flat, single-root document: sequence parameters, boundary inputs and
//! outputs, leaf blocks and connections, every list an ordered vector kept in authored order.
//! Nothing here is sorted, evaluated or resolved: the model is what the author wrote, and
//! [`crate::validate()`] judges it against the catalog.

use crate::identifier::{Identifier, ProvenanceId};

/// One authored sequence.
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
}

impl Sequence {
    /// An empty sequence named `name`, extending nothing.
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
        }
    }
}

/// The type of a sequence parameter or a boundary connector.
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

/// A sequence-level parameter: a name, a type and a literal value.
#[derive(Clone, Debug, PartialEq)]
pub struct SequenceParameter {
    /// The parameter name.
    pub name: Identifier,
    /// The declared type.
    pub value_type: SignalType,
    /// The value. A `Real` parameter accepts an [`Literal::Integer`]; anything else must match
    /// the declared type (the validator reports `parameter-kind-mismatch` otherwise).
    pub value: Literal,
}

/// A boundary input or output declaration.
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
    /// The semantic slot (decision 2).
    pub semantic: SemanticPoint,
}

impl Boundary {
    /// A boundary with no quantity, display unit or semantic field.
    #[must_use]
    pub fn new(name: Identifier, value_type: SignalType) -> Self {
        Self {
            name,
            value_type,
            quantity: None,
            display_unit: None,
            semantic: SemanticPoint::default(),
        }
    }
}

/// The semantic slot of a boundary connector (decision 2): a closed struct of optional fields,
/// syntax-checked by the validator and never emitted as a semantic record before M8. `unit` is
/// emitted as the CXF `unit` attribute.
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
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The instance name.
    pub name: Identifier,
    /// The catalog class path, verbatim (`CDL.Reals.Hysteresis`).
    pub class_path: String,
    /// Parameter assignments, in authored order.
    pub assignments: Vec<Assignment>,
}

impl Block {
    /// A block with no assignments.
    #[must_use]
    pub fn new(name: Identifier, class_path: impl Into<String>) -> Self {
        Self {
            name,
            class_path: class_path.into(),
            assignments: Vec::new(),
        }
    }
}

/// One parameter assignment on a block. An array parameter is assigned one element at a time,
/// under the element name [`oce_api::param_element_name`] mints (`k_2`, `K_1_2`; decision 15).
#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    /// The catalog parameter name, or an array element name.
    pub name: Identifier,
    /// The value.
    pub value: ParamValue,
}

/// The value of a block parameter assignment (decision 3): a literal or a reference to a
/// sequence parameter, never an expression.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    /// A literal: a number, a Boolean, a string or an enumeration member.
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
    /// An enumeration member, `Type.Member`.
    EnumMember(EnumMember),
}

impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Real(a), Self::Real(b)) => a.to_bits() == b.to_bits(),
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::EnumMember(a), Self::EnumMember(b)) => a == b,
            _ => false,
        }
    }
}

impl Literal {
    /// The literal's kind, for messages: `Integer`, `Real`, `Boolean`, `String` or `enum member`.
    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Integer(_) => "Integer",
            Self::Real(_) => "Real",
            Self::Boolean(_) => "Boolean",
            Self::Text(_) => "String",
            Self::EnumMember(_) => "enum member",
        }
    }
}

/// An enumeration member as authored: `type_name` is the last segment of the enumeration class
/// (`SimpleController`) and `member` the member (`PI`). The catalog resolves the full path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumMember {
    /// The enumeration type's last segment; empty when the author wrote no `Type.` prefix.
    pub type_name: String,
    /// The member name.
    pub member: String,
}

impl EnumMember {
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

    /// The `Type.Member` spelling; the inverse of [`EnumMember::parse`].
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// The driving endpoint: a boundary input or a block output port.
    pub source: Endpoint,
    /// The driven endpoint: a boundary output or a block input port.
    pub target: Endpoint,
}

/// One end of a connection. Port names are never authored as declarations; they come from the
/// catalog (#440), and the validator checks them.
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
    /// The canonical `source -> target` spelling.
    #[must_use]
    pub fn spelling(&self) -> String {
        format!("{} -> {}", self.source.spelling(), self.target.spelling())
    }
}
