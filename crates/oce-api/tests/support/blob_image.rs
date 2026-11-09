//! An independent, test-side `OCEB` writer for hostile-input tests.
//!
//! `Image` is the blob as plain lists, written from the format table in
//! `docs/design/compiled-schedule-blob.md` without the production encoder: it canonicalizes
//! nothing, so a test can put any list out of order, repeat an entry, override a count or a tag,
//! and still produce correct section lengths, total length and CRC-32C, so the mutation reaches the
//! check under test. `Image::from_decoded(..).to_bytes()` reproducing every golden is itself an
//! oracle cross-check of the production encoder.

#![allow(dead_code)]

use oce_api::Value;
use oce_graph::blob::{Decoded, crc32c};

/// A `value` as written: any tag, any payload.
#[derive(Clone, Debug)]
pub(crate) enum Val {
    Real(u64),
    Integer(i64),
    Boolean(u8),
    Str(Vec<u8>),
    Enum(Vec<u8>, u32),
    /// A bare tag byte with no payload (an invalid tag).
    Tag(u8),
}

impl Val {
    pub(crate) fn real(x: f64) -> Self {
        Self::Real(x.to_bits())
    }

    pub(crate) fn from_value(value: &Value) -> Self {
        match value {
            Value::Real(x) => Self::Real(x.to_bits()),
            Value::Integer(n) => Self::Integer(*n),
            Value::Boolean(b) => Self::Boolean(u8::from(*b)),
            Value::String(s) => Self::Str(s.as_bytes().to_vec()),
            Value::Enum { class, ordinal } => Self::Enum(
                oce_model::enum_descriptor(*class)
                    .unwrap()
                    .class_path
                    .as_bytes()
                    .to_vec(),
                *ordinal,
            ),
        }
    }
}

/// A `vtype` as written.
#[derive(Clone, Debug)]
pub(crate) enum VType {
    Real,
    Integer,
    Boolean,
    Str,
    Enum(Vec<u8>),
    Tag(u8),
}

#[derive(Clone, Debug)]
pub(crate) struct Inst {
    pub(crate) class: u32,
    pub(crate) kind: u8,
    pub(crate) state_len: u32,
    pub(crate) state_crc: u32,
    pub(crate) inputs: Vec<u32>,
    pub(crate) outputs: Vec<u32>,
    pub(crate) params: Vec<(Vec<u8>, Val)>,
}

#[derive(Clone, Debug)]
pub(crate) struct In {
    pub(crate) path: Vec<u8>,
    pub(crate) vtype: VType,
    /// `None` absent; `Some((tag, value))` writes the optional tag then the value.
    pub(crate) min: Option<Val>,
    pub(crate) max: Option<Val>,
    pub(crate) targets: Vec<u32>,
    pub(crate) payload: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(crate) struct Out {
    pub(crate) path: Vec<u8>,
    pub(crate) source: u32,
    pub(crate) payload: Vec<u8>,
}

/// Count overrides: `Some(n)` writes `n` in place of the list's real length.
#[derive(Clone, Debug, Default)]
pub(crate) struct Counts {
    pub(crate) classes: Option<u32>,
    pub(crate) blocks: Option<u32>,
    pub(crate) connectors: Option<u32>,
    pub(crate) schedule: Option<u32>,
    pub(crate) connections: Option<u32>,
    pub(crate) inputs: Option<u32>,
    pub(crate) outputs: Option<u32>,
}

#[derive(Clone, Debug)]
pub(crate) struct Image {
    pub(crate) magic: [u8; 4],
    pub(crate) format: u16,
    pub(crate) profile: u16,
    pub(crate) version: [u16; 3],
    pub(crate) flags: u16,
    pub(crate) content_id: [u8; 32],
    pub(crate) classes: Vec<(Vec<u8>, u64)>,
    pub(crate) connectors: u32,
    pub(crate) instances: Vec<Inst>,
    pub(crate) schedule: Vec<u32>,
    pub(crate) connections: Vec<(u32, u32)>,
    pub(crate) inputs: Vec<In>,
    pub(crate) outputs: Vec<Out>,
    pub(crate) counts: Counts,
    /// Bytes appended to each section's payload (index 0 is section 1).
    pub(crate) section_tail: [Vec<u8>; 5],
    /// Bytes between the last section and the trailer.
    pub(crate) body_tail: Vec<u8>,
}

impl Image {
    /// An empty image with this build's header fields.
    pub(crate) fn new() -> Self {
        let version = oce_graph::blob::ENGINE_VERSION;
        Self {
            magic: *b"OCEB",
            format: 2,
            profile: 1,
            version: [version.major, version.minor, version.patch],
            flags: 0,
            content_id: [0; 32],
            classes: Vec::new(),
            connectors: 0,
            instances: Vec::new(),
            schedule: Vec::new(),
            connections: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            counts: Counts::default(),
            section_tail: Default::default(),
            body_tail: Vec::new(),
        }
    }

    /// Rebuild a decoded executable as an image, deriving class hashes, kinds, state lengths and
    /// initial-state CRCs independently of the production encoder.
    pub(crate) fn from_decoded(decoded: &Decoded<'_>) -> Self {
        let mut image = Self::new();
        image.content_id = decoded.content_id.unwrap_or([0; 32]);
        let mut paths: Vec<&str> = decoded
            .model
            .blocks
            .iter()
            .map(|b| b.class_iri.as_ref())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        image.classes = paths
            .iter()
            .map(|p| {
                (
                    p.as_bytes().to_vec(),
                    oce_blocks::class_signature_hash(p).unwrap(),
                )
            })
            .collect();
        image.connectors = u32::try_from(decoded.model.connectors.len()).unwrap();
        for (block, instance) in decoded.model.blocks.iter().zip(&decoded.blocks) {
            let stateful = instance.kind() == oce_blocks::BlockKind::Stateful;
            let mut region = vec![0u64; instance.state_len()];
            instance.init_state(&mut region, &block.params);
            image.instances.push(Inst {
                class: u32::try_from(
                    paths
                        .iter()
                        .position(|p| *p == block.class_iri.as_ref())
                        .unwrap(),
                )
                .unwrap(),
                kind: u8::from(stateful),
                state_len: u32::try_from(instance.state_len()).unwrap(),
                state_crc: if stateful { words_crc(&region) } else { 0 },
                inputs: block.inputs.iter().map(|c| c.0).collect(),
                outputs: block.outputs.iter().map(|c| c.0).collect(),
                params: block
                    .params
                    .values
                    .iter()
                    .map(|(n, v)| (n.as_bytes().to_vec(), Val::from_value(v)))
                    .collect(),
            });
        }
        image.schedule = decoded.schedule.order.iter().map(|b| b.0).collect();
        let mut connections: Vec<(u32, u32)> = decoded
            .model
            .connections
            .iter()
            .map(|c| (c.from.0, c.to.0))
            .collect();
        connections.sort_by_key(|&(from, to)| (to, from));
        image.connections = connections;
        image.inputs = decoded
            .inputs
            .iter()
            .map(|input| In {
                path: input.path.as_bytes().to_vec(),
                vtype: match input.value_type {
                    oce_api::ValueType::Real => VType::Real,
                    oce_api::ValueType::Integer => VType::Integer,
                    oce_api::ValueType::Boolean => VType::Boolean,
                    oce_api::ValueType::String => VType::Str,
                    oce_api::ValueType::Enum(_) => unreachable!("decoder refuses Enum inputs"),
                },
                min: input.min.as_ref().map(Val::from_value),
                max: input.max.as_ref().map(Val::from_value),
                targets: input.targets.iter().map(|c| c.0).collect(),
                payload: input.semantic_point.to_vec(),
            })
            .collect();
        image.outputs = decoded
            .outputs
            .iter()
            .map(|output| Out {
                path: output.path.as_bytes().to_vec(),
                source: output.source.0,
                payload: output.semantic_point.to_vec(),
            })
            .collect();
        image
    }

    /// The five section payloads, in order.
    pub(crate) fn sections(&self) -> [Vec<u8>; 5] {
        let mut classes = Vec::new();
        u32_(&mut classes, self.counts.classes, self.classes.len());
        for (path, hash) in &self.classes {
            bytes_(&mut classes, path);
            classes.extend_from_slice(&hash.to_le_bytes());
        }

        let mut instances = Vec::new();
        u32_(&mut instances, self.counts.blocks, self.instances.len());
        instances.extend_from_slice(
            &self
                .counts
                .connectors
                .unwrap_or(self.connectors)
                .to_le_bytes(),
        );
        for inst in &self.instances {
            instances.extend_from_slice(&inst.class.to_le_bytes());
            instances.push(inst.kind);
            instances.extend_from_slice(&inst.state_len.to_le_bytes());
            instances.extend_from_slice(&inst.state_crc.to_le_bytes());
            for ports in [&inst.inputs, &inst.outputs] {
                u32_(&mut instances, None, ports.len());
                for id in ports {
                    instances.extend_from_slice(&id.to_le_bytes());
                }
            }
            u32_(&mut instances, None, inst.params.len());
            for (name, value) in &inst.params {
                bytes_(&mut instances, name);
                val(&mut instances, value);
            }
        }

        let mut schedule = Vec::new();
        u32_(&mut schedule, self.counts.schedule, self.schedule.len());
        for id in &self.schedule {
            schedule.extend_from_slice(&id.to_le_bytes());
        }

        let mut connections = Vec::new();
        u32_(
            &mut connections,
            self.counts.connections,
            self.connections.len(),
        );
        for (from, to) in &self.connections {
            connections.extend_from_slice(&from.to_le_bytes());
            connections.extend_from_slice(&to.to_le_bytes());
        }

        let mut io = Vec::new();
        u32_(&mut io, self.counts.inputs, self.inputs.len());
        for input in &self.inputs {
            bytes_(&mut io, &input.path);
            match &input.vtype {
                VType::Real => io.push(0),
                VType::Integer => io.push(1),
                VType::Boolean => io.push(2),
                VType::Str => io.push(3),
                VType::Enum(path) => {
                    io.push(4);
                    bytes_(&mut io, path);
                }
                VType::Tag(tag) => io.push(*tag),
            }
            for bound in [&input.min, &input.max] {
                match bound {
                    None => io.push(0),
                    Some(value) => {
                        io.push(1);
                        val(&mut io, value);
                    }
                }
            }
            u32_(&mut io, None, input.targets.len());
            for target in &input.targets {
                io.extend_from_slice(&target.to_le_bytes());
            }
            bytes_(&mut io, &input.payload);
        }
        u32_(&mut io, self.counts.outputs, self.outputs.len());
        for output in &self.outputs {
            bytes_(&mut io, &output.path);
            io.extend_from_slice(&output.source.to_le_bytes());
            bytes_(&mut io, &output.payload);
        }

        let mut sections = [classes, instances, schedule, connections, io];
        for (section, tail) in sections.iter_mut().zip(&self.section_tail) {
            section.extend_from_slice(tail);
        }
        sections
    }

    /// The whole blob: header, the five framed sections, the body tail and a correct trailer.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let sections = self.sections();
        let mut framed = Vec::new();
        for (index, payload) in sections.iter().enumerate() {
            framed.extend_from_slice(&(index as u32 + 1).to_le_bytes());
            framed.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
            framed.extend_from_slice(payload);
        }
        framed.extend_from_slice(&self.body_tail);
        self.assemble(&framed)
    }

    /// The header around `body`, with the true total length and a correct trailer.
    pub(crate) fn assemble(&self, body: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.magic);
        bytes.extend_from_slice(&self.format.to_le_bytes());
        bytes.extend_from_slice(&self.profile.to_le_bytes());
        for part in self.version {
            bytes.extend_from_slice(&part.to_le_bytes());
        }
        bytes.extend_from_slice(&self.flags.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&self.content_id);
        bytes.extend_from_slice(body);
        reseal(&mut bytes);
        bytes
    }
}

/// Rewrite a blob body's total length (header offset 16) and append its CRC-32C trailer. `body`
/// is everything before the trailer.
pub(crate) fn reseal(body: &mut Vec<u8>) {
    let total = u32::try_from(body.len() + 4).unwrap();
    if body.len() >= 20 {
        body[16..20].copy_from_slice(&total.to_le_bytes());
    }
    let crc = crc32c(body);
    body.extend_from_slice(&crc.to_le_bytes());
}

/// Replace a whole blob's trailer after editing bytes in place (length unchanged).
pub(crate) fn rechecksum(blob: &mut Vec<u8>) {
    blob.truncate(blob.len() - 4);
    reseal(blob);
}

pub(crate) fn words_crc(words: &[u64]) -> u32 {
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    crc32c(&bytes)
}

fn u32_(out: &mut Vec<u8>, over: Option<u32>, len: usize) {
    out.extend_from_slice(&over.unwrap_or(u32::try_from(len).unwrap()).to_le_bytes());
}

fn bytes_(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
    out.extend_from_slice(bytes);
}

fn val(out: &mut Vec<u8>, value: &Val) {
    match value {
        Val::Real(bits) => {
            out.push(0);
            out.extend_from_slice(&bits.to_le_bytes());
        }
        Val::Integer(n) => {
            out.push(1);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Val::Boolean(b) => {
            out.push(2);
            out.push(*b);
        }
        Val::Str(s) => {
            out.push(3);
            bytes_(out, s);
        }
        Val::Enum(path, ordinal) => {
            out.push(4);
            bytes_(out, path);
            out.extend_from_slice(&ordinal.to_le_bytes());
        }
        Val::Tag(tag) => out.push(*tag),
    }
}
