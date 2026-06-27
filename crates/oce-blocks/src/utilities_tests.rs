use std::cell::RefCell;
use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{Assert, Block, BlockKind, Ctx, Diagnostics, ParamRule, SunRiseSet, Time, lookup};

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

fn sun_step(
    block: &SunRiseSet,
    diag: &CapturingDiagnostics,
    region: &mut [u64],
    t: f64,
) -> (f64, f64, bool) {
    let cx = Ctx::new(t, diag);
    let mut outputs = Vec::new();
    block.emit_from_state(&cx, &[], region, &mut |_, value| outputs.push(value));
    block.update_state(&cx, &[], region);
    assert_eq!(outputs.len(), 3, "SunRiseSet emits three outputs");
    let next_rise = outputs[0].as_real().expect("nextSunRise is Real");
    let next_set = outputs[1].as_real().expect("nextSunSet is Real");
    let sun_up = outputs[2].as_boolean().expect("sunUp is Boolean");
    (next_rise, next_set, sun_up)
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

#[test]
fn sun_rise_set_signature_is_stateful_three_output_source() {
    let block = SunRiseSet::default();
    let sig = block.signature();
    assert_eq!(sig.class_path, "CDL.Utilities.SunRiseSet");
    assert!(sig.inputs.is_empty());
    assert_eq!(sig.outputs.len(), 3);
    assert!(sig.stateful);
    assert_eq!(block.kind(), BlockKind::Stateful);
    assert_eq!(block.state_len(), 5);
    assert!(!block.feeds_through(0, 0));
}

#[test]
fn sun_rise_set_updates_event_times_when_crossed() {
    let block = SunRiseSet::default();
    let diag = CapturingDiagnostics::default();
    let mut region = vec![0; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());

    let (first_rise, first_set, first_up) = sun_step(&block, &diag, &mut region, 0.0);
    assert!(first_rise.is_finite());
    assert!(first_set.is_finite());
    assert!(
        (0.0..43_200.0).contains(&first_rise),
        "equatorial January sunrise should be in the first half-day, got {first_rise}"
    );
    assert!(
        (43_200.0..86_400.0).contains(&first_set),
        "equatorial January sunset should be in the second half-day, got {first_set}"
    );
    assert!(!first_up, "midnight before sunrise should not report sunUp");

    let (second_rise, second_set, second_up) =
        sun_step(&block, &diag, &mut region, first_rise + 1.0);
    assert!(
        second_rise > first_rise,
        "crossing sunrise advances nextSunRise"
    );
    assert_eq!(second_set.to_bits(), first_set.to_bits());
    assert!(second_up, "after sunrise and before sunset sunUp is true");

    let (_, third_set, third_up) = sun_step(&block, &diag, &mut region, first_set + 1.0);
    assert!(third_set > first_set, "crossing sunset advances nextSunSet");
    assert!(!third_up, "after sunset sunUp is false");
    assert!(diag.events.borrow().is_empty());
}

#[test]
fn sun_rise_set_initializes_polar_day_with_sun_up_true() {
    let block = SunRiseSet {
        lat: 1.256_637_061_435_9,
        lon: -1.256_637_061_435_9,
        tim_zon: -18_000.0,
    };
    let diag = CapturingDiagnostics::default();
    let mut region = vec![0; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());

    let t = 172.0 * 86_400.0;
    let (next_rise, next_set, sun_up) = sun_step(&block, &diag, &mut region, t);
    assert!(next_rise.is_finite());
    assert!(next_set.is_finite());
    assert!(
        next_set < next_rise,
        "source polar-day initialization keeps next sunset before next sunrise"
    );
    assert!(sun_up);
    assert!(diag.events.borrow().is_empty());
}

#[test]
fn sun_rise_set_invalid_coordinates_warn_once_and_fail_closed() {
    let block = SunRiseSet {
        lat: std::f64::consts::PI,
        lon: 0.0,
        tim_zon: 0.0,
    };
    let diag = CapturingDiagnostics::default();
    let mut region = vec![0; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());

    let (rise, set, sun_up) = sun_step(&block, &diag, &mut region, 0.0);
    assert!(rise.is_nan());
    assert!(set.is_nan());
    assert!(!sun_up);
    let (rise_again, set_again, sun_up_again) = sun_step(&block, &diag, &mut region, 60.0);
    assert!(rise_again.is_nan());
    assert!(set_again.is_nan());
    assert!(!sun_up_again);

    let events = diag.events.borrow();
    assert_eq!(
        events.len(),
        1,
        "invalid coordinate warning is once per instance"
    );
    assert_eq!(events[0].0, "CDL.Utilities.SunRiseSet");
    assert!(events[0].1.contains("invalid coordinates"));
}

#[test]
fn registry_constructs_sun_rise_set_and_exposes_param_rules() {
    let entry = lookup("CDL.Utilities.SunRiseSet").expect("SunRiseSet registered");
    assert_eq!(
        entry.param_rules(),
        &[
            ParamRule::Required { name: "lat" },
            ParamRule::Required { name: "lon" },
            ParamRule::Required { name: "timZon" },
            ParamRule::RealGreaterOrEqual {
                name: "lat",
                min: -std::f64::consts::FRAC_PI_2,
            },
            ParamRule::RealLessOrEqualConstant {
                name: "lat",
                max: std::f64::consts::FRAC_PI_2,
            },
            ParamRule::RealGreaterOrEqual {
                name: "lon",
                min: -std::f64::consts::PI,
            },
            ParamRule::RealLessOrEqualConstant {
                name: "lon",
                max: std::f64::consts::PI,
            },
            ParamRule::RealFinite { name: "timZon" },
        ]
    );

    let block = (entry.make)(&ParamTable {
        values: vec![
            (Arc::from("lat"), Value::Real(0.645_771_823_237_9)),
            (Arc::from("lon"), Value::Real(-2.129_301_687_433_1)),
            (Arc::from("timZon"), Value::Real(-28_800.0)),
        ],
    });
    assert_eq!(block.signature().class_path, "CDL.Utilities.SunRiseSet");
    assert_eq!(block.kind(), BlockKind::Stateful);
}
