//! The per-document budget of the identities a model holds and writes under its blocks' instance
//! IRIs (issue #807; owner ruling of 2026-11-23).
//!
//! A scalar parameter is held under its block's instance IRI and exported as `<IRI>.<name>`
//! whatever its node is called; a padded output of a `hasInstance` instance is the identity
//! `<IRI>.<name>` the document never writes; and an input a boundary drives is exported as
//! `<IRI>.in<position>`. None of these costs the document the IRI's length: a parameter node
//! `urn:p.k` that 500 instances list costs each listing about 20 bytes, and each instance's IRI
//! can be 64 KiB written once under a `@context` prefix. So the bytes of these identities were
//! bounded by no budget: the export wrote each one twice or three times, about 296 MiB for a
//! model the facade loads and without limit for a graph `oce_cxf::import_cxf` read alone (whose
//! undeclared names the facade's load gate never saw).
//!
//! [`MAX_DOCUMENT_INSTANCE_PATH_BYTES`] bounds them together, each counted as the identity's
//! length (the instance IRI as expanded, `.`, the name or `in` and the position):
//!
//! - **every listing of a scalar parameter**, declared by the class or not, a parameter node many
//!   instances list counted once for each listing, before anything is built for the listing
//!   (`resolve::instance_params`);
//! - **every padded output identity** of an active derivation-shaped node, the outputs of one node
//!   together, before lowering builds any of them (`resolve::instance_paths`);
//! - **every input a boundary drives**, after Step 9 has found them (`resolve::instance_paths`).
//!   The reader builds no identity for these; the export spells them.
//!
//! The three are charged in that order, so in a document past the budget which identity is
//! refused follows the order of the document's nodes, instances and listings; whether a document
//! is refused does not: it is refused exactly when the identities take more than the budget
//! together, in any order (a refusal reserves nothing, so every identity that fits is admitted).
//!
//! What it does not charge, and what bounds each instead:
//!
//! - **An authored port, member or boundary IRI**: the document writes it (or a `@context` token
//!   the 8 MiB expansion budget counts), so the 8 MiB document admission and that budget bound
//!   its bytes, and the reader and the export hold or write each a fixed number of times.
//! - **An array element's path**: the element and path budgets of `arrays.rs` count every element
//!   path, minted or written, with the instance IRI, `.` and the element's name.
//! - **A minted output `<IRI>.out<k>`**: the export mints it only for an output connector with no
//!   IRI of its own, which the reader never makes (every connector it makes is named by a node,
//!   a listed member or a padded identity). Only a hand-built graph has one.

use oce_diag::{DiagCode, Diagnostic, excerpt};

/// Maximum number of bytes the identities one document holds under its blocks' instance IRIs
/// may take together: 8 MiB (issue #807; owner ruling of 2026-11-23). Each is counted as the
/// instance IRI as expanded, `.`, and the name, or `in` and the position, for every listing of a
/// scalar parameter, every padded output and every input a boundary drives (module docs).
///
/// No tracked or `open-control-library` document holds more than 15,240 bytes of them
/// (`cooling_only_controller.jsonld`: 167 parameters and 43 boundary-driven inputs under
/// 213 blocks), and none has an instance IRI longer than 128 bytes. Held to 8 MiB, the export
/// writes at most 16 MiB of parameter `@id`s, twice each, and 24 MiB of minted input `@id`s,
/// three times each, for any graph the reader returns. Crossing it is
/// [`DiagCode::ResourceLimitExceeded`] on the listing, the node or the boundary input; a refusal
/// reserves nothing.
pub(crate) const MAX_DOCUMENT_INSTANCE_PATH_BYTES: usize = 8 * 1024 * 1024;

/// What an identity under an instance IRI names, for the refusal's message.
#[derive(Clone, Copy, Debug)]
pub(crate) enum InstancePath {
    /// A listing of a scalar parameter: `<IRI>.<name>`.
    Parameter,
    /// The padded outputs of one derivation-shaped node: `<IRI>.<name>` each.
    PaddedOutputs,
    /// An input a boundary drives: `<IRI>.in<position>`.
    BoundaryInput,
}

/// The running per-document budget of [`MAX_DOCUMENT_INSTANCE_PATH_BYTES`].
#[derive(Debug)]
pub(crate) struct InstancePathBudget {
    remaining: usize,
}

/// An identity that did not fit [`InstancePathBudget`]: its bytes (`None` when they overflow a
/// `usize`) and what the document's identities already took.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InstancePathRefusal {
    kind: InstancePath,
    bytes: Option<usize>,
    used: usize,
}

impl InstancePathBudget {
    /// The full budget for one document (less what a test reserved through `reserving`).
    pub(crate) fn per_document() -> Self {
        #[cfg(test)]
        let reserved = RESERVED.with(std::cell::Cell::get);
        #[cfg(not(test))]
        let reserved = 0;
        Self {
            remaining: MAX_DOCUMENT_INSTANCE_PATH_BYTES - reserved,
        }
    }

    /// Reserve `bytes` for identities of `kind`, or report that they do not fit, reserving
    /// nothing. `bytes` is `None` when counting them overflowed.
    pub(crate) fn take(
        &mut self,
        kind: InstancePath,
        bytes: Option<usize>,
    ) -> Result<(), InstancePathRefusal> {
        match bytes {
            Some(bytes) if bytes <= self.remaining => {
                self.remaining -= bytes;
                Ok(())
            }
            _ => Err(InstancePathRefusal {
                kind,
                bytes,
                used: MAX_DOCUMENT_INSTANCE_PATH_BYTES.saturating_sub(self.remaining),
            }),
        }
    }

    /// Reserve one identity `<owner>.<suffix>` of `owner_bytes`, `.` and `suffix_bytes`.
    pub(crate) fn take_path(
        &mut self,
        kind: InstancePath,
        owner_bytes: usize,
        suffix_bytes: usize,
    ) -> Result<(), InstancePathRefusal> {
        self.take(kind, path_bytes(owner_bytes, suffix_bytes))
    }
}

#[cfg(test)]
thread_local! {
    /// The bytes the next documents read on this thread find already taken.
    static RESERVED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Run `read` with `reserved` bytes of every document's budget already taken, so a test pins the
/// budget's edges at its real size with a small document instead of one of 8 MiB of identities.
#[cfg(test)]
pub(crate) fn reserving<T>(reserved: usize, read: impl FnOnce() -> T) -> T {
    assert!(reserved <= MAX_DOCUMENT_INSTANCE_PATH_BYTES);
    let before = RESERVED.with(|cell| cell.replace(reserved));
    let result = read();
    RESERVED.with(|cell| cell.set(before));
    result
}

/// The bytes of `<owner>.<suffix>`, `None` past `usize::MAX`.
pub(crate) fn path_bytes(owner_bytes: usize, suffix_bytes: usize) -> Option<usize> {
    owner_bytes.checked_add(1)?.checked_add(suffix_bytes)
}

impl InstancePathRefusal {
    /// The refusal as a diagnostic on `subject` (the listing, the node or the boundary input),
    /// naming an excerpt of the instance IRI `owner` its identities are under.
    pub(crate) fn diagnostic(self, subject: &str, owner: &str) -> Diagnostic {
        let Self { kind, bytes, used } = self;
        let what = match kind {
            InstancePath::Parameter => "parameter path",
            InstancePath::PaddedOutputs => "padded output identities",
            InstancePath::BoundaryInput => "boundary-driven input identity",
        };
        let size = bytes.map_or_else(|| "more than usize::MAX".to_owned(), |b| b.to_string());
        Diagnostic::error(
            DiagCode::ResourceLimitExceeded,
            format!(
                "{what} of {size} byte(s) under `{}` does not fit: the identities the document \
                 holds under instance IRIs already take {used} of the \
                 {MAX_DOCUMENT_INSTANCE_PATH_BYTES} bytes a document may hold",
                excerpt(owner)
            ),
        )
        .with_subject(subject.to_owned())
    }
}

#[cfg(test)]
#[path = "instance_path_budget_tests.rs"]
mod tests;
