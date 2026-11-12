//! The importer (importer note `docs/design/importer.md`; gate item 1): two pure functions, one
//! per source form, each returning a [`Sequence`](crate::Sequence) in the source's own order plus
//! an [`ImportReport`] of what was read and not carried.
//!
//! - [`import_cxf`] reads one flat, single-root CXF document, the dialect the library corpus
//!   uses: leaf blocks, boundary inputs and outputs with `unit`, `quantity`, `displayUnit`, `min`
//!   and `max`, sequence parameters of a scalar or enumeration type, block parameters (array
//!   parameters, enumeration members and library constants included) and connections.
//! - [`import_graph_v0`] reads one document in the frozen prototype's compact block-graph JSON.
//!
//! The importer is not a validator and not an evaluator. It decodes structure, resolves the names
//! a mapping needs (array templates, member paths) through `oce-api`, and refuses with a typed
//! [`ImportError`] anything it cannot carry exactly, never a panic. Whether a class is authorable,
//! a name is reserved, an assignment is in range or the graph has a loop is the validator's
//! judgement, reported in the author's terms after import. It never evaluates an expression,
//! never fills a default, never renames, and never drops an attribute the engine reads. What it
//! drops (descriptive parameter attributes, labels, the root's `@type`, v0 `unit` and
//! `description`) is listed in the report.
//!
//! Out-of-subset constructs are refused with their own [`ImportErrorKind`] in a fixed precedence,
//! whatever the node order: nested composite, conditional component, expression value, port
//! attribute, name. The JSON is read strictly: a key written twice or an integer outside `i64` is
//! refused, not resolved.

mod comments;
mod cxf;
mod error;
mod nodes;
mod precedence;
mod report;
mod strict;
mod v0;
mod values;

pub use error::{IMPORT_ERROR_CODES, ImportError, ImportErrorKind, Limit, SourceForm, StrictJson};
pub use report::{
    Dropped, DroppedProperty, ImportOptions, ImportReport, Imported, Normalised, Truncated,
};
pub use v0::{MAX_V0_COMMENT_LINE_BYTES, V0_PROVENANCE_COMMENT};

/// Read one flat CXF document into the authoring model.
///
/// # Errors
///
/// An [`ImportError`] whose [`ImportErrorKind`] names the refusal: `too-large` above
/// [`oce_api::MAX_CXF_BYTES`] (checked before parsing), `not-strict-json` for bytes the strict
/// reader refuses, an out-of-subset kind for a construct outside the authoring subset (in the
/// fixed precedence), and a structural or value kind otherwise. Never panics.
pub fn import_cxf(bytes: &[u8], options: &ImportOptions) -> Result<Imported, ImportError> {
    cxf::import(bytes, options)
}

/// Read one document in the frozen prototype's v0 block-graph JSON into the authoring model
/// (importer note, "The v0 graph JSON importer").
///
/// Lossy by construction: v0 has no sequence parameters, references, bounds, quantity or display
/// unit, so none are produced. A boundary's `unit` and `description` become comment lines on its
/// annotations (and [`Dropped`] entries), never `SemanticPoint.unit`; the sequence gets the one
/// comment line [`V0_PROVENANCE_COMMENT`]. Block classes are `CDL.` plus the v0 class.
///
/// # Errors
///
/// An [`ImportError`]: `too-large` above [`crate::MAX_WIRE_BYTES`], `not-strict-json`,
/// `limit-exceeded` past the wire form's depth, list or string bounds, `unknown-key` /
/// `missing-key` for a key the v0 schema does not define or requires, `unknown-datatype` for a
/// type other than `bool`, `real` and `int`, `string-value` and `wrong-shape` for a parameter
/// value that is a string or null, an object or a list, and `malformed-connection`. Never panics.
pub fn import_graph_v0(bytes: &[u8], options: &ImportOptions) -> Result<Imported, ImportError> {
    v0::import(bytes, options)
}
