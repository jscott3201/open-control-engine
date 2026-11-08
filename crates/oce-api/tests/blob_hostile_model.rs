//! Hostile compiled-schedule blob models (GitHub #400): well-framed blobs whose classes,
//! instances, connections, I/O, schedule or state are wrong. Every one is a typed refusal naming
//! the index it concerns; positive controls show each hand-built model decodes when correct.

#[path = "support/blob_image.rs"]
mod blob_image;

use std::sync::Arc;

use blob_image::{Image, In, Inst, Out, VType, Val, words_crc};
use oce_api::{Engine, EngineStateError, OcError, Value};
use oce_graph::blob::{
    BlobError, BlobSource, ConnectionFault, ConnectorFault, EncodeFault, InputFault, InstanceField,
    OutputFault, ScheduleFault, decode, encode,
};
use oce_graph::{EvalContext, MAX_STATE_WORDS, eval_tick};
use oce_model::ParamTable;

fn class(path: &str) -> (Vec<u8>, u64) {
    (
        path.as_bytes().to_vec(),
        oce_blocks::class_signature_hash(path).unwrap(),
    )
}

/// One instance of `path` with `params`, its kind, state length and initial-state CRC computed
/// from the registry.
fn inst(class: u32, path: &str, params: &[(&str, Value)], inputs: &[u32], outputs: &[u32]) -> Inst {
    let table = ParamTable {
        values: params
            .iter()
            .map(|(n, v)| (Arc::from(*n), v.clone()))
            .collect(),
    };
    let block = (oce_blocks::lookup(path).unwrap().make)(&table);
    let stateful = block.kind() == oce_blocks::BlockKind::Stateful;
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &table);
    Inst {
        class,
        kind: u8::from(stateful),
        state_len: u32::try_from(block.state_len()).unwrap(),
        state_crc: if stateful { words_crc(&region) } else { 0 },
        inputs: inputs.to_vec(),
        outputs: outputs.to_vec(),
        params: params
            .iter()
            .map(|(n, v)| (n.as_bytes().to_vec(), Val::from_value(v)))
            .collect(),
    }
}

fn out(path: &str, source: u32) -> Out {
    Out {
        path: path.as_bytes().to_vec(),
        source,
        payload: Vec::new(),
    }
}

fn input(path: &str, vtype: VType, targets: &[u32]) -> In {
    In {
        path: path.as_bytes().to_vec(),
        vtype,
        min: None,
        max: None,
        targets: targets.to_vec(),
        payload: Vec::new(),
    }
}

/// `Constant(k = 1.5).y` (c0) → `Abs.u` (c1); `Abs.y` (c2) is the boundary output.
fn chain() -> Image {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.Abs"), class("CDL.Reals.Sources.Constant")];
    image.connectors = 3;
    image.instances = vec![
        inst(
            1,
            "CDL.Reals.Sources.Constant",
            &[("k", Value::Real(-1.5))],
            &[],
            &[0],
        ),
        inst(0, "CDL.Reals.Abs", &[], &[1], &[2]),
    ];
    image.schedule = vec![0, 1];
    image.connections = vec![(0, 1)];
    image.outputs = vec![out("urn:test:y", 2)];
    image
}

/// `Add(u1 = boundary input a, u2 = Constant)`: c0 const y, c1 u1, c2 u2, c3 y.
fn adder() -> Image {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.Add"), class("CDL.Reals.Sources.Constant")];
    image.connectors = 4;
    image.instances = vec![
        inst(
            1,
            "CDL.Reals.Sources.Constant",
            &[("k", Value::Real(2.0))],
            &[],
            &[0],
        ),
        inst(0, "CDL.Reals.Add", &[], &[1, 2], &[3]),
    ];
    image.schedule = vec![0, 1];
    image.connections = vec![(0, 2)];
    image.inputs = vec![input("urn:test:a", VType::Real, &[1])];
    image.outputs = vec![out("urn:test:y", 3)];
    image
}

/// `Constant.y` (c0) → `UnitDelay.u` (c1); `UnitDelay.y` (c2) is the output.
fn delay() -> Image {
    let mut image = Image::new();
    image.classes = vec![
        class("CDL.Discrete.UnitDelay"),
        class("CDL.Reals.Sources.Constant"),
    ];
    image.connectors = 3;
    image.instances = vec![
        inst(
            1,
            "CDL.Reals.Sources.Constant",
            &[("k", Value::Real(4.0))],
            &[],
            &[0],
        ),
        inst(
            0,
            "CDL.Discrete.UnitDelay",
            &[
                ("samplePeriod", Value::Real(1.0)),
                ("y_start", Value::Real(3.0)),
            ],
            &[1],
            &[2],
        ),
    ];
    image.schedule = vec![0, 1];
    image.connections = vec![(0, 1)];
    image.outputs = vec![out("urn:test:y", 2)];
    image
}

fn refusal(image: &Image) -> BlobError {
    match decode(&image.to_bytes()) {
        Err(error) => error,
        Ok(_) => panic!("expected a refusal"),
    }
}

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
fn boundary_input_defects_are_refused() {
    let base = adder();
    let edit = |f: &dyn Fn(&mut Image)| {
        let mut image = base.clone();
        f(&mut image);
        image
    };
    let cases: Vec<(InputFault, Image)> = vec![
        (
            InputFault::EnumType,
            edit(&|i| i.inputs[0].vtype = VType::Enum(b"CDL.Types.SimpleController".to_vec())),
        ),
        (
            InputFault::NoTargets,
            edit(&|i| i.inputs[0].targets.clear()),
        ),
        (
            InputFault::TargetOutOfRange,
            edit(&|i| i.inputs[0].targets = vec![9]),
        ),
        (
            InputFault::TargetNotInput,
            edit(&|i| i.inputs[0].targets = vec![0]),
        ),
        (
            InputFault::TargetTypeMismatch,
            edit(&|i| i.inputs[0].vtype = VType::Integer),
        ),
        (
            InputFault::TargetTypeMismatch,
            edit(&|i| i.inputs[0].vtype = VType::Str),
        ),
        (
            InputFault::TargetShared,
            edit(&|i| i.inputs.push(input("urn:test:b", VType::Real, &[1]))),
        ),
        (
            InputFault::BoundTypeMismatch,
            edit(&|i| i.inputs[0].min = Some(Val::Integer(0))),
        ),
        (
            InputFault::BoundTypeMismatch,
            edit(&|i| i.inputs[0].max = Some(Val::Boolean(1))),
        ),
    ];
    for (fault, image) in cases {
        // The shared target is reported on the second input that claims it.
        let input = u32::from(fault == InputFault::TargetShared);
        assert_eq!(
            refusal(&image),
            BlobError::InvalidInput { input, fault },
            "{fault:?}"
        );
    }
    let mut boolean_bound = Image::new();
    boolean_bound.classes = vec![class("CDL.Logical.Not")];
    boolean_bound.connectors = 2;
    boolean_bound.instances = vec![inst(0, "CDL.Logical.Not", &[], &[0], &[1])];
    boolean_bound.schedule = vec![0];
    boolean_bound.inputs = vec![input("urn:test:u", VType::Boolean, &[0])];
    decode(&boolean_bound.to_bytes()).unwrap();
    boolean_bound.inputs[0].min = Some(Val::Boolean(0));
    assert_eq!(
        refusal(&boolean_bound),
        BlobError::InvalidInput {
            input: 0,
            fault: InputFault::BoundTypeMismatch
        }
    );
    let mut bounded = adder();
    bounded.inputs[0].min = Some(Val::real(-1.0));
    bounded.inputs[0].max = Some(Val::real(1.0));
    let bytes = bounded.to_bytes();
    let decoded = decode(&bytes).unwrap();
    assert!(
        decoded.inputs[0]
            .min
            .as_ref()
            .unwrap()
            .bit_eq(&Value::Real(-1.0))
    );
}

#[test]
fn boundary_output_defects_are_refused() {
    let cases: Vec<(OutputFault, Image)> = vec![
        (OutputFault::SourceOutOfRange, {
            let mut i = chain();
            i.outputs[0].source = 3;
            i
        }),
        (OutputFault::SourceNotOutput, {
            let mut i = chain();
            i.outputs[0].source = 1;
            i
        }),
        (OutputFault::DuplicatePath, {
            let mut i = chain();
            i.outputs.push(out("urn:test:y", 0));
            i
        }),
    ];
    for (fault, image) in cases {
        let error = refusal(&image);
        assert!(
            matches!(error, BlobError::InvalidOutput { fault: f, .. } if f == fault),
            "{fault:?}: {error:?}"
        );
    }
}

#[test]
fn the_schedule_must_be_a_permutation_respecting_emit_before() {
    let cases: Vec<(ScheduleFault, Vec<u32>)> = vec![
        (ScheduleFault::CountMismatch, vec![0]),
        (ScheduleFault::CountMismatch, vec![0, 1, 1]),
        (ScheduleFault::BlockOutOfRange, vec![0, 2]),
        (ScheduleFault::Duplicate, vec![0, 0]),
        (ScheduleFault::EmitBefore, vec![1, 0]),
    ];
    for (fault, order) in cases {
        let mut image = chain();
        image.schedule = order.clone();
        let error = refusal(&image);
        assert!(
            matches!(error, BlobError::InvalidSchedule { fault: f, .. } if f == fault),
            "{order:?}: {error:?}"
        );
    }
    // A loop breaker reads its input only in the state update, so it may precede its producer.
    let mut cut = delay();
    cut.schedule = vec![1, 0];
    decode(&cut.to_bytes()).unwrap();
}

#[test]
fn a_changed_initial_state_crc_is_refused() {
    let mut image = delay();
    let expected = image.instances[1].state_crc ^ 1;
    image.instances[1].state_crc = expected;
    assert_eq!(
        refusal(&image),
        BlobError::InitialStateMismatch {
            block: 1,
            expected,
            found: expected ^ 1
        }
    );
}

/// `TriggeredMovingMean(n)` owns `state_len(n)` words; its inputs come from two boundary inputs.
fn moving_mean(n: i64) -> Image {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Discrete.TriggeredMovingMean")];
    image.connectors = 3;
    image.instances = vec![inst(
        0,
        "CDL.Discrete.TriggeredMovingMean",
        &[("n", Value::Integer(n))],
        &[0, 1],
        &[2],
    )];
    image.schedule = vec![0];
    image.inputs = vec![
        input("urn:test:trigger", VType::Boolean, &[1]),
        input("urn:test:u", VType::Real, &[0]),
    ];
    image
}

#[test]
fn state_at_the_budget_is_accepted_and_one_word_past_it_is_refused() {
    let words = |n: i64| moving_mean(n).instances[0].state_len as usize;
    let overhead = words(1) - 1;
    let at_limit = i64::try_from(MAX_STATE_WORDS - overhead).unwrap();
    assert_eq!(words(at_limit), MAX_STATE_WORDS);
    let bytes = moving_mean(at_limit).to_bytes();
    let decoded = decode(&bytes).unwrap();
    assert_eq!(decoded.state.words.len(), MAX_STATE_WORDS);
    drop(decoded);
    assert_eq!(
        refusal(&moving_mean(at_limit + 1)),
        BlobError::StateTooLarge {
            block: 0,
            requested: MAX_STATE_WORDS as u64 + 1
        }
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
