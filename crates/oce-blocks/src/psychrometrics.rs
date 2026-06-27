//! Algebraic `CDL.Psychrometrics` formula blocks.
//!
//! The Buildings CDL psychrometric blocks are pure algebraic wrappers around ASHRAE/Stull
//! correlations and `Buildings.Utilities.Psychrometrics.Functions.saturationPressure`. Open Control
//! Engine mirrors those formulas with the pinned pure-Rust `libm` math strategy used by the Reals
//! transcendental blocks. The blocks keep the tick path wall-clock-free, allocation-free, and
//! panic-free; non-finite runtime values and singular pressure configurations fail closed to
//! canonical NaN outputs instead of emitting plausible finite values.

use oce_model::Value;

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, emit_real, read_real};

const KELVIN_OFFSET: f64 = 273.15;
const HUMIDITY_RATIO_COEFFICIENT: f64 = 0.621_964_713_077_498_9;

/// `CDL.Psychrometrics.DewPoint_TDryBulPhi` computes dew-point temperature from dry-bulb
/// temperature and relative humidity.
///
/// Inputs are `TDryBul` in kelvin and `phi` as a 0..1 relative humidity fraction. The output
/// `TDewPoi` is kelvin. The upstream correlation is documented as valid for dew points from 0 degC
/// to 93 degC and contains no runtime assertion. Open Control Engine still rejects non-finite
/// runtime inputs and non-positive/overflowed vapor pressure with canonical NaN so invalid sensor
/// values remain fail-visible.
#[derive(Clone, Copy, Debug, Default)]
pub struct DewPointTDryBulPhi;

impl Block for DewPointTDryBulPhi {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature =
            psychrometric_signature("CDL.Psychrometrics.DewPoint_TDryBulPhi");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let t_dry_bulb = read_real(inputs, 0);
        let phi = read_real(inputs, 1);
        if !t_dry_bulb.is_finite() || !phi.is_finite() {
            emit_real(0, f64::NAN, emit);
            return;
        }

        let p_w = phi * saturation_pressure(t_dry_bulb);
        if !p_w.is_finite() || p_w <= 0.0 {
            emit_real(0, f64::NAN, emit);
            return;
        }

        let alpha = libm::log(p_w / 1000.0);
        let dew_c = 6.54
            + 14.526 * alpha
            + 0.7389 * alpha * alpha
            + 0.09486 * alpha * alpha * alpha
            + 0.4569 * libm::pow(p_w / 1000.0, 0.1984);
        emit_real(0, dew_c + KELVIN_OFFSET, emit);
    }
}

/// `CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi` computes moist-air specific enthalpy.
///
/// Inputs are `TDryBul` in kelvin and `phi` as a 0..1 relative humidity fraction. Parameter `pAtm`
/// is atmospheric pressure in pascals and defaults to 101325 Pa. The output `h` is J/kg dry air.
/// `pAtm` must be finite and positive on validated models. Direct-construction invalid values,
/// non-finite runtime inputs, and saturated or over-saturated pressure denominators fail closed to
/// canonical NaN rather than evaluating the source division through a singularity.
#[derive(Clone, Copy, Debug)]
pub struct SpecificEnthalpyTDryBulPhi {
    /// Atmospheric pressure in pascals.
    pub(crate) p_atm: f64,
}

impl Default for SpecificEnthalpyTDryBulPhi {
    fn default() -> Self {
        Self { p_atm: 101_325.0 }
    }
}

impl Block for SpecificEnthalpyTDryBulPhi {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature =
            psychrometric_signature("CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let t_dry_bulb = read_real(inputs, 0);
        let phi = read_real(inputs, 1);
        if !t_dry_bulb.is_finite()
            || !phi.is_finite()
            || !self.p_atm.is_finite()
            || self.p_atm <= 0.0
        {
            emit_real(0, f64::NAN, emit);
            return;
        }

        let t_c = t_dry_bulb - KELVIN_OFFSET;
        let p_w = phi * saturation_pressure(t_dry_bulb);
        let denominator = self.p_atm - p_w;
        if !p_w.is_finite() || !denominator.is_finite() || denominator <= 0.0 {
            emit_real(0, f64::NAN, emit);
            return;
        }

        let w = HUMIDITY_RATIO_COEFFICIENT * p_w / denominator;
        let h = 1006.0 * t_c + w * (2_501_014.5 + 1860.0 * t_c);
        emit_real(0, h, emit);
    }
}

/// `CDL.Psychrometrics.WetBulb_TDryBulPhi` computes wet-bulb temperature with the Stull formula.
///
/// Inputs are `TDryBul` in kelvin and `phi` as a 0..1 relative humidity fraction. The output
/// `TWetBul` is kelvin. Buildings documents the Stull approximation as valid for 5..99% relative
/// humidity and -20..50 degC dry-bulb temperature at sea-level pressure; finite out-of-range values
/// mirror the unguarded source formula, while non-finite inputs fail closed to canonical NaN.
#[derive(Clone, Copy, Debug, Default)]
pub struct WetBulbTDryBulPhi;

impl Block for WetBulbTDryBulPhi {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature =
            psychrometric_signature("CDL.Psychrometrics.WetBulb_TDryBulPhi");
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        true
    }

    fn step_algebraic(&self, _ctx: &Ctx<'_>, inputs: &[Value], emit: &mut dyn FnMut(usize, Value)) {
        let t_dry_bulb = read_real(inputs, 0);
        let phi = read_real(inputs, 1);
        if !t_dry_bulb.is_finite() || !phi.is_finite() {
            emit_real(0, f64::NAN, emit);
            return;
        }

        let t_c = t_dry_bulb - KELVIN_OFFSET;
        let rh_per = 100.0 * phi;
        if !rh_per.is_finite() {
            emit_real(0, f64::NAN, emit);
            return;
        }

        let wet_bulb = KELVIN_OFFSET
            + t_c * libm::atan(0.151_977 * libm::sqrt(rh_per + 8.313_659))
            + libm::atan(t_c + rh_per)
            - libm::atan(rh_per - 1.676_331)
            + 0.003_918_38 * libm::pow(rh_per, 1.5) * libm::atan(0.023_101 * rh_per)
            - 4.686_035;
        emit_real(0, wet_bulb, emit);
    }
}

const fn psychrometric_signature(class_path: &'static str) -> BlockSignature {
    BlockSignature {
        class_path,
        inputs: &[PortKind::Real, PortKind::Real],
        outputs: &[PortKind::Real],
        stateful: false,
    }
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
    let exponent = -13.928_169_0 - (-13.928_169_0) * libm::pow(r1, -1.5) + 34.707_823_8
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
