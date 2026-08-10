//! Independent revision-1 byte and fingerprint vector.

use crate::state::{
    BlockKey, BlockManifestEntry, ConnectorKey, ConnectorManifestEntry, EnumManifestEntry,
    ExecutionManifest, Portability, StateImage, WireDir, WireValue, WireValueType,
};
use crate::{EngineStateSnapshot, state_codec};
use oce_blocks::BlockKind;

struct ExpectedWriter(Vec<u8>);

impl ExpectedWriter {
    fn byte(&mut self, value: u8) {
        self.0.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn u128(&mut self, value: u128) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn string(&mut self, value: &str) {
        self.u32(value.len() as u32);
        self.0.extend_from_slice(value.as_bytes());
    }

    fn block_key(&mut self, key: &BlockKey) {
        match key {
            BlockKey::Authored(value) => {
                self.byte(0);
                self.string(value);
            }
            BlockKey::PassThrough {
                input_path,
                output_path,
            } => {
                self.byte(1);
                self.string(input_path);
                self.string(output_path);
            }
            BlockKey::Dense(_) => panic!("dense key in durable vector"),
        }
    }

    fn connector_key(&mut self, key: &ConnectorKey) {
        self.block_key(&key.owner);
        self.byte(match key.direction {
            WireDir::In => 0,
            WireDir::Out => 1,
        });
        self.u32(key.port_index);
    }

    fn value_type(&mut self, value: &WireValueType) {
        match value {
            WireValueType::Real => self.byte(0),
            WireValueType::Integer => self.byte(1),
            WireValueType::Boolean => self.byte(2),
            WireValueType::String => self.byte(3),
            WireValueType::Enum(class_path) => {
                self.byte(4);
                self.string(class_path);
            }
        }
    }

    fn value(&mut self, value: &WireValue) {
        match value {
            WireValue::Real(bits) => {
                self.byte(0);
                self.u64(*bits);
            }
            WireValue::Integer(value) => {
                self.byte(1);
                self.0.extend_from_slice(&value.to_le_bytes());
            }
            WireValue::Boolean(value) => {
                self.byte(2);
                self.byte(u8::from(*value));
            }
            WireValue::String(value) => {
                self.byte(3);
                self.string(value);
            }
            WireValue::Enum {
                class_path,
                ordinal,
            } => {
                self.byte(4);
                self.string(class_path);
                self.u32(*ordinal);
            }
        }
    }
}

fn key(direction: WireDir, port_index: u32) -> ConnectorKey {
    ConnectorKey {
        owner: BlockKey::Authored("urn:test:block".into()),
        direction,
        port_index,
    }
}

fn vector_image() -> StateImage {
    let block_key = BlockKey::Authored("urn:test:block".into());
    let pass_through_key = BlockKey::PassThrough {
        input_path: "urn:test:pass:input".into(),
        output_path: "urn:test:pass:output".into(),
    };
    let input = key(WireDir::In, 0);
    let outputs = (0..5)
        .map(|index| key(WireDir::Out, index))
        .collect::<Vec<_>>();
    let enum_path = "CDL.Types.SimpleController".to_owned();
    let mut connectors = vec![ConnectorManifestEntry {
        key: input.clone(),
        path: "urn:test:input".into(),
        declaration_order: 9,
        value_type: WireValueType::Real,
    }];
    for (index, value_type) in [
        WireValueType::Real,
        WireValueType::Integer,
        WireValueType::Boolean,
        WireValueType::String,
        WireValueType::Enum(enum_path.clone()),
    ]
    .into_iter()
    .enumerate()
    {
        connectors.push(ConnectorManifestEntry {
            key: outputs[index].clone(),
            path: format!("urn:test:output:{index}"),
            declaration_order: index as u32,
            value_type,
        });
    }
    let mut driver_of = vec![(input.clone(), outputs[0].clone())];
    driver_of.extend(outputs.iter().cloned().map(|key| (key.clone(), key)));
    driver_of.sort();
    let mut values = vec![(input.clone(), WireValue::Real(0x7ff8_0000_0000_0042))];
    values.extend([
        (outputs[0].clone(), WireValue::Real((-0.0f64).to_bits())),
        (outputs[1].clone(), WireValue::Integer(i64::MIN)),
        (outputs[2].clone(), WireValue::Boolean(true)),
        (outputs[3].clone(), WireValue::String("Grüße".into())),
        (
            outputs[4].clone(),
            WireValue::Enum {
                class_path: enum_path.clone(),
                ordinal: 2,
            },
        ),
    ]);
    StateImage {
        execution_revision: 1,
        fingerprint: 0,
        model_id: "urn:test:model:Δ".into(),
        state_t: 3.5f64.to_bits(),
        prev_t: Some(3.5f64.to_bits()),
        manifest: ExecutionManifest {
            portability: Portability::TargetBound {
                arch: std::env::consts::ARCH.into(),
                os: std::env::consts::OS.into(),
            },
            enums: vec![EnumManifestEntry {
                class_path: enum_path.clone(),
                members: vec!["P".into(), "PI".into(), "PD".into(), "PID".into()],
            }],
            blocks: vec![
                BlockManifestEntry {
                    key: block_key.clone(),
                    class_path: "CDL.Reals.Sin".into(),
                    kind: BlockKind::Stateful,
                    state_revision: 1,
                    state_len: 1,
                    params: vec![
                        ("bool".into(), WireValue::Boolean(false)),
                        (
                            "enum".into(),
                            WireValue::Enum {
                                class_path: enum_path,
                                ordinal: 1,
                            },
                        ),
                        ("int".into(), WireValue::Integer(i64::MAX)),
                        ("real".into(), WireValue::Real(f64::INFINITY.to_bits())),
                        ("string".into(), WireValue::String("λ".into())),
                    ],
                    inputs: vec![input.clone()],
                    outputs: outputs.clone(),
                },
                BlockManifestEntry {
                    key: pass_through_key.clone(),
                    class_path: "urn:oce:lowering#PassThrough.Real".into(),
                    kind: BlockKind::Algebraic,
                    state_revision: 0,
                    state_len: 0,
                    params: Vec::new(),
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                },
            ],
            connectors: connectors.clone(),
            connections: vec![(outputs[0].clone(), input.clone())],
            schedule: vec![block_key.clone(), pass_through_key],
            connector_order: connectors.iter().map(|entry| entry.key.clone()).collect(),
            driver_of,
            state_slots: vec![(block_key, 0, 1)],
            external_inputs: vec![input],
            boundary_outputs: vec![("urn:test:boundary".into(), outputs[0].clone())],
        },
        values,
        words: vec![0x0123_4567_89ab_cdef],
    }
}

fn expected_manifest(manifest: &ExecutionManifest) -> Vec<u8> {
    let mut out = ExpectedWriter(Vec::new());
    match &manifest.portability {
        Portability::CrossPlatform => out.byte(0),
        Portability::TargetBound { arch, os } => {
            out.byte(1);
            out.string(arch);
            out.string(os);
        }
    }
    out.u32(manifest.enums.len() as u32);
    for entry in &manifest.enums {
        out.string(&entry.class_path);
        out.u32(entry.members.len() as u32);
        for member in &entry.members {
            out.string(member);
        }
    }
    out.u32(manifest.blocks.len() as u32);
    for block in &manifest.blocks {
        out.block_key(&block.key);
        out.string(&block.class_path);
        out.byte(match block.kind {
            BlockKind::Algebraic => 0,
            BlockKind::Stateful => 1,
        });
        out.u32(block.state_revision);
        out.u64(block.state_len);
        out.u32(block.params.len() as u32);
        for (name, value) in &block.params {
            out.string(name);
            out.value(value);
        }
        out.u32(block.inputs.len() as u32);
        for key in &block.inputs {
            out.connector_key(key);
        }
        out.u32(block.outputs.len() as u32);
        for key in &block.outputs {
            out.connector_key(key);
        }
    }
    out.u32(manifest.connectors.len() as u32);
    for connector in &manifest.connectors {
        out.connector_key(&connector.key);
        out.string(&connector.path);
        out.u32(connector.declaration_order);
        out.value_type(&connector.value_type);
    }
    out.u32(manifest.connections.len() as u32);
    for (from, to) in &manifest.connections {
        out.connector_key(from);
        out.connector_key(to);
    }
    out.u32(manifest.schedule.len() as u32);
    for key in &manifest.schedule {
        out.block_key(key);
    }
    out.u32(manifest.connector_order.len() as u32);
    for key in &manifest.connector_order {
        out.connector_key(key);
    }
    out.u32(manifest.driver_of.len() as u32);
    for (key, driver) in &manifest.driver_of {
        out.connector_key(key);
        out.connector_key(driver);
    }
    out.u32(manifest.state_slots.len() as u32);
    for (key, offset, length) in &manifest.state_slots {
        out.block_key(key);
        out.u64(*offset);
        out.u64(*length);
    }
    out.u32(manifest.external_inputs.len() as u32);
    for key in &manifest.external_inputs {
        out.connector_key(key);
    }
    out.u32(manifest.boundary_outputs.len() as u32);
    for (name, source) in &manifest.boundary_outputs {
        out.string(name);
        out.connector_key(source);
    }
    out.0
}

fn fnv(bytes: &[u8]) -> u128 {
    bytes
        .iter()
        .fold(0x6c62_272e_07bb_0142_62b8_2175_6295_c58d, |hash, byte| {
            (hash ^ u128::from(*byte)).wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b)
        })
}

fn expected_snapshot(image: &StateImage, manifest: &[u8], fingerprint: u128) -> Vec<u8> {
    let mut body = ExpectedWriter(Vec::new());
    body.u128(fingerprint);
    body.string(&image.model_id);
    body.u64(image.state_t);
    body.byte(1);
    body.u64(image.prev_t.unwrap());
    body.u64(manifest.len() as u64);
    body.0.extend_from_slice(manifest);
    body.u32(image.values.len() as u32);
    for (key, value) in &image.values {
        body.connector_key(key);
        body.value(value);
    }
    body.u64(image.words.len() as u64);
    for word in &image.words {
        body.u64(*word);
    }
    body.u32(0);

    let mut bytes = ExpectedWriter(Vec::new());
    bytes.0.extend_from_slice(b"OCESTAT\0");
    bytes.u32(1);
    bytes.u32(1);
    bytes.u64(body.0.len() as u64);
    bytes.0.extend_from_slice(&body.0);
    let checksum = fnv(&bytes.0);
    bytes.u128(checksum);
    bytes.0
}

#[test]
fn production_codec_matches_the_independent_complete_vector() {
    let mut image = vector_image();
    let manifest = expected_manifest(&image.manifest);
    let mut fingerprint_preimage = 1u32.to_le_bytes().to_vec();
    fingerprint_preimage.extend_from_slice(&manifest);
    image.fingerprint = fnv(&fingerprint_preimage);
    let expected = expected_snapshot(&image, &manifest, image.fingerprint);
    let actual = state_codec::encode_snapshot(&image, false).unwrap();
    assert_eq!(actual, expected);
    let decoded = EngineStateSnapshot::from_bytes(&expected).unwrap();
    assert_eq!(decoded.as_bytes(), expected);
}

#[test]
fn populated_portable_vector_is_target_independent() {
    let mut image = vector_image();
    image.manifest.portability = Portability::CrossPlatform;
    image.manifest.blocks[0].class_path = "CDL.Reals.Add".into();
    let manifest = expected_manifest(&image.manifest);
    let mut fingerprint_preimage = 1u32.to_le_bytes().to_vec();
    fingerprint_preimage.extend_from_slice(&manifest);
    image.fingerprint = fnv(&fingerprint_preimage);
    let expected = expected_snapshot(&image, &manifest, image.fingerprint);
    let actual = state_codec::encode_snapshot(&image, false).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        fnv(&actual),
        30_166_327_748_914_738_958_954_550_944_390_124_402
    );
    EngineStateSnapshot::from_bytes(&actual).unwrap();
}

fn encoded_vector(mut image: StateImage) -> Vec<u8> {
    let manifest = expected_manifest(&image.manifest);
    let mut fingerprint_preimage = 1u32.to_le_bytes().to_vec();
    fingerprint_preimage.extend_from_slice(&manifest);
    image.fingerprint = fnv(&fingerprint_preimage);
    expected_snapshot(&image, &manifest, image.fingerprint)
}

#[test]
fn complete_vector_rejects_missing_duplicate_and_invalid_enum_values() {
    let mut missing = vector_image();
    missing.values.pop();
    assert!(EngineStateSnapshot::from_bytes(&encoded_vector(missing)).is_err());

    let mut duplicate = vector_image();
    duplicate.values.push(duplicate.values[0].clone());
    assert!(EngineStateSnapshot::from_bytes(&encoded_vector(duplicate)).is_err());

    let mut invalid_enum = vector_image();
    let (_, WireValue::Enum { ordinal, .. }) = invalid_enum.values.last_mut().unwrap() else {
        panic!("last vector value is enum");
    };
    *ordinal = 5;
    assert!(EngineStateSnapshot::from_bytes(&encoded_vector(invalid_enum)).is_err());

    let mut wrong_members = vector_image();
    wrong_members.manifest.enums[0].members.swap(0, 1);
    assert!(EngineStateSnapshot::from_bytes(&encoded_vector(wrong_members)).is_err());
}

#[test]
fn unknown_execution_revision_retains_future_catalog_descriptors_for_restore() {
    let mut image = vector_image();
    image.execution_revision = 2;
    image.manifest.portability = Portability::CrossPlatform;
    image.manifest.enums[0].members.swap(0, 1);
    let manifest = crate::state_manifest_codec::encode_manifest(&image.manifest, false).unwrap();
    image.fingerprint = crate::state_manifest::fingerprint(image.execution_revision, &manifest);
    let bytes = state_codec::encode_snapshot(&image, false).unwrap();
    let snapshot = EngineStateSnapshot::from_bytes(&bytes).unwrap();

    let mut target = crate::Engine::in_memory();
    target
        .build_model_in_memory(
            oce_model::ModelGraph::new(),
            Some("urn:test:future-revision-target"),
        )
        .unwrap();
    assert!(matches!(
        target.restore_state(&snapshot),
        Err(crate::OcError::State(
            crate::EngineStateError::IncompatibleExecution { .. }
        ))
    ));
}

#[test]
fn large_unique_future_enum_descriptor_decodes_without_quadratic_duplicate_scanning() {
    let mut image = vector_image();
    image.execution_revision = 2;
    image.manifest.portability = Portability::CrossPlatform;
    image.manifest.enums[0].members = (0..20_000)
        .map(|ordinal| std::sync::Arc::from(format!("member-{ordinal:05}")))
        .collect();
    let manifest = crate::state_manifest_codec::encode_manifest(&image.manifest, false).unwrap();
    image.fingerprint = crate::state_manifest::fingerprint(image.execution_revision, &manifest);
    let bytes = state_codec::encode_snapshot(&image, false).unwrap();
    EngineStateSnapshot::from_bytes(&bytes).unwrap();
}

#[test]
fn durable_decoder_rejects_a_dense_block_key_tag() {
    let mut bytes = encoded_vector(vector_image());
    let mut needle = vec![0];
    needle.extend_from_slice(&("urn:test:block".len() as u32).to_le_bytes());
    needle.extend_from_slice(b"urn:test:block");
    let offset = bytes
        .windows(needle.len())
        .position(|window| window == needle)
        .unwrap();
    bytes[offset] = 2;
    let trailer = bytes.len() - 16;
    let checksum = fnv(&bytes[..trailer]);
    bytes[trailer..].copy_from_slice(&checksum.to_le_bytes());
    assert!(EngineStateSnapshot::from_bytes(&bytes).is_err());
}
