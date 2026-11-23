//! The load-time check that a model's durable state snapshot fits `MAX_SNAPSHOT_BYTES` (issue
//! #815, owner ruling of 2026-11-23): a model that loads can be snapshotted.
//!
//! A snapshot spells a block's IRI in every connector key that names the block (its ports, its
//! wires, the schedule's connector order and drivers) and a boundary input's IRI in every input it
//! drives, so a long identity a document writes once can make the snapshot far larger than the
//! document, and the canonical encoding does not count all of these (an unwired port, a boundary
//! input's targets). The load counts the snapshot the loaded model would write, with the codec's
//! own counting writer, and refuses the load past the cap.
//!
//! **What is counted, and what can grow after the load.** Of a snapshot's fields, the header, the
//! model id, the run state's words (their number is fixed by the model), every connector's value
//! (no connector carries text: no catalog port is a `String`) and the manifest's keys, connectors,
//! schedule and input definitions are fixed when the model loads. Two things can grow:
//! - the previous frame's time, absent before the first frame and 8 bytes after it, which the
//!   check counts as present;
//! - a `String` parameter's value, which the manifest carries, and which a host can lengthen with
//!   `Engine::set_param` and `Engine::resume`: those are the host's own bytes, and a snapshot
//!   after such an edit can still be refused (`SnapshotTooLarge`).
//!
//! So a model that loads can be snapshotted at any time, unless a host has since lengthened a
//! `String` parameter.
//!
//! **What the check costs.** It assembles the manifest the snapshot would encode, whose keys share
//! the model's allocations (a pointer each), sorts its records (two keys naming one block compare
//! by pointer, never by reading the block's IRI), counts its encoding without writing it, and
//! drops it. The run state's words are counted, not copied.

use oce_diag::{DiagCode, Diagnostic};
use oce_validate::ValidationError;

use crate::state::{
    EXECUTION_ABI_REVISION, EngineStateError, MAX_SNAPSHOT_BYTES, StateImage, WireValue,
};
use crate::state_manifest::{ManifestSource, assemble_manifest};

/// The bytes the previous frame's time takes once a frame has run: absent from a snapshot taken
/// before the first frame, present in every later one.
const PREVIOUS_TIME_BYTES: usize = 8;

/// The length of the durable snapshot of `source` under `model_id`, as `Engine::state_snapshot`
/// would write it before the first frame, past the cap too; `usize::MAX` for one whose counts
/// pass the format's 32-bit fields; or `None` for a model no durable snapshot can be taken of (a
/// block or connector with no authored identity, a value no snapshot carries).
pub(crate) fn counted_snapshot_bytes(source: &ManifestSource<'_>, model_id: &str) -> Option<usize> {
    match count(source, model_id) {
        Ok(length) => Some(length),
        Err(EngineStateError::SnapshotTooLarge { .. }) => Some(usize::MAX),
        Err(_) => None,
    }
}

fn count(source: &ManifestSource<'_>, model_id: &str) -> Result<usize, EngineStateError> {
    let assembled = assemble_manifest(source, true)?;
    let manifest_len = crate::state_manifest_codec::manifest_len(&assembled.manifest, false)?;
    let mut values = Vec::with_capacity(assembled.connector_keys_by_dense.len());
    for (dense, key) in assembled.connector_keys_by_dense.iter().enumerate() {
        let value =
            source
                .state
                .values
                .get(dense)
                .ok_or_else(|| EngineStateError::IneligibleModel {
                    subject: format!("connector#{dense}"),
                    detail: "connector has no run-state value".into(),
                })?;
        values.push((key.clone(), WireValue::from_value(value)?));
    }
    values.sort_by(|left, right| left.0.cmp(&right.0));
    let image = StateImage {
        execution_revision: EXECUTION_ABI_REVISION,
        fingerprint: 0,
        model_id: model_id.to_owned(),
        state_t: 0,
        prev_t: None,
        manifest: assembled.manifest,
        values,
        words: Vec::new(),
    };
    crate::state_codec::counted_snapshot_len(&image, false, manifest_len, source.state.words.len())
}

/// Refuse a model whose durable snapshot, once a frame has run, would take more than
/// `MAX_SNAPSHOT_BYTES`, as one `resource-limit-exceeded` error naming the length. A model no
/// durable snapshot can be taken of is not refused here: its snapshot is refused when asked for,
/// as before.
pub(crate) fn check_snapshot_fits(
    source: &ManifestSource<'_>,
    model_id: &str,
) -> Result<(), ValidationError> {
    let Some(length) = counted_snapshot_bytes(source, model_id) else {
        return Ok(());
    };
    let length = length.saturating_add(PREVIOUS_TIME_BYTES);
    if u64::try_from(length).unwrap_or(u64::MAX) <= MAX_SNAPSHOT_BYTES {
        return Ok(());
    }
    let length = if length == usize::MAX {
        "more than 4 GiB".to_owned()
    } else {
        format!("{length} bytes")
    };
    Err(ValidationError {
        diagnostics: vec![Diagnostic::error(
            DiagCode::ResourceLimitExceeded,
            format!(
                "the model's durable state snapshot would take {length} once a frame has \
                 run, more than the {MAX_SNAPSHOT_BYTES} bytes a snapshot may take (it spells a \
                 block's IRI in every connector key and a boundary input's IRI in every input it \
                 drives)"
            ),
        )],
    })
}

#[cfg(test)]
#[path = "snapshot_fit_tests.rs"]
mod tests;
