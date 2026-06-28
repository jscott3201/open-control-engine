//! G36 MultiZone VAV OutdoorAirFlow ASHRAE 62.1 SumZone sequence oracle.

use crate::oracle::{Golden, InputSeries, ValueKind};

use super::{OUTDOOR_AIRFLOW_SUMZONE, input_i, input_r, r, sequence_golden};

pub(super) fn goldens() -> Vec<Golden> {
    let time: Vec<f64> = (0..6).map(f64::from).collect();
    let operation_mode_1 = [1, 1, 4, 7, 1, 1];
    let operation_mode_2 = [1, 7, 1, 6, 1, 1];
    let population_flow_1 = [1.0, 2.0, 2.5, 10.0, 0.0, 1.25];
    let population_flow_2 = [2.0, 4.0, 0.5, 20.0, 0.0, 2.5];
    let population_flow_3 = [3.0, 8.0, 1.5, 30.0, 0.0, 5.0];
    let area_flow_1 = [4.0, 1.0, 6.0, 3.0, 0.0, 8.0];
    let area_flow_2 = [5.0, 3.0, 2.0, 2.0, 1.0, 13.0];
    let area_flow_3 = [6.0, 5.0, 1.0, 1.0, 0.0, 21.0];
    let primary_flow_1 = [10.0, 2.0, 5.0, 0.0, 0.0, 1.0];
    let primary_flow_2 = [20.0, 4.0, 0.5, 0.0, 0.00005, 1.0];
    let primary_flow_3 = [30.0, 8.0, 1.5, 0.0, 0.5, 1.0];
    let minimum_outdoor_air_1 = [1.0, 1.0, 10.0, 1.0, 1.0, 0.2];
    let minimum_outdoor_air_2 = [2.0, 2.0, 0.1, 1.0, 1.0, 0.8];
    let minimum_outdoor_air_3 = [3.0, 4.0, 2.0, 1.0, 0.1, 0.3];

    let mut population_sum = Vec::with_capacity(time.len());
    let mut area_sum = Vec::with_capacity(time.len());
    let mut primary_sum = Vec::with_capacity(time.len());
    let mut maximum_outdoor_air_fraction = Vec::with_capacity(time.len());

    for row in 0..time.len() {
        let outputs = outdoor_airflow_sumzone_outputs(
            operation_mode_1[row],
            operation_mode_2[row],
            [
                population_flow_1[row],
                population_flow_2[row],
                population_flow_3[row],
            ],
            [area_flow_1[row], area_flow_2[row], area_flow_3[row]],
            [
                primary_flow_1[row],
                primary_flow_2[row],
                primary_flow_3[row],
            ],
            [
                minimum_outdoor_air_1[row],
                minimum_outdoor_air_2[row],
                minimum_outdoor_air_3[row],
            ],
        );
        population_sum.push(outputs.population_sum);
        area_sum.push(outputs.area_sum);
        primary_sum.push(outputs.primary_sum);
        maximum_outdoor_air_fraction.push(outputs.maximum_outdoor_air_fraction);
    }

    let inputs = outdoor_airflow_sumzone_inputs(SumZoneInputColumns {
        operation_mode_1: &operation_mode_1,
        operation_mode_2: &operation_mode_2,
        population_flow_1: &population_flow_1,
        population_flow_2: &population_flow_2,
        population_flow_3: &population_flow_3,
        area_flow_1: &area_flow_1,
        area_flow_2: &area_flow_2,
        area_flow_3: &area_flow_3,
        primary_flow_1: &primary_flow_1,
        primary_flow_2: &primary_flow_2,
        primary_flow_3: &primary_flow_3,
        minimum_outdoor_air_1: &minimum_outdoor_air_1,
        minimum_outdoor_air_2: &minimum_outdoor_air_2,
        minimum_outdoor_air_3: &minimum_outdoor_air_3,
    });

    vec![
        sequence_golden(
            OUTDOOR_AIRFLOW_SUMZONE,
            "summed_adjusted_population_breathing_zone_flow",
            ValueKind::Real,
            time.clone(),
            population_sum.into_iter().map(r).collect(),
            "OutdoorAirFlow.ASHRAE62_1.SumZone nGro=2/nZon=3: overlapping zone-group matrix covers both occupied, one occupied, none occupied, and zero-flow rows",
            "Pinned ASHRAE62_1/SumZone.mo: groFlo = zonGroMat * VAdjPopBreZon_flow with zonGroMat=[1,1,0;0,1,1]; occupied groups pass through via BooleanToReal, then MultiSum",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_SUMZONE,
            "summed_adjusted_area_breathing_zone_flow",
            ValueKind::Real,
            time.clone(),
            area_sum.into_iter().map(r).collect(),
            "OutdoorAirFlow.ASHRAE62_1.SumZone nGro=2/nZon=3: adjusted area aggregation follows the same occupancy-gated group matrix",
            "Pinned ASHRAE62_1/SumZone.mo: groFlo1 = zonGroMat * VAdjAreBreZon_flow; occupied operation mode is G36 OperationModes.occupied=1; unoccupied groups contribute zero",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_SUMZONE,
            "summed_zone_primary_airflow",
            ValueKind::Real,
            time.clone(),
            primary_sum.into_iter().map(r).collect(),
            "OutdoorAirFlow.ASHRAE62_1.SumZone nGro=2/nZon=3: zone primary airflow aggregation covers occupied groups and near-zero primary flow rows",
            "Pinned ASHRAE62_1/SumZone.mo: groFlo2 = zonGroMat * VZonPri_flow and the group results are occupancy gated before MultiSum",
            inputs.clone(),
        ),
        sequence_golden(
            OUTDOOR_AIRFLOW_SUMZONE,
            "maximum_zone_outdoor_air_fraction",
            ValueKind::Real,
            time,
            maximum_outdoor_air_fraction.into_iter().map(r).collect(),
            "OutdoorAirFlow.ASHRAE62_1.SumZone nGro=2/nZon=3: maximum fraction covers min cap, zero/near-zero denominator guard, and overlapping-zone amplification above 1",
            "Pinned ASHRAE62_1/SumZone.mo: min1=min(VMinOA_flow,VZonPri_flow), max2=max(VZonPri_flow,1e-4), div1=min1/max2, groFlo3=zonGroMatTra*occupied, mul3=groFlo3*div1, then MultiMax",
            inputs,
        ),
    ]
}

struct SumZoneOutputs {
    population_sum: f64,
    area_sum: f64,
    primary_sum: f64,
    maximum_outdoor_air_fraction: f64,
}

fn outdoor_airflow_sumzone_outputs(
    operation_mode_1: i64,
    operation_mode_2: i64,
    population_flow: [f64; 3],
    area_flow: [f64; 3],
    primary_flow: [f64; 3],
    minimum_outdoor_air: [f64; 3],
) -> SumZoneOutputs {
    let occupied = [operation_mode_1 == 1, operation_mode_2 == 1];
    let population_group = [
        population_flow[0] + population_flow[1],
        population_flow[1] + population_flow[2],
    ];
    let area_group = [area_flow[0] + area_flow[1], area_flow[1] + area_flow[2]];
    let primary_group = [
        primary_flow[0] + primary_flow[1],
        primary_flow[1] + primary_flow[2],
    ];
    let zone_fraction = [
        outdoor_air_fraction(primary_flow[0], minimum_outdoor_air[0]),
        outdoor_air_fraction(primary_flow[1], minimum_outdoor_air[1]),
        outdoor_air_fraction(primary_flow[2], minimum_outdoor_air[2]),
    ];
    let membership = [
        gated(occupied[0], 1.0),
        gated(occupied[0], 1.0) + gated(occupied[1], 1.0),
        gated(occupied[1], 1.0),
    ];

    SumZoneOutputs {
        population_sum: gated(occupied[0], population_group[0])
            + gated(occupied[1], population_group[1]),
        area_sum: gated(occupied[0], area_group[0]) + gated(occupied[1], area_group[1]),
        primary_sum: gated(occupied[0], primary_group[0])
            + gated(occupied[1], primary_group[1]),
        maximum_outdoor_air_fraction: (membership[0] * zone_fraction[0])
            .max(membership[1] * zone_fraction[1])
            .max(membership[2] * zone_fraction[2]),
    }
}

fn outdoor_air_fraction(primary: f64, minimum_outdoor_air: f64) -> f64 {
    primary.min(minimum_outdoor_air) / primary.max(1e-4)
}

fn gated(enabled: bool, value: f64) -> f64 {
    if enabled { value } else { 0.0 }
}

struct SumZoneInputColumns<'a> {
    operation_mode_1: &'a [i64],
    operation_mode_2: &'a [i64],
    population_flow_1: &'a [f64],
    population_flow_2: &'a [f64],
    population_flow_3: &'a [f64],
    area_flow_1: &'a [f64],
    area_flow_2: &'a [f64],
    area_flow_3: &'a [f64],
    primary_flow_1: &'a [f64],
    primary_flow_2: &'a [f64],
    primary_flow_3: &'a [f64],
    minimum_outdoor_air_1: &'a [f64],
    minimum_outdoor_air_2: &'a [f64],
    minimum_outdoor_air_3: &'a [f64],
}

fn outdoor_airflow_sumzone_inputs(input: SumZoneInputColumns<'_>) -> Vec<InputSeries> {
    vec![
        input_i("operation_mode_1", input.operation_mode_1.iter().copied()),
        input_i("operation_mode_2", input.operation_mode_2.iter().copied()),
        input_r("population_flow_1", input.population_flow_1.iter().copied()),
        input_r("population_flow_2", input.population_flow_2.iter().copied()),
        input_r("population_flow_3", input.population_flow_3.iter().copied()),
        input_r("area_flow_1", input.area_flow_1.iter().copied()),
        input_r("area_flow_2", input.area_flow_2.iter().copied()),
        input_r("area_flow_3", input.area_flow_3.iter().copied()),
        input_r("primary_flow_1", input.primary_flow_1.iter().copied()),
        input_r("primary_flow_2", input.primary_flow_2.iter().copied()),
        input_r("primary_flow_3", input.primary_flow_3.iter().copied()),
        input_r(
            "minimum_outdoor_air_1",
            input.minimum_outdoor_air_1.iter().copied(),
        ),
        input_r(
            "minimum_outdoor_air_2",
            input.minimum_outdoor_air_2.iter().copied(),
        ),
        input_r(
            "minimum_outdoor_air_3",
            input.minimum_outdoor_air_3.iter().copied(),
        ),
    ]
}
