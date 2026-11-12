//! What an import read and did not carry (importer note, "What the importer is, and is not";
//! decisions 9, 15, 17 and 21).
//!
//! An [`ImportReport`] names the source form and lists, in reading order, every property that was
//! read and deliberately dropped ([`Dropped`], a closed [`DroppedProperty`]), the one
//! normalisation the CXF importer makes ([`Normalised`], decision 21) and every v0 comment line
//! that had to be truncated to stay within the text form's line bound ([`Truncated`]). Nothing the
//! engine reads is ever dropped; anything else the importer cannot carry exactly is refused.

use crate::identifier::ProvenanceId;
use crate::model::{Sequence, SignalType};

use super::error::SourceForm;

/// How to import.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportOptions {
    /// The provenance id the imported sequence derives from, set on
    /// [`Sequence::derived_from`]; none by default. The importer never derives one from the source
    /// (decision 15).
    pub derived_from: Option<ProvenanceId>,
}

impl ImportOptions {
    /// The default options: no `derived_from`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// These options with `derived_from` set.
    #[must_use]
    pub fn with_derived_from(mut self, derived_from: ProvenanceId) -> Self {
        self.derived_from = Some(derived_from);
        self
    }
}

/// A successful import: the sequence in the source's own order, and the report.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    /// The imported sequence.
    pub sequence: Sequence,
    /// What was read and not carried.
    pub report: ImportReport,
}

/// What an import read and did not carry, in reading order.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReport {
    /// The source form read.
    pub source: SourceForm,
    /// Every property read and dropped.
    pub dropped: Vec<Dropped>,
    /// Every normalisation made.
    pub normalised: Vec<Normalised>,
    /// Every v0 comment line truncated.
    pub truncated: Vec<Truncated>,
}

impl ImportReport {
    /// An empty report for `source`.
    #[must_use]
    pub fn new(source: SourceForm) -> Self {
        Self {
            source,
            dropped: Vec::new(),
            normalised: Vec::new(),
            truncated: Vec::new(),
        }
    }

    /// Whether nothing was dropped, normalised or truncated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dropped.is_empty() && self.normalised.is_empty() && self.truncated.is_empty()
    }
}

/// The closed set of properties an import reads and drops.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DroppedProperty {
    /// The CXF root's `@type` when it is not `S231:Block` (the Modelica class the sequence was
    /// translated from; decision 15).
    RootType,
    /// A CXF `S231:label` that differs from the node's local name.
    Label,
    /// A parameter's descriptive `S231:unit` (CXF; the engine uses none on a parameter node, and
    /// the importer checks only its shape, issue #559), or a v0 boundary `unit` (written as a
    /// comment line instead; decision 17).
    Unit,
    /// A CXF parameter's `S231:quantity`.
    Quantity,
    /// A CXF parameter's `S231:displayUnit`.
    DisplayUnit,
    /// A CXF parameter's `S231:min` (decision 9: the engine uses no parameter bound; its shape is
    /// checked, issue #559).
    Min,
    /// A CXF parameter's `S231:max` (as [`DroppedProperty::Min`]).
    Max,
    /// A v0 boundary `description` (written as comment lines instead; decision 17).
    Description,
}

impl DroppedProperty {
    /// The property's stable name: `root-type`, `label`, `unit`, `quantity`, `display-unit`,
    /// `min`, `max` or `description`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RootType => "root-type",
            Self::Label => "label",
            Self::Unit => "unit",
            Self::Quantity => "quantity",
            Self::DisplayUnit => "display-unit",
            Self::Min => "min",
            Self::Max => "max",
            Self::Description => "description",
        }
    }
}

/// One property read and dropped.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dropped {
    /// The node `@id` (CXF) or the boundary name (v0).
    pub subject: String,
    /// The property.
    pub property: DroppedProperty,
    /// The value as written: a string's text, any other value as compact JSON.
    pub value: String,
}

/// One normalisation the CXF importer made: a boundary or block port typed by a library constant
/// package (`S231:IntegerInput` with `isOfDataType` `…G36.Types.OperationModes`) read as an
/// Integer, which is how the engine reads it (decision 21). The emitter then writes
/// `S231:Integer`, which loads to the same type and the same content id.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Normalised {
    /// The connector node's `@id`.
    pub subject: String,
    /// The `isOfDataType` IRI as written.
    pub data_type: String,
    /// The type it was read as.
    pub to: SignalType,
}

/// One v0 comment line truncated at a character boundary (importer note, "Comments from v0
/// text").
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Truncated {
    /// The boundary name.
    pub subject: String,
    /// The property the line came from ([`DroppedProperty::Description`] or
    /// [`DroppedProperty::Unit`]).
    pub property: DroppedProperty,
    /// The line's index among the boundary's comment lines (0-based).
    pub line: usize,
    /// The line's length in bytes before truncation.
    pub bytes: usize,
    /// The line's length in bytes after truncation.
    pub kept: usize,
}
