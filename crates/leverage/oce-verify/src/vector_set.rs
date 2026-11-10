//! A loaded vector set: the strict bounded load, the canonical form and `spec_id` (vectors note,
//! "The vector format"; decisions 1, 2 and 16).
//!
//! A load runs five stages, and the first refusal wins:
//!
//! 1. **Bytes.** The length against [`VerifyLimits::max_bytes`], before a byte is examined; then
//!    UTF-8; then no byte-order mark.
//! 2. **Strict JSON.** One pass over the whole text: syntax, the three profile bounds, repeated
//!    keys, control characters, non-finite numbers and integer tokens outside `i64`, at the
//!    first such place in document order.
//! 3. **The tag.** A root `schema` string other than [`crate::VECTORS_SCHEMA`] is
//!    `unknown-schema`, whatever else the document holds: a foreign or older document is refused
//!    by name before its shape is judged.
//! 4. **Shape.** The closed objects, the count bounds and the time integer range, at the first
//!    such place in document order.
//! 5. **Rules.** The clock, step lists, windows and instants, in the order
//!    `validate` documents.
//!
//! Nothing here opens a file, loads an engine or resolves a name.

use crate::canonical;
use crate::clock::Clock;
use crate::decode;
use crate::error::{VectorError, VectorErrorKind};
use crate::fnv;
use crate::limits::VerifyLimits;
use crate::model::{Parameter, Scenario};
use crate::strict;
use crate::validate;

/// One vector set in the `oce/vectors/v1` format: a clock, optional set-level parameter edits
/// and scenarios.
///
/// Built only by [`VectorSet::from_json_bytes`], so every rule of the format holds for a value
/// of this type. Two sets are equal when they write the same canonical document: the kind each
/// number was written as is part of the set (`60` is not `60.0`), and so is the difference
/// between an absent `params` and an empty one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VectorSet {
    clock: Clock,
    params: Option<Vec<Parameter>>,
    scenarios: Vec<Scenario>,
}

impl VectorSet {
    /// Read one vector set from `bytes` under `limits`. Strict and bounded; it opens nothing and
    /// never panics. A document need not be in the canonical form to load.
    ///
    /// Memory and time are bounded by the byte bound: the reader holds the document's values
    /// once, materialises nothing per tick, and stops at the first refusal.
    ///
    /// # Errors
    /// A [`VectorError`] whose [`VectorError::code`] is one of
    /// [`crate::VECTOR_ERROR_CODES`], from the first failing stage in the order the module
    /// documentation gives.
    pub fn from_json_bytes(bytes: &[u8], limits: &VerifyLimits) -> Result<Self, VectorError> {
        let limits = limits.effective();
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if length > limits.max_bytes {
            return Err(VectorError::on(
                VectorErrorKind::InputTooLarge,
                "document",
                format!(
                    "the document is {length} bytes; at most {} are allowed",
                    limits.max_bytes
                ),
            ));
        }
        let text = std::str::from_utf8(bytes).map_err(|error| {
            VectorError::on(
                VectorErrorKind::NotUtf8,
                "document",
                format!("byte {} is not UTF-8", error.valid_up_to()),
            )
        })?;
        if text.starts_with('\u{feff}') {
            return Err(VectorError::on(
                VectorErrorKind::ByteOrderMark,
                "document",
                "the document starts with a byte-order mark",
            ));
        }
        if let Some(tag) = strict::scan(text)?
            && tag != crate::VECTORS_SCHEMA
        {
            return Err(unknown_schema(&tag));
        }
        let written = decode::decode(text, &limits)?;
        if written.schema != crate::VECTORS_SCHEMA {
            return Err(unknown_schema(&written.schema));
        }
        validate::vector_set(written, &limits)
    }

    /// Put together a set whose parts `validate` has checked.
    pub(crate) fn assemble(
        clock: Clock,
        params: Option<Vec<Parameter>>,
        scenarios: Vec<Scenario>,
    ) -> Self {
        Self {
            clock,
            params,
            scenarios,
        }
    }

    /// The set's clock.
    #[must_use]
    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The set-level parameter edits, sorted by path bytes, as written and unresolved; `None`
    /// when the set has no `params` key (an empty map is `Some` and empty).
    #[must_use]
    pub fn params(&self) -> Option<&[Parameter]> {
        self.params.as_deref()
    }

    /// The scenarios, in file order; no name repeats. May be empty.
    #[must_use]
    pub fn scenarios(&self) -> &[Scenario] {
        &self.scenarios
    }

    /// The canonical document of this set: keys sorted by their UTF-8 bytes at every depth,
    /// two-space indentation, line feeds, one final line feed, ASCII only, an integer token as a
    /// decimal integer and a real in the one canonical spelling. Deterministic, and a fixed
    /// point: loading the result and writing it again gives the same bytes.
    ///
    /// The result loads again under the limits this set loaded under provided it is itself
    /// within them: the canonical spelling of a document can be longer than the document (a
    /// character outside ASCII is six bytes per UTF-16 unit, and a compact document gains its
    /// indentation), so a string near [`crate::MAX_STRING_BYTES`] or a document near the byte
    /// bound may canonicalise past it.
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        canonical::vector_set(&self.clock, self.params(), &self.scenarios)
    }

    /// The identity of this set: `spec:fnv1a128:` and the 32 lowercase hex digits of the
    /// FNV-1a-128 hash of [`Self::to_canonical_json`]. Two documents that differ only in
    /// spelling (key order, whitespace, escapes) have one id; it is an identity, not a
    /// cryptographic digest.
    #[must_use]
    pub fn spec_id(&self) -> String {
        fnv::tagged("spec:", self.to_canonical_json().as_bytes())
    }
}

fn unknown_schema(tag: &str) -> VectorError {
    VectorError::on(
        VectorErrorKind::UnknownSchema,
        "schema",
        format!(
            "the schema is `{tag}`; this reader reads `{}` only",
            crate::VECTORS_SCHEMA
        ),
    )
}
