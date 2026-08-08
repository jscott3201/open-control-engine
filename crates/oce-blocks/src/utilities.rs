//! `CDL.Utilities` blocks that interact with diagnostics or model-time utility formulas.

use std::sync::Arc;

use oce_model::{ParamTable, Value, determinism::canonicalize_real};

use crate::{Block, BlockKind, BlockSignature, Ctx, PortKind, Time, emit_real, read_bool};

const SUN_NEXT_RISE_WORD: usize = 0;
const SUN_NEXT_SET_WORD: usize = 1;
const SUN_START_TIME_WORD: usize = 2;
const SUN_INITIALIZED_WORD: usize = 3;
const SUN_WARNED_WORD: usize = 4;
const SUN_STATE_WORDS: usize = 5;

const SECONDS_PER_DAY: f64 = 86_400.0;
const HALF_DAY_SECONDS: f64 = 43_200.0;
const JULIAN_YEAR_DAYS: f64 = 365.25;
const MAX_POLAR_SEARCH_DAYS: i32 = 732;

const LAT_MIN: f64 = -std::f64::consts::FRAC_PI_2;
const LAT_MAX: f64 = std::f64::consts::FRAC_PI_2;
const LON_MIN: f64 = -std::f64::consts::PI;
const LON_MAX: f64 = std::f64::consts::PI;

/// `CDL.Utilities.Assert` — warning sink with no signal outputs.
///
/// The upstream implementation is `Buildings/Controls/OBC/CDL/Utilities/Assert.mo`: line 11 is
/// the stateless equation `assert(u, message, AssertionLevel.warning)`. There is no `pre`, `edge`,
/// latch, or warn-once state in lines 1-12, so the engine emits one warning on every evaluation
/// whose Boolean input is `false`. The warning is routed only through [`Ctx::warn`]; the block has
/// no output connector and does not mutate scheduler state.
#[derive(Clone, Debug)]
pub struct Assert {
    pub(crate) message: Arc<str>,
}

impl Default for Assert {
    fn default() -> Self {
        Self {
            message: Arc::from(""),
        }
    }
}

/// `CDL.Utilities.SunRiseSet` - next sunrise/sunset event-time source.
///
/// The Buildings source uses model time only: it never reads wall-clock time, daylight saving
/// state, locale, or host timezone. Parameters are latitude `lat` and longitude `lon` in radians
/// plus `timZon` in seconds relative to UTC. Outputs are `nextSunRise` and `nextSunSet` in model
/// seconds, plus `sunUp = nextSunSet < nextSunRise`. The two event-time outputs are stateful and
/// update when model time reaches the previously held event time.
///
/// The upstream helper searches day-by-day through polar no-rise/no-set periods. This
/// implementation mirrors that search but bounds it to two Julian years, warning once and emitting
/// canonical `NaN` event times if invalid coordinates or an unbounded polar case would otherwise
/// make the tick non-terminating. It has no signal inputs and therefore no direct feedthrough.
#[derive(Clone, Copy, Debug)]
pub struct SunRiseSet {
    /// Latitude in radians.
    pub(crate) lat: f64,
    /// Longitude in radians.
    pub(crate) lon: f64,
    /// Time zone offset from UTC in seconds.
    pub(crate) tim_zon: f64,
}

impl Default for SunRiseSet {
    fn default() -> Self {
        Self {
            lat: 0.0,
            lon: 0.0,
            tim_zon: 0.0,
        }
    }
}

impl SunRiseSet {
    fn step(self, ctx: &Ctx<'_>, region: &[u64]) -> SunRiseSetStep {
        if !valid_solar_params(self) || !ctx.t().is_finite() {
            return SunRiseSetStep::invalid();
        }

        if !sun_initialized(region) {
            return self.initial_step(ctx.t());
        }

        let sta_tim = f64::from_bits(region[SUN_START_TIME_WORD]);
        let tim_dif_loc_civ = self.time_difference_local_civil();
        let mut next_sun_rise = f64::from_bits(region[SUN_NEXT_RISE_WORD]);
        let mut next_sun_set = f64::from_bits(region[SUN_NEXT_SET_WORD]);

        if ctx.t() >= next_sun_rise {
            let Some(next) = compute_sun_rise(ctx.t(), sta_tim, tim_dif_loc_civ, self.lat) else {
                return SunRiseSetStep::invalid();
            };
            next_sun_rise = next;
        }
        if ctx.t() >= next_sun_set {
            let Some(next) = compute_sun_set(ctx.t(), sta_tim, tim_dif_loc_civ, self.lat) else {
                return SunRiseSetStep::invalid();
            };
            next_sun_set = next;
        }

        SunRiseSetStep {
            next_sun_rise,
            next_sun_set,
            sun_up: next_sun_set < next_sun_rise,
            sta_tim,
            valid: true,
        }
    }

    fn initial_step(self, t: Time) -> SunRiseSetStep {
        let sta_tim = t;
        let tim_dif_loc_civ = self.time_difference_local_civil();
        let Some(next_sun_rise) =
            compute_sun_rise(t - SECONDS_PER_DAY, sta_tim, tim_dif_loc_civ, self.lat)
        else {
            return SunRiseSetStep::invalid();
        };
        let Some(mut next_sun_set) =
            compute_sun_set(t - SECONDS_PER_DAY, sta_tim, tim_dif_loc_civ, self.lat)
        else {
            return SunRiseSetStep::invalid();
        };

        if current_cos_hour(t, self.lat).is_some_and(|cos_hou| cos_hou < -1.0) {
            next_sun_set -= SECONDS_PER_DAY;
        }

        SunRiseSetStep {
            next_sun_rise,
            next_sun_set,
            sun_up: next_sun_set < next_sun_rise,
            sta_tim,
            valid: true,
        }
    }

    fn time_difference_local_civil(self) -> f64 {
        self.lon * HALF_DAY_SECONDS / std::f64::consts::PI - self.tim_zon
    }
}

impl Block for SunRiseSet {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Utilities.SunRiseSet",
            inputs: &[],
            outputs: &[PortKind::Real, PortKind::Real, PortKind::Boolean],
            stateful: true,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Stateful
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn state_len(&self) -> usize {
        SUN_STATE_WORDS
    }

    fn init_state(&self, region: &mut [u64], _params: &ParamTable) {
        region.fill(0);
        region[SUN_NEXT_RISE_WORD] = f64::NAN.to_bits();
        region[SUN_NEXT_SET_WORD] = f64::NAN.to_bits();
        region[SUN_START_TIME_WORD] = 0.0f64.to_bits();
    }

    fn emit_from_state(
        &self,
        ctx: &Ctx<'_>,
        _inputs: &[Value],
        region: &[u64],
        emit: &mut dyn FnMut(usize, Value),
    ) {
        let step = self.step(ctx, region);
        emit_real(0, step.next_sun_rise, emit);
        emit_real(1, step.next_sun_set, emit);
        emit(2, Value::Boolean(step.sun_up));
    }

    fn update_state(&self, ctx: &Ctx<'_>, _inputs: &[Value], region: &mut [u64]) {
        let step = self.step(ctx, region);
        if !step.valid && region[SUN_WARNED_WORD] == 0 {
            ctx.warn(
                self.signature().class_path,
                "SunRiseSet: invalid coordinates/timezone or no bounded sunrise/sunset solution",
            );
            region[SUN_WARNED_WORD] = 1;
        }
        region[SUN_NEXT_RISE_WORD] = canonicalize_real(step.next_sun_rise).to_bits();
        region[SUN_NEXT_SET_WORD] = canonicalize_real(step.next_sun_set).to_bits();
        region[SUN_START_TIME_WORD] = canonicalize_real(step.sta_tim).to_bits();
        region[SUN_INITIALIZED_WORD] = 1;
    }
}

#[derive(Clone, Copy, Debug)]
struct SunRiseSetStep {
    next_sun_rise: f64,
    next_sun_set: f64,
    sun_up: bool,
    sta_tim: Time,
    valid: bool,
}

impl SunRiseSetStep {
    fn invalid() -> Self {
        Self {
            next_sun_rise: f64::NAN,
            next_sun_set: f64::NAN,
            sun_up: false,
            sta_tim: 0.0,
            valid: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct HourAngle {
    hou_ang: f64,
    t_next: Time,
    tim_cor: Time,
}

fn sun_initialized(region: &[u64]) -> bool {
    region[SUN_INITIALIZED_WORD] != 0
}

fn valid_solar_params(block: SunRiseSet) -> bool {
    block.lat.is_finite()
        && block.lon.is_finite()
        && block.tim_zon.is_finite()
        && (LAT_MIN..=LAT_MAX).contains(&block.lat)
        && (LON_MIN..=LON_MAX).contains(&block.lon)
}

fn next_hour_angle(t: Time, tim_dif_loc_civ: Time, lat: f64) -> Option<HourAngle> {
    for i_day in 1..=MAX_POLAR_SEARCH_DAYS {
        let t_next = t + f64::from(i_day) * SECONDS_PER_DAY;
        let bt =
            std::f64::consts::PI * ((t_next + SECONDS_PER_DAY) / SECONDS_PER_DAY - 81.0) / 182.0;
        let eqn_tim = equation_of_time(bt);
        let tim_cor = eqn_tim + tim_dif_loc_civ;
        let dec_ang = declination_angle(t_next);
        let cos_hou = -libm::tan(lat) * libm::tan(dec_ang);
        if cos_hou.is_finite() && cos_hou.abs() <= 1.0 {
            return Some(HourAngle {
                hou_ang: libm::acos(cos_hou),
                t_next,
                tim_cor,
            });
        }
    }
    None
}

fn compute_sun_rise(t: Time, sta_tim: Time, tim_dif_loc_civ: Time, lat: f64) -> Option<Time> {
    let hour = next_hour_angle(t, tim_dif_loc_civ, lat)?;
    let sun_rise =
        (12.0 - hour.hou_ang * 24.0 / (2.0 * std::f64::consts::PI) - hour.tim_cor / 3600.0)
            * 3600.0
            + libm::floor(hour.t_next / SECONDS_PER_DAY) * SECONDS_PER_DAY;
    Some(if sta_tim > sun_rise {
        sun_rise + SECONDS_PER_DAY
    } else {
        sun_rise
    })
}

fn compute_sun_set(t: Time, sta_tim: Time, tim_dif_loc_civ: Time, lat: f64) -> Option<Time> {
    let hour = next_hour_angle(t, tim_dif_loc_civ, lat)?;
    let sun_set = (12.0 + hour.hou_ang * 24.0 / (2.0 * std::f64::consts::PI)
        - hour.tim_cor / 3600.0)
        * 3600.0
        + libm::floor(hour.t_next / SECONDS_PER_DAY) * SECONDS_PER_DAY;
    Some(if sta_tim > sun_set {
        sun_set + SECONDS_PER_DAY
    } else {
        sun_set
    })
}

fn current_cos_hour(t: Time, lat: f64) -> Option<f64> {
    let dec_ang = declination_angle(t);
    let cos_hou = -libm::tan(lat) * libm::tan(dec_ang);
    cos_hou.is_finite().then_some(cos_hou)
}

fn equation_of_time(bt: f64) -> Time {
    60.0 * (9.87 * libm::sin(2.0 * bt) - 7.53 * libm::cos(bt) - 1.5 * libm::sin(bt))
}

fn declination_angle(t: Time) -> f64 {
    let k1 = libm::sin(23.45 * 2.0 * std::f64::consts::PI / 360.0);
    let k2 = 2.0 * std::f64::consts::PI / JULIAN_YEAR_DAYS;
    libm::asin(-k1 * libm::cos((t / SECONDS_PER_DAY + 10.0) * k2))
}

pub(crate) const SUN_RISE_SET_PARAM_RULES: &[crate::ParamRule] = &[
    crate::ParamRule::Required {
        name: "lat",
        kind: oce_model::ValueType::Real,
    },
    crate::ParamRule::Required {
        name: "lon",
        kind: oce_model::ValueType::Real,
    },
    crate::ParamRule::Required {
        name: "timZon",
        kind: oce_model::ValueType::Real,
    },
    crate::ParamRule::RealGreaterOrEqual {
        name: "lat",
        min: LAT_MIN,
    },
    crate::ParamRule::RealLessOrEqualConstant {
        name: "lat",
        max: LAT_MAX,
    },
    crate::ParamRule::RealGreaterOrEqual {
        name: "lon",
        min: LON_MIN,
    },
    crate::ParamRule::RealLessOrEqualConstant {
        name: "lon",
        max: LON_MAX,
    },
    crate::ParamRule::RealFinite { name: "timZon" },
];

impl Block for Assert {
    fn signature(&self) -> &'static BlockSignature {
        static SIG: BlockSignature = BlockSignature {
            class_path: "CDL.Utilities.Assert",
            inputs: &[PortKind::Boolean],
            outputs: &[],
            stateful: false,
        };
        &SIG
    }

    fn kind(&self) -> BlockKind {
        BlockKind::Algebraic
    }

    fn feeds_through(&self, _in_idx: usize, _out_idx: usize) -> bool {
        false
    }

    fn step_algebraic(&self, ctx: &Ctx<'_>, inputs: &[Value], _emit: &mut dyn FnMut(usize, Value)) {
        if !read_bool(inputs, 0) {
            ctx.warn(self.signature().class_path, &self.message);
        }
    }
}
