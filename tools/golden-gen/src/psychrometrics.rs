//! `CDL.Psychrometrics` formula goldens.
//!
//! These Tier-A references are re-derived from the pinned Buildings CDL psychrometric block
//! equations plus `Buildings.Utilities.Psychrometrics.Functions.saturationPressure`. This module is
//! independent from `oce-blocks`; it duplicates the source formulas explicitly for oracle evidence.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

const KELVIN_OFFSET: f64 = 273.15;
const DEFAULT_TICKS: [f64; 13] = [
    0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0, 420.0, 480.0, 540.0, 600.0,
    660.0, 720.0,
];
const PRESSURE_TICKS: [f64; 7] = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0, 360.0];

/// Build `CDL.Psychrometrics` goldens.
pub fn goldens() -> Vec<Golden> {
    let t = vec![
        253.15,
        273.15,
        273.16,
        293.15,
        323.15,
        373.16,
        293.15,
        293.15,
        f64::INFINITY,
        f64::NEG_INFINITY,
        293.15,
        293.15,
        f64::NAN,
    ];
    let phi = vec![
        0.05,
        0.0,
        0.5,
        0.5,
        0.99,
        1.0,
        1.2,
        -0.05,
        0.5,
        0.5,
        f64::INFINITY,
        f64::NEG_INFINITY,
        0.5,
    ];

    let pressure_t = 373.16;
    let pressure_saturation = saturation_pressure(pressure_t);
    let pressure_phi_equal = 90_000.0 / pressure_saturation;
    let pressure_phi_below = f64::from_bits(pressure_phi_equal.to_bits() - 1);
    let pressure_phi_above = f64::from_bits(pressure_phi_equal.to_bits() + 1);

    vec![
        psychrometric(
            "CDL.Psychrometrics.DewPoint_TDryBulPhi",
            "TDewPoi",
            DEFAULT_TICKS.to_vec(),
            t.clone(),
            phi.clone(),
            dew_point,
            "TDryBul=[253.15,273.15,273.16,293.15,323.15,373.16,293.15,293.15,+Inf,-Inf,293.15,293.15,NaN] K; phi=[0.05,0.0,0.5,0.5,0.99,1.0,1.2,-0.05,0.5,0.5,+Inf,-Inf,0.5]; ice branch, zero humidity, nonzero-humidity regStep transition, liquid branch, high-temperature edge, RH outside [0,1], non-finite input guard",
            "if TDryBul or phi is non-finite, or p_w is non-finite/non-positive, TDewPoi=NaN; otherwise p_w=phi*saturationPressure(TDryBul); alpha=log(p_w/1000); TDewPoi=(C14+C15*alpha+C16*alpha^2+C17*alpha^3+C18*(p_w/1000)^0.1984)+273.15",
        ),
        psychrometric(
            "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
            "h",
            DEFAULT_TICKS.to_vec(),
            t,
            phi,
            |tdb, hum| specific_enthalpy(tdb, hum, 101_325.0),
            "TDryBul=[253.15,273.15,273.16,293.15,323.15,373.16,293.15,293.15,+Inf,-Inf,293.15,293.15,NaN] K; phi=[0.05,0.0,0.5,0.5,0.99,1.0,1.2,-0.05,0.5,0.5,+Inf,-Inf,0.5]; default pAtm=101325 Pa, dry-air zero anchor, nonzero-humidity regStep transition, RH outside [0,1], non-finite input guard",
            "if TDryBul/phi/pAtm is non-finite, pAtm<=0, p_w non-finite, or pAtm-p_w<=0, h=NaN; otherwise TDryBul_degC=TDryBul-273.15; p_w=phi*saturationPressure(TDryBul); w=0.6219647130774989*p_w/(pAtm-p_w); h=1006*TDryBul_degC+w*(2501014.5+1860*TDryBul_degC)",
        ),
        psychrometric(
            "CDL.Psychrometrics.WetBulb_TDryBulPhi",
            "TWetBul",
            DEFAULT_TICKS.to_vec(),
            vec![
                252.15,
                253.15,
                273.15,
                293.15,
                323.15,
                324.15,
                293.15,
                293.15,
                f64::INFINITY,
                f64::NEG_INFINITY,
                293.15,
                293.15,
                f64::NAN,
            ],
            vec![
                0.5,
                0.05,
                0.0,
                0.5,
                0.99,
                0.5,
                1.2,
                -0.05,
                0.5,
                0.5,
                f64::INFINITY,
                f64::NEG_INFINITY,
                0.5,
            ],
            wet_bulb,
            "TDryBul=[252.15,253.15,273.15,293.15,323.15,324.15,293.15,293.15,+Inf,-Inf,293.15,293.15,NaN] K; phi=[0.5,0.05,0.0,0.5,0.99,0.5,1.2,-0.05,0.5,0.5,+Inf,-Inf,0.5]; below/at/inside/at/above documented Stull dry-bulb range, RH outside [0,1], non-finite input guard",
            "if TDryBul or phi is non-finite, TWetBul=NaN; otherwise TWetBul=273.15+TDryBul_degC*atan(0.151977*sqrt(100*phi+8.313659))+atan(TDryBul_degC+100*phi)-atan(100*phi-1.676331)+0.00391838*(100*phi)^1.5*atan(0.023101*100*phi)-4.686035",
        ),
        psychrometric(
            "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
            "h",
            PRESSURE_TICKS.to_vec(),
            vec![293.15, 293.15, pressure_t, pressure_t, pressure_t, 293.15, 293.15],
            vec![
                0.5,
                1.2,
                pressure_phi_below,
                pressure_phi_equal,
                pressure_phi_above,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ],
            |tdb, hum| specific_enthalpy(tdb, hum, 90_000.0),
            "scenario=custom_p_atm_pressure_boundaries; pAtm=90000 Pa; includes custom pressure, RH>1, pAtm-p_w just above zero, exactly zero, just below zero, and phi=+/-Inf runtime guards",
            "custom pAtm uses the same finite-positive and denominator guard: denominator > 0 follows source enthalpy formula; denominator <= 0 and non-finite inputs produce NaN",
        )
        .with_scenario("custom_p_atm_pressure_boundaries")
        .with_provenance("pAtm", "90000 Pa"),
    ]
}

fn psychrometric(
    class_path: &'static str,
    signal: &'static str,
    time: Vec<f64>,
    t_dry_bulb: Vec<f64>,
    phi: Vec<f64>,
    f: fn(f64, f64) -> f64,
    input_desc: &'static str,
    rule_desc: &'static str,
) -> Golden {
    assert_eq!(time.len(), t_dry_bulb.len(), "{class_path} input length");
    assert_eq!(time.len(), phi.len(), "{class_path} input length");
    let y = t_dry_bulb
        .iter()
        .zip(&phi)
        .map(|(&tdb, &hum)| Sample::Real(f(tdb, hum)))
        .collect();
    Golden::new(
        class_path,
        signal,
        ValueKind::Real,
        time,
        y,
        input_desc,
        rule_desc,
    )
    .with_inputs(vec![
        input_r("TDryBul", t_dry_bulb),
        input_r("phi", phi),
    ])
    .with_provenance(
        "math_library",
        "libm 0.2.16, default-features=false, pure Rust deterministic math",
    )
}

fn dew_point(t_dry_bulb: f64, phi: f64) -> f64 {
    if !t_dry_bulb.is_finite() || !phi.is_finite() {
        return f64::NAN;
    }

    let p_w = phi * saturation_pressure(t_dry_bulb);
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

fn specific_enthalpy(t_dry_bulb: f64, phi: f64, p_atm: f64) -> f64 {
    if !t_dry_bulb.is_finite() || !phi.is_finite() || !p_atm.is_finite() || p_atm <= 0.0 {
        return f64::NAN;
    }

    let t_c = t_dry_bulb - KELVIN_OFFSET;
    let p_w = phi * saturation_pressure(t_dry_bulb);
    let denominator = p_atm - p_w;
    if !p_w.is_finite() || !denominator.is_finite() || denominator <= 0.0 {
        return f64::NAN;
    }

    let w = 0.621_964_713_077_498_9 * p_w / denominator;
    1006.0 * t_c + w * (2_501_014.5 + 1860.0 * t_c)
}

fn wet_bulb(t_dry_bulb: f64, phi: f64) -> f64 {
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

fn saturation_pressure(t_sat: f64) -> f64 {
    regularized_step(
        t_sat - 273.16,
        saturation_pressure_liquid(t_sat),
        sublimation_pressure_ice(t_sat),
        1.0,
    )
}

fn saturation_pressure_liquid(t_sat: f64) -> f64 {
    611.657 * libm::exp(17.2799 - 4102.99 / (t_sat - 35.719))
}

fn sublimation_pressure_ice(t_sat: f64) -> f64 {
    let r1 = t_sat / 273.16;
    let exponent = -13.928_169_0
        - (-13.928_169_0) * libm::pow(r1, -1.5)
        + 34.707_823_8
        - 34.707_823_8 * libm::pow(r1, -1.25);
    libm::exp(exponent) * 611.657
}

fn regularized_step(x: f64, y1: f64, y2: f64, x_small: f64) -> f64 {
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

fn input_r(name: &'static str, values: impl IntoIterator<Item = f64>) -> InputSeries {
    InputSeries::new(
        name,
        ValueKind::Real,
        values.into_iter().map(Sample::Real).collect(),
    )
}
