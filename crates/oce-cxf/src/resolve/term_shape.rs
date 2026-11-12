//! The shape of a §7.4.1 term (`S231:unit`, `S231:quantity`, `S231:displayUnit`) on every node
//! that carries one: a connector, a boundary and a parameter (GitHub issues #559 and #602; owner
//! rulings of 2026-11-11 and 2026-11-12).
//!
//! A term is one of exactly three shapes ([`TermAttr`]): a string, a typed literal holding exactly
//! `@value` and `@type`, or an IRI node holding exactly `@id`. Anything else is refused as
//! `malformed-document`, subject the node, one refusal per term:
//!
//! - **another JSON shape** (a number, a boolean, a list, an object that is none of the three);
//! - **an object carrying both `@id` and `@value`**, with or without `@type`. JSON-LD does not let
//!   a value object carry `@id`, and the term read would depend on whether `@type` is present: the
//!   typed-literal reading took the `@value` and dropped the `@id`, the IRI-node reading the
//!   reverse;
//! - **a key beside those**: `@language`, `@index`, a property such as `qudt:symbol`. Nothing reads
//!   it, and export writes every term back as a bare string, so it would be lost without a word;
//! - **an empty or whitespace-only term**: the string, the `@value` or the `@id`, as [`str::trim`]
//!   reads whitespace. The unit gate would read it as a declared unit: unequal to `"K"`, so a
//!   cluster that an absent unit would let load is refused as a mismatch; alone, propagated to
//!   every peer that declares none, and reported to the host as a unit;
//! - **a term carrying a Unicode format (`Cf`) or control (`Cc`) character** anywhere in the
//!   string, the `@value` or the `@id` (issue #629; owner ruling of 2026-11-12, the rule GitHub
//!   #641 applies to a typing token, decided by the same [`crate::is_format_or_control`]): a
//!   zero-width space, a byte-order mark, U+2060, U+180E, a soft hyphen, a bidirectional control,
//!   a C0 or C1 control. No reader can see one, so `"K\u{200B}"` would reach the unit gate and the
//!   host's point list as a unit that looks like `K` and equals no other. The message names the
//!   first such code point and quotes the term as an excerpt with each such character written
//!   `\u{…}`.
//!
//! The problems are checked in that order and the first one found is reported, so a term is
//! refused once. The blank check comes before the invisible-character check: a term of whitespace
//! alone is blank, even where that whitespace is also `Cc` (a tab, a line feed, U+0085). Whitespace
//! of that kind around or inside a visible term is not trimmed away first, as it is for a typing
//! token: `"K\t"` is refused as carrying U+0009, since no other rule refuses it (a term may carry
//! surrounding spaces) and it would reach the gate as a unit that is not `K`. A `@context` inside a
//! term object is refused before any of these, by the expansion pass, as a scoped context.

use crate::dto::TermAttr;
use crate::excerpt::{excerpt, named_keys};
use crate::format_controls::{describe, first_format_or_control, visible};

/// What a refusal calls an empty or whitespace-only string.
pub(super) const BLANK_STRING: &str = "an empty or whitespace-only string";

/// What a refusal calls a typed literal whose `@value` is one.
pub(super) const BLANK_VALUE: &str = "a typed literal whose @value is empty or whitespace-only";

/// What is wrong with `term`, as the noun phrase a refusal reads (`S231:unit … is <it>`), or
/// `None` when it is one of the three shapes, not blank and free of format and control characters.
pub(super) fn problem(term: &TermAttr) -> Option<String> {
    match term {
        TermAttr::Other(_) => Some("not a string, typed literal, or IRI node".to_owned()),
        TermAttr::Typed { extra, .. } if extra.contains_key("@id") => Some(BOTH.to_owned()),
        TermAttr::Iri { extra, .. } if extra.contains_key("@value") => Some(BOTH.to_owned()),
        TermAttr::Typed { extra, .. } if !extra.is_empty() => Some(format!(
            "a typed literal carrying {} beside @value and @type",
            named_keys(extra)
        )),
        TermAttr::Iri { extra, .. } if !extra.is_empty() => Some(format!(
            "an IRI node carrying {} beside its @id",
            named_keys(extra)
        )),
        TermAttr::Bare(text) if blank(text) => Some(BLANK_STRING.to_owned()),
        TermAttr::Typed { value, .. } if blank(value) => Some(BLANK_VALUE.to_owned()),
        TermAttr::Iri { id, .. } if blank(id) => {
            Some("an IRI node whose @id is empty or whitespace-only".to_owned())
        }
        TermAttr::Bare(text) => invisible(text, "the string", ", which carries"),
        TermAttr::Typed { value, .. } => {
            invisible(value, "a typed literal whose @value", " carries")
        }
        TermAttr::Iri { id, .. } => invisible(id, "an IRI node whose @id", " carries"),
    }
}

/// What a refusal calls `text`, the lexical form of a well-shaped term, when it carries a format
/// or control character: `<holder> `<text>`<carries> U+200B, a format character (Cf) that no
/// reader can see`, the text an excerpt with each such character escaped; `None` when it carries
/// none.
fn invisible(text: &str, holder: &str, carries: &str) -> Option<String> {
    let found = first_format_or_control(text)?;
    Some(format!(
        "{holder} `{}`{carries} {} that no reader can see",
        visible(&excerpt(text)),
        describe(found)
    ))
}

/// What a refusal calls an object carrying both `@id` and `@value`.
const BOTH: &str = "an object carrying both @id and @value";

/// Whether `text` is empty or holds only Unicode `White_Space`.
pub(super) fn blank(text: &str) -> bool {
    text.trim().is_empty()
}
