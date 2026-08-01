//! Scenario tests for `CDL.Logical.VariablePulse`.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, Ctx, LogicalVariablePulse, NoopDiagnostics, ParamRule, PortKind, lookup,
};

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn tick_at(block: &dyn Block, region: &mut [u64], t: f64, u: f64) -> bool {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let inputs = [Value::Real(u)];
    let mut out = None;
    block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
        assert_eq!(idx, 0, "VariablePulse has one output");
        let Value::Boolean(y) = val else {
            panic!("expected Boolean output, got {val:?}");
        };
        out = Some(y);
    });
    block.update_state(&cx, &inputs, region);
    out.expect("VariablePulse must emit one output")
}

fn run(block: &dyn Block, steps: &[(f64, f64)]) -> Vec<bool> {
    let mut region = init_region(block);
    steps
        .iter()
        .map(|&(t, u)| tick_at(block, &mut region, t, u))
        .collect()
}

fn parameter_table(values: &[(&str, Value)]) -> ParamTable {
    ParamTable {
        values: values
            .iter()
            .map(|(name, value)| (Arc::from(*name), value.clone()))
            .collect(),
    }
}

fn registry_trace(parameters: &ParamTable, steps: &[(f64, f64)]) -> Vec<bool> {
    let block = (lookup("CDL.Logical.VariablePulse").unwrap().make)(parameters);
    let mut region = vec![0; block.state_len()];
    block.init_state(&mut region, parameters);
    steps
        .iter()
        .map(|&(time, input)| tick_at(block.as_ref(), &mut region, time, input))
        .collect()
}

#[test]
fn minimum_hold_tracks_period_and_changes_the_trace() {
    let derived = parameter_table(&[("period", Value::Real(1.0))]);
    let explicit = parameter_table(&[
        ("period", Value::Real(1.0)),
        ("minTruFalHol", Value::Real(0.01)),
    ]);
    let sensitive = parameter_table(&[
        ("period", Value::Real(1.0)),
        ("minTruFalHol", Value::Real(0.05)),
    ]);
    let steps = [
        (0.0, 0.01),
        (0.011, 0.01),
        (0.02, 0.01),
        (1.0, 0.01),
        (1.011, 0.01),
    ];
    let derived_trace = registry_trace(&derived, &steps);
    assert_eq!(derived_trace, registry_trace(&explicit, &steps));
    assert_ne!(derived_trace, registry_trace(&sensitive, &steps));
}

fn variable_pulse(period: f64, delta_u: f64, min_true_false_hold: f64) -> LogicalVariablePulse {
    LogicalVariablePulse {
        period,
        delta_u,
        min_true_false_hold,
    }
}

#[test]
fn registry_rules_pin_source_parameter_bounds_and_warning() {
    assert_eq!(
        lookup("CDL.Logical.VariablePulse").unwrap().param_rules(),
        &[
            ParamRule::Required { name: "period" },
            ParamRule::RealGreaterOrEqual {
                name: "deltaU",
                min: 0.001,
            },
            ParamRule::RealLessOrEqualConstant {
                name: "deltaU",
                max: 0.5,
            },
            ParamRule::RealGreaterOrEqual {
                name: "minTruFalHol",
                min: 1e-37,
            },
            ParamRule::RealGreaterOrEqualScaledWarning {
                left: "period",
                right: "minTruFalHol",
                factor: 2.0,
            },
        ]
    );
}

#[test]
fn signature_and_state_contract_are_pinned() {
    let block = LogicalVariablePulse::default();
    assert_eq!(block.kind(), BlockKind::Stateful);
    assert_eq!(block.state_len(), 5);
    assert!(block.feeds_through(0, 0));
    assert_eq!(block.signature().class_path, "CDL.Logical.VariablePulse");
    assert_eq!(block.signature().inputs, &[PortKind::Real]);
    assert_eq!(block.signature().outputs, &[PortKind::Boolean]);

    let entry = lookup("CDL.Logical.VariablePulse").expect("registry entry");
    let constructed = (entry.make)(&ParamTable::default());
    assert_eq!(
        constructed.signature().class_path,
        "CDL.Logical.VariablePulse"
    );
}

#[test]
fn constant_zero_and_one_inputs_are_constant_outputs() {
    let block = variable_pulse(4.0, 0.01, 0.04);
    assert_eq!(
        run(&block, &[(0.0, 0.0), (1.0, 0.0), (4.0, 0.0)]),
        vec![false, false, false]
    );
    assert_eq!(
        run(&block, &[(0.0, 1.0), (1.0, 1.0), (4.0, 1.0)]),
        vec![true, true, true]
    );
}

#[test]
fn duty_ratio_boundaries_are_rising_inclusive_and_falling_exclusive() {
    let block = variable_pulse(4.0, 0.01, 0.04);
    assert_eq!(
        run(
            &block,
            &[
                (0.0, 0.75),
                (2.999_999, 0.75),
                (3.0, 0.75),
                (3.5, 0.75),
                (4.0, 0.75)
            ]
        ),
        vec![true, true, false, false, true]
    );
}

#[test]
fn width_changes_above_delta_reset_cycle_anchor() {
    let block = variable_pulse(4.0, 0.125, 0.04);
    assert_eq!(
        run(
            &block,
            &[(0.0, 0.25), (1.0, 0.25), (2.0, 0.375), (2.0, 0.75)]
        ),
        vec![true, false, false, true],
        "diff equal to deltaU must not reset, but a larger diff starts a new pulse at the same time"
    );
}

#[test]
fn minimum_hold_delays_output_change_after_width_reset() {
    let block = variable_pulse(3.0, 0.01, 1.0);
    assert_eq!(
        run(
            &block,
            &[
                (0.0, 0.5),
                (1.5, 0.5),
                (1.6, 0.9),
                (2.49, 0.9),
                (2.6, 0.9),
                (3.2, 0.5),
                (4.7, 0.5),
                (5.0, 0.5),
            ],
        ),
        vec![true, false, false, false, true, true, false, false]
    );
}

#[test]
fn short_period_uses_adjusted_period_from_minimum_hold_warning_path() {
    let block = variable_pulse(0.0, 0.01, 1.0);
    assert_eq!(
        run(&block, &[(0.0, 0.5), (0.75, 0.5), (1.02, 0.5)]),
        vec![true, true, false],
        "adjustedPeriod=max(period,2.02*minTruFalHol) keeps t=0.75 high and t=1.02 low"
    );
}

#[test]
fn direct_invalid_timing_parameters_degrade_without_panicking() {
    let block = variable_pulse(f64::NAN, f64::NAN, -1.0);
    assert_eq!(
        run(&block, &[(0.0, 0.5), (0.5, 0.5), (1.0, 0.5)]),
        vec![true, false, true]
    );

    let non_finite_input = variable_pulse(1.0, 0.01, 0.01);
    assert_eq!(run(&non_finite_input, &[(0.0, f64::NAN)]), vec![false]);
}

#[test]
fn repeated_runs_are_deterministic() {
    let block = variable_pulse(3.0, 0.01, 1.0);
    let steps = [
        (0.0, 0.5),
        (1.5, 0.5),
        (1.6, 0.9),
        (2.49, 0.9),
        (2.6, 0.9),
        (3.2, 0.5),
        (4.7, 0.5),
        (5.0, 0.5),
    ];
    let mut left_region = init_region(&block);
    let left = steps
        .iter()
        .map(|&(t, u)| tick_at(&block, &mut left_region, t, u))
        .collect::<Vec<_>>();
    let mut right_region = init_region(&block);
    let right = steps
        .iter()
        .map(|&(t, u)| tick_at(&block, &mut right_region, t, u))
        .collect::<Vec<_>>();
    assert_eq!(left, right);
    assert_eq!(left_region, right_region);
}
