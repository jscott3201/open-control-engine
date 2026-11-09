//! Column kinds and widths (decision 21) and the frame section of the canonical encoding
//! (decision 23), on a hand-built trace with Boolean and Enum outputs, which no corpus fixture
//! has.

use oce_api::{Engine, NonFiniteInputPolicy, Value};

use super::*;
use crate::{DuplicateTime, OnRefusal, ReplayLimits, TimeBase};

/// An Enum value of some registered class, from a fixture's Enum parameter (the facade exposes
/// no enum class constructor).
fn enum_value(ordinal: u32) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oce-cxf/tests/fixtures/g36/cooling_only_controller.jsonld"
    );
    let mut engine = Engine::new();
    engine.load_cxf(&std::fs::read(path).unwrap()).unwrap();
    let class = engine
        .params()
        .iter()
        .find_map(|(_, v, _)| match v {
            Value::Enum { class, .. } => Some(class),
            _ => None,
        })
        .expect("the controller has an Enum parameter");
    Value::Enum { class, ordinal }
}

fn trace(outputs: Vec<(String, TraceColumn)>, kinds: &[TraceKind]) -> Trace {
    Trace {
        meta: TraceMeta {
            inputs: Vec::new(),
            outputs: outputs
                .iter()
                .zip(kinds)
                .map(|((path, _), kind)| TraceOutput {
                    path: path.clone(),
                    kind: *kind,
                    enum_class_index: None,
                })
                .collect(),
            time: TimeBase::Recorded,
            duplicate_time: DuplicateTime::Refuse,
            on_refusal: OnRefusal::Stop,
            non_finite: NonFiniteInputPolicy::Refuse,
            retains_inputs: false,
            snapshot_every: None,
            snapshot_at: Vec::new(),
            limits: ReplayLimits::MAX,
            origin: None,
            ignored_columns: Vec::new(),
        },
        times: vec![0.0, 1.0],
        status: vec![0, 3],
        outputs,
        mask: None,
        inputs: None,
        diagnostics: Vec::new(),
        refusals: Vec::new(),
        snapshots: Vec::new(),
        sparse_bytes: 0,
    }
}

#[test]
fn boolean_and_enum_columns_are_one_and_four_bytes_and_refuse_other_kinds() {
    assert_eq!(TraceKind::Real.width(), 8);
    assert_eq!(TraceKind::Integer.width(), 8);
    assert_eq!(TraceKind::Boolean.width(), 1);
    assert_eq!(TraceKind::Enum.width(), 4);
    assert_eq!(
        [
            TraceKind::Real,
            TraceKind::Integer,
            TraceKind::Boolean,
            TraceKind::Enum
        ]
        .map(TraceKind::tag),
        [0, 1, 2, 3]
    );
    let mut flags = TraceColumn::new(TraceKind::Boolean, 1, 4);
    assert!(flags.push(&Value::Boolean(true)));
    assert!(!flags.push(&Value::Integer(1)), "no kind widening");
    flags.push_zero();
    assert_eq!(flags, TraceColumn::Boolean(vec![0, 1, 0]));
    let mut modes = TraceColumn::new(TraceKind::Enum, 0, 2);
    assert!(modes.push(&enum_value(3)));
    assert!(!modes.push(&Value::Integer(3)));
    assert_eq!(modes, TraceColumn::Enum(vec![3]));
    assert_eq!(TraceKind::of(&enum_value(1)), Some(TraceKind::Enum));
    assert_eq!(TraceKind::of(&Value::String("s".into())), None);
}

#[test]
fn the_frame_section_encodes_each_column_in_order_with_its_width() {
    let outputs = vec![
        ("a.flag".to_owned(), TraceColumn::Boolean(vec![1, 0])),
        ("a.mode".to_owned(), TraceColumn::Enum(vec![2, 0])),
        ("a.y".to_owned(), TraceColumn::Real(vec![-0.0, 0.0])),
    ];
    let t = trace(
        outputs,
        &[TraceKind::Boolean, TraceKind::Enum, TraceKind::Real],
    );
    assert_eq!(t.bytes_per_frame(), 9 + 1 + 4 + 8);
    assert_eq!(t.frame_bytes(), 2 * 22);
    assert_eq!(t.byte_size(), 2 * 22);
    assert!(!t.is_clean());
    let bytes = t.canonical_bytes();
    assert_eq!(&bytes[..8], b"OCETRACE");
    assert_eq!(&bytes[8..12], &1_u32.to_le_bytes());
    // The frame section: row count, times, status, then each output column.
    let mut frame = Vec::new();
    frame.extend_from_slice(&2_u64.to_le_bytes());
    frame.extend_from_slice(&0.0_f64.to_bits().to_le_bytes());
    frame.extend_from_slice(&1.0_f64.to_bits().to_le_bytes());
    frame.extend_from_slice(&[0, 3]);
    frame.extend_from_slice(&[1, 0]);
    frame.extend_from_slice(&2_u32.to_le_bytes());
    frame.extend_from_slice(&0_u32.to_le_bytes());
    frame.extend_from_slice(&(-0.0_f64).to_bits().to_le_bytes());
    frame.extend_from_slice(&0.0_f64.to_bits().to_le_bytes());
    frame.extend_from_slice(&[0, 0]); // no mask, no inputs
    let tail = 3 * 8; // three empty sparse-table counts
    let at = bytes.len() - tail - frame.len();
    assert_eq!(&bytes[at..bytes.len() - tail], &frame[..]);
    assert_eq!(&bytes[bytes.len() - tail..], &[0_u8; 24]);
}

#[test]
fn the_substitution_mask_is_read_least_significant_bit_first() {
    let mut t = trace(Vec::new(), &[]);
    t.meta.inputs = (0..10)
        .map(|i| TraceInput {
            path: format!("i{i}"),
            kind: TraceKind::Real,
            enum_class: None,
            policy: crate::MissingPolicy::Refuse,
        })
        .collect();
    t.mask = Some(vec![0b0000_0001, 0b0000_0010, 0b1000_0000, 0]);
    assert_eq!(t.mask_width(), 2);
    assert!(t.substituted(0, 0) && t.substituted(0, 9));
    assert!(!t.substituted(0, 1) && !t.substituted(0, 8));
    assert!(t.substituted(1, 7) && !t.substituted(1, 0));
    assert!(
        !t.substituted(0, 10) && !t.substituted(5, 0),
        "out of range is false"
    );
}
