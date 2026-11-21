//! The four closed change enums, one per section (diff note, "Categories and sections";
//! decisions 5, 6, 12 and 14).
//!
//! Each is `#[non_exhaustive]` and serialized as one object tagged by a stable kebab-case `kind`.
//! Every entry has a `subject`: what changed, spelled as the validator spells subjects (a block,
//! `block.param`, `block.port`, a boundary, a parameter, a list). A section is sorted by subject
//! bytewise, then by kind in the order the variants are declared here, then by the rest of the
//! entry ([`sort_section`]).

use serde::{Deserialize, Serialize};

use crate::data::{Assignment, Confidence, Direction, Driver, GraphList, Literal, Side};
use crate::work::compared;

/// A change to the blocks, their assignments or the connections.
///
/// Every connection change is reported once, at its target: an added or removed block carries
/// its own input drivers, and any other target whose driver moved is `rewired`.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum GraphChange {
    /// A block present only in head. Also half of a class change under one name, which is
    /// `removed` plus `added` and never a reparameterization.
    Added {
        /// The head block.
        subject: String,
        /// Its class path.
        class: String,
        /// Its assignments, in authored order.
        params: Vec<Assignment>,
        /// Its driven input ports, sorted by port, then source.
        inputs: Vec<Driver>,
    },
    /// A block present only in base.
    Removed {
        /// The base block.
        subject: String,
        /// Its class path.
        class: String,
        /// Its assignments, in authored order.
        params: Vec<Assignment>,
        /// Its driven input ports, sorted by port, then source.
        inputs: Vec<Driver>,
    },
    /// A base block and a head block the rename pass matched: same class, same assignments,
    /// agreeing neighbourhood.
    Renamed {
        /// The head block.
        subject: String,
        /// The base block's name.
        from: String,
        /// The class path both have.
        class: String,
        /// The head block's assignments, in authored order (the base block's are the same set).
        params: Vec<Assignment>,
        /// How much of the neighbourhood was seen.
        confidence: Confidence,
    },
    /// An assignment whose value differs, or that exists on one side only (that side `None`) and
    /// is not the catalog's literal default.
    Reparameterized {
        /// `block.param`.
        subject: String,
        /// The base value, or `None` when base does not assign the parameter.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<Literal>,
        /// The head value, or `None` when head does not assign the parameter.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<Literal>,
    },
    /// A target whose driver changed, compared after renames are mapped.
    Rewired {
        /// `block.port` in head names, or a boundary output.
        subject: String,
        /// The base driver as base spells it, or `None` when the target was not driven.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head driver as head spells it, or `None` when the target is not driven.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
    /// The same elements in another authored order. No semantic claim: the canonical content id
    /// ignores authored order, the emitted document does not.
    Reordered {
        /// `blocks`, `connections`, or the block whose assignments moved.
        subject: String,
        /// Which list, so a block named `blocks` cannot be mistaken for the block list.
        list: GraphList,
    },
    /// An assignment that exists on one side only and equals the catalog's literal default.
    ParameterDefaulted {
        /// `block.param`.
        subject: String,
        /// The assigned value.
        value: Literal,
        /// The side that assigns it.
        side: Side,
    },
}

/// A change to a sequence-level parameter. Each entry lists as `affects` the `block.param`
/// assignments that reference the parameter (head side; base side for a removed parameter),
/// sorted; those assignments are not repeated in the graph section.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ParameterChange {
    /// A parameter present only in head.
    Added {
        /// The parameter.
        subject: String,
        /// Its declared type (`Real`, `Integer`, `Boolean` or an enumeration type name).
        #[serde(rename = "type")]
        value_type: String,
        /// Its value.
        value: Literal,
        /// The head assignments that reference it.
        affects: Vec<String>,
    },
    /// A parameter present only in base.
    Removed {
        /// The parameter.
        subject: String,
        /// Its declared type.
        #[serde(rename = "type")]
        value_type: String,
        /// Its value.
        value: Literal,
        /// The base assignments that referenced it.
        affects: Vec<String>,
    },
    /// The value differs under one declared type.
    Changed {
        /// The parameter.
        subject: String,
        /// The declared type both sides have.
        #[serde(rename = "type")]
        value_type: String,
        /// The base value.
        old: Literal,
        /// The head value.
        new: Literal,
        /// The head assignments that reference it.
        affects: Vec<String>,
    },
    /// The declared type differs (the value may too).
    TypeChanged {
        /// The parameter.
        subject: String,
        /// The base type.
        old_type: String,
        /// The head type.
        new_type: String,
        /// The base value.
        old: Literal,
        /// The head value.
        new: Literal,
        /// The head assignments that reference it.
        affects: Vec<String>,
    },
    /// The same parameters in another authored order.
    Reordered {
        /// `params`.
        subject: String,
    },
}

/// A change to a boundary input or output. A connector is identified by its direction and name;
/// one that changes direction is `removed` plus `added`.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum InterfaceChange {
    /// A connector present only in head.
    Added {
        /// The connector.
        subject: String,
        /// Input or output.
        direction: Direction,
        /// Its type (`Real`, `Integer` or `Boolean`).
        #[serde(rename = "type")]
        value_type: String,
        /// Its `unit`, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        unit: Option<String>,
        /// Its `quantity`, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        quantity: Option<String>,
        /// Its `displayUnit`, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        display_unit: Option<String>,
        /// Its `min` bound, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        min: Option<Literal>,
        /// Its `max` bound, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        max: Option<Literal>,
    },
    /// A connector present only in base.
    Removed {
        /// The connector.
        subject: String,
        /// Input or output.
        direction: Direction,
        /// Its type.
        #[serde(rename = "type")]
        value_type: String,
        /// Its `unit`, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        unit: Option<String>,
        /// Its `quantity`, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        quantity: Option<String>,
        /// Its `displayUnit`, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        display_unit: Option<String>,
        /// Its `min` bound, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        min: Option<Literal>,
        /// Its `max` bound, if authored.
        #[serde(deserialize_with = "crate::read::nullable")]
        max: Option<Literal>,
    },
    /// The connector's type differs.
    TypeChanged {
        /// The connector.
        subject: String,
        /// Input or output.
        direction: Direction,
        /// The base type.
        old: String,
        /// The head type.
        new: String,
    },
    /// The `unit` differs (`None` where a side authors none).
    UnitChanged {
        /// The connector.
        subject: String,
        /// Input or output.
        direction: Direction,
        /// The base unit.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head unit.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
    /// The `quantity` differs.
    QuantityChanged {
        /// The connector.
        subject: String,
        /// Input or output.
        direction: Direction,
        /// The base quantity.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head quantity.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
    /// The `displayUnit` differs.
    DisplayUnitChanged {
        /// The connector.
        subject: String,
        /// Input or output.
        direction: Direction,
        /// The base display unit.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head display unit.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
    /// The `min` bound differs. On an input the bound is part of the acceptance domain and inside
    /// the canonical content id.
    MinChanged {
        /// The connector.
        subject: String,
        /// Input or output.
        direction: Direction,
        /// The base bound.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<Literal>,
        /// The head bound.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<Literal>,
    },
    /// The `max` bound differs.
    MaxChanged {
        /// The connector.
        subject: String,
        /// Input or output.
        direction: Direction,
        /// The base bound.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<Literal>,
        /// The head bound.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<Literal>,
    },
    /// The same connectors in another authored order.
    Reordered {
        /// `inputs` or `outputs`.
        subject: String,
    },
}

/// A change that never reaches the emitted graph. Every kind has the same shape: a subject and
/// the old and new text, `None` where a side has nothing.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum MetadataChange {
    /// The sequence name differs.
    SequenceRenamed {
        /// `sequence`.
        subject: String,
        /// The base name.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head name.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
    /// The `derived_from` provenance id differs.
    DerivedFromChanged {
        /// `derived_from`.
        subject: String,
        /// The base id, `None` for none.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head id, `None` for none.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
    /// A semantic field of a boundary other than `unit` differs.
    SemanticChanged {
        /// `boundary.field`: `name`, `label`, `brick`, `s223`, `qudt`, `haystack` (the tags
        /// joined by one space, in authored order) or `provisional` (`true` or `false`).
        subject: String,
        /// The base value.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head value.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
    /// The comment lines above a statement differ.
    CommentChanged {
        /// `sequence` (the header), `sequence.trailing`, or the statement: a parameter, a
        /// boundary, a block, `block.param`, or a connection as `source -> target`.
        subject: String,
        /// The base lines as the model holds them, joined by a line feed; `None` for no comment.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head lines.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
    /// A value written another way: an Integer literal against the equal Real where a Real is
    /// expected, or a changed unit suffix or written number. The value itself is unchanged, with
    /// one exception: when a value changes and its unit suffix changes with it, the change of
    /// value is an entry of its own section and this entry records the two spellings.
    LiteralRespelled {
        /// A parameter, `block.param`, or `boundary.min` / `boundary.max`.
        subject: String,
        /// The base spelling, as the text form writes it.
        #[serde(deserialize_with = "crate::read::nullable")]
        old: Option<String>,
        /// The head spelling.
        #[serde(deserialize_with = "crate::read::nullable")]
        new: Option<String>,
    },
}

impl GraphChange {
    /// What changed: a block, `block.param`, `block.port`, a boundary output or a list name.
    #[must_use]
    pub fn subject(&self) -> &str {
        match self {
            Self::Added { subject, .. }
            | Self::Removed { subject, .. }
            | Self::Renamed { subject, .. }
            | Self::Reparameterized { subject, .. }
            | Self::Rewired { subject, .. }
            | Self::Reordered { subject, .. }
            | Self::ParameterDefaulted { subject, .. } => subject,
        }
    }
}

impl ParameterChange {
    /// What changed: a parameter name, or `params`.
    #[must_use]
    pub fn subject(&self) -> &str {
        match self {
            Self::Added { subject, .. }
            | Self::Removed { subject, .. }
            | Self::Changed { subject, .. }
            | Self::TypeChanged { subject, .. }
            | Self::Reordered { subject } => subject,
        }
    }
}

impl InterfaceChange {
    /// What changed: a connector name, `inputs` or `outputs`.
    #[must_use]
    pub fn subject(&self) -> &str {
        match self {
            Self::Added { subject, .. }
            | Self::Removed { subject, .. }
            | Self::TypeChanged { subject, .. }
            | Self::UnitChanged { subject, .. }
            | Self::QuantityChanged { subject, .. }
            | Self::DisplayUnitChanged { subject, .. }
            | Self::MinChanged { subject, .. }
            | Self::MaxChanged { subject, .. }
            | Self::Reordered { subject } => subject,
        }
    }
}

impl MetadataChange {
    /// What changed; see each kind.
    #[must_use]
    pub fn subject(&self) -> &str {
        match self {
            Self::SequenceRenamed { subject, .. }
            | Self::DerivedFromChanged { subject, .. }
            | Self::SemanticChanged { subject, .. }
            | Self::CommentChanged { subject, .. }
            | Self::LiteralRespelled { subject, .. } => subject,
        }
    }
}

/// An entry of a section: it names its subject, says whether it is only a reordering, and has a
/// total order.
pub(crate) trait Entry: Ord {
    /// The entry's subject.
    fn entry_subject(&self) -> &str;

    /// Whether the entry says only that a list is in another authored order.
    fn is_reordering(&self) -> bool;
}

impl Entry for GraphChange {
    fn entry_subject(&self) -> &str {
        self.subject()
    }

    fn is_reordering(&self) -> bool {
        matches!(self, Self::Reordered { .. })
    }
}

impl Entry for ParameterChange {
    fn entry_subject(&self) -> &str {
        self.subject()
    }

    fn is_reordering(&self) -> bool {
        matches!(self, Self::Reordered { .. })
    }
}

impl Entry for InterfaceChange {
    fn entry_subject(&self) -> &str {
        self.subject()
    }

    fn is_reordering(&self) -> bool {
        matches!(self, Self::Reordered { .. })
    }
}

impl Entry for MetadataChange {
    fn entry_subject(&self) -> &str {
        self.subject()
    }

    fn is_reordering(&self) -> bool {
        false
    }
}

/// Whether a section differs in authored order at most: it holds no entry other than
/// `reordered`. This is what a section flag of the summary says.
pub(crate) fn same_but_for_order<E: Entry>(section: &[E]) -> bool {
    section.iter().all(Entry::is_reordering)
}

/// Sort a section: by subject bytewise, then by kind in enum order, then by the rest of the
/// entry (the derived order compares the variant first, then its fields as declared).
pub(crate) fn sort_section<E: Entry>(section: &mut [E]) {
    section.sort_by(|a, b| {
        compared();
        a.entry_subject()
            .as_bytes()
            .cmp(b.entry_subject().as_bytes())
            .then_with(|| a.cmp(b))
    });
}
