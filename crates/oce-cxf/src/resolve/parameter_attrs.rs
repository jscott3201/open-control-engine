//! The shape check of a parameter's descriptive attributes (GitHub issue #559; owner ruling of
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
//! - a **term** (`unit`, `quantity`, `displayUnit`) is a string, a typed literal or an IRI node,
//!   the three shapes a connector's term takes ([`crate::dto::TermAttr`]). Anything else (a
//!   number, a boolean, a list, an object that is neither) is refused.
//!
//! JSON `null` reads as absent, as on a connector. The value of a well-shaped attribute is still
//! never read. A shape outside the typed DTO's arms (an object with no `@value`) already fails
//! [`crate::parse_document`], before this check.
//!
//! The check runs at the two places a parameter is read: every parameter a leaf instance grounds
//! (its `hasParameter` and `hasConstant` lists and its classified `hasInstance` parameter members),
//! and every declaration of an active composite's own chain, in the lowering pass. A parameter
//! node read twice is reported once per read. A node pruned by a false conditional guard is not
//! read and so not checked.

use oce_diag::{DiagCode, Diagnostic};

use crate::dto::{CxfValue, Node, TermAttr};

/// Refuse each malformed descriptive attribute of the parameter node `node`, read as `subject`.
pub(super) fn refuse_malformed(node: &Node, subject: &str, diags: &mut Vec<Diagnostic>) {
    for (which, bound) in [("min", &node.min), ("max", &node.max)] {
        let found = match bound {
            Some(CxfValue::List(_)) => "a list",
            Some(CxfValue::Bool(_)) => "a boolean",
            Some(
                CxfValue::Int(_) | CxfValue::Float(_) | CxfValue::Typed { .. } | CxfValue::Expr(_),
            )
            | None => continue,
        };
        diags.push(malformed(
            subject,
            format!(
                "S231:{which} on a parameter is {found}; a bound is one number, typed literal or \
                 expression string"
            ),
        ));
    }
    for (which, term) in [
        ("unit", &node.unit),
        ("quantity", &node.quantity),
        ("displayUnit", &node.display_unit),
    ] {
        if let Some(TermAttr::Other(_)) = term {
            diags.push(malformed(
                subject,
                format!(
                    "S231:{which} on a parameter is not a string, typed literal, or IRI node — \
                     malformed term"
                ),
            ));
        }
    }
}

fn malformed(subject: &str, message: String) -> Diagnostic {
    Diagnostic::error(DiagCode::MalformedDocument, message).with_subject(subject.to_owned())
}
