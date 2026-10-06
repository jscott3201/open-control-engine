//! Native completed frames: hand-derived arithmetic and HostTick recurrence oracles.
//! Expected files are authored, not generated from engine output. No Modelica solver claim.

mod support;

use std::fmt::Write as _;
use std::sync::Arc;

use oce_api::{CompletedFrame, Engine, EngineStateError, OcError, Value};
use support::recording_store::{RecordingStore, StoreCallSnapshot};

const ADD: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");
const PRE: &[u8] = include_bytes!("fixtures/frame_pre.jsonld");
const DELAY: &[u8] = include_bytes!("fixtures/frame_delay.jsonld");

fn render(frame: &CompletedFrame) -> String {
    let mut text = format!("{}|{:016x}\n", frame.sequence(), frame.time().to_bits());
    for (path, value) in frame.outputs() {
        match value {
            Value::Real(value) => writeln!(text, "{path}|real:{:016x}", value.to_bits()),
            Value::Boolean(value) => writeln!(text, "{path}|bool:{value}"),
            _ => panic!("fixture value type"),
        }
        .unwrap();
    }
    for event in frame.diagnostics() {
        writeln!(
            text,
            "{}|{}|{:016x}|{:?}",
            event.block,
            event.message,
            event.t.to_bits(),
            event.level
        )
        .unwrap();
    }
    text
}

#[test]
fn arithmetic_commits_complete_values_without_store_and_retains_independent_results() {
    for _ in 0..3 {
        let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
        engine.load_cxf(ADD).unwrap();
        let startup = engine.state_snapshot().unwrap();
        engine.store().reset_calls();
        engine.store().arm_hot_path_guard();
        let mut results = Vec::new();
        for (a, b, expected) in [(1.5, 2.25, 3.75), (-4.0, 0.5, -3.5)] {
            let frame = engine
                .prepare_frame(
                    0.0,
                    &[
                        ("urn:legacy-frame:b", Value::Real(b)),
                        ("urn:legacy-frame:a", Value::Real(a)),
                    ],
                )
                .unwrap();
            let completed = engine.execute_frame(frame).unwrap();
            assert_eq!(
                completed.outputs().len(),
                1,
                "not the internal driver plus its alias"
            );
            assert_eq!(completed.outputs()[0].0, "urn:legacy-frame:y");
            assert!(completed.outputs()[0].1.bit_eq(&Value::Real(expected)));
            assert!(
                engine
                    .get_output("urn:legacy-frame:y")
                    .unwrap()
                    .bit_eq(&Value::Real(expected))
            );
            for point in engine
                .io()
                .iter()
                .filter(|p| p.direction == oce_api::PointDirection::Out)
            {
                assert!(
                    engine
                        .get_output(&point.path)
                        .unwrap()
                        .bit_eq(&Value::Real(expected))
                );
            }
            assert!(completed.diagnostics().is_empty());
            results.push(completed);
        }
        assert_eq!(
            results.iter().map(render).collect::<String>().as_bytes(),
            include_bytes!("fixtures/completed_arithmetic.txt")
        );
        assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
        assert!(matches!(
            engine.restore_state(&startup),
            Err(OcError::State(EngineStateError::DurableTargetAdvanced))
        ));
    }
}

#[test]
fn equal_time_feedback_advances_once_and_refusal_consumes_no_position() {
    for _ in 0..3 {
        let mut engine = Engine::in_memory();
        engine.load_cxf(PRE).unwrap();
        assert!(engine.input_definitions().unwrap().is_empty());
        let old = engine.prepare_frame(1.0, &[]).unwrap();
        let first = engine.prepare_frame(2.0, &[]).unwrap();
        let first = engine.execute_frame(first).unwrap();
        let snapshot = engine.state_snapshot().unwrap();
        assert!(matches!(
            engine.execute_frame(old),
            Err(OcError::TimeRegression { .. })
        ));
        assert_eq!(
            snapshot.as_bytes(),
            engine.state_snapshot().unwrap().as_bytes()
        );
        let second = engine.prepare_frame(2.0, &[]).unwrap();
        let second = engine.execute_frame(second).unwrap();
        assert_eq!(
            format!("{}{}", render(&first), render(&second)).as_bytes(),
            include_bytes!("fixtures/completed_pre.txt")
        );
        assert_eq!(first.sequence(), 1);
        assert_eq!(second.sequence(), 2);
        // Distinct declared outputs sharing one driver are both logical boundary outputs.
        assert_eq!(first.outputs().len(), 2);
        assert_eq!(first.outputs()[0].0, "urn:pre:a");
        assert_eq!(first.outputs()[1].0, "urn:pre:z");
    }
}

#[test]
fn sampled_delay_matches_hand_recurrence_and_retained_frames_survive_lifecycle_changes() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(DELAY).unwrap();
    let startup = engine.state_snapshot().unwrap();
    let mut retained = Vec::new();
    // UnitDelay emits the preceding sample; equal time emits held state but takes no new sample.
    for (time, input, expected) in [
        (0.0, 2.0, 0.0),
        (1.0, 3.0, 2.0),
        (1.0, 4.0, 2.0),
        (2.0, 5.0, 3.0),
    ] {
        let plan = engine
            .prepare_frame(
                time,
                &[
                    ("urn:frame:a", Value::Real(0.0)),
                    ("urn:frame:b", Value::Real(input)),
                ],
            )
            .unwrap();
        let frame = engine.execute_frame(plan).unwrap();
        assert!(frame.outputs()[0].1.bit_eq(&Value::Real(expected)));
        retained.push(frame);
    }
    let image = retained.iter().map(render).collect::<String>();
    assert_eq!(
        image.as_bytes(),
        include_bytes!("fixtures/completed_delay.txt")
    );
    let checkpoint = engine.checkpoint().unwrap();
    let plan = engine
        .prepare_frame(
            8.0,
            &[
                ("urn:frame:a", Value::Real(0.0)),
                ("urn:frame:b", Value::Real(5.0)),
            ],
        )
        .unwrap();
    assert_eq!(engine.execute_frame(plan).unwrap().sequence(), 5);
    engine.restore_checkpoint(&checkpoint).unwrap();
    engine.halt().unwrap();
    engine.resume().unwrap();
    let plan = engine
        .prepare_frame(
            2.0,
            &[
                ("urn:frame:a", Value::Real(0.0)),
                ("urn:frame:b", Value::Real(9.0)),
            ],
        )
        .unwrap();
    assert_eq!(engine.execute_frame(plan).unwrap().sequence(), 6);
    engine.halt().unwrap();
    engine
        .set_param("urn:frame:delay.samplePeriod", Value::Real(1.0))
        .unwrap();
    engine.resume().unwrap();
    let plan = engine
        .prepare_frame(
            0.0,
            &[
                ("urn:frame:a", Value::Real(0.0)),
                ("urn:frame:b", Value::Real(9.0)),
            ],
        )
        .unwrap();
    assert_eq!(engine.execute_frame(plan).unwrap().sequence(), 7);
    engine.load_cxf(DELAY).unwrap();
    engine.restore_state(&startup).unwrap();
    let plan = engine
        .prepare_frame(
            0.0,
            &[
                ("urn:frame:a", Value::Real(0.0)),
                ("urn:frame:b", Value::Real(9.0)),
            ],
        )
        .unwrap();
    assert_eq!(engine.execute_frame(plan).unwrap().sequence(), 8);
    engine.load_cxf(PRE).unwrap();
    let plan = engine.prepare_frame(0.0, &[]).unwrap();
    assert_eq!(engine.execute_frame(plan).unwrap().sequence(), 9);
    assert_eq!(retained.iter().map(render).collect::<String>(), image);
    assert_eq!(render(&retained[0].clone()), render(&retained[0]));
}

#[test]
fn fanout_tail_and_native_passthrough_bits_reach_only_the_boundary() {
    let mut engine = Engine::in_memory();
    engine
        .load_cxf(include_bytes!(
            "../../oce-cxf/tests/fixtures/boundary_fanout.jsonld"
        ))
        .unwrap();
    let plan = engine
        .prepare_frame(
            -0.0,
            &[(
                "http://example.org#g36.profile.boundary_fanout.u",
                Value::Real(4.0),
            )],
        )
        .unwrap();
    let frame = engine.execute_frame(plan).unwrap();
    assert_eq!(frame.time().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(frame.outputs().len(), 1);
    // Independent arithmetic: 2*4 + 3*4 = 20. Dropping either fan-out arm cannot pass.
    assert!(frame.outputs()[0].1.bit_eq(&Value::Real(20.0)));
    engine
        .load_cxf(include_bytes!(
            "../../oce-cxf/tests/fixtures/pass_through_miniature.jsonld"
        ))
        .unwrap();
    for real in [
        -0.0,
        0.0,
        f64::from_bits(1),
        f64::from_bits(0x7ff8_1234_5678_9abc),
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let plan = engine
            .prepare_frame(
                0.0,
                &[
                    (
                        "http://example.org#PassThroughMiniature.realIn",
                        Value::Real(real),
                    ),
                    (
                        "http://example.org#PassThroughMiniature.integerIn",
                        Value::Integer(i64::from(i32::MIN)),
                    ),
                    (
                        "http://example.org#PassThroughMiniature.booleanIn",
                        Value::Boolean(true),
                    ),
                ],
            )
            .unwrap();
        let frame = engine.execute_frame(plan).unwrap();
        assert_eq!(
            frame.outputs().len(),
            3,
            "exclude internal keep.y; do not duplicate pass-through aliases"
        );
        for ((path, value), (suffix, expected)) in frame.outputs().iter().zip([
            ("booleanOut", Value::Boolean(true)),
            ("integerOut", Value::Integer(i64::from(i32::MIN))),
            ("realOut", Value::Real(real)),
        ]) {
            assert_eq!(
                path,
                &format!("http://example.org#PassThroughMiniature.{suffix}")
            );
            assert!(value.bit_eq(&expected));
        }
    }
}

#[test]
fn warning_only_empty_output_frames_retain_exact_deterministic_diagnostics() {
    for _ in 0..3 {
        // Duplicate the assertion with distinct message text, preserving authored schedule order.
        let text = std::str::from_utf8(include_bytes!("fixtures/assertion_model.jsonld")).unwrap();
        let mut document: serde_json::Value = serde_json::from_str(text).unwrap();
        let nodes = document["@graph"].as_array_mut().unwrap();
        nodes[0]["S231:containsBlock"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"@id":"urn:assert#second"}));
        nodes.last_mut().unwrap()["S231:isConnectedTo"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"@id":"urn:assert#second.u"}));
        for index in [1, 2, 3] {
            let mut node = serde_json::to_string(&nodes[index])
                .unwrap()
                .replace("urn:assert#check", "urn:assert#second");
            node = node.replace("freezestat tripped", "second warning");
            nodes.push(serde_json::from_str(&node).unwrap());
        }
        let mut engine = Engine::with_store(Arc::new(RecordingStore::default()));
        engine
            .load_cxf(&serde_json::to_vec(&document).unwrap())
            .unwrap();
        engine.store().reset_calls();
        let mut frames = Vec::new();
        for input in [false, true, false] {
            let plan = engine
                .prepare_frame(0.0, &[("urn:assert#u", Value::Boolean(input))])
                .unwrap();
            let result = engine.execute_frame(plan).unwrap();
            assert!(
                result.outputs().is_empty(),
                "undeclared invert.y is internal"
            );
            assert_eq!(result.diagnostics().len(), if input { 0 } else { 2 });
            frames.push(result);
        }
        assert_eq!(
            frames.iter().map(render).collect::<String>().as_bytes(),
            include_bytes!("fixtures/completed_warnings.txt")
        );
        assert_eq!(engine.store().calls(), StoreCallSnapshot::default());
        let retained = frames.iter().map(render).collect::<String>();
        let checkpoint = engine.checkpoint().unwrap();
        let plan = engine
            .prepare_frame(1.0, &[("urn:assert#u", Value::Boolean(false))])
            .unwrap();
        engine.execute_frame(plan).unwrap();
        engine.restore_checkpoint(&checkpoint).unwrap();
        engine.halt().unwrap();
        engine.resume().unwrap();
        engine.halt().unwrap();
        engine
            .set_param(
                "urn:assert#check.message",
                Value::String(Arc::from("changed")),
            )
            .unwrap();
        engine.resume().unwrap();
        engine.load_cxf(ADD).unwrap();
        assert_eq!(frames.iter().map(render).collect::<String>(), retained);
    }
}
