//! The published JSON Schema of the vector format (vectors note decision 17).
//!
//! [`vectors_json_schema`] derives the schema from the reader's own shapes ([`crate::dto`]) with
//! `schemars` (draft 2020-12) and writes it with the canonical writer (keys sorted).
//! [`VECTORS_JSON_SCHEMA`] is the checked-in copy, `schema/vectors.schema.json`; a test
//! regenerates it and compares byte for byte, and only the `OCE_BLESS_VERIFY` switch rewrites it.
//!
//! The schema keeps to a strict profile: [`SCHEMA_PROFILE_KEYWORDS`] lists every keyword it
//! uses. Every object with `properties` is closed (`additionalProperties: false`); the two map
//! shapes (`inputs` and `params`) are objects whose `additionalProperties` is the schema of
//! their values; `anyOf` appears only over a value's shapes, an input's and an expectation's two
//! forms; and there is no `oneOf`, `allOf`, `const`, `pattern`, `format`, `default`, `minItems`
//! or `null`. An optional key is one that `required` leaves out.
//!
//! A JSON Schema cannot say everything the reader checks (the cadence, step order, a window's
//! ticks, the bounds, duplicate keys), so a document that satisfies the schema can still be
//! refused; a document the reader loads always satisfies the schema.

use schemars::generate::SchemaSettings;

use crate::canonical;
use crate::dto::VectorSetDto;

/// The checked-in schema of the vector format, `schema/vectors.schema.json`.
pub const VECTORS_JSON_SCHEMA: &str = include_str!("../schema/vectors.schema.json");

/// Every JSON Schema keyword the published schema may use. A test walks the schema and refuses
/// any other.
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

/// Derive the vector format's schema and write it canonically (sorted keys, two-space
/// indentation, one final line feed). Deterministic: the bytes depend only on the reader's
/// shapes and their documentation.
#[must_use]
pub fn vectors_json_schema() -> String {
    let schema = SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<VectorSetDto>();
    canonical::sorted(schema.as_value()).to_document()
}
