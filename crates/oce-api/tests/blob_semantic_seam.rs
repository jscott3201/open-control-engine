//! The OBC §7.17 seam in the compiled-schedule blob (GitHub #400): `SemanticPoint` payloads are
//! non-computational, so filling them and mutating every payload byte must leave the decoded
//! executable and its outputs identical.
//!
//! "Identical executable" is checked two ways: re-encoding the decoded executable with its
//! payloads emptied reproduces the checked-in golden byte for byte, and the freshly allocated run
//! state is equal word for word. "Identical outputs" is checked by ticking the payload-free and the
//! payload-filled executables through the same frames and comparing every connector value.

// Shared helpers, declared once for this binary in its root `facade.rs`:
// `support/blob_corpus.rs` and `support/blob_image.rs`.
use crate::blob_corpus;
use crate::blob_image;

use blob_corpus::{FIXTURES, golden};
use blob_image::rechecksum;
use oce_graph::blob::{BlobOutput, BlobSource, Decoded, decode, encode};
use oce_graph::{EvalContext, eval_tick};
use oce_model::{Value, ValueType};

const PAYLOAD_BYTES: usize = 3;

/// The decoded executable re-encoded with every payload emptied.
fn without_payloads(decoded: &Decoded<'_>) -> Vec<u8> {
    let inputs: Vec<_> = decoded
        .inputs
        .iter()
        .map(|input| oce_graph::blob::BlobInput {
            semantic_point: &[],
            ..input.clone()
        })
        .collect();
    let outputs: Vec<BlobOutput<'_>> = decoded
        .outputs
        .iter()
        .map(|output| BlobOutput {
            semantic_point: &[],
            ..output.clone()
        })
        .collect();
    encode(&BlobSource {
        inputs: &inputs,
        outputs: &outputs,
        ..decoded.source()
    })
    .unwrap()
}

/// Tick `decoded` through eight deterministic frames and return every connector value after each.
fn trace(decoded: &mut Decoded<'_>) -> Vec<Vec<Value>> {
    let mut rows = Vec::new();
    for k in 0..8u32 {
        for (slot, input) in decoded.inputs.iter().enumerate() {
            let value = match input.value_type {
                ValueType::Real => Value::Real(f64::from(k) * 1.5 - slot as f64),
                ValueType::Integer => Value::Integer(i64::from(k % 3)),
                ValueType::Boolean => Value::Boolean((k as usize + slot).is_multiple_of(2)),
                other => other.zero_value(),
            };
            for target in &input.targets {
                decoded.state.values[target.0 as usize] = value.clone();
            }
        }
        let mut ctx = EvalContext {
            model: &decoded.model,
            schedule: &decoded.schedule,
            blocks: &decoded.blocks,
            diagnostics: &oce_blocks::NoopDiagnostics,
            state: &mut decoded.state,
        };
        eval_tick(&mut ctx, f64::from(k));
        rows.push(decoded.state.values.clone());
    }
    rows
}

fn same_trace(left: &[Vec<Value>], right: &[Vec<Value>]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.bit_eq(y)))
}

#[test]
fn filling_and_mutating_every_semantic_point_byte_changes_nothing_that_computes() {
    let mut mutations = 0usize;
    for name in FIXTURES {
        let plain = golden(name);
        let mut baseline = decode(&plain).unwrap();
        let reference = trace(&mut baseline);
        let baseline = decode(&plain).unwrap();

        // Fill every payload with distinct bytes.
        let fill: Vec<Vec<u8>> = (0..baseline.inputs.len() + baseline.outputs.len())
            .map(|record| {
                (0..PAYLOAD_BYTES)
                    .map(|byte| (record * 31 + byte * 7) as u8)
                    .collect()
            })
            .collect();
        let inputs: Vec<_> = baseline
            .inputs
            .iter()
            .zip(&fill)
            .map(|(input, payload)| oce_graph::blob::BlobInput {
                semantic_point: payload,
                ..input.clone()
            })
            .collect();
        let outputs: Vec<_> = baseline
            .outputs
            .iter()
            .zip(&fill[baseline.inputs.len()..])
            .map(|(output, payload)| BlobOutput {
                semantic_point: payload,
                ..output.clone()
            })
            .collect();
        let filled = encode(&BlobSource {
            inputs: &inputs,
            outputs: &outputs,
            ..baseline.source()
        })
        .unwrap();
        if fill.is_empty() {
            assert!(filled == plain, "{name}: no I/O, nothing to fill");
            continue;
        }
        assert!(filled != plain && filled.len() == plain.len() + fill.len() * PAYLOAD_BYTES);

        // Locate every payload byte: the bytes where `filled` carries a fill pattern. The
        // decoded executable hands each payload back as a slice of `filled`, so its address
        // gives the offset.
        let offsets: Vec<usize> = {
            let decoded = decode(&filled).unwrap();
            let base = filled.as_ptr() as usize;
            decoded
                .inputs
                .iter()
                .map(|i| i.semantic_point)
                .chain(decoded.outputs.iter().map(|o| o.semantic_point))
                .flat_map(|payload| {
                    let start = payload.as_ptr() as usize - base;
                    start..start + payload.len()
                })
                .collect()
        };
        assert_eq!(offsets.len(), fill.len() * PAYLOAD_BYTES, "{name}");

        let mut variants = vec![filled.clone()];
        for &offset in &offsets {
            for value in [0x00, 0xff, filled[offset] ^ 0x5a] {
                let mut mutated = filled.clone();
                mutated[offset] = value;
                rechecksum(&mut mutated);
                variants.push(mutated);
            }
        }
        for bytes in &variants {
            let mut decoded = decode(bytes).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert!(
                without_payloads(&decoded) == plain,
                "{name}: executable changed"
            );
            assert_eq!(decoded.state.words, baseline.state.words, "{name}");
            assert!(
                decoded
                    .state
                    .values
                    .iter()
                    .zip(&baseline.state.values)
                    .all(|(a, b)| a.bit_eq(b)),
                "{name}: initial values changed"
            );
            assert!(
                same_trace(&trace(&mut decoded), &reference),
                "{name}: outputs changed"
            );
            mutations += 1;
        }
    }
    assert!(
        mutations > 1_000,
        "the seam test exercised {mutations} variants"
    );
}
