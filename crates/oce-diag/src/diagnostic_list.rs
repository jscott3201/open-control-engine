//! A diagnostic list held to a budget of text: the CXF reader's (GitHub issue #695) and each list
//! the validator builds (issue #726).
//!
//! A diagnostic copies document text: its subject is an `@id` as authored or expanded, and most
//! messages name that `@id` again. Nothing else ties the number of diagnostics to the length of
//! the strings they copy: one node with a long `@id` and many refused keys raises a diagnostic
//! per key, each a copy or two of the `@id`. Unbounded, 400 keys on a 1 MiB `@id`, a document of
//! 2 MiB, are over 1.2 GB of diagnostics. A refusal has to be a bounded answer, whatever the
//! document.
//!
//! A [`DiagnosticList`] therefore lists diagnostics, in the order they are raised, while their
//! text fits [`MAX_DIAGNOSTIC_TEXT_BYTES`]. The text of a diagnostic is its message and its
//! subject, and a subject is counted for every diagnostic that carries it, whether or not the
//! diagnostics share one allocation of it (owner ruling of 2026-11-21): the budget is what a
//! consumer that copies or prints each diagnostic pays, a receipt, a report, a log or a binding,
//! and what the list holds is never more. The first diagnostic that does not fit ends the list:
//! it and every later one are counted, not kept, and the list closes with one
//! [`DiagCode::ResourceLimitExceeded`] error that states the count and names the first
//! diagnostic left out, by its code and an excerpt of its subject. That error makes the outcome
//! a refusal even when everything unlisted was a warning, so a load never succeeds with
//! warnings left out.
//!
//! Two ways in:
//!
//! - [`DiagnosticList::push`] takes a diagnostic already built. Past the budget it is dropped,
//!   so the list is bounded, but the caller has paid to build it.
//! - [`DiagnosticList::raise`] takes a closure and does not call it once the list has ended. A
//!   loop that can raise many diagnostics about one long string (the keys of one node, the
//!   `@type` tokens of one node, a composite's repeated declarations) uses this one, so the work
//!   of a read is bounded as its memory is: past the budget a finding costs a counter and
//!   nothing else.
//!
//! What a list holds beside its text is one record per listed diagnostic (48 bytes on a 64-bit
//! target, in a vector that may hold twice as many as it lists) and the header of each
//! allocation. A listed message is shrunk to its length, so the capacity a `format!` leaves
//! behind, up to twice the length, is not held.
//!
//! The list reads as a slice of everything it holds, the closing error included, so
//! `is_empty()` and [`crate::has_errors`] see a list that has ended as non-empty and failed.
//!
//! Two stages build one: `oce-cxf`, the list of one read, and `oce-validate`, the list of each of
//! its passes (unification, validation, a block's parameters). The type lives here so both hold
//! the same budget and close with the same error (issue #726). A stage that sorts its
//! diagnostics sorts the listed ones and appends the closing error ([`DiagnosticList::into_parts`]),
//! so which diagnostics are listed follows the order they were raised in, and the closing error
//! stays last.

use std::fmt::{self, Write as _};
use std::ops::Deref;

use crate::quoting::{excerpt, visible};
use crate::{DiagCode, Diagnostic};

/// The most message and subject text, in bytes, the diagnostics of one list hold together: 8 MiB,
/// the size of the largest document the facade admits. The closing error is outside the count.
pub const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// The diagnostics of one read or one validation pass, in the order raised, within
/// [`MAX_DIAGNOSTIC_TEXT_BYTES`] of text; see the module documentation.
///
/// Invariant: `unlisted > 0` exactly when the last element of `held` is the closing error.
#[derive(Default)]
pub struct DiagnosticList {
    /// The listed diagnostics, then the closing error when the list has ended.
    held: Vec<Diagnostic>,
    /// The text of the listed diagnostics, in bytes: each one's message and subject. Never above
    /// [`MAX_DIAGNOSTIC_TEXT_BYTES`].
    listed_text_bytes: usize,
    /// How many diagnostics were raised after the list ended, the one that ended it included.
    unlisted: u64,
    /// What the closing error says of the diagnostic that ended the list: its code, and an
    /// excerpt of its subject. Empty while the list has not ended.
    first_unlisted: String,
}

impl DiagnosticList {
    /// An empty list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// List `diagnostic` when the list has not ended and its text fits the budget; otherwise end
    /// the list, if it has not ended, and count the diagnostic. A listed message gives back the
    /// capacity it does not use.
    pub fn push(&mut self, mut diagnostic: Diagnostic) {
        if self.unlisted > 0 {
            self.count_unlisted();
            return;
        }
        let subject_bytes = diagnostic.subject.as_deref().map_or(0, str::len);
        let remaining = MAX_DIAGNOSTIC_TEXT_BYTES - self.listed_text_bytes;
        // Compared without adding the two lengths, whose sum could wrap on a 32-bit target.
        if diagnostic.message.len() <= remaining
            && subject_bytes <= remaining - diagnostic.message.len()
        {
            self.listed_text_bytes += diagnostic.message.len() + subject_bytes;
            diagnostic.message.shrink_to_fit();
            self.held.push(diagnostic);
            return;
        }
        // The diagnostic that ends the list is named in the closing error, so a list that could
        // list nothing still says what was refused and where. The subject is quoted as every
        // message quotes authored text: at most 64 bytes, format and control characters escaped.
        self.first_unlisted = match diagnostic.subject.as_deref() {
            Some(subject) => format!("`{}` on `{}`", diagnostic.code, visible(&excerpt(subject))),
            None => format!("`{}` with no subject", diagnostic.code),
        };
        self.held.push(Diagnostic::error(
            DiagCode::ResourceLimitExceeded,
            String::new(),
        ));
        self.count_unlisted();
    }

    /// [`DiagnosticList::push`] the diagnostic `build` returns, without calling `build` once the
    /// list has ended: the finding is counted and nothing is formatted or copied for it.
    pub fn raise(&mut self, build: impl FnOnce() -> Diagnostic) {
        if self.unlisted == 0 {
            self.push(build());
        } else {
            self.count_unlisted();
        }
    }

    /// [`DiagnosticList::push`] each of `diagnostics`, in order.
    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        for diagnostic in diagnostics {
            self.push(diagnostic);
        }
    }

    /// Whether the list has ended: a diagnostic did not fit, so it and every later one are
    /// counted and none is built. A list never reopens. A caller that would record findings to
    /// raise again later records only their count once the list has ended.
    pub fn has_ended(&self) -> bool {
        self.unlisted > 0
    }

    /// How many diagnostics were raised, listed or not; the closing error is not one of them.
    pub fn raised(&self) -> u64 {
        let closing = usize::from(self.unlisted > 0);
        ((self.held.len() - closing) as u64).saturating_add(self.unlisted)
    }

    /// The listed diagnostics in the order raised, and the closing error when the list ended.
    /// A caller that sorts the list sorts the first and appends the second, so the count stays
    /// last.
    pub fn into_parts(mut self) -> (Vec<Diagnostic>, Option<Diagnostic>) {
        let closing = if self.unlisted > 0 {
            self.held.pop()
        } else {
            None
        };
        (self.held, closing)
    }

    /// Count one more diagnostic as unlisted on a list that has ended, and state the count in
    /// the closing error. Its message is rewritten in place, so counting allocates nothing unless
    /// the number gains a digit.
    fn count_unlisted(&mut self) {
        self.unlisted = self.unlisted.saturating_add(1);
        let (unlisted, first) = (self.unlisted, &self.first_unlisted);
        if let Some(closing) = self.held.last_mut() {
            closing.message.clear();
            // Writing to a `String` cannot fail.
            let _ = write!(
                closing.message,
                "{unlisted} more diagnostic(s) are not listed, the first of them {first}: a \
                 diagnostic list holds at most {MAX_DIAGNOSTIC_TEXT_BYTES} bytes of message and \
                 subject text"
            );
        }
    }
}

/// Formats as the slice the list reads as.
impl fmt::Debug for DiagnosticList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&self.held).finish()
    }
}

impl Deref for DiagnosticList {
    type Target = [Diagnostic];

    fn deref(&self) -> &[Diagnostic] {
        &self.held
    }
}

/// Everything the list holds, in the order raised, the closing error last.
impl IntoIterator for DiagnosticList {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.held.into_iter()
    }
}

/// A list of the diagnostics the iterator yields, each [`DiagnosticList::push`]ed in order.
impl FromIterator<Diagnostic> for DiagnosticList {
    fn from_iter<I: IntoIterator<Item = Diagnostic>>(diagnostics: I) -> Self {
        let mut list = Self::new();
        list.extend(diagnostics);
        list
    }
}

#[cfg(test)]
#[path = "diagnostic_list_tests.rs"]
mod tests;
