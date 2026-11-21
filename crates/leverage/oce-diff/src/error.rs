//! Why a diff of two loaded documents could not be made (diff note, decision 15; issue #666).
//!
//! The authoring diff is infallible: both of its inputs are parsed models. A topology is a view
//! of a loaded document, and two of its facts are only conventions the loader does not check:
//! that every block and boundary is named under the document's root, and that a block's ports
//! can be named from its class and ground parameters. A topology that breaks either is refused
//! with a typed [`DiffError`], never guessed at and never a panic.

use std::fmt;

use crate::data::Side;

/// Why [`crate::diff_topology`] refused. Closed and typed; [`DiffError::code`] is a stable
/// kebab-case string, and [`DiffError::CODES`] lists every one.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiffError {
    /// A block or a boundary of one side is not named under that side's root: its path is not
    /// the root id (`Topology::model_id`), a `.` and a non-empty name with no `#`, `/` or `:`.
    /// Identity at this level is the path with the root prefix stripped, and the canonical
    /// content id names an IRI by what follows its last `#`, `/` or `:`, so only such a name
    /// is the name the id gives the element. A document that names a block in another
    /// namespace or with a delimiter in its name, a document the engine exported (whose root is
    /// the exporter's fixed `@id` while its instance IRIs keep their own; issue #717 is to make
    /// it diffable) and a model with a synthetic root key are refused this way. A block with no
    /// instance IRI, which the facade spells `b<id>`, is not refused: it is matched by the
    /// rename pass only.
    OutsideRoot {
        /// The side the path is on.
        side: Side,
        /// The root id of that side.
        root: String,
        /// The block or boundary path.
        path: String,
        /// The delimiter (`#`, `/` or `:`) the name under the root holds, when the path is under
        /// the root and is refused for that alone; `None` when the path is not under the root.
        delimiter: Option<char>,
    },
    /// The ports of a block could not be named: the facade's `class_ports` refused its class and
    /// ground parameters, or named a different number of ports than the block has. A port is
    /// named by its index, so a block whose ports cannot be named cannot be wired. Unreachable
    /// for a loaded document: a load builds each block from the same class and parameters.
    UnresolvedPorts {
        /// The side the block is on.
        side: Side,
        /// The block's path as the topology spells it.
        block: String,
        /// The block's class path.
        class: String,
    },
}

impl DiffError {
    /// Every code [`DiffError::code`] returns, in the order the variants are declared.
    pub const CODES: &'static [&'static str] = &["outside-root", "unresolved-ports"];

    /// The stable kebab-case code of the refusal.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::OutsideRoot { .. } => "outside-root",
            Self::UnresolvedPorts { .. } => "unresolved-ports",
        }
    }
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Base => "base",
        Side::Head => "head",
    }
}

impl fmt::Display for DiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutsideRoot {
                side,
                root,
                path,
                delimiter: None,
            } => write!(
                f,
                "{}: `{path}` is not named under the root `{root}`",
                side_name(*side)
            ),
            Self::OutsideRoot {
                side,
                root,
                path,
                delimiter: Some(delimiter),
            } => write!(
                f,
                "{}: `{path}` is under the root `{root}`, but its name holds `{delimiter}`, which the content id reads as the end of a namespace",
                side_name(*side)
            ),
            Self::UnresolvedPorts { side, block, class } => write!(
                f,
                "{}: the ports of block `{block}` ({class}) cannot be named",
                side_name(*side)
            ),
        }
    }
}

impl std::error::Error for DiffError {}
