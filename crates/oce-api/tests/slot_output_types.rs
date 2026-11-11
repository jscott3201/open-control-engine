//! `FrameSlots::output_types()`: every boundary output is typed at load, in output-slot order,
//! before the first frame, and every accepted frame agrees (issue #460).
//!
//! * **Oracle.** The expected type of each output is read from the CXF document itself by a plain
//!   `serde_json` walk that shares no helper with the engine: the root's `S231:hasOutput` nodes
//!   and each one's authored `@type` (`S231:RealOutput`, `S231:IntegerOutput`,
//!   `S231:BooleanOutput`). The map must type each path as the document declares it, which also
//!   pins the order: a list in any other order than `outputs()` types some path wrongly, and a
//!   transposition control proves the comparison sees that.
//! * **Corpus.** The 46 G36 catalog fixtures and the two library rules under
//!   `fixtures/library_rules/`. After load and before any frame the slice has the length of
//!   `outputs()`; then the shared 160-frame schedule runs through the slot entry and every output
//!   value of every frame must have its entry's type, and the named path's first
//!   `CompletedFrame` must agree too. Volume pins keep the sweep from going vacuous.
//! * **Edge cases.** A model with no boundary output, pass-through declared outputs of each
//!   CXF-reachable type, a reload of identical bytes, a dirty parameter resume, a failed load and
//!   a read that allocates nothing. An Enum and a String boundary output, which no CXF document
//!   can declare, are hand-built in the private `frame_slots_tests.rs`.

// Shared helpers, declared once for this binary in its root `facade_allocation_census.rs`:
// `support/blob_corpus.rs` and `../../oce-graph/tests/support/frame_schedule.rs`.
use crate::blob_corpus;
use crate::frame_schedule;

use std::hint::black_box;

use allocation_counter::measure;
use blob_corpus::{FIXTURES, fixture_bytes};
use frame_schedule::{TICKS, input_value, time_at};
use oce_api::{Engine, FrameSlots, OcError, Value, ValueType};

const DELAY: &[u8] = include_bytes!("fixtures/frame_delay.jsonld");
const ASSERTION: &[u8] = include_bytes!("fixtures/assertion_model.jsonld");
const LIBRARY_RULES: [(&str, &[u8]); 2] = [
    (
        "AHU-0004",
        include_bytes!("fixtures/library_rules/AHU-0004/rule.cxf.jsonld"),
    ),
    (
        "AHU-0016",
        include_bytes!("fixtures/library_rules/AHU-0016/rule.cxf.jsonld"),
    ),
];

fn loaded(bytes: &[u8]) -> Engine {
    let mut engine = Engine::new();
    engine.load_cxf(bytes).expect("fixture loads");
    engine
}

/// The 46 catalog fixtures, then the two library rules: `(name, CXF bytes)`.
fn corpus() -> Vec<(String, Vec<u8>)> {
    FIXTURES
        .iter()
        .map(|name| ((*name).to_owned(), fixture_bytes(name)))
        .chain(
            LIBRARY_RULES
                .iter()
                .map(|(name, bytes)| ((*name).to_owned(), bytes.to_vec())),
        )
        .collect()
}

/// The root's declared outputs as `(@id, authored type)`, read from the document alone: the one
/// node that contains blocks and is contained by none, its `S231:hasOutput` references, and each
/// referenced node's `@type`.
fn declared_output_types(bytes: &[u8]) -> Vec<(String, ValueType)> {
    let document: serde_json::Value = serde_json::from_slice(bytes).expect("fixture is JSON");
    let graph = document["@graph"].as_array().expect("@graph is an array");
    let refs = |node: &serde_json::Value, key: &str| -> Vec<String> {
        match &node[key] {
            serde_json::Value::Array(items) => items
                .iter()
                .map(|item| item["@id"].as_str().expect("reference has @id").to_owned())
                .collect(),
            serde_json::Value::Object(map) => {
                vec![map["@id"].as_str().expect("reference has @id").to_owned()]
            }
            _ => Vec::new(),
        }
    };
    let contained: Vec<String> = graph
        .iter()
        .flat_map(|node| refs(node, "S231:containsBlock"))
        .collect();
    let roots: Vec<&serde_json::Value> = graph
        .iter()
        .filter(|node| {
            !refs(node, "S231:containsBlock").is_empty()
                && !contained.contains(&node["@id"].as_str().expect("node has @id").to_owned())
        })
        .collect();
    assert_eq!(roots.len(), 1, "a fixture has exactly one root");
    refs(roots[0], "S231:hasOutput")
        .into_iter()
        .map(|iri| {
            let node = graph
                .iter()
                .find(|node| node["@id"].as_str() == Some(iri.as_str()))
                .unwrap_or_else(|| panic!("declared output {iri} has a node"));
            let value_type = match node["@type"].as_str() {
                Some("S231:RealOutput") => ValueType::Real,
                Some("S231:IntegerOutput") => ValueType::Integer,
                Some("S231:BooleanOutput") => ValueType::Boolean,
                other => panic!("declared output {iri} has unexpected @type {other:?}"),
            };
            (iri, value_type)
        })
        .collect()
}

/// Every output path the map types differently from the document, or that the document does not
/// declare. Empty means slot `j`'s type is the declared type of `paths[j]` for every `j`.
fn disagreements(
    paths: &[String],
    types: &[ValueType],
    declared: &[(String, ValueType)],
) -> Vec<String> {
    paths
        .iter()
        .zip(types)
        .filter(|(path, value_type)| {
            !declared
                .iter()
                .any(|(iri, declared_type)| iri == *path && declared_type == *value_type)
        })
        .map(|(path, value_type)| format!("{path}: {value_type:?}"))
        .collect()
}

/// Frame `k`'s values in slot order, from the map's own definitions.
fn frame(slots: &FrameSlots, k: u32) -> Vec<Value> {
    slots
        .inputs()
        .iter()
        .enumerate()
        .map(|(slot, d)| input_value(d.value_type, d.min.as_ref(), d.max.as_ref(), slot, k))
        .collect()
}

#[test]
fn every_corpus_output_is_typed_as_declared_before_any_frame_and_every_frame_agrees() {
    let (mut real, mut integer, mut boolean) = (0_usize, 0_usize, 0_usize);
    let (mut frames, mut values, mut mixed, mut transposed) = (0_u64, 0_u64, 0_usize, 0_usize);
    let corpus = corpus();
    assert_eq!(
        corpus.len(),
        48,
        "46 catalog fixtures and two library rules"
    );
    for (name, bytes) in &corpus {
        let mut engine = loaded(bytes);
        let slots = engine.frame_slots().unwrap();
        let types = slots.output_types().to_vec();
        let paths = slots.outputs().to_vec();

        // Known at load: nothing has executed, and the slice is parallel to `outputs()`.
        assert_eq!(types.len(), paths.len(), "{name}: one type per output slot");
        assert!(!types.is_empty(), "{name}: the corpus declares outputs");
        let declared = declared_output_types(bytes);
        assert_eq!(
            paths.len(),
            declared.len(),
            "{name}: every declared output is a slot"
        );
        assert_eq!(
            disagreements(&paths, &types, &declared),
            Vec::<String>::new(),
            "{name}: a slot's type is its path's declared type"
        );
        for (slot, path) in paths.iter().enumerate() {
            assert_eq!(slots.output_slot(path), Some(slot), "{name}: {path}");
            // The latest-state view before the first frame already holds a value of that type.
            assert_eq!(
                engine.get_output(path).unwrap().value_type(),
                types[slot],
                "{name}: {path}: seeded latest value"
            );
        }
        for value_type in &types {
            match value_type {
                ValueType::Real => real += 1,
                ValueType::Integer => integer += 1,
                ValueType::Boolean => boolean += 1,
                other => panic!("{name}: no CXF document declares a {other:?} output"),
            }
        }

        // Control: where two neighbouring slots differ in type, swapping their types must be
        // reported, so the comparison above is sensitive to order and not only to the multiset.
        if types.windows(2).any(|pair| pair[0] != pair[1]) {
            mixed += 1;
            let at = types
                .windows(2)
                .position(|pair| pair[0] != pair[1])
                .unwrap();
            let mut swapped = types.clone();
            swapped.swap(at, at + 1);
            assert_eq!(
                disagreements(&paths, &swapped, &declared).len(),
                2,
                "{name}: a transposed type list is caught"
            );
            transposed += 1;
        }

        // The named path's first frame, on its own load of the same bytes.
        let mut named = loaded(bytes);
        let first = frame(&slots, 0);
        let entries: Vec<(&str, Value)> = slots
            .inputs()
            .iter()
            .zip(&first)
            .map(|(definition, value)| (definition.path.as_str(), value.clone()))
            .collect();
        let plan = named.prepare_frame(time_at(0), &entries).unwrap();
        let completed = named.execute_frame(plan).unwrap();
        assert_eq!(completed.outputs().len(), types.len(), "{name}");
        for (slot, (path, value)) in completed.outputs().iter().enumerate() {
            assert_eq!(*path, paths[slot], "{name}: named output order");
            assert_eq!(
                value.value_type(),
                types[slot],
                "{name}: {path}: named path"
            );
        }

        // Every accepted frame of the shared schedule through the slot entry.
        for k in 0..TICKS {
            let inputs = frame(&slots, k);
            let accepted = engine
                .execute_slots(&slots, time_at(k), &inputs)
                .unwrap_or_else(|error| panic!("{name}: frame {k}: {error}"));
            assert_eq!(accepted.output_count(), types.len(), "{name}: frame {k}");
            assert!(accepted.output(types.len()).is_none(), "{name}: frame {k}");
            for (slot, value) in accepted.outputs().enumerate() {
                assert_eq!(
                    value.value_type(),
                    types[slot],
                    "{name}: frame {k}: {}",
                    paths[slot]
                );
                assert!(
                    accepted.output(slot).unwrap().bit_eq(value),
                    "{name}: frame {k}: slot {slot}"
                );
                values += 1;
            }
            frames += 1;
        }
        // The map read after the run is the map read before it.
        assert_eq!(slots.output_types(), types.as_slice(), "{name}");
    }
    // Volume pins: the 132 catalog outputs (102 Real, 18 Integer, 12 Boolean, counted from the
    // documents) plus one Boolean per library rule; ten fixtures interleave types. A sweep that
    // stops discovering outputs, or a corpus with no mixed-type boundary, would otherwise pass
    // vacuously.
    assert_eq!((real, integer, boolean), (102, 18, 14));
    assert_eq!(frames, 48 * u64::from(TICKS));
    assert_eq!(values, (102 + 18 + 14) * u64::from(TICKS));
    assert_eq!((mixed, transposed), (10, 10));
}

#[test]
fn a_model_with_no_boundary_output_has_an_empty_type_slice() {
    // The assertion fixture's only block is `CDL.Utilities.Assert`, which has no output port.
    let mut engine = loaded(ASSERTION);
    let slots = engine.frame_slots().unwrap();
    assert!(slots.outputs().is_empty());
    assert!(slots.output_types().is_empty());
    assert_eq!(slots.inputs().len(), 1);
    let accepted = engine
        .execute_slots(&slots, 0.0, &[Value::Boolean(true)])
        .unwrap();
    assert_eq!(accepted.output_count(), 0);
    assert_eq!(accepted.outputs().count(), 0);
    assert!(accepted.output(0).is_none());
    assert!(slots.output_types().is_empty());
}

#[test]
fn a_pass_through_declared_output_has_the_type_of_the_input_it_copies() {
    // One pass-through per CXF-reachable type, beside nothing else at the boundary.
    let path = format!(
        "{}/../oce-cxf/tests/fixtures/pass_through_miniature.jsonld",
        env!("CARGO_MANIFEST_DIR")
    );
    let miniature = std::fs::read(path).expect("read the pass-through miniature");
    // Both return-fan fixtures declare `y1RetFan` as a Boolean pass-through among driven outputs.
    let documents = [
        ("pass_through_miniature".to_owned(), miniature, 3_usize),
        (
            "multizone_vav_return_fan_airflow_tracking".to_owned(),
            fixture_bytes("multizone_vav_return_fan_airflow_tracking"),
            1,
        ),
        (
            "multizone_vav_return_fan_direct_pressure".to_owned(),
            fixture_bytes("multizone_vav_return_fan_direct_pressure"),
            1,
        ),
    ];
    let mut seen = Vec::new();
    for (name, bytes, expected_pairs) in documents {
        let mut engine = loaded(&bytes);
        let slots = engine.frame_slots().unwrap();
        let pairs = engine.topology().pass_through;
        assert_eq!(pairs.len(), expected_pairs, "{name}: pass-through pairs");
        assert_eq!(
            disagreements(
                slots.outputs(),
                slots.output_types(),
                &declared_output_types(&bytes)
            ),
            Vec::<String>::new(),
            "{name}"
        );
        // Each pass-through input carries a value that is not its type's seed (`+0.0`, `0`,
        // `false`), so an output that came back equal was copied in this frame, sign bit included.
        let mut inputs = frame(&slots, 3);
        for pair in &pairs {
            let input = slots.input_slot(&pair.input).expect("pass-through input");
            inputs[input] = match slots.inputs()[input].value_type {
                ValueType::Real => Value::Real(-0.0),
                ValueType::Integer => Value::Integer(i64::from(i32::MIN)),
                ValueType::Boolean => Value::Boolean(true),
                other => panic!("{name}: no CXF document declares a {other:?} input"),
            };
        }
        let accepted = engine.execute_slots(&slots, 0.0, &inputs).unwrap();
        for pair in &pairs {
            let input = slots.input_slot(&pair.input).expect("pass-through input");
            let output = slots
                .output_slot(&pair.output)
                .expect("pass-through output");
            let value_type = slots.output_types()[output];
            assert_eq!(
                value_type,
                slots.inputs()[input].value_type,
                "{name}: {}",
                pair.output
            );
            let value = accepted.output(output).unwrap();
            assert_eq!(value.value_type(), value_type, "{name}: {}", pair.output);
            assert!(value.bit_eq(&inputs[input]), "{name}: {}", pair.output);
            seen.push(value_type);
        }
    }
    assert_eq!(
        seen,
        [
            ValueType::Real,
            ValueType::Integer,
            ValueType::Boolean,
            ValueType::Boolean,
            ValueType::Boolean
        ]
    );
}

/// The largest mixed boundary in the corpus: Real, Integer and Boolean outputs interleaved.
const MIXED: &str = "multizone_vav_freeze_protection";

#[test]
fn a_reload_of_identical_bytes_gives_a_new_map_with_the_same_types() {
    let bytes = fixture_bytes(MIXED);
    let mut engine = loaded(&bytes);
    let before = engine.frame_slots().unwrap();
    let kinds: std::collections::BTreeSet<String> = before
        .output_types()
        .iter()
        .map(|value_type| format!("{value_type:?}"))
        .collect();
    assert_eq!(kinds.len(), 3, "the fixture mixes all three corpus types");
    let inputs = frame(&before, 0);
    engine.execute_slots(&before, 0.0, &inputs).unwrap();

    engine.load_cxf(&bytes).unwrap();
    let after = engine.frame_slots().unwrap();
    assert_eq!(after.outputs(), before.outputs());
    assert_eq!(after.output_types(), before.output_types());
    // The types are equal, the maps are not interchangeable: the old one is superseded.
    assert!(matches!(
        engine.execute_slots(&before, 0.0, &inputs),
        Err(OcError::StalePreparedFrame)
    ));
    let accepted = engine.execute_slots(&after, 0.0, &inputs).unwrap();
    for (slot, value) in accepted.outputs().enumerate() {
        assert_eq!(value.value_type(), after.output_types()[slot]);
    }
    // A second engine of the same bytes and a clone of the map report the same types.
    let other = loaded(&bytes).frame_slots().unwrap();
    assert_eq!(other.output_types(), before.output_types());
    assert_eq!(before.clone().output_types(), before.output_types());
}

#[test]
fn a_dirty_parameter_resume_keeps_the_old_map_stale_and_the_types_unchanged() {
    let good = [Value::Real(0.5), Value::Real(2.0)];
    let mut engine = loaded(DELAY);
    let before = engine.frame_slots().unwrap();
    assert_eq!(before.outputs(), ["urn:frame:y".to_owned()]);
    assert_eq!(before.output_types(), [ValueType::Real]);
    engine.execute_slots(&before, 0.0, &good).unwrap();

    engine.halt().unwrap();
    engine
        .set_param("urn:frame:delay.samplePeriod", Value::Real(2.0))
        .unwrap();
    // While the edit is pending no new map exists; the old one still answers what it was built
    // with, and is refused.
    assert!(matches!(engine.frame_slots(), Err(OcError::State(_))));
    assert_eq!(before.output_types(), [ValueType::Real]);
    assert!(matches!(
        engine.execute_slots(&before, 1.0, &good),
        Err(OcError::State(_))
    ));
    engine.resume().unwrap();
    assert!(matches!(
        engine.execute_slots(&before, 1.0, &good),
        Err(OcError::StalePreparedFrame)
    ));
    let after = engine.frame_slots().unwrap();
    assert_eq!(after.outputs(), before.outputs());
    assert_eq!(after.output_types(), before.output_types());
    let accepted = engine.execute_slots(&after, 1.0, &good).unwrap();
    assert_eq!(accepted.output(0).unwrap().value_type(), ValueType::Real);

    // A clean resume and a failed load keep the map and its types.
    engine.halt().unwrap();
    engine.resume().unwrap();
    assert!(engine.load_cxf(b"not a document").is_err());
    assert_eq!(after.output_types(), [ValueType::Real]);
    let accepted = engine.execute_slots(&after, 2.0, &good).unwrap();
    assert_eq!(accepted.output(0).unwrap().value_type(), ValueType::Real);
}

#[test]
fn a_reload_of_a_different_model_types_the_new_boundary_not_the_old() {
    let mut engine = loaded(DELAY);
    let delay = engine.frame_slots().unwrap();
    engine
        .load_cxf(&fixture_bytes("cooling_only_alarms"))
        .unwrap();
    let alarms = engine.frame_slots().unwrap();
    assert_eq!(delay.output_types(), [ValueType::Real]);
    assert_eq!(alarms.output_types(), [ValueType::Integer; 3]);
    // The superseded map keeps its own answer and is refused, never rebound to the new schema.
    assert!(matches!(
        engine.execute_slots(&delay, 0.0, &[Value::Real(0.0), Value::Real(0.0)]),
        Err(OcError::StalePreparedFrame)
    ));
}

#[test]
fn reading_the_output_types_allocates_nothing() {
    let positive = measure(|| {
        black_box(vec![0_u8; 64]);
    });
    assert_eq!(positive.count_total, 1, "the counter observes allocations");
    let engine = loaded(&fixture_bytes(MIXED));
    let slots = engine.frame_slots().unwrap();
    let mut seen = 0_usize;
    let census = measure(|| {
        for _ in 0..64 {
            let types = black_box(slots.output_types());
            seen += types.len();
            for (slot, value_type) in types.iter().enumerate() {
                black_box((slot, *value_type));
            }
        }
    });
    assert_eq!(seen, 64 * slots.outputs().len());
    assert_eq!(census.count_total, 0, "{census:?}");
    assert_eq!(census.bytes_total, 0, "{census:?}");
}
