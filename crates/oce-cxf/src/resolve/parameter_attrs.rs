//! The shape check of a parameter's descriptive attributes (GitHub issue #559; owner rulings of
//! 2026-11-11).
//!
//! A parameter node may carry `S231:min`, `S231:max`, `S231:unit`, `S231:quantity` and
//! `S231:displayUnit`. The engine uses none of them for control, and before this check nothing
//! read them on a parameter, so a malformed one loaded unjudged: the list of a typed literal's
//! values, `["30", "http://www.w3.org/2001/XMLSchema#double"]`, among them. Each is now held to a
//! legal JSON-LD shape where the parameter is read, and a malformed one is refused with
//! `malformed-document`, subject the parameter node:
//!
//! - a **bound** (`min`, `max`) is one number, one typed literal (`{"@value": …, "@type": …}`) or
//!   one string. The string is the form modelica-json writes for a bound over other parameters or
//!   a constant expression (`"yMin"`, `"100*1e-15"`); it is not evaluated, because the bound is
//!   not used. A list and a boolean are refused: no bound is either.
//! - a **term** (`unit`, `quantity`, `displayUnit`) is held to the shape every term is, on a
//!   connector and a boundary too ([`super::term_shape`], GitHub #602): a string, a typed literal
//!   holding exactly `@value` and `@type`, or an IRI node holding exactly `@id`, carrying no
//!   Unicode format or control character (issue #629). Anything else (a number, a boolean, a list,
//!   an object carrying both `@id` and `@value` or another key, a zero-width space or a tab in the
//!   term) is refused.
//! - **No string may be empty or whitespace-only** (owner ruling): not a bound or term written as
//!   a string, not a typed literal's `@value`, and not an IRI node's `@id` (the term's lexical
//!   form, [`crate::dto::TermAttr::as_term`]). Whitespace is Unicode `White_Space`, as
//!   [`str::trim`] reads it.
//!
//! - **A `unit` or `displayUnit` written as an IRI node** goes through the rule a connector's does
//!   (issue #631; design note `docs/design/unit-spellings.md`, decision 2): it is read through the
//!   closed unit table ([`crate::unit_iri`]), under any key that names the property
//!   (`qudt:hasUnit`, `S231:hasUnit` and `S231:hasDisplayUnit` among them), and one the table does
//!   not map is refused as `non-subset-construct`, subject the parameter node. `quantity` is not.
//!
//! JSON `null` reads as absent, as on a connector. The value of a well-shaped attribute is still
//! never used for control: it reaches no gate, no `ParamAttrs` field and no point.
//!
//! Not every malformed bound reaches this check. A bound whose JSON matches none of
//! [`crate::dto::CxfValue`]'s arms (an object that is not a typed literal with a string `@value`
//! and `@type`: `{}`, `{"@id": …}`, `{"@value": "30"}`, `{"@list": …}`, or a list holding one)
//! fails [`crate::parse_document`] for the whole document, as `CxfError::Json`, with no code and no
//! subject, wherever the parameter is, read or not. A term never does: `TermAttr` ends in a
//! catch-all arm, so every malformed term arrives here.
//!
//! The check runs at the two places a parameter is read: every parameter a leaf instance grounds
//! (its `hasParameter` and `hasConstant` lists and its classified `hasInstance` parameter members),
//! and every declaration of an active composite's own chain, in the lowering pass. Each parameter
//! node is checked once per resolve, however often it is read ([`CheckedParameters`]): two leaves
//! listing one node, one leaf listing it twice, or a declaration a leaf also lists. A node pruned
//! by a false conditional guard is not read and so not checked.

use std::collections::HashSet;

use oce_diag::{DiagCode, Diagnostic};

use super::term_shape::{self, BLANK_STRING, BLANK_VALUE, blank};
use crate::dto::{CxfValue, Node};
use oce_diag::DiagnosticList;

/// The parameter nodes already checked in one resolve, keyed by their expanded `@id`, so a node
/// read more than once is reported once. Lookup-only: never iterated, so it orders nothing.
#[derive(Debug, Default)]
pub(super) struct CheckedParameters(HashSet<String>);

impl CheckedParameters {
    /// Refuse each malformed descriptive attribute of the parameter node `node`, read as
    /// `subject`, unless that node was already checked in this resolve.
    pub(super) fn refuse_malformed(
        &mut self,
        node: &Node,
        subject: &str,
        diags: &mut DiagnosticList,
    ) {
        if !self.0.insert(subject.to_owned()) {
            return;
        }
        for (which, bound) in [("min", &node.min), ("max", &node.max)] {
            let found = match bound {
                Some(CxfValue::List(_)) => "a list",
                Some(CxfValue::Bool(_)) => "a boolean",
                Some(CxfValue::Expr(text)) if blank(text) => BLANK_STRING,
                Some(CxfValue::Typed { value, .. }) if blank(value) => BLANK_VALUE,
                Some(
                    CxfValue::Int(_)
                    | CxfValue::Float(_)
                    | CxfValue::Typed { .. }
                    | CxfValue::Expr(_),
                )
                | None => continue,
            };
            diags.push(malformed(
                subject,
                format!(
                    "S231:{which} on a parameter is {found}; a bound is one number, typed literal \
                     or expression string"
                ),
            ));
        }
        for (which, term) in [
            ("unit", &node.unit),
            ("quantity", &node.quantity),
            ("displayUnit", &node.display_unit),
        ] {
            let Some(term) = term.as_ref() else {
                continue;
            };
            if let Some(found) = term_shape::problem(term) {
                diags.push(malformed(
                    subject,
                    format!("S231:{which} on a parameter is {found} — malformed term"),
                ));
                continue;
            }
            // `quantity` is a verbatim lexical term; only a unit goes through the table.
            if which == "quantity" {
                continue;
            }
            if let Some(found) = term_shape::unmapped_unit(term) {
                diags.push(
                    Diagnostic::error(
                        DiagCode::NonSubsetConstruct,
                        format!("S231:{which} on a parameter is {found}"),
                    )
                    .with_subject(subject.to_owned()),
                );
            }
        }
    }
}

fn malformed(subject: &str, message: String) -> Diagnostic {
    Diagnostic::error(DiagCode::MalformedDocument, message).with_subject(subject.to_owned())
}
