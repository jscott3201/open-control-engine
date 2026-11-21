#![forbid(unsafe_code)]
//! Structural diff for Open Control Engine sequences (design note `docs/design/diff.md`,
//! issue #665).
//!
//! Status: the first of the four parts of the diff (M3): the authoring-level diff, the bounded
//! rename pass and the two renderers. The CXF adapter, the blob comparison and the `oce diff`
//! verb build on it.
//!
//! [`diff`] compares two authoring models (`oce_author::Sequence`, from the text or the wire
//! form) and returns a [`Diff`]: plain serde data, tagged [`DIFF_SCHEMA`], with a [`Summary`],
//! closed-enum [`Note`]s and four sorted sections.
//!
//! - **Identity is the authored block name.** The same name and class path is the same block;
//!   the same name with another class path is `removed` plus `added`; the blocks left over go
//!   through the rename pass.
//! - **The rename pass** is deterministic and bounded: a candidate pair has the same class and
//!   the same assignments bit for bit, a pair is accepted only when its neighbourhood agrees and
//!   each side has exactly one candidate, for at most [`MAX_RENAME_ROUNDS`] rounds over at most
//!   [`MAX_RENAME_CANDIDATES`] unmatched blocks a side and [`MAX_RENAME_PAIRS`] gated pairs. It
//!   only changes how a difference is presented: [`Summary::graph_equal`] never depends on it.
//! - **Four sections.** [`GraphChange`] (blocks, assignments and connections, each connection
//!   change once, at its target), [`ParameterChange`] (sequence parameters with what they
//!   affect), [`InterfaceChange`] (boundary inputs and outputs) and [`MetadataChange`] (names,
//!   provenance, semantic fields, comments and spellings).
//! - **Exact values.** A Real is its spelling and its bits ([`Literal::Real`]), never a JSON
//!   number, so `-0.0` against `0.0` and one NaN payload against another are differences.
//! - **Renderers.** [`render_text`] and [`render_markdown`] are pure functions of the data.
//!
//! The diff never validates and never evaluates: it takes parsed models as they are, reads the
//! catalog only for a parameter's literal default and type, and loads no engine. It is two-way.
//! It does not match blocks by what they compute: a block with commuted operands or a retuned
//! parameter under a new name is `removed` plus `added`, for a reviewer to judge.
//!
//! The crate sits above the facade: among `oce-*` packages it depends on `oce-api` and
//! `oce-author` only.

mod authoring;
mod catalog;
mod changes;
mod data;
mod json;
mod pairing;
mod render;
mod wiring;
mod work;

pub use authoring::diff;
pub use changes::{GraphChange, InterfaceChange, MetadataChange, ParameterChange};
pub use data::{
    Assignment, BlockCounts, Confidence, DIFF_SCHEMA, Diff, DiffOptions, Direction, Driver,
    GraphList, Literal, MAX_RENAME_CANDIDATES, MAX_RENAME_PAIRS, MAX_RENAME_ROUNDS, Note,
    SequenceRef, Side, Source, Summary,
};
pub use render::{render_markdown, render_text};
