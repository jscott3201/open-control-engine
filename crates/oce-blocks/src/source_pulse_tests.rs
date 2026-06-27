use oce_model::determinism::CANONICAL_NAN_BITS;
use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, Ctx, IntegerPulse, LogicalPulse, NoopDiagnostics, ParamRule, PortKind,
    RealPulse, Time, lookup,
};

fn out_at(block: &dyn Block, t: Time) -> Value {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(t, &diag);
    let mut out = None;
    block.step_algebraic(&cx, &[], &mut |idx, val| {
        assert_eq!(idx, 0);
        out = Some(val);
    });
    out.expect("single-output source emits one value")
}

fn bool_trace(block: &dyn Block, times: &[Time]) -> Vec<bool> {
    times
        .iter()
        .map(|t| match out_at(block, *t) {
            Value::Boolean(y) => y,
            other => panic!("expected Boolean output, got {other:?}"),
        })
        .collect()
}

fn assert_real_trace(block: &dyn Block, cases: &[(Time, f64)]) {
    for &(t, want) in cases {
        let got = out_at(block, t);
        assert!(
            got.bit_eq(&Value::Real(want)),
            "t={t}: got {got:?}, want {want:?}"
        );
    }
}

fn assert_integer_trace(block: &dyn Block, cases: &[(Time, i64)]) {
    for &(t, want) in cases {
        match out_at(block, t) {
            Value::Integer(got) => assert_eq!(got, want, "t={t}"),
            other => panic!("t={t}: expected Integer output, got {other:?}"),
        }
    }
}

#[test]
fn source_signatures_are_zero_input_algebraic_sources() {
    let logical = LogicalPulse::default();
    assert_eq!(logical.kind(), BlockKind::Algebraic);
    assert_eq!(logical.state_len(), 0);
    assert!(!logical.feeds_through(0, 0));
    assert_eq!(logical.signature().class_path, "CDL.Logical.Sources.Pulse");
    assert!(logical.signature().inputs.is_empty());
    assert_eq!(logical.signature().outputs, &[PortKind::Boolean]);

    let real = RealPulse::default();
    assert_eq!(real.kind(), BlockKind::Algebraic);
    assert_eq!(real.state_len(), 0);
    assert!(!real.feeds_through(0, 0));
    assert_eq!(real.signature().class_path, "CDL.Reals.Sources.Pulse");
    assert!(real.signature().inputs.is_empty());
    assert_eq!(real.signature().outputs, &[PortKind::Real]);

    let integer = IntegerPulse::default();
    assert_eq!(integer.kind(), BlockKind::Algebraic);
    assert_eq!(integer.state_len(), 0);
    assert!(!integer.feeds_through(0, 0));
    assert_eq!(integer.signature().class_path, "CDL.Integers.Sources.Pulse");
    assert!(integer.signature().inputs.is_empty());
    assert_eq!(integer.signature().outputs, &[PortKind::Integer]);
}

#[test]
fn source_boundaries_are_rising_inclusive_and_falling_exclusive() {
    let block = LogicalPulse {
        width: 0.2,
        period: 2.0,
        shift: 0.6,
    };
    let times = [-1.4, -1.0, 0.0, 0.6, 1.0, 2.6, 3.0, 9.0, 10.6];
    assert_eq!(
        bool_trace(&block, &times),
        vec![true, true, false, true, false, true, false, false, true]
    );
}

#[test]
fn source_shift_folds_positive_and_negative_period_offsets() {
    let times = [0.0, 0.1, 0.49, 0.5, 2.1, 2.5, 4.1, 4.5];
    let expected = vec![false, true, true, false, true, false, true, false];
    for shift in [0.1, 2.1, 4.1, -1.9, -3.9] {
        let block = LogicalPulse {
            width: 0.2,
            period: 2.0,
            shift,
        };
        assert_eq!(bool_trace(&block, &times), expected, "shift={shift}");
    }
}

#[test]
fn source_width_one_stays_true_at_all_boundaries() {
    let block = LogicalPulse {
        width: 1.0,
        period: 2.0,
        shift: 0.6,
    };
    let times = [-10.0, -1.4, -1.0, 0.0, 0.6, 1.0, 2.6, 9.0];
    assert_eq!(bool_trace(&block, &times), vec![true; times.len()]);
}

#[test]
fn source_direct_invalid_timing_parameters_degrade_deterministically() {
    let invalid_period = LogicalPulse {
        width: 0.5,
        period: 0.0,
        shift: 0.25,
    };
    assert_eq!(
        bool_trace(&invalid_period, &[0.25, 0.75, 1.25, 1.75]),
        vec![true, false, true, false]
    );

    let invalid_width = LogicalPulse {
        width: f64::NAN,
        period: 1.0,
        shift: 0.0,
    };
    assert_eq!(
        bool_trace(&invalid_width, &[0.0, 0.5, 1.0]),
        vec![true, false, true]
    );

    let clamped_width = LogicalPulse {
        width: 2.0,
        period: 1.0,
        shift: 0.0,
    };
    assert_eq!(
        bool_trace(&clamped_width, &[0.0, 0.25, 0.5, 0.75, 1.0]),
        vec![true; 5]
    );
}

#[test]
fn real_source_maps_logical_false_to_offset_and_true_to_offset_plus_amplitude() {
    let block = RealPulse {
        amplitude: 2.0,
        width: 0.5,
        period: 1.0,
        shift: 0.0,
        offset: 0.2,
    };
    assert_real_trace(&block, &[(0.0, 2.2), (0.5, 0.2), (1.0, 2.2), (1.5, 0.2)]);

    let negative = RealPulse {
        amplitude: -3.0,
        width: 0.5,
        period: 1.0,
        shift: 0.0,
        offset: 1.0,
    };
    assert_real_trace(&negative, &[(0.0, -2.0), (0.5, 1.0)]);
}

#[test]
fn real_source_canonicalizes_nan_outputs() {
    let block = RealPulse {
        amplitude: f64::from_bits(0x7ff0_0000_0000_0001),
        width: 0.5,
        period: 1.0,
        shift: 0.0,
        offset: 0.0,
    };
    assert!(out_at(&block, 0.0).bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))));
}

#[test]
fn integer_source_maps_logical_pulse_exactly_and_wraps_high_sum() {
    let block = IntegerPulse {
        amplitude: 3,
        width: 0.5,
        period: 1.0,
        shift: -1.25,
        offset: -2,
    };
    assert_integer_trace(&block, &[(0.0, 1), (0.25, -2), (0.75, 1), (1.25, -2)]);

    let wrapping = IntegerPulse {
        amplitude: 1,
        width: 0.5,
        period: 1.0,
        shift: 0.0,
        offset: i64::MAX,
    };
    assert_integer_trace(&wrapping, &[(0.0, i64::MIN), (0.5, i64::MAX)]);
}

#[test]
fn source_pulse_registry_rules_and_constructors_are_pinned() {
    let expected_rules = &[
        ParamRule::Required { name: "period" },
        ParamRule::RealGreaterOrEqual {
            name: "period",
            min: 1e-37,
        },
        ParamRule::RealGreaterOrEqual {
            name: "width",
            min: 1e-37,
        },
        ParamRule::RealLessOrEqualConstant {
            name: "width",
            max: 1.0,
        },
    ];
    for path in [
        "CDL.Logical.Sources.Pulse",
        "CDL.Reals.Sources.Pulse",
        "CDL.Integers.Sources.Pulse",
    ] {
        assert_eq!(
            lookup(path).unwrap().param_rules(),
            expected_rules,
            "{path}"
        );
    }

    let logical = (lookup("CDL.Logical.Sources.Pulse").unwrap().make)(&ParamTable {
        values: vec![
            ("width".into(), Value::Real(0.2)),
            ("period".into(), Value::Real(2.0)),
            ("shift".into(), Value::Real(0.6)),
        ],
    });
    assert!(out_at(logical.as_ref(), 0.6).bit_eq(&Value::Boolean(true)));

    let real = (lookup("CDL.Reals.Sources.Pulse").unwrap().make)(&ParamTable {
        values: vec![
            ("amplitude".into(), Value::Real(2.0)),
            ("width".into(), Value::Real(0.5)),
            ("period".into(), Value::Real(1.0)),
            ("offset".into(), Value::Real(0.2)),
        ],
    });
    assert!(out_at(real.as_ref(), 0.0).bit_eq(&Value::Real(2.2)));

    let integer = (lookup("CDL.Integers.Sources.Pulse").unwrap().make)(&ParamTable {
        values: vec![
            ("amplitude".into(), Value::Integer(3)),
            ("width".into(), Value::Real(0.5)),
            ("period".into(), Value::Real(1.0)),
            ("shift".into(), Value::Real(-1.25)),
            ("offset".into(), Value::Integer(-2)),
        ],
    });
    assert!(out_at(integer.as_ref(), 0.0).bit_eq(&Value::Integer(1)));
}
