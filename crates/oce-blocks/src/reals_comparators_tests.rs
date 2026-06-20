//! M2-PR-A1 scalar `CDL.Reals` algebraic tests. Expected numeric values are hand-derived from
//! `_spec/03` §4.1 and pinned by IEEE-754 bits where exact equality matters.

use oce_model::Value;

use super::{
    Block, BlockKind, Ctx, Greater, GreaterThreshold, Hysteresis, Less, LessThreshold,
    NoopDiagnostics,
};

fn real_inputs(xs: &[f64]) -> Vec<Value> {
    xs.iter().copied().map(Value::Real).collect()
}

fn bool_out(block: &dyn Block, inputs: &[f64]) -> bool {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut out = None;
    block.step_algebraic(&cx, &real_inputs(inputs), &mut |idx, val| {
        assert_eq!(idx, 0, "A2 comparator blocks have one output");
        out = Some(val);
    });
    match out.expect("block must emit one output") {
        Value::Boolean(y) => y,
        other => panic!("expected Boolean output, got {other:?}"),
    }
}

fn drive_bool_values(block: &dyn Block, steps: &[Vec<f64>]) -> Vec<Value> {
    let mut region = vec![0u64; block.state_len()];
    block.init_state(&mut region, &oce_model::ParamTable::default());
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let mut trace = Vec::with_capacity(steps.len());
    for step in steps {
        let inputs = real_inputs(step);
        let mut out = None;
        block.emit_from_state(&cx, &inputs, &region, &mut |idx, val| {
            assert_eq!(idx, 0, "A2 comparator blocks have one output");
            out = Some(val);
        });
        block.update_state(&cx, &inputs, &mut region);
        trace.push(out.expect("stateful comparator must emit one output"));
    }
    trace
}

fn drive_bool(block: &dyn Block, steps: &[Vec<f64>]) -> Vec<bool> {
    drive_bool_values(block, steps)
        .into_iter()
        .map(|v| match v {
            Value::Boolean(y) => y,
            other => panic!("expected Boolean output, got {other:?}"),
        })
        .collect()
}

fn assert_bool_perturb_moves(block: &dyn Block, base: &[f64], variants: &[(usize, f64)]) {
    let base_y = if block.kind() == BlockKind::Algebraic {
        bool_out(block, base)
    } else {
        drive_bool(block, &[base.to_vec()])[0]
    };
    for &(idx, replacement) in variants {
        assert!(block.feeds_through(idx, 0));
        let mut changed = base.to_vec();
        changed[idx] = replacement;
        let y = if block.kind() == BlockKind::Algebraic {
            bool_out(block, &changed)
        } else {
            drive_bool(block, &[changed])[0]
        };
        assert_ne!(
            y, base_y,
            "perturbing input {idx} must move comparator output for R-FT-5"
        );
    }
}
#[test]
fn comparators_h0_are_combinational_and_bit_identical_to_plain_relations() {
    assert_eq!(Greater::default().kind(), BlockKind::Algebraic);
    assert_eq!(Greater::default().state_len(), 0);
    assert!(!bool_out(&Greater::default(), &[2.0, 2.0]));
    assert!(bool_out(
        &Greater::default(),
        &[f64::from_bits(2.0f64.to_bits() + 1), 2.0]
    ));

    assert_eq!(Less::default().kind(), BlockKind::Algebraic);
    assert!(!bool_out(&Less::default(), &[2.0, 2.0]));
    assert!(bool_out(
        &Less::default(),
        &[f64::from_bits(2.0f64.to_bits() - 1), 2.0]
    ));

    let gt = GreaterThreshold {
        t: 0.3,
        h: 0.0,
        pre_y_start: true,
    };
    assert_eq!(gt.kind(), BlockKind::Algebraic);
    assert_eq!(gt.state_len(), 0);
    assert!(!bool_out(&gt, &[0.3]));
    assert!(bool_out(&gt, &[0.1 + 0.2 + f64::EPSILON]));

    let lt = LessThreshold {
        t: 0.3,
        h: 0.0,
        pre_y_start: true,
    };
    assert_eq!(lt.kind(), BlockKind::Algebraic);
    assert_eq!(lt.state_len(), 0);
    assert!(!bool_out(&lt, &[0.3]));
    assert!(bool_out(&lt, &[0.1 + 0.2 - f64::EPSILON]));

    let mut empty = [];
    gt.init_state(&mut empty, &oce_model::ParamTable::default());
    assert!(
        !bool_out(&gt, &[0.3]),
        "h=0 ignores pre_y_start and stays stateless"
    );
}

#[test]
fn comparators_feedthrough_and_state_contract_match_r_reals_1() {
    assert_bool_perturb_moves(&Greater::default(), &[2.0, 1.0], &[(0, 0.0), (1, 3.0)]);
    assert_bool_perturb_moves(
        &Greater {
            h: 1.0,
            pre_y_start: false,
        },
        &[1.0, 2.0],
        &[(0, 3.0), (1, 0.0)],
    );
    assert_bool_perturb_moves(&Less::default(), &[1.0, 2.0], &[(0, 3.0), (1, 0.0)]);
    assert_bool_perturb_moves(
        &Less {
            h: 1.0,
            pre_y_start: false,
        },
        &[2.0, 1.0],
        &[(0, 0.0), (1, 3.0)],
    );
    assert_bool_perturb_moves(
        &GreaterThreshold {
            t: 2.0,
            h: 0.0,
            pre_y_start: false,
        },
        &[3.0],
        &[(0, 1.0)],
    );
    assert_bool_perturb_moves(
        &LessThreshold {
            t: 2.0,
            h: 0.0,
            pre_y_start: false,
        },
        &[1.0],
        &[(0, 3.0)],
    );
    assert_bool_perturb_moves(
        &Hysteresis {
            u_low: 1.0,
            u_high: 3.0,
            pre_y_start: false,
        },
        &[2.0],
        &[(0, 4.0)],
    );

    for block in [
        &Greater {
            h: 1.0,
            pre_y_start: false,
        } as &dyn Block,
        &Less {
            h: 1.0,
            pre_y_start: false,
        },
        &GreaterThreshold {
            t: 2.0,
            h: 1.0,
            pre_y_start: false,
        },
        &LessThreshold {
            t: 2.0,
            h: 1.0,
            pre_y_start: false,
        },
        &Hysteresis::default(),
    ] {
        assert_eq!(block.kind(), BlockKind::Stateful);
        assert_eq!(block.state_len(), 1);
        assert!(block.feeds_through(0, 0));
    }
}

#[test]
fn comparator_band_switching_points_match_buildings_asymmetric_rules() {
    let greater = Greater {
        h: 2.0,
        pre_y_start: false,
    };
    assert_eq!(
        drive_bool(
            &greater,
            &[
                vec![10.0, 10.0], // at threshold: false
                vec![12.0, 10.0], // u2+h is already above threshold: true
                vec![8.1, 10.0],  // inside band from prior true: hold
                vec![8.0, 10.0],  // lower edge u2-h: reset
            ],
        ),
        vec![false, true, true, false]
    );

    let less = Less {
        h: 2.0,
        pre_y_start: false,
    };
    assert_eq!(
        drive_bool(
            &less,
            &[
                vec![10.0, 10.0], // at threshold: false
                vec![8.0, 10.0],  // u2-h is below threshold: true
                vec![11.9, 10.0], // inside band from prior true: hold
                vec![12.0, 10.0], // upper edge u2+h: reset
            ],
        ),
        vec![false, true, true, false]
    );

    let gt = GreaterThreshold {
        t: 5.0,
        h: 1.0,
        pre_y_start: false,
    };
    assert_eq!(
        drive_bool(&gt, &[vec![5.0], vec![6.0], vec![4.1], vec![4.0]]),
        vec![false, true, true, false]
    );

    let lt = LessThreshold {
        t: 5.0,
        h: 1.0,
        pre_y_start: false,
    };
    assert_eq!(
        drive_bool(&lt, &[vec![5.0], vec![4.0], vec![5.9], vec![6.0]]),
        vec![false, true, true, false]
    );
}

#[test]
fn hysteresis_switching_points_match_buildings_reference() {
    let block = Hysteresis {
        u_low: 2.0,
        u_high: 5.0,
        pre_y_start: false,
    };
    assert_eq!(
        drive_bool(&block, &[vec![5.0], vec![5.1], vec![2.0], vec![1.9]]),
        vec![false, true, true, false]
    );
}

#[test]
fn comparators_do_not_chatter_inside_the_band() {
    let gt = GreaterThreshold {
        t: 10.0,
        h: 2.0,
        pre_y_start: false,
    };
    assert_eq!(
        drive_bool(
            &gt,
            &[
                vec![9.9],
                vec![10.1], // set
                vec![9.0],  // hold inside (8,10]
                vec![8.1],  // still hold
                vec![8.0],  // reset
                vec![9.9],  // remains false until crossing t again
            ],
        ),
        vec![false, true, true, true, false, false]
    );

    let hyst = Hysteresis {
        u_low: 2.0,
        u_high: 5.0,
        pre_y_start: false,
    };
    assert_eq!(
        drive_bool(
            &hyst,
            &[
                vec![4.0],
                vec![5.2], // set
                vec![3.0], // hold inside [uLow,uHigh]
                vec![2.0], // hold at uLow
                vec![1.9], // reset
                vec![4.0], // remains false until crossing uHigh
            ],
        ),
        vec![false, true, true, true, false, false]
    );
}

#[test]
fn comparator_init_state_seeds_initial_hold_state() {
    let seeded_greater = Greater {
        h: 2.0,
        pre_y_start: true,
    };
    let unseeded_greater = Greater {
        h: 2.0,
        pre_y_start: false,
    };
    assert_eq!(
        drive_bool(&seeded_greater, &[vec![9.0, 10.0]]),
        vec![true],
        "pre_y_start=true holds inside the Greater band at tick 0"
    );
    assert_eq!(
        drive_bool(&unseeded_greater, &[vec![9.0, 10.0]]),
        vec![false]
    );

    let seeded_hysteresis = Hysteresis {
        u_low: 2.0,
        u_high: 5.0,
        pre_y_start: true,
    };
    let unseeded_hysteresis = Hysteresis {
        u_low: 2.0,
        u_high: 5.0,
        pre_y_start: false,
    };
    assert_eq!(drive_bool(&seeded_hysteresis, &[vec![3.0]]), vec![true]);
    assert_eq!(drive_bool(&unseeded_hysteresis, &[vec![3.0]]), vec![false]);
}

#[test]
fn comparator_nan_and_infinity_inputs_are_panic_free() {
    assert!(!bool_out(&Greater::default(), &[f64::NAN, 0.0]));
    assert!(bool_out(&Greater::default(), &[f64::INFINITY, 0.0]));
    assert!(!bool_out(&Less::default(), &[f64::NAN, 0.0]));
    assert!(bool_out(&Less::default(), &[f64::NEG_INFINITY, 0.0]));

    let gt_seeded = Greater {
        h: 1.0,
        pre_y_start: true,
    };
    assert_eq!(drive_bool(&gt_seeded, &[vec![f64::NAN, 0.0]]), vec![false]);

    let hyst_seeded = Hysteresis {
        u_low: 0.0,
        u_high: 1.0,
        pre_y_start: true,
    };
    assert_eq!(
        drive_bool(
            &hyst_seeded,
            &[vec![f64::INFINITY], vec![f64::NEG_INFINITY], vec![f64::NAN]],
        ),
        vec![true, false, false]
    );
}

#[test]
fn comparator_outputs_are_bit_deterministic_across_reruns() {
    let h0_a = Value::Boolean(bool_out(&Greater::default(), &[0.1 + 0.2, 0.3]));
    let h0_b = Value::Boolean(bool_out(&Greater::default(), &[0.1 + 0.2, 0.3]));
    assert!(h0_a.bit_eq(&h0_b));

    let gt = GreaterThreshold {
        t: 10.0,
        h: 2.0,
        pre_y_start: false,
    };
    let seq = [vec![9.9], vec![10.1], vec![9.0], vec![8.0], vec![10.1]];
    let run1 = drive_bool_values(&gt, &seq);
    let run2 = drive_bool_values(&gt, &seq);
    for (idx, (a, b)) in run1.iter().zip(&run2).enumerate() {
        assert!(a.bit_eq(b), "stateful comparator output {idx} diverged");
    }
    let expected = [
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Boolean(true),
        Value::Boolean(false),
        Value::Boolean(true),
    ];
    for (idx, (got, want)) in run1.iter().zip(expected).enumerate() {
        assert!(got.bit_eq(&want), "trace[{idx}] got {got:?}, want {want:?}");
    }
}
