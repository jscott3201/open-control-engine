//! G36 Generic.AirEconomizerHighLimits restricted fixed-dry-bulb oracle.

use crate::oracle::{Golden, ValueKind};

use super::{
    AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18, AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21,
    AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24, r, sequence_golden, unit_ticks,
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
