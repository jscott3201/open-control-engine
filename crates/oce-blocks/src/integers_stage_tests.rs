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

/// The source equations' literal stage search (the loop `stage_for_u` replaced), kept here as
/// the oracle for the exact binary search.
fn literal_source_stage(n: i64, u: f64) -> i64 {
    let n = n.max(1);
    let threshold = |stage_idx: i64| (stage_idx - 1) as f64 / n as f64;
    if u >= threshold(n) {
        return n;
    }
    for i in 2..=n {
        if u < threshold(i) && u >= threshold(i - 1) {
            return i - 1;
        }
    }
    0
}

fn stage_with_n(n: i64) -> IntegerStage {
    IntegerStage {
        n,
        hold_duration: 0.0,
        h: 0.001,
        pre_y_start: 0,
    }
}

/// Inputs that probe every threshold of an `n`-stage block exactly and one ulp either side, plus
/// the non-finite and signed-zero edge values.
fn probe_inputs(n: i64) -> Vec<f64> {
    let mut inputs = vec![
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.0,
        -0.0,
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        1.0,
        2.0,
        -1.0,
        f64::MAX,
        f64::MIN,
    ];
    for k in 1..=n {
        let threshold = (k - 1) as f64 / n as f64;
        inputs.extend([threshold, threshold.next_down(), threshold.next_up()]);
    }
    inputs
}

#[test]
fn exact_stage_search_matches_the_literal_source_loop_on_every_threshold() {
    for n in [-3, 0, 1, 2, 3, 4, 5, 7, 10, 64, 100, 257, 1000, 4099] {
        let stage = stage_with_n(n);
        for u in probe_inputs(n.max(1)) {
            assert_eq!(
                stage.stage_for_u(u),
                literal_source_stage(n, u),
                "n={n} u={u:e} (bits {:#x})",
                u.to_bits()
            );
        }
    }
}

#[test]
fn exact_stage_search_matches_the_literal_source_loop_on_a_dense_input_sweep() {
    for n in [3, 6, 9, 11, 49, 333] {
        let stage = stage_with_n(n);
        for step in -20..=2_020 {
            let u = f64::from(step) / 2_000.0;
            assert_eq!(
                stage.stage_for_u(u),
                literal_source_stage(n, u),
                "n={n} u={u}"
            );
        }
    }
}

/// A hostile authored `n` must not stall a tick: the source loop would run `n - 1` iterations on
/// every stage event. The exact search answers in at most `64` halvings even at `i64::MAX`, so
/// this test finishing at all is the bound; the answers are checked against the threshold
/// definition (the loop itself is infeasible to run at this `n`).
#[test]
fn hostile_stage_count_answers_in_logarithmic_steps_without_overflow() {
    for n in [i64::MAX, i64::MAX - 1, 1 << 53, (1 << 53) + 1, 1 << 40] {
        let stage = stage_with_n(n);
        let threshold = |k: i64| (k - 1) as f64 / n as f64;
        assert_eq!(stage.stage_for_u(1.0), n, "u=1 selects every stage, n={n}");
        assert_eq!(
            stage.stage_for_u(f64::NAN),
            0,
            "NaN selects no stage, n={n}"
        );
        assert_eq!(
            stage.stage_for_u(-0.5),
            0,
            "negative u selects no stage, n={n}"
        );
        for u in [0.0, 0.25, 0.5, 0.75, 0.999_999] {
            let y = stage.stage_for_u(u);
            assert!((1..n).contains(&y), "n={n} u={u} y={y}");
            assert!(
                threshold(y) <= u && u < threshold(y + 1),
                "n={n} u={u} y={y}"
            );
        }
    }
    // Through the block API: a registry-built Stage with an `i64::MAX` stage count ticks.
    let parameters = parameter_table(&[
        ("n", Value::Integer(i64::MAX)),
        ("holdDuration", Value::Real(0.0)),
        ("h", Value::Real(0.0)),
    ]);
    let trace = registry_trace(&parameters, &[(0.0, 0.0), (1.0, 1.0), (2.0, 0.5)]);
    assert_eq!(trace[1], i64::MAX);
}
