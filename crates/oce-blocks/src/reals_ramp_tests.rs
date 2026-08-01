//! Scenario tests for base `CDL.Reals.Ramp`.

use std::sync::Arc;

use oce_model::determinism::CANONICAL_NAN_BITS;
use oce_model::{ParamTable, Value};

use super::{Block, BlockKind, Ctx, NoopDiagnostics, PortKind, Ramp, lookup};

fn inputs(u: f64, active: bool) -> [Value; 2] {
    [Value::Real(u), Value::Boolean(active)]
}

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn tick(block: &dyn Block, region: &mut [u64], t: f64, u: f64, active: bool) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let vals = inputs(u, active);
    let mut out = None;
    block.emit_from_state(&cx, &vals, region, &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    block.update_state(&cx, &vals, region);
    out.expect("Ramp emits y")
}

fn drive(block: &dyn Block, steps: &[(f64, f64, bool)]) -> (Vec<Value>, Vec<u64>) {
    let mut region = init_region(block);
    let trace = steps
        .iter()
        .map(|&(t, u, active)| tick(block, &mut region, t, u, active))
        .collect();
    (trace, region)
}

fn parameter_table(values: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

fn registry_trace(parameters: &ParamTable, steps: &[(f64, f64, bool)]) -> Vec<Value> {
    let block = (lookup("CDL.Reals.Ramp").unwrap().make)(parameters);
    let mut region = vec![0; block.state_len()];
    block.init_state(&mut region, parameters);
    steps
        .iter()
        .map(|&(time, input, active)| tick(block.as_ref(), &mut region, time, input, active))
        .collect()
}

fn values_bit_equal(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.bit_eq(right))
}

#[test]
fn derivative_time_constant_tracks_raising_rate_and_changes_the_trace() {
    let derived = parameter_table(&[
        ("raisingSlewRate", Value::Real(2.0)),
        ("fallingSlewRate", Value::Real(-100.0)),
    ]);
    let explicit = parameter_table(&[
        ("raisingSlewRate", Value::Real(2.0)),
        ("fallingSlewRate", Value::Real(-100.0)),
        ("Td", Value::Real(0.002)),
    ]);
    let sensitive = parameter_table(&[
        ("raisingSlewRate", Value::Real(2.0)),
        ("fallingSlewRate", Value::Real(-100.0)),
        ("Td", Value::Real(0.01)),
    ]);
    let steps = [(0.0, 0.0, true), (0.001, -0.1, true), (0.002, -0.1, true)];
    let derived_trace = registry_trace(&derived, &steps);
    assert!(values_bit_equal(
        &derived_trace,
        &registry_trace(&explicit, &steps)
    ));
    assert!(!values_bit_equal(
        &derived_trace,
        &registry_trace(&sensitive, &steps)
    ));
}

fn assert_trace_bits(got: &[Value], want: &[u64]) {
    assert_eq!(got.len(), want.len());
    for (idx, (got, want)) in got.iter().zip(want).enumerate() {
        let want = Value::Real(f64::from_bits(*want));
        assert!(got.bit_eq(&want), "trace[{idx}] got {got:?}, want {want:?}");
    }
}

#[test]
fn signature_state_and_feedthrough_are_pinned() {
    let ramp = Ramp::default();
    assert_eq!(ramp.signature().class_path, "CDL.Reals.Ramp");
    assert_eq!(
        ramp.signature().inputs,
        &[PortKind::Real, PortKind::Boolean]
    );
    assert_eq!(ramp.signature().outputs, &[PortKind::Real]);
    assert_eq!(ramp.kind(), BlockKind::Stateful);
    assert_eq!(ramp.state_len(), 3);
    assert!(ramp.feeds_through(0, 0));
    assert!(ramp.feeds_through(1, 0));
}

#[test]
fn inactive_path_passes_current_input_and_holds_internal_state() {
    let ramp = Ramp {
        raising_slew_rate: 2.0,
        falling_slew_rate: -2.0,
        td: 1.0,
    };
    let (trace, region) = drive(
        &ramp,
        &[
            (0.0, 1.0, true),
            (1.0, 3.0, true),
            (2.0, -9.0, false),
            (3.0, -8.0, false),
        ],
    );
    assert_trace_bits(
        &trace,
        &[
            1.0f64.to_bits(),
            2.0f64.to_bits(),
            (-9.0f64).to_bits(),
            (-8.0f64).to_bits(),
        ],
    );
    assert_eq!(
        region[0],
        2.0f64.to_bits(),
        "source y_internal holds while inactive even though y passes current u through"
    );
}

#[test]
fn active_rising_reinitializes_to_current_input_on_the_same_tick() {
    let ramp = Ramp {
        raising_slew_rate: 0.5,
        falling_slew_rate: -0.5,
        td: 1.0,
    };
    let (trace, region) = drive(
        &ramp,
        &[
            (0.0, 0.0, false),
            (1.0, 10.0, true),
            (2.0, 20.0, true),
            (3.0, 20.0, true),
        ],
    );
    assert_trace_bits(
        &trace,
        &[
            0.0f64.to_bits(),
            10.0f64.to_bits(),
            10.5f64.to_bits(),
            11.0f64.to_bits(),
        ],
    );
    assert_eq!(region[0], 11.0f64.to_bits());
}

#[test]
fn held_active_limits_rising_and_falling_steps() {
    let ramp = Ramp {
        raising_slew_rate: 2.0,
        falling_slew_rate: -3.0,
        td: 0.1,
    };
    let (trace, region) = drive(
        &ramp,
        &[(0.0, 0.0, true), (1.0, 10.0, true), (2.0, -10.0, true)],
    );
    assert_trace_bits(
        &trace,
        &[0.0f64.to_bits(), 2.0f64.to_bits(), (-1.0f64).to_bits()],
    );
    assert_eq!(region[0], (-1.0f64).to_bits());
}

#[test]
fn default_td_is_ramp_specific_not_limit_slew_rate_default() {
    let ramp = Ramp {
        raising_slew_rate: 2.0,
        falling_slew_rate: -2.0,
        td: 2.0 * 0.001,
    };
    let (trace, _) = drive(&ramp, &[(0.0, 0.0, true), (1.0, 10.0, true)]);
    assert_trace_bits(&trace, &[0.0f64.to_bits(), 2.0f64.to_bits()]);
}

#[test]
fn output_and_state_words_are_deterministic() {
    let ramp = Ramp {
        raising_slew_rate: 1.25,
        falling_slew_rate: -0.75,
        td: 0.4,
    };
    let steps = [
        (0.0, 0.25, false),
        (0.2, 0.50, true),
        (0.4, 1.50, true),
        (0.7, -1.00, true),
        (0.9, 4.00, false),
        (1.1, 2.00, true),
    ];
    let first = drive(&ramp, &steps);
    let second = drive(&ramp, &steps);
    assert_eq!(first.0.len(), second.0.len());
    for (idx, (a, b)) in first.0.iter().zip(&second.0).enumerate() {
        assert!(a.bit_eq(b), "trace[{idx}] {a:?} vs {b:?}");
    }
    assert_eq!(first.1, second.1);
}

#[test]
fn non_finite_inputs_are_panic_free_and_nan_outputs_are_canonicalized() {
    let ramp = Ramp {
        raising_slew_rate: 1.0,
        falling_slew_rate: -1.0,
        td: 1.0,
    };

    let negative_nan = f64::from_bits(0xfff8_0000_0000_0001);
    let (inactive_trace, inactive_region) = drive(
        &ramp,
        &[(0.0, negative_nan, false), (1.0, f64::INFINITY, false)],
    );
    assert!(
        inactive_trace[0].bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))),
        "inactive pass-through must canonicalize NaN output bits"
    );
    assert!(inactive_trace[1].bit_eq(&Value::Real(f64::INFINITY)));
    assert_eq!(
        inactive_region[0], CANONICAL_NAN_BITS,
        "state storage must canonicalize NaN payloads even while inactive output passes through"
    );

    let alternate_nan = f64::from_bits(0x7ff0_0000_0000_0001);
    let (_, alternate_region) = drive(&ramp, &[(0.0, alternate_nan, true)]);
    assert_eq!(
        alternate_region[0], CANONICAL_NAN_BITS,
        "active initial reinitialization must also canonicalize NaN payloads"
    );

    let (active_trace, active_region) = drive(
        &ramp,
        &[
            (0.0, 0.0, true),
            (1.0, f64::INFINITY, true),
            (2.0, f64::NEG_INFINITY, true),
        ],
    );
    assert!(active_trace[0].bit_eq(&Value::Real(0.0)));
    assert!(active_trace[1].bit_eq(&Value::Real(1.0)));
    assert!(active_trace[2].bit_eq(&Value::Real(0.0)));
    assert_eq!(
        active_region[0],
        0.0f64.to_bits(),
        "infinite targets remain bounded by configured slew rates"
    );
}
