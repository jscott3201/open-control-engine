//! The generator of `data/generated.json` (design note decisions 3 to 5).
//!
//! It walks the catalog's own names ([`walk::catalog_walk`]), looks each connector and parameter
//! up in the vendored CXF-Core document by `@id` ([`core::Core`]), reads back the declared unit
//! and quantity through the pinned inverse tables ([`units`]), and renders the canonical
//! document. Everything it cannot read exactly is a typed [`GenerateError`] naming the class and
//! the subject; nothing is guessed and nothing is skipped silently.

pub mod core;
pub mod units;
pub mod walk;

use std::fmt;

use oce_vocab::{
    Connector, GeneratedClass, GeneratedVocabulary, Parameter, Side, Source, VOCAB_FORMAT,
};
use serde_json::{Map, Value};

use self::core::{Core, reference};
use self::walk::ClassWalk;

/// The Modelica Buildings commit the vendored document was generated from.
pub const BUILDINGS_COMMIT: &str = "a131864e4c4df22ebcd52bb8da439de0087ac365";
/// The modelica-json commit that generated it.
pub const MODELICA_JSON_COMMIT: &str = "85721b828a6ff8d9d3c1a48ff9a59808d2fa31fb";
/// The vendored document, relative to the repository root.
pub const DOCUMENT: &str = "third_party/modelica-buildings-cdl/cxf-core/CXF-Core.jsonld";

/// Classes the catalog has and the pinned Buildings does not: generated with `upstream: false`
/// and no text. Any other class the document lacks fails the generator, and so does a listed
/// class the document gains.
pub const ABSENT_CLASSES: &[&str] = &["CDL.Logical.TrueHoldWithReset"];

/// `(class, parameter)` the catalog lists and CXF-Core omits because upstream declares them
/// `protected`: generated with an empty description. Any other missing member fails the
/// generator, and so does a listed one the document gains.
pub const ABSENT_MEMBERS: &[(&str, &str)] = &[
    ("CDL.Reals.MatrixGain", "nout"),
    ("CDL.Reals.MatrixGain", "nin"),
];

/// Why the generator refused. `subject` is a connector or parameter name, or empty for the
/// class itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerateError {
    /// The document is larger than [`core::MAX_CORE_BYTES`].
    CoreTooLarge { bytes: usize, limit: usize },
    /// The document is not JSON.
    CoreNotJson(String),
    /// The document is not an object with an `@graph` array of nodes that each have an `@id`.
    CoreShape(&'static str),
    /// Two nodes share an `@id`.
    DuplicateNode { id: String },
    /// A catalog class, connector or parameter has no node, and is not pinned as absent.
    MissingNode { class: String, subject: String },
    /// A class or member pinned as absent has a node: the pin list is stale.
    UnexpectedNode { class: String, subject: String },
    /// A node's field is missing or has the wrong shape (`S231:description`, `@type`, a unit
    /// or quantity reference).
    MalformedNode {
        class: String,
        subject: String,
        field: &'static str,
    },
    /// A `qudt:hasUnit` identifier is neither in the pinned table nor an S231 fallback.
    UnknownUnit {
        class: String,
        subject: String,
        identifier: String,
    },
    /// A `qudt:hasQuantityKind` identifier is not in the pinned table.
    UnknownQuantity {
        class: String,
        subject: String,
        identifier: String,
    },
    /// A quantity kind has more than one inverse and the subject has no pinned resolution.
    AmbiguousQuantity {
        class: String,
        subject: String,
        identifier: String,
    },
    /// A width-driven or positional class has no probe table.
    MissingProbe { class: String },
    /// A class's ports could not be read into connectors.
    UnreadablePorts { class: String, detail: String },
}

impl fmt::Display for GenerateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// What the document says about one connector or parameter.
struct MemberText {
    description: String,
    unit: String,
    quantity: String,
}

impl MemberText {
    fn empty() -> Self {
        Self {
            description: String::new(),
            unit: String::new(),
            quantity: String::new(),
        }
    }
}

/// Which node types a member may carry. `discrete` connectors and enumeration parameters lose
/// their `@type` upstream, so an absent one is accepted; a present one must fit.
fn type_fits(node: &Map<String, Value>, side: Option<Side>) -> bool {
    let Some(node_type) = node.get("@type") else {
        return true;
    };
    let allowed: &[&str] = match side {
        Some(Side::Input) => &["S231:RealInput", "S231:IntegerInput", "S231:BooleanInput"],
        Some(Side::Output) => &[
            "S231:RealOutput",
            "S231:IntegerOutput",
            "S231:BooleanOutput",
        ],
        None => &["S231:Parameter"],
    };
    node_type.as_str().is_some_and(|t| allowed.contains(&t))
}

/// Read one member. `side` is the connector's side, or `None` for a parameter.
fn member_text(
    core: &Core,
    class: &str,
    upstream: bool,
    name: &str,
    side: Option<Side>,
) -> Result<MemberText, GenerateError> {
    let subject = || (class.to_owned(), name.to_owned());
    let node = upstream.then(|| core.member(class, name)).flatten();
    let pinned_absent = !upstream || ABSENT_MEMBERS.contains(&(class, name));
    let node = match (node, pinned_absent) {
        (Some(node), false) => node,
        (None, true) => return Ok(MemberText::empty()),
        (None, false) => {
            let (class, subject) = subject();
            return Err(GenerateError::MissingNode { class, subject });
        }
        (Some(_), true) => {
            let (class, subject) = subject();
            return Err(GenerateError::UnexpectedNode { class, subject });
        }
    };
    let malformed = |field| {
        let (class, subject) = subject();
        GenerateError::MalformedNode {
            class,
            subject,
            field,
        }
    };
    if !type_fits(node, side) {
        return Err(malformed("@type"));
    }
    let description = match node.get("S231:description") {
        Some(Value::String(text)) if !text.is_empty() => text.clone(),
        _ => return Err(malformed("S231:description")),
    };
    let unit = match node.get("qudt:hasUnit") {
        None => String::new(),
        Some(value) => {
            let identifier = reference(value).ok_or_else(|| malformed("qudt:hasUnit"))?;
            let Some(unit) = units::modelica_unit(identifier) else {
                let (class, subject) = subject();
                return Err(GenerateError::UnknownUnit {
                    class,
                    subject,
                    identifier: identifier.to_owned(),
                });
            };
            unit.to_owned()
        }
    };
    let quantity = match node.get("qudt:hasQuantityKind") {
        None => String::new(),
        Some(value) => {
            let identifier = reference(value).ok_or_else(|| malformed("qudt:hasQuantityKind"))?;
            let (class, subject) = subject();
            let identifier = identifier.to_owned();
            match units::modelica_quantities(&identifier).as_slice() {
                [] => {
                    return Err(GenerateError::UnknownQuantity {
                        class,
                        subject,
                        identifier,
                    });
                }
                [only] => (*only).to_owned(),
                candidates => {
                    let resolved = units::RESOLVED_QUANTITIES
                        .iter()
                        .find(|(c, s, _)| *c == class && *s == subject)
                        .map(|(_, _, quantity)| *quantity)
                        .filter(|quantity| candidates.contains(quantity));
                    let Some(resolved) = resolved else {
                        return Err(GenerateError::AmbiguousQuantity {
                            class,
                            subject,
                            identifier,
                        });
                    };
                    resolved.to_owned()
                }
            }
        }
    };
    Ok(MemberText {
        description,
        unit,
        quantity,
    })
}

/// Project `core` through `walk` into the generated vocabulary.
pub fn generate(core: &Core, walk: &[ClassWalk]) -> Result<GeneratedVocabulary, GenerateError> {
    let mut classes = Vec::with_capacity(walk.len());
    for class in walk {
        let path = class.class_path.as_str();
        let upstream = core.has_class(path);
        let whole_class = || (path.to_owned(), String::new());
        match (upstream, ABSENT_CLASSES.contains(&path)) {
            (true, false) | (false, true) => {}
            (false, false) => {
                let (class, subject) = whole_class();
                return Err(GenerateError::MissingNode { class, subject });
            }
            (true, true) => {
                let (class, subject) = whole_class();
                return Err(GenerateError::UnexpectedNode { class, subject });
            }
        }
        let mut connectors = Vec::with_capacity(class.connectors.len());
        for connector in &class.connectors {
            let text = member_text(core, path, upstream, &connector.name, Some(connector.side))?;
            connectors.push(Connector {
                description: text.description,
                dims: connector.dims,
                name: connector.name.clone(),
                quantity: text.quantity,
                side: connector.side,
                unit: text.unit,
            });
        }
        let mut parameters = Vec::with_capacity(class.parameters.len());
        for name in &class.parameters {
            let text = member_text(core, path, upstream, name, None)?;
            parameters.push(Parameter {
                description: text.description,
                name: name.clone(),
                quantity: text.quantity,
                unit: text.unit,
            });
        }
        classes.push(GeneratedClass {
            class_path: path.to_owned(),
            connectors,
            parameters,
            upstream,
        });
    }
    Ok(GeneratedVocabulary {
        classes,
        format: VOCAB_FORMAT,
        source: Source {
            buildings_commit: BUILDINGS_COMMIT.to_owned(),
            document: DOCUMENT.to_owned(),
            modelica_json_commit: MODELICA_JSON_COMMIT.to_owned(),
            sha256: core.sha256().to_owned(),
        },
    })
}

/// The canonical bytes of `data/generated.json` for the document `bytes` and the live catalog.
pub fn regenerate(bytes: &[u8]) -> Result<String, GenerateError> {
    let core = Core::parse(bytes)?;
    Ok(generate(&core, &walk::catalog_walk()?)?.to_json())
}
