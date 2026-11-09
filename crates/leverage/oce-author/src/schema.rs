//! The published JSON Schema of the wire form (decision 8).
//!
//! [`wire_schema`] derives the schema from the wire DTOs with `schemars` (draft 2020-12) and the
//! `class` enumeration from the non-reserved class paths of [`oce_api::catalog`], and writes it
//! with the canonical writer (keys sorted). [`WIRE_SCHEMA`] is the checked-in copy,
//! `schema/sequence.schema.json`; a test regenerates it and compares byte for byte, and only the
//! `OCE_BLESS_AUTHOR` switch rewrites it.
//!
//! The schema keeps to a strict profile that structured-output decoders accept: every object is
//! closed (`additionalProperties: false`) with every property required, `anyOf` appears only over
//! the value shapes, and there is no `oneOf`, `allOf`, `pattern` or `minItems`.
//! [`SCHEMA_PROFILE_KEYWORDS`] lists every keyword the profile allows.

use schemars::generate::SchemaSettings;
use schemars::{Schema, SchemaGenerator, json_schema};

use crate::json;
use crate::wire::WireSequence;

/// The checked-in wire-form schema, `schema/sequence.schema.json`.
pub const WIRE_SCHEMA: &str = include_str!("../schema/sequence.schema.json");

/// Every JSON Schema keyword the strict profile allows. A test walks the published schema and
/// refuses any other.
pub const SCHEMA_PROFILE_KEYWORDS: &[&str] = &[
    "$schema",
    "$defs",
    "$ref",
    "title",
    "description",
    "type",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "enum",
    "anyOf",
];

/// The non-reserved class paths of the catalog, in catalog order: the `class` enumeration.
#[must_use]
pub fn authorable_class_paths() -> Vec<&'static str> {
    oce_api::catalog()
        .iter()
        .filter(|entry| !entry.reserved)
        .map(|entry| entry.class_path)
        .collect()
}

/// The schema of a block's `class`: a string from the catalog's authorable class paths.
pub(crate) fn class_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "type": "string",
        "enum": authorable_class_paths(),
    })
}

/// Derive the wire-form schema and write it canonically (sorted keys, two-space indentation, one
/// trailing newline). Deterministic: the bytes depend only on the DTOs and the catalog.
#[must_use]
pub fn wire_schema() -> String {
    let schema = SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<WireSequence>();
    json::sorted(schema.as_value()).to_document()
}
