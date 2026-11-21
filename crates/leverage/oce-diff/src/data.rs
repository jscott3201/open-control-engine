//! The diff as data (diff note, "Output"; decisions 7 and 13).
//!
//! A [`Diff`] is plain serde data: a schema tag, where the two sides came from, a [`Summary`],
//! closed-enum [`Note`]s and four sorted sections of changes ([`crate::ParameterChange`],
//! [`crate::InterfaceChange`], [`crate::GraphChange`], [`crate::MetadataChange`]). Nothing in it
//! is a float: a Real is carried as its spelling and its bits ([`Literal::Real`]). Every enum is
//! `#[non_exhaustive]` and serialized as a kebab-case string; every object is closed, so reading
//! a document with a key this crate does not define fails.

use serde::{Deserialize, Serialize};

use crate::changes::{GraphChange, InterfaceChange, MetadataChange, ParameterChange};

/// The schema tag every [`Diff`] carries. The format stays at revision 1 until 1.0.0: it changes
/// in place and its goldens are re-blessed.
pub const DIFF_SCHEMA: &str = "oce-diff/1";

/// The most unmatched blocks on one side the rename pass examines. With more on either side the
/// pass does not run and the diff carries [`Note::RenamePassSkipped`].
pub const MAX_RENAME_CANDIDATES: usize = 2_048;

/// The most gated pairs (an unmatched base block and an unmatched head block of the same class
/// with the same assignments) the rename pass examines. With more, the pass does not run and the
/// diff carries [`Note::RenamePassSkipped`].
pub const MAX_RENAME_PAIRS: usize = 65_536;

/// The most rounds the rename pass runs. Pairs accepted by then are reported; blocks a further
/// round would have matched stay removed and added.
pub const MAX_RENAME_ROUNDS: usize = 8;

/// What [`crate::diff`] is asked to do beyond comparing.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffOptions {
    /// Whether the rename pass runs. `false` (the CLI's `--no-renames`) leaves every unmatched
    /// block removed or added and always puts [`Note::RenamePassOff`] on the diff. Default
    /// `true`.
    pub renames: bool,
    /// The caller's statement that both sides load to equal canonical content ids (decision 8).
    /// Equal ids prove equal loaded graphs, and block names are inside the id, so no block can be
    /// left unmatched and the rename pass is skipped. They prove nothing else, so every section
    /// is still compared in full. If blocks are left unmatched on both sides all the same, the
    /// statement was wrong for these inputs: they are reported removed and added and the diff
    /// carries [`Note::RenamePassOff`]. Default `false`.
    pub canonical_ids_equal: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            renames: true,
            canonical_ids_equal: false,
        }
    }
}

impl DiffOptions {
    /// The defaults: the rename pass on, nothing known about content ids.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// A two-way structural diff of a base and a head, as data.
///
/// Each section is sorted by subject bytewise, then by kind in enum order, then by the rest of
/// the entry, so the same two inputs always give the same bytes.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diff {
    /// The schema tag, [`DIFF_SCHEMA`].
    pub schema: String,
    /// The level the two sides were compared at.
    pub source: Source,
    /// The content id of the catalog the literal defaults were read from
    /// (`oce_api::catalog_content_id`).
    pub catalog: String,
    /// The base side.
    pub base: SequenceRef,
    /// The head side.
    pub head: SequenceRef,
    /// Which sections differ, and the block counts.
    pub summary: Summary,
    /// What the reader should know about how the diff was made, in enum order, each at most once.
    pub notes: Vec<Note>,
    /// Changes to sequence-level parameters.
    pub parameters: Vec<ParameterChange>,
    /// Changes to boundary inputs and outputs.
    pub interface: Vec<InterfaceChange>,
    /// Changes to blocks, their assignments and the connections.
    pub graph: Vec<GraphChange>,
    /// Changes that never reach the emitted graph: names of the document, provenance, semantic
    /// fields, comments and spellings.
    pub metadata: Vec<MetadataChange>,
}

/// The level a diff was made at.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// Two authoring models ([`crate::diff`]).
    Authoring,
    /// Two loaded CXF documents, compared through their topology views.
    Cxf,
}

/// One side of a diff: the sequence name and what it derives from.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceRef {
    /// The sequence name.
    pub sequence: String,
    /// The `derived_from` provenance id as authored, or `None` when the sequence derives from
    /// nothing (`null` in JSON; the text form spells it `none`).
    pub derived_from: Option<String>,
}

/// Which sections of a diff are empty, and how the blocks matched.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    /// True only when every section is empty.
    pub equal: bool,
    /// True when the graph section is empty. It is decided by name and class alone and never by
    /// the rename pass: a renamed block has another name, so any rename leaves it false.
    pub graph_equal: bool,
    /// True when the parameters section is empty; `None` at a level that cannot tell a
    /// sequence-parameter change from a graph change.
    pub parameters_equal: Option<bool>,
    /// True when the interface section is empty.
    pub interface_equal: bool,
    /// True when the metadata section is empty; `None` at a level that carries no metadata.
    pub metadata_equal: Option<bool>,
    /// How the blocks of the two sides matched.
    pub blocks: BlockCounts,
}

/// How the blocks of the two sides matched. `by_name + renamed + base_only` is the base's block
/// count and `by_name + renamed + head_only` the head's.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockCounts {
    /// Blocks with the same name and the same class path on both sides.
    pub by_name: usize,
    /// Pairs the rename pass matched.
    pub renamed: usize,
    /// Base blocks matched to nothing (reported `removed`).
    pub base_only: usize,
    /// Head blocks matched to nothing (reported `added`).
    pub head_only: usize,
}

/// A fact about how a diff was made. Closed; serialized as its kebab-case code.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Note {
    /// Both sides have blocks and none kept its name and class.
    NamesDisjoint,
    /// Both sides had unmatched blocks and the rename pass did not run because a cap was passed
    /// ([`MAX_RENAME_CANDIDATES`] unmatched blocks on a side, or [`MAX_RENAME_PAIRS`] gated
    /// pairs): unmatched blocks are removed and added.
    RenamePassSkipped,
    /// The rename pass was turned off by the caller ([`DiffOptions`]).
    RenamePassOff,
}

/// How much of a renamed pair's neighbourhood the rename pass saw.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    /// Every port of the pair agrees at the end of the pass.
    Exact,
    /// The pair was accepted on the ports that agreed, and at least one other port ends at a
    /// block that was never matched, or at a different one.
    High,
}

/// One side of a diff.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    /// The first argument.
    Base,
    /// The second argument.
    Head,
}

/// The direction of a boundary connector.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    /// A boundary input (`in`).
    In,
    /// A boundary output (`out`).
    Out,
}

impl Direction {
    /// The text form's statement word: `in` or `out`.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Out => "out",
        }
    }
}

/// The list a `reordered` graph entry is about.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphList {
    /// The sequence's blocks; the entry's subject is `blocks`.
    Blocks,
    /// The sequence's connections; the entry's subject is `connections`.
    Connections,
    /// One block's assignments; the entry's subject is the block name.
    Assignments,
}

/// A value as authored, mirroring the model's tagged literal. One JSON object with one shape per
/// kind: `{"real", "bits"}`, `{"integer"}`, `{"boolean"}`, `{"text"}`, `{"ref"}`, `{"member"}`.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Literal {
    /// A Real: the shortest spelling that reads back to the same value (`0.6`, `-0.0`, `1e21`,
    /// `NaN`, `inf`) and its IEEE-754 bits as `0x` and 16 lowercase hex digits. The bits are the
    /// value; the spelling is for people. Never a JSON number, so no consumer's float parse can
    /// lose a signed zero or a NaN payload.
    Real {
        /// The spelling.
        real: String,
        /// The bits, `0x` then 16 lowercase hex digits.
        bits: String,
    },
    /// An Integer, the full `i64`.
    Integer {
        /// The value.
        integer: i64,
    },
    /// A Boolean.
    Boolean {
        /// The value.
        boolean: bool,
    },
    /// A string.
    Text {
        /// The text.
        text: String,
    },
    /// A reference to a sequence parameter, by name. Never resolved.
    Ref {
        /// The parameter name.
        #[serde(rename = "ref")]
        name: String,
    },
    /// A member of a named type, as spelled: `SimpleController.PI`.
    Member {
        /// The `Type.Member` spelling.
        member: String,
    },
}

impl Literal {
    /// A Real from its value: the spelling Rust's `{:?}` gives an `f64` (the authoring text
    /// form's canonical spelling) and the value's bits.
    #[must_use]
    pub fn real(value: f64) -> Self {
        Self::Real {
            real: format!("{value:?}"),
            bits: format!("0x{:016x}", value.to_bits()),
        }
    }
}

/// One parameter assignment of an added, removed or renamed block.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    /// The parameter name, or an array element name (`k_2`).
    pub name: String,
    /// The value.
    pub value: Literal,
}

/// One driven input port of an added or removed block.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    /// The block's input port.
    pub port: String,
    /// What drives it, spelled as on the entry's own side: a boundary input or `block.port`.
    pub source: String,
}

impl Diff {
    /// The diff as JSON text: keys sorted at every depth, two-space indentation, one final line
    /// feed. The same data always gives the same bytes, on every platform. Never panics.
    #[must_use]
    pub fn to_json(&self) -> String {
        crate::json::canonical(self)
    }
}
