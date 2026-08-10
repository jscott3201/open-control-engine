//! Checked little-endian primitives for the state-snapshot codec.

use std::cell::Cell;
use std::rc::Rc;

use crate::state::EngineStateError;

const MAX_DECODE_ALLOCATION_BYTES: usize =
    crate::state::MAX_SNAPSHOT_BYTES as usize * 2 + 8 * 1024 * 1024;
const DECODE_INPUT_EXPANSION: usize = 16;

#[derive(Clone)]
pub(crate) struct DecodeBudget {
    remaining: Rc<Cell<usize>>,
}

impl DecodeBudget {
    pub(crate) fn for_input(input_bytes: usize) -> Self {
        Self {
            remaining: Rc::new(Cell::new(
                input_bytes
                    .saturating_mul(DECODE_INPUT_EXPANSION)
                    .min(MAX_DECODE_ALLOCATION_BYTES),
            )),
        }
    }

    fn claim(&self, bytes: usize, offset: u64, detail: &str) -> Result<(), EngineStateError> {
        let remaining = self.remaining.get();
        let Some(next) = remaining.checked_sub(bytes) else {
            return Err(EngineStateError::MalformedSnapshot {
                offset,
                detail: format!("{detail} exceeds the snapshot decode-allocation budget"),
            });
        };
        self.remaining.set(next);
        Ok(())
    }
}

pub(crate) struct Writer {
    bytes: Vec<u8>,
    logical_len: usize,
    materialize: bool,
}

impl Writer {
    pub(crate) fn new() -> Self {
        Self {
            bytes: Vec::new(),
            logical_len: 0,
            materialize: true,
        }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity.min(crate::state::MAX_SNAPSHOT_BYTES as usize)),
            logical_len: 0,
            materialize: true,
        }
    }

    pub(crate) fn counting() -> Self {
        Self {
            bytes: Vec::new(),
            logical_len: 0,
            materialize: false,
        }
    }

    pub(crate) fn position(&self) -> usize {
        self.logical_len
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.extend(&[value]);
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.extend(&value.to_le_bytes());
    }

    pub(crate) fn i64(&mut self, value: i64) {
        self.extend(&value.to_le_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.extend(&value.to_le_bytes());
    }

    pub(crate) fn u128(&mut self, value: u128) {
        self.extend(&value.to_le_bytes());
    }

    pub(crate) fn string(&mut self, value: &str) -> Result<(), EngineStateError> {
        let length =
            u32::try_from(value.len()).map_err(|_| EngineStateError::SnapshotTooLarge {
                actual_bytes: u64::try_from(value.len()).unwrap_or(u64::MAX),
                max_bytes: crate::state::MAX_SNAPSHOT_BYTES,
            })?;
        self.u32(length);
        self.extend(value.as_bytes());
        Ok(())
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.extend(value);
    }

    pub(crate) fn patch_u64(&mut self, offset: usize, value: u64) {
        if self.materialize
            && let Some(target) = self.bytes.get_mut(offset..offset.saturating_add(8))
        {
            target.copy_from_slice(&value.to_le_bytes());
        }
    }

    pub(crate) fn finish(self) -> Result<Vec<u8>, EngineStateError> {
        if self.logical_len > crate::state::MAX_SNAPSHOT_BYTES as usize {
            Err(EngineStateError::SnapshotTooLarge {
                actual_bytes: u64::try_from(self.logical_len).unwrap_or(u64::MAX),
                max_bytes: crate::state::MAX_SNAPSHOT_BYTES,
            })
        } else {
            Ok(self.bytes)
        }
    }

    fn extend(&mut self, value: &[u8]) {
        let start = self.logical_len;
        self.logical_len = self.logical_len.saturating_add(value.len());
        if self.materialize
            && self.logical_len <= crate::state::MAX_SNAPSHOT_BYTES as usize
            && self.bytes.len() == start
        {
            self.bytes.extend_from_slice(value);
        }
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    base_offset: u64,
    budget: DecodeBudget,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], base_offset: u64, budget: DecodeBudget) -> Self {
        Self {
            bytes,
            position: 0,
            base_offset,
            budget,
        }
    }

    pub(crate) fn offset(&self) -> u64 {
        self.base_offset
            .saturating_add(u64::try_from(self.position).unwrap_or(u64::MAX))
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.position)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.position == self.bytes.len()
    }

    pub(crate) fn u8(&mut self) -> Result<u8, EngineStateError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32, EngineStateError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("fixed-size slice");
        Ok(u32::from_le_bytes(bytes))
    }

    pub(crate) fn i64(&mut self) -> Result<i64, EngineStateError> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("fixed-size slice");
        Ok(i64::from_le_bytes(bytes))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, EngineStateError> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("fixed-size slice");
        Ok(u64::from_le_bytes(bytes))
    }

    pub(crate) fn u128(&mut self) -> Result<u128, EngineStateError> {
        let bytes: [u8; 16] = self.take(16)?.try_into().expect("fixed-size slice");
        Ok(u128::from_le_bytes(bytes))
    }

    pub(crate) fn string(&mut self) -> Result<String, EngineStateError> {
        self.string_bounded(crate::state::MAX_SNAPSHOT_BYTES as usize)
    }

    pub(crate) fn string_bounded(
        &mut self,
        allocation_limit: usize,
    ) -> Result<String, EngineStateError> {
        let length_offset = self.offset();
        let length =
            usize::try_from(self.u32()?).map_err(|_| EngineStateError::MalformedSnapshot {
                offset: length_offset,
                detail: "string length does not fit usize".into(),
            })?;
        if length > allocation_limit {
            return Err(EngineStateError::MalformedSnapshot {
                offset: length_offset,
                detail: "string allocation exceeds its bounded input budget".into(),
            });
        }
        let string_offset = self.offset();
        let bytes = self.take(length)?;
        let value =
            std::str::from_utf8(bytes).map_err(|error| EngineStateError::MalformedSnapshot {
                offset: string_offset.saturating_add(error.valid_up_to() as u64),
                detail: "string is not valid UTF-8".into(),
            })?;
        self.claim_allocation(length, "string allocation")?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(length)
            .map_err(|_| EngineStateError::MalformedSnapshot {
                offset: length_offset,
                detail: "string allocation exceeds available resources".into(),
            })?;
        owned.push_str(value);
        Ok(owned)
    }

    pub(crate) fn slice(&mut self, length: usize) -> Result<&'a [u8], EngineStateError> {
        self.take(length)
    }

    pub(crate) fn bounded_count(
        &self,
        count: u64,
        minimum_bytes: usize,
        detail: &str,
    ) -> Result<usize, EngineStateError> {
        let count = usize::try_from(count).map_err(|_| EngineStateError::MalformedSnapshot {
            offset: self.offset(),
            detail: format!("{detail} does not fit usize"),
        })?;
        let minimum = count.checked_mul(minimum_bytes).ok_or_else(|| {
            EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: format!("{detail} byte count overflows"),
            }
        })?;
        if minimum > self.remaining() {
            return Err(EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: format!("{detail} exceeds remaining input"),
            });
        }
        Ok(count)
    }

    pub(crate) fn bounded_vec<T>(
        &self,
        count: usize,
        minimum_encoded_bytes: usize,
        detail: &str,
    ) -> Result<Vec<T>, EngineStateError> {
        let allocation = count.checked_mul(std::mem::size_of::<T>()).ok_or_else(|| {
            EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: format!("{detail} allocation size overflows"),
            }
        })?;
        let minimum_input = count.checked_mul(minimum_encoded_bytes).ok_or_else(|| {
            EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: format!("{detail} input size overflows"),
            }
        })?;
        if minimum_input > self.remaining() {
            return Err(EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: format!("{detail} exceeds remaining input"),
            });
        }
        let allocation_ratio = std::mem::size_of::<T>()
            .max(1)
            .div_ceil(minimum_encoded_bytes.max(1));
        let allocation_budget = self
            .remaining()
            .saturating_mul(allocation_ratio)
            .min(crate::state::MAX_SNAPSHOT_BYTES as usize);
        if allocation > allocation_budget {
            return Err(EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: format!("{detail} allocation exceeds its bounded input budget"),
            });
        }
        self.claim_allocation(allocation, detail)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: format!("{detail} allocation failed"),
            })?;
        Ok(values)
    }

    pub(crate) fn claim_allocation(
        &self,
        bytes: usize,
        detail: &str,
    ) -> Result<(), EngineStateError> {
        self.budget.claim(bytes, self.offset(), detail)
    }

    pub(crate) fn push<T>(
        &self,
        values: &mut Vec<T>,
        value: T,
        detail: &str,
    ) -> Result<(), EngineStateError> {
        if values.len() == values.capacity() {
            return Err(EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: format!("{detail} exceeded its bounded allocation"),
            });
        }
        values.push(value);
        Ok(())
    }

    pub(crate) fn malformed<T>(&self, detail: impl Into<String>) -> Result<T, EngineStateError> {
        Err(EngineStateError::MalformedSnapshot {
            offset: self.offset(),
            detail: detail.into(),
        })
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], EngineStateError> {
        let end = self.position.checked_add(length).ok_or_else(|| {
            EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: "byte range overflows".into(),
            }
        })?;
        let value = self.bytes.get(self.position..end).ok_or_else(|| {
            EngineStateError::MalformedSnapshot {
                offset: self.offset(),
                detail: "unexpected end of snapshot".into(),
            }
        })?;
        self.position = end;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_vector_allocation_cannot_exceed_the_snapshot_cap() {
        let reader = Reader::new(&[], 0, DecodeBudget::for_input(0));
        let count = crate::state::MAX_SNAPSHOT_BYTES as usize / 1024 + 1;
        assert!(matches!(
            reader.bounded_vec::<[u8; 1024]>(count, 1, "hostile vector"),
            Err(EngineStateError::MalformedSnapshot { .. })
        ));
    }

    #[test]
    fn readers_share_one_input_scaled_allocation_budget() {
        let bytes = [0; 256];
        let budget = DecodeBudget::for_input(bytes.len());
        for _ in 0..DECODE_INPUT_EXPANSION {
            let reader = Reader::new(&bytes, 0, budget.clone());
            reader
                .bounded_vec::<[u8; 256]>(1, 256, "shared vector")
                .unwrap();
        }
        let reader = Reader::new(&bytes, 0, budget);
        assert!(matches!(
            reader.bounded_vec::<[u8; 256]>(1, 256, "shared vector"),
            Err(EngineStateError::MalformedSnapshot { .. })
        ));
    }
}
