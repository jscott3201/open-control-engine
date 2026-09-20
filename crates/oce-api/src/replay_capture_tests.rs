//! Detached full-domain receipt probes: not claims of String/enum signal support in live CXF.

use super::*;
use crate::{ReplayError, ReplayRecord};

#[path = "../tests/replay_oracle/mod.rs"]
mod oracle;

fn receipt() -> CompletedFrame {
    let record = ReplayRecord::from_bytes(&oracle::values()).unwrap();
    CompletedFrame {
        _generation: Arc::new(()),
        sequence: 99,
        time: record.time(),
        target_bound: false,
        inputs: record.inputs().to_vec(),
        outputs: record.outputs().to_vec(),
        diagnostics: record.diagnostics().to_vec(),
    }
}

#[test]
fn independently_authored_full_domain_bytes_are_the_capture_encoding_too() {
    let frame = receipt();
    for _ in 0..3 {
        assert_eq!(frame.replay_record().unwrap().as_bytes(), oracle::values());
    }
    let mut both = frame.clone();
    both.outputs.clone_from(&both.inputs);
    let payload = oracle::hex(include_str!("../tests/fixtures/replay_values.hex"));
    // The tail is a zero output count (4) followed by two diagnostics (4 + 22 + 17).
    let end = payload.len() - 47;
    let expected = oracle::assemble(
        oracle::DESCRIPTOR,
        None,
        &[&payload[..end], &payload[8..end], &payload[end + 4..]].concat(),
    );
    assert_eq!(both.replay_record().unwrap().as_bytes(), expected);
    both.sequence = u64::MAX;
    assert_eq!(
        both.replay_record().unwrap().as_bytes(),
        expected,
        "sequence is not wire position"
    );
}

#[test]
fn every_real_bit_class_round_trips_and_single_bit_drift_refuses() {
    let mut frame = receipt();
    frame.inputs.clear();
    frame.diagnostics.clear();
    for bits in [
        0,
        1,
        0x000f_ffff_ffff_ffff,
        0x0010_0000_0000_0000,
        0x7fef_ffff_ffff_ffff,
        0x8000_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff0_0000_0000_0042,
        0x7ff8_0000_0000_0042,
        0xfff8_0000_0000_0043,
    ] {
        frame.outputs = vec![("x".into(), Value::Real(f64::from_bits(bits)))];
        let mut payload = oracle::hex("0000000000000080 00000000 01000000 01000000 78 00");
        payload.extend(bits.to_le_bytes());
        payload.extend(0_u32.to_le_bytes());
        let expected = oracle::assemble(oracle::DESCRIPTOR, None, &payload);
        let record = frame.replay_record().unwrap();
        assert_eq!(record.as_bytes(), expected);
        record.verify(&frame).unwrap();
        frame.outputs[0].1 = Value::Real(f64::from_bits(bits ^ 1));
        assert_eq!(
            record.verify(&frame),
            Err(ReplayError::OutputMismatch { index: 0 })
        );
    }
}

#[test]
fn diagnostic_fields_order_duplicates_counts_and_value_identity_are_exact() {
    let frame = receipt();
    let record = frame.replay_record().unwrap();
    let mutations: &[fn(&mut CompletedFrame)] = &[
        |f| f.diagnostics[0].block.push('x'),
        |f| f.diagnostics[0].message.push('x'),
        |f| f.diagnostics[0].t = 0.0,
        |f| f.diagnostics.swap(0, 1),
        |f| {
            f.diagnostics.remove(0);
        },
    ];
    for change in mutations {
        let mut other = frame.clone();
        change(&mut other);
        for _ in 0..3 {
            assert_eq!(
                record.verify(&other),
                Err(ReplayError::DiagnosticMismatch { index: 0 })
            );
        }
    }
    let mut other = frame.clone();
    other.diagnostics.push(other.diagnostics[1].clone());
    assert_eq!(
        record.verify(&other),
        Err(ReplayError::DiagnosticMismatch { index: 2 })
    );
    let duplicates = other.replay_record().unwrap();
    assert_eq!(duplicates.diagnostics().len(), 3);
    duplicates.verify(&other).unwrap();
    let mut other = frame.clone();
    other.inputs[6].1 = Value::Enum {
        class: oce_model::EnumClassId::ZERO_TIME,
        ordinal: 4,
    };
    assert_eq!(
        record.verify(&other),
        Err(ReplayError::InputMismatch { index: 6 })
    );
    other.inputs[6].1 = Value::Enum {
        class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
        ordinal: 3,
    };
    assert_eq!(
        record.verify(&other),
        Err(ReplayError::InputMismatch { index: 6 })
    );
    other.inputs.pop();
    assert_eq!(
        record.verify(&other),
        Err(ReplayError::InputMismatch { index: 6 })
    );
    other = frame.clone();
    other.inputs[0].0.push('x');
    assert_eq!(
        record.verify(&other),
        Err(ReplayError::InputMismatch { index: 0 })
    );
    other = frame.clone();
    other.target_bound = true;
    assert_eq!(record.verify(&other), Err(ReplayError::TargetMismatch));
}

#[test]
fn capture_refuses_ineligible_enum_time_and_oversize_before_unbounded_encoding() {
    let mut frame = receipt();
    for value in [
        Value::Enum {
            class: oce_model::EnumClassId(u32::MAX),
            ordinal: 1,
        },
        Value::Enum {
            class: oce_model::EnumClassId::SIMPLE_CONTROLLER,
            ordinal: 0,
        },
    ] {
        frame.inputs[6].1 = value;
        assert_eq!(
            frame.replay_record().unwrap_err(),
            ReplayError::EnumDomain { offset: 0 }
        );
    }
    frame = receipt();
    frame.time = f64::NAN;
    assert_eq!(
        frame.replay_record().unwrap_err(),
        ReplayError::NonFiniteTime
    );
    frame = receipt();
    frame.outputs.push((
        "s".into(),
        Value::String("x".repeat(crate::MAX_REPLAY_BYTES).into()),
    ));
    let census = allocation_counter::measure(|| {
        assert!(matches!(
            frame.replay_record(),
            Err(ReplayError::TooLarge {
                limit_bytes: crate::MAX_REPLAY_BYTES,
                ..
            })
        ));
    });
    assert!(
        census.bytes_total < 4096,
        "sizes before allocating the large wire buffer"
    );
}

#[test]
fn detached_empty_receipt_has_one_canonical_zero_count_representation() {
    let mut frame = receipt();
    frame.inputs.clear();
    frame.outputs.clear();
    frame.diagnostics.clear();
    let payload = oracle::hex("0000000000000080 00000000 00000000 00000000");
    assert_eq!(
        frame.replay_record().unwrap().as_bytes(),
        oracle::assemble(oracle::DESCRIPTOR, None, &payload)
    );
}
