//! Hostile compiled-schedule blob models (GitHub #400): well-framed blobs whose classes,
//! instances, parameters, connectors or connections are wrong, and what the encoder and the facade
//! refuse. Every one is a typed refusal naming the index it concerns; positive controls show each
//! hand-built model decodes when correct.

#[path = "support/blob_image.rs"]
mod blob_image;
#[path = "support/blob_models.rs"]
mod blob_models;

use blob_image::{Image, VType, Val};
use blob_models::{adder, chain, class, delay, input, inst, refusal};
use oce_api::{Engine, EngineStateError, OcError, Value};
use oce_graph::blob::{
    BlobError, BlobSource, ConnectionFault, ConnectorFault, EncodeFault, InstanceField,
    OutputFault, decode, encode,
};
use oce_graph::{EvalContext, eval_tick};

#[test]
fn the_hand_built_models_decode_and_tick() {
    for image in [chain(), adder(), delay()] {
        let bytes = image.to_bytes();
        let decoded = decode(&bytes).unwrap();
        assert!(encode(&decoded.source()).unwrap() == bytes, "re-encode");
    }
    let bytes = chain().to_bytes();
    let mut decoded = decode(&bytes).unwrap();
    let mut ctx = EvalContext {
        model: &decoded.model,
        schedule: &decoded.schedule,
        blocks: &decoded.blocks,
        diagnostics: &oce_blocks::NoopDiagnostics,
        state: &mut decoded.state,
    };
    eval_tick(&mut ctx, 0.0);
    assert_eq!(
        decoded.state.values[2].as_real().unwrap().to_bits(),
        1.5f64.to_bits()
    );
}

#[test]
fn unknown_classes_and_hash_mismatches_name_the_class() {
    let mut unknown = chain();
    unknown.classes[0].0 = b"CDL.Reals.Abz".to_vec();
    assert_eq!(refusal(&unknown), BlobError::UnknownClass { class: 0 });

    let mut mismatch = chain();
    let expected = mismatch.classes[1].1;
    mismatch.classes[1].1 ^= 1;
    assert_eq!(
        refusal(&mismatch),
        BlobError::ClassSignatureMismatch {
            class: 1,
            expected,
            found: expected ^ 1
        }
    );

    let mut unused = chain();
    unused.classes.insert(1, class("CDL.Reals.Add"));
    unused.instances[0].class = 2;
    assert_eq!(refusal(&unused), BlobError::UnusedClass { class: 1 });
}

#[test]
fn an_instance_that_disagrees_with_its_constructed_block_is_refused() {
    let block = 1;
    let cases: Vec<(InstanceField, Image)> = vec![
        (InstanceField::ClassIndex, {
            let mut i = chain();
            i.instances[1].class = 2;
            i
        }),
        (InstanceField::Kind, {
            let mut i = chain();
            i.instances[1].kind = 1;
            i
        }),
        (InstanceField::StateLen, {
            let mut i = delay();
            i.instances[1].state_len += 1;
            i
        }),
        (InstanceField::StateCrc, {
            let mut i = chain();
            i.instances[1].state_crc = 7;
            i
        }),
        (InstanceField::InputArity, {
            let mut i = chain();
            i.connectors = 4;
            i.instances[1].inputs = vec![1, 3];
            i
        }),
        (InstanceField::OutputArity, {
            let mut i = chain();
            i.instances[1].outputs = vec![];
            i
        }),
    ];
    for (field, image) in cases {
        assert_eq!(
            refusal(&image),
            BlobError::InstanceMismatch { block, field },
            "{field:?}"
        );
    }
}

#[test]
fn parameter_defects_are_refused() {
    let mut duplicate = chain();
    duplicate.instances[0].params = vec![
        (b"k".to_vec(), Val::real(1.0)),
        (b"j".to_vec(), Val::real(1.0)),
        (b"k".to_vec(), Val::real(2.0)),
    ];
    assert_eq!(
        refusal(&duplicate),
        BlobError::DuplicateParameter {
            block: 0,
            parameter: 2
        }
    );
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut non_finite = chain();
        non_finite.instances[0].params[0].1 = Val::real(x);
        assert_eq!(
            refusal(&non_finite),
            BlobError::NonFiniteParameter {
                block: 0,
                parameter: 0
            }
        );
    }
    let mut unknown = chain();
    unknown.instances[0].params[0].1 = Val::Enum(b"CDL.Types.NotAType".to_vec(), 1);
    assert!(matches!(
        refusal(&unknown),
        BlobError::UnknownEnumClass { .. }
    ));
    // A known class spelled by its unqualified or fully qualified name is not the canonical path.
    let mut spelled = chain();
    spelled.instances[0].params[0].1 = Val::Enum(
        b"Buildings.Controls.OBC.CDL.Types.SimpleController".to_vec(),
        1,
    );
    assert!(matches!(
        refusal(&spelled),
        BlobError::UnknownEnumClass { .. }
    ));
    for ordinal in [0u32, 5, u32::MAX] {
        let mut range = chain();
        range.instances[0].params[0].1 = Val::Enum(b"CDL.Types.SimpleController".to_vec(), ordinal);
        assert!(
            matches!(refusal(&range), BlobError::EnumOrdinalOutOfRange { ordinal: o, .. } if o == ordinal),
            "{ordinal}"
        );
    }
    let mut in_range = chain();
    in_range.instances[0].params[0].1 = Val::Enum(b"CDL.Types.SimpleController".to_vec(), 4);
    decode(&in_range.to_bytes()).unwrap();
}

#[test]
fn connector_ids_must_cover_the_arena_exactly_once() {
    let mut duplicated = chain();
    duplicated.instances[1].inputs = vec![0];
    assert_eq!(
        refusal(&duplicated),
        BlobError::InvalidConnector {
            connector: 0,
            fault: ConnectorFault::Duplicated
        }
    );
    let mut out_of_range = chain();
    out_of_range.instances[1].inputs = vec![3];
    assert_eq!(
        refusal(&out_of_range),
        BlobError::InvalidConnector {
            connector: 3,
            fault: ConnectorFault::OutOfRange
        }
    );
    let mut missing = chain();
    missing.connectors = 4;
    assert_eq!(
        refusal(&missing),
        BlobError::InvalidConnector {
            connector: 3,
            fault: ConnectorFault::Missing
        }
    );
}

#[test]
fn connections_must_be_typed_output_to_input_edges_between_blocks() {
    let cases: Vec<(ConnectionFault, Image)> = vec![
        (ConnectionFault::OutOfRange, {
            let mut i = chain();
            i.connections = vec![(9, 1)];
            i
        }),
        (ConnectionFault::ProducerNotOutput, {
            let mut i = chain();
            i.connections = vec![(1, 1)];
            i
        }),
        (ConnectionFault::ConsumerNotInput, {
            let mut i = chain();
            i.connections = vec![(0, 2)];
            i
        }),
        (ConnectionFault::KindMismatch, {
            let mut i = Image::new();
            i.classes = vec![
                class("CDL.Logical.Not"),
                class("CDL.Reals.Sources.Constant"),
            ];
            i.connectors = 3;
            i.instances = vec![
                inst(1, "CDL.Reals.Sources.Constant", &[], &[], &[0]),
                inst(0, "CDL.Logical.Not", &[], &[1], &[2]),
            ];
            i.schedule = vec![0, 1];
            i.connections = vec![(0, 1)];
            i
        }),
        (ConnectionFault::SameBlock, {
            let mut i = delay();
            i.connections = vec![(2, 1)];
            i
        }),
        (ConnectionFault::ConsumerIsInputTarget, {
            let mut i = chain();
            i.inputs = vec![input("urn:test:u", VType::Real, &[1])];
            i
        }),
        (ConnectionFault::MultipleDrivers, {
            let mut i = adder();
            i.inputs.clear();
            i.connections = vec![(0, 1), (0, 1), (0, 2)];
            i
        }),
    ];
    for (fault, image) in cases {
        let error = refusal(&image);
        assert!(
            matches!(error, BlobError::InvalidConnection { fault: f, .. } if f == fault),
            "{fault:?}: {error:?}"
        );
    }
}

#[test]
fn every_input_connector_needs_a_driver_or_a_boundary_input() {
    let mut undriven = chain();
    undriven.connections.clear();
    assert_eq!(
        refusal(&undriven),
        BlobError::UndrivenInput { connector: 1 }
    );
}

#[test]
fn the_encoder_refuses_what_it_cannot_represent() {
    let bytes = chain().to_bytes();
    let decoded = decode(&bytes).unwrap();

    let mut outputs = decoded.outputs.clone();
    outputs.push(outputs[0].clone());
    let duplicate = BlobSource {
        outputs: &outputs,
        ..decoded.source()
    };
    assert_eq!(
        encode(&duplicate).err(),
        Some(BlobError::InvalidOutput {
            output: 1,
            fault: OutputFault::DuplicatePath
        })
    );

    let mut model = decoded.model.clone();
    model.blocks[0].params.values[0].1 = Value::Real(f64::NAN);
    let non_finite = BlobSource {
        model: &model,
        ..decoded.source()
    };
    assert_eq!(
        encode(&non_finite).err(),
        Some(BlobError::Unencodable {
            index: 0,
            fault: EncodeFault::NonFiniteParameter
        })
    );

    let mut model = decoded.model.clone();
    model.blocks[1].id = oce_model::BlockId(5);
    let sparse = BlobSource {
        model: &model,
        ..decoded.source()
    };
    assert_eq!(
        encode(&sparse).err(),
        Some(BlobError::Unencodable {
            index: 1,
            fault: EncodeFault::IdNotDense
        })
    );

    let adder_bytes = adder().to_bytes();
    let adder = decode(&adder_bytes).unwrap();
    let mut inputs = adder.inputs.clone();
    inputs.push(inputs[0].clone());
    let duplicate_input = BlobSource {
        inputs: &inputs,
        ..adder.source()
    };
    assert_eq!(
        encode(&duplicate_input).err(),
        Some(BlobError::Unencodable {
            index: 1,
            fault: EncodeFault::DuplicateInputPath
        })
    );
}

#[test]
fn the_facade_refuses_to_compile_without_a_model_or_with_pending_edits() {
    let engine = Engine::new();
    assert!(matches!(
        engine.compile_blob(),
        Err(OcError::State(EngineStateError::NoLoadedModel))
    ));
    let mut engine = Engine::new();
    engine
        .load_cxf(include_bytes!(
            "../../oce-cxf/tests/fixtures/minimal_loop.jsonld"
        ))
        .unwrap();
    let clean = engine.compile_blob().unwrap();
    let advanced = {
        let mut engine = Engine::new();
        engine
            .load_cxf(include_bytes!(
                "../../oce-cxf/tests/fixtures/minimal_loop.jsonld"
            ))
            .unwrap();
        let frame = engine
            .prepare_frame(
                0.0,
                &[("http://example.org#MinLoop.uSet", Value::Real(1.0))],
            )
            .unwrap();
        engine.execute_frame(frame).unwrap();
        engine.compile_blob().unwrap()
    };
    assert_eq!(advanced, clean, "executed frames do not change the blob");
    engine.halt().unwrap();
    engine
        .set_param("http://example.org#MinLoop.con.k", Value::Real(3.0))
        .unwrap();
    assert!(matches!(
        engine.compile_blob(),
        Err(OcError::State(EngineStateError::PendingParameterEdits))
    ));
    engine.resume().unwrap();
    let edited = engine.compile_blob().unwrap();
    assert_ne!(edited.bytes, clean.bytes, "a resumed edit changes the blob");
}
