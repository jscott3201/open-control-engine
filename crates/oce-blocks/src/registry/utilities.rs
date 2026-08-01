use oce_model::ParamTable;

use super::{real_param, string_param};
use crate::utilities::SUN_RISE_SET_PARAM_RULES;
use crate::{Assert, Block, ParamDefault, RegistryEntry, SunRiseSet};

const ASSERT_MESSAGE_FALLBACK: &str = "";
const LAT_FALLBACK: f64 = 0.0;
const LON_FALLBACK: f64 = 0.0;
const TIM_ZON_FALLBACK: f64 = 0.0;

pub(super) const ASSERT_PARAM_DEFAULTS: &[ParamDefault] = &[param_default_required!("message")];
pub(super) const SUN_RISE_SET_PARAM_DEFAULTS: &[ParamDefault] = &[
    param_default_required!("lat"),
    param_default_required!("lon"),
    param_default_required!("timZon"),
];

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Utilities.Assert",
        make: make_assert,
    },
    RegistryEntry {
        class_path: "CDL.Utilities.SunRiseSet",
        make: make_sun_rise_set,
    },
];

/// Upstream `Utilities/Assert.mo` (pin `a131864`) declares `parameter String message` with NO
/// default value ("Message written when u becomes false"). Omitting it previously fell through to a
/// silent empty-string engine default, so a tripped assertion would emit a blank diagnostic; the
/// message is required at load time.
pub(super) const ASSERT_PARAM_RULES: &[crate::ParamRule] = &[crate::ParamRule::Required {
    name: "message",
    kind: oce_model::ValueType::String,
}];

fn make_assert(p: &ParamTable) -> Box<dyn Block> {
    Box::new(Assert {
        message: string_param(p, "message", ASSERT_MESSAGE_FALLBACK),
    })
}

pub(super) const SUN_RISE_SET_RULES: &[crate::ParamRule] = SUN_RISE_SET_PARAM_RULES;

fn make_sun_rise_set(p: &ParamTable) -> Box<dyn Block> {
    Box::new(SunRiseSet {
        lat: real_param(p, "lat", LAT_FALLBACK),
        lon: real_param(p, "lon", LON_FALLBACK),
        tim_zon: real_param(p, "timZon", TIM_ZON_FALLBACK),
    })
}
