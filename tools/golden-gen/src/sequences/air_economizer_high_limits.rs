//! G36 Generic.AirEconomizerHighLimits restricted dry-bulb oracles.

use crate::oracle::{Golden, InputSeries, Sample, ValueKind};

use super::{
    AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL, AIR_ECONOMIZER_HIGH_LIMITS_FIXED_18,
    AIR_ECONOMIZER_HIGH_LIMITS_FIXED_21, AIR_ECONOMIZER_HIGH_LIMITS_FIXED_24,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_21, AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_22,
    AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_23, AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_FIXED_24, r,
    sequence_golden, unit_ticks,
};

pub(super) fn goldens() -> Vec<Golden> {
    let fixed_cases = [
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

    let mut out = fixed_cases
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
        .collect::<Vec<_>>();

    let return_air_temperature = [289.25, 293.15, 297.5, 301.75];
    let differential_cases = [
        (
            AIR_ECONOMIZER_HIGH_LIMITS_ASHRAE_DIFFERENTIAL,
            0.0,
            "ASHRAE 90.1 differential dry-bulb high-limit allowed-zone branch with return_air_temperature=[289.25,293.15,297.5,301.75] K",
            "AirEconomizerHighLimits.mo ASHRAE90_1 + DifferentialDryBulb selected branch uses TCut=TRet; fixture-local AddParameter(p=0) is the explicit Real identity bridge",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_0,
            0.0,
            "Title24 differential dry-bulb high-limit bucket for California zones 1, 3, 5, and 11 through 16; return_air_temperature=[289.25,293.15,297.5,301.75] K",
            "AirEconomizerHighLimits.mo California_Title_24 + DifferentialDryBulb zero-offset bucket uses TCut=TRet; fixture-local AddParameter(p=0) is the explicit Real identity bridge",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_1,
            -1.0,
            "Title24 differential dry-bulb high-limit bucket for California zones 2, 4, and 10; return_air_temperature=[289.25,293.15,297.5,301.75] K",
            "AirEconomizerHighLimits.mo California_Title_24 + DifferentialDryBulb table selects addPar.p=-1 K, so TCut=TRet-1 K",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_2,
            -2.0,
            "Title24 differential dry-bulb high-limit bucket for California zones 6, 8, and 9; return_air_temperature=[289.25,293.15,297.5,301.75] K",
            "AirEconomizerHighLimits.mo California_Title_24 + DifferentialDryBulb table selects addPar1.p=-2 K, so TCut=TRet-2 K",
        ),
        (
            AIR_ECONOMIZER_HIGH_LIMITS_TITLE24_DIFFERENTIAL_OFFSET_3,
            -3.0,
            "Title24 differential dry-bulb high-limit bucket for California zone 7; return_air_temperature=[289.25,293.15,297.5,301.75] K",
            "AirEconomizerHighLimits.mo California_Title_24 + DifferentialDryBulb table selects addPar2.p=-3 K, so TCut=TRet-3 K",
        ),
    ];

    out.extend(
        differential_cases
            .into_iter()
            .map(|(scenario, offset, input_desc, rule_desc)| {
                sequence_golden(
                    scenario,
                    "temperature_cutoff",
                    ValueKind::Real,
                    unit_ticks(return_air_temperature.len()),
                    return_air_temperature
                        .iter()
                        .map(|&temperature| r(temperature + offset))
                        .collect(),
                    input_desc,
                    rule_desc,
                    return_air_inputs(&return_air_temperature),
                )
            }),
    );

    out
}

fn return_air_inputs(return_air_temperature: &[f64]) -> Vec<InputSeries> {
    vec![InputSeries::new(
        "return_air_temperature",
        ValueKind::Real,
        return_air_temperature
            .iter()
            .copied()
            .map(Sample::Real)
            .collect(),
    )]
}
