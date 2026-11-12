//! A state snapshot as bytes, for tests that read or edit one the way a host could.
//!
//! Nothing here calls the codec: the header offsets, FNV-1a-128 and the execution fingerprint are
//! written out from `docs/state-compatibility.md`, so a test built on them checks the decoder
//! against the documented layout and not against itself.
//!
//! The capture recipe the fixtures share is here too ([`delay_after_two_frames`]).

#![allow(dead_code)]

use std::ops::Range;

use oce_api::{Engine, EngineStateSnapshot, EngineVersion, OcError, Value};

/// The format-revision field, a little-endian `u32`.
pub(crate) const FORMAT_FIELD: Range<usize> = 8..12;
/// The execution-state ABI revision field, a little-endian `u32`.
pub(crate) const ABI_FIELD: Range<usize> = 12..16;
/// The engine-version field: major, minor and patch, each a little-endian `u16`.
pub(crate) const VERSION_FIELD: Range<usize> = 16..22;
/// The body-length field, a little-endian `u64`.
pub(crate) const BODY_LENGTH_FIELD: Range<usize> = 22..30;
/// Bytes in the fixed header; the body, which starts with the fingerprint, follows.
pub(crate) const HEADER_BYTES: usize = 30;
/// Bytes in the header of a snapshot written before the engine version was stamped: magic, both
/// revisions, then the body length at `16..24`.
pub(crate) const UNSTAMPED_HEADER_BYTES: usize = 24;
/// Bytes in the FNV-1a-128 trailer.
pub(crate) const TRAILER_BYTES: usize = 16;
/// The smallest input the decoder reads a header from: a header and a trailer with no body.
pub(crate) const SHORTEST_INSPECTED: usize = HEADER_BYTES + TRAILER_BYTES;

/// The subject of the capture recipe: an `Add` feeding a `UnitDelay` (sample period 1 s), with
/// boundary inputs `urn:frame:a` (bounded to [-1, 1]) and `urn:frame:b`.
pub(crate) const DELAY_MODEL: &[u8] = include_bytes!("../fixtures/frame_delay.jsonld");

/// The frames of the capture recipe, in order: model time, then `urn:frame:a` and `urn:frame:b`.
pub(crate) const DELAY_FRAMES: [(f64, f64, f64); 2] = [(0.0, 0.5, 2.0), (1.0, 0.5, 2.0)];

/// A freshly loaded [`DELAY_MODEL`], inside its durable restore window.
pub(crate) fn delay_target() -> Engine {
    let mut engine = Engine::new();
    engine.load_cxf(DELAY_MODEL).unwrap();
    engine
}

/// Accept one complete frame of [`DELAY_MODEL`] at `time` and return its boundary output
/// `urn:frame:y`.
pub(crate) fn delay_frame(engine: &mut Engine, time: f64, a: f64, b: f64) -> Value {
    let frame = engine
        .prepare_frame(
            time,
            &[
                ("urn:frame:a", Value::Real(a)),
                ("urn:frame:b", Value::Real(b)),
            ],
        )
        .unwrap();
    let completed = engine.execute_frame(frame).unwrap();
    let [(path, value)] = completed.outputs() else {
        panic!("the delay model has one boundary output");
    };
    assert_eq!(&**path, "urn:frame:y");
    value.clone()
}

/// The capture recipe: load [`DELAY_MODEL`] and accept [`DELAY_FRAMES`]. `state_snapshot()` on the
/// result is the populated snapshot the fixtures hold (model time 1.0, `UnitDelay` output 2.5).
pub(crate) fn delay_after_two_frames() -> Engine {
    let mut engine = delay_target();
    for (time, a, b) in DELAY_FRAMES {
        delay_frame(&mut engine, time, a, b);
    }
    engine
}

/// Decode one line of lowercase hex, as the `.hex` fixtures store their bytes.
pub(crate) fn unhex(text: &str) -> Vec<u8> {
    let hex = text.trim();
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
        .collect()
}

/// FNV-1a-128 as `docs/state-compatibility.md` specifies it.
pub(crate) fn fnv(bytes: &[u8]) -> u128 {
    bytes.iter().fold(
        0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128,
        |hash, byte| {
            (hash ^ u128::from(*byte)).wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b)
        },
    )
}

/// The trailer a snapshot carries.
pub(crate) fn trailer(bytes: &[u8]) -> u128 {
    u128::from_le_bytes(bytes[bytes.len() - TRAILER_BYTES..].try_into().unwrap())
}

/// The trailer the bytes before it call for.
pub(crate) fn computed_trailer(bytes: &[u8]) -> u128 {
    fnv(&bytes[..bytes.len() - TRAILER_BYTES])
}

/// Recompute the trailer after an edit, as a host editing the bytes would.
pub(crate) fn reseal(bytes: &mut [u8]) {
    let checksum = computed_trailer(bytes);
    let start = bytes.len() - TRAILER_BYTES;
    bytes[start..].copy_from_slice(&checksum.to_le_bytes());
}

/// The six bytes the header spells `version` with.
pub(crate) fn version_field(version: EngineVersion) -> [u8; 6] {
    let mut field = [0u8; 6];
    field[..2].copy_from_slice(&version.major.to_le_bytes());
    field[2..4].copy_from_slice(&version.minor.to_le_bytes());
    field[4..].copy_from_slice(&version.patch.to_le_bytes());
    field
}

/// The engine version a stamped header names.
pub(crate) fn stamped_version(bytes: &[u8]) -> EngineVersion {
    let field = &bytes[VERSION_FIELD];
    EngineVersion {
        major: u16::from_le_bytes([field[0], field[1]]),
        minor: u16::from_le_bytes([field[2], field[3]]),
        patch: u16::from_le_bytes([field[4], field[5]]),
    }
}

/// Where the canonical manifest sits in a snapshot whose fixed header is `header_bytes` long: the
/// body is the fingerprint, the model id, the model time, the optional prior time and then the
/// length-prefixed manifest.
pub(crate) fn manifest_range(bytes: &[u8], header_bytes: usize) -> Range<usize> {
    let model_id = header_bytes + 16;
    let model_id_len = u32::from_le_bytes(bytes[model_id..model_id + 4].try_into().unwrap());
    let prior_time_tag = model_id + 4 + usize::try_from(model_id_len).unwrap() + 8;
    let manifest_len = prior_time_tag + 1 + usize::from(bytes[prior_time_tag] == 1) * 8;
    let start = manifest_len + 8;
    let len = u64::from_le_bytes(bytes[manifest_len..start].try_into().unwrap());
    start..start + usize::try_from(len).unwrap()
}

/// The execution fingerprint a stamped snapshot carries for `manifest`: FNV-1a-128 over the ABI
/// revision, the engine-version field and the canonical manifest bytes.
pub(crate) fn fingerprint(abi_revision: u32, version: EngineVersion, manifest: &[u8]) -> u128 {
    let mut preimage = abi_revision.to_le_bytes().to_vec();
    preimage.extend_from_slice(&version_field(version));
    preimage.extend_from_slice(manifest);
    fnv(&preimage)
}

/// The fingerprint of a snapshot written before the engine version was stamped: FNV-1a-128 over
/// the ABI revision and the canonical manifest bytes.
pub(crate) fn unstamped_fingerprint(abi_revision: u32, manifest: &[u8]) -> u128 {
    let mut preimage = abi_revision.to_le_bytes().to_vec();
    preimage.extend_from_slice(manifest);
    fnv(&preimage)
}

/// The same state as `bytes` holds, as an engine of `version` with this build's layout would have
/// written it: the version field, the fingerprint that covers it and the trailer all say
/// `version`, so the result is consistent in every respect and differs from `bytes` in nothing
/// else.
pub(crate) fn restamp(bytes: &[u8], version: EngineVersion) -> Vec<u8> {
    let mut stamped = bytes.to_vec();
    stamped[VERSION_FIELD].copy_from_slice(&version_field(version));
    let abi_revision = u32::from_le_bytes(stamped[ABI_FIELD].try_into().unwrap());
    let manifest = manifest_range(&stamped, HEADER_BYTES);
    let fingerprint = fingerprint(abi_revision, version, &stamped[manifest]);
    stamped[HEADER_BYTES..HEADER_BYTES + 16].copy_from_slice(&fingerprint.to_le_bytes());
    reseal(&mut stamped);
    stamped
}

/// What a host does with snapshot bytes its envelope has approved: decode them, then restore into
/// the target. A decode refusal returns before `restore_state` is called.
pub(crate) fn restore_from_bytes(target: &mut Engine, bytes: &[u8]) -> Result<(), OcError> {
    let snapshot = EngineStateSnapshot::from_bytes(bytes)?;
    target.restore_state(&snapshot)
}

/// One connector value of a snapshot of [`DELAY_MODEL`]: the owning block, the direction (0 in,
/// 1 out), the port index and the Real value's bits.
pub(crate) type ConnectorReal = (String, u8, u32, u64);

/// The part of a [`DELAY_MODEL`] snapshot that follows the manifest, read by hand: the keyed
/// connector values, then the state words. Every key of that model is an authored block key and
/// every value a Real; anything else, nonzero reserved flags or bytes left over before the
/// trailer fail the test.
pub(crate) fn delay_payload(bytes: &[u8], header_bytes: usize) -> (Vec<ConnectorReal>, Vec<u64>) {
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let u64_at = |at: usize| u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
    let mut at = manifest_range(bytes, header_bytes).end;
    let value_count = u32_at(at);
    at += 4;
    let mut values = Vec::new();
    for _ in 0..value_count {
        assert_eq!(bytes[at], 0, "an authored block key");
        let owner_len = usize::try_from(u32_at(at + 1)).unwrap();
        let owner = std::str::from_utf8(&bytes[at + 5..at + 5 + owner_len]).unwrap();
        at += 5 + owner_len;
        let (direction, port) = (bytes[at], u32_at(at + 1));
        assert_eq!(bytes[at + 5], 0, "a Real value");
        values.push((owner.to_owned(), direction, port, u64_at(at + 6)));
        at += 14;
    }
    let word_count = usize::try_from(u64_at(at)).unwrap();
    at += 8;
    let words = (0..word_count)
        .map(|index| u64_at(at + index * 8))
        .collect();
    at += word_count * 8;
    assert_eq!(u32_at(at), 0, "reserved flags");
    assert_eq!(
        at + 4,
        bytes.len() - TRAILER_BYTES,
        "nothing after the flags"
    );
    (values, words)
}

/// The connector values the capture recipe leaves, in the snapshot's key order: the second
/// frame's inputs on the `Add` (0.5 and 2.0), its sum 2.5, the `UnitDelay` input connector (0.0
/// in every capture, on every commit the recipe was run on) and the delayed output 2.5, the first
/// frame's sum.
pub(crate) fn delay_recipe_values() -> Vec<ConnectorReal> {
    [
        ("urn:frame:add", 0, 0, 0.5f64),
        ("urn:frame:add", 0, 1, 2.0),
        ("urn:frame:add", 1, 0, 2.5),
        ("urn:frame:delay", 0, 0, 0.0),
        ("urn:frame:delay", 1, 0, 2.5),
    ]
    .map(|(owner, direction, port, value)| (owner.to_owned(), direction, port, value.to_bits()))
    .to_vec()
}

/// The five `UnitDelay` state words the capture recipe leaves: 2.5 twice, negative zero, and two
/// words equal to 1. Pinned from the captures, not derived here from the block's state layout.
pub(crate) fn delay_recipe_words() -> Vec<u64> {
    vec![
        2.5f64.to_bits(),
        2.5f64.to_bits(),
        (-0.0f64).to_bits(),
        1,
        1,
    ]
}
