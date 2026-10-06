//! Independent byte construction from the revision-1 specification, not the product encoder.
//! The two hex payloads are hand-authored: Add(1.5,2.25)=3.75 and the five native value domains.
//! Descriptor text is the existing independently authored compatibility golden. Header lengths
//! and FNV trailer are assembled here; no production hashing/encoding/private symbol is called.
#![allow(dead_code)]

pub const DESCRIPTOR: &str = include_str!("../fixtures/compatibility.txt");

pub fn hex(text: &str) -> Vec<u8> {
    let compact: String = text.split_whitespace().collect();
    assert_eq!(compact.len() % 2, 0);
    compact
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

pub fn string(bytes: &mut Vec<u8>, text: &str) {
    bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
    bytes.extend_from_slice(text.as_bytes());
}

pub fn checksum(bytes: &[u8]) -> u128 {
    // Two-word schoolbook multiplication, independent of StableHash's wrapping u128 product.
    let (mut hi, mut lo) = (0x6c62272e07bb0142_u64, 0x62b821756295c58d_u64);
    for &byte in bytes {
        lo ^= u64::from(byte);
        let low_product = u128::from(lo) * 0x13b;
        hi = hi
            .wrapping_mul(0x13b)
            .wrapping_add(lo.wrapping_mul(0x01000000))
            .wrapping_add((low_product >> 64) as u64);
        lo = low_product as u64;
    }
    (u128::from(hi) << 64) | u128::from(lo)
}

pub fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 16;
    let hash = checksum(&bytes[..end]);
    bytes[end..].copy_from_slice(&hash.to_le_bytes());
}

pub fn assemble(descriptor: &str, target: Option<(&str, &str)>, payload: &[u8]) -> Vec<u8> {
    let mut body = vec![0, u8::from(target.is_some())];
    if let Some((arch, os)) = target {
        string(&mut body, arch);
        string(&mut body, os);
    }
    string(&mut body, descriptor);
    body.extend_from_slice(payload);
    let mut bytes = b"OCERPLY\0\x01\x00\x00\x00".to_vec();
    bytes.extend_from_slice(&(body.len() as u64).to_le_bytes());
    bytes.extend(body);
    bytes.extend([0; 16]);
    seal(&mut bytes);
    bytes
}

pub fn add() -> Vec<u8> {
    assemble(
        DESCRIPTOR,
        None,
        &hex(include_str!("../fixtures/replay_add.hex")),
    )
}
pub fn values() -> Vec<u8> {
    assemble(
        DESCRIPTOR,
        None,
        &hex(include_str!("../fixtures/replay_values.hex")),
    )
}

pub fn body_start() -> usize {
    20 + 2 + 4 + DESCRIPTOR.len()
}

pub fn offset(bytes: &[u8], needle: &[u8]) -> usize {
    bytes
        .windows(needle.len())
        .position(|p| p == needle)
        .expect("oracle field exists")
}
