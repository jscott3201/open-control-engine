//! A loaded commissioning spec: the bounded load, the findings, the canonical form and
//! `spec_id` (commissioning note, "Test-spec file"; decisions 3, 18 and 20).
//!
//! A load runs five stages, and the first refusal wins:
//!
//! 1. **Bytes.** The length against [`SpecLimits::max_bytes`], before a byte is examined; then
//!    UTF-8; then no byte-order mark.
//! 2. **Comments.** `//` and `/* */` outside strings are blanked in place; a block comment that
//!    is never closed is `unterminated-comment`.
//! 3. **Strict JSON.** One pass over the whole text: syntax, the three profile bounds, repeated
//!    keys, control characters, non-finite numbers and integer tokens outside `i64`, at the
//!    first such place in document order.
//! 4. **The tag.** A root `schema` string other than [`crate::COMMISSIONING_SPEC_SCHEMA`] is
//!    `unknown-schema`, whatever else the document holds. A document with no `schema` is an LBL
//!    spec and is read as this format.
//! 5. **Shape and rules.** The closed objects, the count bounds and what each value must be, at
//!    the first such place in document order.
//!
//! Nothing here opens a file, loads an engine or resolves a name: a path is kept as a path, and
//! a key or a name is kept as written.

use oce_replay::TimeOrigin;

use super::canonical;
use super::decode;
use super::error::SpecError;
use super::fields::{self, Reader};
use super::limits::SpecLimits;
use super::model::SpecReference;
use super::tolerances::PartialTolerances;
use crate::canonical::Json;
use crate::error::Location;
use crate::fnv;
use crate::number::Number;
use crate::strict::Kind;

/// The code of the one finding a spec load reports: a field of OBC §12.7 that the spec gives
/// and the verifier does not act on (commissioning note, decision 18). The four Scenario 1
/// fields always are (`controller`, `modelJsonDirectory`, `generateJson`, `runController`), and
/// so is a reference's `controllerOutput` when the reference also gives `expected`, which is
/// then the expected-output source.
pub const LBL_FIELD_IGNORED: &str = "lbl-field-ignored";

/// Every finding code a spec load can report, in declaration order.
pub const SPEC_FINDING_CODES: &[&str] = &[LBL_FIELD_IGNORED];

/// Something a spec load noticed that is no refusal: the spec loaded, and a receipt will say
/// this about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    code: &'static str,
    at: String,
}

impl Finding {
    /// The finding's stable kebab-case code, one of [`SPEC_FINDING_CODES`].
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// The field the finding is about, as a path into the spec: `controller`,
    /// `references[1].runController`. Indexes count from zero in file order.
    #[must_use]
    pub fn at(&self) -> &str {
        &self.at
    }
}

/// One commissioning test spec in the `oce/commissioning-spec/v1` format: a strict-JSON superset
/// of OBC §12.7 Listing 12.1.
///
/// Built only by a load, so every rule of the format holds for a value of this type. Two specs
/// are equal when they write the same canonical document: the kind each number was written as is
/// part of the spec (`60` is not `60.0`), and so are the difference between an absent list and
/// an empty one and the values of the fields the verifier ignores. The order of an object's keys
/// is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommissioningSpec {
    pub(crate) time_origin: Option<TimeOrigin>,
    pub(crate) tolerances: Option<PartialTolerances>,
    pub(crate) sampling: Option<Number>,
    pub(crate) references: Vec<SpecReference>,
    /// `modelJsonDirectory` as written, when present: kept for the canonical form only.
    pub(crate) model_json_directory: Option<Json>,
    /// `controller` as written, when present: kept for the canonical form only.
    pub(crate) controller: Option<Json>,
}

impl CommissioningSpec {
    /// Read one spec from `bytes` under `limits`. Strict and bounded; it opens nothing and never
    /// panics. `//` and `/* */` comments outside strings are removed first, so OBC §12.7
    /// Listing 12.1 loads as published and so does a copy with comments added. A document need
    /// not be in the canonical form to load.
    ///
    /// Memory and time are bounded by the byte bound: the reader holds the document's values
    /// once, and stops at the first refusal.
    ///
    /// # Errors
    /// A [`SpecError`] whose [`SpecError::code`] is one of [`crate::SPEC_ERROR_CODES`], from the
    /// first failing stage in the order the module documentation gives.
    pub fn from_json_bytes(bytes: &[u8], limits: &SpecLimits) -> Result<Self, SpecError> {
        let limits = limits.effective();
        Self::from_text(fields::utf8(bytes, &limits)?, &limits)
    }

    /// [`Self::from_json_bytes`] for text that is already UTF-8.
    ///
    /// # Errors
    /// As [`Self::from_json_bytes`], without `not-utf8`.
    pub fn from_json_str(text: &str, limits: &SpecLimits) -> Result<Self, SpecError> {
        Self::from_text(text, &limits.effective())
    }

    fn from_text(text: &str, limits: &SpecLimits) -> Result<Self, SpecError> {
        let (stripped, tag) = fields::prepared(text, limits)?;
        if let Some(tag) = tag
            && tag != crate::COMMISSIONING_SPEC_SCHEMA
        {
            let refusal = decode::unknown_schema(&tag, schema_location(&stripped));
            return Err(refusal.into());
        }
        Ok(decode::document(&stripped, limits)?)
    }

    /// `time.origin`: the wall-clock instant the spec's model seconds count from, when it gives
    /// one. Trend times written as instants are converted against it; it never reaches the
    /// engine.
    #[must_use]
    pub fn time_origin(&self) -> Option<&TimeOrigin> {
        self.time_origin.as_ref()
    }

    /// The top-level `tolerances`: what every output is compared under before its reference's
    /// `outputs` keys are laid over it. `None` when the spec gives none.
    #[must_use]
    pub fn tolerances(&self) -> Option<&PartialTolerances> {
        self.tolerances.as_ref()
    }

    /// The top-level `sampling` in seconds, the default for every reference: finite and above
    /// zero. `None` when the spec gives none.
    #[must_use]
    pub fn sampling_s(&self) -> Option<f64> {
        self.sampling.map(Number::seconds)
    }

    /// The comparison sampling of `reference`, in seconds: its own `sampling`, else the spec's;
    /// `None` when neither is given, and both series are then compared at their own times.
    #[must_use]
    pub fn sampling_for(&self, reference: &SpecReference) -> Option<f64> {
        reference.sampling_s().or_else(|| self.sampling_s())
    }

    /// The references, in file order. May be empty.
    #[must_use]
    pub fn references(&self) -> &[SpecReference] {
        &self.references
    }

    /// Every finding of the load, in an order that does not depend on the file's key order: the
    /// top-level fields sorted by name, then each reference in index order with its fields
    /// sorted by name. The one code is [`LBL_FIELD_IGNORED`].
    #[must_use]
    pub fn findings(&self) -> Vec<Finding> {
        let mut all = self.top_level_findings();
        for index in 0..self.references.len() {
            all.extend(self.reference_findings(index));
        }
        all
    }

    /// The findings a receipt of the reference at `reference` carries: those of the top level,
    /// then that reference's, each sorted by field name. A `reference` past the end has only the
    /// top level's.
    #[must_use]
    pub fn findings_for(&self, reference: usize) -> Vec<Finding> {
        let mut all = self.top_level_findings();
        all.extend(self.reference_findings(reference));
        all
    }

    fn top_level_findings(&self) -> Vec<Finding> {
        // In byte order of the field names.
        [
            ("controller", self.controller.is_some()),
            ("modelJsonDirectory", self.model_json_directory.is_some()),
        ]
        .into_iter()
        .filter(|(_, present)| *present)
        .map(|(field, _)| Finding {
            code: LBL_FIELD_IGNORED,
            at: field.to_owned(),
        })
        .collect()
    }

    fn reference_findings(&self, index: usize) -> Vec<Finding> {
        let Some(reference) = self.references.get(index) else {
            return Vec::new();
        };
        // In byte order of the field names. `controllerOutput` is read when it is the only
        // expected-output source; beside an `expected` it is not, and is reported like the rest.
        let unread_output = reference.controller_output.is_some() && reference.expected.is_some();
        [
            ("controllerOutput", unread_output),
            ("generateJson", reference.generate_json.is_some()),
            ("runController", reference.run_controller.is_some()),
        ]
        .into_iter()
        .filter(|(_, present)| *present)
        .map(|(field, _)| Finding {
            code: LBL_FIELD_IGNORED,
            at: format!("references[{index}].{field}"),
        })
        .collect()
    }

    /// The canonical document of this spec: always with its `schema` tag, every object's keys
    /// sorted by their UTF-8 bytes at every depth, those of an `outputs` and an `indicators` map
    /// included; two-space indentation, line feeds, one final line feed, ASCII only, an
    /// integer token as a decimal integer and a real in the one canonical spelling. No comment
    /// survives. Deterministic, and a fixed point: loading the result and writing it again
    /// gives the same bytes, and the same spec.
    ///
    /// The result loads again under the limits this spec loaded under provided it is itself
    /// within them: the canonical spelling of a document can be longer than the document (a
    /// character outside ASCII is six bytes per UTF-16 unit, and a compact document gains its
    /// indentation), so a string near [`crate::MAX_STRING_BYTES`] or a document near the byte
    /// bound may canonicalise past it.
    #[must_use]
    pub fn to_canonical_json(&self) -> String {
        canonical::spec(self).to_document()
    }

    /// The identity of this spec: `spec:fnv1a128:` and the 32 lowercase hex digits of the
    /// FNV-1a-128 hash of [`Self::to_canonical_json`]. Two documents that differ only in
    /// spelling (comments, whitespace, escapes, the key order of any object, a missing `schema`)
    /// have one id; it is an identity, not a cryptographic digest.
    #[must_use]
    pub fn spec_id(&self) -> String {
        fnv::tagged("spec:", self.to_canonical_json().as_bytes())
    }
}

/// Where the root object's `schema` value is written; the start of the text when the root has
/// none, which a caller that found a tag there never sees.
fn schema_location(text: &str) -> Location {
    let start = Location::new(1, 1);
    let mut cursor = Reader::reporting(text);
    if !matches!(cursor.peek(), Ok((Kind::Object, _))) {
        return start;
    }
    let Ok(mut object) = cursor.open_object() else {
        return start;
    };
    while let Ok(Some((key, _))) = cursor.next_key(&mut object) {
        if key == "schema" {
            return cursor.peek().map_or(start, |(_, at)| at);
        }
        if cursor.skip_value().is_err() {
            break;
        }
    }
    start
}
