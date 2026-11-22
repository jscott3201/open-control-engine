//! What the reads of one instance's parameters share, and the string bytes a document's
//! parameter tables may hold (issues #711, #731 and #736).
//!
//! Three costs of building a leaf's parameter table grew with something the document writes
//! once:
//!
//! - **The sibling names.** Every preserved array parameter checks each name it mints against the
//!   local names of the instance's parameters. The set of those names was built again for every
//!   array, so a leaf with `P` array parameters built `P` sets of `P` names: an empty array costs
//!   the document about 150 bytes and the reader `P` insertions. [`SiblingNames`] builds the set
//!   once per instance, on the first array that asks, from every parameter of the instance. The
//!   array's own local name is in it, which changes nothing: an element name is the base name and
//!   at least one `_` and index, and the array's local name is the base name alone or the base
//!   name and a `[`, so no element name equals it.
//! - **A value read more than once.** The one value of a broadcast array (`k[n]` with the value
//!   `[v]`) was grounded once per element, each grounding a value of its own: a string value was
//!   copied `n` times. A parameter node listed again on one instance was grounded again for each
//!   listing, and a failure copied the value text into its error each time. [`Groundings`]
//!   grounds such a value once per instance and hands every later read the same result: a string
//!   value is one shared allocation, and a failure is one error whose diagnostic is built again
//!   only while the diagnostic list keeps it.
//! - **String values per table entry.** A string value shared by many table entries is still
//!   copied once per entry by every consumer that writes each entry out (the content id's
//!   canonical bytes, a compiled blob, a parameter listing). So the string bytes the tables of one
//!   document hold are counted once per entry, shared or not, against
//!   [`MAX_DOCUMENT_STRING_PARAMETER_BYTES`] ([`StringParameterBudget`]).
//!
//! What grounding once changes, for a document that loads either way: nothing. A later read
//! could only ground differently if a name the value reads were bound between the two reads in
//! the sibling region the reads share. The names bound there between two elements of one
//! broadcast are the array's own element names, and a name that a sibling parameter has is never
//! minted (it is the collision refusal); so an element name the value reads was bound before by
//! another array of the same instance minting the same name, and the validator refuses the
//! instance for that duplicate. A node listed twice on one instance is refused in every case: by
//! `duplicate-parameter` when both listings ground, by the failed grounding otherwise. In those
//! refused documents the reader's diagnostics may differ from grounding each read again.
//!
//! What it does change: the document's evaluation budget (`MAX_DOCUMENT_EVAL_FUEL`) bounds the
//! work evaluation does, and the work is now done once. A broadcast or repeated value is charged
//! its evaluation once where it was charged once per read, so a document refused before for the
//! cost of evaluating the same value again (`q[40]` broadcasting `sum(1:1048576)`) now loads. The
//! same values written out once per element are as many evaluations, charged each, as before.
//!
//! Every set and map here is looked up, never iterated, so none of them orders anything.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;

use oce_diag::{DiagCode, Diagnostic};
use oce_expr::Scope;
use oce_model::Value;

use crate::dto::CxfValue;
use crate::ground::{GroundErr, ground_param_value};
use crate::resolve::local_name;

/// The most bytes the string values of every parameter table of one document may hold together,
/// each table entry counted, whether or not entries share one allocation: 8 MiB, the size of the
/// largest document the facade admits (issue #736).
///
/// A document writes a string value once, but one value can reach many entries: every element of
/// a broadcast array, every instance that lists one parameter node, every parameter that reads
/// another. The reader shares one allocation among them, but each consumer that writes the table
/// out copies the value once per entry: the content id's canonical bytes, a compiled blob, a
/// parameter listing, a receipt. A 60 KB string broadcast over 4,096 elements, a 67 KB document,
/// once loaded at a peak of 506 MB. So the budget is what such a consumer pays, and a document
/// whose every string value is written out once for each entry that holds it fits it. The string
/// parameters the engine's catalog has are messages (`Utilities.Assert`): no model of the tracked
/// corpus or of `open-control-library` holds more than 176 bytes of string values. Crossing it is
/// [`DiagCode::ResourceLimitExceeded`] on the parameter whose value does not fit, raised before
/// that value enters the table.
pub(crate) const MAX_DOCUMENT_STRING_PARAMETER_BYTES: usize = 8 * 1024 * 1024;

/// The running per-document budget of string parameter bytes,
/// [`MAX_DOCUMENT_STRING_PARAMETER_BYTES`], drawn on by every table entry whose value is a
/// string.
#[derive(Debug)]
pub(crate) struct StringParameterBudget {
    remaining: usize,
}

/// A string value that did not fit [`StringParameterBudget`]: its length and what the document's
/// tables already held.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StringBudgetRefusal {
    bytes: usize,
    used: usize,
}

impl StringParameterBudget {
    /// The full budget for one document.
    pub(crate) fn per_document() -> Self {
        Self {
            remaining: MAX_DOCUMENT_STRING_PARAMETER_BYTES,
        }
    }

    /// Reserve the bytes of `value` when it is a string (nothing for any other value), or report
    /// that it does not fit, reserving nothing.
    pub(crate) fn take(&mut self, value: &Value) -> Result<(), StringBudgetRefusal> {
        let Value::String(text) = value else {
            return Ok(());
        };
        if text.len() <= self.remaining {
            self.remaining -= text.len();
            return Ok(());
        }
        Err(StringBudgetRefusal {
            bytes: text.len(),
            used: MAX_DOCUMENT_STRING_PARAMETER_BYTES - self.remaining,
        })
    }
}

impl StringBudgetRefusal {
    /// The refusal as a diagnostic on the parameter node `parameter_iri`, naming the instance
    /// `owner` whose table the value would enter.
    pub(crate) fn diagnostic(self, parameter_iri: &str, owner: &str) -> Diagnostic {
        let Self { bytes, used } = self;
        Diagnostic::error(
            DiagCode::ResourceLimitExceeded,
            format!(
                "string parameter value of {bytes} byte(s) on `{owner}` does not fit: the \
                 parameter tables of the document already hold {used} of the \
                 {MAX_DOCUMENT_STRING_PARAMETER_BYTES} bytes of string values a document may \
                 hold, each table entry counted"
            ),
        )
        .with_subject(parameter_iri.to_owned())
    }
}

/// The local names of one instance's parameters, built once, on the first lookup, from every
/// parameter the instance lists (issue #731).
pub(crate) struct SiblingNames<'a> {
    iris: &'a [&'a str],
    names: OnceCell<BTreeSet<&'a str>>,
}

impl<'a> SiblingNames<'a> {
    /// The names of the parameters `iris`, not yet built.
    pub(crate) fn of(iris: &'a [&'a str]) -> Self {
        Self {
            iris,
            names: OnceCell::new(),
        }
    }

    /// Whether `name` is the local name of a parameter of the instance.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.names
            .get_or_init(|| {
                #[cfg(test)]
                NAMES_INSERTED.with(|inserted| {
                    inserted.set(inserted.get() + self.iris.len() as u64);
                });
                self.iris.iter().map(|&iri| local_name(iri)).collect()
            })
            .contains(name)
    }
}

/// The grounding of a value, kept by [`Groundings`] or made for one read.
pub(crate) enum Grounded<'m> {
    /// A result kept for every read of the value on this instance.
    Kept(&'m Result<Value, GroundErr>),
    /// A result made for this read alone.
    Fresh(Result<Value, GroundErr>),
}

impl Deref for Grounded<'_> {
    type Target = Result<Value, GroundErr>;

    fn deref(&self) -> &Self::Target {
        match self {
            Grounded::Kept(result) => result,
            Grounded::Fresh(result) => result,
        }
    }
}

/// The values one instance reads more than once, each grounded on its first read and kept for
/// the later ones (issues #736 and #711): the one value of a broadcast array, and every value of
/// a parameter node the instance lists more than once.
pub(crate) struct Groundings<'a> {
    /// The parameter IRIs the instance lists more than once, sorted, each once.
    repeated: Vec<&'a str>,
    /// The kept results, keyed by the address of the value in the document, which outlives the
    /// instance's reads.
    kept: BTreeMap<*const CxfValue, Result<Value, GroundErr>>,
}

impl<'a> Groundings<'a> {
    /// Nothing kept yet, for an instance listing the parameters `iris`.
    pub(crate) fn of(iris: &[&'a str]) -> Self {
        let mut repeated = Vec::new();
        if iris.len() > 1 {
            let mut sorted = iris.to_vec();
            sorted.sort_unstable();
            for pair in sorted.windows(2) {
                if pair[0] == pair[1] && repeated.last() != Some(&pair[0]) {
                    repeated.push(pair[0]);
                }
            }
        }
        Self {
            repeated,
            kept: BTreeMap::new(),
        }
    }

    /// Whether the instance lists the parameter `iri` more than once.
    pub(crate) fn is_repeated(&self, iri: &str) -> bool {
        self.repeated.binary_search(&iri).is_ok()
    }

    /// Ground `value` against `scope`, or hand back the result kept from its first read. Only a
    /// read with `keep` set keeps its result or reads a kept one.
    pub(crate) fn ground(
        &mut self,
        value: &CxfValue,
        scope: &dyn Scope,
        keep: bool,
    ) -> Grounded<'_> {
        if !keep {
            return Grounded::Fresh(ground_param_value(value, scope));
        }
        Grounded::Kept(
            self.kept
                .entry(std::ptr::from_ref(value))
                .or_insert_with(|| {
                    #[cfg(test)]
                    GROUNDED.with(|grounded| grounded.set(grounded.get() + 1));
                    ground_param_value(value, scope)
                }),
        )
    }
}

/// What one instance's parameter reads share: its sibling names and the values it reads more
/// than once.
pub(crate) struct InstanceReads<'a> {
    /// The local names of the instance's parameters, for the minted-name collision check.
    pub(crate) sibling_names: SiblingNames<'a>,
    /// The groundings kept for a value read more than once.
    pub(crate) groundings: Groundings<'a>,
}

impl<'a> InstanceReads<'a> {
    /// Nothing built or kept yet, for an instance listing the parameters `iris`.
    pub(crate) fn of(iris: &'a [&'a str]) -> Self {
        Self {
            sibling_names: SiblingNames::of(iris),
            groundings: Groundings::of(iris),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The names [`SiblingNames`] inserted into its sets on this thread, counted in test builds
    /// only.
    static NAMES_INSERTED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// The kept values [`Groundings::ground`] grounded on this thread, counted in test builds
    /// only.
    static GROUNDED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Run `run` and return how many names it inserted into sibling-name sets and how many kept
/// values it grounded on this thread, with its result: counts, the same on every run and every
/// host, never elapsed time.
#[cfg(test)]
pub(crate) fn reads_counted_by<R>(run: impl FnOnce() -> R) -> (u64, u64, R) {
    NAMES_INSERTED.with(|inserted| inserted.set(0));
    GROUNDED.with(|grounded| grounded.set(0));
    let result = run();
    (
        NAMES_INSERTED.with(std::cell::Cell::get),
        GROUNDED.with(std::cell::Cell::get),
        result,
    )
}

#[cfg(test)]
#[path = "parameter_reads_tests.rs"]
mod tests;
