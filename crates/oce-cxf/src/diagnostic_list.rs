//! The diagnostic list one read builds, held to a budget of text (GitHub issue #695).
//!
//! A diagnostic copies document text: its subject is an `@id` as authored or expanded, and most
//! messages name that `@id` again. Nothing else ties the number of diagnostics to the length of
//! the strings they copy, so one node with a long `@id` and many refused keys built a list of
//! `keys × id bytes`: 400 keys on a 1 MiB `@id`, a document of 2 MiB, held over 1.2 GB of
//! diagnostics. A refusal has to be a bounded answer, whatever the document.
//!
//! A [`DiagnosticList`] therefore lists diagnostics, in the order they are raised, while the
//! text it holds fits [`MAX_DIAGNOSTIC_TEXT_BYTES`]. The text of a diagnostic is its message and
//! its subject. A subject several diagnostics share as one allocation (an `Arc` cloned, as the
//! resolver does for a connector's findings) is held once and counted once; a subject copied
//! for each diagnostic is counted for each. The first diagnostic that does not fit ends the
//! list: it and every later one are counted, not kept, and the list closes with one
//! [`DiagCode::ResourceLimitExceeded`] error that states the count. That error makes the
//! outcome a refusal even when everything unlisted was a warning, so a load never succeeds with
//! warnings left out.
//!
//! Two ways in:
//!
//! - [`DiagnosticList::push`] takes a diagnostic already built. Past the budget it is dropped,
//!   so the list is bounded, but the caller has paid to build it.
//! - [`DiagnosticList::raise`] takes a closure and does not call it once the list has ended. A
//!   loop that can raise many diagnostics about one long string (the keys of one node, the
//!   `@type` tokens of one node, the elements of one array) uses this one, so the work of a
//!   read is bounded as its memory is: past the budget a finding costs a counter and nothing
//!   else.
//!
//! The list reads as a slice of everything it holds, the closing error included, so
//! `is_empty()` and [`oce_diag::has_errors`] see a list that has ended as non-empty and failed.

use std::collections::HashSet;
use std::fmt::{self, Write as _};
use std::ops::Deref;
use std::sync::Arc;

use oce_diag::{DiagCode, Diagnostic};

/// The most message and subject text, in bytes, the diagnostics of one list hold together: 8 MiB,
/// the size of the largest document the facade admits. The closing error is outside the count.
pub(crate) const MAX_DIAGNOSTIC_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// The diagnostics of one read, in the order raised, within [`MAX_DIAGNOSTIC_TEXT_BYTES`] of
/// text; see the module documentation.
///
/// Invariant: `unlisted > 0` exactly when the last element of `held` is the closing error.
#[derive(Default)]
pub(crate) struct DiagnosticList {
    /// The listed diagnostics, then the closing error when the list has ended.
    held: Vec<Diagnostic>,
    /// The text the listed diagnostics hold, in bytes: every message, and every subject
    /// allocation once. Never above [`MAX_DIAGNOSTIC_TEXT_BYTES`].
    listed_text_bytes: usize,
    /// The subject allocations already counted in `listed_text_bytes`, by address. Every one is
    /// kept alive by a listed diagnostic, so an address names one allocation. Lookup only.
    counted_subjects: HashSet<*const u8>,
    /// How many diagnostics were raised after the list ended, the one that ended it included.
    unlisted: u64,
}

impl DiagnosticList {
    /// An empty list.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// List `diagnostic` when the list has not ended and its text fits the budget; otherwise end
    /// the list, if it has not ended, and count the diagnostic.
    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        if self.unlisted == 0 {
            // A subject allocation this list already holds costs nothing more.
            let uncounted = diagnostic
                .subject
                .as_ref()
                .map(|subject| (Arc::as_ptr(subject).cast::<u8>(), subject.len()))
                .filter(|(address, _)| !self.counted_subjects.contains(address));
            let subject_bytes = uncounted.map_or(0, |(_, bytes)| bytes);
            let remaining = MAX_DIAGNOSTIC_TEXT_BYTES - self.listed_text_bytes;
            // Compared without adding the two lengths, whose sum could wrap on a 32-bit target.
            if diagnostic.message.len() <= remaining
                && subject_bytes <= remaining - diagnostic.message.len()
            {
                self.listed_text_bytes += diagnostic.message.len() + subject_bytes;
                if let Some((address, _)) = uncounted {
                    self.counted_subjects.insert(address);
                }
                self.held.push(diagnostic);
                return;
            }
        }
        self.leave_unlisted();
    }

    /// [`DiagnosticList::push`] the diagnostic `build` returns, without calling `build` once the
    /// list has ended: the finding is counted and nothing is formatted or copied for it.
    pub(crate) fn raise(&mut self, build: impl FnOnce() -> Diagnostic) {
        if self.unlisted == 0 {
            self.push(build());
        } else {
            self.leave_unlisted();
        }
    }

    /// Count one diagnostic that was raised and is not kept, ending the list if it has not
    /// ended. For a caller that knows a finding was raised and no longer holds its text.
    pub(crate) fn leave_unlisted(&mut self) {
        self.count_unlisted(1);
    }

    /// [`DiagnosticList::push`] each of `diagnostics`, in order.
    pub(crate) fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        for diagnostic in diagnostics {
            self.push(diagnostic);
        }
    }

    /// Add everything `other` raised, in order: each diagnostic it lists is pushed, then each
    /// one it left unlisted is left unlisted here.
    pub(crate) fn append(&mut self, other: DiagnosticList) {
        let unlisted = other.unlisted;
        let (listed, _) = other.into_parts();
        self.extend(listed);
        if unlisted > 0 {
            self.count_unlisted(unlisted);
        }
    }

    /// How many diagnostics were raised, listed or not; the closing error is not one of them.
    pub(crate) fn raised(&self) -> u64 {
        let closing = usize::from(self.unlisted > 0);
        ((self.held.len() - closing) as u64).saturating_add(self.unlisted)
    }

    /// The listed diagnostics in the order raised, and the closing error when the list ended.
    /// A caller that sorts the list sorts the first and appends the second, so the count stays
    /// last.
    pub(crate) fn into_parts(mut self) -> (Vec<Diagnostic>, Option<Diagnostic>) {
        let closing = if self.unlisted > 0 {
            self.held.pop()
        } else {
            None
        };
        (self.held, closing)
    }

    /// Count `more` (at least one) further diagnostics as unlisted, ending the list with the
    /// closing error on the first. The closing error's message is rewritten in place, so a later
    /// count allocates nothing unless the number gains a digit.
    fn count_unlisted(&mut self, more: u64) {
        if self.unlisted == 0 {
            self.held.push(Diagnostic::error(
                DiagCode::ResourceLimitExceeded,
                String::new(),
            ));
        }
        self.unlisted = self.unlisted.saturating_add(more);
        let unlisted = self.unlisted;
        if let Some(closing) = self.held.last_mut() {
            closing.message.clear();
            // Writing to a `String` cannot fail.
            let _ = write!(
                closing.message,
                "{unlisted} more diagnostic(s) are not listed: a diagnostic list holds at most \
                 {MAX_DIAGNOSTIC_TEXT_BYTES} bytes of message and subject text"
            );
        }
    }
}

/// Formats as the slice the list reads as. The bookkeeping holds addresses, which differ from
/// one run to the next and so stay out of anything a test or a log may compare.
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
