//! G36 MultiZone VAV PlantRequests sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{PLANT_REQUESTS, i, input_r, latch, sequence_golden, true_delay};

pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..20).map(|tick| f64::from(tick) * 60.0).collect();
    let supply_air_temperature_setpoint = [
        300.0, 295.0, 295.0, 295.0, 295.0, 300.0, 300.0, 300.0, 300.0, 320.0,
        320.0, 320.0, 320.0, 320.0, 320.0, 310.0, 300.0, 300.0, 300.0, 300.0,
    ];
    let supply_air_temperature = [
        300.0, 299.0, 299.0, 299.0, 297.5, 300.0, 300.0, 300.0, 300.0, 300.0,
        300.0, 300.0, 300.0, 300.0, 300.0, 300.0, 300.0, 300.0, 300.0, 300.0,
    ];
    let cooling_coil_valve = [
        0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.9, 0.8, 0.05, 0.05, 0.05, 0.05, 0.05,
        0.05, 0.05, 0.05, 0.05, 0.05, 0.05, 0.05,
    ];
    let heating_coil_valve = [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        0.0, 0.0, 1.0, 0.9, 0.8, 0.05,
    ];

    let (chilled_water_reset, chiller_plant, hot_water_reset, hot_water_plant) =
        plant_requests_trace(
            &time,
            &supply_air_temperature,
            &supply_air_temperature_setpoint,
            &cooling_coil_valve,
            &heating_coil_valve,
        );
    let inputs = plant_requests_inputs(
        &supply_air_temperature,
        &supply_air_temperature_setpoint,
        &cooling_coil_valve,
        &heating_coil_valve,
    );

    vec![
        sequence_golden(
            PLANT_REQUESTS,
            "chilled_water_reset_request",
            ValueKind::Integer,
            time.clone(),
            chilled_water_reset.into_iter().map(i).collect(),
            "PlantRequests: cooling SAT error holds above 3 K for 120s, then above 2 K, then cooling valve latch set/clear",
            "Pinned PlantRequests.mo WaterBased cooling branch: Subtract(TAirSup,TAirSupSet), GreaterThreshold(t=3/2,h=THys), TrueDelay(120s), valve Latch set at >0.95 and clear at <0.85, integer switch ladder 3/2/1/0",
            inputs.clone(),
        ),
        sequence_golden(
            PLANT_REQUESTS,
            "chiller_plant_request",
            ValueKind::Integer,
            time.clone(),
            chiller_plant.into_iter().map(i).collect(),
            "PlantRequests: cooling valve plant latch sets at >0.95 and remains set until valve drops below 0.1",
            "Pinned PlantRequests.mo WaterBased cooling plant branch: valve GreaterThreshold(t=0.95,h=posHys) sets Latch and LessThreshold(t=0.1,h=posHys) clears it; switch emits 1 or 0",
            inputs.clone(),
        ),
        sequence_golden(
            PLANT_REQUESTS,
            "hot_water_reset_request",
            ValueKind::Integer,
            time.clone(),
            hot_water_reset.into_iter().map(i).collect(),
            "PlantRequests: heating SAT deficit holds above 17 K for 300s, then above 8 K, then heating valve latch set/clear",
            "Pinned PlantRequests.mo WaterBased heating branch: Subtract(TAirSupSet,TAirSup), GreaterThreshold(t=17/8,h=THys), TrueDelay(300s), valve Latch set at >0.95 and clear at <0.85, integer switch ladder 3/2/1/0",
            inputs.clone(),
        ),
        sequence_golden(
            PLANT_REQUESTS,
            "hot_water_plant_request",
            ValueKind::Integer,
            time,
            hot_water_plant.into_iter().map(i).collect(),
            "PlantRequests: heating valve plant latch sets at >0.95 and remains set until valve drops below 0.1",
            "Pinned PlantRequests.mo WaterBased heating plant branch: valve GreaterThreshold(t=0.95,h=posHys) sets Latch and LessThreshold(t=0.1,h=posHys) clears it; switch emits 1 or 0",
            inputs,
        ),
    ]
}

fn plant_requests_trace(
    time: &[f64],
    supply_air_temperature: &[f64],
    supply_air_temperature_setpoint: &[f64],
    cooling_coil_valve: &[f64],
    heating_coil_valve: &[f64],
) -> (Vec<i64>, Vec<i64>, Vec<i64>, Vec<i64>) {
    const T_HYS: f64 = 0.1;
    const POS_HYS: f64 = 0.05;

    let cooling_difference: Vec<f64> = supply_air_temperature
        .iter()
        .zip(supply_air_temperature_setpoint)
        .map(|(&supply, &setpoint)| supply - setpoint)
        .collect();
    let heating_difference: Vec<f64> = supply_air_temperature_setpoint
        .iter()
        .zip(supply_air_temperature)
        .map(|(&setpoint, &supply)| setpoint - supply)
        .collect();

    let cooling_reset_3 = true_delay(
        time,
        &greater_threshold(&cooling_difference, 3.0, T_HYS),
        120.0,
    );
    let cooling_reset_2 = true_delay(
        time,
        &greater_threshold(&cooling_difference, 2.0, T_HYS),
        120.0,
    );
    let cooling_valve_high = greater_threshold(cooling_coil_valve, 0.95, POS_HYS);
    let cooling_reset_clear = less_threshold(cooling_coil_valve, 0.85, POS_HYS);
    let cooling_plant_clear = less_threshold(cooling_coil_valve, 0.1, POS_HYS);
    let cooling_reset_1 = latch(&cooling_valve_high, &cooling_reset_clear);
    let chiller_plant = latch(&cooling_valve_high, &cooling_plant_clear)
        .into_iter()
        .map(i64::from)
        .collect::<Vec<_>>();
    let chilled_water_reset = switch_ladder(&cooling_reset_3, &cooling_reset_2, &cooling_reset_1);

    let heating_reset_3 = true_delay(
        time,
        &greater_threshold(&heating_difference, 17.0, T_HYS),
        300.0,
    );
    let heating_reset_2 = true_delay(
        time,
        &greater_threshold(&heating_difference, 8.0, T_HYS),
        300.0,
    );
    let heating_valve_high = greater_threshold(heating_coil_valve, 0.95, POS_HYS);
    let heating_reset_clear = less_threshold(heating_coil_valve, 0.85, POS_HYS);
    let heating_plant_clear = less_threshold(heating_coil_valve, 0.1, POS_HYS);
    let heating_reset_1 = latch(&heating_valve_high, &heating_reset_clear);
    let hot_water_plant = latch(&heating_valve_high, &heating_plant_clear)
        .into_iter()
        .map(i64::from)
        .collect::<Vec<_>>();
    let hot_water_reset = switch_ladder(&heating_reset_3, &heating_reset_2, &heating_reset_1);

    (
        chilled_water_reset,
        chiller_plant,
        hot_water_reset,
        hot_water_plant,
    )
}

fn greater_threshold(values: &[f64], threshold: f64, hysteresis: f64) -> Vec<bool> {
    let mut previous = false;
    let mut out = Vec::with_capacity(values.len());
    for &value in values {
        let next = (!previous && value > threshold)
            || (previous && value > threshold - hysteresis);
        out.push(next);
        previous = next;
    }
    out
}

fn less_threshold(values: &[f64], threshold: f64, hysteresis: f64) -> Vec<bool> {
    let mut previous = false;
    let mut out = Vec::with_capacity(values.len());
    for &value in values {
        let next = (!previous && value < threshold)
            || (previous && value < threshold + hysteresis);
        out.push(next);
        previous = next;
    }
    out
}

fn switch_ladder(three: &[bool], two: &[bool], one: &[bool]) -> Vec<i64> {
    three
        .iter()
        .zip(two)
        .zip(one)
        .map(|((&three, &two), &one)| {
            if three {
                3
            } else if two {
                2
            } else if one {
                1
            } else {
                0
            }
        })
        .collect()
}

fn plant_requests_inputs(
    supply_air_temperature: &[f64],
    supply_air_temperature_setpoint: &[f64],
    cooling_coil_valve: &[f64],
    heating_coil_valve: &[f64],
) -> Vec<InputSeries> {
    vec![
        input_r(
            "supply_air_temperature",
            supply_air_temperature.iter().copied(),
        ),
        input_r(
            "supply_air_temperature_setpoint",
            supply_air_temperature_setpoint.iter().copied(),
        ),
        input_r("cooling_coil_valve", cooling_coil_valve.iter().copied()),
        input_r("heating_coil_valve", heating_coil_valve.iter().copied()),
    ]
}
