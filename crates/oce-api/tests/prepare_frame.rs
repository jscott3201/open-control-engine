//! Complete-frame preparation is read-only, not execution. Expectations are hand-derived
//! from the two authored Add boundary inputs; no external oracle exists for this facade policy.

use oce_api::{Engine, OcError, Value, ValueType};

const MODEL: &[u8] = include_bytes!("fixtures/legacy_frame_add.jsonld");
const A: &str = "urn:legacy-frame:a";
const B: &str = "urn:legacy-frame:b";
const Y: &str = "urn:legacy-frame:y";

#[test]
fn complete_reordered_inputs_prepare_without_staging_or_advancing() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    let before = engine.state_snapshot().unwrap();
    let definitions = engine.input_definitions().unwrap();
    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[0].path, A);
    assert_eq!(definitions[1].path, B);
    assert_eq!(definitions[0].value_type, ValueType::Real);
    let frame = engine
        .prepare_frame(4.0, &[(B, Value::Real(2.0)), (A, Value::Real(-0.0))])
        .unwrap();
    drop(frame);
    assert_eq!(
        engine.state_snapshot().unwrap().as_bytes(),
        before.as_bytes()
    );
    assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Real(0.0)));
    // Preparation does not close the fresh durable-restore window.
    engine.restore_state(&before).unwrap();
}

#[test]
fn duplicate_missing_unknown_direction_and_type_refusals_are_read_only() {
    for (entries, kind) in [
        (
            vec![(A, Value::Real(1.0)), (A, Value::Real(1.0))],
            "duplicate",
        ),
        (vec![(B, Value::Real(2.0))], "missing"),
        (vec![("unknown", Value::Real(1.0))], "unknown"),
        (vec![(Y, Value::Real(1.0))], "direction"),
        (
            vec![(A, Value::Boolean(true)), (B, Value::Real(2.0))],
            "type",
        ),
    ] {
        let mut engine = Engine::in_memory();
        engine.load_cxf(MODEL).unwrap();
        let before = engine.state_snapshot().unwrap();
        let error = engine.prepare_frame(4.0, &entries).unwrap_err();
        match kind {
            "duplicate" => assert!(matches!(error, OcError::FrameDuplicateInput(p) if p == A)),
            "missing" => assert!(matches!(error, OcError::FrameMissingInput(p) if p == A)),
            "unknown" => assert!(
                matches!(error, OcError::FrameUnknownInput { prefix, bytes: 7 } if prefix == "unknown")
            ),
            "direction" => {
                assert!(matches!(error, OcError::FrameNotInput { prefix, .. } if prefix == Y))
            }
            "type" => assert!(matches!(error, OcError::InputType(p) if p == A)),
            _ => unreachable!(),
        }
        assert_eq!(
            engine.state_snapshot().unwrap().as_bytes(),
            before.as_bytes()
        );
        assert!(engine.watch(&[Y]).unwrap()[0].1.bit_eq(&Value::Real(0.0)));
        engine.restore_state(&before).unwrap();
    }
}

#[test]
fn refusal_precedence_and_canonical_identity_repeat_under_entry_reordering() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    let cases = [
        (
            vec![
                ("z-unknown", Value::Real(0.0)),
                ("a-unknown", Value::Real(0.0)),
                (A, Value::Boolean(true)),
                (A, Value::Real(0.0)),
            ],
            "unknown frame input 'a-unknown' (9 UTF-8 bytes)",
        ),
        (
            vec![
                (Y, Value::Real(0.0)),
                (A, Value::Boolean(true)),
                (A, Value::Real(0.0)),
            ],
            "frame key is not a boundary input 'urn:legacy-frame:y' (18 UTF-8 bytes)",
        ),
        (
            vec![
                (B, Value::Boolean(true)),
                (B, Value::Real(0.0)),
                (A, Value::Real(0.0)),
                (A, Value::Real(0.0)),
            ],
            "duplicate frame input 'urn:legacy-frame:a'",
        ),
        (
            vec![(B, Value::Boolean(true))],
            "missing frame input 'urn:legacy-frame:a'",
        ),
        (
            vec![(B, Value::Boolean(true)), (A, Value::Boolean(true))],
            "input type mismatch for 'urn:legacy-frame:a'",
        ),
    ];
    let mut rendered = String::new();
    for (mut entries, expected) in cases {
        for _ in 0..entries.len() {
            entries.rotate_left(1);
            assert_eq!(
                engine.prepare_frame(0.0, &entries).unwrap_err().to_string(),
                expected
            );
            entries.reverse();
            assert_eq!(
                engine.prepare_frame(0.0, &entries).unwrap_err().to_string(),
                expected
            );
            entries.reverse();
        }
        rendered.push_str(expected);
        rendered.push('\n');
    }
    assert_eq!(
        rendered.as_bytes(),
        include_bytes!("fixtures/frame_refusals.txt")
    );
    assert!(matches!(
        engine.prepare_frame(f64::NAN, &[("bad", Value::Real(0.0))]),
        Err(OcError::NonFiniteTime { .. })
    ));
}

#[test]
fn unbounded_real_bits_are_accepted_but_time_is_finite_and_nondecreasing() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    for real in [
        f64::from_bits(0x7ff8_0000_0000_0042),
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.0,
        f64::from_bits(1),
    ] {
        engine
            .prepare_frame(-1.0, &[(A, Value::Real(real)), (B, Value::Real(0.0))])
            .unwrap();
    }
    let plan = engine
        .prepare_frame(4.0, &[(A, Value::Real(0.0)), (B, Value::Real(0.0))])
        .unwrap();
    engine.execute_frame(plan).unwrap();
    engine
        .prepare_frame(4.0, &[(B, Value::Real(0.0)), (A, Value::Real(0.0))])
        .unwrap();
    assert!(
        matches!(engine.prepare_frame(3.0, &[]), Err(OcError::TimeRegression { now, prev }) if now.to_bits() == 3.0_f64.to_bits() && prev.to_bits() == 4.0_f64.to_bits())
    );
}

#[test]
fn submitted_keys_are_not_reexpanded_or_rebound_to_elided_child_names() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    for key in ["urn:legacy-frame:add.u1", "a", "base:a"] {
        assert!(
            matches!(engine.prepare_frame(0.0, &[(key, Value::Real(0.0)), (B, Value::Real(0.0))]), Err(OcError::FrameUnknownInput { prefix, .. }) if prefix == key)
        );
    }
    // Context expansion occurs once, at ingest; the same canonical snapshot results.
    let original = std::str::from_utf8(MODEL).unwrap();
    let compact = original
        .replace(
            "\"S231\": \"http://data.ashrae.org/S231P#\"",
            "\"S231\": \"http://data.ashrae.org/S231P#\", \"base\": \"urn:legacy-frame:\"",
        )
        .replace("\"urn:legacy-frame:a\"", "\"base:a\"");
    engine.load_cxf(compact.as_bytes()).unwrap();
    assert_eq!(engine.input_definitions().unwrap()[0].path, A);
    engine
        .prepare_frame(0.0, &[(A, Value::Real(0.0)), (B, Value::Real(0.0))])
        .unwrap();
}

#[test]
fn unknown_key_diagnostics_do_not_clone_unbounded_utf8_input() {
    let mut engine = Engine::in_memory();
    engine.load_cxf(MODEL).unwrap();
    let key = "界".repeat(100_000);
    let allocation = allocation_counter::measure(|| {
        let error = engine
            .prepare_frame(0.0, &[(key.as_str(), Value::Real(0.0))])
            .unwrap_err();
        assert!(
            matches!(error, OcError::FrameUnknownInput { prefix, bytes: 300_000 } if prefix.len() == 63 && prefix.chars().count() == 21)
        );
    });
    assert_eq!(
        allocation.bytes_total,
        2 * std::mem::size_of::<Option<&Value>>() as u64 + 63
    );
    assert_eq!(allocation.bytes_current, 0);
}
