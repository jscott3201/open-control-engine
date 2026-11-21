//! One token read against the `@context` prefix table, with nothing copied (GitHub issue #709).
//!
//! A token read through a prefix is as long as the IRI the prefix binds, and a document sets
//! both that length and how many tokens it writes. So reading a token builds nothing:
//! [`expand_token`] classifies it and, where it expands, returns the expansion as an [`Iri`], the
//! bound IRI and the token's suffix still where the document holds them. Whoever needs to compare
//! the expansion (is it this term's mapping, is it in this namespace, does it end in `hasUnit`)
//! asks the [`Iri`], in work that does not grow with the bound IRI's length. Whoever needs to
//! keep it joins it through the document's [`super::expansion_budget::ExpansionBudget`], the one
//! place an expansion is copied.

use std::collections::BTreeMap;
use std::fmt;

use crate::excerpt::{EXCERPT_BYTES, excerpt};

/// Declared `@context` prefix/term substitutions: term → IRI.
///
/// Invariant: every IRI in the table passes [`is_absolute_iri`]. The table is filled only by
/// [`super::prefix_table`], which refuses any other binding, and [`expand_curie`] relies on it to
/// judge a joined expansion by the token's suffix alone.
pub(super) type PrefixTable = BTreeMap<String, String>;

/// An absolute IRI a token stands for, as the one or two pieces of document text it is read
/// from: `head` then `tail`. A whole-token term match is the bound IRI and an empty tail; a
/// compact IRI is the bound IRI and the token's suffix; a token kept as written is the token.
///
/// Every method reads the two pieces in place. None of them copies the IRI, and none but
/// [`Iri::is`] (which compares lengths first) and [`fmt::Display`] does work that grows with
/// both pieces.
#[derive(Clone, Copy, Debug)]
pub(super) struct Iri<'a> {
    /// The leading piece.
    head: &'a str,
    /// The trailing piece; empty when the IRI is one piece.
    tail: &'a str,
}

impl<'a> Iri<'a> {
    /// The IRI that is `text` itself.
    pub(super) fn whole(text: &'a str) -> Self {
        Self {
            head: text,
            tail: "",
        }
    }

    /// The IRI in bytes.
    pub(super) fn len(self) -> usize {
        self.head.len() + self.tail.len()
    }

    /// Whether the IRI has no characters.
    pub(super) fn is_empty(self) -> bool {
        self.head.is_empty() && self.tail.is_empty()
    }

    /// The IRI as one piece of document text, when it is one: `None` for a compact IRI with a
    /// suffix, which exists whole only once it is joined.
    pub(super) fn as_whole(self) -> Option<&'a str> {
        if self.tail.is_empty() {
            Some(self.head)
        } else if self.head.is_empty() {
            Some(self.tail)
        } else {
            None
        }
    }

    /// Whether the IRI is exactly `text`. Texts of different lengths are told apart without
    /// reading either.
    pub(super) fn is(self, text: &str) -> bool {
        self.len() == text.len()
            && text
                .strip_prefix(self.head)
                .is_some_and(|rest| rest == self.tail)
    }

    /// The IRI without its leading `prefix`, or `None` when it does not start with it. Work is
    /// linear in `prefix`.
    pub(super) fn strip_prefix(self, prefix: &str) -> Option<Self> {
        if prefix.len() <= self.head.len() {
            let head = self.head.strip_prefix(prefix)?;
            Some(Self {
                head,
                tail: self.tail,
            })
        } else {
            let across = prefix.strip_prefix(self.head)?;
            Some(Self::whole(self.tail.strip_prefix(across)?))
        }
    }

    /// Whether the IRI ends with `suffix`. Work is linear in `suffix`.
    pub(super) fn ends_with(self, suffix: &str) -> bool {
        if suffix.len() <= self.tail.len() {
            self.tail.ends_with(suffix)
        } else {
            suffix
                .strip_suffix(self.tail)
                .is_some_and(|across| self.head.ends_with(across))
        }
    }

    /// Whether any character of the IRI is whitespace.
    pub(super) fn contains_whitespace(self) -> bool {
        self.head.contains(char::is_whitespace) || self.tail.contains(char::is_whitespace)
    }

    /// The IRI as a message quotes authored text: [`excerpt`] of it joined, built from at most
    /// one character more than [`EXCERPT_BYTES`] bytes of it.
    pub(super) fn excerpt(self) -> String {
        let mut lead = String::new();
        for c in self.head.chars().chain(self.tail.chars()) {
            lead.push(c);
            if lead.len() > EXCERPT_BYTES {
                break;
            }
        }
        excerpt(&lead).into_owned()
    }

    /// The IRI joined into one string of exactly its length: a copy of both pieces. The caller
    /// accounts for it, through the document's expansion budget or a bounded diagnostic list.
    pub(super) fn joined(self) -> String {
        let mut joined = String::with_capacity(self.len());
        joined.push_str(self.head);
        joined.push_str(self.tail);
        joined
    }
}

/// Writes the IRI joined, for a message that names it whole.
impl fmt::Display for Iri<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.head)?;
        f.write_str(self.tail)
    }
}

/// Whether `value` has syntactically absolute-IRI form: everything before the FIRST colon is
/// a nonempty ASCII scheme per RFC 3986 §3.1 — a letter, then letters, digits, `+`, `-`, or
/// `.` — and the remainder after that colon is nonempty and free of whitespace. The whole
/// remainder rule is engine policy, not conformance: a canonical durable key is nonempty and
/// whitespace-free — ASCII and Unicode alike — by choice. RFC 3986 agrees on ASCII
/// whitespace (no URI production admits one), but RFC 3987's `ucschar` (§2.2) admits
/// U+00A0, U+2028, and other `char::is_whitespace` characters, so this predicate refuses
/// some RFC 3987-conformant IRIs; and `ab:` is a legal URI (`path-empty`, RFC 3986 §3.3)
/// that carries no name to key a durable identity on. The contract stays syntactic form,
/// not semantic reachability: `S231:x` passes when `S231` is not an active prefix because it is an
/// RFC-valid scheme. [`super::merge_context_entries`] separately refuses a binding value when its
/// scheme position names an active prefix, closing the nested compact-IRI ambiguity before this
/// predicate runs. The predicate guards both directions of the remaining binding/identity
/// symmetry: `@context` binding values (so the legal JSON-LD binding `{"ex": "urn:"}` refuses as
/// `NonSubsetConstruct`), the two keep-as-written identity arms of [`expand_token`], and the
/// declared-prefix arm's joined expansion.
pub(super) fn is_absolute_iri(value: &str) -> bool {
    match value.split_once(':') {
        Some((scheme, rest)) => {
            let mut chars = scheme.chars();
            let scheme_ok = match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                }
                _ => false,
            };
            scheme_ok && !rest.is_empty() && !rest.contains(char::is_whitespace)
        }
        None => false,
    }
}

/// The outcome of expanding one token against the prefix table. Nothing is copied: an expansion
/// borrows the table and the token.
pub(super) enum Expansion<'a> {
    /// A whole-token term match, or a CURIE with a declared prefix whose joined expansion
    /// passes [`is_absolute_iri`]: the canonical form.
    Expanded(Iri<'a>),
    /// Kept as written: a `//`-suffix or undeclared-prefix token that is itself a
    /// syntactically absolute IRI ([`is_absolute_iri`]).
    Verbatim,
    /// No canonical absolute form: no `:` and no whole-token term match; a prefix that is
    /// not an RFC 3986 scheme (empty, or scheme-invalid like `2024`), whether or not the
    /// suffix starts with `//`; a valid scheme whose remainder is empty or carries
    /// whitespace (`ab:`, `ab:c d`); or a declared-prefix CURIE whose joined expansion
    /// fails the predicate (a whitespace-bearing suffix, `ex:c d` with `ex` bound).
    Relative,
}

/// Classify and expand one token. Pure, and it allocates nothing; the caller decides what
/// `Relative` means per slot.
///
/// [`is_absolute_iri`] gates the outcome of every arm. The two keep-as-written arms are
/// symmetric with the `@context` binding check: a token survives verbatim only when the
/// predicate accepts it, whether its suffix starts with `//` (never CURIE-split) or its
/// prefix is simply undeclared — so a spelling refused as a binding value cannot become a
/// durable key on those arms. The declared-prefix arm holds the JOINED result to the same
/// predicate: a declared prefix rescues a scheme-invalid CURIE
/// spelling (`2024:x` with `2024` bound expands, since the binding supplies the scheme)
/// but never a whitespace-bearing suffix (`ex:c d` refuses with `ex` bound). No arm can
/// produce a durable key the predicate refuses.
pub(super) fn expand_token<'a>(token: &'a str, table: &'a PrefixTable) -> Expansion<'a> {
    // A whole-token term match wins over CURIE splitting (JSON-LD term semantics): a token
    // that IS a declared term substitutes even though it contains no `:`.
    if let Some(iri) = table.get(token) {
        return Expansion::Expanded(Iri::whole(iri));
    }
    expand_curie(token, table)
}

/// [`expand_token`] without its whole-token term match: a token read as a compact IRI through its
/// prefix, or kept as an absolute IRI. The prefix it looks up is everything before the first `:`,
/// so it is never the token itself; [`super::own_expansion`] reads a term's own expansion this
/// way, without copying the table.
pub(super) fn expand_curie<'a>(token: &'a str, table: &'a PrefixTable) -> Expansion<'a> {
    match token.split_once(':') {
        // A value whose suffix begins with `//` is interpreted as an IRI, not
        // CURIE-expanded (JSON-LD 1.1 §4.1.5, Compact IRIs), so this token is
        // never CURIE-split and never prefix-rescued: `http://…` stays absolute even when
        // its scheme collides with a declared prefix name, and a scheme-invalid spelling
        // (`2024://x`) has no canonical form at all. It must refuse here rather than fall
        // through to the prefix lookup, which would mint `<bound-iri>//x` as a durable key.
        Some((_, suffix)) if suffix.starts_with("//") => {
            if is_absolute_iri(token) {
                Expansion::Verbatim
            } else {
                Expansion::Relative
            }
        }
        Some((prefix, suffix)) => match table.get(prefix) {
            // The JOINED expansion must pass the same predicate the other arms enforce.
            // Every accepted binding contributes a valid scheme and a clean nonempty
            // remainder head ([`PrefixTable`]'s invariant), so the join can only fail on an
            // authored suffix carrying whitespace (`ex:c d`) — expanding that would mint a key
            // the engine's own importer refuses, splitting compact/expanded twins of one
            // subject. The suffix alone is read: the bound IRI is as long as the document made
            // it, and reading it for every token through it is work of tokens × IRI bytes.
            Some(iri) => {
                if suffix.contains(char::is_whitespace) {
                    Expansion::Relative
                } else {
                    Expansion::Expanded(Iri {
                        head: iri,
                        tail: suffix,
                    })
                }
            }
            None if is_absolute_iri(token) => Expansion::Verbatim,
            // `2024:x` and `:x` (no RFC 3986 scheme) and `ab:` / `ab:c d` (valid scheme,
            // empty or whitespace-bearing remainder) expand against nothing and have no
            // absolute form: refusing beats minting a durable key from a malformed spelling
            // with zero diagnostics.
            None => Expansion::Relative,
        },
        None => Expansion::Relative,
    }
}

#[cfg(test)]
#[path = "token_expansion_tests.rs"]
mod tests;
