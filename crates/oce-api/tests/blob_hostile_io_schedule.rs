//! Hostile compiled-schedule blob I/O, schedules and state (GitHub #400): boundary inputs and
//! outputs, bounds, emit-before (including a stateful block that feeds through), initial-state
//! CRCs, the state budget, and the target-bound flag. Every defect is a typed refusal.

// Shared helpers, declared once for this binary in its root `facade.rs`:
// `support/blob_image.rs` and `support/blob_models.rs`.
use crate::blob_image;
use crate::blob_models;

use std::sync::Arc;

use blob_image::{Image, VType, Val};
use blob_models::{adder, chain, class, delay, input, inst, moving_mean, out, refusal};
use oce_api::Value;
use oce_graph::MAX_STATE_WORDS;
use oce_graph::blob::{
    BlobError, BlobSource, EncodeFault, InputFault, OutputFault, ScheduleFault, decode, encode,
};
use oce_model::ParamTable;

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
fn non_finite_or_crossed_input_bounds_are_refused_by_decoder_and_encoder() {
    let cases: Vec<(Option<Val>, Option<Val>)> = vec![
        (Some(Val::real(f64::NAN)), None),
        (None, Some(Val::real(f64::INFINITY))),
        (Some(Val::real(f64::NEG_INFINITY)), Some(Val::real(1.0))),
        (Some(Val::real(2.0)), Some(Val::real(1.0))),
    ];
    for (min, max) in cases {
        let mut image = adder();
        image.inputs[0].min = min.clone();
        image.inputs[0].max = max.clone();
        assert_eq!(
            refusal(&image),
            BlobError::InvalidInput {
                input: 0,
                fault: InputFault::InvalidBound
            },
            "{min:?} / {max:?}"
        );
    }
    // Integer bounds cross too.
    let mut integer = Image::new();
    integer.classes = vec![class("CDL.Integers.Abs")];
    integer.connectors = 2;
    integer.instances = vec![inst(0, "CDL.Integers.Abs", &[], &[0], &[1])];
    integer.schedule = vec![0];
    integer.inputs = vec![input("urn:test:n", VType::Integer, &[0])];
    integer.inputs[0].min = Some(Val::Integer(5));
    integer.inputs[0].max = Some(Val::Integer(5));
    decode(&integer.to_bytes()).unwrap();
    integer.inputs[0].max = Some(Val::Integer(4));
    assert_eq!(
        refusal(&integer),
        BlobError::InvalidInput {
            input: 0,
            fault: InputFault::InvalidBound
        }
    );
    // A single point stays legal, and so do signed zeros, which compare equal as the facade's
    // domain check compares them.
    for (lo, hi) in [(1.5, 1.5), (0.0, -0.0), (-0.0, 0.0)] {
        let mut point = adder();
        point.inputs[0].min = Some(Val::real(lo));
        point.inputs[0].max = Some(Val::real(hi));
        decode(&point.to_bytes()).unwrap();
    }

    // The encoder refuses to emit what the decoder refuses.
    let bytes = adder().to_bytes();
    let decoded = decode(&bytes).unwrap();
    for (min, max) in [
        (Some(Value::Real(f64::NAN)), None),
        (Some(Value::Real(3.0)), Some(Value::Real(2.0))),
    ] {
        let mut inputs = decoded.inputs.clone();
        inputs[0].min = min;
        inputs[0].max = max;
        let source = BlobSource {
            inputs: &inputs,
            ..decoded.source()
        };
        assert_eq!(
            encode(&source).err(),
            Some(BlobError::Unencodable {
                index: 0,
                fault: EncodeFault::InvalidInputBound
            })
        );
    }
}

#[test]
fn a_stateful_block_whose_input_feeds_through_must_follow_its_producer() {
    // Hysteresis is stateful, yet its output depends on its input in the same tick, so it reads
    // the input at emit time: scheduling it before its producer violates emit-before, unlike the
    // UnitDelay loop breaker that `delay()` reorders legally.
    let probe = (oce_blocks::lookup("CDL.Reals.Hysteresis").unwrap().make)(&ParamTable {
        values: vec![
            (Arc::from("uLow"), Value::Real(0.0)),
            (Arc::from("uHigh"), Value::Real(1.0)),
        ],
    });
    assert_eq!(probe.kind(), oce_blocks::BlockKind::Stateful);
    assert!(probe.feeds_through(0, 0));
    let mut image = Image::new();
    image.classes = vec![
        class("CDL.Reals.Hysteresis"),
        class("CDL.Reals.Sources.Constant"),
    ];
    image.connectors = 3;
    image.instances = vec![
        inst(1, "CDL.Reals.Sources.Constant", &[], &[], &[0]),
        inst(
            0,
            "CDL.Reals.Hysteresis",
            &[("uLow", Value::Real(0.0)), ("uHigh", Value::Real(1.0))],
            &[1],
            &[2],
        ),
    ];
    image.schedule = vec![0, 1];
    image.connections = vec![(0, 1)];
    decode(&image.to_bytes()).unwrap();
    image.schedule = vec![1, 0];
    assert_eq!(
        refusal(&image),
        BlobError::InvalidSchedule {
            position: 0,
            fault: ScheduleFault::EmitBefore
        }
    );
}

#[test]
fn a_target_bound_class_makes_the_decoded_executable_target_bound() {
    let mut sin = Image::new();
    sin.classes = vec![class("CDL.Reals.Sin")];
    sin.connectors = 2;
    sin.instances = vec![inst(0, "CDL.Reals.Sin", &[], &[0], &[1])];
    sin.schedule = vec![0];
    sin.inputs = vec![input("urn:test:u", VType::Real, &[0])];
    sin.outputs = vec![out("urn:test:y", 1)];
    let bytes = sin.to_bytes();
    assert!(decode(&bytes).unwrap().target_bound);
    for image in [chain(), adder(), delay()] {
        assert!(!decode(&image.to_bytes()).unwrap().target_bound);
    }
}
