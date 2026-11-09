//! A compiled-schedule blob an earlier build wrote under the same format number (issue #509).
//!
//! Every format stays at revision 1 until 1.0.0 and changes in place, so the format field cannot
//! tell an older layout from the current one. `fixtures/blob_legacy.oceb` is the
//! `generic_air_economizer_high_limits_ashrae_differential` blob exactly as the v0.4.0 tag encoded
//! it: format 1, with the retired FNV-1a-128 export tag in the 32-byte content-id field where the
//! current layout carries the canonical content id's SHA-256 digest. It is never re-blessed. The
//! decoder has to refuse it by structure, length or CRC, with a typed error, and the execution
//! entry must not build an executable from it.

use oce_graph::blob::{BlobError, Executable, FORMAT_VERSION, decode};

const OLD_LAYOUT: &[u8] = include_bytes!("fixtures/blob_legacy.oceb");

#[test]
fn old_layout_fixture_carries_the_current_format_number() {
    // The precondition of the refusal test below: the version check alone cannot refuse it.
    assert_eq!(&OLD_LAYOUT[..4], b"OCEB");
    assert_eq!(
        u16::from_le_bytes([OLD_LAYOUT[4], OLD_LAYOUT[5]]),
        FORMAT_VERSION
    );
}

#[test]
fn old_layout_blob_sharing_the_format_number_is_refused_with_a_typed_error() {
    match decode(OLD_LAYOUT) {
        Err(BlobError::UnsupportedFormat { found, supported }) => panic!(
            "the format field no longer distinguishes the old layout, yet the refusal was \
             UnsupportedFormat {{ found: {found}, supported: {supported} }}"
        ),
        Err(_) => {}
        Ok(decoded) => panic!(
            "old-layout blob was ACCEPTED: {} blocks, {} inputs, {} outputs, content-id field {:02x?}",
            decoded.model.blocks.len(),
            decoded.inputs.len(),
            decoded.outputs.len(),
            &OLD_LAYOUT[20..52],
        ),
    }
}

#[test]
fn old_layout_blob_never_becomes_an_executable() {
    assert!(
        Executable::from_blob(OLD_LAYOUT).is_err(),
        "the execution entry built an executable from an old-layout blob"
    );
}
