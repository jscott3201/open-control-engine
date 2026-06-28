//! G36 Generic.AirEconomizerHighLimits restricted fixed-dry-bulb oracle.

use crate::oracle::{Golden, ValueKind};

use super::{
    AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18, AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21,
    AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24, AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22, AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24, r, sequence_golden, unit_ticks,
};

pub(super) fn goldens() -> Vec<Golden> {
    let cases = [
        (
            AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24,
            297.15,
            "ASHRAE 90.1 fixed dry-bulb high-limit bucket for climate zones 1B, 2B, 3B, 3C, 4B, 4C, 5B, 5C, 6B, 7, and 8",
            "AirEconomizerHighLimits.mo ASHRAE90_1 + FixedDryBulb table selects con.k = 273.15 + 24 K",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21,
            294.15,
            "ASHRAE 90.1 fixed dry-bulb high-limit bucket for climate zones 5A and 6A",
            "AirEconomizerHighLimits.mo ASHRAE90_1 + FixedDryBulb table selects con1.k = 273.15 + 21 K",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18,
            291.15,
            "ASHRAE 90.1 fixed dry-bulb high-limit bucket for climate zones 1A, 2A, 3A, and 4A",
            "AirEconomizerHighLimits.mo ASHRAE90_1 + FixedDryBulb table selects con2.k = 273.15 + 18 K",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24,
            297.15,
            "Title24 fixed dry-bulb high-limit bucket for California climate zones 1, 3, 5, and 11 through 16",
            "AirEconomizerHighLimits.mo California_Title_24 + FixedDryBulb table selects con5.k = 273.15 + 24 K",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23,
            296.15,
            "Title24 fixed dry-bulb high-limit bucket for California climate zones 2, 4, and 10",
            "AirEconomizerHighLimits.mo California_Title_24 + FixedDryBulb table selects con6.k = 273.15 + 23 K",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22,
            295.15,
            "Title24 fixed dry-bulb high-limit bucket for California climate zones 6, 8, and 9",
            "AirEconomizerHighLimits.mo California_Title_24 + FixedDryBulb table selects con7.k = 273.15 + 22 K",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21,
            294.15,
            "Title24 fixed dry-bulb high-limit bucket for California climate zone 7",
            "AirEconomizerHighLimits.mo California_Title_24 + FixedDryBulb table selects con8.k = 273.15 + 21 K",
        ),
    ];

    cases
        .into_iter()
        .map(|(scenario, cutoff, input_desc, rule_desc)| {
            sequence_golden(
                scenario,
                "temperature_cutoff",
                ValueKind::Real,
                unit_ticks(1),
                vec![r(cutoff)],
                input_desc,
                rule_desc,
                Vec::new(),
            )
        })
        .collect()
}
