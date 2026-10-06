//! Public CXF assertion contract. Expected files are hand-authored, never engine-generated.
//!
//! Analytical oracle: pinned Buildings Utilities/Assert.mo:11 is
//! `assert(u, message, AssertionLevel.warning)`: Boolean true is silent, false warns,
//! with no Real tolerance or escalation. Repetition is OCE's per-HostTick collection contract,
//! not an external Modelica solver trace. There is no external diagnostics-channel oracle here.
//! The class-path diagnostic source is preserved; it is not an instance-path guarantee.

use std::fmt::Write as _;

use oce_api::{AssertLevel, Engine, OcError, Value};

const MODEL: &[u8] = include_bytes!("fixtures/assertion_model.jsonld");
const U: &str = "urn:assert#u";
const Y: &str = "urn:assert#invert.y";

#[test]
fn default_severity_is_warning_not_an_unemitted_failure() {
    assert_eq!(AssertLevel::default(), AssertLevel::Warning);
    assert_eq!(
        format!("{:?}\n", AssertLevel::default()).as_bytes(),
        include_bytes!("fixtures/assertion_default.txt")
    );
}

fn loaded() -> Engine {
    let mut engine = Engine::in_memory();
    let report = engine.load_cxf(MODEL).expect("actual CXF Assert loads");
    assert_eq!(report.block_count, 2);
    assert!(report.warnings.is_empty());
    engine
}

fn events() -> String {
    let mut engine = loaded();
    let mut rendered = String::new();
    for (t, input, count, output) in [
        (-0.5_f64, true, 0, false),
        (0.0, false, 1, true),
        (0.0, false, 1, true),
        (0.25, true, 0, false),
        (0.5, false, 1, true),
        (1.0, true, 0, false),
    ] {
        let prepared = engine
            .prepare_frame(t, &[(U, Value::Boolean(input))])
            .unwrap();
        let report = engine
            .execute_frame(prepared)
            .expect("warnings do not stop execution");
        assert_eq!(
            report.diagnostics().len(),
            count,
            "Boolean assertion truth table"
        );
        assert!(
            engine
                .get_output(Y)
                .unwrap()
                .bit_eq(&Value::Boolean(output))
        );
        writeln!(
            rendered,
            "{:016x}|events={count}|output={output}",
            t.to_bits()
        )
        .unwrap();
        for event in report.diagnostics() {
            writeln!(
                rendered,
                "{}|{}|{:016x}|{:?}",
                event.block,
                event.message,
                event.t.to_bits(),
                event.level
            )
            .unwrap();
        }
    }
    rendered
}

#[test]
fn boolean_assertions_repeat_warning_records_and_continue_bit_exactly() {
    for _ in 0..3 {
        assert_eq!(
            events().as_bytes(),
            include_bytes!("fixtures/assertion_events.txt")
        );
    }
}

#[test]
fn first_false_tick_warns_and_non_boolean_input_is_refused_without_coercion() {
    let mut engine = loaded();
    for value in [Value::Real(0.0), Value::Real(f64::NAN), Value::Integer(0)] {
        assert!(
            matches!(engine.prepare_frame(0.0, &[(U, value)]), Err(OcError::InputType(p)) if p == U)
        );
    }
    let prepared = engine
        .prepare_frame(0.0, &[(U, Value::Boolean(false))])
        .unwrap();
    let report = engine.execute_frame(prepared).unwrap();
    assert_eq!(report.diagnostics().len(), 1);
    let event = &report.diagnostics()[0];
    assert_eq!(event.level, AssertLevel::Warning);
    assert_eq!(event.block, "CDL.Utilities.Assert");
    assert_eq!(event.message, "freezestat tripped");
    assert_eq!(event.t.to_bits(), 0);
}

#[test]
fn each_false_frame_retains_its_own_warning_and_inspections_do_not_clear_it() {
    let mut engine = loaded();
    let mut retained = Vec::new();
    for time in [0.0, 0.0, 0.5] {
        let prepared = engine
            .prepare_frame(time, &[(U, Value::Boolean(false))])
            .unwrap();
        let frame = engine.execute_frame(prepared).unwrap();
        assert_eq!(frame.diagnostics().len(), 1);
        assert!(engine.get_output(Y).unwrap().bit_eq(&Value::Boolean(true)));
        engine.watch(&[Y]).unwrap();
        retained.push(frame);
    }
    for (index, frame) in retained.iter().enumerate() {
        assert_eq!(frame.sequence(), index as u64 + 1);
        assert_eq!(frame.diagnostics().len(), 1);
    }
}
