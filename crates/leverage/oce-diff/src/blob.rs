//! Same or different for two compiled-schedule blobs (diff note, "What `oce-diff` is, and is
//! not"; decisions 3, 8 and 15; issue #667).
//!
//! A blob's block ids are declaration-order numbers that renumber whenever order or topology
//! moves, so two blobs are never diffed structurally. [`compare_blobs`] reads each side through
//! the facade's `oce_api::inspect_blob`, the decoder's own reader (this crate parses no blob
//! byte), and compares what the two summaries record on three axes and nothing more: the content
//! id, the class table by class path and class signature hash, and the per-instance initial-state
//! CRCs. Beside them it says whether the bytes are equal and what this build would do with each
//! side.
//!
//! **Each axis is the blobs' own statement.** The content id is read from a header and never
//! recomputed, and a CRC-32C trailer is not tamper evidence, so equal ids are never taken as
//! proof of anything: the class table and the instances are compared whatever the ids say.
//!
//! **Compared is not runnable.** A blob whose summary reads may still be one this build refuses
//! to run (a class signature hash or an initial-state CRC that is not this build's, a parameter,
//! connector, connection or schedule fault); `oce_api::BlobSummary::decode_refusal` says so, and
//! each [`BlobSide`] carries it. So a comparison whose every axis is `same` says nothing about
//! whether either blob runs here: [`BlobSide::accepted`] does.

use std::collections::BTreeMap;

use oce_api::{BlobError, BlobSummary, ContentId, OcError, inspect_blob};
use serde::{Serialize, Serializer};

use crate::changes::{Entry, sort_section};
use crate::data::Side;
use crate::error::DiffError;
use crate::work::{Counted, visited};

/// What a comparison of two recorded facts found.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Equality {
    /// Both sides record the same thing.
    Same,
    /// Both sides record a value and the values differ.
    Different,
    /// The two cannot be compared: a side records no content id, or the block lists do not
    /// align by class.
    Unknown,
}

/// A change to the class table, by class path. Each entry is tagged by a stable kebab-case
/// `kind`; a hash is written as `0x` and 16 lowercase hex digits, never a JSON number.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ClassChange {
    /// A class only head's table holds.
    Added {
        /// The class path.
        subject: String,
        /// The class signature hash head records for it.
        #[serde(serialize_with = "hex64")]
        hash: u64,
    },
    /// A class only base's table holds.
    Removed {
        /// The class path.
        subject: String,
        /// The class signature hash base records for it.
        #[serde(serialize_with = "hex64")]
        hash: u64,
    },
    /// A class both tables hold under different class signature hashes. One engine version
    /// gives each class one hash, so one of the two blobs was written by a build whose class
    /// registry differs from the other's, and at most one of them is this build's.
    SignatureChanged {
        /// The class path.
        subject: String,
        /// The hash base records.
        #[serde(serialize_with = "hex64")]
        old: u64,
        /// The hash head records.
        #[serde(serialize_with = "hex64")]
        new: u64,
    },
}

impl ClassChange {
    /// The class path the entry is about.
    #[must_use]
    pub fn subject(&self) -> &str {
        match self {
            Self::Added { subject, .. }
            | Self::Removed { subject, .. }
            | Self::SignatureChanged { subject, .. } => subject,
        }
    }
}

impl Entry for ClassChange {
    fn entry_subject(&self) -> &str {
        self.subject()
    }

    fn is_reordering(&self) -> bool {
        false
    }
}

/// One side of a blob comparison: the content id its header records, and this build's verdict on
/// it.
///
/// Written as `{"accepted", "content_id"}`. The refusal itself is the facade's typed
/// [`BlobError`], kept in memory only, so a change to the facade's wording of a refusal moves no
/// golden.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobSide {
    /// The canonical content id the blob's header records (`BlobSummary::content_id`), read and
    /// never recomputed; `None` when the header records none (32 zero bytes).
    pub content_id: Option<ContentId>,
    /// This build's verdict on the blob (`BlobSummary::decode_refusal`): `None` when its full
    /// decoder accepts the bytes, or the decoder's own first refusal when it does not.
    pub decode_refusal: Option<BlobError>,
}

impl BlobSide {
    /// Whether this build accepts the blob: its decoder, the one the execution entry runs,
    /// returns no refusal for these bytes. Acceptance is of the bytes by this build; it says
    /// nothing about where they came from.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.decode_refusal.is_none()
    }
}

impl Serialize for BlobSide {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Written<'a> {
            accepted: bool,
            content_id: Option<&'a str>,
        }
        Written {
            accepted: self.accepted(),
            content_id: self.content_id.as_ref().map(ContentId::as_str),
        }
        .serialize(serializer)
    }
}

/// Two compiled-schedule blobs compared as same or different (diff note, decision 15).
///
/// Four facts about the pair and each side's own two: whether the bytes are equal; whether the
/// recorded content ids are; the class-table entries that differ, by class path; and whether the
/// per-instance initial-state CRCs are. Nothing else a blob holds is compared (no parameter,
/// connection, schedule or I/O path), and no entry is matched across the two by block id.
///
/// Written, never read back: [`BlobComparison::to_json`] writes it with sorted keys, and a
/// side's refusal is kept as `accepted` only.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BlobComparison {
    /// Whether the two inputs are the same bytes. When they are, every other axis is `Same`,
    /// except `content_id`, which is `Unknown` for a blob that records no id.
    pub same_bytes: bool,
    /// The recorded content ids: `Same` or `Different` when both blobs record one, `Unknown`
    /// when either records none. The headers' statement, never checked against the bodies.
    pub content_id: Equality,
    /// The class-table entries that differ, by class path: a class in one table only (`added`,
    /// `removed`) or under two hashes (`signature-changed`). Sorted by class path bytewise, at
    /// most one entry a class; empty when the tables are equal.
    pub classes: Vec<ClassChange>,
    /// The per-instance initial state. `Unknown` unless the block lists align by class: the
    /// same number of instances, and the same class path at each position in block id order.
    /// When they align, `Same` when every position records the same state word count and the
    /// same initial-state CRC-32C, and `Different` otherwise.
    pub initial_state: Equality,
    /// The base side.
    pub base: BlobSide,
    /// The head side.
    pub head: BlobSide,
}

impl BlobComparison {
    /// The comparison as JSON text: keys sorted at every depth, two-space indentation, one final
    /// line feed, as [`crate::Diff::to_json`] writes. The same data always gives the same bytes,
    /// on every platform. Never panics.
    #[must_use]
    pub fn to_json(&self) -> String {
        crate::json::canonical(self)
    }
}

/// Compare two compiled-schedule blobs as same or different.
///
/// Each side is read through `oce_api::inspect_blob`, base first, and the two summaries are
/// compared on three axes, each on its own and in full: [`BlobComparison::content_id`],
/// [`BlobComparison::classes`] and [`BlobComparison::initial_state`].
/// [`BlobComparison::same_bytes`] compares the inputs themselves.
///
/// **Same version, same registry.** `inspect_blob` reads only a blob this engine version wrote
/// (a blob is accepted only by the version that wrote it) whose every class this build
/// registers, so two blobs compare only under those two conditions; anything else is a
/// [`DiffError::UnreadableBlob`].
///
/// **A blob this build would refuse to run is still compared.** Its summary reads, and its side
/// carries the decoder's refusal ([`BlobSide::decode_refusal`]), so "same" on every axis is
/// never "runnable": read [`BlobSide::accepted`] for that. A blob from another build of the same
/// engine version whose registry differs is how a `signature-changed` class arises, and it is
/// refused here with `BlobError::ClassSignatureMismatch`.
///
/// # Errors
///
/// [`DiffError::UnreadableBlob`] carrying the side and the facade's typed refusal when
/// `inspect_blob` reads no summary from a side's bytes (truncated, corrupted or empty bytes,
/// another engine version's blob, an unregistered class). Base is read before head, so when both
/// are refused the error names base. [`DiffError::UnexpectedRefusal`] if the facade refuses with
/// an error that is not a blob refusal, which it documents it never does. Hostile bytes are
/// refused, never a panic: the facade's reader is the only parser.
///
/// # Cost
///
/// Two calls of `inspect_blob`, each of which costs what the decoder's building pass costs for
/// its bytes before the summary is built: the state the blocks declare (at most the engine's
/// state budget, 32 MiB) and what the block constructors allocate from the blob's parameters,
/// which is capped per block but **not across the model, so not bounded by the blob length**
/// until the model-wide budget of GitHub #700 lands. Do not compare untrusted bytes without your
/// own memory limit (`docs/compiled-schedule-blob.md`, "What a summary costs"). The comparison
/// itself is linear in the instances and in the class tables, which hold only registered classes.
pub fn compare_blobs(base: &[u8], head: &[u8]) -> Result<BlobComparison, DiffError> {
    let base_summary = summary(base, Side::Base)?;
    let head_summary = summary(head, Side::Head)?;
    let base_facts = Facts::of(&base_summary);
    let head_facts = Facts::of(&head_summary);
    Ok(BlobComparison {
        same_bytes: base == head,
        content_id: content_id(base_facts.content_id, head_facts.content_id),
        classes: class_changes(&base_facts.classes, &head_facts.classes),
        initial_state: initial_state(&base_facts.instances, &head_facts.instances),
        base: side(&base_summary),
        head: side(&head_summary),
    })
}

/// Read one side's summary, or say why the facade read none.
fn summary(bytes: &[u8], side: Side) -> Result<BlobSummary, DiffError> {
    inspect_blob(bytes).map_err(|error| refused(side, error))
}

/// The refusal of a side whose summary the facade did not read.
fn refused(side: Side, error: OcError) -> DiffError {
    match error {
        OcError::Blob(refusal) => DiffError::UnreadableBlob { side, refusal },
        other => DiffError::UnexpectedRefusal {
            side,
            message: other.to_string(),
        },
    }
}

fn side(summary: &BlobSummary) -> BlobSide {
    BlobSide {
        content_id: summary.content_id.clone(),
        decode_refusal: summary.decode_refusal,
    }
}

/// What the comparison reads of one summary, and nothing more.
pub(crate) struct Facts<'a> {
    /// The recorded content id.
    pub(crate) content_id: Option<&'a ContentId>,
    /// The class table: class path and recorded signature hash, in table order.
    pub(crate) classes: Vec<(&'a str, u64)>,
    /// The instances, in block id order.
    pub(crate) instances: Vec<Instance<'a>>,
}

/// What the comparison reads of one instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Instance<'a> {
    /// The instance's class path; `None` when its class index is outside the class table, which
    /// the facade's summary never holds.
    pub(crate) class: Option<&'a str>,
    /// The recorded state word count.
    pub(crate) state_words: u32,
    /// The recorded CRC-32C of the initial state words.
    pub(crate) initial_state_crc: u32,
}

impl<'a> Facts<'a> {
    fn of(summary: &'a BlobSummary) -> Self {
        let classes: Vec<(&str, u64)> = summary
            .classes
            .iter()
            .map(|class| (class.class_path.as_str(), class.signature_hash))
            .collect();
        let instances = summary
            .instances
            .iter()
            .map(|instance| Instance {
                class: usize::try_from(instance.class)
                    .ok()
                    .and_then(|index| classes.get(index))
                    .map(|&(path, _)| path),
                state_words: instance.state_words,
                initial_state_crc: instance.initial_state_crc,
            })
            .collect();
        Self {
            content_id: summary.content_id.as_ref(),
            classes,
            instances,
        }
    }
}

/// Compare two recorded content ids.
pub(crate) fn content_id(base: Option<&ContentId>, head: Option<&ContentId>) -> Equality {
    match (base, head) {
        (Some(base), Some(head)) if base == head => Equality::Same,
        (Some(_), Some(_)) => Equality::Different,
        _ => Equality::Unknown,
    }
}

/// The class-table entries that differ, by class path, sorted by class path bytewise.
pub(crate) fn class_changes<'a>(
    base: &[(&'a str, u64)],
    head: &[(&'a str, u64)],
) -> Vec<ClassChange> {
    // A summary's class table is strictly increasing by path (the facade's invariant), so each
    // path is one key here.
    let table = |classes: &[(&'a str, u64)]| -> BTreeMap<Counted<&'a str>, u64> {
        classes
            .iter()
            .map(|&(path, hash)| (Counted(path), hash))
            .collect()
    };
    let (base_table, head_table) = (table(base), table(head));
    visited(base.len() + head.len());
    let mut changes = Vec::new();
    for (&Counted(path), &old) in &base_table {
        match head_table.get(&Counted(path)) {
            None => changes.push(ClassChange::Removed {
                subject: path.to_owned(),
                hash: old,
            }),
            Some(&new) if new != old => changes.push(ClassChange::SignatureChanged {
                subject: path.to_owned(),
                old,
                new,
            }),
            Some(_) => {}
        }
    }
    for (&Counted(path), &hash) in &head_table {
        if !base_table.contains_key(&Counted(path)) {
            changes.push(ClassChange::Added {
                subject: path.to_owned(),
                hash,
            });
        }
    }
    sort_section(&mut changes);
    changes
}

/// Compare the initial state of two block lists: `Unknown` unless they align by class, then
/// `Same` or `Different` by each position's state word count and initial-state CRC.
pub(crate) fn initial_state(base: &[Instance<'_>], head: &[Instance<'_>]) -> Equality {
    if base.len() != head.len() {
        return Equality::Unknown;
    }
    visited(base.len());
    let mut same = true;
    for (base, head) in base.iter().zip(head) {
        match (base.class, head.class) {
            (Some(base_class), Some(head_class)) if base_class == head_class => {}
            _ => return Equality::Unknown,
        }
        // Two CRCs say the same state only of words of one length.
        if base.state_words != head.state_words || base.initial_state_crc != head.initial_state_crc
        {
            same = false;
        }
    }
    if same {
        Equality::Same
    } else {
        Equality::Different
    }
}

/// Write a 64-bit hash as `0x` and 16 lowercase hex digits.
fn hex64<S: Serializer>(hash: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("0x{hash:016x}"))
}

#[cfg(test)]
#[path = "blob_tests.rs"]
mod tests;
