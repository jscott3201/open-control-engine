//! The bytes `@context` expansion may add to one document (GitHub issue #709).
//!
//! A token read through a prefix is as long as the IRI the prefix binds. The document sets that
//! length and the number of tokens, and pays a few bytes for each: 1,024 `@type` tokens `p:T`
//! under a 64 KiB binding are a document of 78 KB that held 202 MB once every token was
//! expanded, and loaded. Nothing else ties the two factors together, so the pass that expands a
//! document counts what it adds.
//!
//! An [`ExpansionBudget`] is one document's count. Every expansion the pass copies is joined
//! through [`ExpansionBudget::expand`], which charges the bytes the expansion is longer than the
//! token it replaces, and joins it only while the document's total stays within
//! [`MAX_CONTEXT_EXPANSION_BYTES`]. The first expansion that does not fit ends the budget: it is
//! not built, one [`DiagCode::ResourceLimitExceeded`] error names it, and no later expansion is
//! built either, so a refused document costs what it added before the refusal and no more. The
//! pass reads nothing after that token, so the error is the last diagnostic it raises.
//!
//! What is counted is what expansion adds, never the document's own text: a token written as an
//! absolute IRI is not expanded and adds nothing, and one that expands to no more bytes than it
//! is written in (a long term bound to a short IRI) adds nothing. The identity and typing text
//! of the expanded working copy is therefore at most the document's own plus the budget.
//!
//! Reading a token is not charged, because it copies nothing ([`super::token_expansion`]).

use oce_diag::{DiagCode, Diagnostic};

use super::token_expansion::Iri;
use oce_diag::DiagnosticList;
use oce_diag::excerpt;
use oce_diag::visible;

/// The most bytes `@context` expansion adds to one document, over every token it expands: 8 MiB,
/// the size of the largest document the facade admits. An expansion adds its length less the
/// length of the token it is written as.
pub(super) const MAX_CONTEXT_EXPANSION_BYTES: usize = 8 * 1024 * 1024;

/// Where an expanded token is written, for the refusal that names it.
#[derive(Clone, Copy)]
pub(super) struct Site<'a> {
    /// The slot the token is written in, as a message names it: `@id`, `S231:hasInput`, `@type`,
    /// `S231:unit`, `property key`.
    pub(super) slot: &'a str,
    /// Whether the token is a structural reference in that slot (`S231:hasInput reference`), as
    /// the relative-reference refusal names one.
    pub(super) reference: bool,
    /// The `@id` of the node the token is written on, or how the node is named when it has none;
    /// `None` when the token is the node's own `@id`.
    pub(super) node: Option<&'a str>,
}

/// What `@context` expansion has added to one document, held to
/// [`MAX_CONTEXT_EXPANSION_BYTES`]; see the module documentation.
///
/// Invariant: `added` is never above the limit, and once `ended` is set nothing is joined.
#[derive(Default)]
pub(super) struct ExpansionBudget {
    /// The bytes the expansions joined so far added together.
    added: usize,
    /// Whether an expansion did not fit.
    ended: bool,
}

impl ExpansionBudget {
    /// A budget nothing is charged to yet.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Whether an expansion did not fit. The pass stops reading the document once it has.
    pub(super) fn has_ended(&self) -> bool {
        self.ended
    }

    /// `iri`, the expansion of `token`, joined into one string, when the bytes it adds fit the
    /// budget; `None` when they do not or the budget has ended.
    ///
    /// The first expansion that does not fit ends the budget and raises the refusal: one
    /// `resource-limit-exceeded` error about `site`, whose subject is the node (the token, for a
    /// node's own `@id`) and whose message quotes the token as an excerpt and states the sizes.
    /// Nothing is copied for an expansion that is not returned.
    pub(super) fn expand(
        &mut self,
        iri: Iri<'_>,
        token: &str,
        site: Site<'_>,
        diags: &mut DiagnosticList,
    ) -> Option<String> {
        if self.ended {
            return None;
        }
        let adds = iri.len().saturating_sub(token.len());
        if adds <= MAX_CONTEXT_EXPANSION_BYTES - self.added {
            self.added += adds;
            return Some(iri.joined());
        }
        self.ended = true;
        let (expanded, added) = (iri.len(), self.added);
        diags.push({
            let quoted = visible(&excerpt(token)).into_owned();
            let subject = match site.node {
                None => token.to_owned(),
                Some("") => quoted.clone(),
                Some(node) => node.to_owned(),
            };
            Diagnostic::error(
                DiagCode::ResourceLimitExceeded,
                format!(
                    "{}{} `{quoted}` expands to {expanded} bytes through the document's @context, \
                     {adds} more than it is written in; expansion had added {added} bytes, and \
                     may add at most {MAX_CONTEXT_EXPANSION_BYTES} to one document",
                    site.slot,
                    if site.reference { " reference" } else { "" }
                ),
            )
            .with_subject(subject)
        });
        None
    }
}

#[cfg(test)]
#[path = "expansion_budget_tests.rs"]
mod tests;
