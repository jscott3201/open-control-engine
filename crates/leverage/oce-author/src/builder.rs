//! A typed builder over the model.
//!
//! The builder refuses, with a typed [`BuildError`], every defect it can see without the catalog:
//! a malformed or duplicate name, a malformed provenance id or semantic field, an assignment to a
//! block not yet declared, a reference to a sequence parameter not yet declared, an array element
//! index the template does not take, and a malformed connection endpoint. What needs the catalog
//! (classes, parameters, kinds, rules, ports) is [`crate::validate()`]'s.

use std::fmt;

use crate::identifier::{
    Identifier, IdentifierError, ProvenanceId, ProvenanceIdError, check_identifier_syntax,
};
use crate::model::{
    Assignment, Block, Boundary, Connection, Endpoint, Literal, ParamValue, Sequence,
    SequenceParameter, SignalType,
};
use crate::validate::boundary_syntax_problems;

/// Why the builder refused a step.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuildError {
    /// A declared name is not a valid identifier.
    InvalidName {
        /// The name as given.
        name: String,
        /// The rule it breaks.
        error: IdentifierError,
    },
    /// A name is already declared as a parameter, input, output or block.
    DuplicateName {
        /// The name.
        name: String,
    },
    /// The provenance id is malformed.
    InvalidProvenanceId {
        /// The id as given.
        id: String,
        /// The rule it breaks.
        error: ProvenanceIdError,
    },
    /// A boundary's semantic slot, `quantity` or `display_unit` is malformed.
    InvalidSemanticPoint {
        /// The boundary name.
        name: String,
        /// The first problem.
        problem: String,
    },
    /// An assignment names a block not yet declared.
    UnknownBlock {
        /// The block name as given.
        block: String,
    },
    /// A reference names a sequence parameter not yet declared.
    UnknownReference {
        /// The referenced name as given.
        name: String,
    },
    /// An assigned parameter name is not an identifier.
    InvalidParameterName {
        /// The name as given.
        name: String,
        /// The rule it breaks.
        error: IdentifierError,
    },
    /// An array element's indices do not fit the template's rank, or one is zero.
    InvalidElementIndex {
        /// The template as given.
        template: String,
        /// The indices as given.
        indices: Vec<usize>,
    },
    /// A connection endpoint is not `name` or `block.port` with identifier parts.
    MalformedEndpoint {
        /// The endpoint as given.
        spelling: String,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName { name, error } => write!(f, "name `{name}`: {error}"),
            Self::DuplicateName { name } => write!(f, "name `{name}` is already declared"),
            Self::InvalidProvenanceId { id, error } => write!(f, "provenance id `{id}`: {error}"),
            Self::InvalidSemanticPoint { name, problem } => {
                write!(f, "boundary `{name}`: {problem}")
            }
            Self::UnknownBlock { block } => write!(f, "block `{block}` is not declared"),
            Self::UnknownReference { name } => {
                write!(f, "sequence parameter `{name}` is not declared")
            }
            Self::InvalidParameterName { name, error } => {
                write!(f, "parameter name `{name}`: {error}")
            }
            Self::InvalidElementIndex { template, indices } => {
                write!(
                    f,
                    "indices {indices:?} do not fit the template `{template}`"
                )
            }
            Self::MalformedEndpoint { spelling } => {
                write!(f, "endpoint `{spelling}` is not `name` or `block.port`")
            }
        }
    }
}

impl std::error::Error for BuildError {}

/// Builds a [`Sequence`] one checked step at a time, in authored order.
///
/// ```
/// use oce_author::{Boundary, Identifier, Literal, SequenceBuilder, SignalType, validate};
///
/// let sequence = SequenceBuilder::new("econ_enable")?
///     .parameter("dTHys", SignalType::Real, Literal::Integer(1))?
///     .input(Boundary::new(Identifier::new("TOut")?, SignalType::Real))?
///     .input(Boundary::new(Identifier::new("TOutCut")?, SignalType::Real))?
///     .output(Boundary::new(Identifier::new("ecoEna")?, SignalType::Boolean))?
///     .block("sub", "CDL.Reals.Subtract")?
///     .block("hys", "CDL.Reals.Hysteresis")?
///     .assign("hys", "uLow", Literal::Real(0.0))?
///     .assign_ref("hys", "uHigh", "dTHys")?
///     .connect("TOutCut", "sub.u1")?
///     .connect("TOut", "sub.u2")?
///     .connect("sub.y", "hys.u")?
///     .connect("hys.y", "ecoEna")?
///     .build();
/// assert!(validate(&sequence).is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug)]
pub struct SequenceBuilder {
    sequence: Sequence,
}

fn declared(name: &str) -> Result<Identifier, BuildError> {
    Identifier::new(name).map_err(|error| BuildError::InvalidName {
        name: name.to_owned(),
        error,
    })
}

impl SequenceBuilder {
    /// Start a sequence named `name`.
    ///
    /// # Errors
    ///
    /// [`BuildError::InvalidName`].
    pub fn new(name: &str) -> Result<Self, BuildError> {
        Ok(Self {
            sequence: Sequence::new(declared(name)?),
        })
    }

    fn is_declared(&self, name: &str) -> bool {
        let s = &self.sequence;
        s.params.iter().any(|p| p.name.as_str() == name)
            || s.inputs.iter().any(|b| b.name.as_str() == name)
            || s.outputs.iter().any(|b| b.name.as_str() == name)
            || s.blocks.iter().any(|b| b.name.as_str() == name)
    }

    fn fresh(&self, name: &str) -> Result<Identifier, BuildError> {
        let id = declared(name)?;
        if self.is_declared(name) {
            return Err(BuildError::DuplicateName {
                name: name.to_owned(),
            });
        }
        Ok(id)
    }

    /// Set the provenance id this sequence derives from.
    ///
    /// # Errors
    ///
    /// [`BuildError::InvalidProvenanceId`].
    pub fn extends(mut self, id: &str) -> Result<Self, BuildError> {
        let id = ProvenanceId::new(id).map_err(|error| BuildError::InvalidProvenanceId {
            id: id.to_owned(),
            error,
        })?;
        self.sequence.extends = Some(id);
        Ok(self)
    }

    /// Declare a sequence parameter. Whether the literal fits the type is the validator's
    /// (`parameter-kind-mismatch`), as is finiteness.
    ///
    /// # Errors
    ///
    /// [`BuildError::InvalidName`] or [`BuildError::DuplicateName`].
    pub fn parameter(
        mut self,
        name: &str,
        value_type: SignalType,
        value: Literal,
    ) -> Result<Self, BuildError> {
        let name = self.fresh(name)?;
        self.sequence
            .params
            .push(SequenceParameter::new(name, value_type, value));
        Ok(self)
    }

    fn boundary(&self, boundary: &Boundary) -> Result<(), BuildError> {
        self.fresh(boundary.name.as_str())?;
        if let Some(problem) = boundary_syntax_problems(boundary).into_iter().next() {
            return Err(BuildError::InvalidSemanticPoint {
                name: boundary.name.as_str().to_owned(),
                problem,
            });
        }
        Ok(())
    }

    /// Declare a boundary input.
    ///
    /// # Errors
    ///
    /// [`BuildError::InvalidName`], [`BuildError::DuplicateName`] or
    /// [`BuildError::InvalidSemanticPoint`].
    pub fn input(mut self, boundary: Boundary) -> Result<Self, BuildError> {
        self.boundary(&boundary)?;
        self.sequence.inputs.push(boundary);
        Ok(self)
    }

    /// Declare a boundary output.
    ///
    /// # Errors
    ///
    /// As [`SequenceBuilder::input`].
    pub fn output(mut self, boundary: Boundary) -> Result<Self, BuildError> {
        self.boundary(&boundary)?;
        self.sequence.outputs.push(boundary);
        Ok(self)
    }

    /// Declare a block of catalog class `class_path`. The class is checked by the validator.
    ///
    /// # Errors
    ///
    /// [`BuildError::InvalidName`] or [`BuildError::DuplicateName`].
    pub fn block(mut self, name: &str, class_path: &str) -> Result<Self, BuildError> {
        let name = self.fresh(name)?;
        self.sequence.blocks.push(Block::new(name, class_path));
        Ok(self)
    }

    fn push_assignment(
        mut self,
        block: &str,
        parameter: &str,
        value: ParamValue,
    ) -> Result<Self, BuildError> {
        check_identifier_syntax(parameter).map_err(|error| BuildError::InvalidParameterName {
            name: parameter.to_owned(),
            error,
        })?;
        let Some(target) = self
            .sequence
            .blocks
            .iter_mut()
            .find(|b| b.name.as_str() == block)
        else {
            return Err(BuildError::UnknownBlock {
                block: block.to_owned(),
            });
        };
        target
            .assignments
            .push(Assignment::new(Identifier::unchecked(parameter), value));
        Ok(self)
    }

    /// Assign a literal (or any [`ParamValue`]) to a parameter of a declared block.
    ///
    /// # Errors
    ///
    /// [`BuildError::UnknownBlock`] or [`BuildError::InvalidParameterName`].
    pub fn assign(
        self,
        block: &str,
        parameter: &str,
        value: impl Into<ParamValue>,
    ) -> Result<Self, BuildError> {
        self.push_assignment(block, parameter, value.into())
    }

    /// Assign a reference to a declared sequence parameter.
    ///
    /// # Errors
    ///
    /// [`BuildError::UnknownReference`], [`BuildError::UnknownBlock`] or
    /// [`BuildError::InvalidParameterName`].
    pub fn assign_ref(
        self,
        block: &str,
        parameter: &str,
        target: &str,
    ) -> Result<Self, BuildError> {
        if !self
            .sequence
            .params
            .iter()
            .any(|p| p.name.as_str() == target)
        {
            return Err(BuildError::UnknownReference {
                name: target.to_owned(),
            });
        }
        self.push_assignment(
            block,
            parameter,
            ParamValue::Ref(Identifier::unchecked(target)),
        )
    }

    /// Assign one element of an array parameter, named from the catalog `template` (`k_<i>`,
    /// `K_<row>_<col>`) and 1-based `indices` by [`oce_api::param_element_name`] (decision 15).
    ///
    /// # Errors
    ///
    /// [`BuildError::InvalidElementIndex`], or as [`SequenceBuilder::assign`].
    pub fn assign_element(
        self,
        block: &str,
        template: &str,
        indices: &[usize],
        value: impl Into<ParamValue>,
    ) -> Result<Self, BuildError> {
        let Some(name) = oce_api::param_element_name(template, indices) else {
            return Err(BuildError::InvalidElementIndex {
                template: template.to_owned(),
                indices: indices.to_vec(),
            });
        };
        self.push_assignment(block, &name, value.into())
    }

    /// Connect `source` to `target`, each `name` or `block.port`. What the endpoints name is
    /// the validator's.
    ///
    /// # Errors
    ///
    /// [`BuildError::MalformedEndpoint`].
    pub fn connect(mut self, source: &str, target: &str) -> Result<Self, BuildError> {
        let endpoint = |spelling: &str| -> Result<Endpoint, BuildError> {
            let parsed = Endpoint::parse(spelling);
            let ok = match &parsed {
                Endpoint::Boundary(name) => check_identifier_syntax(name.as_str()).is_ok(),
                Endpoint::Port { block, port } => {
                    check_identifier_syntax(block.as_str()).is_ok()
                        && check_identifier_syntax(port.as_str()).is_ok()
                }
            };
            ok.then_some(parsed)
                .ok_or_else(|| BuildError::MalformedEndpoint {
                    spelling: spelling.to_owned(),
                })
        };
        let connection = Connection::new(endpoint(source)?, endpoint(target)?);
        self.sequence.connections.push(connection);
        Ok(self)
    }

    /// The sequence, in authored order.
    #[must_use]
    pub fn build(self) -> Sequence {
        self.sequence
    }
}
