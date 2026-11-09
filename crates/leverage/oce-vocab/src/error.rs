//! Why a vocabulary file, or the pair of them, was refused.

use std::fmt;

/// Why vocabulary data was refused. Never a panic.
///
/// The first group is a single file read by [`crate::GeneratedVocabulary::from_json`] or
/// [`crate::CuratedVocabulary::from_json`]; the second is the pair joined by
/// [`crate::Vocabulary::from_parts`]; the third is the pair against the facade's catalog.
/// `path` fields spell where in the document the value sits (`classes[3].connectors[0].unit`).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VocabError {
    /// The document starts with a byte-order mark.
    ByteOrderMark,
    /// The document is larger than [`crate::MAX_VOCAB_BYTES`].
    TooLarge {
        /// The document size in bytes.
        bytes: usize,
        /// The limit.
        limit: usize,
    },
    /// The document is not JSON of the closed shape: a syntax error, an unknown, missing or
    /// repeated key, a `null`, a wrong type or an unknown enumerator.
    Json {
        /// One-based line of the refusal.
        line: usize,
        /// One-based column of the refusal.
        column: usize,
        /// What the parser found.
        detail: String,
    },
    /// `format` is not [`crate::VOCAB_FORMAT`].
    UnsupportedFormat {
        /// The format the document declares.
        format: u32,
    },
    /// A string is longer than [`crate::MAX_VOCAB_STRING_BYTES`].
    StringTooLong {
        /// Where the string is.
        path: String,
        /// Its length in bytes.
        bytes: usize,
        /// The limit.
        limit: usize,
    },
    /// A string carries a control character (C0, DEL or C1), a line break included.
    ControlCharacter {
        /// Where the string is.
        path: String,
    },
    /// A string starts or ends with white space.
    SurroundingWhitespace {
        /// Where the string is.
        path: String,
    },
    /// A string that must say something is empty: a class path, a name, a note's text or
    /// evidence, a curated description, a field of `source`.
    EmptyField {
        /// Where the string is.
        path: String,
    },
    /// A connector's `dims` is not 0 (scalar), 1 (vector) or 2 (matrix).
    InvalidDims {
        /// Where the value is.
        path: String,
        /// The value.
        dims: u8,
    },
    /// A commit or digest in `source` is not lowercase hexadecimal of the expected length.
    InvalidSource {
        /// The field (`buildings_commit`, `modelica_json_commit` or `sha256`).
        field: &'static str,
    },
    /// The document parses but is not the canonical spelling of what it says: a key out of
    /// order, different white space or escapes, a `\r\n` line ending, a missing final newline.
    NotCanonical {
        /// One-based line where the document first differs from its canonical rewrite; the
        /// last line when the document only lacks its final newline.
        line: usize,
    },

    /// A class path appears twice in one file.
    DuplicateClass {
        /// The class path.
        class: String,
    },
    /// A connector or parameter name appears twice in one generated class, or a curated class
    /// describes one subject twice.
    DuplicateSubject {
        /// The class path.
        class: String,
        /// The repeated name.
        subject: String,
    },
    /// `curated.json` names a class `generated.json` does not have.
    UnknownClass {
        /// The class path.
        class: String,
    },
    /// A curated description or note names a subject that is not a connector or parameter of
    /// its class (a note's class-level subject is the empty string).
    UnknownSubject {
        /// The class path.
        class: String,
        /// The subject as written.
        subject: String,
    },
    /// A curated description names a subject whose generated description is not empty. Curation
    /// is additive in v1 (design note decision 6): it never replaces generated text.
    CuratedReplacesGenerated {
        /// The class path.
        class: String,
        /// The connector or parameter name.
        subject: String,
    },
    /// A curated class is out of the generated (catalog) order.
    CuratedOutOfOrder {
        /// The first class path that is out of order.
        class: String,
    },
    /// A curated class carries neither a description nor a note.
    EmptyCuratedClass {
        /// The class path.
        class: String,
    },
    /// A connector or parameter has no description after the merge: the generated one is empty
    /// and `curated.json` supplies none.
    Undescribed {
        /// The class path.
        class: String,
        /// The connector or parameter name.
        subject: String,
    },

    /// The vocabulary names a class, a parameter or a named-class connector the facade's catalog
    /// does not have, or lists it out of the catalog's order (`subject` is empty for a class).
    NotInCatalog {
        /// The class path.
        class: String,
        /// The connector or parameter name, or empty for the class itself.
        subject: String,
    },
    /// The facade's catalog has an authorable class, a parameter or a named-class connector the
    /// vocabulary lacks (`subject` is empty for a class).
    MissingFromVocabulary {
        /// The class path.
        class: String,
        /// The connector or parameter name, or empty for the class itself.
        subject: String,
    },
}

impl fmt::Display for VocabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ByteOrderMark => write!(f, "the document starts with a byte-order mark"),
            Self::TooLarge { bytes, limit } => {
                write!(
                    f,
                    "the document is {bytes} bytes, over the {limit}-byte limit"
                )
            }
            Self::Json {
                line,
                column,
                detail,
            } => write!(f, "line {line}, column {column}: {detail}"),
            Self::UnsupportedFormat { format } => write!(f, "unsupported format {format}"),
            Self::StringTooLong { path, bytes, limit } => {
                write!(f, "{path} is {bytes} bytes, over the {limit}-byte limit")
            }
            Self::ControlCharacter { path } => write!(f, "{path} carries a control character"),
            Self::SurroundingWhitespace { path } => {
                write!(f, "{path} starts or ends with white space")
            }
            Self::EmptyField { path } => write!(f, "{path} is empty"),
            Self::InvalidDims { path, dims } => write!(f, "{path} is {dims}, not 0, 1 or 2"),
            Self::InvalidSource { field } => {
                write!(
                    f,
                    "source.{field} is not lowercase hexadecimal of the expected length"
                )
            }
            Self::NotCanonical { line } => write!(
                f,
                "line {line}: not the canonical spelling (sorted keys, two-space indent, LF)"
            ),
            Self::DuplicateClass { class } => write!(f, "class `{class}` appears twice"),
            Self::DuplicateSubject { class, subject } => {
                write!(f, "`{subject}` appears twice in `{class}`")
            }
            Self::UnknownClass { class } => {
                write!(f, "curated class `{class}` is not a generated class")
            }
            Self::UnknownSubject { class, subject } => write!(
                f,
                "`{subject}` is not a connector or parameter of `{class}`"
            ),
            Self::CuratedReplacesGenerated { class, subject } => write!(
                f,
                "curated description of `{class}` `{subject}` would replace a generated one"
            ),
            Self::CuratedOutOfOrder { class } => {
                write!(f, "curated class `{class}` is out of catalog order")
            }
            Self::EmptyCuratedClass { class } => {
                write!(f, "curated class `{class}` has no description and no note")
            }
            Self::Undescribed { class, subject } => {
                write!(
                    f,
                    "`{class}` `{subject}` has no description after the merge"
                )
            }
            Self::NotInCatalog { class, subject } => write!(
                f,
                "the catalog has no `{class}` `{subject}` where the vocabulary lists it"
            ),
            Self::MissingFromVocabulary { class, subject } => write!(
                f,
                "the vocabulary has no `{class}` `{subject}` where the catalog lists it"
            ),
        }
    }
}

impl std::error::Error for VocabError {}
