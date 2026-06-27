use oce_model::determinism::CANONICAL_NAN_BITS;
use oce_model::{ParamTable, Value};

use super::{
    Block, BlockKind, CivilTime, Ctx, NoopDiagnostics, ParamRule, SourceRamp, SourceSin, Time,
    lookup,
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

fn assert_source_trace(block: &dyn Block, cases: &[(Time, f64)]) {
    for &(t, want) in cases {
        let got = out_at(block, t);
        assert!(
            got.bit_eq(&Value::Real(want)),
            "t={t}: got {got:?}, want {want:?}"
        );
    }
}

#[test]
fn civil_time_emits_scheduler_time_without_state() {
    let block = CivilTime;
    assert_eq!(block.kind(), BlockKind::Algebraic);
    assert_eq!(block.state_len(), 0);
    assert!(block.signature().inputs.is_empty());
    assert_eq!(block.signature().outputs, &[crate::PortKind::Real]);
    assert_source_trace(&block, &[(-1.0, -1.0), (0.0, 0.0), (1.25, 1.25)]);
}

#[test]
fn source_ramp_matches_piecewise_boundaries() {
    let block = SourceRamp {
        height: 2.0,
        duration: 3.0,
        offset: 0.5,
        start_time: 1.0,
    };
    assert_source_trace(
        &block,
        &[(0.0, 0.5), (1.0, 0.5), (2.5, 1.5), (4.0, 2.5), (5.0, 2.5)],
    );
}

#[test]
fn source_ramp_allows_negative_height_and_start_time() {
    let block = SourceRamp {
        height: -2.0,
        duration: 2.0,
        offset: 10.0,
        start_time: -1.0,
    };
    assert_source_trace(
        &block,
        &[(-2.0, 10.0), (-1.0, 10.0), (0.0, 9.0), (1.0, 8.0)],
    );
}

#[test]
fn source_ramp_direct_invalid_duration_degrades_to_one_second() {
    let block = SourceRamp {
        height: 2.0,
        duration: 0.0,
        offset: 0.0,
        start_time: 0.0,
    };
    assert_source_trace(&block, &[(0.0, 0.0), (0.5, 1.0), (1.0, 2.0), (2.0, 2.0)]);
}

#[test]
fn source_ramp_nan_output_is_canonicalized() {
    let block = SourceRamp {
        height: f64::from_bits(0x7ff0_0000_0000_0001),
        duration: 1.0,
        offset: 0.0,
        start_time: 0.0,
    };
    assert!(out_at(&block, 2.0).bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))));
}

#[test]
fn source_sin_matches_source_equation_boundaries() {
    let block = SourceSin {
        amplitude: 2.0,
        freq_hz: 0.25,
        phase: std::f64::consts::FRAC_PI_2,
        offset: 0.5,
        start_time: 1.0,
    };
    assert_source_trace(
        &block,
        &[
            (0.0, 0.5),
            (1.0, 0.5 + 2.0 * libm::sin(std::f64::consts::FRAC_PI_2)),
            (2.0, 0.5 + 2.0 * libm::sin(std::f64::consts::PI)),
            (3.0, -1.5),
            (4.0, 0.5 + 2.0 * libm::sin(2.0 * std::f64::consts::PI)),
            (
                5.0,
                0.5 + 2.0 * libm::sin(2.0 * std::f64::consts::PI + std::f64::consts::FRAC_PI_2),
            ),
        ],
    );
}

#[test]
fn source_sin_allows_negative_frequency_phase_and_start_time() {
    let block = SourceSin {
        amplitude: 3.0,
        freq_hz: -0.5,
        phase: std::f64::consts::FRAC_PI_2,
        offset: -1.0,
        start_time: -2.0,
    };
    assert_source_trace(
        &block,
        &[
            (-3.0, -1.0),
            (-2.0, 2.0),
            (-1.5, -1.0),
            (-1.0, -4.0),
            (0.0, 2.0),
        ],
    );
}

#[test]
fn source_sin_nan_output_is_canonicalized() {
    let block = SourceSin {
        amplitude: f64::from_bits(0x7ff0_0000_0000_0001),
        ..SourceSin::default()
    };
    assert!(out_at(&block, 0.0).bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))));
}

#[test]
fn source_sin_non_finite_frequency_follows_libm_domain() {
    let block = SourceSin {
        freq_hz: f64::INFINITY,
        ..SourceSin::default()
    };
    assert!(out_at(&block, 1.0).bit_eq(&Value::Real(f64::from_bits(CANONICAL_NAN_BITS))));
}

#[test]
fn source_blocks_are_pure_for_repeated_step() {
    let blocks: [&dyn Block; 3] = [
        &CivilTime,
        &SourceRamp {
            height: 2.0,
            duration: 3.0,
            offset: 0.5,
            start_time: 1.0,
        },
        &SourceSin {
            amplitude: 2.0,
            freq_hz: 0.25,
            phase: std::f64::consts::FRAC_PI_2,
            offset: 0.5,
            start_time: 1.0,
        },
    ];
    for block in blocks {
        let a = out_at(block, 2.5);
        let b = out_at(block, 2.5);
        assert!(a.bit_eq(&b), "{} diverged", block.signature().class_path);
        assert_eq!(block.state_len(), 0);
        assert!(!block.feeds_through(0, 0));
    }
}

#[test]
fn source_ramp_registry_rules_are_pinned() {
    assert!(
        lookup("CDL.Reals.Sources.CivilTime")
            .unwrap()
            .param_rules()
            .is_empty()
    );
    assert_eq!(
        lookup("CDL.Reals.Sources.Ramp").unwrap().param_rules(),
        &[
            ParamRule::Required { name: "duration" },
            ParamRule::RealGreaterOrEqual {
                name: "duration",
                min: 1e-37,
            },
        ]
    );
    assert_eq!(
        lookup("CDL.Reals.Sources.Sin").unwrap().param_rules(),
        &[
            ParamRule::Required { name: "freqHz" },
            ParamRule::Real { name: "amplitude" },
            ParamRule::Real { name: "freqHz" },
            ParamRule::Real { name: "phase" },
            ParamRule::Real { name: "offset" },
            ParamRule::Real { name: "startTime" },
        ]
    );
}

#[test]
fn source_registry_constructors_resolve_parameters() {
    let civil_time = (lookup("CDL.Reals.Sources.CivilTime").unwrap().make)(&ParamTable::default());
    assert!(out_at(civil_time.as_ref(), -1.0).bit_eq(&Value::Real(-1.0)));

    let source_ramp = (lookup("CDL.Reals.Sources.Ramp").unwrap().make)(&ParamTable {
        values: vec![
            ("height".into(), Value::Real(2.0)),
            ("duration".into(), Value::Real(3.0)),
            ("offset".into(), Value::Real(0.5)),
            ("startTime".into(), Value::Real(1.0)),
        ],
    });
    assert!(out_at(source_ramp.as_ref(), 2.5).bit_eq(&Value::Real(1.5)));

    let source_sin = (lookup("CDL.Reals.Sources.Sin").unwrap().make)(&ParamTable {
        values: vec![
            ("amplitude".into(), Value::Real(2.0)),
            ("freqHz".into(), Value::Real(0.25)),
            ("phase".into(), Value::Real(std::f64::consts::FRAC_PI_2)),
            ("offset".into(), Value::Real(0.5)),
            ("startTime".into(), Value::Real(1.0)),
        ],
    });
    assert!(out_at(source_sin.as_ref(), 1.0).bit_eq(&Value::Real(
        0.5 + 2.0 * libm::sin(std::f64::consts::FRAC_PI_2)
    )));
}
