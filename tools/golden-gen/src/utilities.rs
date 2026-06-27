//! `CDL.Utilities` source-derived goldens.
//!
//! These references duplicate the pinned Buildings `SunRiseSet.mo` formulas independently from
//! `oce-blocks`. The oracle owns its own state simulation for the held `nextSunRise`/`nextSunSet`
//! event outputs and records the solar parameters used for each scenario.

use crate::oracle::{Golden, Sample, ValueKind};

const CLASS_PATH: &str = "CDL.Utilities.SunRiseSet";
const SECONDS_PER_DAY: f64 = 86_400.0;
const HALF_DAY_SECONDS: f64 = 43_200.0;
const JULIAN_YEAR_DAYS: f64 = 365.25;
const MAX_POLAR_SEARCH_DAYS: i32 = 732;

#[derive(Clone, Copy, Debug)]
struct SolarParams {
    lat: f64,
    lon: f64,
    tim_zon: f64,
}

#[derive(Clone, Copy, Debug)]
struct SolarOutput {
    next_sun_rise: f64,
    next_sun_set: f64,
    sun_up: bool,
}

#[derive(Clone, Copy, Debug)]
struct SolarState {
    next_sun_rise: f64,
    next_sun_set: f64,
    sta_tim: f64,
}

#[derive(Clone, Copy, Debug)]
struct HourAngle {
    hou_ang: f64,
    t_next: f64,
    tim_cor: f64,
}

/// Build `CDL.Utilities` goldens.
pub fn goldens() -> Vec<Golden> {
    let mut out = Vec::new();
    out.extend(scenario(
        None,
        SolarParams {
            lat: 0.0,
            lon: 0.0,
            tim_zon: 0.0,
        },
        default_equator_times(),
        "default equator/UTC parameters; samples cover initialization, sunrise crossing, and sunset crossing",
    ));
    out.extend(scenario(
        Some("san_francisco_validation"),
        SolarParams {
            lat: 0.645_771_823_237_9,
            lon: -2.129_301_687_433_1,
            tim_zon: -28_800.0,
        },
        validation_city_times(SolarParams {
            lat: 0.645_771_823_237_9,
            lon: -2.129_301_687_433_1,
            tim_zon: -28_800.0,
        }),
        "San Francisco parameters from Buildings Utilities.Validation.SunRiseSet; samples cross same-day rise/set events",
    ));
    out.extend(scenario(
        Some("arctic_polar_day"),
        SolarParams {
            lat: 1.256_637_061_435_9,
            lon: -1.256_637_061_435_9,
            tim_zon: -18_000.0,
        },
        arctic_polar_day_times(),
        "Arctic validation parameters during northern-summer polar-day interval; samples cover held sun-up period and next sunset crossing",
    ));
    out
}

fn default_equator_times() -> Vec<f64> {
    let params = SolarParams {
        lat: 0.0,
        lon: 0.0,
        tim_zon: 0.0,
    };
    let first = initial_state(params, 0.0).expect("default equator initializes");
    vec![0.0, first.next_sun_rise + 1.0, first.next_sun_set + 1.0]
}

fn validation_city_times(params: SolarParams) -> Vec<f64> {
    let t0 = 180.0 * SECONDS_PER_DAY;
    let first = initial_state(params, t0).expect("validation city initializes");
    vec![t0, first.next_sun_rise + 60.0, first.next_sun_set + 60.0]
}

fn arctic_polar_day_times() -> Vec<f64> {
    let params = SolarParams {
        lat: 1.256_637_061_435_9,
        lon: -1.256_637_061_435_9,
        tim_zon: -18_000.0,
    };
    let t0 = 172.0 * SECONDS_PER_DAY;
    let first = initial_state(params, t0).expect("arctic polar day initializes");
    let middle = if first.next_sun_set > t0 + SECONDS_PER_DAY {
        (t0 + first.next_sun_set) / 2.0
    } else {
        t0 + SECONDS_PER_DAY
    };
    vec![t0, middle, first.next_sun_set + 60.0]
}

fn scenario(
    name: Option<&'static str>,
    params: SolarParams,
    time: Vec<f64>,
    input_desc: &'static str,
) -> Vec<Golden> {
    let outputs = simulate(params, &time);
    let rule_desc = "stateful Buildings SunRiseSet recurrence: initialize nextSunRise/nextSunSet from computeSunRise/computeSunSet(time-86400), apply polar-day nextSunSet-86400 branch when cosHou<-1, update each held event time when t>=pre(event), and emit sunUp=nextSunSet<nextSunRise";
    let param_desc = format!(
        "lat={} rad, lon={} rad, timZon={} s",
        params.lat, params.lon, params.tim_zon
    );
    let math = "libm 0.2.16, default-features=false, pure Rust deterministic trig";

    [
        (
            "nextSunRise",
            ValueKind::Real,
            outputs
                .iter()
                .map(|output| Sample::Real(output.next_sun_rise))
                .collect::<Vec<_>>(),
        ),
        (
            "nextSunSet",
            ValueKind::Real,
            outputs
                .iter()
                .map(|output| Sample::Real(output.next_sun_set))
                .collect::<Vec<_>>(),
        ),
        (
            "sunUp",
            ValueKind::Boolean,
            outputs
                .iter()
                .map(|output| Sample::Boolean(output.sun_up))
                .collect::<Vec<_>>(),
        ),
    ]
    .into_iter()
    .map(|(signal, kind, samples)| {
        let mut golden = Golden::new(CLASS_PATH, signal, kind, time.clone(), samples, input_desc, rule_desc)
            .with_provenance("parameters", param_desc.clone())
            .with_provenance("math_library", math);
        if let Some(name) = name {
            golden = golden.with_scenario(name);
        }
        golden
    })
    .collect()
}

fn simulate(params: SolarParams, times: &[f64]) -> Vec<SolarOutput> {
    let mut state = None::<SolarState>;
    let mut outputs = Vec::with_capacity(times.len());
    for &t in times {
        let next = match state {
            Some(prev) => step_state(params, prev, t).expect("solar step has bounded solution"),
            None => initial_state(params, t).expect("solar init has bounded solution"),
        };
        outputs.push(SolarOutput {
            next_sun_rise: next.next_sun_rise,
            next_sun_set: next.next_sun_set,
            sun_up: next.next_sun_set < next.next_sun_rise,
        });
        state = Some(next);
    }
    outputs
}

fn initial_state(params: SolarParams, t: f64) -> Option<SolarState> {
    let sta_tim = t;
    let tim_dif_loc_civ = time_difference_local_civil(params);
    let next_sun_rise = compute_sun_rise(t - SECONDS_PER_DAY, sta_tim, tim_dif_loc_civ, params.lat)?;
    let mut next_sun_set = compute_sun_set(t - SECONDS_PER_DAY, sta_tim, tim_dif_loc_civ, params.lat)?;
    if current_cos_hour(t, params.lat).is_some_and(|cos_hou| cos_hou < -1.0) {
        next_sun_set -= SECONDS_PER_DAY;
    }
    Some(SolarState {
        next_sun_rise,
        next_sun_set,
        sta_tim,
    })
}

fn step_state(params: SolarParams, prev: SolarState, t: f64) -> Option<SolarState> {
    let tim_dif_loc_civ = time_difference_local_civil(params);
    let mut next_sun_rise = prev.next_sun_rise;
    let mut next_sun_set = prev.next_sun_set;
    if t >= next_sun_rise {
        next_sun_rise = compute_sun_rise(t, prev.sta_tim, tim_dif_loc_civ, params.lat)?;
    }
    if t >= next_sun_set {
        next_sun_set = compute_sun_set(t, prev.sta_tim, tim_dif_loc_civ, params.lat)?;
    }
    Some(SolarState {
        next_sun_rise,
        next_sun_set,
        sta_tim: prev.sta_tim,
    })
}

fn time_difference_local_civil(params: SolarParams) -> f64 {
    params.lon * HALF_DAY_SECONDS / std::f64::consts::PI - params.tim_zon
}

fn next_hour_angle(t: f64, tim_dif_loc_civ: f64, lat: f64) -> Option<HourAngle> {
    for i_day in 1..=MAX_POLAR_SEARCH_DAYS {
        let t_next = t + f64::from(i_day) * SECONDS_PER_DAY;
        let bt = std::f64::consts::PI * ((t_next + SECONDS_PER_DAY) / SECONDS_PER_DAY - 81.0)
            / 182.0;
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

fn compute_sun_rise(t: f64, sta_tim: f64, tim_dif_loc_civ: f64, lat: f64) -> Option<f64> {
    let hour = next_hour_angle(t, tim_dif_loc_civ, lat)?;
    let sun_rise = (12.0 - hour.hou_ang * 24.0 / (2.0 * std::f64::consts::PI)
        - hour.tim_cor / 3600.0)
        * 3600.0
        + libm::floor(hour.t_next / SECONDS_PER_DAY) * SECONDS_PER_DAY;
    Some(if sta_tim > sun_rise {
        sun_rise + SECONDS_PER_DAY
    } else {
        sun_rise
    })
}

fn compute_sun_set(t: f64, sta_tim: f64, tim_dif_loc_civ: f64, lat: f64) -> Option<f64> {
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

fn current_cos_hour(t: f64, lat: f64) -> Option<f64> {
    let dec_ang = declination_angle(t);
    let cos_hou = -libm::tan(lat) * libm::tan(dec_ang);
    cos_hou.is_finite().then_some(cos_hou)
}

fn equation_of_time(bt: f64) -> f64 {
    60.0 * (9.87 * libm::sin(2.0 * bt) - 7.53 * libm::cos(bt) - 1.5 * libm::sin(bt))
}

fn declination_angle(t: f64) -> f64 {
    let k1 = libm::sin(23.45 * 2.0 * std::f64::consts::PI / 360.0);
    let k2 = 2.0 * std::f64::consts::PI / JULIAN_YEAR_DAYS;
    libm::asin(-k1 * libm::cos((t / SECONDS_PER_DAY + 10.0) * k2))
}
