#![forbid(unsafe_code)]
//! `oce-author`: the authoring layer's library (authoring note `docs/design/authoring-layer.md`,
//! GitHub issue #463).
//!
//! A person or an agent writes a control sequence without touching JSON-LD. This crate holds:
//!
//! - the typed **model** ([`Sequence`] and its parts) — one flat, single-root sequence of
//!   parameters, boundary connectors, leaf blocks and connections, in authored order;
//! - a typed **builder** ([`SequenceBuilder`]) that refuses what it can see without the catalog;
//! - the strict-JSON **wire form** ([`from_wire_str`], [`to_wire_string`]) with closed objects,
//!   bounded reads and one canonical spelling, and its JSON **schema** ([`wire_schema`],
//!   [`WIRE_SCHEMA`]) derived from the wire DTOs and pinned byte for byte;
//! - the two-stage **validator** ([`validate()`], [`Finding`]): stage 1 judges the model against
//!   the catalog `oce-api` publishes ([`validate_author_side`]); stage 2 emits, loads the bytes in
//!   a throwaway engine and maps every engine diagnostic back onto the model;
//! - the **emitter** ([`emit()`], [`EmitOptions`]): one deterministic, canonical path from the model
//!   to flat CXF, in authored order, refusing any model stage 1 refuses;
//! - a minimal **importer** ([`import_cxf`]) from flat CXF back to the model, sized to the
//!   library round trip (gate item 1).
//!
//! The text form and the `oce` CLI are later items of the M2 milestone. The crate sits above the
//! facade: it depends on `oce-api` only, never evaluates an expression, and loads only throwaway
//! engines of its own (stage 2), never one a host runs.

mod builder;
mod emit;
mod identifier;
mod import;
mod json;
mod location;
mod model;
mod schema;
mod validate;
mod wire;

pub use builder::{BuildError, SequenceBuilder};
pub use emit::{CLASS_BASE, DEFAULT_NAMESPACE, EmitError, EmitOptions, MAX_NAMESPACE_BYTES, emit};
pub use identifier::{
    Identifier, IdentifierError, MAX_IDENTIFIER_BYTES, MAX_PROVENANCE_ID_BYTES,
    MODELICA_RESERVED_WORDS, ProvenanceId, ProvenanceIdError, TEXT_FORM_KEYWORDS,
    check_declared_name, check_identifier_syntax, check_provenance_id,
};
pub use import::{ImportError, import_cxf};
pub use location::Location;
pub use model::{
    Annotations, Assignment, Block, Boundary, Connection, Endpoint, EnumMember, Literal,
    ParamValue, SemanticPoint, Sequence, SequenceParameter, SignalType,
};
pub use schema::{SCHEMA_PROFILE_KEYWORDS, WIRE_SCHEMA, authorable_class_paths, wire_schema};
pub use validate::{
    CLASS_NOT_AUTHORABLE, CLASS_NOT_FOUND, DOCUMENT_TOO_LARGE, DUPLICATE_NAME, DUPLICATE_PARAMETER,
    EMPTY_SEQUENCE, Finding, INVALID_NAME, INVALID_PROVENANCE_ID, INVALID_SEMANTIC_POINT,
    LOAD_FAILED, MALFORMED_CONNECTION, MAX_SEMANTIC_FIELD_BYTES, MISSING_REQUIRED_PARAMETER,
    NON_FINITE_PARAMETER, Origin, PARAMETER_KIND_MISMATCH, PARAMETER_NOT_SETTABLE,
    PARAMETER_OUT_OF_RANGE, STAGE_ONE_CODES, Severity, Stage, UNKNOWN_ENUM_LITERAL,
    UNKNOWN_PARAMETER, UNKNOWN_PARAMETER_REFERENCE, UNKNOWN_PORT, UNKNOWN_SOURCE, UNKNOWN_TARGET,
    validate, validate_author_side,
};
pub use wire::{
    MAX_WIRE_BYTES, MAX_WIRE_DEPTH, MAX_WIRE_LIST_LEN, MAX_WIRE_STRING_BYTES, WireError,
    from_wire_bytes, from_wire_str, to_wire_string,
};
