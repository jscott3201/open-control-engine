//! Checked little-endian primitives, the value and value-type tag tables in both directions, and
//! CRC-32C. The conventions are the state snapshot's (`oce-api`'s `state_wire.rs`), duplicated
//! here rather than shared, so that snapshot bytes and code stay untouched
//! (`docs/design/compiled-schedule-blob.md`, "How this relates to the state snapshot").

use alloc::vec::Vec;

use oce_model::{Value, ValueType, enum_descriptor};

use super::error::{BlobError, EncodeFault, MalformedFault};

/// CRC-32C (Castagnoli: reflected polynomial `0x82F63B78`, initial value and final XOR
/// `0xFFFFFFFF`) of `bytes`. The check value of `b"123456789"` is `0xE3069283`.
///
/// Bitwise and table-less: no dependency and no 1 KiB table in flash. A blob is decoded once per
/// boot, so the eight shifts per byte cost nothing that matters.
#[must_use]
pub fn crc32c(bytes: &[u8]) -> u32 {
    let mut crc = Crc32c::new();
    crc.update(bytes);
    crc.finish()
}

/// Incremental CRC-32C, for checksumming state words without serializing them first.
pub(super) struct Crc32c(u32);

impl Crc32c {
    pub(super) const fn new() -> Self {
        Self(0xFFFF_FFFF)
    }

    pub(super) fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (self.0 & 1).wrapping_neg();
                self.0 = (self.0 >> 1) ^ (0x82F6_3B78 & mask);
            }
        }
    }

    pub(super) const fn finish(&self) -> u32 {
        !self.0
    }
}

/// The CRC-32C of state words, each as its eight little-endian bytes.
pub(super) fn state_crc(words: &[u64]) -> u32 {
    let mut crc = Crc32c::new();
    for word in words {
        crc.update(&word.to_le_bytes());
    }
    crc.finish()
}

/// A value as it sits in the blob, before its Enum class path is resolved.
#[derive(Clone, Copy, Debug)]
pub(super) enum RawValue<'a> {
    Real(u64),
    Integer(i64),
    Boolean(bool),
    String(&'a str),
    Enum { class_path: &'a str, ordinal: u32 },
}

/// A value type as it sits in the blob, before its Enum class path is resolved.
#[derive(Clone, Copy, Debug)]
pub(super) enum RawType {
    Real,
    Integer,
    Boolean,
    String,
    /// An Enum type; its class path is read and checked as UTF-8, then dropped, because no input
    /// may be Enum-typed.
    Enum,
}

/// A `u32` length-prefixed list of `u32`s, borrowed from the blob, with the absolute offset of its
/// first element.
#[derive(Clone, Copy, Debug)]
pub(super) struct U32List<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl U32List<'_> {
    pub(super) fn len(&self) -> usize {
        self.bytes.len() / 4
    }

    /// The absolute blob offset of element `index`.
    pub(super) fn offset_of(&self, index: usize) -> u32 {
        u32::try_from(self.offset.saturating_add(index.saturating_mul(4))).unwrap_or(u32::MAX)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
    }
}

/// A forward reader over one region of the blob. Every read is bounds-checked and returns a typed
/// [`BlobError::Malformed`] at the absolute offset of the failing field; nothing panics.
#[derive(Clone)]
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    base: usize,
}

impl<'a> Reader<'a> {
    /// A reader over `bytes`, whose first byte sits at absolute blob offset `base`.
    pub(super) fn new(bytes: &'a [u8], base: usize) -> Self {
        Self {
            bytes,
            position: 0,
            base,
        }
    }

    /// The absolute blob offset of the next byte. Blobs are at most 64 MiB, so it fits a `u32`.
    pub(super) fn offset(&self) -> u32 {
        u32::try_from(self.base.saturating_add(self.position)).unwrap_or(u32::MAX)
    }

    pub(super) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    pub(super) fn malformed<T>(&self, fault: MalformedFault) -> Result<T, BlobError> {
        Err(self.fault(fault))
    }

    pub(super) fn fault(&self, fault: MalformedFault) -> BlobError {
        BlobError::Malformed {
            offset: self.offset(),
            fault,
        }
    }

    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], BlobError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| self.fault(MalformedFault::Truncated))?;
        let slice = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| self.fault(MalformedFault::Truncated))?;
        self.position = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], BlobError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub(super) fn u8(&mut self) -> Result<u8, BlobError> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, BlobError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub(super) fn u32(&mut self) -> Result<u32, BlobError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub(super) fn u64(&mut self) -> Result<u64, BlobError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub(super) fn i64(&mut self) -> Result<i64, BlobError> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    /// A `u32` count, accepted only if the remaining bytes could hold that many records of at least
    /// `minimum_record` bytes each. This is the proof that precedes any allocation sized by it.
    pub(super) fn count(&mut self, minimum_record: usize) -> Result<usize, BlobError> {
        let at = self.clone();
        let count = usize::try_from(self.u32()?)
            .map_err(|_| at.fault(MalformedFault::CountExceedsInput))?;
        let needed = count
            .checked_mul(minimum_record)
            .ok_or_else(|| at.fault(MalformedFault::CountExceedsInput))?;
        if needed > self.remaining() {
            return at.malformed(MalformedFault::CountExceedsInput);
        }
        Ok(count)
    }

    /// A `u32` byte length followed by that many bytes, borrowed.
    pub(super) fn bytes_field(&mut self) -> Result<&'a [u8], BlobError> {
        let length = self.count(1)?;
        self.take(length)
    }

    /// A `u32` byte length followed by that many UTF-8 bytes, borrowed.
    pub(super) fn str(&mut self) -> Result<&'a str, BlobError> {
        let length = self.count(1)?;
        let start = self.position;
        let bytes = self.take(length)?;
        core::str::from_utf8(bytes).map_err(|error| BlobError::Malformed {
            offset: u32::try_from(self.base + start + error.valid_up_to()).unwrap_or(u32::MAX),
            fault: MalformedFault::InvalidUtf8,
        })
    }

    /// A count-prefixed list of `u32`s, borrowed.
    pub(super) fn u32_list(&mut self) -> Result<U32List<'a>, BlobError> {
        self.u32_list_at_most(usize::MAX)
    }

    /// A count-prefixed list of `u32`s whose count may not exceed `limit`
    /// ([`MalformedFault::ArityTooLarge`] at the count's offset).
    pub(super) fn u32_list_at_most(&mut self, limit: usize) -> Result<U32List<'a>, BlobError> {
        let at = self.clone();
        let count = self.count(4)?;
        if count > limit {
            return at.malformed(MalformedFault::ArityTooLarge);
        }
        let offset = self.base.saturating_add(self.position);
        // `count * 4` cannot overflow: `count` already passed the same product against the input.
        Ok(U32List {
            bytes: self.take(count * 4)?,
            offset,
        })
    }

    /// A tag byte that must be `0` or `1`.
    pub(super) fn flag(&mut self) -> Result<bool, BlobError> {
        let at = self.clone();
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => at.malformed(MalformedFault::InvalidTag),
        }
    }

    /// A `value`: tag 0 Real bits · 1 Integer · 2 Boolean `u8` 0/1 · 3 String · 4 Enum class path
    /// and `u32` 1-based ordinal.
    pub(super) fn value(&mut self) -> Result<RawValue<'a>, BlobError> {
        let at = self.clone();
        Ok(match self.u8()? {
            0 => RawValue::Real(self.u64()?),
            1 => RawValue::Integer(self.i64()?),
            2 => RawValue::Boolean(self.flag()?),
            3 => RawValue::String(self.str()?),
            4 => RawValue::Enum {
                class_path: self.str()?,
                ordinal: self.u32()?,
            },
            _ => return at.malformed(MalformedFault::InvalidTag),
        })
    }

    /// A `vtype`: tag 0 Real · 1 Integer · 2 Boolean · 3 String · 4 Enum and its class path.
    pub(super) fn value_type(&mut self) -> Result<RawType, BlobError> {
        let at = self.clone();
        Ok(match self.u8()? {
            0 => RawType::Real,
            1 => RawType::Integer,
            2 => RawType::Boolean,
            3 => RawType::String,
            4 => {
                self.str()?;
                RawType::Enum
            }
            _ => return at.malformed(MalformedFault::InvalidTag),
        })
    }

    /// An `opt<value>`: `0` absent, or `1` followed by a `value`.
    pub(super) fn optional_value(&mut self) -> Result<Option<RawValue<'a>>, BlobError> {
        Ok(if self.flag()? {
            Some(self.value()?)
        } else {
            None
        })
    }
}

/// An appending writer. Lengths and counts that do not fit a `u32` are typed refusals.
pub(super) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(super) fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(super) fn len(&self) -> usize {
        self.bytes.len()
    }

    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(super) fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(super) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(super) fn u16(&mut self, value: u16) {
        self.raw(&value.to_le_bytes());
    }

    pub(super) fn u32(&mut self, value: u32) {
        self.raw(&value.to_le_bytes());
    }

    pub(super) fn u64(&mut self, value: u64) {
        self.raw(&value.to_le_bytes());
    }

    pub(super) fn i64(&mut self, value: i64) {
        self.raw(&value.to_le_bytes());
    }

    /// A count or length as `u32`.
    pub(super) fn count(&mut self, value: usize) -> Result<(), BlobError> {
        self.u32(u32::try_from(value).map_err(|_| unencodable(EncodeFault::CountOverflow, 0))?);
        Ok(())
    }

    pub(super) fn str(&mut self, value: &str) -> Result<(), BlobError> {
        self.count(value.len())?;
        self.raw(value.as_bytes());
        Ok(())
    }

    pub(super) fn bytes_field(&mut self, value: &[u8]) -> Result<(), BlobError> {
        self.count(value.len())?;
        self.raw(value);
        Ok(())
    }

    /// Overwrite four bytes at `at` with `value` (a section length or the total length).
    pub(super) fn patch_u32(&mut self, at: usize, value: u32) {
        if let Some(slot) = self.bytes.get_mut(at..at + 4) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
    }

    /// A `value`, the inverse of [`Reader::value`]. An Enum value is written by its canonical
    /// class path; a class without a descriptor cannot be encoded.
    pub(super) fn value(&mut self, value: &Value, index: u32) -> Result<(), BlobError> {
        match value {
            Value::Real(x) => {
                self.u8(0);
                self.u64(x.to_bits());
            }
            Value::Integer(n) => {
                self.u8(1);
                self.i64(*n);
            }
            Value::Boolean(b) => {
                self.u8(2);
                self.u8(u8::from(*b));
            }
            Value::String(s) => {
                self.u8(3);
                self.str(s)?;
            }
            Value::Enum { class, ordinal } => {
                let descriptor = enum_descriptor(*class)
                    .ok_or(unencodable(EncodeFault::UnknownEnumClass, index))?;
                self.u8(4);
                self.str(descriptor.class_path)?;
                self.u32(*ordinal);
            }
        }
        Ok(())
    }

    /// A `vtype`, the inverse of [`Reader::value_type`].
    pub(super) fn value_type(
        &mut self,
        value_type: ValueType,
        index: u32,
    ) -> Result<(), BlobError> {
        match value_type {
            ValueType::Real => self.u8(0),
            ValueType::Integer => self.u8(1),
            ValueType::Boolean => self.u8(2),
            ValueType::String => self.u8(3),
            ValueType::Enum(class) => {
                let descriptor = enum_descriptor(class)
                    .ok_or(unencodable(EncodeFault::UnknownEnumClass, index))?;
                self.u8(4);
                self.str(descriptor.class_path)?;
            }
        }
        Ok(())
    }

    /// An `opt<value>`.
    pub(super) fn optional_value(
        &mut self,
        value: Option<&Value>,
        index: u32,
    ) -> Result<(), BlobError> {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                self.value(value, index)?;
            }
        }
        Ok(())
    }
}

pub(super) const fn unencodable(fault: EncodeFault, index: u32) -> BlobError {
    BlobError::Unencodable { index, fault }
}
