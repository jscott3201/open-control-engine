//! Scenario tests for `CDL.Integers.Stage`.

use std::sync::Arc;

use oce_model::{ParamTable, Value};

use super::{Block, BlockKind, Ctx, IntegerStage, NoopDiagnostics, PortKind, lookup};

fn init_region(block: &dyn Block) -> Vec<u64> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &ParamTable::default());
    region
}

fn tick(block: &dyn Block, region: &mut [u64], t: f64, u: f64) -> i64 {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let inputs = [Value::Real(u)];
    let mut out = None;
    block.emit_from_state(&cx, &inputs, region, &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    block.update_state(&cx, &inputs, region);
    match out.expect("Stage emits y") {
        Value::Integer(y) => y,
        other => panic!("expected integer y, got {other:?}"),
    }
}

fn run(block: &dyn Block, steps: &[(f64, f64)]) -> (Vec<i64>, Vec<u64>) {
    let mut region = init_region(block);
    let trace = steps
        .iter()
        .map(|&(t, u)| tick(block, &mut region, t, u))
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

fn registry_trace(parameters: &ParamTable, steps: &[(f64, f64)]) -> Vec<i64> {
    let block = (lookup("CDL.Integers.Stage").unwrap().make)(parameters);
    let mut region = vec![0; block.state_len()];
    block.init_state(&mut region, parameters);
    steps
        .iter()
        .map(|&(time, input)| tick(block.as_ref(), &mut region, time, input))
        .collect()
}

#[test]
fn hysteresis_tracks_stage_count_and_changes_the_trace() {
    let derived = parameter_table(&[("n", Value::Integer(4)), ("holdDuration", Value::Real(0.0))]);
    let explicit = parameter_table(&[
        ("n", Value::Integer(4)),
        ("holdDuration", Value::Real(0.0)),
        ("h", Value::Real(0.005)),
    ]);
    let sensitive = parameter_table(&[
        ("n", Value::Integer(4)),
        ("holdDuration", Value::Real(0.0)),
        ("h", Value::Real(0.01)),
    ]);
    let steps = [(0.0, 0.0), (1.0, 0.007), (2.0, 0.007)];
    let derived_trace = registry_trace(&derived, &steps);
    assert_eq!(derived_trace, registry_trace(&explicit, &steps));
    assert_ne!(derived_trace, registry_trace(&sensitive, &steps));
}

fn second_tick_y(block: &dyn Block, u: f64) -> i64 {
    let (trace, _) = run(block, &[(0.0, 0.0), (1.0, u)]);
    trace[1]
}

#[test]
fn source_signature_and_feedthrough_are_pinned() {
    let stage = IntegerStage::default();
    assert_eq!(stage.signature().class_path, "CDL.Integers.Stage");
    assert_eq!(stage.signature().inputs, &[PortKind::Real]);
    assert_eq!(stage.signature().outputs, &[PortKind::Integer]);
    assert_eq!(stage.kind(), BlockKind::Stateful);
    assert_eq!(stage.state_len(), 8);
    assert!(stage.feeds_through(0, 0));
}

#[test]
fn hold_duration_preserves_pre_y_start_until_guard_opens() {
    let stage = IntegerStage {
        n: 4,
        hold_duration: 2.0,
        h: 0.05,
        pre_y_start: 0,
    };
    let (got, _) = run(
        &stage,
        &[(0.0, 1.0), (1.0, 1.0), (2.0, 1.0), (3.0, 0.0), (4.0, 0.0)],
    );
    assert_eq!(got, [0, 0, 4, 4, 1]);
}

#[test]
fn elapsed_deadline_remains_valid_while_the_stage_condition_is_inactive() {
    let stage = IntegerStage {
        n: 4,
        hold_duration: 2.0,
        h: 0.05,
        pre_y_start: 0,
    };
    let (_, region) = run(&stage, &[(0.0, 0.0), (10.0, 0.0)]);
    assert!(f64::from_bits(region[1]) < 10.0);
    stage.validate_state(&region, 10.0, 10.0).unwrap();
}

#[test]
fn stage_thresholds_are_lower_inclusive_and_top_threshold_selects_n() {
    let stage = IntegerStage {
        n: 4,
        hold_duration: 0.0,
        h: 0.001,
        pre_y_start: 0,
    };
    assert_eq!(second_tick_y(&stage, 0.25_f64.next_down()), 1);
    assert_eq!(second_tick_y(&stage, 0.25), 2);
    assert_eq!(second_tick_y(&stage, 0.5_f64.next_down()), 2);
    assert_eq!(second_tick_y(&stage, 0.5), 3);
    assert_eq!(second_tick_y(&stage, 0.75_f64.next_down()), 3);
    assert_eq!(second_tick_y(&stage, 0.75), 4);
}

#[test]
fn hysteresis_holds_stage_until_lower_band_is_crossed() {
    let stage = IntegerStage {
        n: 4,
        hold_duration: 0.0,
        h: 0.05,
        pre_y_start: 0,
    };
    let (got, _) = run(
        &stage,
        &[
            (0.0, 0.0),
            (1.0, 0.31),
            (2.0, 0.46),
            (3.0, 0.44),
            (4.0, 0.19),
        ],
    );
    assert_eq!(got, [0, 2, 2, 2, 1]);
}

#[test]
fn zero_is_not_clamped_away_despite_output_min_annotation() {
    let stage = IntegerStage {
        n: 4,
        hold_duration: 0.0,
        h: 0.05,
        pre_y_start: 3,
    };
    let (got, _) = run(&stage, &[(0.0, 0.0), (1.0, -0.10), (2.0, 0.0)]);
    assert_eq!(got, [3, 0, 0]);
}

#[test]
fn when_condition_must_rearm_before_another_event_fires() {
    let stage = IntegerStage {
        n: 4,
        hold_duration: 0.0,
        h: 0.05,
        pre_y_start: 3,
    };
    let (got, _) = run(&stage, &[(0.0, 0.06), (1.0, -0.10), (2.0, 0.06)]);
    assert_eq!(got, [3, 3, 3]);
}

#[test]
fn n_one_uses_single_zero_threshold() {
    let stage = IntegerStage {
        n: 1,
        hold_duration: 0.0,
        h: 0.02,
        pre_y_start: 0,
    };
    let (got, _) = run(&stage, &[(0.0, 0.0), (1.0, 0.03), (2.0, 0.0)]);
    assert_eq!(got, [0, 1, 1]);
}

#[test]
fn emitted_trace_and_state_words_are_deterministic() {
    let stage = IntegerStage {
        n: 4,
        hold_duration: 1.5,
        h: 0.05,
        pre_y_start: 0,
    };
    let steps = [
        (0.0, 0.0),
        (1.0, 0.31),
        (1.5, 0.31),
        (2.0, 0.44),
        (3.0, 0.19),
        (3.5, 0.19),
    ];
    let first = run(&stage, &steps);
    let second = run(&stage, &steps);
    assert_eq!(first.0, second.0);
    assert_eq!(first.1, second.1);
}
