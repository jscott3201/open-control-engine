//! The deterministic frame schedule both execution paths take, and the run digest that ties them.
//!
//! Shared by `oce-graph`'s `blob_execution` test (the blob entry, built `no_std + alloc` in the
//! `no-std-build` CI job) and `oce-api`'s `blob_dual_path_tests.rs` (the facade), which includes
//! this file by path. Inputs are a function of each slot's type and acceptance domain only, so the
//! two sides derive the same frames from their own I/O tables (the blob carries the facade's
//! effective domain).
//!
//! Reals sweep the declared range (or a span around zero when unbounded, signed zero included),
//! Integers step through the low end of their range, and Booleans toggle at a different period per
//! input. Steps cycle from 0.75 s to 300 s so the 160 frames span about 3.3 hours and the G36
//! timers (5 to 15 minutes) expire.

#![allow(dead_code)]

use oce_model::{Value, ValueType};

/// Frames per fixture.
pub const TICKS: u32 = 160;

/// The step before each frame cycles through these seconds.
pub const STEPS: [f64; 5] = [0.75, 1.0, 15.0, 60.0, 300.0];

/// The values an input with no declared bounds cycles through.
const UNBOUNDED_REALS: [f64; 11] = [-0.0, 0.1, 1.0, 0.4, 22.5, 0.6, 0.05, 293.15, 0.7, 2.0, -5.0];

/// The model time of frame `k`: zero, then the running sum of the cycled steps.
pub fn time_at(k: u32) -> f64 {
    (1..=k).fold(0.0, |time, j| time + STEPS[j as usize % STEPS.len()])
}

/// The value of input `slot` at frame `k`, inside its acceptance domain.
pub fn input_value(
    value_type: ValueType,
    min: Option<&Value>,
    max: Option<&Value>,
    slot: usize,
    k: u32,
) -> Value {
    // Each input holds a value for a slot-dependent number of frames (1 to 7), so a condition can
    // persist across the long steps that time alarms and holds.
    let held = k as usize / [2, 5, 8, 11, 14, 1, 4][slot % 7];
    let phase = (held * 7 + slot * 3) % 11;
    match value_type {
        ValueType::Real => {
            let min = min.and_then(|v| v.as_real().ok());
            let max = max.and_then(|v| v.as_real().ok());
            let x = match (min, max) {
                (Some(lo), Some(hi)) => lo + (hi - lo) * (phase as f64 / 10.0),
                (Some(lo), None) => lo + phase as f64 * 1.5,
                (None, Some(hi)) => hi - phase as f64 * 1.5,
                // Unbounded: small magnitudes (flows, fractions, signed zero) and plant-scale ones.
                (None, None) => UNBOUNDED_REALS[phase],
            };
            Value::Real(x)
        }
        ValueType::Integer => {
            let lo = min
                .and_then(|v| v.as_integer().ok())
                .unwrap_or(i64::from(i32::MIN));
            let hi = max
                .and_then(|v| v.as_integer().ok())
                .unwrap_or(i64::from(i32::MAX));
            let base = if lo <= 0 && 0 <= hi { 0 } else { lo };
            let top = hi.min(base + 6);
            Value::Integer(base + (phase as i64 % (top - base + 1)))
        }
        ValueType::Boolean => Value::Boolean((held / (slot % 3 + 1) + slot).is_multiple_of(2)),
        ValueType::String => Value::String("".into()),
        ValueType::Enum(class) => Value::Enum { class, ordinal: 1 },
    }
}

/// FNV-1a-64 over a run: per frame its time, sequence, boundary outputs in slot order, every
/// connector value and every state word. Values hash by tag and exact bits.
pub struct RunDigest(u64);

impl Default for RunDigest {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl RunDigest {
    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Fold one value in by tag and exact bits.
    pub fn value(&mut self, value: &Value) {
        match value {
            Value::Real(x) => {
                self.bytes(&[0]);
                self.bytes(&x.to_bits().to_le_bytes());
            }
            Value::Integer(n) => {
                self.bytes(&[1]);
                self.bytes(&n.to_le_bytes());
            }
            Value::Boolean(b) => self.bytes(&[2, u8::from(*b)]),
            Value::String(s) => {
                self.bytes(&[3]);
                self.bytes(&(s.len() as u64).to_le_bytes());
                self.bytes(s.as_bytes());
            }
            Value::Enum { class, ordinal } => {
                self.bytes(&[4]);
                self.bytes(&class.0.to_le_bytes());
                self.bytes(&ordinal.to_le_bytes());
            }
        }
    }

    /// Fold one accepted frame in.
    pub fn frame<'v>(
        &mut self,
        time: f64,
        sequence: u64,
        outputs: impl IntoIterator<Item = &'v Value>,
        values: &[Value],
        words: &[u64],
    ) {
        self.bytes(&time.to_bits().to_le_bytes());
        self.bytes(&sequence.to_le_bytes());
        for value in outputs {
            self.value(value);
        }
        for value in values {
            self.value(value);
        }
        for word in words {
            self.bytes(&word.to_le_bytes());
        }
    }

    /// The digest as sixteen lowercase hex digits.
    pub fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// The checked-in digest table: one `name<TAB>digest` line per fixture, sorted by name.
pub const DIGEST_TABLE: &str = "tests/fixtures/blob_frame_digests.tsv";

/// Parse the digest table text into `(name, digest)` pairs.
pub fn parse_digests(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(|line| {
            let (name, digest) = line.split_once('\t').expect("name<TAB>digest");
            (name.to_owned(), digest.to_owned())
        })
        .collect()
}
