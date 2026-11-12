//! The terms of the S231 vocabulary the engine reads, and the refusal of a typing token that names
//! none of them (GitHub #620; owner rulings of 2026-11-12).
//!
//! A typing token, a `@type` value or an `isOfDataType` reference, in the S231 namespace must name a
//! term of the vocabulary: on any node, in either slot. The resolver reads a node's `@type` only
//! where it looks up a leaf's class or a connector's data type, so a misspelt S231 type on a
//! composite, a parameter, a connector that declares `isOfDataType`, or any other node loaded
//! without a word, and an `isOfDataType` naming a CDL enumeration was matched by its last dotted
//! segment alone. A term outside the vocabulary is now refused as `malformed-document`, subject the
//! node, the token and term quoted as excerpts ([`crate::excerpt`]).
//!
//! The vocabulary is one set, built once and read with one lookup per token:
//!
//! - **[`TERMS`]**, the 33 vocabulary terms, each with its provenance: the 23 classes the vendored
//!   `CXF-Core.jsonld` declares; the 8 terms it uses as a type or data type without declaring them
//!   (`BooleanInput`, `BooleanOutput`, `Datatype`, `EnumerationType`, `Real`, `Integer`, `Boolean`,
//!   `String`); and the 2 the engine writes that CXF-Core has neither declared nor used
//!   (`StringInput`, `StringOutput`). Each also records whether the engine reads or writes it.
//! - **the registered CDL block classes**, each written `Buildings.Controls.OBC.CDL.…`: the block
//!   registry's classes but its three internal `urn:oce:lowering#PassThrough.*` identities, which
//!   are never an S231 term.
//! - **the CDL and G36 enumeration and constant-package rows**, each by its full path
//!   (`Buildings.Controls.OBC.CDL.Types.SimpleController`), never by its last segment.
//!
//! A term a newer CXF-Core adds is refused until [`TERMS`] holds it: a test pins the table against
//! the vendored `CXF-Core.jsonld`, so a pin advance that adds a class fails it and says to update
//! the table, and another pins the class count against the registry.
//!
//! Only the S231 namespace is judged: the expanded IRI under it, or a literal `S231:` prefix in a
//! document that binds no `S231`. Another namespace's class (a leaf's `http://example.org#…` class
//! path, a composite's own class) is the readers' to judge. The retired namespace and the `https:`
//! spellings are refused before this check, as a namespace mismatch.

use std::collections::HashSet;
use std::sync::OnceLock;

use oce_diag::{DiagCode, Diagnostic};

use crate::excerpt::excerpt;
use crate::export::S231_CONTEXT_IRI;

/// Where a term of [`TERMS`] stands in the vendored `CXF-Core.jsonld`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CxfCore {
    /// An `rdfs:Class` it declares.
    Declared,
    /// A type or data type it uses without declaring it.
    UsedUndeclared,
    /// Neither declared nor used there.
    Absent,
}

/// One vocabulary term: its name after `S231:`, its place in CXF-Core, and whether the engine
/// reads or writes it. The provenance is the table's record, which its pin tests read; the check
/// reads only the name.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the provenance is read by the table's pin tests")
)]
pub(super) struct Term {
    /// The term, as written after `S231:`.
    pub(super) name: &'static str,
    /// Where it stands in CXF-Core.
    pub(super) cxf_core: CxfCore,
    /// Whether the engine's resolver reads it or its exporters write it.
    pub(super) engine: bool,
}

const fn term(name: &'static str, cxf_core: CxfCore, engine: bool) -> Term {
    Term {
        name,
        cxf_core,
        engine,
    }
}

use CxfCore::{Absent, Declared, UsedUndeclared};

/// The vocabulary terms, in name order within each provenance.
pub(super) const TERMS: [Term; 33] = [
    // Declared by CXF-Core as an `rdfs:Class`.
    term("AnalogDataType", Declared, false),
    term("AnalogInput", Declared, true),
    term("AnalogOutput", Declared, true),
    term("Block", Declared, true),
    term("BooleanDataType", Declared, false),
    term("CompositeBlock", Declared, false),
    term("Connector", Declared, false),
    term("Constant", Declared, false),
    term("DataType", Declared, false),
    term("ElementaryBlock", Declared, false),
    term("EnumerationDatatype", Declared, false),
    term("ExtensionBlock", Declared, true),
    term("InputConnector", Declared, false),
    term("IntegerDataType", Declared, false),
    term("IntegerInput", Declared, true),
    term("IntegerOutput", Declared, true),
    term("OutputConnector", Declared, false),
    term("Package", Declared, false),
    term("Parameter", Declared, true),
    term("RealDatatype", Declared, false),
    term("RealInput", Declared, true),
    term("RealOutput", Declared, true),
    term("StringDatatype", Declared, false),
    // Used by CXF-Core as a type or data type, never declared there.
    term("Boolean", UsedUndeclared, true),
    term("BooleanInput", UsedUndeclared, true),
    term("BooleanOutput", UsedUndeclared, true),
    term("Datatype", UsedUndeclared, false),
    term("EnumerationType", UsedUndeclared, false),
    term("Integer", UsedUndeclared, true),
    term("Real", UsedUndeclared, true),
    term("String", UsedUndeclared, true),
    // Written by the engine, absent from CXF-Core.
    term("StringInput", Absent, true),
    term("StringOutput", Absent, true),
];

/// The Modelica package every CDL and G36 class path starts with.
const OBC_PACKAGE: &str = "Buildings.Controls.OBC.";

/// Every term the vocabulary holds, built once: [`TERMS`], the registered CDL block classes and
/// the enumeration rows.
fn vocabulary() -> &'static HashSet<String> {
    static VOCABULARY: OnceLock<HashSet<String>> = OnceLock::new();
    VOCABULARY.get_or_init(|| {
        let terms = TERMS.iter().map(|term| term.name.to_owned());
        let classes = oce_blocks::catalog()
            .iter()
            .filter(|entry| entry.class_path.starts_with("CDL."))
            .map(|entry| format!("{OBC_PACKAGE}{}", entry.class_path));
        let enumerations = oce_model::enumeration_classes()
            .iter()
            .map(|row| row.qualified_path.to_owned());
        terms.chain(classes).chain(enumerations).collect()
    })
}

/// Refuse a typing token in the S231 namespace whose term the vocabulary does not hold, after
/// expansion; returns whether it refused. `owner` is the node's expanded `@id`.
pub(super) fn check_term(
    token: &str,
    slot: &str,
    owner: &str,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let Some(term) = token
        .strip_prefix(S231_CONTEXT_IRI)
        .or_else(|| token.strip_prefix("S231:"))
    else {
        return false;
    };
    if vocabulary().contains(term) {
        return false;
    }
    let (quoted, named) = (excerpt(token), excerpt(term));
    let subject = if owner.is_empty() { &quoted } else { owner };
    diags.push(
        Diagnostic::error(
            DiagCode::MalformedDocument,
            format!(
                "{slot} `{quoted}` on node `{owner}` names `{named}`, which is no term of the S231 \
                 vocabulary the engine reads (a CXF-Core class or data type, a term the engine \
                 writes, a registered CDL block class, or a CDL or G36 enumeration type by its full \
                 path)"
            ),
        )
        .with_subject(subject.to_owned()),
    );
    true
}

#[cfg(test)]
#[path = "vocabulary_terms_tests.rs"]
mod tests;
