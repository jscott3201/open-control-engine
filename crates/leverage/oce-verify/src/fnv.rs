//! FNV-1a-128, the hash behind `spec_id` (vectors note decision 16; commissioning note decision
//! 20).
//!
//! Offset basis `0x6c62272e07bb014262b821756295c58d`; for each byte, XOR it into the low bits and
//! multiply by the prime `0x0000000001000000000000000000013b` modulo 2^128. It is an identity for
//! canonical bytes, not a cryptographic digest: it names what was compared and does not resist a
//! chosen collision. The facade exposes no FNV function, so it is implemented here and pinned by
//! the published test vectors.

/// The FNV-1a-128 offset basis.
const OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;

/// The FNV-128 prime, 2^88 + 0x13b.
const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

/// The FNV-1a-128 hash of `bytes`.
pub(crate) fn fnv1a128(bytes: &[u8]) -> u128 {
    bytes.iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
    })
}

/// `<prefix>fnv1a128:<32 lowercase hex digits>` over `bytes`, most significant digit first.
pub(crate) fn tagged(prefix: &str, bytes: &[u8]) -> String {
    format!("{prefix}fnv1a128:{:032x}", fnv1a128(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values. The empty string, `a` and `foobar` are rows of the FNV reference test
    /// suite (Fowler, Noll, Vo); every row was computed outside this crate with
    /// arbitrary-precision integers (`h = ((h ^ byte) * prime) mod 2^128` in Python). They are
    /// literals: nothing here derives an expected value with the function under test.
    const REFERENCE: &[(&[u8], u128)] = &[
        (b"", 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d),
        (b"a", 0xd228_cb69_6f1a_8caf_7891_2b70_4e4a_8964),
        (b"b", 0xd228_cb69_721a_8caf_7891_2b70_4e4a_8d15),
        (b"foobar", 0x343e_1662_793c_64bf_6f0d_3597_ba44_6f18),
        (b"\0", 0xd228_cb69_101a_8caf_7891_2b70_4e4a_147f),
        (b"\xff", 0xd228_cb68_f51a_8caf_7891_2b70_4e49_f346),
        (
            b"The quick brown fox jumps over the lazy dog",
            0x68cc_e4cd_885e_a042_39f0_2af3_0e29_7870,
        ),
    ];

    #[test]
    fn the_reference_inputs_hash_to_their_reference_values() {
        for (input, expected) in REFERENCE {
            assert_eq!(fnv1a128(input), *expected, "{input:?}");
        }
    }

    #[test]
    fn one_changed_byte_or_one_added_zero_byte_changes_the_hash() {
        assert_ne!(fnv1a128(b"oce/vectors/v1"), fnv1a128(b"oce/vectors/v2"));
        assert_ne!(fnv1a128(b"a"), fnv1a128(b"a\0"));
        assert_ne!(fnv1a128(b""), fnv1a128(b"\0"));
    }

    #[test]
    fn a_megabyte_of_one_byte_matches_its_reference_value() {
        // 2^20 bytes of 0x5a: long enough that the multiply wraps at every step.
        let input = vec![0x5a_u8; 1 << 20];
        assert_eq!(fnv1a128(&input), 0xe827_10c5_3d54_aade_16ad_0af3_2295_c58d);
    }

    #[test]
    fn a_tagged_id_is_the_prefix_and_thirty_two_lowercase_hex_digits() {
        assert_eq!(
            tagged("spec:", b"a"),
            "spec:fnv1a128:d228cb696f1a8caf78912b704e4a8964"
        );
        // Leading zero digits are kept: the id is always 32 digits.
        assert_eq!(tagged("", b"").len(), "fnv1a128:".len() + 32);
        let id = tagged("spec:", b"");
        assert!(
            id["spec:fnv1a128:".len()..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
}
