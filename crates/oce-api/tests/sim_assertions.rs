//! Public CXF assertion contract. Expected files are hand-authored, never engine-generated.
//!
//! Analytical oracle: pinned Buildings Utilities/Assert.mo:11 is
//! `assert(u, message, AssertionLevel.warning)`: Boolean true is silent, false warns,
//! with no Real tolerance or escalation. Repetition is OCE's per-HostTick collection contract,
//! not an external Modelica solver trace. There is no external diagnostics-channel oracle here.
//! The class-path diagnostic source is preserved; it is not an instance-path guarantee.

use std::fmt::Write as _;

use oce_api::{AssertLevel, CollectSpec, Engine, InputSource, OcError, SimSpec, Value};

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
    engine.set_realtime_epoch_unix_nanos(1_000_000_000);
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
        engine.set_input(U, Value::Boolean(input)).unwrap();
        let report = engine
            .step_realtime(t)
            .expect("warnings do not stop execution");
        assert_eq!(report.asserts.len(), count, "Boolean assertion truth table");
        assert_eq!(report.written, 1, "the sibling output is still written");
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
        for event in report.asserts {
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
        assert!(matches!(engine.set_input(U, value), Err(OcError::InputType(p)) if p == U));
    }
    engine.set_input(U, Value::Boolean(false)).unwrap();
    let report = engine.step_realtime(0.0).unwrap();
    assert_eq!(report.asserts.len(), 1);
    let event = &report.asserts[0];
    assert_eq!(event.level, AssertLevel::Warning);
    assert_eq!(event.block, "CDL.Utilities.Assert");
    assert_eq!(event.message, "freezestat tripped");
    assert_eq!(event.t.to_bits(), 0);
}

#[test]
fn ordinary_tick_and_simulation_keep_the_no_op_diagnostic_sink() {
    let mut engine = loaded();
    engine.set_input(U, Value::Boolean(false)).unwrap();
    assert!(
        engine
            .tick(0.0)
            .unwrap()
            .iter()
            .any(|(_, v)| v.bit_eq(&Value::Boolean(true)))
    );
    for inputs in [
        InputSource::Constant(vec![(U.to_owned(), Value::Boolean(false))]),
        InputSource::Closure(Box::new(|_| vec![(U.to_owned(), Value::Boolean(false))])),
    ] {
        let metrics = engine
            .simulate(&SimSpec {
                t_start: 0.0,
                t_stop: 1.0,
                step: 0.5,
                inputs,
                collect: CollectSpec::Named {
                    points: vec![Y.to_owned()],
                    stride: 1,
                },
            })
            .unwrap();
        assert_eq!(metrics.ticks, 3);
        assert_eq!(metrics.trace.rows(), 3);
        assert!(
            metrics
                .trace
                .column(0)
                .unwrap()
                .iter()
                .all(|v| v.bit_eq(&Value::Boolean(true)))
        );
    }
}
