//! Why an import was refused (importer note, "`ImportError`"; decision 16).
//!
//! An [`ImportError`] is the source form, the subject the refusal sits on (a node `@id` in CXF, a
//! JSON path such as `blocks[2].params[0]` in v0 graph JSON, or `document`) and one closed
//! [`ImportErrorKind`] with a stable kebab-case [`ImportErrorKind::code`]. The out-of-subset kinds
//! map one to one onto the authoring note's out-of-scope list, so a test or an agent branches on
//! the code, never on a message. [`IMPORT_ERROR_CODES`] lists every code in declaration order and
//! an in-crate test pins it.

use std::fmt;

/// The source form an import read.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceForm {
    /// One flat, single-root CXF document ([`crate::import_cxf`]).
    Cxf,
    /// One document in the frozen prototype's compact block-graph JSON, v0
    /// ([`crate::import_graph_v0`]).
    GraphV0,
}

impl SourceForm {
    /// The form's name for messages: `CXF` or `v0 graph JSON`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cxf => "CXF",
            Self::GraphV0 => "v0 graph JSON",
        }
    }
}

/// Why the strict JSON reader refused the bytes.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrictJson {
    /// The bytes are not UTF-8; carries the offset of the first bad byte.
    NotUtf8 {
        /// Byte offset.
        offset: usize,
    },
    /// An object writes one key twice (`serde_json` would keep the last value).
    DuplicateKey {
        /// The repeated key.
        key: String,
    },
    /// An integer token (no fraction or exponent) outside the `i64` range (`serde_json` would
    /// keep a `u64` or read a longer token as a float).
    IntegerOutOfRange {
        /// The token as written.
        token: String,
    },
    /// Anything else `serde_json` refuses: broken syntax, a number that overflows binary64, or
    /// nesting past its recursion limit (128).
    Syntax {
        /// What the parser found.
        detail: String,
    },
}

impl fmt::Display for StrictJson {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUtf8 { offset } => write!(f, "byte {offset} is not UTF-8"),
            Self::DuplicateKey { key } => write!(f, "the key `{key}` is written twice"),
            Self::IntegerOutOfRange { token } => {
                write!(f, "the integer {token} is outside the i64 range")
            }
            Self::Syntax { detail } => f.write_str(detail),
        }
    }
}

/// Which v0 read bound was exceeded (the wire form's bounds, [`crate::MAX_WIRE_DEPTH`],
/// [`crate::MAX_WIRE_LIST_LEN`] and [`crate::MAX_WIRE_STRING_BYTES`]).
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    /// Objects and arrays nest deeper than `limit` levels.
    Depth {
        /// The limit.
        limit: usize,
    },
    /// A list has more than `limit` entries.
    ListLength {
        /// The limit.
        limit: usize,
    },
    /// A string is longer than `limit` bytes (UTF-8, after unescaping).
    StringLength {
        /// The limit.
        limit: usize,
    },
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Depth { limit } => write!(f, "nesting deeper than {limit} levels"),
            Self::ListLength { limit } => write!(f, "a list longer than {limit} entries"),
            Self::StringLength { limit } => write!(f, "a string longer than {limit} bytes"),
        }
    }
}

/// What an import refused. Each kind has one stable code ([`ImportErrorKind::code`]);
/// `#[non_exhaustive]` per crate policy, so consumers branch on the code.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportErrorKind {
    /// Above [`oce_api::MAX_CXF_BYTES`] (CXF) or [`crate::MAX_WIRE_BYTES`] (v0), checked before
    /// parsing.
    TooLarge {
        /// The document size in bytes.
        bytes: usize,
        /// The limit.
        limit: usize,
    },
    /// The strict reader refused the bytes.
    NotStrictJson(StrictJson),
    /// A v0 depth, list length or string length bound.
    LimitExceeded(Limit),
    /// A property, top-level key or `@context` term the form does not define.
    UnknownKey {
        /// The key.
        key: String,
    },
    /// A required key is absent (a v0 key, a value-less parameter, a node without `@id`).
    MissingKey {
        /// The key.
        key: String,
    },
    /// A value of the wrong JSON shape: a relation that is not references, a label that is not a
    /// string, a v0 value that is null, an object or an array.
    WrongShape {
        /// The key whose value has the wrong shape (`document` for the top level).
        key: String,
    },
    /// No node contains blocks.
    NoRoot,
    /// A node `@id` is declared twice.
    DuplicateId,
    /// A reference to no node, or to a node outside the root's IRI.
    Unresolved {
        /// The IRI that does not resolve.
        reference: String,
    },
    /// A node no root relation reaches, or one reached twice.
    StrayNode,
    /// A root `@id` or a local part that breaks the identifier syntax (a dot, a non-ASCII byte,
    /// more than 64 bytes), or a member path deeper than `block.member`.
    InvalidName,
    /// A second node with `containsBlock`.
    NestedComposite,
    /// `hasInstance`.
    InstanceMember,
    /// `hasConstant`.
    ConstantMember,
    /// `isConditionalComponent` or `conditionalExpression`.
    ConditionalComponent,
    /// An array boundary or port.
    ArrayConnector,
    /// `isArray` on a root parameter.
    ArraySequenceParameter,
    /// A value that is an expression, or a bare name that is not a sequence parameter.
    ExpressionValue,
    /// `unit`, `quantity`, `displayUnit`, `min`, `max` or `nominal` on a block port.
    PortAttribute {
        /// The attribute, as written (`S231:max`).
        attribute: String,
    },
    /// `nominal` or `unbounded` on a boundary.
    ConnectorAttribute {
        /// The attribute, as written (`S231:nominal`).
        attribute: String,
    },
    /// `isConnectedTo` on a boundary output or a block input.
    DrivingSink,
    /// A block `@type` not under `Buildings.Controls.OBC.`.
    ClassOutsideObc,
    /// A value or bound whose shape disagrees with its declared type, or a connector whose `@type`
    /// and `isOfDataType` disagree.
    DatatypeMismatch,
    /// An `isOfDataType` (or a v0 `type`) that is no supported scalar and no named type.
    UnknownDatatype,
    /// A member path whose type the facade table lacks.
    UnknownMemberType,
    /// A non-finite or unparsable number, an unknown XSD datatype, a bad string escape.
    MalformedLiteral,
    /// Array sizes that disagree with the value, no catalog template, a rank the catalog does not
    /// have.
    ArrayShape,
    /// A v0 string parameter value.
    StringValue,
    /// A v0 connection without exactly one `->`.
    MalformedConnection,
}

/// Every [`ImportErrorKind::code`], in declaration order.
pub const IMPORT_ERROR_CODES: &[&str] = &[
    "too-large",
    "not-strict-json",
    "limit-exceeded",
    "unknown-key",
    "missing-key",
    "wrong-shape",
    "no-root",
    "duplicate-id",
    "unresolved-reference",
    "stray-node",
    "invalid-name",
    "nested-composite",
    "instance-member",
    "constant-member",
    "conditional-component",
    "array-connector",
    "array-sequence-parameter",
    "expression-value",
    "port-attribute",
    "connector-attribute",
    "driving-sink",
    "class-outside-obc",
    "datatype-mismatch",
    "unknown-datatype",
    "unknown-member-type",
    "malformed-literal",
    "array-shape",
    "string-value",
    "malformed-connection",
];

impl ImportErrorKind {
    /// The stable kebab-case code, for reports (one of [`IMPORT_ERROR_CODES`]).
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::TooLarge { .. } => "too-large",
            Self::NotStrictJson(_) => "not-strict-json",
            Self::LimitExceeded(_) => "limit-exceeded",
            Self::UnknownKey { .. } => "unknown-key",
            Self::MissingKey { .. } => "missing-key",
            Self::WrongShape { .. } => "wrong-shape",
            Self::NoRoot => "no-root",
            Self::DuplicateId => "duplicate-id",
            Self::Unresolved { .. } => "unresolved-reference",
            Self::StrayNode => "stray-node",
            Self::InvalidName => "invalid-name",
            Self::NestedComposite => "nested-composite",
            Self::InstanceMember => "instance-member",
            Self::ConstantMember => "constant-member",
            Self::ConditionalComponent => "conditional-component",
            Self::ArrayConnector => "array-connector",
            Self::ArraySequenceParameter => "array-sequence-parameter",
            Self::ExpressionValue => "expression-value",
            Self::PortAttribute { .. } => "port-attribute",
            Self::ConnectorAttribute { .. } => "connector-attribute",
            Self::DrivingSink => "driving-sink",
            Self::ClassOutsideObc => "class-outside-obc",
            Self::DatatypeMismatch => "datatype-mismatch",
            Self::UnknownDatatype => "unknown-datatype",
            Self::UnknownMemberType => "unknown-member-type",
            Self::MalformedLiteral => "malformed-literal",
            Self::ArrayShape => "array-shape",
            Self::StringValue => "string-value",
            Self::MalformedConnection => "malformed-connection",
        }
    }
}

impl fmt::Display for ImportErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { bytes, limit } => {
                write!(f, "the document is {bytes} bytes; at most {limit} are read")
            }
            Self::NotStrictJson(why) => write!(f, "not strict JSON: {why}"),
            Self::LimitExceeded(limit) => write!(f, "over a read bound: {limit}"),
            Self::UnknownKey { key } => write!(f, "`{key}` is not defined here"),
            Self::MissingKey { key } => write!(f, "`{key}` is required here"),
            Self::WrongShape { key } => write!(f, "`{key}` has the wrong shape"),
            Self::NoRoot => f.write_str("no node contains blocks; there is no root"),
            Self::DuplicateId => f.write_str("the `@id` is declared twice"),
            Self::Unresolved { reference } => {
                write!(f, "`{reference}` names no node under the root")
            }
            Self::StrayNode => f.write_str(
                "the node is reached by no root relation, or by more than one reference",
            ),
            Self::InvalidName => f.write_str(
                "the name breaks the identifier syntax, or the path is deeper than `block.member`",
            ),
            Self::NestedComposite => {
                f.write_str("a second composite (nested composites are outside the subset)")
            }
            Self::InstanceMember => f.write_str("`hasInstance` is outside the subset"),
            Self::ConstantMember => f.write_str("`hasConstant` is outside the subset"),
            Self::ConditionalComponent => {
                f.write_str("a conditional component is outside the subset")
            }
            Self::ArrayConnector => f.write_str("an array connector is outside the subset"),
            Self::ArraySequenceParameter => {
                f.write_str("an array sequence parameter is outside the subset")
            }
            Self::ExpressionValue => f.write_str(
                "the value is an expression, or names no sequence parameter (outside the subset)",
            ),
            Self::PortAttribute { attribute } => write!(
                f,
                "`{attribute}` on a block port is not authorable (outside the subset)"
            ),
            Self::ConnectorAttribute { attribute } => {
                write!(f, "`{attribute}` on a boundary is not carried")
            }
            Self::DrivingSink => {
                f.write_str("a boundary output or block input drives a connection")
            }
            Self::ClassOutsideObc => {
                f.write_str("the block class is outside `Buildings.Controls.OBC`")
            }
            Self::DatatypeMismatch => {
                f.write_str("the value or connector disagrees with its declared type")
            }
            Self::UnknownDatatype => {
                f.write_str("the declared type is no supported scalar and no named type")
            }
            Self::UnknownMemberType => f.write_str(
                "the member path names no enumeration or constant package the engine knows",
            ),
            Self::MalformedLiteral => f.write_str(
                "a non-finite or unparsable number, an unknown datatype or a bad string escape",
            ),
            Self::ArrayShape => {
                f.write_str("the array's sizes, rank or catalog template disagree with its value")
            }
            Self::StringValue => f.write_str("a v0 string parameter value is not carried"),
            Self::MalformedConnection => {
                f.write_str("the connection is not `source -> target` with exactly one `->`")
            }
        }
    }
}

/// Why an import was refused. Never a panic.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportError {
    /// The source form being read.
    pub source: SourceForm,
    /// Where the refusal sits: a node `@id` (CXF), a JSON path (v0), or `document`.
    pub subject: String,
    /// What was refused.
    pub kind: ImportErrorKind,
}

impl ImportError {
    /// A refusal of `kind` at `subject` while reading `source`.
    #[must_use]
    pub fn new(source: SourceForm, subject: impl Into<String>, kind: ImportErrorKind) -> Self {
        Self {
            source,
            subject: subject.into(),
            kind,
        }
    }

    /// The kind's stable code ([`ImportErrorKind::code`]).
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} import refused `{}` ({}): {}",
            self.source.as_str(),
            self.subject,
            self.kind.code(),
            self.kind
        )
    }
}

impl std::error::Error for ImportError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// One value of every kind, in declaration order. The match below is exhaustive inside the
    /// crate, so a new kind fails to compile here until it is listed.
    fn every_kind() -> Vec<ImportErrorKind> {
        let all = vec![
            ImportErrorKind::TooLarge { bytes: 1, limit: 0 },
            ImportErrorKind::NotStrictJson(StrictJson::Syntax {
                detail: String::new(),
            }),
            ImportErrorKind::LimitExceeded(Limit::Depth { limit: 8 }),
            ImportErrorKind::UnknownKey { key: String::new() },
            ImportErrorKind::MissingKey { key: String::new() },
            ImportErrorKind::WrongShape { key: String::new() },
            ImportErrorKind::NoRoot,
            ImportErrorKind::DuplicateId,
            ImportErrorKind::Unresolved {
                reference: String::new(),
            },
            ImportErrorKind::StrayNode,
            ImportErrorKind::InvalidName,
            ImportErrorKind::NestedComposite,
            ImportErrorKind::InstanceMember,
            ImportErrorKind::ConstantMember,
            ImportErrorKind::ConditionalComponent,
            ImportErrorKind::ArrayConnector,
            ImportErrorKind::ArraySequenceParameter,
            ImportErrorKind::ExpressionValue,
            ImportErrorKind::PortAttribute {
                attribute: String::new(),
            },
            ImportErrorKind::ConnectorAttribute {
                attribute: String::new(),
            },
            ImportErrorKind::DrivingSink,
            ImportErrorKind::ClassOutsideObc,
            ImportErrorKind::DatatypeMismatch,
            ImportErrorKind::UnknownDatatype,
            ImportErrorKind::UnknownMemberType,
            ImportErrorKind::MalformedLiteral,
            ImportErrorKind::ArrayShape,
            ImportErrorKind::StringValue,
            ImportErrorKind::MalformedConnection,
        ];
        for kind in &all {
            match kind {
                ImportErrorKind::TooLarge { .. }
                | ImportErrorKind::NotStrictJson(_)
                | ImportErrorKind::LimitExceeded(_)
                | ImportErrorKind::UnknownKey { .. }
                | ImportErrorKind::MissingKey { .. }
                | ImportErrorKind::WrongShape { .. }
                | ImportErrorKind::NoRoot
                | ImportErrorKind::DuplicateId
                | ImportErrorKind::Unresolved { .. }
                | ImportErrorKind::StrayNode
                | ImportErrorKind::InvalidName
                | ImportErrorKind::NestedComposite
                | ImportErrorKind::InstanceMember
                | ImportErrorKind::ConstantMember
                | ImportErrorKind::ConditionalComponent
                | ImportErrorKind::ArrayConnector
                | ImportErrorKind::ArraySequenceParameter
                | ImportErrorKind::ExpressionValue
                | ImportErrorKind::PortAttribute { .. }
                | ImportErrorKind::ConnectorAttribute { .. }
                | ImportErrorKind::DrivingSink
                | ImportErrorKind::ClassOutsideObc
                | ImportErrorKind::DatatypeMismatch
                | ImportErrorKind::UnknownDatatype
                | ImportErrorKind::UnknownMemberType
                | ImportErrorKind::MalformedLiteral
                | ImportErrorKind::ArrayShape
                | ImportErrorKind::StringValue
                | ImportErrorKind::MalformedConnection => {}
            }
        }
        all
    }

    #[test]
    fn the_import_error_code_list_is_pinned() {
        // The literal list is the contract: a renamed, removed or added code breaks this test.
        let pinned = [
            "too-large",
            "not-strict-json",
            "limit-exceeded",
            "unknown-key",
            "missing-key",
            "wrong-shape",
            "no-root",
            "duplicate-id",
            "unresolved-reference",
            "stray-node",
            "invalid-name",
            "nested-composite",
            "instance-member",
            "constant-member",
            "conditional-component",
            "array-connector",
            "array-sequence-parameter",
            "expression-value",
            "port-attribute",
            "connector-attribute",
            "driving-sink",
            "class-outside-obc",
            "datatype-mismatch",
            "unknown-datatype",
            "unknown-member-type",
            "malformed-literal",
            "array-shape",
            "string-value",
            "malformed-connection",
        ];
        let codes: Vec<&str> = every_kind().iter().map(ImportErrorKind::code).collect();
        assert_eq!(codes, pinned);
        assert_eq!(IMPORT_ERROR_CODES, pinned);
    }

    #[test]
    fn every_code_is_kebab_case_and_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for code in IMPORT_ERROR_CODES {
            assert!(
                code.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
                    && !code.starts_with('-')
                    && !code.ends_with('-'),
                "{code}"
            );
            assert!(seen.insert(*code), "{code} twice");
        }
    }

    #[test]
    fn the_display_names_the_form_the_subject_and_the_code() {
        let error = ImportError::new(
            SourceForm::GraphV0,
            "blocks[0]",
            ImportErrorKind::StringValue,
        );
        let text = error.to_string();
        assert!(
            text.starts_with("v0 graph JSON import refused `blocks[0]` (string-value)"),
            "{text}"
        );
        assert_eq!(error.code(), "string-value");
    }
}
