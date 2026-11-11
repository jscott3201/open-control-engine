//! What equal ids promise, checked empirically (`docs/design/content-id.md`, "What equal ids
//! promise"): for every corpus document and each of its permuted variants, which share its
//! canonical content id, the shared 160-frame schedule produces the same boundary outputs by raw
//! bits after every successful frame, keyed by name, the same state words per block keyed by
//! block name, and the same refusals. Warnings compare as sets per frame: their order follows the
//! schedule, which authored order may change.

// Shared helper, declared once for this binary in its root `facade.rs`.
use crate::content_id_corpus as corpus;
#[path = "../../oce-graph/tests/support/frame_schedule.rs"]
mod frame_schedule;

use std::collections::BTreeMap;

use corpus::*;
use oce_api::{Engine, Value};
use serde_json::Value as Json;

/// A permuted variant: its bytes and the namespace its instance IRIs use.
type Variant = (&'static str, fn(&Json) -> Vec<u8>, &'static str);

const VARIANTS: &[Variant] = &[
    (
        "@graph reversed",
        |d| to_bytes(&reverse_graph(d)),
        NAMESPACE,
    ),
    (
        "@graph rotated by half",
        |d| {
            let half = d["@graph"].as_array().unwrap().len() / 2;
            to_bytes(&rotate_graph(d, half))
        },
        NAMESPACE,
    ),
    (
        "containsBlock and isConnectedTo reversed",
        |d| {
            let d = reverse_relation(d, "S231:containsBlock", |_| true);
            to_bytes(&reverse_relation(&d, "S231:isConnectedTo", |_| true))
        },
        NAMESPACE,
    ),
    (
        "namespace moved, keys reversed",
        |d| reverse_keys(&reverse_graph(&move_namespace(d, "urn:oce:author:"))),
        "urn:oce:author:",
    ),
];

/// One frame's observation, every name with its namespace replaced by [`NAMESPACE`].
#[derive(Debug, PartialEq)]
enum Frame {
    Refused,
    Accepted {
        outputs: BTreeMap<String, String>,
        state: BTreeMap<String, Vec<u64>>,
        warnings: Vec<String>,
    },
}

fn bits(value: &Value) -> String {
    match value {
        Value::Real(x) => format!("real:{:016x}", x.to_bits()),
        other => format!("{other:?}"),
    }
}

/// Run the shared schedule on `engine`, whose instance IRIs use `namespace`.
fn run(engine: &mut Engine, namespace: &str) -> Vec<Frame> {
    let normal = |text: &str| text.replace(namespace, NAMESPACE);
    let definitions = engine.input_definitions().unwrap();
    // Slots follow the namespace-free names, so every variant draws the same value per input.
    let mut order: Vec<usize> = (0..definitions.len()).collect();
    order.sort_by_key(|&index| normal(&definitions[index].path));
    let blob = engine.compile_blob().unwrap();
    let decoded = oce_graph::blob::decode(&blob.bytes).unwrap();
    let topology = engine.topology();
    let mut frames = Vec::new();
    for k in 0..frame_schedule::TICKS {
        let entries: Vec<(String, Value)> = order
            .iter()
            .enumerate()
            .map(|(slot, &index)| {
                let definition = &definitions[index];
                let value = frame_schedule::input_value(
                    definition.value_type,
                    definition.min.as_ref(),
                    definition.max.as_ref(),
                    slot,
                    k,
                );
                (definition.path.clone(), value)
            })
            .collect();
        let borrowed: Vec<(&str, Value)> = entries
            .iter()
            .map(|(path, value)| (path.as_str(), value.clone()))
            .collect();
        let completed = engine
            .prepare_frame(frame_schedule::time_at(k), &borrowed)
            .and_then(|plan| engine.execute_frame(plan));
        let Ok(completed) = completed else {
            frames.push(Frame::Refused);
            continue;
        };
        let words = engine.state_words();
        let state = decoded
            .state
            .slots
            .iter()
            .map(|slot| {
                let name = normal(&topology.blocks[slot.block.0 as usize].instance_path);
                (name, words[slot.offset..slot.offset + slot.len].to_vec())
            })
            .collect();
        let mut warnings: Vec<String> = completed
            .diagnostics()
            .iter()
            .map(|event| normal(&format!("{event:?}")))
            .collect();
        warnings.sort();
        frames.push(Frame::Accepted {
            outputs: completed
                .outputs()
                .iter()
                .map(|(path, value)| (normal(path), bits(value)))
                .collect(),
            state,
            warnings,
        });
    }
    frames
}

#[test]
fn equal_ids_run_the_frame_corpus_to_identical_outputs_state_and_refusals() {
    let (mut compared, mut accepted, mut stateful) = (0, 0, 0);
    for name in corpus() {
        let document = fixture_json(&name);
        let mut original = load(&fixture_bytes(&name));
        let id = original.content_id().unwrap();
        let reference = run(&mut original, NAMESPACE);
        for frame in &reference {
            if let Frame::Accepted { state, .. } = frame {
                accepted += 1;
                stateful += usize::from(!state.is_empty());
            }
        }
        for (variant, apply, namespace) in VARIANTS {
            let mut engine = load(&apply(&document));
            assert_eq!(engine.content_id().unwrap(), id, "{name}: {variant}");
            let frames = run(&mut engine, namespace);
            assert_eq!(frames.len(), reference.len());
            for (k, (left, right)) in reference.iter().zip(&frames).enumerate() {
                assert_eq!(left, right, "{name}: {variant}: frame {k}");
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 47 * VARIANTS.len());
    // The comparison is not vacuous: frames are accepted and carry state.
    assert!(accepted > 47 * 100, "{accepted} accepted frames");
    assert!(stateful > 0);
}
