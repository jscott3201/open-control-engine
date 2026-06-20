use std::cell::RefCell;
use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{Assert, Block, BlockKind, Ctx, Diagnostics, Time, lookup};

#[derive(Default)]
struct CapturingDiagnostics {
    events: RefCell<Vec<(String, String, Time)>>,
}

impl Diagnostics for CapturingDiagnostics {
    fn warn(&self, source: &str, message: &str, t: Time) {
        self.events
            .borrow_mut()
            .push((source.to_string(), message.to_string(), t));
    }
}

fn step(block: &dyn Block, diag: &CapturingDiagnostics, t: f64, u: bool) {
    let cx = Ctx::new(t, diag);
    let mut emitted = 0usize;
    block.step_algebraic(&cx, &[Value::Boolean(u)], &mut |_, _| emitted += 1);
    assert_eq!(emitted, 0, "Assert has no signal output");
}

fn run_trace(inputs: &[(f64, bool)]) -> Vec<(String, String, u64)> {
    let block = Assert {
        message: Arc::from("freezestat tripped"),
    };
    let diag = CapturingDiagnostics::default();
    for (t, u) in inputs {
        step(&block, &diag, *t, *u);
    }
    diag.events
        .borrow()
        .iter()
        .map(|(source, message, t)| (source.clone(), message.clone(), t.to_bits()))
        .collect()
}

#[test]
fn assert_signature_is_zero_output_algebraic_sink() {
    let block = Assert::default();
    let sig = block.signature();
    assert_eq!(sig.class_path, "CDL.Utilities.Assert");
    assert_eq!(sig.inputs.len(), 1);
    assert!(
        sig.outputs.is_empty(),
        "Assert must not declare signal outputs"
    );
    assert!(!sig.stateful);
    assert_eq!(block.kind(), BlockKind::Algebraic);
    assert_eq!(block.state_len(), 0);
    assert!(!block.feeds_through(0, 0));
}

#[test]
fn assert_warns_on_every_false_evaluation_and_never_on_true() {
    let block = Assert {
        message: Arc::from("damper proof failed"),
    };
    let diag = CapturingDiagnostics::default();

    for (t, u) in [(0.0, true), (1.0, true)] {
        step(&block, &diag, t, u);
    }
    assert!(
        diag.events.borrow().is_empty(),
        "true input satisfies the assertion"
    );

    for (t, u) in [(2.0, false), (3.0, false), (4.0, true), (5.0, false)] {
        step(&block, &diag, t, u);
    }

    let events = diag.events.borrow();
    let got: Vec<(&str, &str, u64)> = events
        .iter()
        .map(|(source, message, t)| (source.as_str(), message.as_str(), t.to_bits()))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "CDL.Utilities.Assert",
                "damper proof failed",
                2.0f64.to_bits()
            ),
            (
                "CDL.Utilities.Assert",
                "damper proof failed",
                3.0f64.to_bits()
            ),
            (
                "CDL.Utilities.Assert",
                "damper proof failed",
                5.0f64.to_bits()
            ),
        ],
        "Assert.mo has a stateless assert equation, so false is level-triggered with no warn-once latch"
    );
}

#[test]
fn assert_warns_on_first_tick_false() {
    let events = run_trace(&[(0.0, false)]);
    assert_eq!(
        events,
        vec![(
            "CDL.Utilities.Assert".to_string(),
            "freezestat tripped".to_string(),
            0.0f64.to_bits()
        )]
    );
}

#[test]
fn assert_event_sequence_is_deterministic() {
    let inputs = [
        (0.0, false),
        (0.5, false),
        (1.0, true),
        (2.0, false),
        (2.0, false),
    ];
    assert_eq!(run_trace(&inputs), run_trace(&inputs));
}

#[test]
fn registry_constructs_assert_message_parameter() {
    let entry = lookup("CDL.Utilities.Assert").expect("Assert registered");
    let block = (entry.make)(&ParamTable {
        values: vec![(
            Arc::from("message"),
            Value::String(Arc::from("latched fault")),
        )],
    });
    let diag = CapturingDiagnostics::default();
    step(block.as_ref(), &diag, 7.0, false);
    let events = diag.events.borrow();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "CDL.Utilities.Assert");
    assert_eq!(events[0].1, "latched fault");
    assert_eq!(events[0].2.to_bits(), 7.0f64.to_bits());
}
