//! Formula, edge, registry, and determinism tests for `CDL.Psychrometrics` blocks.

use std::sync::Arc;

use oce_model::Value;

use super::{
    Block, BlockKind, Ctx, DewPointTDryBulPhi, NoopDiagnostics, ParamRule, PortKind,
    SpecificEnthalpyTDryBulPhi, WetBulbTDryBulPhi, lookup,
};
use crate::ParamTable;

const CANONICAL_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;
const KELVIN_OFFSET: f64 = 273.15;

fn real_out(block: &dyn Block, t_dry_bulb: f64, phi: f64) -> f64 {
    let diag = NoopDiagnostics;
    let cx = Ctx::new(0.0, &diag);
    let inputs = [Value::Real(t_dry_bulb), Value::Real(phi)];
    let mut out = None;
    block.step_algebraic(&cx, &inputs, &mut |idx, val| {
        assert_eq!(idx, 0, "psychrometric blocks have one output");
        out = Some(val);
    });
    match out.expect("block emits output") {
        Value::Real(y) => y,
        other => panic!("expected Real output, got {other:?}"),
    }
}

fn expected_dew_point(t_dry_bulb: f64, phi: f64) -> f64 {
    if !t_dry_bulb.is_finite() || !phi.is_finite() {
        return f64::NAN;
    }

    let p_w = phi * expected_saturation_pressure(t_dry_bulb);
    if !p_w.is_finite() || p_w <= 0.0 {
        return f64::NAN;
    }

    let alpha = libm::log(p_w / 1000.0);
    6.54 + 14.526 * alpha
        + 0.7389 * alpha * alpha
        + 0.09486 * alpha * alpha * alpha
        + 0.4569 * libm::pow(p_w / 1000.0, 0.1984)
        + KELVIN_OFFSET
}

fn expected_specific_enthalpy(t_dry_bulb: f64, phi: f64, p_atm: f64) -> f64 {
    if !t_dry_bulb.is_finite() || !phi.is_finite() || !p_atm.is_finite() || p_atm <= 0.0 {
        return f64::NAN;
    }

    let t_c = t_dry_bulb - KELVIN_OFFSET;
    let p_w = phi * expected_saturation_pressure(t_dry_bulb);
    let denominator = p_atm - p_w;
    if !p_w.is_finite() || !denominator.is_finite() || denominator <= 0.0 {
        return f64::NAN;
    }

    let w = 0.621_964_713_077_498_9 * p_w / denominator;
    1006.0 * t_c + w * (2_501_014.5 + 1860.0 * t_c)
}

fn expected_wet_bulb(t_dry_bulb: f64, phi: f64) -> f64 {
    if !t_dry_bulb.is_finite() || !phi.is_finite() {
        return f64::NAN;
    }

    let t_c = t_dry_bulb - KELVIN_OFFSET;
    let rh_per = 100.0 * phi;
    if !rh_per.is_finite() {
        return f64::NAN;
    }

    KELVIN_OFFSET
        + t_c * libm::atan(0.151_977 * libm::sqrt(rh_per + 8.313_659))
        + libm::atan(t_c + rh_per)
        - libm::atan(rh_per - 1.676_331)
        + 0.003_918_38 * libm::pow(rh_per, 1.5) * libm::atan(0.023_101 * rh_per)
        - 4.686_035
}

fn expected_saturation_pressure(t_sat: f64) -> f64 {
    expected_regularized_step(
        t_sat - 273.16,
        expected_saturation_pressure_liquid(t_sat),
        expected_sublimation_pressure_ice(t_sat),
        1.0,
    )
}

fn expected_saturation_pressure_liquid(t_sat: f64) -> f64 {
    611.657 * libm::exp(17.2799 - 4102.99 / (t_sat - 35.719))
}

fn expected_sublimation_pressure_ice(t_sat: f64) -> f64 {
    let r1 = t_sat / 273.16;
    let exponent = -13.928_169_0 - (-13.928_169_0) * libm::pow(r1, -1.5) + 34.707_823_8
        - 34.707_823_8 * libm::pow(r1, -1.25);
    libm::exp(exponent) * 611.657
}

fn expected_regularized_step(x: f64, y1: f64, y2: f64, x_small: f64) -> f64 {
    if x > x_small {
        y1
    } else if x < -x_small {
        y2
    } else if x_small > 0.0 {
        let ratio = x / x_small;
        ratio * (ratio * ratio - 3.0) * (y2 - y1) / 4.0 + (y1 + y2) / 2.0
    } else {
        (y1 + y2) / 2.0
    }
}

fn assert_bits(actual: f64, expected: f64) {
    assert_eq!(actual.to_bits(), expected.to_bits(), "actual={actual:?}");
}

#[test]
fn registry_exposes_psychrometric_signatures() {
    let cases: &[(&str, &str)] = &[
        (
            "CDL.Psychrometrics.DewPoint_TDryBulPhi",
            "DewPoint_TDryBulPhi",
        ),
        (
            "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
            "SpecificEnthalpy_TDryBulPhi",
        ),
        (
            "CDL.Psychrometrics.WetBulb_TDryBulPhi",
            "WetBulb_TDryBulPhi",
        ),
    ];

    for &(class_path, suffix) in cases {
        let entry = lookup(class_path).unwrap_or_else(|| panic!("{suffix} registered"));
        let block = (entry.make)(&ParamTable::default());
        let signature = block.signature();
        assert_eq!(signature.class_path, class_path);
        assert_eq!(signature.inputs, &[PortKind::Real, PortKind::Real]);
        assert_eq!(signature.outputs, &[PortKind::Real]);
        assert!(!signature.stateful);
        assert_eq!(block.kind(), BlockKind::Algebraic);
        assert_eq!(block.state_len(), 0);
        assert!(block.feeds_through(0, 0));
        assert!(block.feeds_through(1, 0));
    }
}

#[test]
fn specific_enthalpy_reads_p_atm_and_exposes_real_param_rule() {
    let params = ParamTable {
        values: vec![(Arc::from("pAtm"), Value::Real(90_000.0))],
    };
    let entry = lookup("CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi").unwrap();
    assert_eq!(
        entry.param_rules(),
        &[ParamRule::RealFiniteGreaterThan {
            name: "pAtm",
            min: 0.0,
        }]
    );
    let block = (entry.make)(&params);
    assert_bits(
        real_out(block.as_ref(), 293.15, 0.5),
        expected_specific_enthalpy(293.15, 0.5, 90_000.0),
    );
}

#[test]
fn formulas_match_source_derived_anchors() {
    let samples = [
        (253.15, 0.05),
        (273.15, 0.0),
        (273.16, 0.5),
        (293.15, 0.5),
        (323.15, 0.99),
        (373.16, 1.0),
    ];

    let dew = DewPointTDryBulPhi;
    let enthalpy = SpecificEnthalpyTDryBulPhi::default();
    let wet = WetBulbTDryBulPhi;

    for (t, phi) in samples {
        assert_bits(real_out(&dew, t, phi), expected_dew_point(t, phi));
        assert_bits(
            real_out(&enthalpy, t, phi),
            expected_specific_enthalpy(t, phi, 101_325.0),
        );
        assert_bits(real_out(&wet, t, phi), expected_wet_bulb(t, phi));
    }
}

#[test]
fn hand_pinned_numeric_anchors_cover_units_pressure_and_range_edges() {
    let custom_pressure = SpecificEnthalpyTDryBulPhi { p_atm: 90_000.0 };

    assert_bits(
        real_out(&DewPointTDryBulPhi, 293.15, 0.5),
        282.4598738198692,
    );
    assert_bits(
        real_out(&DewPointTDryBulPhi, 293.15, 1.2),
        296.1397588298837,
    );
    assert_bits(
        real_out(&SpecificEnthalpyTDryBulPhi::default(), 293.15, -0.05),
        18_299.286_177_526_268,
    );
    assert_bits(
        real_out(&custom_pressure, 293.15, 0.5),
        40_912.156_584_822_15,
    );
    assert_bits(
        real_out(&custom_pressure, 373.16, 0.878_645_541_534_310_3),
        1.033_620_503_115_673_8e22,
    );
    assert_bits(
        real_out(&WetBulbTDryBulPhi, 252.15, 0.5),
        251.586_760_550_067_3,
    );
    assert_bits(
        real_out(&WetBulbTDryBulPhi, 324.15, 0.5),
        313.500_416_505_372_1,
    );
    assert_bits(
        real_out(&WetBulbTDryBulPhi, 293.15, 1.2),
        295.664_048_880_514_14,
    );
}

#[test]
fn zero_humidity_and_invalid_pressure_outputs_are_pinned() {
    let enthalpy = SpecificEnthalpyTDryBulPhi::default();
    assert_bits(real_out(&enthalpy, 273.15, 0.0), 0.0);

    let zero_pressure = SpecificEnthalpyTDryBulPhi { p_atm: 0.0 };
    assert_bits(
        real_out(&zero_pressure, 273.15, 0.0),
        f64::from_bits(CANONICAL_NAN_BITS),
    );

    let dew = DewPointTDryBulPhi;
    assert_bits(
        real_out(&dew, 293.15, 0.0),
        f64::from_bits(CANONICAL_NAN_BITS),
    );
}

#[test]
fn non_finite_runtime_inputs_and_direct_parameters_fail_closed() {
    let dew = DewPointTDryBulPhi;
    let enthalpy = SpecificEnthalpyTDryBulPhi::default();
    let wet = WetBulbTDryBulPhi;
    let invalid_p_atm_cases = [
        SpecificEnthalpyTDryBulPhi { p_atm: f64::NAN },
        SpecificEnthalpyTDryBulPhi {
            p_atm: f64::INFINITY,
        },
        SpecificEnthalpyTDryBulPhi {
            p_atm: f64::NEG_INFINITY,
        },
    ];

    for (t, phi) in [
        (f64::NAN, 0.5),
        (f64::INFINITY, 0.5),
        (f64::NEG_INFINITY, 0.5),
        (293.15, f64::NAN),
        (293.15, f64::INFINITY),
        (293.15, f64::NEG_INFINITY),
    ] {
        assert_bits(real_out(&dew, t, phi), f64::from_bits(CANONICAL_NAN_BITS));
        assert_bits(
            real_out(&enthalpy, t, phi),
            f64::from_bits(CANONICAL_NAN_BITS),
        );
        assert_bits(real_out(&wet, t, phi), f64::from_bits(CANONICAL_NAN_BITS));
    }

    for block in invalid_p_atm_cases {
        assert_bits(
            real_out(&block, 293.15, 0.5),
            f64::from_bits(CANONICAL_NAN_BITS),
        );
    }
}

#[test]
fn specific_enthalpy_pressure_denominator_boundary_is_pinned() {
    let t = 373.16;
    let p_atm = 90_000.0;
    let saturation = expected_saturation_pressure(t);
    let phi_equal = p_atm / saturation;
    let phi_below = f64::from_bits(phi_equal.to_bits() - 1);
    let phi_above = f64::from_bits(phi_equal.to_bits() + 1);
    let block = SpecificEnthalpyTDryBulPhi { p_atm };

    assert_bits(
        real_out(&block, t, phi_below),
        expected_specific_enthalpy(t, phi_below, p_atm),
    );
    assert_bits(
        real_out(&block, t, phi_equal),
        f64::from_bits(CANONICAL_NAN_BITS),
    );
    assert_bits(
        real_out(&block, t, phi_above),
        f64::from_bits(CANONICAL_NAN_BITS),
    );
}

#[test]
fn repeated_runs_are_bit_identical() {
    let cases: &[(&dyn Block, f64, f64)] = &[
        (&DewPointTDryBulPhi, 293.15, 0.5),
        (&SpecificEnthalpyTDryBulPhi::default(), 293.15, 0.5),
        (&WetBulbTDryBulPhi, 293.15, 0.5),
    ];

    for &(block, t, phi) in cases {
        let first = real_out(block, t, phi).to_bits();
        for _ in 0..100 {
            assert_eq!(real_out(block, t, phi).to_bits(), first);
        }
    }
}
