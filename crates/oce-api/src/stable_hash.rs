//! Stable, dependency-free hashing shared by facade identity surfaces.

use oce_store::DomainKey;

pub(crate) struct StableHash {
    value: u128,
}

impl StableHash {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    pub(crate) fn new() -> Self {
        Self {
            value: Self::OFFSET,
        }
    }

    pub(crate) fn finish(self) -> u128 {
        self.value
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.value ^= u128::from(*byte);
            self.value = self.value.wrapping_mul(Self::PRIME);
        }
    }

    pub(crate) fn write_str(&mut self, value: &str) {
        self.write_usize(value.len());
        self.write_bytes(value.as_bytes());
    }

    pub(crate) fn write_opt_str(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.write_bool(true);
                self.write_str(value);
            }
            None => self.write_bool(false),
        }
    }

    pub(crate) fn write_domain_key(&mut self, key: &DomainKey) {
        self.write_str(key.as_str());
    }

    pub(crate) fn write_bool(&mut self, value: bool) {
        self.write_bytes(&[u8::from(value)]);
    }

    pub(crate) fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    pub(crate) fn write_u32(&mut self, value: u32) {
        self.write_bytes(&value.to_le_bytes());
    }

    pub(crate) fn write_u64(&mut self, value: u64) {
        self.write_bytes(&value.to_le_bytes());
    }

    pub(crate) fn write_i64(&mut self, value: i64) {
        self.write_bytes(&value.to_le_bytes());
    }
}
