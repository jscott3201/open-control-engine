//! Behavioral cross-checks for defaults derived from other resolved parameters.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use crate::{Block, Ctx, NoopDiagnostics, lookup};

fn params(values: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

fn assert_values_bit_equal(left: &[Value], right: &[Value]) {
    assert_eq!(left.len(), right.len());
    assert!(
        left.iter()
            .zip(right)
            .all(|(left, right)| left.bit_eq(right))
    );
}

fn stateful_trace(block: &dyn Block, steps: &[(f64, Vec<Value>)]) -> Vec<Vec<Value>> {
    let mut region = vec![0; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    let diagnostics = NoopDiagnostics;
    steps
        .iter()
        .map(|(time, inputs)| {
            let context = Ctx::new(*time, &diagnostics);
            let mut outputs = Vec::new();
            block.emit_from_state(&context, inputs, &region, &mut |index, value| {
                assert_eq!(index, outputs.len());
                outputs.push(value);
            });
            block.update_state(&context, inputs, &mut region);
            outputs
        })
        .collect()
}

fn assert_stateful_traces_bit_equal(
    left: &dyn Block,
    right: &dyn Block,
    steps: &[(f64, Vec<Value>)],
) {
    let left = stateful_trace(left, steps);
    let right = stateful_trace(right, steps);
    assert_eq!(left.len(), right.len());
    for (left, right) in left.iter().zip(right.iter()) {
        assert_values_bit_equal(left, right);
    }
}

#[test]
fn slew_family_derived_rates_and_time_constants_match_explicit_values() {
    let slew_entry = lookup("CDL.Reals.LimitSlewRate").unwrap();
    let slew_derived = (slew_entry.make)(&params(&[("raisingSlewRate", Value::Real(2.0))]));
    let slew_explicit = (slew_entry.make)(&params(&[
        ("raisingSlewRate", Value::Real(2.0)),
        ("fallingSlewRate", Value::Real(-2.0)),
        ("Td", Value::Real(20.0)),
    ]));
    let slew_steps = [
        (0.0, vec![Value::Real(0.0)]),
        (1.0, vec![Value::Real(10.0)]),
        (2.0, vec![Value::Real(-10.0)]),
    ];
    assert_stateful_traces_bit_equal(slew_derived.as_ref(), slew_explicit.as_ref(), &slew_steps);

    let ramp_entry = lookup("CDL.Reals.Ramp").unwrap();
    let ramp_derived = (ramp_entry.make)(&params(&[("raisingSlewRate", Value::Real(2.0))]));
    let ramp_explicit = (ramp_entry.make)(&params(&[
        ("raisingSlewRate", Value::Real(2.0)),
        ("fallingSlewRate", Value::Real(-2.0)),
        ("Td", Value::Real(0.002)),
    ]));
    let ramp_steps = [
        (0.0, vec![Value::Real(0.0), Value::Boolean(true)]),
        (0.001, vec![Value::Real(10.0), Value::Boolean(true)]),
        (0.002, vec![Value::Real(-10.0), Value::Boolean(true)]),
    ];
    assert_stateful_traces_bit_equal(ramp_derived.as_ref(), ramp_explicit.as_ref(), &ramp_steps);
}

#[test]
fn timing_stage_and_reset_derived_scalars_match_explicit_values() {
    let hold_entry = lookup("CDL.Logical.TrueFalseHold").unwrap();
    let hold_derived = (hold_entry.make)(&params(&[("trueHoldDuration", Value::Real(0.5))]));
    let hold_explicit = (hold_entry.make)(&params(&[
        ("trueHoldDuration", Value::Real(0.5)),
        ("falseHoldDuration", Value::Real(0.5)),
    ]));
    let hold_steps = [
        (0.0, vec![Value::Boolean(false)]),
        (0.1, vec![Value::Boolean(true)]),
        (0.7, vec![Value::Boolean(false)]),
        (1.3, vec![Value::Boolean(false)]),
    ];
    assert_stateful_traces_bit_equal(hold_derived.as_ref(), hold_explicit.as_ref(), &hold_steps);

    let pulse_entry = lookup("CDL.Logical.VariablePulse").unwrap();
    let pulse_derived = (pulse_entry.make)(&params(&[("period", Value::Real(2.0))]));
    let pulse_explicit = (pulse_entry.make)(&params(&[
        ("period", Value::Real(2.0)),
        ("minTruFalHol", Value::Real(0.02)),
    ]));
    let pulse_steps = [
        (0.0, vec![Value::Real(0.25)]),
        (0.5, vec![Value::Real(0.25)]),
        (1.0, vec![Value::Real(0.75)]),
        (2.0, vec![Value::Real(0.75)]),
    ];
    assert_stateful_traces_bit_equal(
        pulse_derived.as_ref(),
        pulse_explicit.as_ref(),
        &pulse_steps,
    );

    let stage_entry = lookup("CDL.Integers.Stage").unwrap();
    let stage_derived = (stage_entry.make)(&params(&[
        ("n", Value::Integer(4)),
        ("holdDuration", Value::Real(0.0)),
    ]));
    let stage_explicit = (stage_entry.make)(&params(&[
        ("n", Value::Integer(4)),
        ("holdDuration", Value::Real(0.0)),
        ("h", Value::Real(0.005)),
    ]));
    let stage_steps = [
        (0.0, vec![Value::Real(0.0)]),
        (1.0, vec![Value::Real(0.6)]),
        (2.0, vec![Value::Real(1.0)]),
    ];
    assert_stateful_traces_bit_equal(
        stage_derived.as_ref(),
        stage_explicit.as_ref(),
        &stage_steps,
    );
}

fn algebraic_outputs(class_path: &str, parameters: ParamTable, inputs: &[Value]) -> Vec<Value> {
    let block = (lookup(class_path).unwrap().make)(&parameters);
    let context = Ctx::new(0.0, &NoopDiagnostics);
    let mut outputs = Vec::new();
    block.step_algebraic(&context, inputs, &mut |index, value| {
        assert_eq!(index, outputs.len());
        outputs.push(value);
    });
    outputs
}

#[test]
fn width_indexed_derived_defaults_match_explicit_elements_for_every_typed_family() {
    for (class_path, inputs) in [
        (
            "CDL.Routing.BooleanExtractSignal",
            vec![Value::Boolean(false), Value::Boolean(true)],
        ),
        (
            "CDL.Routing.IntegerExtractSignal",
            vec![Value::Integer(7), Value::Integer(9)],
        ),
        (
            "CDL.Routing.RealExtractSignal",
            vec![Value::Real(7.0), Value::Real(9.0)],
        ),
    ] {
        let derived = algebraic_outputs(
            class_path,
            params(&[("nin", Value::Integer(2)), ("nout", Value::Integer(2))]),
            &inputs,
        );
        let explicit = algebraic_outputs(
            class_path,
            params(&[
                ("nin", Value::Integer(2)),
                ("nout", Value::Integer(2)),
                ("extract_1", Value::Integer(1)),
                ("extract_2", Value::Integer(2)),
            ]),
            &inputs,
        );
        assert_values_bit_equal(&derived, &explicit);
    }

    for (class_path, inputs) in [
        (
            "CDL.Routing.BooleanVectorFilter",
            vec![Value::Boolean(false), Value::Boolean(true)],
        ),
        (
            "CDL.Routing.IntegerVectorFilter",
            vec![Value::Integer(7), Value::Integer(9)],
        ),
        (
            "CDL.Routing.RealVectorFilter",
            vec![Value::Real(7.0), Value::Real(9.0)],
        ),
    ] {
        let derived = algebraic_outputs(
            class_path,
            params(&[("nin", Value::Integer(2)), ("nout", Value::Integer(2))]),
            &inputs,
        );
        let explicit = algebraic_outputs(
            class_path,
            params(&[
                ("nin", Value::Integer(2)),
                ("nout", Value::Integer(2)),
                ("msk_1", Value::Boolean(true)),
                ("msk_2", Value::Boolean(true)),
            ]),
            &inputs,
        );
        assert_values_bit_equal(&derived, &explicit);
    }

    let matrix_inputs = [Value::Real(7.0), Value::Real(9.0)];
    let derived = algebraic_outputs(
        "CDL.Reals.MatrixGain",
        params(&[("nin", Value::Integer(2)), ("nout", Value::Integer(2))]),
        &matrix_inputs,
    );
    let explicit = algebraic_outputs(
        "CDL.Reals.MatrixGain",
        params(&[
            ("nin", Value::Integer(2)),
            ("nout", Value::Integer(2)),
            ("K_1_1", Value::Real(1.0)),
            ("K_1_2", Value::Real(0.0)),
            ("K_2_1", Value::Real(0.0)),
            ("K_2_2", Value::Real(1.0)),
        ]),
        &matrix_inputs,
    );
    assert_values_bit_equal(&derived, &explicit);
}
