//! Peak decoder memory against blob length (GitHub #400 review): a hostile connector count or port
//! list cannot make `decode` allocate far more than the bytes it was given before it refuses, and
//! the cost of the shapes that do decode is pinned per encoded byte.
//!
//! Ratios are peak heap bytes held at once during `decode` (`allocation_counter`'s `bytes_max`)
//! divided by the blob length. The bounds are recorded in `oce_graph::blob`'s "Allocation"
//! documentation and in `docs/compiled-schedule-blob.md`.

#[path = "support/blob_image.rs"]
mod blob_image;

use std::sync::Arc;

use allocation_counter::measure;
use blob_image::{Image, In, Inst, Out, VType, Val};
use oce_blocks::MAX_RESOLVED_ARITY;
use oce_graph::blob::{BlobError, InstanceField, MalformedFault, decode};
use oce_model::{ParamTable, Value};

fn class(path: &str) -> (Vec<u8>, u64) {
    (
        path.as_bytes().to_vec(),
        oce_blocks::class_signature_hash(path).unwrap(),
    )
}

fn stateless(class: u32, inputs: Vec<u32>, outputs: Vec<u32>, params: &[(&str, Value)]) -> Inst {
    Inst {
        class,
        kind: 0,
        state_len: 0,
        state_crc: 0,
        inputs,
        outputs,
        params: params
            .iter()
            .map(|(n, v)| (n.as_bytes().to_vec(), Val::from_value(v)))
            .collect(),
    }
}

/// Peak bytes held during `decode(bytes)`, the result, as a ratio of the blob length.
fn peak(bytes: &[u8]) -> (f64, Result<(), BlobError>) {
    let mut outcome = Ok(());
    let info = measure(|| {
        outcome = decode(bytes).map(|_| ());
    });
    (info.bytes_max as f64 / bytes.len() as f64, outcome)
}

/// One `Constant` (no inputs) whose record nonetheless lists `n` input ids, with a connector count
/// of `n + 1` so every id is in range: the shape that made the old decoder build a 144-byte
/// connector per id before the arity check.
fn constant_with_bogus_inputs(n: u32) -> Vec<u8> {
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.Sources.Constant")];
    image.connectors = n + 1;
    image.instances = vec![stateless(0, (1..=n).collect(), vec![0], &[])];
    image.schedule = vec![0];
    image.to_bytes()
}

/// The old amplification shape is refused at its arity check holding at most 8× the blob.
const HOSTILE_PEAK_RATIO: f64 = 8.0;

#[test]
fn a_hostile_port_list_is_refused_within_eight_times_the_blob_length() {
    let bytes = constant_with_bogus_inputs(1_000_000);
    assert!(bytes.len() > 4_000_000);
    let (ratio, outcome) = peak(&bytes);
    assert_eq!(
        outcome,
        Err(BlobError::InstanceMismatch {
            block: 0,
            field: InstanceField::InputArity
        })
    );
    assert!(
        ratio <= HOSTILE_PEAK_RATIO,
        "peak {ratio:.2}x the blob length"
    );
}

#[test]
fn a_port_list_longer_than_any_class_resolves_is_refused_before_any_allocation_by_count() {
    let n = u32::try_from(MAX_RESOLVED_ARITY).unwrap() + 1;
    let bytes = constant_with_bogus_inputs(n);
    let (ratio, outcome) = peak(&bytes);
    assert!(matches!(
        outcome,
        Err(BlobError::Malformed {
            fault: MalformedFault::ArityTooLarge,
            ..
        })
    ));
    assert!(ratio < 0.01, "peak {ratio:.4}x the blob length");
    // At the bound itself the structure pass accepts the list and the arity check refuses it.
    let at_bound = constant_with_bogus_inputs(n - 1);
    assert!(matches!(
        decode(&at_bound),
        Err(BlobError::InstanceMismatch {
            field: InstanceField::InputArity,
            ..
        })
    ));
}

/// Many minimal well-formed instances, refused only at the schedule: the decoder has built the
/// whole arena and every block by then.
const MANY_BLOCKS_PEAK_RATIO: f64 = 12.0;

#[test]
fn many_small_instances_refused_late_stay_within_their_recorded_ratio() {
    let blocks = 100_000u32;
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.Sources.Constant")];
    image.connectors = blocks;
    image.instances = (0..blocks)
        .map(|b| stateless(0, vec![], vec![b], &[("k", Value::Real(1.0))]))
        .collect();
    image.schedule = vec![];
    let bytes = image.to_bytes();
    let (ratio, outcome) = peak(&bytes);
    assert!(matches!(outcome, Err(BlobError::InvalidSchedule { .. })));
    assert!(
        ratio <= MANY_BLOCKS_PEAK_RATIO,
        "peak {ratio:.2}x the blob length"
    );
}

/// The densest accepted shape per encoded byte: one wide block whose inputs all fan out from one
/// boundary input, so each connector costs eight encoded bytes (a port slot and a target).
const WIDE_ACCEPTED_PEAK_RATIO: f64 = 32.0;

#[test]
fn a_wide_accepted_block_stays_within_its_recorded_ratio() {
    let nin = 200_000u32;
    let params = ParamTable {
        values: vec![(Arc::from("nin"), Value::Integer(i64::from(nin)))],
    };
    let block = (oce_blocks::lookup("CDL.Reals.MultiSum").unwrap().make)(&params);
    assert_eq!(block.resolved_signature().inputs.len(), nin as usize);
    let mut image = Image::new();
    image.classes = vec![class("CDL.Reals.MultiSum")];
    image.connectors = nin + 1;
    image.instances = vec![stateless(
        0,
        (0..nin).collect(),
        vec![nin],
        &[("nin", Value::Integer(i64::from(nin)))],
    )];
    image.schedule = vec![0];
    image.inputs = vec![In {
        path: b"urn:test:u".to_vec(),
        vtype: VType::Real,
        min: None,
        max: None,
        targets: (0..nin).collect(),
        payload: Vec::new(),
    }];
    image.outputs = vec![Out {
        path: b"urn:test:y".to_vec(),
        source: nin,
        payload: Vec::new(),
    }];
    let bytes = image.to_bytes();
    let (ratio, outcome) = peak(&bytes);
    assert_eq!(outcome, Ok(()));
    assert!(
        ratio <= WIDE_ACCEPTED_PEAK_RATIO,
        "peak {ratio:.2}x the blob length"
    );
}
