//! The IRI scheme (authoring note, "Identity"; decisions 6 and 10).
//!
//! Every node the emitter writes is named `{namespace}{sequence}` (the root) or
//! `{namespace}{sequence}.{local}`, where `local` is a declared name, `block.param` or
//! `block.port`. Stripping the root prefix from an engine diagnostic's subject therefore yields
//! the model path, which is how stage 2 maps a diagnostic back ([`Scheme::model_path`]). Block
//! classes are named under the fixed class base [`CLASS_BASE`], independent of the instance
//! namespace, because the engine's class bridge reads only the fragment after the last `#`.

/// The default instance namespace.
pub const DEFAULT_NAMESPACE: &str = "urn:oce:author:";

/// The longest namespace [`crate::EmitOptions`] accepts, in bytes.
pub const MAX_NAMESPACE_BYTES: usize = 256;

/// The class base every block `@type` is written under (decision 10):
/// `http://example.org#Buildings.Controls.OBC.<class_path>`.
pub const CLASS_BASE: &str = "http://example.org#Buildings.Controls.OBC.";

/// The prefix of a full enumeration member or enumeration class path in CXF.
pub(crate) const MODELICA_PACKAGE: &str = "Buildings.Controls.OBC.";

/// The `S231` context IRI.
pub(crate) const S231_IRI: &str = "http://data.ashrae.org/S231P#";

/// The XML Schema double datatype IRI a Real literal is typed with.
pub(crate) const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";

/// Why a namespace is refused, if it is: it must be non-empty, at most
/// [`MAX_NAMESPACE_BYTES`] bytes of printable ASCII with no space or IRI-reserved delimiter
/// (`"<>{}|\^` and the backtick), and end in `#`, `/` or `:`, so the content-id note's
/// last-delimiter rule (GitHub #470) strips exactly the namespace.
pub(crate) fn namespace_problem(namespace: &str) -> Option<&'static str> {
    if namespace.is_empty() {
        return Some("is empty");
    }
    if namespace.len() > MAX_NAMESPACE_BYTES {
        return Some("is longer than 256 bytes");
    }
    if namespace
        .bytes()
        .any(|b| !(0x21..=0x7e).contains(&b) || b"\"<>{}|\\^`".contains(&b))
    {
        return Some("contains a character outside printable ASCII or an IRI-reserved one");
    }
    if !namespace.ends_with(['#', '/', ':']) {
        return Some("does not end in `#`, `/` or `:`");
    }
    None
}

/// The IRIs of one emitted sequence.
#[derive(Clone, Debug)]
pub(crate) struct Scheme {
    root: String,
    prefix: String,
}

impl Scheme {
    /// The scheme for `sequence` under `namespace`.
    pub(crate) fn new(namespace: &str, sequence: &str) -> Self {
        let root = format!("{namespace}{sequence}");
        let prefix = format!("{root}.");
        Self { root, prefix }
    }

    /// The root (sequence) IRI.
    pub(crate) fn root(&self) -> &str {
        &self.root
    }

    /// The IRI of a declared name or a `block.member` path.
    pub(crate) fn local(&self, path: &str) -> String {
        format!("{}{path}", self.prefix)
    }

    /// The IRI of `block.member`.
    pub(crate) fn member(&self, block: &str, member: &str) -> String {
        format!("{}{block}.{member}", self.prefix)
    }

    /// The model path an IRI names: `Some("")` for the root, `Some(path)` under the root prefix,
    /// `None` for anything else.
    pub(crate) fn model_path<'a>(&self, iri: &'a str) -> Option<&'a str> {
        if iri == self.root {
            return Some("");
        }
        iri.strip_prefix(self.prefix.as_str())
            .filter(|path| !path.is_empty())
    }
}

/// The class IRI of a catalog class path.
pub(crate) fn class_iri(class_path: &str) -> String {
    format!("{CLASS_BASE}{class_path}")
}
