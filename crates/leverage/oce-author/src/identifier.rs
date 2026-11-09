//! Authored names and the `extends` provenance id.
//!
//! An [`Identifier`] names a sequence, a sequence parameter, a boundary input or output, a block,
//! an assigned parameter or a port. Its rules come from the authoring note ("The authoring
//! model"): `[A-Za-z_][A-Za-z0-9_]*`, at most [`MAX_IDENTIFIER_BYTES`] bytes, and — for a name the
//! author declares — not a Modelica reserved word ([`MODELICA_RESERVED_WORDS`]) or a text-form
//! keyword ([`TEXT_FORM_KEYWORDS`]).
//!
//! The model can hold a name that breaks these rules, because the wire form must be able to carry
//! one so that [`crate::validate()`] reports it in the author's terms (`invalid-name`) instead of a
//! parse error. [`Identifier::new`] is the checked constructor the builder uses;
//! [`Identifier::unchecked`] is the escape hatch the wire parser uses.

use std::fmt;

/// The longest identifier, in bytes.
pub const MAX_IDENTIFIER_BYTES: usize = 64;

/// The longest provenance id, in bytes (decision 5).
pub const MAX_PROVENANCE_ID_BYTES: usize = 256;

/// The Modelica 3.6 reserved words (Modelica Language Specification §2.3.3). An authored name
/// is never one of them, because every name becomes a Modelica-shaped instance name.
pub const MODELICA_RESERVED_WORDS: &[&str] = &[
    "algorithm",
    "and",
    "annotation",
    "block",
    "break",
    "class",
    "connect",
    "connector",
    "constant",
    "constrainedby",
    "der",
    "discrete",
    "each",
    "else",
    "elseif",
    "elsewhen",
    "encapsulated",
    "end",
    "enumeration",
    "equation",
    "expandable",
    "extends",
    "external",
    "false",
    "final",
    "flow",
    "for",
    "function",
    "if",
    "import",
    "impure",
    "in",
    "initial",
    "inner",
    "input",
    "loop",
    "model",
    "not",
    "operator",
    "or",
    "outer",
    "output",
    "package",
    "parameter",
    "partial",
    "protected",
    "public",
    "pure",
    "record",
    "redeclare",
    "replaceable",
    "return",
    "stream",
    "then",
    "true",
    "type",
    "when",
    "while",
    "within",
];

/// The keywords of the text form (authoring note, "Text form"): the statement words, `none`, the
/// Boolean literals and the three type names. An authored name is never one of them, so the text
/// form (a later item) needs no change to the model.
pub const TEXT_FORM_KEYWORDS: &[&str] = &[
    "sequence", "extends", "none", "param", "in", "out", "block", "true", "false", "Real",
    "Integer", "Boolean",
];

/// The words the text form reads as non-finite numbers (`NaN`, `inf`, `Infinity`), compared
/// ignoring ASCII case. A declared name is never one of them, so a reference to a declared name is
/// always spellable in text (issue #482 review); [`check_declared_name`] refuses them as
/// [`IdentifierError::Keyword`].
pub const NON_FINITE_WORDS: &[&str] = &["nan", "inf", "infinity"];

/// Why a name is not a valid identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentifierError {
    /// The name is empty.
    Empty,
    /// The name is longer than [`MAX_IDENTIFIER_BYTES`]; carries its length in bytes.
    TooLong(usize),
    /// The first character is not an ASCII letter or `_`.
    BadStart,
    /// A character is not an ASCII letter, digit or `_` (a dot included); carries its byte offset.
    BadCharacter(usize),
    /// The name is a Modelica reserved word.
    ReservedWord,
    /// The name is a text-form keyword, or a non-finite word ([`NON_FINITE_WORDS`], any case).
    Keyword,
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the name is empty"),
            Self::TooLong(len) => write!(
                f,
                "the name is {len} bytes long; at most {MAX_IDENTIFIER_BYTES} are allowed"
            ),
            Self::BadStart => f.write_str("a name starts with an ASCII letter or `_`"),
            Self::BadCharacter(at) => write!(
                f,
                "byte {at} is not an ASCII letter, digit or `_` (dots are not allowed)"
            ),
            Self::ReservedWord => f.write_str("the name is a Modelica reserved word"),
            Self::Keyword => f.write_str("the name is a text-form keyword"),
        }
    }
}

impl std::error::Error for IdentifierError {}

/// Check the syntax of a name: `[A-Za-z_][A-Za-z0-9_]*` and at most [`MAX_IDENTIFIER_BYTES`]
/// bytes. Reserved words and keywords are not checked here (port and catalog parameter names are
/// not authored, so only [`check_declared_name`] refuses them).
///
/// # Errors
///
/// The first rule the name breaks.
pub fn check_identifier_syntax(name: &str) -> Result<(), IdentifierError> {
    let bytes = name.as_bytes();
    let Some(&first) = bytes.first() else {
        return Err(IdentifierError::Empty);
    };
    if bytes.len() > MAX_IDENTIFIER_BYTES {
        return Err(IdentifierError::TooLong(bytes.len()));
    }
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return Err(IdentifierError::BadStart);
    }
    if let Some(at) = bytes
        .iter()
        .position(|&b| !(b.is_ascii_alphanumeric() || b == b'_'))
    {
        return Err(IdentifierError::BadCharacter(at));
    }
    Ok(())
}

/// Check a name the author declares (a sequence, parameter, boundary or block name): the syntax
/// of [`check_identifier_syntax`], then not a Modelica reserved word, then not a text-form keyword
/// or a non-finite word ([`NON_FINITE_WORDS`], any case).
///
/// # Errors
///
/// The first rule the name breaks.
pub fn check_declared_name(name: &str) -> Result<(), IdentifierError> {
    check_identifier_syntax(name)?;
    if MODELICA_RESERVED_WORDS.contains(&name) {
        return Err(IdentifierError::ReservedWord);
    }
    if TEXT_FORM_KEYWORDS.contains(&name)
        || NON_FINITE_WORDS
            .iter()
            .any(|w| w.eq_ignore_ascii_case(name))
    {
        return Err(IdentifierError::Keyword);
    }
    Ok(())
}

/// An authored name. See the module documentation for the rules and why the model can hold a
/// name that breaks them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier(String);

impl Identifier {
    /// A checked declared name ([`check_declared_name`]).
    ///
    /// # Errors
    ///
    /// The first rule `name` breaks.
    pub fn new(name: &str) -> Result<Self, IdentifierError> {
        check_declared_name(name)?;
        Ok(Self(name.to_owned()))
    }

    /// A name taken as written, unchecked. The wire parser uses it so that [`crate::validate()`]
    /// can report a bad name as a finding; call [`Identifier::check`] to judge it.
    #[must_use]
    pub fn unchecked(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Judge the name as a declared name ([`check_declared_name`]).
    ///
    /// # Errors
    ///
    /// The first rule the name breaks.
    pub fn check(&self) -> Result<(), IdentifierError> {
        check_declared_name(&self.0)
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why a provenance id is malformed (decision 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProvenanceIdError {
    /// The id is empty; an absent `extends` is `None`, not an empty id.
    Empty,
    /// The id is longer than [`MAX_PROVENANCE_ID_BYTES`]; carries its length in bytes.
    TooLong(usize),
    /// There is no lowercase format prefix (`[a-z][a-z0-9]*`) before the first colon.
    BadPrefix,
    /// Nothing follows the colon.
    EmptyBody,
    /// A byte is not printable non-space ASCII; carries its byte offset.
    BadCharacter(usize),
}

impl fmt::Display for ProvenanceIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("the provenance id is empty"),
            Self::TooLong(len) => write!(
                f,
                "the provenance id is {len} bytes long; at most {MAX_PROVENANCE_ID_BYTES} are allowed"
            ),
            Self::BadPrefix => f.write_str(
                "a provenance id starts with a lowercase format prefix (`[a-z][a-z0-9]*`) and a colon",
            ),
            Self::EmptyBody => f.write_str("nothing follows the format prefix's colon"),
            Self::BadCharacter(at) => {
                write!(f, "byte {at} is not printable, non-space ASCII")
            }
        }
    }
}

impl std::error::Error for ProvenanceIdError {}

/// Check the decision-5 grammar: a lowercase format prefix `[a-z][a-z0-9]*`, a colon, then at
/// least one byte of printable non-space ASCII (`!` to `~`), at most [`MAX_PROVENANCE_ID_BYTES`]
/// bytes in all. `cxf:fnv1a128:0123abcd` fits.
///
/// # Errors
///
/// The first rule the id breaks.
pub fn check_provenance_id(id: &str) -> Result<(), ProvenanceIdError> {
    let bytes = id.as_bytes();
    if bytes.is_empty() {
        return Err(ProvenanceIdError::Empty);
    }
    if bytes.len() > MAX_PROVENANCE_ID_BYTES {
        return Err(ProvenanceIdError::TooLong(bytes.len()));
    }
    if let Some(at) = bytes.iter().position(|&b| !(b'!'..=b'~').contains(&b)) {
        return Err(ProvenanceIdError::BadCharacter(at));
    }
    let Some(colon) = bytes.iter().position(|&b| b == b':') else {
        return Err(ProvenanceIdError::BadPrefix);
    };
    let prefix = &bytes[..colon];
    let prefix_ok = prefix.first().is_some_and(u8::is_ascii_lowercase)
        && prefix
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    if !prefix_ok {
        return Err(ProvenanceIdError::BadPrefix);
    }
    if colon + 1 == bytes.len() {
        return Err(ProvenanceIdError::EmptyBody);
    }
    Ok(())
}

/// An opaque provenance id for `extends` (decision 5): stored, round-tripped and syntax-checked,
/// never resolved, merged or emitted. Like [`Identifier`], the model can hold a malformed one so
/// the validator reports it (`invalid-provenance-id`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProvenanceId(String);

impl ProvenanceId {
    /// A checked provenance id ([`check_provenance_id`]).
    ///
    /// # Errors
    ///
    /// The first rule `id` breaks.
    pub fn new(id: &str) -> Result<Self, ProvenanceIdError> {
        check_provenance_id(id)?;
        Ok(Self(id.to_owned()))
    }

    /// An id taken as written, unchecked (the wire parser's path).
    #[must_use]
    pub fn unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Judge the id ([`check_provenance_id`]).
    ///
    /// # Errors
    ///
    /// The first rule the id breaks.
    pub fn check(&self) -> Result<(), ProvenanceIdError> {
        check_provenance_id(&self.0)
    }
}

impl fmt::Display for ProvenanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
