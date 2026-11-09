//! The `OCECANON` version 1 byte encoding of a canonical graph (`docs/design/content-id.md`,
//! "Canonical bytes"): fixed-width little-endian integers, `u32`-length-prefixed UTF-8, Reals as
//! `f64::to_bits`, the blob's value and value-type tag tables, and `u32`-counted lists with no
//! section frames or padding. The bytes are hashed, never parsed.

use oce_model::{Value, ValueType, enum_descriptor};

use super::ContentIdError;
use super::graph::{CanonicalGraph, Driver, Labels, Name};

/// The magic every encoding starts with.
pub(super) const MAGIC: &[u8; 8] = b"OCECANON";
/// The encoding version; `canon1` in the id text.
pub(super) const VERSION: u16 = 1;

/// Encode `graph` as `OCECANON` v1 into one `Vec<u8>`, allocated once at its exact length: a
/// first pass over the same writer only counts bytes.
///
/// # Errors
/// [`ContentIdError::UnknownEnumClass`] for an Enum type or value with no descriptor, and
/// [`ContentIdError::TooLarge`] for a count or length past `u32`, neither reachable from a loaded
/// document. Never panics.
pub(super) fn encode(graph: &CanonicalGraph<'_>) -> Result<Vec<u8>, ContentIdError> {
    let mut size = Writer(0_usize);
    write(&mut size, graph)?;
    let mut bytes = Writer(Vec::with_capacity(size.0));
    write(&mut bytes, graph)?;
    Ok(bytes.0)
}

/// Where encoded bytes go: appended to a vector, or only counted.
trait Sink {
    fn raw(&mut self, bytes: &[u8]);
}

impl Sink for Vec<u8> {
    fn raw(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

impl Sink for usize {
    fn raw(&mut self, bytes: &[u8]) {
        *self = self.saturating_add(bytes.len());
    }
}

fn write<S: Sink>(w: &mut Writer<S>, graph: &CanonicalGraph<'_>) -> Result<(), ContentIdError> {
    w.raw(MAGIC);
    w.u16(VERSION);
    w.u16(0);

    w.count(graph.inputs.len())?;
    for input in &graph.inputs {
        w.name(&input.name)?;
        w.value_type(input.value_type)?;
        w.labels(&input.labels)?;
        w.optional_value(input.min)?;
        w.optional_value(input.max)?;
        w.count(input.targets.len())?;
        for (block, port) in &input.targets {
            w.name(block)?;
            w.u32(*port);
        }
    }

    w.count(graph.outputs.len())?;
    for output in &graph.outputs {
        w.name(&output.name)?;
        w.value_type(output.value_type)?;
        w.labels(&output.labels)?;
        match &output.driver {
            Driver::Block(block, port) => {
                w.u8(0);
                w.name(block)?;
                w.u32(*port);
            }
            Driver::Input(input) => {
                w.u8(1);
                w.name(input)?;
            }
        }
    }

    w.count(graph.blocks.len())?;
    for block in &graph.blocks {
        w.name(&block.name)?;
        w.str(block.class_path)?;
        w.u64(block.signature_hash);
        w.count(block.params.len())?;
        for (name, value) in &block.params {
            w.str(name)?;
            w.value(value)?;
        }
    }

    w.count(graph.connections.len())?;
    for edge in &graph.connections {
        w.name(&edge.target)?;
        w.u32(edge.input_port);
        w.name(&edge.source)?;
        w.u32(edge.output_port);
    }
    Ok(())
}

struct Writer<S>(S);

impl<S: Sink> Writer<S> {
    fn raw(&mut self, bytes: &[u8]) {
        self.0.raw(bytes);
    }

    fn u8(&mut self, value: u8) {
        self.raw(&[value]);
    }

    fn u16(&mut self, value: u16) {
        self.raw(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.raw(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.raw(&value.to_le_bytes());
    }

    fn count(&mut self, value: usize) -> Result<(), ContentIdError> {
        self.u32(u32::try_from(value).map_err(|_| ContentIdError::TooLarge)?);
        Ok(())
    }

    fn str(&mut self, value: &str) -> Result<(), ContentIdError> {
        self.count(value.len())?;
        self.raw(value.as_bytes());
        Ok(())
    }

    fn optional_str(&mut self, value: Option<&str>) -> Result<(), ContentIdError> {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                self.str(value)?;
            }
        }
        Ok(())
    }

    fn name(&mut self, name: &Name) -> Result<(), ContentIdError> {
        self.u8(name.form);
        self.str(&name.text)
    }

    fn labels(&mut self, labels: &Labels<'_>) -> Result<(), ContentIdError> {
        self.optional_str(labels.quantity)?;
        self.optional_str(labels.unit)?;
        self.optional_str(labels.display_unit)
    }

    /// The blob's `vtype` tags: 0 Real, 1 Integer, 2 Boolean, 3 String, 4 Enum and class path.
    fn value_type(&mut self, value_type: ValueType) -> Result<(), ContentIdError> {
        match value_type {
            ValueType::Real => self.u8(0),
            ValueType::Integer => self.u8(1),
            ValueType::Boolean => self.u8(2),
            ValueType::String => self.u8(3),
            ValueType::Enum(class) => {
                let descriptor = enum_descriptor(class).ok_or(ContentIdError::UnknownEnumClass)?;
                self.u8(4);
                self.str(descriptor.class_path)?;
            }
        }
        Ok(())
    }

    /// The blob's `value` tags: 0 Real bits, 1 Integer, 2 Boolean, 3 String, 4 Enum class path and
    /// `u32` ordinal.
    fn value(&mut self, value: &Value) -> Result<(), ContentIdError> {
        match value {
            Value::Real(x) => {
                self.u8(0);
                self.u64(x.to_bits());
            }
            Value::Integer(n) => {
                self.u8(1);
                self.raw(&n.to_le_bytes());
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
                let descriptor = enum_descriptor(*class).ok_or(ContentIdError::UnknownEnumClass)?;
                self.u8(4);
                self.str(descriptor.class_path)?;
                self.u32(*ordinal);
            }
        }
        Ok(())
    }

    fn optional_value(&mut self, value: Option<&Value>) -> Result<(), ContentIdError> {
        match value {
            None => self.u8(0),
            Some(value) => {
                self.u8(1);
                self.value(value)?;
            }
        }
        Ok(())
    }
}
