//! Revision-1 replay encoding and allocation-free admission before owned decoding.

use crate::replay::ReplayData;
use crate::replay_wire::{Reader, Writer};
use crate::{
    AssertEvent, AssertLevel, CompletedFrame, MAX_REPLAY_BYTES, ReplayError, ReplayRecord,
    StatePortability, Value,
};

const MAGIC: &[u8; 8] = b"OCERPLY\0";
const HEADER: usize = 20;
const TRAILER: usize = 16;

pub(crate) fn hash(bytes: &[u8]) -> u128 {
    let mut hash = crate::stable_hash::StableHash::new();
    hash.write_bytes(bytes);
    hash.finish()
}

pub(crate) fn encode(frame: &CompletedFrame, descriptor: &str) -> Result<Vec<u8>, ReplayError> {
    let mut sizing = Writer {
        bytes: None,
        len: 0,
    };
    content(&mut sizing, frame, descriptor, 0)?;
    sizing.write(&[0; TRAILER])?;
    let len = sizing.len;
    let mut writer = Writer {
        bytes: Some(Vec::with_capacity(len)),
        len: 0,
    };
    content(
        &mut writer,
        frame,
        descriptor,
        (len - HEADER - TRAILER) as u64,
    )?;
    let checksum = hash(writer.bytes.as_ref().expect("encoding pass"));
    writer.write(&checksum.to_le_bytes())?;
    Ok(writer.bytes.expect("encoding pass"))
}

fn content(
    w: &mut Writer,
    frame: &CompletedFrame,
    descriptor: &str,
    body_len: u64,
) -> Result<(), ReplayError> {
    w.write(MAGIC)?;
    w.write(&1_u32.to_le_bytes())?;
    w.write(&body_len.to_le_bytes())?;
    w.write(&[0])?; // exact raw bits
    w.write(&[u8::from(frame.target_bound)])?;
    if frame.target_bound {
        w.string(std::env::consts::ARCH)?;
        w.string(std::env::consts::OS)?;
    }
    w.string(descriptor)?;
    w.write(&frame.time().to_bits().to_le_bytes())?;
    for values in [frame.inputs(), frame.outputs()] {
        w.count(values.len())?;
        for (path, value) in values {
            w.string(path)?;
            w.value(value)?;
        }
    }
    w.count(frame.diagnostics().len())?;
    for event in frame.diagnostics() {
        w.string(&event.block)?;
        w.string(&event.message)?;
        w.write(&event.t.to_bits().to_le_bytes())?;
        match event.level {
            AssertLevel::Warning => w.write(&[0])?,
        }
    }
    Ok(())
}

pub(crate) fn decode(bytes: &[u8]) -> Result<ReplayRecord, ReplayError> {
    if bytes.len() > MAX_REPLAY_BYTES {
        return Err(ReplayError::TooLarge {
            actual_bytes: bytes.len(),
            limit_bytes: MAX_REPLAY_BYTES,
        });
    }
    if bytes.len() < HEADER || &bytes[..8] != MAGIC {
        return Err(ReplayError::Header);
    }
    let mut header = Reader::new(bytes, 8);
    let revision = header.u32()?;
    if revision != 1 {
        return Err(ReplayError::UnsupportedFormat { revision });
    }
    let body_len = header.u64()?;
    if body_len.checked_add((HEADER + TRAILER) as u64) != Some(bytes.len() as u64) {
        return Err(ReplayError::Length { offset: 12 });
    }
    let end = bytes.len() - TRAILER;
    if hash(&bytes[..end]) != u128::from_le_bytes(bytes[end..].try_into().expect("trailer length"))
    {
        return Err(ReplayError::Integrity);
    }
    // This pass visits and charges EVERY field before ANY untrusted owned allocation.
    body(&bytes[..end], false)?;
    let data = body(&bytes[..end], true)?;
    Ok(ReplayRecord {
        bytes: bytes.to_vec(),
        data,
    })
}

fn label(r: &mut Reader<'_>) -> Result<(), ReplayError> {
    let offset = r.at;
    let text = r.string()?;
    if text.is_empty()
        || text.len() > 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err(ReplayError::Noncanonical { offset });
    }
    Ok(())
}

fn body(bytes: &[u8], own: bool) -> Result<ReplayData, ReplayError> {
    let mut r = Reader::new(bytes, HEADER);
    match r.u8()? {
        0 => (),
        tag => return Err(ReplayError::UnsupportedExactness { tag }),
    }
    let portability = match r.u8()? {
        0 => StatePortability::Portable,
        1 => {
            let at = r.at;
            label(&mut r)?;
            label(&mut r)?;
            if own {
                // Labels have already been charged; borrow again without double accounting.
                let mut labels = Reader::new(bytes, at);
                StatePortability::TargetBound {
                    arch: labels.string()?.into(),
                    os: labels.string()?.into(),
                }
            } else {
                StatePortability::Portable
            }
        }
        tag => return Err(ReplayError::UnknownPlacement { tag }),
    };
    let descriptor = r.string()?;
    crate::replay_descriptor::fields(descriptor)?;
    let time = f64::from_bits(r.u64()?);
    if !time.is_finite() {
        return Err(ReplayError::NonFiniteTime);
    }
    let inputs = values(&mut r, own)?;
    let outputs = values(&mut r, own)?;
    let count = r.count(17)?;
    let mut diagnostics = if own {
        Vec::with_capacity(count)
    } else {
        Vec::new()
    };
    for _ in 0..count {
        let block = r.string()?;
        let message = r.string()?;
        let t = f64::from_bits(r.u64()?);
        let level = match r.u8()? {
            0 => AssertLevel::Warning,
            tag => return Err(ReplayError::UnknownSeverity { tag }),
        };
        if own {
            diagnostics.push(AssertEvent {
                block: block.into(),
                message: message.into(),
                t,
                level,
            });
        }
    }
    if r.at != bytes.len() {
        return Err(ReplayError::Noncanonical { offset: r.at });
    }
    Ok(ReplayData {
        descriptor: if own {
            descriptor.into()
        } else {
            String::new()
        },
        portability,
        time,
        inputs,
        outputs,
        diagnostics,
    })
}

fn values(r: &mut Reader<'_>, own: bool) -> Result<Vec<(String, Value)>, ReplayError> {
    let count = r.count(6)?;
    let mut values = if own {
        Vec::with_capacity(count)
    } else {
        Vec::new()
    };
    let mut previous = None;
    for _ in 0..count {
        let offset = r.at;
        let key = r.string()?;
        if key.is_empty() || previous.is_some_and(|p| p >= key) {
            return Err(ReplayError::Noncanonical { offset });
        }
        previous = Some(key);
        let value = r.value()?;
        if own {
            values.push((key.into(), value.owned()));
        }
    }
    Ok(values)
}
