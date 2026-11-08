//! A property test over random byte mutations of every compiled-schedule blob golden
//! (GitHub #400): whatever the bytes, `decode` returns a value or a typed refusal and never panics.
//!
//! A seeded xorshift64* generator (no dependency) picks a golden, flips one to four random bytes
//! to random values, and in nine cases out of ten re-checksums the result, so the mutation reaches
//! the structural and semantic checks instead of stopping at the CRC. A decode that succeeds is
//! re-encoded and must decode again. `OCE_BLOB_FUZZ_CASES` overrides the case count (default
//! 10,000) and `OCE_BLOB_FUZZ_SEED` the seed.

#[path = "support/blob_corpus.rs"]
mod blob_corpus;
#[path = "support/blob_image.rs"]
mod blob_image;

use blob_corpus::{FIXTURES, golden};
use blob_image::rechecksum;
use oce_graph::blob::{BlobError, HEADER_BYTES, decode, encode};

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
fn random_byte_mutations_never_panic_the_decoder() {
    let cases = env_u64("OCE_BLOB_FUZZ_CASES", 10_000);
    let mut rng = XorShift(env_u64("OCE_BLOB_FUZZ_SEED", 0x0CEB_0400_F022_5EED).max(1));
    let goldens: Vec<Vec<u8>> = FIXTURES.iter().map(|name| golden(name)).collect();
    let (mut accepted, mut integrity, mut deeper) = (0u64, 0u64, 0u64);
    for case in 0..cases {
        let source = &goldens[rng.below(goldens.len())];
        let mut bytes = source.clone();
        let flips = 1 + rng.below(4);
        // Mostly mutate the body after the header, where the structural checks live.
        for _ in 0..flips {
            let at = if rng.below(8) == 0 {
                rng.below(bytes.len())
            } else {
                HEADER_BYTES + rng.below(bytes.len() - HEADER_BYTES - 4)
            };
            bytes[at] = rng.next() as u8;
        }
        if rng.below(10) != 0 && bytes[16..20] == source[16..20] {
            rechecksum(&mut bytes);
        }
        match decode(&bytes) {
            Ok(decoded) => {
                accepted += 1;
                let again = encode(&decoded.source())
                    .unwrap_or_else(|err| panic!("case {case}: re-encode refused: {err}"));
                decode(&again)
                    .unwrap_or_else(|err| panic!("case {case}: re-decode refused: {err}"));
            }
            Err(BlobError::IntegrityMismatch { .. }) => integrity += 1,
            Err(_) => deeper += 1,
        }
    }
    assert_eq!(accepted + integrity + deeper, cases);
    if cases >= 1_000 {
        assert!(
            deeper > cases / 2,
            "most mutations must reach the checks behind the CRC: {deeper} of {cases}"
        );
    }
}
