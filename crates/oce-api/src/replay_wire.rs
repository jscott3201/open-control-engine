//! Flat, checked replay primitives and allocation accounting shared by validation/materialization.

use std::sync::Arc;

use oce_model::{EnumClassId, enum_descriptor, enum_descriptor_by_path};

use crate::{MAX_REPLAY_BYTES, ReplayError, Value};

pub(crate) const MAX_WORKSPACE: usize = 2 * MAX_REPLAY_BYTES;

pub(crate) struct Reader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) at: usize,
    charged: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], at: usize) -> Self {
        Self {
            bytes,
            at,
            charged: 0,
        }
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], ReplayError> {
        let end = self
            .at
            .checked_add(len)
            .ok_or(ReplayError::Length { offset: self.at })?;
        let value = self
            .bytes
            .get(self.at..end)
            .ok_or(ReplayError::Length { offset: self.at })?;
        self.at = end;
        Ok(value)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, ReplayError> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn u32(&mut self) -> Result<u32, ReplayError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }
    pub(crate) fn u64(&mut self) -> Result<u64, ReplayError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }

    pub(crate) fn charge(&mut self, bytes: usize) -> Result<(), ReplayError> {
        self.charged = self
            .charged
            .checked_add(bytes)
            .filter(|n| *n <= MAX_WORKSPACE)
            .ok_or(ReplayError::WorkspaceLimit)?;
        Ok(())
    }

    pub(crate) fn count(&mut self, minimum_wire: usize) -> Result<usize, ReplayError> {
        let offset = self.at;
        let count = self.u32()? as usize;
        if count > (self.bytes.len() - self.at) / minimum_wire {
            return Err(ReplayError::Length { offset });
        }
        // Fixed wire-policy charge, independent of Rust/native pointer layout. Covers both a
        // (String, Value) row and an AssertEvent; an explicit layout guard below keeps this honest.
        self.charge(count.checked_mul(64).ok_or(ReplayError::WorkspaceLimit)?)?;
        Ok(count)
    }

    pub(crate) fn string(&mut self) -> Result<&'a str, ReplayError> {
        let len = self.u32()? as usize;
        let offset = self.at;
        let value =
            std::str::from_utf8(self.take(len)?).map_err(|_| ReplayError::Utf8 { offset })?;
        self.charge(len)?;
        Ok(value)
    }

    pub(crate) fn value(&mut self) -> Result<ValueView<'a>, ReplayError> {
        Ok(match self.u8()? {
            0 => ValueView::Real(self.u64()?),
            1 => ValueView::Integer(self.u64()? as i64),
            2 => {
                let offset = self.at;
                match self.u8()? {
                    0 => ValueView::Boolean(false),
                    1 => ValueView::Boolean(true),
                    _ => return Err(ReplayError::Noncanonical { offset }),
                }
            }
            3 => {
                let text = self.string()?;
                // Fixed charge covers Arc<str>'s two reference counts and tail alignment.
                self.charge(24)?;
                ValueView::String(text)
            }
            4 => {
                let offset = self.at;
                let path = self.string()?;
                let ordinal = self.u32()?;
                let descriptor = enum_descriptor_by_path(path)
                    .filter(|d| ordinal != 0 && ordinal as usize <= d.members.len())
                    .ok_or(ReplayError::EnumDomain { offset })?;
                ValueView::Enum(descriptor.id, ordinal)
            }
            tag => return Err(ReplayError::UnknownValue { tag }),
        })
    }
}

pub(crate) enum ValueView<'a> {
    Real(u64),
    Integer(i64),
    Boolean(bool),
    String(&'a str),
    Enum(EnumClassId, u32),
}

// The canonical work policy is not allowed to undercharge the native representation.
const _: () = assert!(size_of::<(String, Value)>() <= 64);
const _: () = assert!(size_of::<crate::AssertEvent>() <= 64);
const _: () = assert!(3 * size_of::<usize>() <= 24);

impl ValueView<'_> {
    pub(crate) fn owned(self) -> Value {
        match self {
            Self::Real(bits) => Value::Real(f64::from_bits(bits)),
            Self::Integer(value) => Value::Integer(value),
            Self::Boolean(value) => Value::Boolean(value),
            Self::String(value) => Value::String(Arc::from(value)),
            Self::Enum(class, ordinal) => Value::Enum { class, ordinal },
        }
    }
}

/// A sizing pass and then an exactly sized output pass. Every append checks the public cap.
pub(crate) struct Writer {
    pub(crate) bytes: Option<Vec<u8>>,
    pub(crate) len: usize,
}

impl Writer {
    pub(crate) fn write(&mut self, bytes: &[u8]) -> Result<(), ReplayError> {
        let len = self.len.saturating_add(bytes.len());
        if len > MAX_REPLAY_BYTES {
            return Err(ReplayError::TooLarge {
                actual_bytes: len,
                limit_bytes: MAX_REPLAY_BYTES,
            });
        }
        if let Some(out) = &mut self.bytes {
            out.extend_from_slice(bytes);
        }
        self.len = len;
        Ok(())
    }

    pub(crate) fn count(&mut self, count: usize) -> Result<(), ReplayError> {
        let count = u32::try_from(count).map_err(|_| ReplayError::TooLarge {
            actual_bytes: count,
            limit_bytes: MAX_REPLAY_BYTES,
        })?;
        self.write(&count.to_le_bytes())
    }

    pub(crate) fn string(&mut self, value: &str) -> Result<(), ReplayError> {
        self.count(value.len())?;
        self.write(value.as_bytes())
    }

    pub(crate) fn value(&mut self, value: &Value) -> Result<(), ReplayError> {
        match value {
            Value::Real(value) => {
                self.write(&[0])?;
                self.write(&value.to_bits().to_le_bytes())
            }
            Value::Integer(value) => {
                self.write(&[1])?;
                self.write(&value.to_le_bytes())
            }
            Value::Boolean(value) => self.write(&[2, u8::from(*value)]),
            Value::String(value) => {
                self.write(&[3])?;
                self.string(value)
            }
            Value::Enum { class, ordinal } => {
                let descriptor = enum_descriptor(*class)
                    .filter(|d| *ordinal != 0 && *ordinal as usize <= d.members.len())
                    .ok_or(ReplayError::EnumDomain { offset: 0 })?;
                self.write(&[4])?;
                self.string(descriptor.class_path)?;
                self.write(&ordinal.to_le_bytes())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_charge_is_inclusive_and_checked_before_allocation() {
        let mut reader = Reader::new(&[], 0);
        reader.charge(MAX_WORKSPACE).unwrap();
        assert_eq!(reader.charge(1), Err(ReplayError::WorkspaceLimit));
        let mut reader = Reader::new(&[], 0);
        reader.charge(1).unwrap();
        assert_eq!(reader.charge(usize::MAX), Err(ReplayError::WorkspaceLimit));
    }
}
