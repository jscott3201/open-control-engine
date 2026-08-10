//! Adversarial byte-level tests for the durable snapshot decoder.

use crate::state::{BlockKey, ConnectorKey, WireDir};
use crate::{Engine, EngineStateError, EngineStateSnapshot};

const MINIMAL_LOOP: &[u8] = include_bytes!("../../../oce-cxf/tests/fixtures/minimal_loop.jsonld");

fn snapshot_bytes() -> Vec<u8> {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MINIMAL_LOOP).unwrap();
    engine.tick(0.0).unwrap();
    engine.state_snapshot().unwrap().into_bytes()
}

fn fnv(bytes: &[u8]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
    })
}

fn resign(bytes: &mut [u8]) {
    let trailer = bytes.len() - 16;
    let checksum = fnv(&bytes[..trailer]);
    bytes[trailer..].copy_from_slice(&checksum.to_le_bytes());
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

struct Offsets {
    model_bytes: std::ops::Range<usize>,
    state_t: usize,
    prev_tag: usize,
    manifest_length: usize,
    manifest: std::ops::Range<usize>,
    first_value_tag: usize,
    flags: usize,
}

fn offsets(bytes: &[u8]) -> Offsets {
    let model_length = u32_at(bytes, 40) as usize;
    let model_bytes = 44..44 + model_length;
    let state_t = model_bytes.end;
    let prev_tag = state_t + 8;
    let manifest_length = prev_tag + 1 + usize::from(bytes[prev_tag] == 1) * 8;
    let manifest_start = manifest_length + 8;
    let manifest_end = manifest_start + u64_at(bytes, manifest_length) as usize;
    let values_count = manifest_end;
    assert!(u32_at(bytes, values_count) > 0);
    let mut cursor = values_count + 4;
    cursor = skip_connector_key(bytes, cursor);
    let first_value_tag = cursor;
    Offsets {
        model_bytes,
        state_t,
        prev_tag,
        manifest_length,
        manifest: manifest_start..manifest_end,
        first_value_tag,
        flags: bytes.len() - 16 - 4,
    }
}

fn skip_string(bytes: &[u8], offset: usize) -> usize {
    offset + 4 + u32_at(bytes, offset) as usize
}

fn skip_connector_key(bytes: &[u8], offset: usize) -> usize {
    let mut cursor = offset + 1;
    match bytes[offset] {
        0 => cursor = skip_string(bytes, cursor),
        1 => {
            cursor = skip_string(bytes, cursor);
            cursor = skip_string(bytes, cursor);
        }
        tag => panic!("unexpected block-key tag {tag}"),
    }
    cursor + 1 + 4
}

#[test]
fn every_truncation_boundary_is_a_typed_refusal() {
    let bytes = snapshot_bytes();
    for length in 0..bytes.len() {
        assert!(
            EngineStateSnapshot::from_bytes(&bytes[..length]).is_err(),
            "truncation at {length} was accepted"
        );
    }
}

#[test]
fn fixed_header_refusals_follow_the_pinned_precedence() {
    let bytes = snapshot_bytes();

    let mut bad_magic = bytes.clone();
    bad_magic[0] ^= 1;
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bad_magic),
        Err(EngineStateError::MalformedSnapshot { offset: 0, .. })
    ));

    let mut unsupported = bytes.clone();
    unsupported[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&unsupported),
        Err(EngineStateError::UnsupportedFormat { revision: 2 })
    ));

    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&trailing),
        Err(EngineStateError::MalformedSnapshot { offset: 16, .. })
    ));
}

#[test]
fn body_tags_utf8_lengths_and_flags_refuse_after_valid_integrity() {
    let bytes = snapshot_bytes();
    let layout = offsets(&bytes);

    let mut invalid_utf8 = bytes.clone();
    invalid_utf8[layout.model_bytes.start] = 0xff;
    resign(&mut invalid_utf8);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&invalid_utf8),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut prev_tag = bytes.clone();
    prev_tag[layout.prev_tag] = 2;
    resign(&mut prev_tag);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&prev_tag),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut manifest_length = bytes.clone();
    manifest_length[layout.manifest_length..layout.manifest_length + 8]
        .copy_from_slice(&u64::MAX.to_le_bytes());
    resign(&mut manifest_length);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&manifest_length),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut portability_tag = bytes.clone();
    portability_tag[layout.manifest.start] = 9;
    resign(&mut portability_tag);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&portability_tag),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut flags = bytes.clone();
    flags[layout.flags..layout.flags + 4].copy_from_slice(&1u32.to_le_bytes());
    resign(&mut flags);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&flags),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));
}

#[test]
fn self_consistency_and_value_type_corruption_refuse() {
    let bytes = snapshot_bytes();
    let layout = offsets(&bytes);

    let mut fingerprint = bytes.clone();
    fingerprint[24] ^= 1;
    resign(&mut fingerprint);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&fingerprint),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut nonfinite_time = bytes.clone();
    nonfinite_time[layout.state_t..layout.state_t + 8]
        .copy_from_slice(&f64::INFINITY.to_bits().to_le_bytes());
    resign(&mut nonfinite_time);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&nonfinite_time),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut wrong_value_tag = bytes.clone();
    wrong_value_tag[layout.first_value_tag] = if wrong_value_tag[layout.first_value_tag] == 0 {
        1
    } else {
        0
    };
    resign(&mut wrong_value_tag);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&wrong_value_tag),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));
}

#[test]
fn integrity_mismatch_reports_recomputed_and_carried_values() {
    let mut bytes = snapshot_bytes();
    bytes[24] ^= 1;
    let expected = fnv(&bytes[..bytes.len() - 16]);
    let found = u128::from_le_bytes(bytes[bytes.len() - 16..].try_into().unwrap());
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bytes),
        Err(EngineStateError::IntegrityMismatch {
            expected: actual_expected,
            found: actual_found
        }) if actual_expected == expected && actual_found == found
    ));
}

#[test]
fn key_order_is_lexicographic_over_complete_wire_bytes() {
    let short = BlockKey::Authored("z".into());
    let long = BlockKey::Authored("aa".into());
    assert!(
        short < long,
        "wire length prefix must sort before string bytes"
    );

    let low_numeric = ConnectorKey {
        owner: short.clone(),
        direction: WireDir::Out,
        port_index: 1,
    };
    let low_wire = ConnectorKey {
        owner: short,
        direction: WireDir::Out,
        port_index: 256,
    };
    assert!(
        low_wire < low_numeric,
        "little-endian port bytes must govern canonical order"
    );
}

#[test]
fn hostile_section_counts_refuse_before_large_allocation() {
    let mut bytes = snapshot_bytes();
    let layout = offsets(&bytes);
    let enum_count = layout.manifest.start + 1;
    let enum_remaining = layout.manifest.end - (enum_count + 4);
    bytes[enum_count..enum_count + 4].copy_from_slice(&(enum_remaining as u32).to_le_bytes());
    resign(&mut bytes);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bytes),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));

    let mut bytes = snapshot_bytes();
    let layout = offsets(&bytes);
    let values_start = layout.manifest.end + 4;
    let values_remaining = bytes.len() - 16 - values_start;
    let value_count = (values_remaining / 2) as u32;
    assert!(value_count > 0 && value_count as usize * 2 <= values_remaining);
    bytes[layout.manifest.end..layout.manifest.end + 4].copy_from_slice(&value_count.to_le_bytes());
    resign(&mut bytes);
    assert!(matches!(
        EngineStateSnapshot::from_bytes(&bytes),
        Err(EngineStateError::MalformedSnapshot { .. })
    ));
}
